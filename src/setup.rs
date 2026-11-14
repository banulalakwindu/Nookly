//! Nookly's own installer, in Nookly's style and theme. There is no separate
//! setup program: `NooklySetup-<version>.exe` is nookly.exe itself under
//! another name. Started under that name (or with `--setup`), it shows the
//! install window and copies itself into place, so the download is no
//! bigger than the app.
//!
//! It installs for the current user only (no admin rights):
//! `%LOCALAPPDATA%\Programs\Nookly\nookly.exe` (or where Nookly already
//! is), a Start menu shortcut, an optional desktop shortcut, "Start with
//! Windows", and the entry in Windows' Settings > Apps, whose Uninstall runs
//! `nookly.exe --uninstall`. Installs made by the older Inno Setup installer
//! upgrade in place (same folder, same Apps entry).
//!
//! `/SILENT` (or `/VERYSILENT`, `--quiet`): no windows, keep the current
//! choices, then start Nookly. The in-app updater runs it this way.

use std::path::{Path, PathBuf};

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Registry::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::dialog::{self, Kind, Spec};

/// The Apps entry (the same key the Inno installer used, so an upgrade
/// replaces it instead of adding a second one).
pub const UNINSTALL_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Nookly";
/// The old Inno Setup installer's Apps entry. Windows treats Inno entries
/// specially (Settings > Apps ran Inno's own uninstaller even with our
/// command in it), so an upgrade removes it, and Inno's leftover uninstaller
/// files, and registers the fresh entry above instead.
pub const INNO_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{8D994A83-65FE-4DF7-97E0-76482363209A}_is1";
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const LICENSE: &str = include_str!("../LICENSE");

/// Started as the installer?
pub fn wanted(args: &[String]) -> bool {
    if args.iter().any(|a| a == "--setup") {
        return true;
    }
    let name = std::env::current_exe().ok().and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_lowercase())).unwrap_or_default();
    name.starts_with("nooklysetup")
}

/// Started as `unins000.exe`, the stand-in for the old Inno uninstaller.
pub fn is_inno_stub() -> bool {
    std::env::current_exe().ok().and_then(|p| p.file_stem().map(|s| s.to_string_lossy().eq_ignore_ascii_case("unins000"))).unwrap_or(false)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn reg_get(key: &str, name: &str) -> Option<String> {
    unsafe {
        let mut buf = [0u16; 1024];
        let mut size = (buf.len() * 2) as u32;
        RegGetValueW(HKEY_CURRENT_USER, &HSTRING::from(key), &HSTRING::from(name), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size)).ok().ok()?;
        let n = (size as usize / 2).saturating_sub(1).min(buf.len());
        Some(String::from_utf16_lossy(&buf[..n]))
    }
}

fn reg_open(key: &str) -> Option<HKEY> {
    unsafe {
        let mut h = HKEY::default();
        RegCreateKeyExW(HKEY_CURRENT_USER, &HSTRING::from(key), 0, None, REG_OPTION_NON_VOLATILE, KEY_SET_VALUE, None, &mut h, None).ok().ok()?;
        Some(h)
    }
}

fn reg_sz(h: HKEY, name: &str, value: &str) {
    let w = wide(value);
    unsafe {
        let bytes = std::slice::from_raw_parts(w.as_ptr() as *const u8, w.len() * 2);
        let _ = RegSetValueExW(h, &HSTRING::from(name), 0, REG_SZ, Some(bytes));
    }
}

fn reg_dword(h: HKEY, name: &str, value: u32) {
    unsafe {
        let _ = RegSetValueExW(h, &HSTRING::from(name), 0, REG_DWORD, Some(&value.to_le_bytes()));
    }
}

/// A folder Windows knows (Start menu programs, desktop).
fn known(id: &GUID) -> Option<PathBuf> {
    unsafe {
        let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s.map(PathBuf::from)
    }
}

pub fn start_menu_link() -> Option<PathBuf> {
    known(&FOLDERID_Programs).map(|d| d.join("Nookly.lnk"))
}

pub fn desktop_link() -> Option<PathBuf> {
    known(&FOLDERID_Desktop).map(|d| d.join("Nookly.lnk"))
}

fn make_link(link: &Path, target: &Path) -> Result<()> {
    unsafe {
        let sl: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        sl.SetPath(&HSTRING::from(target.as_os_str()))?;
        if let Some(dir) = target.parent() {
            sl.SetWorkingDirectory(&HSTRING::from(dir.as_os_str()))?;
        }
        sl.SetDescription(w!("Nookly: a living notch for Windows"))?;
        let pf: IPersistFile = sl.cast()?;
        pf.Save(&HSTRING::from(link.as_os_str()), true)
    }
}

