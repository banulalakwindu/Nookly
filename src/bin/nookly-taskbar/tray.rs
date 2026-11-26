//! The notification area ("hidden icons"): the apps' tray icons, shown by
//! the taskbar while Windows' own taskbar is hidden (Windows won't open its
//! hidden-icons flyout then).
//!
//! Apps add, change and remove their icons with Shell_NotifyIcon, which
//! sends a WM_COPYDATA to the first window of class "Shell_TrayWnd". This
//! runs one on its own thread, above Windows' in the z-order so it's found
//! first: each message is read (to keep a copy of the icon) and passed on
//! to Windows' own tray unchanged, with Windows' answer going back to the
//! app. So nothing is lost: when this stops, Windows still has every icon.
//!
//! Clicks on an icon go to its app as Windows sends them (the mouse
//! messages, then NIN_SELECT or WM_CONTEXTMENU for apps on version 4).

use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, Ordering};

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;

use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

/// Sent to the bar when the icons changed.
pub const WM_TRAY_CHANGED: u32 = WM_APP + 40;
/// The window property that marks our tray window (so lookups for Windows'
/// own skip it: see shell::tray).
pub const MARK: PCWSTR = nookly_core::shell::TRAY_MARK;

#[derive(Clone)]
pub struct TrayIcon {
    pub owner: isize,
    pub id: u32,
    pub guid: Option<GUID>,
    pub callback: u32,
    pub version: u32,
    pub tip: String,
    pub hidden: bool,
    /// Premultiplied BGRA (size, pixels), and a number that changes with it.
    pub icon: Option<(u32, Vec<u8>)>,
    pub icon_ver: u32,
    pub pid: u32,
}

impl TrayIcon {
    /// A stable key for caches.
    pub fn key(&self) -> String {
        match self.guid {
            Some(g) => format!("{g:?}"),
            None => format!("{}:{}", self.owner, self.id),
        }
    }
}

static ICONS: Mutex<Vec<TrayIcon>> = Mutex::new(Vec::new());
static BAR: AtomicIsize = AtomicIsize::new(0);
static WND: AtomicIsize = AtomicIsize::new(0);
static VER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// The icons now (visible ones), in the order they were added.
pub fn icons() -> Vec<TrayIcon> {
    ICONS.lock().map(|l| l.iter().filter(|i| !i.hidden && i.icon.is_some()).cloned().collect()).unwrap_or_default()
}

/// The pictures' icons.
pub fn demo(list: Vec<TrayIcon>) {
    if let Ok(mut l) = ICONS.lock() {
        *l = list;
    }
}

/// Running?
pub fn running() -> bool {
    WND.load(Ordering::Relaxed) != 0
}

/// Start the tray (once); `bar` hears WM_TRAY_CHANGED.
pub fn start(bar: HWND) {
    BAR.store(bar.0 as isize, Ordering::Relaxed);
    if running() {
        return;
    }
    WND.store(-1, Ordering::Relaxed);
    std::thread::spawn(|| unsafe { run() });
}

/// Stop it (its icons stay with Windows).
pub fn stop() {
    let h = WND.load(Ordering::Relaxed);
    if h > 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(h as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

unsafe fn run() {
    unsafe {
        let inst = GetModuleHandleW(None).unwrap_or_default();
        let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: inst.into(), lpszClassName: w!("Shell_TrayWnd"), ..Default::default() };
        RegisterClassW(&wc);
        let Ok(h) = CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_TOPMOST, w!("Shell_TrayWnd"), w!(""), WS_POPUP, 0, 0, 0, 0, None, None, Some(inst.into()), None) else {
            WND.store(0, Ordering::Relaxed);
            return;
        };
        let _ = SetPropW(h, MARK, Some(HANDLE(1 as *mut _)));
        WND.store(h.0 as isize, Ordering::Relaxed);
        // What apps tell the taskbar (progress, badges) comes to us too.
        crate::band::start(h, BAR.load(Ordering::Relaxed));
        on_top(h);
        SetTimer(Some(h), 1, 2000, None);
        // Apps add their icons again when they hear this (now to us).
        let msg = RegisterWindowMessageW(w!("TaskbarCreated"));
        let _ = SendNotifyMessageW(HWND_BROADCAST, msg, WPARAM(0), LPARAM(0));
        let mut m = MSG::default();
        while GetMessageW(&mut m, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&m);
            DispatchMessageW(&m);
        }
        WND.store(0, Ordering::Relaxed);
    }
}

