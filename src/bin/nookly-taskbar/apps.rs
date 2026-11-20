//! The apps on the bar: pinned apps first (Windows' own taskbar pins, then
//! the ones pinned in Nookly), then the other apps with windows open. Windows
//! of one app are grouped by its AppUserModelID (what Windows' taskbar
//! groups by), else by its exe.
//!
//! Cheap: the window list is read again only after a window event (made,
//! closed, shown, hidden, renamed, brought to front), at most every 50 ms,
//! and icons are made once per app and kept.

use std::collections::HashMap;
use std::path::Path;

use nookly_core::store::Settings;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PropVariantToStringAlloc};
use windows::Win32::System::Com::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::Shell::PropertiesSystem::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

/// Pins made in Nookly's taskbar.
const PINS: &str = "taskbar_pins.txt";

/// PKEY_AppUserModel_ID.
const PKEY_AUMID: PROPERTYKEY = PROPERTYKEY { fmtid: GUID::from_u128(0x9F4C2855_9F79_4B39_A8D0_E1D42DE1D5F3), pid: 5 };

#[derive(Clone)]
pub struct App {
    /// What groups its windows: the AppUserModelID or exe path, lower case.
    pub key: String,
    pub name: String,
    /// What to start it with (a shortcut, an exe, or shell:AppsFolder\<id>),
    /// and what its icon comes from.
    pub launch: String,
    pub pinned: bool,
    /// Its windows, in the order they opened (they don't move around).
    pub windows: Vec<HWND>,
    /// Its window nearest the front (the one a click brings forward).
    pub front: Option<HWND>,
    /// The exe it runs (lower case), to match windows without an id.
    pub target: Option<String>,
}

/// Pixels of an app's icon (premultiplied BGRA, square).
pub type Icon = (u32, Vec<u8>);

/// An app collection: apps dragged onto each other on the bar, shown as
/// one tile after the apps. Its apps that are open also show in the list.
#[derive(Clone)]
pub struct Group {
    pub name: String,
    pub members: Vec<App>,
}

/// Collections (taskbar_groups.txt): one per line, the name, then a tab
/// and "key|launch|name" for each app.
const GROUPS: &str = "taskbar_groups.txt";

fn read_groups() -> Vec<(String, Vec<(String, String, String)>)> {
    nookly_core::store::read(GROUPS)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let mut parts = l.split('\t');
            let name = parts.next().unwrap_or("").to_string();
            let members = parts
                .filter_map(|m| {
                    let mut f = m.splitn(3, '|');
                    let (k, l, n) = (f.next()?, f.next()?, f.next().unwrap_or(""));
                    (!k.is_empty()).then(|| (k.to_string(), l.to_string(), n.to_string()))
                })
                .collect();
            (name, members)
        })
        .filter(|(_, m): &(String, Vec<_>)| !m.is_empty())
        .collect()
}

fn write_groups(g: &[(String, Vec<(String, String, String)>)]) {
    let clean = |s: &str| s.replace(['\t', '|', '\n', '\r'], " ");
    let text: String = g
        .iter()
        .filter(|(_, m)| !m.is_empty())
        .map(|(n, m)| {
            let ms: Vec<String> = m.iter().map(|(k, l, nm)| format!("{}|{}|{}", clean(k), clean(l), clean(nm))).collect();
            format!("{}\t{}\n", clean(n), ms.join("\t"))
        })
        .collect();
    nookly_core::store::write(GROUPS, &text);
}

/// Put `app` into collection `group` (None: a new one, together with
/// `with`). An app is in one collection at most.
pub fn group_add(group: Option<usize>, app: &App, with: Option<&App>) {
    let mut g = read_groups();
    for (_, m) in g.iter_mut() {
        m.retain(|x| x.0 != app.key && with.is_none_or(|w| x.0 != w.key));
    }
    let entry = |a: &App| (a.key.clone(), a.launch.clone(), a.name.clone());
    match group.filter(|&i| i < g.len()) {
        Some(i) => g[i].1.push(entry(app)),
        None => {
            let mut m = Vec::new();
            if let Some(w) = with {
                m.push(entry(w));
            }
            m.push(entry(app));
            g.push(("Apps".to_string(), m));
        }
    }
    write_groups(&g);
}