/// Where Nookly is (or goes): the folder Windows' Apps entry names, else
/// `%LOCALAPPDATA%\Programs\Nookly`.
pub fn install_dir() -> PathBuf {
    for key in [UNINSTALL_KEY, INNO_KEY] {
        if let Some(d) = reg_get(key, "InstallLocation").map(|s| PathBuf::from(s.trim_end_matches('\\'))).filter(|d| d.join("nookly.exe").exists()) {
            return d;
        }
    }
    let base = std::env::var("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
    base.join("Programs").join("Nookly")
}

/// Close a running Nookly and wait for it (up to 10 s).
fn close_running() -> bool {
    unsafe {
        for _ in 0..100 {
            let h = FindWindowW(w!("NooklyNotch"), None).unwrap_or_default();
            if h.0.is_null() {
                return true;
            }
            let _ = PostMessageW(h, WM_CLOSE, WPARAM(0), LPARAM(0));
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        false
    }
}

/// Copy, retrying while the old exe is still being released.
fn copy_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut last = Ok(0);
    for _ in 0..40 {
        last = std::fs::copy(from, to);
        if last.is_ok() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    last.map(|_| ())
}

/// What the install does, step by step (`--setup --dry-run` prints it).
pub fn plan(dir: &Path, startup: bool, desktop: bool) -> Vec<String> {
    let mut v = vec![
        "close Nookly if it is running".to_string(),
        format!("copy this exe to {}", dir.join("nookly.exe").display()),
        format!("write {}", dir.join("LICENSE.txt").display()),
    ];
    if let Some(l) = start_menu_link() {
        v.push(format!("Start menu shortcut {}", l.display()));
    }
    if desktop {
        if let Some(l) = desktop_link() {
            v.push(format!("desktop shortcut {}", l.display()));
        }
    }
    if dir.join("unins000.dat").exists() {
        v.push("remove the old Inno Setup uninstaller and its Apps entry".into());
    }
    v.push(format!("{} as a link to nookly.exe (opens Nookly's own uninstaller)", dir.join("unins000.exe").display()));
    v.push(format!("Apps entry HKCU\\{UNINSTALL_KEY}"));
    if startup {
        v.push("start with Windows".into());
    }
    v
}

pub fn run(args: &[String]) -> i32 {
    let has = |f: &str| args.iter().any(|a| a.eq_ignore_ascii_case(f));
    let quiet = has("/SILENT") || has("/VERYSILENT") || has("--quiet");
    let dir = install_dir();
    let target = dir.join("nookly.exe");
    let upgrade = target.exists();
    let st = crate::store::Settings::load();
    let theme = dialog::theme(&st);
    let version = crate::modules::update::VERSION;
    let mut startup = if upgrade { reg_get(RUN_KEY, "Nookly").is_some() } else { true };
    let mut desktop = desktop_link().map_or(false, |l| l.exists());
    let mut updates = st.update_check;
    if has("--dry-run") {
        println!("{}", plan(&dir, startup, desktop).join("\n"));
        return 0;
    }
    if !quiet {
        let (r, on) = dialog::show(install_spec(&dir, upgrade, startup, desktop, updates), theme);
        if r != 11 {
            return 1;
        }
        startup = on.first().copied().unwrap_or(startup);
        desktop = on.get(1).copied().unwrap_or(desktop);
        updates = on.get(2).copied().unwrap_or(updates);
    }
    match install(&dir, startup, desktop, quiet) {
        Ok(()) => {}
        Err(why) => {
            if !quiet {
                tell(theme, "Couldn't install Nookly", &why);
            }
            return 2;
        }
    }
    if !quiet {
        // The choice is kept in Nookly's settings (a later line wins).
        let line = format!("\nupdate_check={}\n", updates as u8);
        let _ = std::fs::create_dir_all(crate::store::dir());
        let path = crate::store::dir().join("settings.txt");
        let mut text = std::fs::read_to_string(&path).unwrap_or_default();
        text.push_str(&line);
        let _ = std::fs::write(&path, text);
    }
    let open = quiet || {
        let (r, _) = dialog::show(
            Spec {
                title: if upgrade { "Nookly is up to date".into() } else { "Nookly is installed".into() },
                subtitle: format!("Version {version}"),
                body: "Find it in the Start menu. Hover the top of your screen to open the notch; right-click it for Settings.".into(),
                checks: Vec::new(),
                buttons: vec![("Close".into(), 10, Kind::Plain), ("Open Nookly".into(), 11, Kind::Accent)],
                body_h: 40.0,
                danger_checks: false,
            },
            theme,
        );
        r == 11
    };
    if open {
        let _ = std::process::Command::new(&target).current_dir(&dir).spawn();
    }
    0
}

/// The install window's contents (also drawn for the docs).
pub fn install_spec(dir: &Path, upgrade: bool, startup: bool, desktop: bool, updates: bool) -> Spec {
    Spec {
        title: if upgrade { "Update Nookly".into() } else { "Install Nookly".into() },
        subtitle: format!("Version {}  \u{00B7}  a living notch for Windows", crate::modules::update::VERSION),
        body: format!("Nookly installs just for you, in {}. No admin rights needed. It's free and open source (MIT License); your data stays on this PC.", dir.display()),
        checks: vec![
            ("Start with Windows".into(), "Open Nookly when you sign in".into(), startup),
            ("Desktop shortcut".into(), "Besides the one in the Start menu".into(), desktop),
            ("Check for updates automatically".into(), "Asks GitHub for the latest version about twice a day".into(), updates),
        ],
        buttons: vec![("Cancel".into(), 10, Kind::Plain), (if upgrade { "Update" } else { "Install" }.into(), 11, Kind::Accent)],
        body_h: 54.0,
        danger_checks: false,
    }
}

fn install(dir: &Path, startup: bool, desktop: bool, quiet: bool) -> std::result::Result<(), String> {
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let target = dir.join("nookly.exe");
    if !close_running() {
        return Err("Nookly is still running. Quit it from its tray icon (right-click, Exit) and try again.".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("Couldn't create {}: {e}", dir.display()))?;
    let same = std::fs::canonicalize(&me).ok() == std::fs::canonicalize(&target).ok();
    if !same {
        copy_retry(&me, &target).map_err(|e| format!("Couldn't copy Nookly to {}: {e}", dir.display()))?;
    }
    let _ = std::fs::write(dir.join("LICENSE.txt"), LICENSE);
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    if let Some(l) = start_menu_link() {
        make_link(&l, &target).map_err(|e| format!("Couldn't make the Start menu shortcut: {e}"))?;
    }
    if let Some(l) = desktop_link() {
        if desktop {
            let _ = make_link(&l, &target);
        } else if !quiet && l.exists() {
            let _ = std::fs::remove_file(&l);
        }
    }
    // Upgrading from the Inno Setup installer: its entry and uninstaller go,
    // so Windows can only ever run Nookly's own uninstaller. Windows'
    // Settings > Apps can remember the old entry for a while, so
    // unins000.exe stays as a hard link to nookly.exe (no extra space):
    // started under that name it opens Nookly's uninstaller.
    unsafe {
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(INNO_KEY));
    }
    let stub = dir.join("unins000.exe");
    let _ = std::fs::remove_file(dir.join("unins000.dat"));
    let _ = std::fs::remove_file(&stub);
    if std::fs::hard_link(&target, &stub).is_err() {
        let _ = std::fs::copy(&target, &stub);
    }
    // Windows' Settings > Apps entry.
    let h = reg_open(UNINSTALL_KEY).ok_or("Couldn't register Nookly with Windows")?;
    let exe = target.display().to_string();
    let size_kb = std::fs::metadata(&target).map_or(0, |m| (m.len() / 1024) as u32);
    reg_sz(h, "DisplayName", "Nookly");
    reg_sz(h, "DisplayVersion", crate::modules::update::VERSION);
    reg_sz(h, "Publisher", "Banula Lakwindu");
    reg_sz(h, "DisplayIcon", &exe);
    reg_sz(h, "InstallLocation", &format!("{}\\", dir.display()));
    reg_sz(h, "UninstallString", &format!("\"{exe}\" --uninstall"));
    reg_sz(h, "QuietUninstallString", &format!("\"{exe}\" --uninstall --quiet"));
    reg_sz(h, "URLInfoAbout", &format!("https://github.com/{}", crate::modules::update::REPO));
    reg_sz(h, "HelpLink", &format!("https://github.com/{}/issues", crate::modules::update::REPO));
    reg_dword(h, "NoModify", 1);
    reg_dword(h, "NoRepair", 1);
    reg_dword(h, "EstimatedSize", size_kb);
    unsafe {
        let _ = RegCloseKey(h);
    }
    // Start with Windows: this copy (silent updates keep what was chosen).
    if let Some(h) = reg_open(RUN_KEY) {
        if startup || (quiet && reg_get(RUN_KEY, "Nookly").is_some()) {
            reg_sz(h, "Nookly", &format!("\"{exe}\""));
        } else if !quiet {
            unsafe {
                let _ = RegDeleteValueW(h, w!("Nookly"));
            }
        }
        unsafe {
            let _ = RegCloseKey(h);
        }
    }
    Ok(())
}

fn tell(theme: crate::gfx::Theme, title: &str, body: &str) {
    dialog::show(
        Spec {
            title: title.into(),
            subtitle: "Nookly setup".into(),
            body: body.into(),
            checks: Vec::new(),
            buttons: vec![("OK".into(), 10, Kind::Accent)],
            body_h: 56.0,
            danger_checks: false,
        },
        theme,
    );
}
