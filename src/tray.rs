//! System-tray icon. Left-click opens the Settings window; right-click shows
//! a short menu (Settings, Welcome tour, Check for updates, About, Exit).
//! The Settings window itself is `app/settings.rs`.

use std::sync::OnceLock;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Registry::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::app::{with_app, App, WM_APP_TRAY};
use crate::gfx::make_tray_icon;

const ID_SETTINGS: u32 = 1;
const ID_TOUR: u32 = 2;
const ID_UPDATES: u32 = 6;
const ID_ABOUT: u32 = 8;
const ID_EXIT: u32 = 9;
const ID_UNINSTALL: u32 = 10;

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_NAME: PCWSTR = w!("Nookly");

pub fn taskbar_created() -> u32 {
    static M: OnceLock<u32> = OnceLock::new();
    *M.get_or_init(|| unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) })
}

fn nid(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut n = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        ..Default::default()
    };
    let tip: Vec<u16> = "Nookly".encode_utf16().collect();
    n.szTip[..tip.len()].copy_from_slice(&tip);
    n
}

pub fn add(hwnd: HWND) {
    static ICON: OnceLock<isize> = OnceLock::new();
    let icon = *ICON.get_or_init(|| make_tray_icon().0 as isize);
    let mut n = nid(hwnd);
    n.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    n.uCallbackMessage = WM_APP_TRAY;
    n.hIcon = HICON(icon as *mut _);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_ADD, &n);
    }
}

pub fn remove(hwnd: HWND) {
    let n = nid(hwnd);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &n);
    }
}

fn exe_path() -> String {
    std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default()
}

pub fn startup_enabled() -> bool {
    unsafe {
        let mut size = 0u32;
        RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_NAME, RRF_RT_REG_SZ, None, None, Some(&mut size)).is_ok()
    }
}

/// The startup entry runs this very exe (a copied exe must not switch off
/// the installed one's startup).
pub fn startup_points_here() -> bool {
    unsafe {
        let mut buf = [0u16; 1024];
        let mut size = (buf.len() * 2) as u32;
        if RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_NAME, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size)).is_err() {
            return false;
        }
        let n = (size as usize / 2).saturating_sub(1).min(buf.len());
        let v = String::from_utf16_lossy(&buf[..n]);
        let v = v.trim().trim_matches('"').to_lowercase();
        v == exe_path().to_lowercase()
    }
}

pub fn set_startup(on: bool) {
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, Some(0), KEY_SET_VALUE, &mut key).is_err() {
            return;
        }
        if on {
            let v = format!("\"{}\"", exe_path());
            let w: Vec<u16> = v.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes = std::slice::from_raw_parts(w.as_ptr() as *const u8, w.len() * 2);
            let _ = RegSetValueExW(key, RUN_NAME, Some(0), REG_SZ, Some(bytes));
        } else {
            let _ = RegDeleteValueW(key, RUN_NAME);
        }
        let _ = RegCloseKey(key);
    }
}

/// The tray icon's right-click menu, in Nookly's own style and theme.
pub fn show_menu() {
    let Some((hwnd, theme)) = with_app(|a| (a.hwnd, crate::dialog::theme(&a.settings))) else {
        return;
    };
    match crate::dialog::menu(menu_items(), theme) {
        None => {}
        Some(ID_ABOUT) => about(),
        Some(ID_UNINSTALL) => {
            let _ = std::process::Command::new(exe_path()).arg("--uninstall").spawn();
        }
        Some(ID_EXIT) => unsafe {
            let _ = DestroyWindow(hwnd);
        },
        Some(c) => {
            with_app(|a| command(a, c));
        }
    }
}

pub fn command(app: &mut App, id: u32) {
    match id {
        ID_SETTINGS => app.open_settings(),
        ID_TOUR => app.start_onboarding(),
        ID_UPDATES => {
            if let Some(i) = app.modules.iter().position(|m| m.title() == "Updates") {
                app.call(i, |m, c| m.action(crate::modules::update::A_CHECK_NOW, (0.0, 0.0), c));
            }
        }
        ID_EXIT => unsafe {
            let _ = DestroyWindow(app.hwnd);
        },
        _ => {}
    }
    app.settings.save();
    app.request_frame();
}

/// The tray menu's rows (also drawn for the docs).
pub fn menu_items() -> Vec<crate::dialog::Item> {
    vec![
        (crate::gfx::glyph::SETTINGS, "Settings", ID_SETTINGS, false),
        ("\u{E8BC}", "Welcome tour", ID_TOUR, false),
        ("\u{E895}", "Check for updates", ID_UPDATES, false),
        ("\u{E946}", "About Nookly", ID_ABOUT, false),
        ("", "", 0, false),
        ("\u{E74D}", "Uninstall Nookly...", ID_UNINSTALL, false),
        ("\u{E7E8}", "Exit", ID_EXIT, true),
    ]
}

const GITHUB: u32 = 10;
const UPDATES: u32 = 11;
const CLOSE: u32 = 12;

/// The About window's contents (also drawn for the docs).
pub fn about_spec() -> crate::dialog::Spec {
    use crate::dialog::Kind;
    crate::dialog::Spec {
        title: "Nookly".into(),
        subtitle: format!("Version {}  \u{00B7}  by Banula Lakwindu", crate::modules::update::VERSION),
        body: "A living notch for Windows: music, widgets, calendar, notes, timers, health and much more, one hover away.\n\nFree and open source (MIT License). Lyrics by LRCLIB (lrclib.net). Your data stays on this PC, in %APPDATA%\\Nookly.".into(),
        checks: Vec::new(),
        buttons: vec![("GitHub page".into(), GITHUB, Kind::Plain), ("Check for updates".into(), UPDATES, Kind::Plain), ("Close".into(), CLOSE, Kind::Accent)],
        body_h: 100.0,
        danger_checks: false,
    }
}

/// About Nookly, in Nookly's own style and theme.
pub fn about() {
    let theme = with_app(|a| crate::dialog::theme(&a.settings)).unwrap_or(crate::gfx::THEMES[0]);
    match crate::dialog::show(about_spec(), theme).0 {
        GITHUB => {
            let u = HSTRING::from(format!("https://github.com/{}", crate::modules::update::REPO));
            unsafe {
                ShellExecuteW(None, w!("open"), &u, None, None, SW_SHOWNORMAL);
            }
        }
        UPDATES => {
            with_app(|a| command(a, ID_UPDATES));
        }
        _ => {}
    }
}