/// Take an app out of its collection (a collection left with one app goes).
pub fn group_remove(group: usize, key: &str) {
    let mut g = read_groups();
    if let Some((_, m)) = g.get_mut(group) {
        m.retain(|x| x.0 != key);
        if m.len() < 2 {
            g.remove(group);
        }
    }
    write_groups(&g);
}

/// Undo a collection (its apps go back to the bar).
pub fn group_delete(group: usize) {
    let mut g = read_groups();
    if group < g.len() {
        g.remove(group);
    }
    write_groups(&g);
}

pub struct Apps {
    pub list: Vec<App>,
    pub groups: Vec<Group>,
    pub active: HWND,
    icons: HashMap<String, Option<Icon>>,
    pins: Vec<App>,
    pins_read: Option<std::time::Instant>,
    me: u32,
    /// Each window's exe and app id, read once (they don't change).
    ids: HashMap<isize, (String, Option<String>)>,
}

fn wstr(p: PWSTR) -> String {
    unsafe {
        let s = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as _));
        s
    }
}

fn aumid_of(store: &IPropertyStore) -> Option<String> {
    unsafe {
        let v: PROPVARIANT = store.GetValue(&PKEY_AUMID).ok()?;
        let s = wstr(PropVariantToStringAlloc(&v).ok()?);
        (!s.is_empty()).then_some(s)
    }
}

fn window_aumid(h: HWND) -> Option<String> {
    unsafe { SHGetPropertyStoreForWindow::<IPropertyStore>(h).ok().and_then(|s| aumid_of(&s)) }
}

/// A packaged (Store) app's id, from its process.
fn process_aumid(pid: u32) -> Option<String> {
    unsafe {
        let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 512];
        let mut n = buf.len() as u32;
        let r = windows::Win32::Storage::Packaging::Appx::GetApplicationUserModelId(p, &mut n, Some(PWSTR(buf.as_mut_ptr())));
        let _ = CloseHandle(p);
        (r == ERROR_SUCCESS && n > 1).then(|| String::from_utf16_lossy(&buf[..n as usize - 1]))
    }
}

/// A shell item's display name ("Claude" for shell:AppsFolder\Claude_...).
fn shell_name(path: &str) -> Option<String> {
    unsafe {
        let item: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(path), None).ok()?;
        let s = wstr(item.GetDisplayName(SIGDN_NORMALDISPLAY).ok()?);
        (!s.is_empty()).then_some(s)
    }
}

pub fn exe_of(pid: u32) -> Option<String> {
    unsafe {
        let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
        let _ = CloseHandle(p);
        ok.then(|| String::from_utf16_lossy(&buf[..n as usize]))
    }
}

