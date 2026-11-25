//! Peek: resting on a window in an app's pop-up shows that window on the
//! screen, as Windows' own taskbar does. The other windows turn to glass
//! and come back by themselves when it ends; nothing is moved or activated.
//! Windows' taskbar uses this DWM call; it isn't documented, so without it
//! (or if it refuses) there's simply no peek.

use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::core::*;

type LivePreview = unsafe extern "system" fn(u32, HWND, HWND, u32, *const core::ffi::c_void) -> HRESULT;
/// Peek at one window (1 is the desktop).
const WINDOW: u32 = 3;

static ON: AtomicBool = AtomicBool::new(false);

fn live_preview() -> Option<LivePreview> {
    static F: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
    let p = *F.get_or_init(|| unsafe {
        let m = LoadLibraryW(w!("dwmapi.dll")).ok()?;
        GetProcAddress(m, PCSTR(113 as *const u8)).map(|f| f as usize)
    });
    p.map(|p| unsafe { std::mem::transmute::<usize, LivePreview>(p) })
}

/// Show `h` (`over`: our window that stays on top).
pub fn start(h: HWND, over: HWND) -> bool {
    let ok = live_preview().is_some_and(|f| unsafe { f(1, h, over, WINDOW, std::ptr::null()) }.is_ok());
    if ok {
        ON.store(true, Ordering::Relaxed);
    }
    ok
}

/// End the peek, if there is one (safe from anywhere, any time).
pub fn stop() {
    if ON.swap(false, Ordering::Relaxed) {
        if let Some(f) = live_preview() {
            unsafe {
                let _ = f(0, HWND::default(), HWND::default(), WINDOW, std::ptr::null());
            }
        }
    }
}
