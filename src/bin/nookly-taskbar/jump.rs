//! An app's recent files and its own tasks ("New incognito window"), as
//! Windows' taskbar shows them on a right-click (its jump list).
//!
//! Recent files come from Windows' list for the app's id. Tasks are in the
//! file the app itself left for the taskbar (CustomDestinations), named
//! after its id: shortcuts one after another. Both are read when the menu
//! opens, on a thread of their own (a menu never waits for them).

use std::sync::Mutex;

use windows::Win32::Foundation::*;
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::*;
use windows::Win32::UI::Shell::Common::*;
use windows::Win32::UI::Shell::PropertiesSystem::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use crate::apps::App;

/// Sent to the bar when a jump list has been read.
pub const WM_JUMP: u32 = WM_APP + 42;

const MAX_RECENT: usize = 6;
const MAX_TASKS: usize = 6;

/// PKEY_Title.
const PKEY_TITLE: PROPERTYKEY = PROPERTYKEY { fmtid: GUID::from_u128(0xF29F85E0_4FF9_1068_AB91_08002B27B3D9), pid: 2 };

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub label: String,
    /// What to start, and with what.
    pub path: String,
    pub args: String,
    /// A recent file or folder (else one of the app's tasks).
    pub recent: bool,
    pub folder: bool,
}

/// The last list read: whose (app key), and what's in it.
static READ: Mutex<Option<(String, Vec<Entry>)>> = Mutex::new(None);

/// The list read for this app, if it has come.
pub fn take(key: &str) -> Option<Vec<Entry>> {
    let mut g = READ.lock().ok()?;
    if g.as_ref().is_some_and(|x| x.0 == key) { g.take().map(|x| x.1) } else { None }
}

/// Read an app's list; the bar hears WM_JUMP when it's there.
pub fn read(app: &App, bar: HWND) {
    let (key, launch, target, bar) = (app.key.clone(), app.launch.clone(), app.target.clone(), bar.0 as isize);
    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let list = entries(&key, &launch, target.as_deref());
        if let Ok(mut g) = READ.lock() {
            *g = Some((key, list));
        }
        unsafe {
            let _ = PostMessageW(Some(HWND(bar as *mut _)), WM_JUMP, WPARAM(0), LPARAM(0));
            CoUninitialize();
        }
    });
}

fn entries(key: &str, launch: &str, target: Option<&str>) -> Vec<Entry> {
    let mut ids = ids(key, launch, target);
    // What opens its files: its program, if it's an ordinary one.
    let exe = target.filter(|t| t.ends_with(".exe") && !t.ends_with("\\applicationframehost.exe") && !launch.starts_with("shell:")).map(String::from);
    let explorer = key == "microsoft.windows.explorer";
    let look = |ids: &[String]| {
        let out = ids.iter().map(|id| recent(id, exe.as_deref().filter(|_| !explorer), explorer)).find(|v| !v.is_empty()).unwrap_or_default();
        let more = ids.iter().map(|id| tasks(id)).find(|v| !v.is_empty()).unwrap_or_default();
        (out, more)
    };
    let (mut out, mut more) = look(&ids);
    // Nothing under its path: maybe it names itself in the Start menu.
    if out.is_empty() && more.is_empty() && key.contains('\\') {
        ids = target.and_then(shortcut_id).into_iter().collect();
        (out, more) = look(&ids);
    }
    more.retain(|e| !out.iter().any(|o| o.path == e.path && o.args == e.args));
    more.truncate(MAX_TASKS);
    out.append(&mut more);
    out
}

/// The ids Windows may know the app by: its own, or the one its program's
/// path stands for.
fn ids(key: &str, launch: &str, target: Option<&str>) -> Vec<String> {
    let mut v = Vec::new();
    let mut add = |s: String| {
        if !s.is_empty() && !v.iter().any(|x: &String| x.eq_ignore_ascii_case(&s)) {
            v.push(s);
        }
    };
    if !key.contains('\\') {
        add(key.to_string());
    }
    if let Some(id) = launch.strip_prefix("shell:AppsFolder\\") {
        add(id.to_string());
    }
    // (An app with an id of its own can share its program: a web app its
    // browser's. Then the path is the other's.)
    if let Some(t) = target.filter(|_| key.contains('\\')) {
        add(path_id(t));
    }
    v
}

/// The id of a program that gives none: its path, with the well-known
/// folders as their GUIDs.
fn path_id(path: &str) -> String {
    let mut best: Option<(usize, &str)> = None;
    for (var, sub, guid) in [
        ("ProgramW6432", "", "{6D809377-6AF0-444B-8957-A3773F02200E}"),
        ("ProgramFiles(x86)", "", "{7C5A40EF-A0FB-4BFC-874A-C0F2E0B9FA8E}"),
        ("SystemRoot", "\\System32", "{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}"),
        ("SystemRoot", "\\SysWOW64", "{D65231B0-B2F1-4857-A4CE-A8E7C6EA7D27}"),
        ("SystemRoot", "", "{F38BF404-1D43-42F2-9305-67DE0B28FC23}"),
    ] {
        let Ok(base) = std::env::var(var) else { continue };
        let base = format!("{base}{sub}\\").to_lowercase();
        if path.to_lowercase().starts_with(&base) && best.is_none_or(|(n, _)| base.len() > n) {
            best = Some((base.len(), guid));
        }
    }
    match best {
        Some((n, guid)) => format!("{guid}\\{}", &path[n..]),
        None => path.to_string(),
    }
}