pub fn title(h: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetWindowTextW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn class(h: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(h, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Would Windows' taskbar show a button for this window?
fn eligible(h: HWND, me: u32) -> bool {
    unsafe {
        if !IsWindowVisible(h).as_bool() {
            return false;
        }
        let ex = GetWindowLongW(h, GWL_EXSTYLE) as u32;
        let app = ex & WS_EX_APPWINDOW.0 != 0;
        let owned = GetWindow(h, GW_OWNER).is_ok_and(|o| !o.is_invalid());
        if !app && (owned || ex & WS_EX_TOOLWINDOW.0 != 0 || ex & WS_EX_NOACTIVATE.0 != 0) {
            return false;
        }
        let mut cloaked = 0u32;
        let _ = DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut _, 4);
        if cloaked != 0 {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, Some(&mut pid));
        if pid == me || title(h).is_empty() {
            return false;
        }
        !matches!(class(h).as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "NooklyNotch" | "NooklySettings")
    }
}

unsafe extern "system" fn collect(h: HWND, l: LPARAM) -> BOOL {
    let v = unsafe { &mut *(l.0 as *mut (u32, Vec<HWND>)) };
    if eligible(h, v.0) {
        v.1.push(h);
    }
    TRUE
}

/// A display name from an exe path: "Code" for C:\...\Code.exe.
fn exe_name(path: &str) -> String {
    Path::new(path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

/// "code" -> "Code".
fn pretty(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// The shortcuts pinned to Windows' own taskbar.
fn windows_pins() -> Vec<App> {
    let Ok(base) = std::env::var("APPDATA") else { return Vec::new() };
    let dir = Path::new(&base).join(r"Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar");
    let mut files: Vec<_> = std::fs::read_dir(&dir).map(|d| d.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk"))).collect()).unwrap_or_default();
    files.sort();
    files
        .into_iter()
        .filter_map(|lnk| {
            let path = lnk.to_string_lossy().to_string();
            let (target, aumid) = read_link(&path);
            let target = target.map(|t| t.to_lowercase());
            let key = aumid.or(target.clone())?.to_lowercase();
            Some(App { key, name: exe_name(&path), launch: path, pinned: true, windows: Vec::new(), front: None, target })
        })
        .collect()
}

/// A shortcut's target path and AppUserModelID.
fn read_link(path: &str) -> (Option<String>, Option<String>) {
    unsafe {
        let Ok(link) = CoCreateInstance::<_, IShellLinkW>(&ShellLink, None, CLSCTX_INPROC_SERVER) else { return (None, None) };
        let Ok(file) = link.cast::<IPersistFile>() else { return (None, None) };
        if file.Load(&HSTRING::from(path), STGM_READ).is_err() {
            return (None, None);
        }
        let mut buf = [0u16; 1024];
        let target = link.GetPath(&mut buf, std::ptr::null_mut(), 0).ok().map(|_| String::from_utf16_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap_or(0)])).filter(|s| !s.is_empty());
        let aumid = link.cast::<IPropertyStore>().ok().and_then(|s| aumid_of(&s));
        (target, aumid)
    }
}

/// An item's icon at `px` pixels, through the shell (exe, shortcut, or
/// shell:AppsFolder\<id> for Store apps): premultiplied BGRA.
pub fn icon_pixels(path: &str, px: u32) -> Option<Icon> {
    crate::crumb(format!("icon of {path} at {px}px"));
    unsafe {
        let item: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(path), None).ok()?;
        let f: IShellItemImageFactory = item.cast().ok()?;
        let hbm = f.GetImage(SIZE { cx: px as i32, cy: px as i32 }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK).ok()?;
        let mut bm = BITMAP::default();
        GetObjectW(hbm.into(), std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _));
        let (w, h) = (bm.bmWidth.max(1) as u32, bm.bmHeight.unsigned_abs().max(1));
        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w as i32, biHeight: -(h as i32), biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0, ..Default::default() },
            ..Default::default()
        };
        let mut px_buf = vec![0u8; (w * h * 4) as usize];
        let dc = GetDC(None);
        let n = GetDIBits(dc, hbm, 0, h, Some(px_buf.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
        ReleaseDC(None, dc);
        let _ = DeleteObject(hbm.into());
        if n == 0 {
            return None;
        }
        // No alpha at all (an old-style icon): opaque.
        if px_buf.chunks_exact(4).all(|p| p[3] == 0) {
            for p in px_buf.chunks_exact_mut(4) {
                p[3] = 255;
            }
        }
        for p in px_buf.chunks_exact_mut(4) {
            let a = p[3] as u32;
            for c in 0..3 {
                p[c] = ((p[c] as u32 * a + 127) / 255) as u8;
            }
        }
        // Square it (some come back wider than tall).
        if w != h {
            let s = w.max(h);
            let mut sq = vec![0u8; (s * s * 4) as usize];
            let (ox, oy) = ((s - w) / 2, (s - h) / 2);
            for y in 0..h {
                let src = (y * w * 4) as usize;
                let dst = (((y + oy) * s + ox) * 4) as usize;
                sq[dst..dst + (w * 4) as usize].copy_from_slice(&px_buf[src..src + (w * 4) as usize]);
            }
            return Some((s, sq));
        }
        Some((w, px_buf))
    }
}

impl Apps {
    pub fn new() -> Apps {
        Apps { list: Vec::new(), groups: Vec::new(), active: HWND::default(), icons: HashMap::new(), pins: Vec::new(), pins_read: None, me: std::process::id(), ids: HashMap::new() }
    }

    /// Nookly's own pins (launch strings) and the Windows pins unpinned
    /// here (keys), from taskbar_pins.txt: "+<launch>" and "-<key>" lines.
    /// (A file of the taskbar's own: the notch saves settings.txt whole.)
    fn own_pins() -> (Vec<String>, Vec<String>) {
        let text = nookly_core::store::read(PINS);
        let pick = |c: char| text.lines().filter_map(|l| l.strip_prefix(c)).filter(|x| !x.is_empty()).map(String::from).collect();
        (pick('+'), pick('-'))
    }

    /// What the bar shows of the apps (to know if a scan changed it).
    pub fn signature(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for a in &self.list {
            a.key.hash(&mut h);
            a.windows.iter().map(|w| w.0 as usize).collect::<Vec<_>>().hash(&mut h);
        }
        for g in &self.groups {
            g.name.hash(&mut h);
            g.members.iter().map(|m| (m.key.clone(), m.windows.len())).collect::<Vec<_>>().hash(&mut h);
        }
        (self.active.0 as usize).hash(&mut h);
        h.finish()
    }

    /// Read the windows again and rebuild the list.
    pub fn scan(&mut self, st: &Settings) {
        if self.pins_read.is_none_or(|t| t.elapsed().as_secs() > 30) {
            self.pins = windows_pins();
            self.pins_read = Some(std::time::Instant::now());
        }
        let _ = st;
        let (own, unpinned) = Self::own_pins();
        let mut list: Vec<App> = self.pins.iter().filter(|p| !unpinned.contains(&p.key)).cloned().collect();
        for l in own {
            let key = l.strip_prefix("shell:AppsFolder\\").map(String::from).unwrap_or_else(|| l.clone()).to_lowercase();
            if !list.iter().any(|a| a.key == key) {
                let name = if l.starts_with("shell:") { shell_name(&l).unwrap_or_default() } else { pretty(&exe_name(&l)) };
                let target = (!l.starts_with("shell:")).then(|| l.to_lowercase());
                list.push(App { key, name, launch: l, pinned: true, windows: Vec::new(), front: None, target });
            }
        }
        let mut found: (u32, Vec<HWND>) = (self.me, Vec::new());
        unsafe {
            let _ = EnumWindows(Some(collect), LPARAM(&mut found as *mut _ as isize));
        }
        self.ids.retain(|k, _| found.1.iter().any(|h| h.0 as isize == *k));
        for h in found.1 {
            let (exe, aumid) = self
                .ids
                .entry(h.0 as isize)
                .or_insert_with(|| {
                    crate::crumb(format!("reading window {:?} \"{}\" ({})", h.0, title(h), class(h)));
                    let mut pid = 0u32;
                    unsafe {
                        GetWindowThreadProcessId(h, Some(&mut pid));
                    }
                    let exe = exe_of(pid).unwrap_or_default();
                    let exe_l = exe.to_lowercase();
                    // The window's own id, else its package's (Store apps),
                    // else File Explorer's for its windows (they carry none).
                    let aumid = window_aumid(h).or_else(|| process_aumid(pid)).or_else(|| exe_l.ends_with("\\windows\\explorer.exe").then(|| "Microsoft.Windows.Explorer".to_string()));
                    (exe, aumid)
                })
                .clone();
            let exe_l = exe.to_lowercase();
            let key = aumid.clone().unwrap_or_else(|| exe.clone()).to_lowercase();
            if key.is_empty() {
                continue;
            }
            // Without an id, a pin of the same exe takes the window.
            let at = list.iter().position(|a| a.key == key).or_else(|| if aumid.is_none() { list.iter().position(|a| a.target.as_deref() == Some(exe_l.as_str())) } else { None });
            match at {
                Some(i) => list[i].windows.push(h),
                None => {
                    let launch = match &aumid {
                        // Store apps start through their id, not their exe.
                        Some(id) if exe_l.ends_with("applicationframehost.exe") || exe_l.contains("\\windowsapps\\") || !Path::new(&exe).exists() => format!("shell:AppsFolder\\{id}"),
                        _ => exe.clone(),
                    };
                    let name = if launch.starts_with("shell:") { shell_name(&launch).unwrap_or_default() } else { pretty(&exe_name(&exe)) };
                    list.push(App { key, name, launch, pinned: false, windows: vec![h], front: None, target: Some(exe_l) });
                }
            }
        }
        // Windows are read front to back, an order that changes each time
        // one comes forward. Keep the order apps and windows had (new ones
        // last), as Windows' own taskbar does: tiles, strip icons and
        // preview cards must not swap places under the cursor.
        let old_apps: Vec<String> = self.list.iter().map(|a| a.key.clone()).collect();
        let old_wins: Vec<HWND> = self.list.iter().flat_map(|a| a.windows.iter().copied()).collect();
        let rank_w = |h: &HWND| old_wins.iter().position(|w| w == h).unwrap_or(usize::MAX);
        for a in list.iter_mut() {
            a.front = a.windows.first().copied();
            a.windows.sort_by_key(|h| rank_w(h));
        }
        let pins = list.iter().take_while(|a| a.pinned).count();
        list[pins..].sort_by_key(|a| old_apps.iter().position(|k| *k == a.key).unwrap_or(usize::MAX));
        for a in list.iter_mut() {
            if a.name.is_empty() || a.name.eq_ignore_ascii_case("ApplicationFrameHost") {
                a.name = a.windows.first().map(|&h| title(h)).unwrap_or_default();
            }
        }
        // Collections: their apps leave the list unless they're open.
        let mut groups = Vec::new();
        for (name, members) in read_groups() {
            let mut ms = Vec::new();
            for (key, launch, nm) in members {
                match list.iter().position(|a| a.key == key) {
                    Some(i) if !list[i].windows.is_empty() => ms.push(list[i].clone()),
                    Some(i) => ms.push(list.remove(i)),
                    None => ms.push(App { key, name: nm, launch, pinned: false, windows: Vec::new(), front: None, target: None }),
                }
            }
            groups.push(Group { name, members: ms });
        }
        self.active = unsafe { GetForegroundWindow() };
        self.list = list;
        self.groups = groups;
    }

    /// The app's icon (made once, then kept).
    pub fn icon(&mut self, a: &App, px: u32) -> Option<&Icon> {
        let k = format!("{}@{px}", a.key);
        if !self.icons.contains_key(&k) {
            let i = icon_pixels(&a.launch, px).or_else(|| a.windows.first().and_then(|_| icon_pixels(&a.key, px)));
            self.icons.insert(k.clone(), i);
        }
        self.icons.get(&k).and_then(|i| i.as_ref())
    }

    pub fn forget_icons(&mut self) {
        self.icons.clear();
    }
}

/// Bring a window to the front (restoring it if minimized).
pub fn activate(h: HWND) {
    unsafe {
        if IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_RESTORE);
        }
        let ok = SetForegroundWindow(h).as_bool();
        crate::dbg_log(&format!("activate {:?}: {ok}", h.0));
        if !ok {
            // Windows refuses unless we had the last input; a tap of Alt
            // counts as ours.
            nookly_core::shell::press(&[windows::Win32::UI::Input::KeyboardAndMouse::VK_MENU]);
            let _ = SetForegroundWindow(h);
        }
    }
}

