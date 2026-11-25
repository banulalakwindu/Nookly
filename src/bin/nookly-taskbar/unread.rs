//! The number on the bell: notifications that arrived since the
//! notification centre was last opened.
//!
//! Only counted: their text, sender and app are never read. Windows tells
//! an ordinary app about new ones only through its notification database's
//! time of change (checked once a second, which costs next to nothing);
//! the list is asked for only then.

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicIsize, AtomicU32, Ordering};
use std::time::Duration;

use windows::UI::Notifications::Management::*;
use windows::UI::Notifications::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Sent to the bar when the count changed.
pub const WM_UNREAD: u32 = WM_APP + 41;
const SEEN_FILE: &str = "taskbar_seen.txt";

static COUNT: AtomicU32 = AtomicU32::new(0);
static SEEN: AtomicI64 = AtomicI64::new(0);
static ON: AtomicBool = AtomicBool::new(false);
static AGAIN: AtomicBool = AtomicBool::new(false);
static STARTED: AtomicBool = AtomicBool::new(false);
static BAR: AtomicIsize = AtomicIsize::new(0);

fn now() -> i64 {
    let f = unsafe { GetSystemTimeAsFileTime() };
    ((f.dwHighDateTime as i64) << 32) | f.dwLowDateTime as i64
}

pub fn count() -> u32 {
    COUNT.load(Ordering::Relaxed)
}

/// Opened: nothing is new any more.
pub fn mark_seen() {
    let t = now();
    SEEN.store(t, Ordering::Relaxed);
    COUNT.store(0, Ordering::Relaxed);
    nookly_core::store::write(SEEN_FILE, &t.to_string());
}

/// How many of these times (100 ns since 1601) are after `seen`.
fn newer(times: impl Iterator<Item = i64>, seen: i64) -> u32 {
    times.filter(|&t| t > seen).count() as u32
}

/// Count while the bell is on the bar.
pub fn enable(bar: HWND, on: bool) {
    BAR.store(bar.0 as isize, Ordering::Relaxed);
    if ON.swap(on, Ordering::Relaxed) == on {
        return;
    }
    if !on {
        COUNT.store(0, Ordering::Relaxed);
        return;
    }
    AGAIN.store(true, Ordering::Relaxed);
    if !STARTED.swap(true, Ordering::Relaxed) {
        // The first time, what's already there isn't new.
        match nookly_core::store::read(SEEN_FILE).trim().parse() {
            Ok(t) => SEEN.store(t, Ordering::Relaxed),
            Err(_) => mark_seen(),
        }
        std::thread::spawn(watch);
    }
}

fn watch() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let dir = std::path::PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default()).join(r"Microsoft\Windows\Notifications");
    let stat = |f: &str| std::fs::metadata(dir.join(f)).and_then(|m| m.modified()).ok();
    let mut listener: Option<UserNotificationListener> = None;
    let mut last = None;
    loop {
        std::thread::sleep(Duration::from_millis(1000));
        if !ON.load(Ordering::Relaxed) {
            continue;
        }
        let sig = (stat("wpndatabase.db-wal"), stat("wpndatabase.db"));
        if !AGAIN.swap(false, Ordering::Relaxed) && last == Some(sig) {
            continue;
        }
        last = Some(sig);
        if listener.is_none() {
            listener = UserNotificationListener::Current().ok();
        }
        let Some(l) = listener.as_ref() else { continue };
        // Turned off in Windows' privacy settings: no number.
        if !l.GetAccessStatus().is_ok_and(|s| s == UserNotificationListenerAccessStatus::Allowed) {
            continue;
        }
        let Ok(list) = l.GetNotificationsAsync(NotificationKinds::Toast).and_then(|op| op.join()) else { continue };
        let n = newer(list.into_iter().filter_map(|n| n.CreationTime().ok()).map(|t| t.UniversalTime), SEEN.load(Ordering::Relaxed));
        if COUNT.swap(n, Ordering::Relaxed) != n {
            unsafe {
                let _ = PostMessageW(Some(HWND(BAR.load(Ordering::Relaxed) as *mut _)), WM_UNREAD, WPARAM(0), LPARAM(0));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_newer_count() {
        assert_eq!(super::newer([1, 5, 9, 12].into_iter(), 5), 2);
        assert_eq!(super::newer([].into_iter(), 5), 0);
    }
}
