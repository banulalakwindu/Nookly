//! Progress and badges on app icons: what apps tell the taskbar with
//! ITaskbarList3 (a download's progress, an unread badge).
//!
//! Apps look for the tray window (ours comes first, see tray.rs) and, on it,
//! the mark "TaskbandHWND": the window to tell. Without the mark their setup
//! fails and they tell nobody. Ours is a small window that notes what's told
//! and passes everything on to Windows' own, so it stays in the picture.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, Ordering};

use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use crate::apps::Icon;

/// Sent to the bar when something changed.
pub const WM_BAND: u32 = WM_APP + 43;
const MARK: PCWSTR = w!("TaskbandHWND");

// What apps send: wParam is their window.
const PROGRESS: u32 = WM_USER + 64; // lParam 0..=65534
const STATE: u32 = WM_USER + 65; // lParam a TBPF_* flag
const OVERLAY: u32 = WM_USER + 79; // lParam an icon, or 0

/// A window's progress (as the flags say: none, busy, normal, error, paused).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum State {
    None,
    Busy,
    Normal,
    Error,
    Paused,
}

impl State {
    fn from(flag: isize) -> State {
        match flag {
            1 => State::Busy,
            2 => State::Normal,
            4 => State::Error,
            8 => State::Paused,
            _ => State::None,
        }
    }
}

#[derive(Clone)]
struct Told {
    state: State,
    value: f32,
    badge: Option<(u32, Icon)>,
}

static TOLD: Mutex<Option<HashMap<isize, Told>>> = Mutex::new(None);
static BAR: AtomicIsize = AtomicIsize::new(0);
static WND: AtomicIsize = AtomicIsize::new(0);
static VER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn changed() {
    let b = BAR.load(Ordering::Relaxed);
    if b != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(b as *mut _)), WM_BAND, WPARAM(0), LPARAM(0));
        }
    }
}

/// Make the window (on the tray window's thread) and mark `tray` with it.
pub unsafe fn start(tray: HWND, bar: isize) {
    BAR.store(bar, Ordering::Relaxed);
    unsafe {
        let inst = GetModuleHandleW(None).unwrap_or_default();
        let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: inst.into(), lpszClassName: w!("NooklyTaskBand"), ..Default::default() };
        RegisterClassW(&wc);
        if let Ok(h) = CreateWindowExW(WS_EX_TOOLWINDOW, w!("NooklyTaskBand"), w!(""), WS_POPUP, 0, 0, 0, 0, None, None, Some(inst.into()), None) {
            WND.store(h.0 as isize, Ordering::Relaxed);
            let _ = SetPropW(tray, MARK, Some(HANDLE(h.0)));
        }
    }
}

pub unsafe fn stop(tray: HWND) {
    unsafe {
        let _ = RemovePropW(tray, MARK);
        let h = WND.swap(0, Ordering::Relaxed);
        if h != 0 {
            let _ = DestroyWindow(HWND(h as *mut _));
        }
    }
    if let Ok(mut t) = TOLD.lock() {
        *t = None;
    }
    changed();
}

/// Windows' own window for this.
fn theirs() -> Option<HWND> {
    let tray = nookly_core::shell::tray()?;
    let h = unsafe { GetPropW(tray, MARK) };
    (!h.is_invalid()).then_some(HWND(h.0))
}

unsafe extern "system" fn proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if msg < WM_USER && msg != WM_COPYDATA {
            return DefWindowProcW(h, msg, wp, lp);
        }
        // An icon is the app's to take back: copied now.
        let badge = (msg == OVERLAY && lp.0 != 0).then(|| crate::tray::icon_pixels(HICON(lp.0 as *mut _))).flatten();
        note(msg, wp.0 as isize, lp.0, badge);
        let mut res = 0usize;
        if let Some(t) = theirs() {
            let _ = SendMessageTimeoutW(t, msg, wp, lp, SMTO_ABORTIFHUNG, 1000, Some(&mut res));
        }
        LRESULT(res as isize)
    }
}

fn note(msg: u32, window: isize, l: isize, badge: Option<Icon>) {
    let Ok(mut g) = TOLD.lock() else { return };
    let map = g.get_or_insert_with(HashMap::new);
    let e = map.entry(window).or_insert(Told { state: State::None, value: 0.0, badge: None });
    let before = (e.state, (e.value * 100.0) as i32, e.badge.as_ref().map(|b| b.0));
    match msg {
        PROGRESS => {
            e.value = (l as f32 / 65534.0).clamp(0.0, 1.0);
            // A value shows as a plain bar unless it's said otherwise.
            if matches!(e.state, State::None | State::Busy) {
                e.state = State::Normal;
            }
        }
        STATE => {
            e.state = State::from(l);
            if e.state == State::None {
                e.value = 0.0;
            }
        }
        OVERLAY => e.badge = badge.map(|b| (VER.fetch_add(1, Ordering::Relaxed), b)),
        _ => {}
    }
    let after = (e.state, (e.value * 100.0) as i32, e.badge.as_ref().map(|b| b.0));
    if e.state == State::None && e.badge.is_none() {
        map.remove(&window);
    }
    map.retain(|w, _| unsafe { IsWindow(Some(HWND(*w as *mut _))).as_bool() });
    if before != after {
        changed();
    }
}

/// An app's progress: its windows', the one that matters most (an error, a
/// pause, then the least done).
pub fn progress(windows: &[HWND]) -> Option<(State, f32)> {
    let g = TOLD.lock().ok()?;
    let map = g.as_ref()?;
    let rank = |s: State| match s {
        State::Error => 0,
        State::Paused => 1,
        State::Normal => 2,
        State::Busy => 3,
        State::None => 9,
    };
    windows
        .iter()
        .filter_map(|h| map.get(&(h.0 as isize)))
        .filter(|t| t.state != State::None)
        .map(|t| (t.state, t.value))
        .min_by(|a, b| rank(a.0).cmp(&rank(b.0)).then(a.1.total_cmp(&b.1)))
}

/// An app's badge: its version and picture.
pub fn badge(windows: &[HWND]) -> Option<(u32, Icon)> {
    let g = TOLD.lock().ok()?;
    let map = g.as_ref()?;
    windows.iter().filter_map(|h| map.get(&(h.0 as isize))).find_map(|t| t.badge.clone())
}

/// Is anything busy (its bar moves)?
pub fn busy() -> bool {
    TOLD.lock().ok().and_then(|g| g.as_ref().map(|m| m.values().any(|t| t.state == State::Busy))).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states() {
        assert_eq!(State::from(0), State::None);
        assert_eq!(State::from(1), State::Busy);
        assert_eq!(State::from(2), State::Normal);
        assert_eq!(State::from(4), State::Error);
        assert_eq!(State::from(8), State::Paused);
    }
}