/// The id a Start menu shortcut to this program carries (apps that name
/// themselves only there, as VS Code does). The shortcuts are read once.
fn shortcut_id(target: &str) -> Option<String> {
    static MAP: Mutex<Option<std::collections::HashMap<String, String>>> = Mutex::new(None);
    let mut g = MAP.lock().ok()?;
    let map = g.get_or_insert_with(|| {
        let mut m = std::collections::HashMap::new();
        for var in ["APPDATA", "ProgramData"] {
            if let Ok(b) = std::env::var(var) {
                walk(&std::path::Path::new(&b).join(r"Microsoft\Windows\Start Menu\Programs"), 0, &mut m);
            }
        }
        m
    });
    map.get(&target.to_lowercase()).cloned()
}

fn walk(dir: &std::path::Path, deep: u32, m: &mut std::collections::HashMap<String, String>) {
    let Ok(d) = std::fs::read_dir(dir) else { return };
    for e in d.flatten() {
        let p = e.path();
        if p.is_dir() {
            if deep < 3 {
                walk(&p, deep + 1, m);
            }
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")) {
            let (target, id) = crate::apps::read_link(&p.to_string_lossy());
            if let (Some(t), Some(id)) = (target, id) {
                m.entry(t.to_lowercase()).or_insert(id);
            }
        }
    }
}

/// The name Windows gives an id's file: a CRC-64 of it in capitals (UTF-16).
fn id_hash(id: &str) -> u64 {
    const POLY: u64 = 0x92C6_4265_D321_39A4;
    let mut c = u64::MAX;
    for b in id.to_uppercase().encode_utf16().flat_map(|u| u.to_le_bytes()) {
        let mut x = (c ^ b as u64) & 0xFF;
        for _ in 0..8 {
            x = if x & 1 != 0 { (x >> 1) ^ POLY } else { x >> 1 };
        }
        c = x ^ (c >> 8);
    }
    c
}

fn wstr(p: PWSTR) -> String {
    unsafe {
        let s = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as _));
        s
    }
}

/// Windows' recent (File Explorer: frequent) files of an id.
fn recent(id: &str, exe: Option<&str>, frequent: bool) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    unsafe {
        let Ok(l) = CoCreateInstance::<_, IApplicationDocumentLists>(&ApplicationDocumentLists, None, CLSCTX_INPROC_SERVER) else { return out };
        if l.SetAppID(&HSTRING::from(id)).is_err() {
            return out;
        }
        let Ok(a) = l.GetList::<IObjectArray>(if frequent { ADLT_FREQUENT } else { ADLT_RECENT }, 12) else { return out };
        for i in 0..a.GetCount().unwrap_or(0) {
            if out.len() >= MAX_RECENT {
                break;
            }
            let Ok(u) = a.GetAt::<IUnknown>(i) else { continue };
            if let Ok(item) = u.cast::<IShellItem>() {
                let name = item.GetDisplayName(SIGDN_NORMALDISPLAY).map(wstr).unwrap_or_default();
                let path = item.GetDisplayName(SIGDN_DESKTOPABSOLUTEPARSING).map(wstr).unwrap_or_default();
                if name.is_empty() || path.is_empty() {
                    continue;
                }
                // Not what's gone. (Looked up on this PC's own disks only: a
                // network or cloud drive can take long to answer.)
                let disk = path.get(..3).filter(|d| d.ends_with(":\\"));
                let own = disk.is_some_and(|d| own_disk(d));
                if own && !std::path::Path::new(&path).exists() {
                    continue;
                }
                let folder = explorer_folder(frequent, &path, own);
                let (p, args) = match exe {
                    Some(x) if !folder => (x.to_string(), format!("\"{path}\"")),
                    _ => (path, String::new()),
                };
                out.push(Entry { label: name, path: p, args, recent: true, folder });
            } else if let Ok(link) = u.cast::<IShellLinkW>() {
                if let Some(mut e) = link_entry(&link) {
                    e.recent = true;
                    out.push(e);
                }
            }
        }
    }
    out
}

/// A disk of this PC that answers at once.
fn own_disk(root: &str) -> bool {
    use windows::Win32::Storage::FileSystem::GetDriveTypeW;
    let system = std::env::var("SystemDrive").unwrap_or_default().to_lowercase();
    root.to_lowercase().starts_with(&system) || (unsafe { GetDriveTypeW(&HSTRING::from(root)) } == 3 && !cloud(root))
}