/// Above Windows' tray in the z-order (Shell_NotifyIcon finds the first).
fn on_top(h: HWND) {
    unsafe {
        let _ = SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

/// Run `f` with our tray window behind Windows' own, so what `f` asks of
/// the taskbar (the screen strip) goes straight to Windows. Passed on by
/// us, to ourselves, Windows says yes to the strip's position but never
/// keeps it free (maximized windows went under the bar).
pub fn step_aside<R>(f: impl FnOnce() -> R) -> R {
    let h = WND.load(Ordering::Relaxed);
    if h <= 0 {
        return f();
    }
    let h = HWND(h as *mut _);
    unsafe {
        let _ = SetWindowPos(h, Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
    let r = f();
    on_top(h);
    r
}

fn changed() {
    VER.fetch_add(1, Ordering::Relaxed);
    let b = BAR.load(Ordering::Relaxed);
    if b != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(b as *mut _)), WM_TRAY_CHANGED, WPARAM(0), LPARAM(0));
        }
    }
}

unsafe extern "system" fn proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_COPYDATA => {
                let theirs = nookly_core::shell::tray();
                // Windows' answer is the app's answer.
                let mut res = 0usize;
                let passed = theirs.is_some_and(|t| SendMessageTimeoutW(t, WM_COPYDATA, wp, lp, SMTO_ABORTIFHUNG, 3000, Some(&mut res)).0 != 0);
                if let Some(cds) = (lp.0 as *const COPYDATASTRUCT).as_ref() {
                    if cds.dwData == 1 && !cds.lpData.is_null() && cds.cbData >= 8 + 24 {
                        let data = std::slice::from_raw_parts(cds.lpData as *const u8, cds.cbData as usize);
                        let ok = read(data);
                        if !passed {
                            return LRESULT(ok as isize);
                        }
                    }
                }
                LRESULT(if passed { res as isize } else { 0 })
            }
            WM_TIMER => {
                on_top(h);
                // Icons whose app went away without saying so.
                let gone = ICONS.lock().map(|mut l| {
                    let n = l.len();
                    l.retain(|i| IsWindow(Some(HWND(i.owner as *mut _))).as_bool());
                    n != l.len()
                });
                if gone.unwrap_or(false) {
                    changed();
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                crate::band::stop(h);
                let _ = DestroyWindow(h);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            // Anything else apps send the tray: Windows' answer.
            m if m >= WM_USER => match nookly_core::shell::tray() {
                Some(t) => {
                    let mut res = 0usize;
                    let _ = SendMessageTimeoutW(t, m, wp, lp, SMTO_ABORTIFHUNG, 1000, Some(&mut res));
                    LRESULT(res as isize)
                }
                None => DefWindowProcW(h, msg, wp, lp),
            },
            _ => DefWindowProcW(h, msg, wp, lp),
        }
    }
}

