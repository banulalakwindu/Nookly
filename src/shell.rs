//! The Windows taskbar, for the taskbar add-on: hide it while ours runs and
//! always put it back, plus Windows' own Start, Search, notification and
//! tray flyouts, opened from our buttons.
//!
//! Hiding is three steps: the Windows taskbar is set to auto-hide (so it
//! gives up its strip of the screen), its windows are hidden, and made fully
//! see-through: Windows shows them again now and then, for the tenth of a
//! second until we hear of it, and they'd flash over ours. What it was
//! before is written to `taskbar_state.txt` first, so whoever finds the file
//! (the add-on on exit, the notch when the add-on dies, either one at the
//! next start, the uninstaller) can put it back exactly.

use windows::Win32::Foundation::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

const STATE: &str = "taskbar_state.txt";

/// The window property on the taskbar add-on's own tray window (it has
/// Windows' class name, so apps' tray icons reach it).
pub const TRAY_MARK: PCWSTR = w!("Nookly.Tray");

/// The main Windows taskbar window (never the add-on's tray window).
pub fn tray() -> Option<HWND> {
    let mut prev: Option<HWND> = None;
    unsafe {
        while let Ok(h) = FindWindowExW(None, prev, w!("Shell_TrayWnd"), PCWSTR::null()) {
            if h.is_invalid() {
                break;
            }
            if GetPropW(h, TRAY_MARK).is_invalid() {
                return Some(h);
            }
            prev = Some(h);
        }
    }
    None
}

/// The main taskbar and the ones on other screens.
pub fn trays() -> Vec<HWND> {
    let mut v: Vec<HWND> = tray().into_iter().collect();
    let mut prev: Option<HWND> = None;
    unsafe {
        while let Ok(h) = FindWindowExW(None, prev, w!("Shell_SecondaryTrayWnd"), PCWSTR::null()) {
            if h.is_invalid() {
                break;
            }
            v.push(h);
            prev = Some(h);
        }
    }
    v
}

fn appbar_data() -> APPBARDATA {
    APPBARDATA { cbSize: std::mem::size_of::<APPBARDATA>() as u32, hWnd: tray().unwrap_or_default(), ..Default::default() }
}

/// Is the Windows taskbar set to auto-hide?
pub fn autohide() -> bool {
    let mut d = appbar_data();
    unsafe { SHAppBarMessage(ABM_GETSTATE, &mut d) as u32 & ABS_AUTOHIDE != 0 }
}

fn set_autohide(on: bool) {
    let mut d = appbar_data();
    d.lParam = LPARAM(if on { ABS_AUTOHIDE as isize } else { ABS_ALWAYSONTOP as isize });
    unsafe {
        SHAppBarMessage(ABM_SETSTATE, &mut d);
    }
}

/// Has the Windows taskbar been hidden by us (and not put back yet)?
pub fn hidden_by_us() -> bool {
    crate::store::parse_kv(&crate::store::read(STATE)).get("hidden").is_some_and(|v| v == "1")
}

/// The window property on Windows' taskbars that we made see-through (and
/// that weren't layered windows before).
const GHOST: PCWSTR = w!("Nookly.Ghost");

/// Make one of Windows' taskbars fully see-through (clicks go through it
/// then too), or as it was.
fn ghost(h: HWND, on: bool) {
    unsafe {
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
        let layered = WS_EX_LAYERED.0 as isize;
        if on {
            let (mut alpha, mut flags) = (0u8, LAYERED_WINDOW_ATTRIBUTES_FLAGS(0));
            if ex & layered != 0 && GetLayeredWindowAttributes(h, None, Some(&mut alpha), Some(&mut flags)).is_ok() && flags.0 & LWA_ALPHA.0 != 0 && alpha == 0 {
                return;
            }
            if ex & layered == 0 {
                SetWindowLongPtrW(h, GWL_EXSTYLE, ex | layered);
                let _ = SetPropW(h, GHOST, Some(HANDLE(1 as *mut _)));
            }
            let _ = SetLayeredWindowAttributes(h, COLORREF(0), 0, LWA_ALPHA);
        } else if ex & layered != 0 {
            let _ = SetLayeredWindowAttributes(h, COLORREF(0), 255, LWA_ALPHA);
            if !GetPropW(h, GHOST).is_invalid() {
                let _ = RemovePropW(h, GHOST);
                SetWindowLongPtrW(h, GWL_EXSTYLE, GetWindowLongPtrW(h, GWL_EXSTYLE) & !layered);
            }
        }
    }
}