/// A cloud drive shown as a disk (Google Drive: its volume says so).
fn cloud(root: &str) -> bool {
    use windows::Win32::Storage::FileSystem::GetVolumeInformationW;
    let mut name = [0u16; 64];
    let mut fs = [0u16; 32];
    unsafe {
        let _ = GetVolumeInformationW(&HSTRING::from(root), Some(&mut name), None, None, None, Some(&mut fs));
    }
    let t = |b: &[u16]| String::from_utf16_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(0)]).to_lowercase();
    t(&name).contains("drive") || !matches!(t(&fs).as_str(), "ntfs" | "refs" | "exfat" | "fat32")
}

/// A folder? (File Explorer's frequent ones are; elsewhere it's asked for
/// what's on this PC's disks.)
fn explorer_folder(frequent: bool, path: &str, own: bool) -> bool {
    frequent || (own && std::path::Path::new(path).is_dir())
}

/// What a shortcut starts, and its title.
fn link_entry(link: &IShellLinkW) -> Option<Entry> {
    unsafe {
        let mut buf = [0u16; 1024];
        let text = |b: &[u16]| String::from_utf16_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(0)]);
        let path = link.GetPath(&mut buf, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32).map(|_| text(&buf)).unwrap_or_default();
        let mut a = [0u16; 2048];
        let args = link.GetArguments(&mut a).map(|_| text(&a)).unwrap_or_default();
        let title = link
            .cast::<IPropertyStore>()
            .ok()
            .and_then(|s| s.GetValue(&PKEY_TITLE).ok())
            .and_then(|v| PropVariantToStringAlloc(&v).ok())
            .map(wstr)
            .unwrap_or_default();
        let mut d = [0u16; 512];
        let label = if !title.is_empty() { title } else { link.GetDescription(&mut d).map(|_| text(&d)).unwrap_or_default() };
        // "@program,-123": a string in the program's resources.
        let label = if label.starts_with('@') {
            let mut s = [0u16; 256];
            SHLoadIndirectString(&HSTRING::from(&label), &mut s, None).map(|_| text(&s)).unwrap_or_default()
        } else {
            label
        };
        (!label.is_empty() && !path.is_empty()).then_some(Entry { label, path, args, recent: false, folder: false })
    }
}

/// Where the shortcuts in a jump list file start (each is a serialized
/// shortcut, after its class id).
fn links_in(data: &[u8]) -> Vec<usize> {
    const HEAD: [u8; 20] = [0x4C, 0, 0, 0, 1, 0x14, 2, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46];
    data.windows(HEAD.len()).enumerate().filter(|(_, w)| *w == HEAD).map(|(i, _)| i).collect()
}

/// The app's own tasks.
fn tasks(id: &str) -> Vec<Entry> {
    let Ok(base) = std::env::var("APPDATA") else { return Vec::new() };
    let f = std::path::Path::new(&base).join(format!(r"Microsoft\Windows\Recent\CustomDestinations\{:x}.customDestinations-ms", id_hash(id)));
    let Ok(data) = std::fs::read(f) else { return Vec::new() };
    // (A list is a few KB; a huge one isn't one.)
    if data.len() > 1 << 20 {
        return Vec::new();
    }
    let mut out: Vec<Entry> = Vec::new();
    for at in links_in(&data).into_iter().take(40) {
        unsafe {
            let Some(stream) = SHCreateMemStream(Some(&data[at..])) else { continue };
            let Ok(link) = CoCreateInstance::<_, IShellLinkW>(&ShellLink, None, CLSCTX_INPROC_SERVER) else { break };
            let Ok(p) = link.cast::<IPersistStream>() else { continue };
            if p.Load(&stream).is_err() {
                continue;
            }
            if let Some(e) = link_entry(&link) {
                if !out.contains(&e) {
                    out.push(e);
                }
            }
        }
    }
    out
}

/// Open one.
pub fn open(e: &Entry) {
    unsafe {
        ShellExecuteW(None, w!("open"), &HSTRING::from(&e.path), &HSTRING::from(&e.args), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

/// `--dump-jump <app key>`: the list, to check.
pub fn dump(key: &str, launch: &str, target: Option<&str>) -> String {
    let mut s = format!("ids {:?}\n", ids(key, launch, target));
    for e in entries(key, launch, target) {
        s += &format!("  {} {:?} <- {} {}\n", if e.recent { "recent" } else { "task  " }, e.label, e.path, e.args);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_as_windows_does() {
        assert_eq!(format!("{:x}", id_hash("Microsoft.Windows.Explorer")), "f01b4d95cf55d32a");
        assert_eq!(format!("{:x}", id_hash("brave")), "407962ce3d4220f");
        assert_eq!(format!("{:x}", id_hash("{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\notepad.exe")), "9b9cdc69c1c24e2b");
    }

    #[test]
    fn finds_the_shortcuts() {
        let head = [0x4Cu8, 0, 0, 0, 1, 0x14, 2, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46];
        let mut d = vec![9u8; 7];
        d.extend_from_slice(&head);
        d.extend_from_slice(&[0; 11]);
        d.extend_from_slice(&head);
        assert_eq!(links_in(&d), vec![7, 38]);
        assert!(links_in(&d[8..30]).is_empty());
    }
}
