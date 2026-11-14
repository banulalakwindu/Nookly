//! `nookly.exe --uninstall [--quiet] [--keep-data]`: remove Nookly
//! without needing the installer's own uninstaller to be allowed to run
//! (Smart App Control can block an unsigned uninstaller).
//!
//! Order: ask (unless --quiet), close the running copy (only if it is this
//! same exe), unhook Claude Code / Codex, take it out of startup (only if
//! the startup entry points at this exe), delete the data folder if asked,
//! then hand over to the installer's uninstaller when there is one, or
//! delete this exe after it exits. Screenshots in Pictures are never touched,
//! and a folder is only removed if it ends up empty.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn exe() -> PathBuf {
    std::env::current_exe().unwrap_or_default()
}

fn same_file(a: &Path, b: &Path) -> bool {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()).display().to_string().to_lowercase();
    canon(a) == canon(b)
}

/// Full path of the process that owns `hwnd`.
fn window_exe(hwnd: HWND) -> Option<PathBuf> {
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
        let _ = CloseHandle(h);
        ok.then(|| PathBuf::from(String::from_utf16_lossy(&buf[..n as usize])))
    }
}

/// Close the running notch if it is this exe; wait up to 5 s.
fn close_running() -> bool {
    unsafe {
        let Ok(h) = FindWindowW(w!("NooklyNotch"), None) else { return true };
        if h.0.is_null() {
            return true;
        }
        if !window_exe(h).map_or(false, |p| same_file(&p, &exe())) {
            return true;
        }
        let _ = PostMessageW(h, WM_CLOSE, WPARAM(0), LPARAM(0));
        for _ in 0..50 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if !IsWindow(h).as_bool() {
                return true;
            }
        }
        false
    }
}


/// What uninstalling would do, one step per line (also the --dry-run output).
pub fn plan(keep_data: bool) -> Vec<String> {
    let exe = exe();
    let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut v = vec!["close Nookly if this copy is running".to_string(), "disconnect Hermes, Claude Code and Codex".to_string()];
    if crate::tray::startup_points_here() {
        v.push("remove it from Windows startup".into());
    }
    if !keep_data {
        v.push(format!("delete data folder {}", crate::store::dir().display()));
    }
    // Inno's own uninstaller always has its .dat next to it; a lone
    // unins000.exe is Nookly's stand-in link (deleted with the app).
    let unins = dir.join("unins000.exe");
    if unins.exists() && dir.join("unins000.dat").exists() {
        v.push(format!("run {} /VERYSILENT", unins.display()));
    } else {
        if installed_here(&exe) {
            v.push("remove the Start menu and desktop shortcuts".into());
            v.push(format!("remove the Settings > Apps entry HKCU\\{}", crate::setup::UNINSTALL_KEY));
        }
        v.push(format!("delete {}, LICENSE.txt and unins000.exe", exe.display()));
        v.push(format!("remove {} if it is empty", dir.display()));
    }
    v
}

/// This exe is the copy Nookly's setup installed.
fn installed_here(exe: &Path) -> bool {
    same_file(&crate::setup::install_dir().join("nookly.exe"), exe)
}

/// The uninstall question (also drawn for the docs).
pub fn ask_spec(keep: bool) -> crate::dialog::Spec {
    let checks: Vec<_> = (!keep).then(|| ("Also delete my Nookly data".to_string(), "Settings, tasks, water log, Shelf list, pinned clipboard".to_string(), false)).into_iter().collect();
    crate::dialog::Spec {
        title: "Uninstall Nookly".into(),
        subtitle: format!("Version {}", crate::modules::update::VERSION),
        body: "Remove Nookly from this PC? It closes Nookly, takes it out of startup and disconnects Hermes, Claude Code and Codex. Screenshots in Pictures\\Screenshots are kept either way.".into(),
        checks,
        buttons: vec![("Cancel".into(), 10, crate::dialog::Kind::Plain), ("Uninstall".into(), 11, crate::dialog::Kind::Danger)],
        body_h: 70.0,
        danger_checks: true,
    }
}

pub fn run(args: &[String]) -> i32 {
    let quiet = args.iter().any(|a| a == "--quiet");
    let dry = args.iter().any(|a| a == "--dry-run");
    let mut keep = args.iter().any(|a| a == "--keep-data");
    if dry {
        println!("{}", plan(keep).join("\n"));
        return 0;
    }
    // The dialogs wear the notch's theme (read before the data may go).
    let theme = crate::dialog::theme(&crate::store::Settings::load());
    if !quiet {
        let (r, delete) = crate::dialog::show(ask_spec(keep), theme);
        if r != 11 {
            return 1;
        }
        keep = keep || !delete.first().copied().unwrap_or(false);
    }
    if !close_running() {
        if !quiet {
            tell(theme, "Nookly didn't close", "Quit it from the tray icon (right-click it, then Exit) and try again.");
        }
        return 2;
    }
    crate::modules::agents::disconnect_all();
    if crate::tray::startup_points_here() {
        crate::tray::set_startup(false);
    }
    if !keep {
        let _ = std::fs::remove_dir_all(crate::store::dir());
    }
    let exe = exe();
    let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let unins = dir.join("unins000.exe");
    if unins.exists() && dir.join("unins000.dat").exists() {
        // Inno's uninstaller removes the files, shortcuts and registry entry.
        let _ = std::process::Command::new(&unins).args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"]).spawn();
        if !quiet {
            tell(theme, "Nookly was removed", "Thanks for trying it. You can always get it again from its GitHub page.");
        }
        return 0;
    }
    // Installed by Nookly's own setup: its Start menu and desktop shortcuts
    // and the Settings > Apps entry go too (only for the installed copy, so
    // removing a copied exe never touches the real install).
    if installed_here(&exe) {
        for l in [crate::setup::start_menu_link(), crate::setup::desktop_link()].into_iter().flatten() {
            let _ = std::fs::remove_file(l);
        }
        for key in [crate::setup::UNINSTALL_KEY, crate::setup::INNO_KEY] {
            unsafe {
                let _ = windows::Win32::System::Registry::RegDeleteTreeW(windows::Win32::System::Registry::HKEY_CURRENT_USER, &HSTRING::from(key));
            }
        }
    }
    // No uninstaller left to run: delete the exe (and the license next to
    // it) once this process has exited, and the folder only if nothing else
    // is in it.
    let script = format!(
        "ping -n 3 127.0.0.1 >nul & del /f /q \"{}\" & del /f /q \"{}\" & del /f /q \"{}\" & rmdir \"{}\"",
        exe.display(),
        dir.join("LICENSE.txt").display(),
        dir.join("unins000.exe").display(),
        dir.display()
    );
    // Run from %TEMP%: a folder that is anyone's current directory can't be removed.
    let tmp = std::env::temp_dir();
    let _ = std::env::set_current_dir(&tmp);
    let _ = std::process::Command::new("cmd.exe").raw_arg(format!("/c {script}")).current_dir(&tmp).creation_flags(CREATE_NO_WINDOW).spawn();
    if !quiet {
        tell(theme, "Nookly was removed", "Thanks for trying it. You can always get it again from its GitHub page.");
    }
    0
}

/// A short message with an OK button, in Nookly's style.
fn tell(theme: crate::gfx::Theme, title: &str, body: &str) {
    crate::dialog::show(
        crate::dialog::Spec {
            title: title.into(),
            subtitle: "Uninstall Nookly".into(),
            body: body.into(),
            checks: Vec::new(),
            buttons: vec![("OK".into(), 10, crate::dialog::Kind::Accent)],
            body_h: 40.0,
            danger_checks: false,
        },
        theme,
    );
}