fn u32_at(d: &[u8], o: usize) -> u32 {
    d.get(o..o + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn text_at(d: &[u8], o: usize, chars: usize) -> String {
    let b = d.get(o..(o + chars * 2).min(d.len())).unwrap_or(&[]);
    let w: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&c| c != 0).collect();
    String::from_utf16_lossy(&w)
}

const NIF_ICON: u32 = 0x2;
const NIF_TIP: u32 = 0x4;
const NIF_STATE: u32 = 0x8;
const NIF_GUID: u32 = 0x20;
const NIF_MESSAGE: u32 = 0x1;
const NIS_HIDDEN: u32 = 0x1;

/// One Shell_NotifyIcon message (SHELLTRAYDATA: signature, message, then
/// a NOTIFYICONDATA with 32-bit handles). Returns whether it made sense.
fn read(d: &[u8]) -> bool {
    if u32_at(d, 0) != 0x3475_3423 {
        return false;
    }
    let what = u32_at(d, 4);
    let n = &d[8..];
    let size = (u32_at(n, 0) as usize).min(n.len());
    let n = &n[..size];
    let owner = u32_at(n, 4) as i32 as isize;
    let id = u32_at(n, 8);
    let flags = u32_at(n, 12);
    let modern = size >= 952;
    let guid = (modern && flags & NIF_GUID != 0).then(|| {
        let g = &n[936..952];
        GUID::from_values(u32_at(g, 0), u16::from_le_bytes([g[4], g[5]]), u16::from_le_bytes([g[6], g[7]]), [g[8], g[9], g[10], g[11], g[12], g[13], g[14], g[15]])
    });
    let mut list = match ICONS.lock() {
        Ok(l) => l,
        Err(_) => return false,
    };
    let find = |l: &Vec<TrayIcon>| l.iter().position(|i| match (guid, i.guid) {
        (Some(a), Some(b)) => a == b,
        _ => i.owner == owner && i.id == id,
    });
    let apply = |i: &mut TrayIcon| {
        if flags & NIF_MESSAGE != 0 {
            i.callback = u32_at(n, 16);
        }
        if flags & NIF_ICON != 0 {
            i.icon = icon_pixels(HICON(u32_at(n, 20) as usize as *mut _));
            i.icon_ver = i.icon_ver.wrapping_add(1);
        }
        if flags & NIF_TIP != 0 {
            i.tip = text_at(n, 24, if size >= 280 { 128 } else { 64 });
        }
        if flags & NIF_STATE != 0 && size >= 288 {
            let (state, mask) = (u32_at(n, 280), u32_at(n, 284));
            if mask & NIS_HIDDEN != 0 {
                i.hidden = state & NIS_HIDDEN != 0;
            }
        }
    };
    let ok = match what {
        // NIM_ADD
        0 => {
            if find(&list).is_some() {
                false
            } else {
                let mut pid = 0u32;
                unsafe {
                    GetWindowThreadProcessId(HWND(owner as *mut _), Some(&mut pid));
                }
                let mut i = TrayIcon { owner, id, guid, callback: 0, version: 0, tip: String::new(), hidden: false, icon: None, icon_ver: 0, pid };
                apply(&mut i);
                list.push(i);
                true
            }
        }
        // NIM_MODIFY
        1 => match find(&list) {
            Some(k) => {
                apply(&mut list[k]);
                true
            }
            None => false,
        },
        // NIM_DELETE
        2 => match find(&list) {
            Some(k) => {
                list.remove(k);
                true
            }
            None => false,
        },
        // NIM_SETVERSION
        4 => match find(&list) {
            Some(k) => {
                list[k].version = if size >= 804 { u32_at(n, 800) } else { 0 };
                true
            }
            None => false,
        },
        _ => true,
    };
    drop(list);
    if ok {
        changed();
    }
    ok
}

/// An icon's pixels (premultiplied BGRA, square).
pub fn icon_pixels(h: HICON) -> Option<(u32, Vec<u8>)> {
    if h.0.is_null() {
        return None;
    }
    unsafe {
        let mut ii = ICONINFO::default();
        GetIconInfo(h, &mut ii).ok()?;
        let read = |bmp: HBITMAP| -> Option<(u32, u32, Vec<u8>)> {
            if bmp.is_invalid() {
                return None;
            }
            let mut bm = BITMAP::default();
            GetObjectW(bmp.into(), std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _));
            let (w, hh) = (bm.bmWidth.max(1) as u32, bm.bmHeight.unsigned_abs().max(1));
            let mut bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w as i32, biHeight: -(hh as i32), biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0, ..Default::default() },
                ..Default::default()
            };
            let mut px = vec![0u8; (w * hh * 4) as usize];
            let dc = GetDC(None);
            let got = GetDIBits(dc, bmp, 0, hh, Some(px.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
            ReleaseDC(None, dc);
            (got != 0).then_some((w, hh, px))
        };
        let color = read(ii.hbmColor);
        let mask = read(ii.hbmMask);
        if !ii.hbmColor.is_invalid() {
            let _ = DeleteObject(ii.hbmColor.into());
        }
        if !ii.hbmMask.is_invalid() {
            let _ = DeleteObject(ii.hbmMask.into());
        }
        let (w, hh, mut px) = color?;
        if px.chunks_exact(4).all(|p| p[3] == 0) {
            // No alpha: the mask says what shows (black in it = opaque).
            for (k, p) in px.chunks_exact_mut(4).enumerate() {
                let m = mask.as_ref().and_then(|m| m.2.get(k * 4)).copied().unwrap_or(0);
                p[3] = if m == 0 { 255 } else { 0 };
            }
        }
        for p in px.chunks_exact_mut(4) {
            let a = p[3] as u32;
            for c in 0..3 {
                p[c] = ((p[c] as u32 * a + 127) / 255) as u8;
            }
        }
        if w != hh {
            return None;
        }
        Some((w, px))
    }
}

/// Click an icon as Windows does: `right` for the right button; `at` is
/// where (screen pixels) for apps that place their menu there.
pub fn click(icon: &TrayIcon, right: bool, at: (i32, i32)) {
    unsafe {
        // Let it come to the front (its menu or window).
        let _ = AllowSetForegroundWindow(icon.pid);
        let owner = HWND(icon.owner as *mut _);
        let send = |m: u32| {
            let (w, l) = if icon.version >= 4 {
                (((at.0 as u16 as usize) | ((at.1 as u16 as usize) << 16)), ((m as usize) | ((icon.id as usize & 0xFFFF) << 16)) as isize)
            } else {
                (icon.id as usize, m as isize)
            };
            let _ = SendNotifyMessageW(owner, icon.callback, WPARAM(w), LPARAM(l));
        };
        if right {
            send(WM_RBUTTONDOWN);
            send(WM_RBUTTONUP);
            if icon.version >= 4 {
                send(WM_CONTEXTMENU);
            }
        } else {
            send(WM_LBUTTONDOWN);
            send(WM_LBUTTONUP);
            if icon.version >= 4 {
                send(0x400); // NIN_SELECT
            }
        }
    }
}