/// Hide the Windows taskbar, remembering how it was. Safe to call again
/// (Explorer restarted, or it showed itself): the first state is kept.
pub fn hide_windows_taskbar() {
    if !hidden_by_us() {
        let mut kv = std::collections::BTreeMap::new();
        kv.insert("hidden".to_string(), "1".to_string());
        kv.insert("autohide".to_string(), if autohide() { "1" } else { "0" }.to_string());
        crate::store::write(STATE, &crate::store::write_kv(&kv));
    }
    for h in trays() {
        ghost(h, true);
    }
    if !autohide() {
        set_autohide(true);
    }
    for h in trays() {
        unsafe {
            let _ = ShowWindow(h, SW_HIDE);
        }
    }
}

/// Keep it hidden: Windows shows its taskbar again now and then (Start
/// opening, a full-screen app closing). Cheap; called on window events.
pub fn keep_hidden() {
    if !hidden_by_us() {
        return;
    }
    for h in trays() {
        // (A taskbar Windows just made, for a screen plugged in, too.)
        ghost(h, true);
        unsafe {
            if IsWindowVisible(h).as_bool() {
                let _ = ShowWindow(h, SW_HIDE);
            }
        }
    }
}

/// Nookly's own windows stay in sight while the taskbar peeks at a window
/// (the others turn to glass).
pub fn keep_in_peek(h: HWND) {
    use windows::Win32::Graphics::Dwm::{DWMWA_EXCLUDED_FROM_PEEK, DwmSetWindowAttribute};
    let yes = TRUE;
    unsafe {
        let _ = DwmSetWindowAttribute(h, DWMWA_EXCLUDED_FROM_PEEK, &yes as *const BOOL as *const _, 4);
    }
}

/// Put the Windows taskbar back the way it was. Does nothing unless we hid it.
pub fn restore_windows_taskbar() {
    let kv = crate::store::parse_kv(&crate::store::read(STATE));
    if kv.get("hidden").is_none_or(|v| v != "1") {
        return;
    }
    let was_autohide = kv.get("autohide").is_some_and(|v| v == "1");
    if autohide() != was_autohide {
        set_autohide(was_autohide);
    }
    for h in trays() {
        ghost(h, false);
        unsafe {
            let _ = ShowWindow(h, SW_SHOWNA);
        }
    }
    let _ = std::fs::remove_file(crate::store::dir().join(STATE));
}

/// Press and release a key combination (`keys` held in order, released in
/// reverse), as if typed.
pub fn press(keys: &[VIRTUAL_KEY]) {
    let key = |vk: VIRTUAL_KEY, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, ..Default::default() } },
    };
    let mut v: Vec<INPUT> = keys.iter().map(|&k| key(k, false)).collect();
    v.extend(keys.iter().rev().map(|&k| key(k, true)));
    unsafe {
        SendInput(&v, std::mem::size_of::<INPUT>() as i32);
    }
}

/// Windows' own flyouts, opened from our taskbar's buttons.
pub fn open_start() {
    press(&[VK_LWIN]);
}
pub fn open_search() {
    press(&[VK_LWIN, VK_S]);
}
pub fn open_notifications() {
    press(&[VK_LWIN, VK_N]);
}
pub fn open_quick_settings() {
    press(&[VK_LWIN, VK_A]);
}
/// The hidden tray icons: Win+B puts the keyboard on the tray's "show
/// hidden icons" button, Enter opens its flyout.
pub fn open_hidden_icons() {
    press(&[VK_LWIN, VK_B]);
    std::thread::sleep(std::time::Duration::from_millis(120));
    press(&[VK_RETURN]);
}

/// Switch Windows between light and dark mode (apps and the system), as
/// Settings > Personalization > Colors does.
pub fn toggle_dark_mode() {
    use windows::Win32::System::Registry::*;
    use windows::Win32::UI::WindowsAndMessaging::{HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE};
    unsafe {
        let key = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
        let mut v: u32 = 1;
        let mut n = 4u32;
        let _ = RegGetValueW(HKEY_CURRENT_USER, key, w!("AppsUseLightTheme"), RRF_RT_REG_DWORD, None, Some(&mut v as *mut u32 as _), Some(&mut n));
        let new: u32 = if v == 0 { 1 } else { 0 };
        let bytes = new.to_le_bytes();
        for name in [w!("AppsUseLightTheme"), w!("SystemUsesLightTheme")] {
            let _ = RegSetKeyValueW(HKEY_CURRENT_USER, key, name, REG_DWORD.0, Some(bytes.as_ptr() as _), 4);
        }
        let _ = SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, WPARAM(0), LPARAM(w!("ImmersiveColorSet").as_ptr() as isize), SMTO_ABORTIFHUNG, 200, None);
    }
}