pub fn minimize(h: HWND) {
    unsafe {
        let _ = ShowWindow(h, SW_MINIMIZE);
    }
}

pub fn close(h: HWND) {
    unsafe {
        let _ = PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

/// Start an app (a new window of it).
pub fn launch(what: &str) {
    unsafe {
        ShellExecuteW(None, w!("open"), &HSTRING::from(what), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

/// Pin an app to Nookly's taskbar, or unpin it (a Windows pin is only
/// hidden here; Windows' own pins aren't changed).
pub fn set_pinned(app: &App, pin: bool) {
    let text = nookly_core::store::read(PINS);
    let mut lines: Vec<String> = text.lines().filter(|l| !l.is_empty()).map(String::from).collect();
    let plus = format!("+{}", app.launch);
    let minus = format!("-{}", app.key);
    lines.retain(|l| *l != plus && *l != minus && l.strip_prefix('+').is_none_or(|x| x.to_lowercase() != app.key));
    if pin {
        lines.push(plus);
    } else if app.launch.to_lowercase().ends_with(".lnk") {
        lines.push(minus);
    }
    nookly_core::store::write(PINS, &(lines.join("\n") + "\n"));
}

/// A window's exe and AppUserModelID (`--dump-apps`).
pub fn debug_ids(h: HWND) -> (String, Option<String>) {
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(h, Some(&mut pid));
    }
    (exe_of(pid).unwrap_or_default(), window_aumid(h))
}
