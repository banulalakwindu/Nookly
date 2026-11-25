#![windows_subsystem = "windows"]
//! Nookly Taskbar: the optional taskbar add-on (`nookly-taskbar.exe`).
//!
//! It replaces the Windows taskbar with a bar on the bottom edge that has
//! the notch's own shape (flat on the screen edge, rounded corners, flares)
//! and theme, and grows or shrinks to fit what's in it. The notch
//! (`nookly.exe`) starts it, watches it and owns its settings; this exe only
//! draws the bar and handles its clicks.
//!
//!   main.rs    window, frame loop, input, Windows taskbar hide/restore
//!   items.rs   what's in the bar, how each item draws and clicks, pop-ups
//!   apps.rs    open and pinned apps, collections, their icons
//!   bubble.rs  pop-ups that grow out of the bar like the notch
//!   menu.rs    right-click menus (bubbles too)
//!   jump.rs    an app's recent files and tasks, for its menu
//!   band.rs    progress and badges apps report
//!   drop.rs    files dragged onto the bar
//!   peek.rs    showing a window while you rest on it in a pop-up
//!   unread.rs  the number on the bell
//!   layout.rs  fitting the bar to the screen (pure, tested)

mod apps;
mod band;
mod bubble;
mod drop;
mod items;
mod jump;
mod layout;
mod menu;
mod peek;
mod tray;
mod unread;

use std::collections::HashMap;
use std::time::Instant;

use nookly_core::gfx::*;
use nookly_core::store::Settings;
use nookly_core::ui::{Hit, Ui};
use nookly_core::{glass, shell, time};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::{ID2D1Bitmap, ID2D1PathGeometry};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

/// Window class; the notch finds the bar by it.
pub const CLASS: PCWSTR = w!("NooklyTaskbar");
/// The bars on the other screens (the notch deals with the main one).
pub const MORE_CLASS: PCWSTR = w!("NooklyTaskbarMore");
/// Bar height (DIPs at 100%), corner radius.
const BAR_H: f32 = 48.0;
const RAD: f32 = 16.0;
/// A bubble keeps this far inside the ends of what it grows out of (room
/// for its flares and the parent's rounded corners).
pub const HOLD: f32 = 44.0;
/// Hover this long on an app (or the overflow tile) to open its pop-up.
const HOVER_OPEN: f32 = 0.45;
/// Rest this long on a window in a pop-up to see it on the screen.
const PEEK_WAIT: f32 = 0.35;
/// An app asking to be looked at: its glow beats this long, and an
/// auto-hidden bar stays out as long.
const ATTENTION_BEATS: f32 = 3.2;
/// A menu closes on a click elsewhere or Esc; left alone this long, also
/// by itself (in case neither was heard).
const MENU_LEAVE: f32 = 20.0;
/// Our AppBar's callback message.
const WM_APPBAR: u32 = WM_APP + 1;
/// Re-check that the Windows taskbar stays hidden.
const WM_KEEP_HIDDEN: u32 = WM_APP + 2;
/// A window came, went or changed: read the app list again.
const WM_APPS_CHANGED: u32 = WM_APP + 3;
const WM_MOUSELEAVE: u32 = 0x02A3;

thread_local! {
    /// A bar for every screen; the first is the main screen's.
    static BARS: std::cell::RefCell<Vec<Bar>> = const { std::cell::RefCell::new(Vec::new()) };
    /// What the notch was last told (see `tell_shown`).
    static TOLD_OUT: std::cell::Cell<(bool, u64)> = const { std::cell::Cell::new((true, u64::MAX)) };
}

/// The main screen's bar.
fn with_bar<R>(f: impl FnOnce(&mut Bar) -> R) -> Option<R> {
    on_bar(Some(0), f)
}

/// The bar at this place.
pub fn on_bar<R>(k: Option<usize>, f: impl FnOnce(&mut Bar) -> R) -> Option<R> {
    BARS.with(|b| b.try_borrow_mut().ok().and_then(|mut b| b.get_mut(k?).map(f)))
}

/// Every bar.
fn each_bar(mut f: impl FnMut(&mut Bar)) {
    BARS.with(|b| {
        if let Ok(mut b) = b.try_borrow_mut() {
            b.iter_mut().for_each(&mut f);
        }
    });
}

/// Which bar a window belongs to (its own, or one of its pop-ups).
pub fn bar_of(hwnd: HWND) -> Option<usize> {
    BARS.with(|b| b.try_borrow().ok().and_then(|b| b.iter().position(|x| x.hwnd == hwnd || x.is_pop(hwnd))))
}

/// The screens changed, or which of them have a bar.
static SCREENS_DIRTY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The apps' order was changed on one of the bars.
pub static ORDER_DIRTY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A spring toward a target.
#[derive(Clone, Copy, Default)]
struct Spring {
    pos: f32,
    vel: f32,
}

impl Spring {
    /// One step toward `target`: `w` stiffness (rad/s), `z` damping ratio
    /// (under 1 overshoots and wobbles, like the notch).
    fn step(&mut self, target: f32, dt: f32, w: f32, z: f32) {
        let a = w * w * (target - self.pos) - 2.0 * z * w * self.vel;
        self.vel += a * dt;
        self.pos += self.vel * dt;
    }
}

/// The notch's springs: the width overshoots about 12% with a small second
/// wobble and the depth follows a little behind, so the bar stretches and
/// settles like a drop of water. (stiffness, damping ratio)
const SPRING_W: (f32, f32) = (19.0, 0.55);
const SPRING_D: (f32, f32) = (16.0, 0.62);


/// A pop-up above the bar: an app's windows (index into the app list) or
/// the apps that didn't fit; with the x it opens above (window DIPs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pop {
    App(usize, f32),
    Overflow(f32),
    /// A collection (index into `apps.groups`).
    Group(usize, f32),
    /// A widget's full view (its place on the bar, barcfg::placed).
    Widget(usize, f32),
    /// The hidden icons (the apps' tray icons).
    Tray(f32),
    /// A right-click menu (`Bar::menu`).
    Menu(f32),
}

/// Where a dragged app would go: onto an app or a collection (its hit id),
/// or before the app at this place in the list (true: among the bar's
/// tiles, false: in the "more apps" grid).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Drop {
    Onto(u32),
    Between(usize, bool),
}

impl Pop {
    /// Same pop-up (whatever x it opens above)?
    fn same(a: Option<Pop>, b: Pop) -> bool {
        matches!((a, b), (Some(Pop::App(x, _)), Pop::App(y, _)) if x == y)
            || matches!((a, b), (Some(Pop::Overflow(_)), Pop::Overflow(_)))
            || matches!((a, b), (Some(Pop::Group(x, _)), Pop::Group(y, _)) if x == y)
            || matches!((a, b), (Some(Pop::Widget(x, _)), Pop::Widget(y, _)) if x == y)
            || matches!((a, b), (Some(Pop::Tray(_)), Pop::Tray(_)))
            || matches!((a, b), (Some(Pop::Menu(_)), Pop::Menu(_)))
    }
}

/// Waiting for a hidden icon's menu: its app, the pop-up windows it had
/// before the click, when that was, and whether a menu has been seen.
struct TrayHold {
    pid: u32,
    before: Vec<HWND>,
    since: Instant,
    looked: Instant,
    open: bool,
}

/// How long an icon's app may take to show its menu.
const TRAY_WAIT: f32 = 1.5;

/// An app's windows that could be a menu: visible, without a caption, not
/// big, no tooltip.
fn menus_of(pid: u32) -> Vec<HWND> {
    unsafe extern "system" fn each(h: HWND, l: LPARAM) -> BOOL {
        unsafe {
            let v = &mut *(l.0 as *mut (u32, Vec<HWND>));
            let mut p = 0u32;
            GetWindowThreadProcessId(h, Some(&mut p));
            if p != v.0 || !IsWindowVisible(h).as_bool() || GetWindowLongW(h, GWL_STYLE) as u32 & WS_CAPTION.0 == WS_CAPTION.0 {
                return TRUE;
            }
            let mut cloaked = 0u32;
            let _ = windows::Win32::Graphics::Dwm::DwmGetWindowAttribute(h, windows::Win32::Graphics::Dwm::DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut _, 4);
            let mut r = RECT::default();
            let _ = GetWindowRect(h, &mut r);
            let mut c = [0u16; 64];
            let n = GetClassNameW(h, &mut c);
            let class = String::from_utf16_lossy(&c[..n.max(0) as usize]).to_lowercase();
            let (w, hh) = (r.right - r.left, r.bottom - r.top);
            if cloaked == 0 && w > 30 && hh > 12 && w < 1000 && hh < 1400 && !class.contains("tooltip") {
                v.1.push(h);
            }
            TRUE
        }
    }
    let mut v: (u32, Vec<HWND>) = (pid, Vec::new());
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut v as *mut _ as isize));
    }
    v.1
}

/// Is an ordinary Windows menu open (anybody's)?
fn menu_showing() -> bool {
    let mut prev: Option<HWND> = None;
    unsafe {
        while let Ok(h) = FindWindowExW(None, prev, w!("#32768"), PCWSTR::null()) {
            if h.is_invalid() {
                break;
            }
            if IsWindowVisible(h).as_bool() {
                return true;
            }
            prev = Some(h);
        }
    }
    false
}

/// An app pressed (on the bar or in the "more apps" grid): dragged far
/// enough, it's being moved, or dropped on another app (a new collection)
/// or a collection (added to it).
#[derive(Clone)]
struct Drag {
    /// Its key (the list can change meanwhile).
    key: String,
    start: (f32, f32),
    active: bool,
}

/// App icons as bitmaps, made from the pixels `apps` keeps.
#[derive(Default)]
pub struct Icons {
    map: HashMap<String, (u32, ID2D1Bitmap)>,
}

impl Icons {
    pub fn get(&mut self, g: &mut Gfx, list: &mut apps::Apps, app: &apps::App, px: u32) -> Option<ID2D1Bitmap> {
        // Per drawing surface: the bar's and the pop-up's can't share bitmaps.
        let k = format!("{}@{px}#{}", app.key, g.id);
        if let Some((made, b)) = self.map.get(&k) {
            if *made == g.generation {
                return Some(b.clone());
            }
        }
        let (s, px_buf) = list.icon(app, px)?.clone();
        let b = g.make_bitmap(s, s, &px_buf)?;
        self.map.insert(k, (g.generation, b.clone()));
        Some(b)
    }

    /// A badge (its version makes it new).
    pub fn badge(&mut self, g: &mut Gfx, ver: u32, icon: &apps::Icon) -> Option<ID2D1Bitmap> {
        let k = format!("badge{ver}#{}", g.id);
        if let Some((made, b)) = self.map.get(&k) {
            if *made == g.generation {
                return Some(b.clone());
            }
        }
        let b = g.make_bitmap(icon.0, icon.0, &icon.1)?;
        // (Few at a time: the older ones go.)
        if self.map.keys().filter(|k| k.starts_with("badge")).count() > 24 {
            self.map.retain(|k, _| !k.starts_with("badge"));
        }
        self.map.insert(k, (g.generation, b.clone()));
        Some(b)
    }
}

/// The widget faces the notch drew (see surfaces.rs), and their bitmaps on
/// the bar's drawing surface.
#[derive(Default)]
pub struct Faces {
    pub list: Vec<nookly_core::surfaces::Face>,
    bitmaps: HashMap<String, (u32, u32, ID2D1Bitmap)>,
    /// Each face's version (bumped when its pixels change; its bitmap is
    /// made again then, and only then).
    versions: HashMap<String, u32>,
    set: u32,
}

impl Faces {
    pub fn get(&self, id: &str) -> Option<&nookly_core::surfaces::Face> {
        self.list.iter().find(|f| f.id == id)
    }

    /// Face `id` as a bitmap on `g`, and its width in DIPs.
    pub fn bitmap(&mut self, g: &mut Gfx, id: &str) -> Option<(ID2D1Bitmap, f32)> {
        let f = self.list.iter().find(|f| f.id == id)?;
        let k = format!("{id}#{}", g.id);
        let v = self.versions.get(id).copied().unwrap_or(0);
        if let Some((made, set, b)) = self.bitmaps.get(&k) {
            if *made == g.generation && *set == v {
                return Some((b.clone(), f.dip_w));
            }
        }
        let b = g.make_bitmap(f.w, f.h, &f.px)?;
        self.bitmaps.insert(k, (g.generation, v, b.clone()));
        Some((b, f.dip_w))
    }

    pub fn reload(&mut self) -> bool {
        // Only faces whose version changed bring their pixels.
        let known: Vec<(String, u32)> = self.list.iter().map(|f| (f.id.clone(), self.versions.get(&f.id).copied().unwrap_or(0))).collect();
        let Some(got) = nookly_core::surfaces::read_since(&|id, v| known.iter().any(|(k, kv)| k == id && *kv == v)) else { return false };
        let mut changed = got.len() != self.list.len();
        let mut list = Vec::with_capacity(got.len());
        for (f, v, have) in got {
            if have {
                if let Some(old) = self.list.iter().find(|x| x.id == f.id) {
                    list.push(old.clone());
                    continue;
                }
            }
            changed = true;
            self.versions.insert(f.id.clone(), v);
            list.push(f);
        }
        if changed || list.iter().zip(&self.list).any(|(a, b)| a.id != b.id) {
            self.list = list;
            self.set += 1;
            true
        } else {
            false
        }
    }

}

/// Ask the notch to draw the faces again (it starts drawing when asked, and
/// draws for Liquid Glass light or dark behind the bar).
fn want_faces(glass_light: bool, scale: f32) {
    unsafe {
        if let Ok(notch) = FindWindowW(w!("NooklyNotch"), PCWSTR::null()) {
            let msg = RegisterWindowMessageW(nookly_core::surfaces::WANT_MSG);
            let _ = PostMessageW(Some(notch), msg, WPARAM(glass_light as usize), LPARAM((scale * 1000.0).round() as isize));
        }
    }
}

pub struct Bar {
    pub hwnd: HWND,
    /// The screen it's on (its device name; empty: the main one, whichever
    /// that is).
    device: String,
    /// What can be seen of it (out, and which widgets).
    out_seen: (bool, u64),
    /// Used from the keyboard: the thing it's on, and the window to go back
    /// to.
    keys: Option<(Option<u32>, HWND)>,
    /// What it was on on the bar itself (to come back to from a pop-up).
    key_home: Option<u32>,
    /// Files are being dragged over this (a hit id), since when, and it has
    /// been acted on.
    file_over: Option<(u32, Instant, bool)>,
    /// The pop-up bubble over the bar, and the windows bubble grown out of
    /// its grid (the "more apps" tile's or a collection's).
    pop_b: Option<bubble::Bubble>,
    sub_b: Option<bubble::Bubble>,
    /// The app whose windows the inner bubble shows, and the x of its cell.
    pub sub_app: Option<(apps::App, f32)>,
    /// The grid cell under the mouse and since when; when the mouse left
    /// the inner bubble.
    sub_hover: Option<(u32, Instant)>,
    sub_leave: Option<Instant>,
    /// Where live previews go in the bubble being drawn (its own coordinates).
    pub thumb_slots: Vec<(HWND, Rect)>,
    /// Just clicked: no hover pop-up for this tile until the mouse leaves it.
    pub hover_block: Option<u32>,
    drag: Option<Drag>,
    /// The open right-click menu (kept while its bubble closes).
    pub menu: Option<menu::Menu>,
    /// The window in a pop-up the mouse rests on, the one shown on the
    /// screen for it, and when the mouse went off one.
    peek_hover: Option<(HWND, Instant)>,
    peeked: Option<HWND>,
    peek_left: Instant,
    /// The open menu has the arrows and Enter.
    menu_keys: bool,
    /// A hidden icon was clicked: the bar stays while its app's menu is open.
    tray_hold: Option<TrayHold>,
    /// When an app last started asking to be looked at.
    attention_at: Option<Instant>,
    /// The screen strip last reserved (fixed mode).
    appbar_rc: RECT,
    /// When the strip was last looked at, and how often it was asked for again.
    strip_at: Instant,
    strip_tries: u32,
    /// The drawing surface (taken out while a frame is drawn).
    gfx: Option<Gfx>,
    pub settings: Settings,
    /// Monitor rectangle (pixels) and DIP scale.
    mon: RECT,
    pub scale: f32,
    /// Window size in DIPs (the window is the screen's width; it's taller
    /// only while a pop-up is open).
    pub w: f32,
    pub h: f32,
    pub hits: Vec<Hit>,
    pub mouse: (f32, f32),
    inside: bool,
    pub down: Option<u32>,
    /// Body width and how far it's shown (0 hidden .. 1 shown).
    /// Body width and depth (DIPs: 0 hidden in the edge .. BAR_H out), on
    /// the notch's springs, and the pop-up's (0 .. 1 open).
    width: Spring,
    depth: Spring,
    last_step: Instant,
    start: Instant,
    /// Auto-hide: when the cursor left, and whether a full-screen app is in front.
    left_at: Option<Instant>,
    fullscreen: bool,
    appbar: bool,
    /// Liquid Glass: blurred strip behind the bar, and when to look again.
    backdrop: Option<ID2D1Bitmap>,
    backdrop_gen: u32,
    /// A fingerprint of it (the same picture again needs no new frame).
    backdrop_key: u64,
    /// Nothing moved at the last step, and nothing is waited for.
    resting: bool,
    /// Drawn while slid away (and whether that was for a full-screen app).
    drawn_away: Option<bool>,
    glass_light: bool,
    probe: Option<u64>,
    next_probe: Instant,
    next_grab: Instant,
    /// What the bar holds, in order (Settings > Taskbar > Layout).
    pub order: Vec<(items::Item, nookly_core::barcfg::Zone)>,
    /// The widgets on the bar (barcfg::placed), read with the settings.
    pub placed: Vec<nookly_core::barcfg::Placed>,
    pub apps: apps::Apps,
    pub icons: Icons,
    /// Widget values from the notch (the feed).
    pub feed: Vec<nookly_core::feed::Item>,
    pub faces: Faces,
    /// How far the apps-and-widgets strip is scrolled (DIPs), and where
    /// it's heading (the wheel moves this; the strip glides after it).
    pub scroll: f32,
    scroll_to: f32,
    /// Start, Search or the notification centre is open (the Windows key):
    /// an auto-hidden bar comes out with it, as Windows' own does.
    shell_open: bool,
    /// The strip's window while it's drawn (things half out of it fade).
    pub strip_clip: Option<Rect>,
    /// Something it shows changed: draw a frame (else, at rest, it draws
    /// nothing at all).
    pub dirty: bool,
    /// The clock's minute last drawn.
    drawn_minute: u16,
    /// How the apps fit (last frame).
    pub plan_fit: layout::Fit,
    plan: Option<items::Plan>,
    pub popup: Option<Pop>,
    pop_since: Instant,
    /// The hit under the mouse, since when, and when the mouse left the
    /// pop-up (it closes a moment later).
    hover_hit: Option<u32>,
    hover_since: Instant,
    pub pop_leave: Option<Instant>,
}

impl Bar {
    fn t(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    fn g(&mut self) -> &mut Gfx {
        self.gfx.as_mut().expect("gfx")
    }

    pub fn autohide(&self) -> bool {
        nookly_core::barcfg::flag(&self.settings, "bar_autohide", false)
    }
    pub fn align_left(&self) -> bool {
        self.settings.extra.get("bar_align").is_some_and(|v| v == "left")
    }
    /// Pop-ups open on hover (else only on click).
    pub fn hover_opens(&self) -> bool {
        nookly_core::barcfg::flag(&self.settings, "bar_hover", true)
    }

    fn theme(&self) -> Theme {
        pick_theme(&self.settings, &nookly_core::barcfg::theme_key(&self.settings), self.glass_light)
    }

    /// Should the bar be out right now?
    fn wanted(&self) -> bool {
        if self.fullscreen {
            return false;
        }
        if !self.autohide() {
            return true;
        }
        let asking = !self.apps.attention.is_empty() && self.attention_at.is_some_and(|t| t.elapsed().as_secs_f32() < ATTENTION_BEATS);
        asking || self.keys.is_some() || self.tray_hold.is_some() || self.inside || self.popup.is_some() || self.left_at.is_some_and(|t| t.elapsed().as_secs_f32() < 0.7) || self.down.is_some() || self.shell_open
    }

    fn load_settings(&mut self) {
        self.dirty = true;
        self.settings = Settings::load();
        time::H12.store(self.settings.clock12, std::sync::atomic::Ordering::Relaxed);
        time::SECONDS.store(false, std::sync::atomic::Ordering::Relaxed);
        self.order = if self.is_main() { items::order(&self.settings) } else { items::other_order(&self.settings) };
        self.placed = nookly_core::barcfg::placed(&self.settings);
        if !self.is_main() {
            return;
        }
        self.apps.scan(&self.settings);
        if !self.hwnd.is_invalid() {
            unread::enable(self.hwnd, nookly_core::barcfg::anywhere(&self.settings, "notifs"));
        }
        // The hidden icons: Nookly's own tray, while the item is on.
        if nookly_core::barcfg::anywhere(&self.settings, "tray") {
            if !self.hwnd.is_invalid() {
                tray::start(self.hwnd);
            }
        } else {
            tray::stop();
        }
    }

    fn is_main(&self) -> bool {
        self.device.is_empty()
    }

    /// Its screen.
    fn monitor(&self) -> HMONITOR {
        screens().into_iter().find(|s| !self.is_main() && s.1 == self.device).map_or_else(|| unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) }, |s| s.0)
    }

    /// Where its screen's usable area ends.
    fn work_bottom(&self) -> i32 {
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        unsafe {
            let _ = GetMonitorInfoW(self.monitor(), &mut mi);
        }
        mi.rcWork.bottom
    }

    /// Size and place the window over its screen's bottom edge, and
    /// reserve the bar's strip (fixed mode) so maximized windows stop above it.
    fn place(&mut self) {
        self.dirty = true;
        self.drawn_away = None;
        unsafe {
            let hmon = self.monitor();
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(hmon, &mut mi);
            self.mon = mi.rcMonitor;
            let (mut dx, mut dy) = (96u32, 96u32);
            let _ = GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
            let scale = dx as f32 / 96.0 * size_factor(&self.settings);
            if (scale - self.scale).abs() > 0.001 {
                self.icons = Icons::default();
                self.apps.forget_icons();
                if self.is_main() {
                    want_faces(self.glass_light, scale);
                }
            }
            self.scale = scale;
        }
        self.size_window();
        self.sync_appbar();
        self.next_grab = Instant::now();
    }

    /// The window: the screen's width and the bar's height. (Pop-ups have
    /// a window of their own, so this one never changes size for them.)
    fn size_window(&mut self) {
        let pw = self.mon.right - self.mon.left;
        let ph = (BAR_H * self.scale).ceil() as i32;
        self.w = pw as f32 / self.scale;
        self.h = ph as f32 / self.scale;
        if let Some(g) = self.gfx.as_mut() {
            if g.pw != pw || g.ph != ph {
                let _ = g.resize(pw, ph);
            }
        }
        unsafe {
            let z = if self.fullscreen { HWND_NOTOPMOST } else { HWND_TOPMOST };
            let _ = SetWindowPos(self.hwnd, Some(z), self.mon.left, self.mon.bottom - ph, pw, ph, SWP_NOACTIVATE);
        }
    }

    pub fn toggle_pop(&mut self, p: Pop) {
        let same = Pop::same(self.popup, p);
        self.set_pop(if same { None } else { Some(p) });
    }

    /// The app being dragged (once it has moved far enough).
    pub fn dragging(&self) -> Option<usize> {
        let d = self.drag.as_ref().filter(|d| d.active)?;
        self.apps.list.iter().position(|a| a.key == d.key)
    }

    /// Where a drag would drop: the middle of another app or a collection
    /// (to put them together), beside a tile (to move it there; a bar tile
    /// or one in the "more apps" grid), or on the "more apps" tile (into it).
    pub fn drop_at(&self) -> Option<Drop> {
        let from = self.dragging()?;
        let (mx, my) = self.mouse;
        // Over the open "more apps" grid: its cells.
        let in_pop = matches!(self.popup, Some(Pop::Overflow(_))) && self.pop_b.as_ref().is_some_and(|b| b.goal.is_some() && b.shape().contains(mx, my));
        if in_pop {
            let cell = |id: u32| (items::POP_APP..items::POP_APP + 1000).contains(&id);
            let h = self
                .hits
                .iter()
                .filter(|h| cell(h.id))
                .map(|h| (h, (mx - h.rect.cx()).powi(2) + (my - (h.rect.y + h.rect.h / 2.0)).powi(2)))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|x| x.0)?;
            let k = (h.id - items::POP_APP) as usize;
            if k == from {
                return None;
            }
            if (mx - h.rect.cx()).abs() < 9.0 && h.rect.contains(mx, my) {
                return Some(Drop::Onto(h.id));
            }
            let at = k + (mx > h.rect.cx()) as usize;
            return (at != from && at != from + 1).then_some(Drop::Between(at, false));
        }
        if my < -24.0 || my > self.h + 24.0 {
            return None;
        }
        let tile = |id: u32| (items::APP..items::APP + 900).contains(&id);
        if from < self.plan_fit.shown && self.hits.iter().any(|h| h.id == items::OVERFLOW && mx >= h.rect.x && mx < h.rect.r()) {
            return Some(Drop::Onto(items::OVERFLOW));
        }
        if let Some(h) = self.hits.iter().find(|h| (items::GROUP..items::GROUP + 100).contains(&h.id) && mx >= h.rect.x && mx < h.rect.r()) {
            return Some(Drop::Onto(h.id));
        }
        // (A little past a tile's ends counts as its side.)
        let h = self
            .hits
            .iter()
            .filter(|h| tile(h.id))
            .map(|h| (h, (mx - h.rect.cx()).abs()))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .filter(|(h, d)| *d < h.rect.w)
            .map(|x| x.0)?;
        let k = (h.id - items::APP) as usize;
        if k == from {
            return None;
        }
        if (mx - h.rect.cx()).abs() < 9.0 {
            return Some(Drop::Onto(h.id));
        }
        let at = k + (mx > h.rect.cx()) as usize;
        (at != from && at != from + 1).then_some(Drop::Between(at, true))
    }

    pub fn set_pop(&mut self, p: Option<Pop>) {
        self.dirty = true;
        if p != self.popup {
            dbg_log(&format!("pop {:?} -> {p:?}: inside {} in_pop {:?} mouse {:?} shown {:.2}", self.popup, self.inside, self.pop_b.as_ref().map(|b| b.inside), self.mouse, self.shown()));
        }
        if p != self.popup {
            self.pop_since = Instant::now();
        }
        if p.is_none() || !Pop::same(self.popup, p.unwrap_or(Pop::Overflow(0.0))) {
            self.close_sub();
        }
        let was = self.popup;
        self.popup = p;
        self.pop_leave = None;
        let is_menu = |p: Option<Pop>| matches!(p, Some(Pop::Menu(_)));
        if is_menu(was) != is_menu(p) {
            menu::listen(self.hwnd, is_menu(p));
            if !is_menu(p) {
                self.set_menu_keys(false);
            }
        }
        if p.is_none() {
            self.end_peek();
        }
        let widget = |p: Option<Pop>| match p {
            Some(Pop::Widget(k, _)) => Some(k),
            _ => None,
        };
        if widget(was) != widget(p) {
            items::tell_full(self, widget(p));
        }
        match p {
            Some(p) => self.aim_pop(p),
            None => {
                if let Some(b) = self.pop_b.as_mut() {
                    b.close();
                    b.clear_thumbs();
                }
            }
        }
    }

    fn set_menu_keys(&mut self, on: bool) {
        if self.menu_keys != on {
            self.menu_keys = on;
            menu::take_keys(self.hwnd, on);
        }
    }

    /// The widest the bar can get (the screen, less its flares and a margin).
    pub fn max_bar_w(&self) -> f32 {
        self.w - 2.0 * (BAR_H / 4.0 + 8.0)
    }

    /// The bar's width with no pop-up open: what it holds (auto-hide), or
    /// all it can get (it keeps its strip of the screen anyway; what it
    /// holds stays in the middle).
    fn rest_w(&self) -> f32 {
        if self.autohide() { self.content_w() } else { self.max_bar_w() }
    }

    /// The bar's width: its own, and wide enough to hold an open
    /// bubble whole (both its flares land on the bar's flat top), so the
    /// bar and its bubbles always read as one shape.
    fn bar_target_w(&self) -> f32 {
        let full = self.rest_w();
        match self.pop_b.as_ref().and_then(|b| b.goal) {
            Some(g) => full.max(g.w + 2.0 * HOLD).min(self.max_bar_w()),
            None => full,
        }
    }

    /// The bar's left and right at width `w` (where it settles).
    fn bar_span(&self, w: f32) -> (f32, f32) {
        let x0 = if self.align_left() { BAR_H / 4.0 + 6.0 } else { (self.w - w) / 2.0 };
        (x0, x0 + w)
    }

    /// Point the pop-up bubble at `p`: over its tile, standing on the bar,
    /// never past the bar's ends (the bar stretches to hold it); wide
    /// enough to hold the windows bubble grown out of it.
    fn aim_pop(&mut self, p: Pop) {
        let (w, h) = items::pop_size(self, p);
        // Whole pixels tall: its top is a crisp edge for a bubble on it.
        let h = (h * self.scale).round() / self.scale;
        let sub_w = self.sub_b.as_ref().and_then(|b| b.goal).map_or(0.0, |g| g.w + 2.0 * HOLD);
        let w = w.max(sub_w).min(self.max_bar_w() - 2.0 * HOLD);
        let ax = match p {
            Pop::App(_, x) | Pop::Overflow(x) | Pop::Group(_, x) | Pop::Widget(_, x) | Pop::Tray(x) | Pop::Menu(x) => x,
        };
        let bar_w = self.rest_w().max(w + 2.0 * HOLD).min(self.max_bar_w());
        let (x0, x1) = self.bar_span(bar_w);
        let cx = ax.clamp(x0 + HOLD + w / 2.0, (x1 - HOLD - w / 2.0).max(x0 + HOLD + w / 2.0));
        let g = bubble::Goal { cx, w, h, base: self.content().y, from_w: items::TILE };
        if self.pop_b.is_none() {
            self.pop_b = bubble::Bubble::new();
            if let Some(b) = &self.pop_b {
                drop::register(b.hwnd);
            }
        }
        if let Some(b) = self.pop_b.as_mut() {
            if b.goal != Some(g) {
                b.open(g);
            }
        }
    }

    /// Grow the windows bubble for `app` out of the pop-up, over its cell.
    fn open_sub(&mut self, app: apps::App, cell_x: f32) {
        if self.pop_b.as_ref().is_none_or(|b| b.goal.is_none()) {
            return;
        }
        let (w, h) = items::windows_size(self, &app);
        let w = w.min(self.max_bar_w() - 4.0 * HOLD);
        if self.sub_b.is_none() {
            self.sub_b = bubble::Bubble::new();
            if let Some(b) = &self.sub_b {
                drop::register(b.hwnd);
            }
        }
        if let Some(b) = self.sub_b.as_mut() {
            b.open(bubble::Goal { cx: cell_x, w, h, base: 0.0, from_w: 60.0 });
        }
        self.sub_app = Some((app, cell_x));
        self.sub_leave = None;
        // The pop-up widens to hold it, then it's placed on the pop-up's top.
        if let Some(p) = self.popup {
            self.aim_pop(p);
        }
        self.aim_sub();
    }

    /// Keep the windows bubble on the pop-up's top edge, within its ends.
    fn aim_sub(&mut self) {
        let Some(main) = self.pop_b.as_ref().and_then(|b| b.goal) else { return };
        let Some(cell_x) = self.sub_app.as_ref().map(|s| s.1) else { return };
        if let Some(b) = self.sub_b.as_mut() {
            if let Some(mut g) = b.goal {
                let (x0, x1) = (main.cx - main.w / 2.0, main.cx + main.w / 2.0);
                g.cx = cell_x.clamp(x0 + HOLD + g.w / 2.0, (x1 - HOLD - g.w / 2.0).max(x0 + HOLD + g.w / 2.0));
                g.base = main.base - main.h;
                if b.goal != Some(g) {
                    b.open(g);
                }
            }
        }
    }

    fn close_sub(&mut self) {
        if let Some(b) = self.sub_b.as_mut() {
            b.close();
            b.clear_thumbs();
        }
        self.sub_app = None;
        self.sub_hover = None;
        self.sub_leave = None;
    }

    fn appbar_data(&self) -> APPBARDATA {
        APPBARDATA { cbSize: std::mem::size_of::<APPBARDATA>() as u32, hWnd: self.hwnd, uCallbackMessage: WM_APPBAR, uEdge: ABE_BOTTOM, ..Default::default() }
    }

    /// Fixed mode reserves the bar's strip of the screen; auto-hide doesn't.
    fn sync_appbar(&mut self) {
        let want = !self.autohide();
        unsafe {
            let mut d = self.appbar_data();
            if want && !self.appbar {
                let r = tray::step_aside(|| SHAppBarMessage(ABM_NEW, &mut d));
                dbg_log(&format!("appbar new: {r}"));
                self.appbar = true;
            } else if !want && self.appbar {
                tray::step_aside(|| SHAppBarMessage(ABM_REMOVE, &mut d));
                self.appbar = false;
                self.appbar_rc = RECT::default();
                return;
            }
            if self.appbar {
                let hpx = (BAR_H * self.scale).round() as i32;
                d.rc = RECT { left: self.mon.left, top: self.mon.bottom - hpx, right: self.mon.right, bottom: self.mon.bottom };
                let r = tray::step_aside(|| SHAppBarMessage(ABM_QUERYPOS, &mut d));
                dbg_log(&format!("appbar query: {r} {:?} (set {:?})", d.rc, self.appbar_rc));
                d.rc.top = d.rc.bottom - hpx;
                // Setting it again makes Windows tell every app bar (us
                // included) and re-lay windows out: only when it changed.
                if d.rc != self.appbar_rc {
                    self.appbar_rc = d.rc;
                    let r = tray::step_aside(|| SHAppBarMessage(ABM_SETPOS, &mut d));
                    dbg_log(&format!("appbar set: {r} {:?}, area {}", d.rc, self.work_bottom()));
                }
            }
        }
    }

    /// Windows can say yes to the strip and still not keep it free (it did
    /// when asked while it was busy): ask again, a few times at most.
    fn check_strip(&mut self) {
        if !self.appbar || self.autohide() || self.strip_at.elapsed().as_secs_f32() < 1.0 {
            return;
        }
        self.strip_at = Instant::now();
        if self.work_bottom() <= self.appbar_rc.top {
            self.strip_tries = 0;
        } else if self.strip_tries < 5 {
            self.strip_tries += 1;
            self.appbar_rc = RECT::default();
            self.sync_appbar();
        }
    }

    fn remove_appbar(&mut self) {
        if self.appbar {
            let mut d = self.appbar_data();
            unsafe {
                tray::step_aside(|| SHAppBarMessage(ABM_REMOVE, &mut d));
            }
            self.appbar = false;
            self.appbar_rc = RECT::default();
        }
    }

    /// Is a full-screen app (game, video, slideshow) in front on our screen?
    fn check_fullscreen(&mut self) {
        let was = self.fullscreen;
        self.fullscreen = unsafe {
            let fg = GetForegroundWindow();
            let mut r = RECT::default();
            let mut cls = [0u16; 64];
            let n = GetClassNameW(fg, &mut cls);
            let cls = String::from_utf16_lossy(&cls[..n as usize]);
            // A maximized window isn't full screen (it keeps its frame, and
            // right after the Windows taskbar hides it briefly covers the
            // whole screen).
            !fg.is_invalid()
                && fg != self.hwnd
                && !IsZoomed(fg).as_bool()
                && !matches!(cls.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd")
                && GetWindowRect(fg, &mut r).is_ok()
                && r.left <= self.mon.left
                && r.top <= self.mon.top
                && r.right >= self.mon.right
                && r.bottom >= self.mon.bottom
        };
        if was != self.fullscreen {
            self.dirty = true;
            if self.fullscreen && self.popup.is_some() {
                self.set_pop(None);
            }
            unsafe {
                // Let the full-screen app sit on top.
                let z = if self.fullscreen { HWND_NOTOPMOST } else { HWND_TOPMOST };
                let _ = SetWindowPos(self.hwnd, Some(z), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
        }
    }

    /// Lay out the bar for this frame.
    fn make_plan(&mut self) {
        let ear = BAR_H / 4.0;
        let max_w = self.w - 2.0 * (ear + 8.0);
        let mut g = self.gfx.take().expect("gfx");
        let p = items::plan(self, &mut g, max_w);
        self.gfx = Some(g);
        self.plan_fit = p.fit;
        self.plan = Some(p);
    }

    /// Settings > Appearance > Reduce motion (shared with the notch).
    fn reduce_motion(&self) -> bool {
        nookly_core::barcfg::flag(&self.settings, "reduce_motion", false)
    }

    /// How far out the bar is (0 hidden in the edge .. 1 out).
    pub fn shown(&self) -> f32 {
        (self.depth.pos / BAR_H).clamp(0.0, 1.0)
    }

    /// The width the content asks for.
    pub fn content_w(&self) -> f32 {
        let ear = BAR_H / 4.0;
        self.plan.as_ref().map_or(120.0, |p| p.width).min(self.w - 2.0 * (ear + 8.0))
    }

    /// Advance the springs and hover timers. Returns true while anything
    /// still moves.
    /// Where the strip is gliding to.
    pub fn scroll_target(&self) -> f32 {
        self.scroll_to
    }

    fn step(&mut self) -> bool {
        let moving = self.step_inner();
        moving || (self.scroll - self.scroll_to).abs() > 0.3
    }

    fn step_inner(&mut self) -> bool {
        let now = Instant::now();
        let dt = (now - self.last_step).as_secs_f32().min(0.05);
        self.last_step = now;
        self.make_plan();
        // The strip glides to where the wheel sent it.
        let room = self.plan.as_ref().and_then(|p| p.strip).map_or(0.0, |(_, v, c)| (c - v).max(0.0));
        self.scroll_to = self.scroll_to.clamp(0.0, room);
        if (self.scroll - self.scroll_to).abs() > 0.3 {
            self.scroll += (self.scroll_to - self.scroll) * (dt * 16.0).min(1.0);
            self.dirty = true;
        } else if self.scroll != self.scroll_to {
            self.scroll = self.scroll_to;
            self.dirty = true;
        }
        let out = self.wanted();
        // Which widgets can be seen (a widget scrolled out of sight needn't move).
        let mut seen: u64 = 0;
        if let Some(p) = self.plan.as_ref() {
            let view = p.strip.map(|(x0, v, _)| (x0, x0 + v));
            for (k, &(it, x, w)) in p.slots.iter().enumerate() {
                if let items::Item::Widget(pos) = it {
                    let inside = p.in_strip.get(k).copied().unwrap_or(false);
                    let shows = match view.filter(|_| inside) {
                        Some((a, b)) => x - self.scroll + w > a && x - self.scroll < b,
                        None => true,
                    };
                    if shows && pos < 64 {
                        seen |= 1 << pos;
                    }
                }
            }
        }
        self.out_seen = (out, if out { seen } else { 0 });
        let full = self.rest_w();
        // Hiding, it narrows as it sinks into the edge (like the notch
        // closing); coming out, it grows back with a bounce.
        // The pop-up follows what it shows (apps come and go), and the
        // windows bubble stays on the pop-up's top.
        if let Some(p) = self.popup {
            self.aim_pop(p);
            self.aim_sub();
        }
        let (want_w, want_d) = if out { (self.bar_target_w(), BAR_H) } else { (full * 0.55, 0.0) };
        let reduce = self.reduce_motion();
        let mut bubbles = false;
        for b in [self.pop_b.as_mut(), self.sub_b.as_mut()].into_iter().flatten() {
            if b.active() {
                bubbles |= b.step(dt, reduce);
            }
        }
        if reduce {
            self.width = Spring { pos: want_w, vel: 0.0 };
            self.depth = Spring { pos: want_d, vel: 0.0 };
            let before = self.popup;
            let peek = self.step_peek();
            return self.step_hover() || self.popup != before || bubbles || peek;
        }
        // First frame: start small, so the bar bubbles up out of the edge.
        if self.width.pos == 0.0 {
            self.width.pos = full * 0.4;
        }
        let n = ((dt * 240.0).ceil() as usize).clamp(1, 32);
        let h = dt / n as f32;
        for _ in 0..n {
            self.width.step(want_w, h, SPRING_W.0, SPRING_W.1);
            self.depth.step(want_d, h, SPRING_D.0, SPRING_D.1);
        }
        self.depth.pos = self.depth.pos.max(0.0);
        let mut moving = true;
        let dist = (self.width.pos - want_w).abs() + (self.depth.pos - want_d).abs();
        let speed = self.width.vel.abs() + self.depth.vel.abs();
        if dist < 0.3 && speed < 6.0 {
            self.width = Spring { pos: want_w, vel: 0.0 };
            self.depth = Spring { pos: want_d, vel: 0.0 };
            moving = false;
        }
        moving |= bubbles;
        // A pop-up opening (or closing) here needs frames from now on.
        let before = self.popup;
        moving |= self.step_hover();
        moving |= self.step_peek();
        if self.attention_moving() {
            self.dirty = true;
            moving = true;
        }
        moving || self.popup != before
    }

    /// Hover pop-ups: rest on an app (or the overflow tile) to open its
    /// pop-up; leave both the tile and the pop-up to close it. Returns
    /// true while a timer runs (so frames keep coming).
    fn step_hover(&mut self) -> bool {
        let in_pop = self.pop_b.as_ref().is_some_and(|b| b.inside);
        let in_sub = self.sub_b.as_ref().is_some_and(|b| b.inside);
        let id = if self.inside || in_pop || in_sub { self.hit_at(self.mouse.0, self.mouse.1) } else { None };
        if id != self.hover_hit {
            self.hover_hit = id;
            self.hover_since = Instant::now();
        }
        if self.hover_block.is_some() && id != self.hover_block {
            self.hover_block = None;
        }
        let mut waiting = false;
        self.step_tray_hold();
        if self.keys.is_some() && self.shown() > 0.9 {
            self.key_begin();
        }
        // (Typing in a widget's view: it stays, wherever the mouse is.)
        let held = self.tray_hold.is_some() || self.keys.is_some() || self.typing();
        let in_menu = matches!(self.popup, Some(Pop::Menu(_)));
        if in_menu {
            self.set_menu_keys(self.inside || in_pop);
        }
        if self.hover_opens() && !in_menu && !held && self.down.is_none() && self.drag.is_none() && self.hover_block.is_none() {
            let target = match id {
                Some(i) if (items::APP..items::APP + 900).contains(&i) => Some(Pop::App((i - items::APP) as usize, self.mouse.0)),
                Some(items::OVERFLOW) => Some(Pop::Overflow(self.mouse.0)),
                Some(items::TRAY) => Some(Pop::Tray(self.mouse.0)),
                Some(i) if (items::GROUP..items::GROUP + 100).contains(&i) => Some(Pop::Group((i - items::GROUP) as usize, self.mouse.0)),
                Some(i) if (items::WIDGET..items::WIDGET + 500).contains(&i) && items::widget_panel(self, (i - items::WIDGET) as usize) => Some(Pop::Widget((i - items::WIDGET) as usize, self.mouse.0)),
                _ => None,
            };
            if let Some(p) = target {
                if !Pop::same(self.popup, p) {
                    // Already showing one: switch at once; else wait a moment.
                    if self.popup.is_some() || self.hover_since.elapsed().as_secs_f32() >= HOVER_OPEN {
                        let x = self.tile_center(p).unwrap_or(self.mouse.0);
                        self.set_pop(Some(match p {
                            Pop::App(i, _) => Pop::App(i, x),
                            Pop::Overflow(_) => Pop::Overflow(x),
                            Pop::Group(g, _) => Pop::Group(g, x),
                            Pop::Widget(k, _) => Pop::Widget(k, x),
                            Pop::Tray(_) => Pop::Tray(x),
                            Pop::Menu(_) => Pop::Menu(x),
                        }));
                    } else {
                        waiting = true;
                    }
                }
            }
        }
        waiting |= self.step_sub(id, in_sub);
        // Close when the mouse is on neither the pop-up (nor the bubble
        // grown out of it) nor its tile.
        if let Some(p) = self.popup {
            let near = |b: &Option<bubble::Bubble>| b.as_ref().is_some_and(|b| b.active() && b.shape().inset(-6.0).contains(self.mouse.0, self.mouse.1));
            let on_pop = in_pop || in_sub || (self.inside && (near(&self.pop_b) || near(&self.sub_b)));
            let on_tile = match (p, id) {
                (Pop::App(i, _), Some(h)) => h == items::APP + i as u32,
                (Pop::Overflow(_), Some(h)) => h == items::OVERFLOW,
                (Pop::Group(g, _), Some(h)) => h == items::GROUP + g as u32,
                (Pop::Widget(k, _), Some(h)) => h == items::WIDGET + k as u32,
                (Pop::Tray(_), Some(h)) => h == items::TRAY,
                // A menu: anywhere on the bar.
                (Pop::Menu(_), _) => self.inside,
                _ => false,
            };
            if on_pop || on_tile || held || self.drag.is_some() {
                self.pop_leave = None;
            } else {
                let t = *self.pop_leave.get_or_insert_with(Instant::now);
                if in_menu {
                    // A menu waits for a click, not for the mouse (but not
                    // forever: it holds Esc, the arrows and Enter). The idle
                    // loop's look every 250 ms is enough for this.
                    if t.elapsed().as_secs_f32() > MENU_LEAVE {
                        self.set_pop(None);
                    }
                }
                // Click-opened pop-ups stay until a click elsewhere.
                else if self.hover_opens() && t.elapsed().as_secs_f32() > 0.35 {
                    self.set_pop(None);
                } else if self.hover_opens() {
                    waiting = true;
                }
            }
        }
        waiting
    }

    /// The windows bubble over a grid: rest on an app in the grid to grow
    /// its windows out of the pop-up; move to another app to switch;
    /// leave both for a moment to close it.
    fn step_sub(&mut self, id: Option<u32>, in_sub: bool) -> bool {
        if !matches!(self.popup, Some(Pop::Overflow(_)) | Some(Pop::Group(..))) || !self.hover_opens() || self.drag.is_some() {
            return false;
        }
        let cell = id.filter(|i| (items::POP_APP..items::POP_APP + 1000).contains(i) || (items::POP_MEMBER..items::POP_MEMBER + 100).contains(i));
        let on_sub = in_sub || id.is_some_and(|i| (items::SUB_ROW..items::SUB_OPEN + 1).contains(&i));
        if let Some(c) = cell {
            self.sub_leave = None;
            let app = items::grid_app(self, c);
            let same = app.as_ref().zip(self.sub_app.as_ref()).is_some_and(|(a, (s, _))| a.key == s.key);
            if same {
                self.sub_hover = None;
                return false;
            }
            let since = match self.sub_hover {
                Some((h, t)) if h == c => t,
                _ => {
                    self.sub_hover = Some((c, Instant::now()));
                    Instant::now()
                }
            };
            if since.elapsed().as_secs_f32() >= 0.3 {
                let x = self.hits.iter().find(|h| h.id == c).map_or(self.mouse.0, |h| h.rect.cx());
                if let Some(a) = app {
                    self.open_sub(a, x);
                }
                self.sub_hover = None;
                return false;
            }
            return true;
        }
        self.sub_hover = None;
        if on_sub || self.sub_app.is_none() {
            self.sub_leave = None;
            return false;
        }
        let t = *self.sub_leave.get_or_insert_with(Instant::now);
        if t.elapsed().as_secs_f32() > 0.35 {
            self.close_sub();
            return false;
        }
        true
    }

    /// Centre of the tile a pop-up belongs to (window DIPs).
    fn tile_center(&self, p: Pop) -> Option<f32> {
        let id = match p {
            Pop::App(i, _) => items::APP + i as u32,
            Pop::Overflow(_) => items::OVERFLOW,
            Pop::Group(g, _) => items::GROUP + g as u32,
            Pop::Widget(k, _) => items::WIDGET + k as u32,
            Pop::Tray(_) => items::TRAY,
            Pop::Menu(_) => return None,
        };
        self.hits.iter().find(|h| h.id == id).map(|h| h.rect.cx())
    }

    /// The body (without flares) in window DIPs.
    pub fn body(&self) -> Rect {
        // Liquid squash and stretch: growing fast one way, it gives a
        // little the other way (at most 6%), as the notch does.
        let k = |v: f32| if self.reduce_motion() { 0.0 } else { (v * 0.00004).clamp(-0.06, 0.06) };
        let w = (self.width.pos * (1.0 - k(self.depth.vel))).max(0.0);
        let d = self.depth.pos * (1.0 - k(self.width.vel));
        let ear = BAR_H / 4.0;
        let x = if self.align_left() { ear + 6.0 } else { (self.w - w) / 2.0 };
        Rect::new(x, self.h - d, w, d)
    }

    /// Where the content sits: the full-size bar, centred on the body (so
    /// it stays still while the shape wobbles around it).
    pub fn content(&self) -> Rect {
        let body = self.body();
        let w = self.content_w();
        let x = if self.align_left() { body.x } else { body.cx() - w / 2.0 };
        Rect::new(x, self.h - BAR_H, w, BAR_H)
    }

    /// Liquid Glass: grab what's behind the bar's strip when it may have
    /// changed (a cheap probe of the screen just above it, every half
    /// second, and a fresh grab every few seconds anyway).
    fn update_glass(&mut self, theme: &Theme) {
        if !theme.glass || self.shown() < 0.5 {
            if !theme.glass {
                self.backdrop = None;
            }
            return;
        }
        let now = Instant::now();
        let hpx = (BAR_H * self.scale).round() as i32;
        let strip = RECT { left: self.mon.left, top: self.mon.bottom - hpx, right: self.mon.right, bottom: self.mon.bottom };
        let generation = self.g().generation;
        let mut grab = self.backdrop.is_none() || self.backdrop_gen != generation || now >= self.next_grab;
        if now >= self.next_probe {
            self.next_probe = now + std::time::Duration::from_millis(500);
            let above = RECT { top: strip.top - 6, bottom: strip.top - 2, ..strip };
            let p = glass::probe(&[above]);
            if p != self.probe {
                self.probe = p;
                grab = true;
            }
        }
        if !grab {
            return;
        }
        self.next_grab = now + std::time::Duration::from_secs(8);
        if let Some((w, h, mut px, lum)) = glass::capture_behind(self.hwnd, strip) {
            let was = self.glass_light;
            if lum > 0.62 {
                self.glass_light = true;
            } else if lum < 0.5 {
                self.glass_light = false;
            }
            if was != self.glass_light && self.is_main() {
                want_faces(self.glass_light, self.scale);
            }
            glass::tame(&mut px, self.glass_light);
            let key = px.iter().fold(0xcbf2_9ce4_8422_2325u64, |k, b| (k ^ *b as u64).wrapping_mul(0x100_0000_01b3));
            if self.backdrop.is_none() || self.backdrop_gen != generation || key != self.backdrop_key || was != self.glass_light {
                self.backdrop = self.g().make_bitmap(w, h, &px);
                self.backdrop_gen = generation;
                self.backdrop_key = key;
                self.dirty = true;
            }
        }
    }

    /// Liquid Glass: look behind the bar now and then (its own timer); a
    /// new picture means a new frame.
    fn tick_glass(&mut self) {
        let theme = self.theme();
        self.update_glass(&theme);
    }

    fn render(&mut self) {
        self.dirty = false;
        self.drawn_away = (self.depth.pos < 0.5).then_some(self.fullscreen);
        self.drawn_minute = time::Now::get().min;
        let theme = self.theme();
        let mut g = self.gfx.take().expect("gfx");
        g.theme = theme;
        g.begin(self.scale);
        let mut hits = Vec::new();
        let body = self.body();
        let ear = BAR_H / 4.0;
        let t = self.t();
        let mouse = self.mouse;
        if self.depth.pos < 0.5 {
            // Hidden: a near-invisible sliver along the edge still catches
            // the mouse (fully transparent pixels let clicks through).
            if !self.fullscreen {
                g.fill_rect(Rect::new(body.x, self.h - 2.0, body.w, 2.0), hex(0x000000).with_a(0.012));
            }
        } else if let Some(geo) = g.notch_geometry(body, RAD.min(body.h / 2.0), ear.min(body.h / 4.0), nookly_core::store::Edge::Bottom) {
            if theme.glass {
                if let Some(bmp) = self.backdrop.clone() {
                    g.push_layer(Some(&geo), 1.0);
                    g.image_rr(&bmp, Rect::new(0.0, self.h - BAR_H, self.w, BAR_H), 0.0);
                    g.pop_layer();
                }
            }
            g.fill_geometry(&geo, col::NOTCH);
            if theme.glass {
                self.draw_glass_rim(&mut g, &geo, body, ear.min(body.h / 4.0));
            }
            // Content fades in as the bar comes out of the edge.
            let content = self.content();
            g.push_layer(Some(&geo), self.shown().powi(2));
            let plan = self.plan.take();
            {
                let mut ui = Ui { g: &mut g, hits: &mut hits, module: 0, mouse, t };
                if let Some(p) = &plan {
                    items::draw(self, &mut ui, content, p);
                }
            }
            self.plan = plan;
            g.pop_layer();
            // The screen's far corner (a fixed bar; an auto-hidden one has
            // windows there): a click shows the desktop.
            if !self.autohide() {
                let r = Rect::new(self.w - 8.0, 0.0, 8.0, self.h);
                let hot = r.contains(mouse.0, mouse.1);
                g.fill_rect(r, hex(0x000000).with_a(0.012));
                if hot {
                    g.fill_rr_raw(Rect::new(self.w - 4.0, self.h / 2.0 - 10.0, 2.0, 20.0), 1.0, hex(0xFFFFFF).with_a(0.7));
                }
                hits.push(Hit { rect: r, module: 0, id: items::DESKTOP, tip: "" });
            }
        }
        self.hits = hits;
        let (x, y) = (self.mon.left, self.mon.bottom - g.ph);
        g.end_and_present(self.hwnd, x, y);
        self.gfx = Some(g);
        self.render_bubble(false, theme, t);
        self.render_bubble(true, theme, t);
    }

    /// Draw a bubble (the pop-up, or the windows bubble grown out of it):
    /// its shape, then its content fading in as it grows. Its hits join the
    /// bar's, in the bar's coordinates.
    fn render_bubble(&mut self, sub: bool, theme: Theme, t: f64) {
        let taken = if sub { self.sub_b.take() } else { self.pop_b.take() };
        let Some(mut b) = taken else { return };
        if !b.active() || self.depth.pos < 0.5 {
            if !b.active() {
                b.hide();
            }
            if sub {
                self.sub_b = Some(b);
            } else {
                self.pop_b = Some(b);
            }
            return;
        }
        let bar_origin = (self.mon.left, self.mon.bottom - (BAR_H * self.scale).ceil() as i32);
        let (sx, sy) = b.place(self.scale, bar_origin);
        // Liquid Glass: what's behind it (taken once it's shown in place).
        if theme.glass && b.visible && b.backdrop.as_ref().is_none_or(|x| x.2 != b.gfx.generation) {
            // All of where it's going as well as where it is: it's still
            // moving, and what it comes to rest on must have its picture too.
            let c = b.reach();
            let k = self.scale;
            let r = RECT {
                left: (bar_origin.0 + (c.x * k).floor() as i32).max(self.mon.left),
                top: (bar_origin.1 + (c.y * k).floor() as i32).max(self.mon.top),
                right: (bar_origin.0 + (c.r() * k).ceil() as i32).min(self.mon.right),
                bottom: (bar_origin.1 + (c.b() * k).ceil() as i32).min(self.mon.bottom),
            };
            let c = Rect::new((r.left - bar_origin.0) as f32 / k, (r.top - bar_origin.1) as f32 / k, (r.right - r.left) as f32 / k, (r.bottom - r.top) as f32 / k);
            if let Some((w, h, mut px, _)) = glass::capture_behind(b.hwnd, r) {
                glass::tame(&mut px, self.glass_light);
                if let Some(bmp) = b.gfx.make_bitmap(w, h, &px) {
                    b.backdrop = Some((bmp, c, b.gfx.generation));
                }
            }
        }
        b.gfx.theme = theme;
        b.gfx.begin(self.scale);
        let mut hits = Vec::new();
        let alpha = b.content_alpha();
        // The windows bubble stands on the pop-up: the pop-up's rim follows
        // the two together.
        let child = if sub { None } else { self.sub_b.as_ref().filter(|s| s.visible && s.active()).map(|s| s.silhouette()) };
        if let Some(geo) = b.draw_shape(&theme, self.glass_light, child) {
            let content = b.local(b.content());
            let mouse = (self.mouse.0 - b.origin.0, self.mouse.1 - b.origin.1);
            let shape = b.local(b.shape());
            b.gfx.push_layer(Some(&geo), alpha);
            {
                let mut ui = Ui { g: &mut b.gfx, hits: &mut hits, module: 0, mouse, t };
                // The shape catches clicks (they don't fall through).
                ui.hit(shape, 0);
                self.thumb_slots.clear();
                if sub {
                    if let Some((app, _)) = self.sub_app.clone() {
                        items::draw_windows(self, &mut ui, &app, content, items::SUB_IDS);
                    }
                } else if let Some(p) = self.popup {
                    items::draw_pop(self, &mut ui, p, content);
                }
            }
            b.gfx.pop_layer();
        }
        if alpha > 0.3 {
            for mut h in hits {
                h.rect = Rect::new(h.rect.x + b.origin.0, h.rect.y + b.origin.1, h.rect.w, h.rect.h);
                self.hits.push(h);
            }
        }
        b.gfx.end_and_present(b.hwnd, sx, sy);
        // Live previews only once it's grown (DWM draws them unclipped).
        if alpha > 0.9 {
            self.sync_thumbs(&mut b, alpha);
        } else {
            self.thumb_slots.clear();
            b.clear_thumbs();
        }
        if !b.visible {
            unsafe {
                let _ = ShowWindow(b.hwnd, SW_SHOWNOACTIVATE);
                let _ = SetWindowPos(b.hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
            b.visible = true;
        }
        if sub {
            self.sub_b = Some(b);
        } else {
            self.pop_b = Some(b);
        }
    }

    /// Live previews: DWM draws each window's picture into its slot in the
    /// bubble (kept to the window's shape, fading in with it).
    fn sync_thumbs(&mut self, win: &mut bubble::Bubble, alpha: f32) {
        use windows::Win32::Graphics::Dwm::*;
        let slots = std::mem::take(&mut self.thumb_slots);
        win.thumbs.retain(|(h, t)| {
            let keep = slots.iter().any(|(s, _)| s == h);
            if !keep {
                unsafe {
                    let _ = DwmUnregisterThumbnail(*t);
                }
            }
            keep
        });
        for (h, r) in &slots {
            crumb(format!("preview of window {:?}", h.0));
            let t = match win.thumbs.iter().find(|(s, _)| s == h) {
                Some((_, t)) => *t,
                None => match unsafe { DwmRegisterThumbnail(win.hwnd, *h) } {
                    Ok(t) => {
                        win.thumbs.push((*h, t));
                        t
                    }
                    Err(_) => continue,
                },
            };
            let src = unsafe { DwmQueryThumbnailSourceSize(t) }.unwrap_or(SIZE { cx: 16, cy: 9 });
            let (sw, sh) = (src.cx.max(1) as f32, src.cy.max(1) as f32);
            let k = (r.w / sw).min(r.h / sh);
            let (w, hh) = (sw * k, sh * k);
            let (x, y) = (r.x + (r.w - w) / 2.0, r.y + (r.h - hh) / 2.0);
            let sc = self.scale;
            let props = DWM_THUMBNAIL_PROPERTIES {
                dwFlags: DWM_TNP_RECTDESTINATION | DWM_TNP_VISIBLE | DWM_TNP_OPACITY | DWM_TNP_SOURCECLIENTAREAONLY,
                rcDestination: RECT { left: (x * sc).round() as i32, top: (y * sc).round() as i32, right: ((x + w) * sc).round() as i32, bottom: ((y + hh) * sc).round() as i32 },
                opacity: (alpha * 255.0) as u8,
                fVisible: TRUE,
                fSourceClientAreaOnly: FALSE,
                ..Default::default()
            };
            unsafe {
                let _ = DwmUpdateThumbnailProperties(t, &props);
            }
        }
    }

    /// Mouse coordinates of a message, in the bar's DIPs (bubbles sit above
    /// the bar, at negative y).
    fn to_bar(&self, hwnd: HWND, lp: LPARAM) -> (f32, f32) {
        let (x, y) = dips(lp, self.scale);
        for b in [self.pop_b.as_ref(), self.sub_b.as_ref()].into_iter().flatten() {
            if b.hwnd == hwnd {
                return (x + b.origin.0, y + b.origin.1);
            }
        }
        (x, y)
    }

    /// A point of the bar (DIPs) on the screen (pixels).
    pub fn to_screen(&self, x: f32, y: f32) -> (i32, i32) {
        let ph = self.gfx.as_ref().map_or((BAR_H * self.scale).ceil() as i32, |g| g.ph);
        (self.mon.left + (x * self.scale) as i32, self.mon.bottom - ph + (y * self.scale) as i32)
    }

    /// Where the open pop-up's content is (bar coordinates).
    pub fn pop_content(&self) -> Option<Rect> {
        self.pop_b.as_ref().filter(|b| b.active()).map(|b| b.content())
    }

    fn is_pop(&self, hwnd: HWND) -> bool {
        [self.pop_b.as_ref(), self.sub_b.as_ref()].into_iter().flatten().any(|b| b.hwnd == hwnd)
    }

    /// Send the mouse to the notch when it's in an open widget's full view.
    fn forward(&self, what: usize, x: f32, y: f32, wheel: i16) -> bool {
        let Some(r) = items::full_rect(self) else { return false };
        if what != nookly_core::surfaces::INPUT_LEAVE && !r.contains(x, y) {
            return false;
        }
        let (lx, ly) = (((x - r.x) * 4.0) as i16 as u16 as isize, ((y - r.y) * 4.0) as i16 as u16 as isize);
        unsafe {
            if let Ok(notch) = FindWindowW(w!("NooklyNotch"), PCWSTR::null()) {
                let msg = RegisterWindowMessageW(nookly_core::surfaces::INPUT_MSG);
                // A click may be into a text field: the notch takes the keys.
                if what == nookly_core::surfaces::INPUT_UP {
                    let mut pid = 0u32;
                    GetWindowThreadProcessId(notch, Some(&mut pid));
                    let _ = AllowSetForegroundWindow(pid);
                }
                let _ = PostMessageW(Some(notch), msg, WPARAM(what | ((wheel as u16 as usize) << 16)), LPARAM(lx | (ly << 16)));
            }
        }
        true
    }

    fn on_pop_move(&mut self, hwnd: HWND, x: f32, y: f32) {
        self.dirty = true;
        self.mouse = (x, y);
        if !self.forward(nookly_core::surfaces::INPUT_MOVE, x, y, 0) {
            self.forward(nookly_core::surfaces::INPUT_LEAVE, x, y, 0);
        }
        for b in [self.pop_b.as_mut(), self.sub_b.as_mut()].into_iter().flatten() {
            if b.hwnd == hwnd && !b.inside {
                b.inside = true;
                let mut tme = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: b.hwnd, dwHoverTime: 0 };
                unsafe {
                    let _ = TrackMouseEvent(&mut tme);
                }
            }
        }
    }

    fn on_pop_leave(&mut self, hwnd: HWND) {
        self.dirty = true;
        self.forward(nookly_core::surfaces::INPUT_LEAVE, 0.0, 0.0, 0);
        let mut any = false;
        for b in [self.pop_b.as_mut(), self.sub_b.as_mut()].into_iter().flatten() {
            if b.hwnd == hwnd {
                b.inside = false;
            }
            any |= b.inside;
        }
        if !self.inside && !any {
            self.mouse = (-1e4, -1e4);
        }
    }

    /// Liquid Glass's rim of light along the curved side (as on the notch).
    /// With a bubble standing on the bar, the rim follows the outline of
    /// both together (the bar draws its part, below its top edge; the
    /// bubble the rest), so the light runs from one into the other.
    fn draw_glass_rim(&self, g: &mut Gfx, geo: &ID2D1PathGeometry, body: Rect, ear: f32) {
        let light = self.glass_light;
        let bump = self.pop_b.as_ref().filter(|b| b.visible && b.active()).map(|b| b.silhouette());
        let rad = RAD.min(body.h / 2.0);
        let (shape, outline) = match bump {
            Some(_) => (g.bottom_outline(body, rad, ear, bump, true), g.bottom_outline(body, rad, ear, bump, false)),
            None => (None, g.notch_outline(body, rad, ear, nookly_core::store::Edge::Bottom)),
        };
        g.push_clip(Rect::new(-1e4, body.y, 2e4, 1e4));
        g.push_layer(Some(shape.as_ref().unwrap_or(geo)), 1.0);
        if let Some(outline) = &outline {
            g.glass_rim(outline, light, body.h);
        }
        g.pop_layer();
        // The light from above, fading away under a bubble (no hard edge).
        g.push_layer(Some(geo), 1.0);
        let wide = Rect::new(body.x - 20.0, body.y - 20.0, body.w + 40.0, body.h + 40.0);
        let soft = bubble::EAR;
        let bands: Vec<(Rect, f32, f32)> = match bump {
            Some((s, _, e)) => vec![
                (Rect::new(wide.x, wide.y, (s.x - e + soft - wide.x).max(1.0), wide.h), 0.0, 2.0 * soft),
                (Rect::new(s.r() + e - soft, wide.y, (wide.r() - s.r() - e + soft).max(1.0), wide.h), 2.0 * soft, 0.0),
            ],
            None => vec![(wide, 0.0, 0.0)],
        };
        for (r, l, rt) in bands {
            g.push_fade(r, l, rt);
            g.glass_sheen(Rect::new(body.x - ear, body.y, body.w + ear * 2.0, body.h * 0.4), if light { 0.14 } else { 0.07 });
            g.pop_layer();
        }
        g.pop_layer();
        g.pop_clip();
    }

    fn hit_at(&self, x: f32, y: f32) -> Option<u32> {
        self.hits.iter().rev().find(|h| h.rect.contains(x, y)).map(|h| h.id)
    }

    fn on_move(&mut self, x: f32, y: f32) {
        self.dirty = true;
        self.mouse = (x, y);
        if !self.inside {
            self.inside = true;
            self.left_at = None;
            let mut tme = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: self.hwnd, dwHoverTime: 0 };
            unsafe {
                let _ = TrackMouseEvent(&mut tme);
            }
        }
    }

    /// Where the cursor really is: resizing the window (a pop-up opening
    /// or closing) makes Windows say the mouse left even when it didn't.
    fn sync_mouse(&mut self) {
        unsafe {
            let mut p = POINT::default();
            if GetCursorPos(&mut p).is_err() {
                return;
            }
            if WindowFromPoint(p) == self.hwnd {
                let ph = self.gfx.as_ref().map_or(0, |g| g.ph);
                let (x, y) = ((p.x - self.mon.left) as f32 / self.scale, (p.y - (self.mon.bottom - ph)) as f32 / self.scale);
                self.inside = false;
                self.on_move(x, y);
            } else if self.inside {
                self.inside = false;
                self.mouse = (-1e4, -1e4);
                self.left_at = Some(Instant::now());
            }
        }
    }

    fn on_leave(&mut self) {
        self.dirty = true;
        // Still over us (the window just changed size)?
        unsafe {
            let mut p = POINT::default();
            if GetCursorPos(&mut p).is_ok() && WindowFromPoint(p) == self.hwnd {
                self.sync_mouse();
                return;
            }
        }
        self.inside = false;
        if ![self.pop_b.as_ref(), self.sub_b.as_ref()].into_iter().flatten().any(|b| b.inside) {
            self.mouse = (-1e4, -1e4);
        }
        self.left_at = Some(Instant::now());
    }

    fn on_down(&mut self, x: f32, y: f32) {
        self.dirty = true;
        self.down = self.hit_at(x, y);
        self.press_closes_menu(self.down);
        dbg_log(&format!("down at {x:.0},{y:.0}: {:?}", self.down));
        self.start_drag(x, y);
    }

    /// An app pressed, on the bar or in the "more apps" grid, can be
    /// dragged. (The bar gets the mouse until it's let go.)
    fn start_drag(&mut self, x: f32, y: f32) {
        let k = match self.down {
            Some(id) if (items::APP..items::APP + 900).contains(&id) => id - items::APP,
            Some(id) if (items::POP_APP..items::POP_APP + 1000).contains(&id) => id - items::POP_APP,
            _ => return,
        };
        if let Some(a) = self.apps.list.get(k as usize) {
            self.drag = Some(Drag { key: a.key.clone(), start: (x, y), active: false });
            unsafe {
                SetCapture(self.hwnd);
            }
        }
    }

    fn drag_move(&mut self) {
        if let Some(d) = self.drag.as_mut() {
            let (dx, dy) = (self.mouse.0 - d.start.0, self.mouse.1 - d.start.1);
            if !d.active && dx * dx + dy * dy > 36.0 {
                d.active = true;
                // From the "more apps" grid: it stays (to move within it).
                if matches!(self.popup, Some(Pop::Overflow(_))) {
                    self.close_sub();
                } else {
                    self.set_pop(None);
                }
                self.end_peek();
            }
        }
    }

    fn on_up(&mut self, x: f32, y: f32) {
        self.dirty = true;
        self.forward(nookly_core::surfaces::INPUT_UP, x, y, 0);
        if let Some(d) = self.drag.take() {
            unsafe {
                let _ = ReleaseCapture();
            }
            if d.active {
                self.mouse = (x, y);
                self.drag = Some(d);
                let target = self.drop_at();
                let from = self.dragging();
                self.drag = None;
                self.down = None;
                let Some(from) = from else { return };
                let shown = self.plan_fit.shown;
                let to = match target {
                    Some(Drop::Between(k, on_bar)) => Some(layout::landing(from, k, on_bar, shown)),
                    // Onto the "more apps" tile: the first in it.
                    Some(Drop::Onto(items::OVERFLOW)) => Some(layout::landing(from, shown, false, shown)),
                    _ => None,
                };
                if let Some(to) = to {
                    self.apps.move_to(from, to);
                    return;
                }
                if let (Some(app), Some(Drop::Onto(t))) = (self.apps.list.get(from).cloned(), target) {
                    let other = if t >= items::POP_APP && t < items::POP_APP + 1000 { t - items::POP_APP } else { t.wrapping_sub(items::APP) } as usize;
                    if (items::GROUP..items::GROUP + 100).contains(&t) {
                        apps::group_add(Some((t - items::GROUP) as usize), &app, None);
                    } else if let Some(other) = self.apps.list.get(other).filter(|o| !o.launch.is_empty() && !app.launch.is_empty()).cloned() {
                        apps::group_add(None, &app, Some(&other));
                    }
                    let st = self.settings.clone();
                    self.apps.scan(&st);
                    self.dirty = true;
                }
                return;
            }
        }
        let down = self.down.take();
        let hit = self.hit_at(x, y);
        dbg_log(&format!("up at {x:.0},{y:.0}: down {down:?} hit {hit:?} pop {:?}", self.popup));
        match hit.filter(|&id| Some(id) == down) {
            // A click on the pop-up's background, or nothing: keep it / close it.
            Some(0) => {}
            Some(id) => {
                // No hover pop-up for this tile until the mouse leaves it.
                if id == items::OVERFLOW || (items::APP..items::APP + 900).contains(&id) || (items::GROUP..items::GROUP + 100).contains(&id) {
                    self.hover_block = Some(id);
                }
                items::action(self, id, x);
            }
            None if hit.is_none() => self.set_pop(None),
            None => {}
        }
    }

    fn on_middle(&mut self, x: f32, y: f32) {
        if let Some(id) = self.hit_at(x, y) {
            items::middle(self, id);
        }
    }

    /// Right-click: the menu of what's under the mouse.
    fn on_right(&mut self, x: f32, y: f32) {
        let id = self.hit_at(x, y);
        if let Some(i) = id.filter(|i| (items::TRAY_ICON..items::TRAY_ICON + 200).contains(i)) {
            items::tray_click(self, i, true);
            return;
        }
        self.dirty = true;
        menu::open(self, id, x);
    }

    /// The app whose menu is open.
    pub fn menu_app(&self) -> Option<String> {
        match self.popup {
            Some(Pop::Menu(_)) => self.menu.as_ref()?.app.as_ref().map(|a| a.key.clone()),
            _ => None,
        }
    }

    /// A press: a menu closes unless it's on the menu.
    fn press_closes_menu(&mut self, hit: Option<u32>) {
        if matches!(self.popup, Some(Pop::Menu(_))) && !hit.is_some_and(|i| i == 0 || (menu::ID..menu::ID + 100).contains(&i)) {
            self.set_pop(None);
        }
    }

    /// A press anywhere on the screen while a menu is open: one outside our
    /// windows closes it (ours see to it themselves).
    fn pressed_anywhere(&mut self) {
        if !matches!(self.popup, Some(Pop::Menu(_))) {
            return;
        }
        unsafe {
            let mut p = POINT::default();
            if GetCursorPos(&mut p).is_ok() {
                let under = WindowFromPoint(p);
                if under != self.hwnd && !self.is_pop(under) {
                    self.set_pop(None);
                }
            }
        }
    }

    /// Rest on a window in a pop-up: show it on the screen (see peek.rs).
    /// Already showing one, the next shows at once; off them, it ends.
    /// Returns true while it waits.
    fn step_peek(&mut self) -> bool {
        let open = matches!(self.popup, Some(Pop::App(..))) || self.sub_app.is_some();
        let want = self.hover_hit.filter(|_| open).and_then(|id| {
            let row = (items::POP_ROW..items::POP_OPEN).contains(&id) || (items::SUB_ROW..items::SUB_OPEN).contains(&id);
            if row { items::pop_window(self, id) } else { None }
        });
        let want = want.filter(|&h| unsafe { IsWindow(Some(h)).as_bool() && !IsIconic(h).as_bool() });
        if want != self.peek_hover.map(|x| x.0) {
            self.peek_hover = want.map(|h| (h, Instant::now()));
            self.peek_left = Instant::now();
        }
        match (self.peek_hover, self.peeked) {
            (Some((h, _)), Some(p)) if h == p => false,
            (Some((h, since)), on) => {
                if on.is_none() && since.elapsed().as_secs_f32() < PEEK_WAIT {
                    return true;
                }
                crumb(format!("peek at window {:?}", h.0));
                peek::start(h, self.hwnd);
                self.peeked = Some(h);
                false
            }
            (None, Some(_)) => {
                // Between two previews: a moment's grace.
                if open && self.peek_left.elapsed().as_secs_f32() < 0.12 {
                    return true;
                }
                self.end_peek();
                false
            }
            _ => false,
        }
    }

    fn end_peek(&mut self) {
        self.peeked = None;
        self.peek_hover = None;
        peek::stop();
    }

    /// How far into a beat the attention glow is (0..1): three beats when an
    /// app starts asking, then steady.
    pub fn attention_beat(&self) -> f32 {
        let t = self.attention_at.map_or(99.0, |x| x.elapsed().as_secs_f32());
        if t > ATTENTION_BEATS || self.reduce_motion() { 0.4 } else { (t * std::f32::consts::PI * 2.0 / 0.8).sin() * 0.5 + 0.5 }
    }

    fn attention_moving(&self) -> bool {
        !self.apps.attention.is_empty() && !self.reduce_motion() && self.attention_at.is_some_and(|x| x.elapsed().as_secs_f32() <= ATTENTION_BEATS)
    }

    /// A window flashed (the shell told us).
    fn on_flash(&mut self, h: HWND) -> bool {
        // (It may be new: a permission prompt flashes as it comes.)
        if !self.apps.list.iter().any(|a| a.windows.contains(&h)) {
            let st = self.settings.clone();
            self.apps.scan(&st);
        }
        let new = self.apps.flashed(h);
        if new {
            self.attention_at = Some(Instant::now());
            self.dirty = true;
        }
        new
    }

    /// The app list was read again (`before`: its signature until then).
    fn apps_read(&mut self, before: u64) {
        if self.apps.signature() != before || self.popup.is_some() {
            self.dirty = true;
        }
        let gone = match self.popup {
            Some(Pop::App(i, _)) => i >= self.apps.list.len(),
            Some(Pop::Group(g, _)) => g >= self.apps.groups.len(),
            _ => false,
        };
        if gone {
            self.set_pop(None);
        }
    }

    /// A hidden icon is about to be clicked (its app may show a menu).
    pub fn hold_for_tray(&mut self, pid: u32) {
        let now = Instant::now();
        self.tray_hold = Some(TrayHold { pid, before: menus_of(pid), since: now, looked: now, open: false });
    }

    /// Is the clicked icon's menu still open (or about to be)?
    fn step_tray_hold(&mut self) {
        let Some(h) = self.tray_hold.as_mut() else { return };
        if h.looked.elapsed().as_secs_f32() < 0.1 {
            return;
        }
        h.looked = Instant::now();
        let open = menu_showing() || menus_of(h.pid).iter().any(|w| !h.before.contains(w));
        h.open |= open;
        if !open && (h.open || h.since.elapsed().as_secs_f32() > TRAY_WAIT) {
            self.tray_hold = None;
            self.dirty = true;
            // The mouse may be back on the bar, or gone.
            self.left_at = Some(Instant::now());
        }
    }

    /// What the keyboard is on.
    pub fn key_on(&self) -> Option<u32> {
        self.keys.and_then(|k| k.0)
    }

    /// Use the bar from the keyboard (its shortcut): it takes the focus, so
    /// keys come to it and a screen reader reads what they're on.
    fn keys_start(&mut self) {
        if self.keys.is_some() {
            self.keys_end(true);
            return;
        }
        self.dirty = true;
        unsafe {
            let back = GetForegroundWindow();
            let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, ex & !(WS_EX_NOACTIVATE.0 as isize));
            let _ = SetForegroundWindow(self.hwnd);
            let _ = SetFocus(Some(self.hwnd));
            self.keys = Some((None, back));
        }
        self.key_home = None;
        self.key_begin();
    }

    /// On the first app (or the first thing there is), once there's anything
    /// (an auto-hidden bar is still coming out).
    fn key_begin(&mut self) {
        if self.popup.is_some() || !matches!(self.keys, Some((None, _))) {
            return;
        }
        let rects = self.key_rects();
        if let Some(i) = rects.iter().map(|x| x.0).find(|i| (items::APP..items::APP + 900).contains(i)).or(rects.first().map(|x| x.0)) {
            self.key_to(i);
        }
    }

    /// Done with the keyboard: the focus goes back (unless something else
    /// took it).
    fn keys_end(&mut self, back: bool) {
        let Some((_, was)) = self.keys.take() else { return };
        self.dirty = true;
        unsafe {
            let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE.0 as isize);
            let _ = SetWindowTextW(self.hwnd, w!("Nookly Taskbar"));
            if back && IsWindow(Some(was)).as_bool() {
                let _ = SetForegroundWindow(was);
            }
        }
        self.left_at = Some(Instant::now());
    }

    /// What the keyboard can reach: an open pop-up's things, else the bar's.
    fn key_rects(&self) -> Vec<(u32, Rect)> {
        let pop = self.popup.is_some() && !matches!(self.popup, Some(Pop::Menu(_)));
        let mut v: Vec<(u32, Rect)> = Vec::new();
        for h in &self.hits {
            let ours = if pop { h.rect.y + h.rect.h / 2.0 < 0.0 } else { h.rect.y + h.rect.h / 2.0 >= 0.0 };
            if ours && h.id != 0 && h.id != items::DESKTOP && h.id < menu::ID && !v.iter().any(|x| x.0 == h.id) {
                v.push((h.id, h.rect));
            }
        }
        v
    }

    fn key_to(&mut self, id: u32) {
        if let Some(k) = self.keys.as_mut() {
            k.0 = Some(id);
        }
        if self.popup.is_none() {
            self.key_home = Some(id);
        }
        self.dirty = true;
        // Read out: the window is called what it's on.
        let name = items::name(self, id);
        unsafe {
            let _ = SetWindowTextW(self.hwnd, &HSTRING::from(name));
            NotifyWinEvent(EVENT_OBJECT_NAMECHANGE, self.hwnd, OBJID_WINDOW.0, 0);
            NotifyWinEvent(EVENT_OBJECT_FOCUS, self.hwnd, OBJID_CLIENT.0, 0);
        }
    }

    fn key_move(&mut self, dx: i32, dy: i32) {
        let rects = self.key_rects();
        if let Some(id) = items::key_step(&rects, self.key_on(), dx, dy) {
            self.key_to(id);
        }
        // Past what's in sight of a full strip: it scrolls along.
        if self.popup.is_none() && self.plan.as_ref().is_some_and(|p| p.strip.is_some()) {
            self.scroll_to += dx as f32 * 48.0;
        }
    }

    /// A key while the bar has the keyboard. True: it was for us.
    fn on_key(&mut self, vk: VIRTUAL_KEY, shift: bool) -> bool {
        if self.keys.is_none() {
            return false;
        }
        if matches!(self.popup, Some(Pop::Menu(_))) {
            match vk {
                VK_UP => menu::key(self, 2),
                VK_DOWN | VK_TAB => menu::key(self, 3),
                VK_RETURN | VK_SPACE => {
                    menu::key(self, 4);
                    if self.popup.is_none() {
                        self.keys_end(false);
                    }
                }
                VK_ESCAPE => self.set_pop(None),
                _ => return false,
            }
            return true;
        }
        match vk {
            VK_LEFT => self.key_move(-1, 0),
            VK_RIGHT => self.key_move(1, 0),
            VK_TAB => self.key_move(if shift { -1 } else { 1 }, 0),
            VK_UP if self.popup.is_some() => self.key_move(0, -1),
            VK_DOWN if self.popup.is_some() => self.key_move(0, 1),
            VK_HOME | VK_END => {
                if let Some(k) = self.keys.as_mut() {
                    k.0 = None;
                }
                self.key_move(1, 0);
                if vk == VK_END {
                    self.key_move(-1, 0);
                }
            }
            // Its windows (an app), or its menu.
            VK_UP => {
                if let Some(i) = self.key_on() {
                    let x = self.hits.iter().find(|h| h.id == i).map_or(0.0, |h| h.rect.cx());
                    if (items::APP..items::APP + 900).contains(&i) {
                        let k = (i - items::APP) as usize;
                        if self.apps.list.get(k).is_some_and(|a| !a.windows.is_empty()) {
                            self.set_pop(Some(Pop::App(k, x)));
                            // (Into it with the next key.)
                            if let Some(k) = self.keys.as_mut() {
                                k.0 = None;
                            }
                        }
                    }
                }
            }
            VK_APPS | VK_F10 if vk == VK_APPS || shift => {
                let id = self.key_on();
                let x = id.and_then(|i| self.hits.iter().find(|h| h.id == i)).map_or(0.0, |h| h.rect.cx());
                menu::open(self, id, x);
            }
            VK_RETURN | VK_SPACE => {
                if let Some(i) = self.key_on() {
                    let x = self.hits.iter().find(|h| h.id == i).map_or(0.0, |h| h.rect.cx());
                    let was = self.popup;
                    items::action(self, i, x);
                    // A pop-up opened: go on in it. Else it's done.
                    if self.popup.is_some() && self.popup != was {
                        if let Some(k) = self.keys.as_mut() {
                            k.0 = None;
                        }
                    } else if self.popup.is_none() {
                        self.keys_end(false);
                    }
                }
            }
            VK_ESCAPE | VK_DOWN => {
                if self.popup.is_some() {
                    self.set_pop(None);
                    if let Some(i) = self.key_home {
                        self.key_to(i);
                    }
                } else {
                    self.keys_end(true);
                }
            }
            // The first nine apps by number.
            v if (VK_1.0..=VK_9.0).contains(&v.0) => {
                let k = (v.0 - VK_1.0) as usize;
                if k < self.apps.list.len() {
                    self.keys_end(false);
                    items::action(self, items::APP + k as u32, 0.0);
                }
            }
            _ => return false,
        }
        true
    }

    /// A widget's view is open and the keys go to it (the notch, which
    /// draws it, is in front).
    fn typing(&self) -> bool {
        if !matches!(self.popup, Some(Pop::Widget(..))) {
            return false;
        }
        unsafe {
            let mut c = [0u16; 32];
            let n = GetClassNameW(GetForegroundWindow(), &mut c);
            String::from_utf16_lossy(&c[..n.max(0) as usize]) == "NooklyNotch"
        }
    }

    /// Files are dragged over the bar or one of its pop-ups, at this point
    /// of the screen (None: they left). Resting on an app brings it forward,
    /// on a window in a pop-up that window. True where letting go opens
    /// them with an app.
    pub fn file_drag(&mut self, at: Option<(i32, i32)>, over: bool) -> bool {
        self.dirty = true;
        let Some((sx, sy)) = at else {
            self.file_over = None;
            self.inside = false;
            self.mouse = (-1e4, -1e4);
            self.left_at = Some(Instant::now());
            return false;
        };
        let top = self.mon.bottom - (BAR_H * self.scale).ceil() as i32;
        let (x, y) = ((sx - self.mon.left) as f32 / self.scale, (sy - top) as f32 / self.scale);
        self.mouse = (x, y);
        self.inside = over;
        self.left_at = if over { None } else { Some(Instant::now()) };
        let hit = self.hit_at(x, y);
        match (hit, self.file_over) {
            (Some(i), Some((o, since, done))) if i == o => {
                if over && !done && since.elapsed().as_secs_f32() > 0.6 {
                    self.file_over = Some((i, since, true));
                    self.file_rest(i, x);
                }
            }
            (Some(i), _) => self.file_over = Some((i, Instant::now(), false)),
            (None, _) => self.file_over = None,
        }
        hit.and_then(|i| self.drop_app(i)).is_some()
    }

    /// The app that opens what's let go here.
    fn drop_app(&self, id: u32) -> Option<apps::App> {
        let k = match id {
            i if (items::APP..items::APP + 900).contains(&i) => i - items::APP,
            i if (items::POP_APP..items::POP_APP + 1000).contains(&i) => i - items::POP_APP,
            _ => return None,
        };
        self.apps.list.get(k as usize).filter(|a| apps::opens_files(a)).cloned()
    }

    fn file_rest(&mut self, id: u32, x: f32) {
        match id {
            i if (items::APP..items::APP + 900).contains(&i) => {
                let k = (i - items::APP) as usize;
                match self.apps.list.get(k).map(|a| a.windows.clone()).unwrap_or_default().as_slice() {
                    [] => {}
                    [h] => {
                        apps::activate(*h);
                        self.hover_block = Some(id);
                        self.set_pop(None);
                    }
                    _ => {
                        let x = self.hits.iter().find(|h| h.id == id).map_or(x, |h| h.rect.cx());
                        self.set_pop(Some(Pop::App(k, x)));
                    }
                }
            }
            i if (items::POP_ROW..items::POP_OPEN).contains(&i) || (items::SUB_ROW..items::SUB_OPEN).contains(&i) => {
                if let Some(h) = items::pop_window(self, i) {
                    apps::activate(h);
                    self.end_peek();
                }
            }
            i if (items::POP_APP..items::POP_APP + 1000).contains(&i) => {
                if let Some(h) = items::grid_app(self, i).and_then(|a| a.front.or(a.windows.first().copied())) {
                    apps::activate(h);
                }
            }
            items::OVERFLOW => self.set_pop(Some(Pop::Overflow(x))),
            i if (items::GROUP..items::GROUP + 100).contains(&i) => self.set_pop(Some(Pop::Group((i - items::GROUP) as usize, x))),
            _ => {}
        }
    }

    /// Files let go at this point of the screen.
    pub fn file_drop(&mut self, at: (i32, i32), files: &[String]) {
        let app = {
            self.file_drag(Some(at), false);
            self.hit_at(self.mouse.0, self.mouse.1).and_then(|i| self.drop_app(i))
        };
        self.file_drag(None, false);
        if let Some(a) = app.filter(|_| !files.is_empty()) {
            self.set_pop(None);
            apps::open_with(&a, files);
        }
    }

    /// The jump list for the open menu has been read.
    fn on_jump(&mut self) {
        let Some(key) = self.menu_app() else { return };
        if let (Some(list), Some(m)) = (jump::take(&key), self.menu.as_mut()) {
            m.add_jump(list);
            self.dirty = true;
        }
    }

    /// Nothing to work out until something happens (each of those marks it
    /// dirty, or is one of these): the loop wakes for every window that
    /// comes and goes anywhere, which is often.
    fn at_rest(&self) -> bool {
        self.resting
            && !self.dirty
            && !self.inside
            && self.popup.is_none()
            && self.drag.is_none()
            && self.down.is_none()
            && self.keys.is_none()
            && self.tray_hold.is_none()
            && self.file_over.is_none()
            && self.peeked.is_none()
            && self.left_at.is_none_or(|t| t.elapsed().as_secs_f32() > 1.0)
            && self.attention_at.is_none_or(|t| t.elapsed().as_secs_f32() > ATTENTION_BEATS + 1.0)
    }

    /// When to wake next with nothing moving: the clock's next minute,
    /// the glass probe, and the auto-hide reveal.
    fn idle_wait(&self) -> u32 {
        let now = time::Now::get();
        let mut ms = (60 - now.sec as u32) * 1000;
        if self.gfx.as_ref().is_some_and(|g| g.theme.glass) && self.shown() > 0.5 {
            ms = ms.min(500);
        }
        if self.tray_hold.is_some() {
            ms = ms.min(120);
        }
        if self.shown() > 0.5 && band::busy() {
            ms = ms.min(66);
        }
        // An app asking to be looked at keeps an auto-hidden bar out a while.
        if self.autohide() && self.attention_at.is_some_and(|t| t.elapsed().as_secs_f32() < ATTENTION_BEATS + 0.5) {
            ms = ms.min(250);
        }
        // Auto-hide: watch for the cursor at the screen's bottom edge.
        if self.autohide() && (self.left_at.is_some() || self.shown() < 0.5) {
            ms = ms.min(100);
        }
        // Out (auto-hide) or a pop-up open: check where the mouse really is
        // now and then (see verify_mouse), so neither can get stuck.
        if (self.autohide() && self.shown() > 0.5) || self.popup.is_some() {
            ms = ms.min(250);
        }
        ms.max(15)
    }

    /// Where the mouse really is: Windows doesn't always say the mouse left
    /// (a window resized under it, or it left straight from a pop-up), and
    /// the bar would then stay out (auto-hide) and its last pop-up open. And
    /// is Start (or Search) still in front?
    fn verify_mouse(&mut self) {
        unsafe {
            let mut p = POINT::default();
            if GetCursorPos(&mut p).is_err() {
                return;
            }
            // Dragging: the bar has the mouse, wherever it is.
            if self.drag.is_some() {
                return;
            }
            let under = WindowFromPoint(p);
            let bubbles: Vec<HWND> = [self.pop_b.as_ref(), self.sub_b.as_ref()].into_iter().flatten().map(|b| b.hwnd).collect();
            if under != self.hwnd && !bubbles.contains(&under) {
                let mut changed = false;
                if self.inside {
                    self.inside = false;
                    self.left_at = Some(Instant::now());
                    changed = true;
                }
                for b in [self.pop_b.as_mut(), self.sub_b.as_mut()].into_iter().flatten() {
                    if b.inside {
                        b.inside = false;
                        changed = true;
                    }
                }
                // A press that left the bar: its release went elsewhere.
                if self.down.is_some() && self.drag.is_none() {
                    self.down = None;
                    changed = true;
                }
                if changed {
                    self.mouse = (-1e4, -1e4);
                    self.dirty = true;
                }
            }
            // (A press on the bar is left alone: Windows can answer "up" for
            // the mouse button to a window that isn't in front, and clearing
            // it here would lose the click.)
            if self.shell_open && !is_shell(GetForegroundWindow()) {
                self.shell_open = false;
                self.dirty = true;
            }
        }
    }
}

/// What the taskbar is doing right now (a window it reads, an icon it
/// makes...), written to taskbar_crash.txt if Windows ends it with a crash.
static CRUMB: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

pub fn crumb(s: impl Into<String>) {
    if let Ok(mut c) = CRUMB.try_lock() {
        *c = s.into();
    }
}

/// A crash Windows raised (not a Rust panic): what and where, and what the
/// taskbar was doing; then the Windows taskbar comes back.
unsafe extern "system" fn on_crash(info: *const windows::Win32::System::Diagnostics::Debug::EXCEPTION_POINTERS) -> i32 {
    let (code, addr) = unsafe {
        info.as_ref().and_then(|i| i.ExceptionRecord.as_ref()).map_or((0, 0), |r| (r.ExceptionCode.0 as u32, r.ExceptionAddress as usize))
    };
    let base = unsafe { GetModuleHandleW(None).map_or(0, |h| h.0 as usize) };
    let doing = CRUMB.try_lock().map(|c| c.clone()).unwrap_or_else(|_| "?".into());
    nookly_core::store::write("taskbar_crash.txt", &format!("{} crash {code:#x} at {addr:#x} (exe base {base:#x}) while: {doing}\n", time::unix_now() as u64));
    shell::restore_windows_taskbar();
    0
}

/// Debug log (NOOKLY_BAR_LOG=<file>).
pub fn dbg_log(s: &str) {
    let file = std::env::var("NOOKLY_BAR_LOG").ok().or_else(|| {
        let d = nookly_core::store::dir();
        d.join("taskbar_debug.txt").exists().then(|| d.join("taskbar_debug.log").display().to_string())
    });
    if let Some(f) = file {
        use std::io::Write;
        if let Ok(mut fh) = std::fs::OpenOptions::new().create(true).append(true).open(f) {
            let _ = writeln!(fh, "{:.3} {s}", time::unix_now() % 1000.0);
        }
    }
}

/// Settings > Taskbar > Size.
fn size_factor(st: &Settings) -> f32 {
    match st.extra.get("bar_size").map(String::as_str) {
        Some("s") => 0.88,
        Some("l") => 1.12,
        _ => 1.0,
    }
}

fn dips(lp: LPARAM, scale: f32) -> (f32, f32) {
    let x = (lp.0 & 0xFFFF) as i16 as f32;
    let y = ((lp.0 >> 16) & 0xFFFF) as i16 as f32;
    (x / scale, y / scale)
}

static TASKBAR_CREATED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// The notch posts this after saving settings.
static SETTINGS_CHANGED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// A window event arrived and the app list isn't read yet.
static APPS_DIRTY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The notch posts this when the widget values changed.
static FEED_CHANGED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// The notch posts this when the widget faces changed.
static FACES_CHANGED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// The notch: the taskbar's shortcut was pressed.
static KEYS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// The shell's messages (a window flashed).
static SHELL_HOOK: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static BAR_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let tc = TASKBAR_CREATED.load(std::sync::atomic::Ordering::Relaxed);
    let sc = SETTINGS_CHANGED.load(std::sync::atomic::Ordering::Relaxed);
    let me = bar_of(hwnd);
    if msg == tc && tc != 0 {
        // Explorer restarted: its taskbar is back. Hide it again, re-reserve.
        shell::hide_windows_taskbar();
        hook_explorer();
        on_bar(me, |b| {
            b.appbar = false;
            // The new Explorer knows no strip: set it again, even unchanged.
            b.appbar_rc = RECT::default();
            b.sync_appbar();
        });
        return LRESULT(0);
    }
    if msg == SHELL_HOOK.load(std::sync::atomic::Ordering::Relaxed) && msg != 0 {
        // HSHELL_FLASH: a window wants to be looked at.
        if wp.0 as u32 & 0xFFFF == 0x8006 {
            flashed(HWND(lp.0 as *mut _));
        }
        return LRESULT(0);
    }
    if msg == KEYS.load(std::sync::atomic::Ordering::Relaxed) && msg != 0 {
        on_bar(me, |b| b.keys_start());
        return LRESULT(0);
    }
    if msg == unread::WM_UNREAD || msg == band::WM_BAND {
        each_bar(|b| b.dirty = true);
        return LRESULT(0);
    }
    if msg == jump::WM_JUMP {
        on_bar(me, |b| b.on_jump());
        return LRESULT(0);
    }
    if msg == tray::WM_TRAY_CHANGED {
        each_bar(|b| {
            b.dirty = true;
            if let Some(p @ Pop::Tray(_)) = b.popup {
                b.aim_pop(p);
            }
        });
        return LRESULT(0);
    }
    let xc = FACES_CHANGED.load(std::sync::atomic::Ordering::Relaxed);
    if msg == xc && xc != 0 {
        each_bar(|b| {
            if b.faces.reload() {
                b.dirty = true;
                if let Some(p @ Pop::Widget(..)) = b.popup {
                    b.aim_pop(p);
                }
            }
        });
        return LRESULT(0);
    }
    let fc = FEED_CHANGED.load(std::sync::atomic::Ordering::Relaxed);
    if msg == fc && fc != 0 {
        each_bar(|b| {
            if let Some(f) = nookly_core::feed::read() {
                if f != b.feed {
                    b.feed = f;
                    b.dirty = true;
                }
            }
        });
        return LRESULT(0);
    }
    if msg == sc && sc != 0 {
        each_bar(|b| {
            b.load_settings();
            b.place();
        });
        SCREENS_DIRTY.store(true, std::sync::atomic::Ordering::Relaxed);
        return LRESULT(0);
    }
    match msg {
        WM_MOUSEMOVE => {
            on_bar(me, |b| {
                let (x, y) = b.to_bar(hwnd, lp);
                if b.is_pop(hwnd) {
                    b.on_pop_move(hwnd, x, y);
                } else {
                    b.on_move(x, y);
                }
                b.drag_move();
            });
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            on_bar(me, |b| if b.is_pop(hwnd) { b.on_pop_leave(hwnd) } else { b.on_leave() });
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            on_bar(me, |b| {
                let (x, y) = b.to_bar(hwnd, lp);
                if b.is_pop(hwnd) {
                    b.down = b.hit_at(x, y);
                    b.press_closes_menu(b.down);
                    b.forward(nookly_core::surfaces::INPUT_DOWN, x, y, 0);
                    b.start_drag(x, y);
                } else {
                    b.on_down(x, y);
                }
            });
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            on_bar(me, |b| {
                let (x, y) = b.to_bar(hwnd, lp);
                b.on_up(x, y);
            });
            LRESULT(0)
        }
        WM_MBUTTONUP => {
            on_bar(me, |b| {
                let (x, y) = b.to_bar(hwnd, lp);
                b.on_middle(x, y);
            });
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            on_bar(me, |b| {
                let (x, y) = b.to_bar(hwnd, lp);
                b.on_right(x, y);
            });
            LRESULT(0)
        }
        // Something else took the mouse in the middle of a drag: it's off.
        WM_CAPTURECHANGED => {
            on_bar(me, |b| {
                if b.drag.take().is_some() {
                    b.down = None;
                    b.dirty = true;
                }
            });
            LRESULT(0)
        }
        WM_HOTKEY => {
            on_bar(me, |b| menu::key(b, wp.0 as i32));
            LRESULT(0)
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
            if on_bar(me, |b| b.on_key(VIRTUAL_KEY(wp.0 as u16), shift)) == Some(true) {
                return LRESULT(0);
            }
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
        WM_KILLFOCUS => {
            on_bar(me, |b| b.keys_end(false));
            LRESULT(0)
        }
        WM_INPUT => {
            if menu::pressed(lp) {
                on_bar(me, |b| b.pressed_anywhere());
            }
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
        // The wheel over the bar scrolls the apps-and-widgets strip (when it
        // doesn't fit), sideways wheels too.
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            let d = ((wp.0 >> 16) & 0xFFFF) as i16 as f32 / 120.0;
            let d = if msg == WM_MOUSEHWHEEL { -d } else { d };
            let raw = ((wp.0 >> 16) & 0xFFFF) as i16;
            let fwd = on_bar(me, |b| msg == WM_MOUSEWHEEL && b.forward(nookly_core::surfaces::INPUT_WHEEL, b.mouse.0, b.mouse.1, raw));
            if fwd == Some(true) {
                return LRESULT(0);
            }
            on_bar(me, |b| {
                if b.plan.as_ref().is_some_and(|p| p.strip.is_some()) {
                    b.scroll_to -= d * 64.0;
                    b.dirty = true;
                }
            });
            LRESULT(0)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_APPBAR => {
            if wp.0 as u32 == ABN_POSCHANGED {
                on_bar(me, |b| b.sync_appbar());
            } else if wp.0 as u32 == ABN_FULLSCREENAPP {
                on_bar(me, |b| b.check_fullscreen());
            }
            LRESULT(0)
        }
        WM_KEEP_HIDDEN => {
            shell::keep_hidden();
            LRESULT(0)
        }
        WM_APPS_CHANGED => LRESULT(0),
        // (Not for the work area changing: that's our own strip, and
        // answering it would loop.)
        WM_SETTINGCHANGE if wp.0 as u32 == SPI_SETWORKAREA.0 => LRESULT(0),
        WM_DISPLAYCHANGE | WM_DPICHANGED | WM_SETTINGCHANGE => {
            if !on_bar(me, |b| b.is_pop(hwnd)).unwrap_or(false) {
                on_bar(me, |b| b.place());
                SCREENS_DIRTY.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            LRESULT(0)
        }
        // Signing out or shutting down. When Nookly doesn't start with
        // Windows, Windows' own taskbar has to be as it was next time.
        WM_ENDSESSION => {
            if wp.0 != 0 && me == Some(0) && !shell::starts_with_windows() {
                each_bar(|b| b.remove_appbar());
                shell::restore_windows_taskbar();
            }
            LRESULT(0)
        }
        WM_CLOSE if me == Some(0) && !on_bar(me, |b| b.is_pop(hwnd)).unwrap_or(false) => {
            quit();
            LRESULT(0)
        }
        WM_CLOSE => LRESULT(0),
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

/// Leave cleanly: give the screen strip back and put the Windows taskbar back.
fn quit() {
    crumb("quit: appbar");
    peek::stop();
    each_bar(|b| b.remove_appbar());
    shell::restore_windows_taskbar();
    unsafe {
        PostQuitMessage(0);
    }
}

static HOOK_PID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Explorer shows its taskbar again now and then; hear about it.
fn hook_explorer() {
    let Some(t) = shell::tray() else { return };
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(t, Some(&mut pid));
    }
    if pid == 0 || HOOK_PID.swap(pid, std::sync::atomic::Ordering::Relaxed) == pid {
        return;
    }
    unsafe {
        SetWinEventHook(EVENT_OBJECT_SHOW, EVENT_OBJECT_SHOW, None, Some(on_show), pid, 0, WINEVENT_OUTOFCONTEXT);
    }
}

fn post_bar(msg: u32) {
    let h = BAR_HWND.load(std::sync::atomic::Ordering::Relaxed);
    if h != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(h as *mut _)), msg, WPARAM(0), LPARAM(0));
        }
    }
}

unsafe extern "system" fn on_show(_: HWINEVENTHOOK, _: u32, hwnd: HWND, id_object: i32, _: i32, _: u32, _: u32) {
    if id_object == OBJID_WINDOW.0 && shell::trays().contains(&hwnd) {
        post_bar(WM_KEEP_HIDDEN);
    }
}

/// Start, Search or one of Windows' own panels (by the program behind it).
fn is_shell(hwnd: HWND) -> bool {
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    let exe = apps::exe_of(pid).unwrap_or_default().to_ascii_lowercase();
    if ["startmenuexperiencehost.exe", "searchhost.exe", "searchapp.exe", "shellexperiencehost.exe"].iter().any(|n| exe.ends_with(n)) {
        return true;
    }
    // Wi-Fi / sound / battery on newer Windows.
    let mut c = [0u16; 64];
    let n = unsafe { GetClassNameW(hwnd, &mut c) };
    exe.ends_with("shellhost.exe") && String::from_utf16_lossy(&c[..n.max(0) as usize]) == "ControlCenterWindow"
}

/// Top-level windows made, closed, shown, hidden, renamed, minimized or
/// brought to front: the app list needs reading again (once, soon).
unsafe extern "system" fn on_window(_: HWINEVENTHOOK, ev: u32, hwnd: HWND, id_object: i32, id_child: i32, _: u32, _: u32) {
    if id_object != OBJID_WINDOW.0 || id_child != 0 || hwnd.is_invalid() {
        return;
    }
    if ev != EVENT_SYSTEM_FOREGROUND && unsafe { GetAncestor(hwnd, GA_ROOT) } != hwnd {
        return;
    }
    if ev == EVENT_SYSTEM_FOREGROUND {
        // Start, Search or Windows' panels in front: an auto-hidden bar
        // comes out with them.
        let (shell, screen) = unsafe { (is_shell(hwnd), MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY)) };
        each_bar(|b| {
            b.check_fullscreen();
            // (The bar of the screen it opens on.)
            let shell = shell && b.monitor() == screen;
            if b.shell_open != shell {
                b.shell_open = shell;
                b.dirty = true;
            }
            // Another window came to the front: a menu is done.
            if matches!(b.popup, Some(Pop::Menu(_))) {
                b.set_pop(None);
            }
        });
    }
    // Titles show only in pop-ups: a renamed window (a page title, a
    // ticking timer) needs nothing while none is open.
    if ev == EVENT_OBJECT_NAMECHANGE {
        let mut open = false;
        each_bar(|b| open |= b.popup.is_some());
        if !open {
            return;
        }
    }
    if !APPS_DIRTY.swap(true, std::sync::atomic::Ordering::Relaxed) {
        post_bar(WM_APPS_CHANGED);
    }
}

/// A window flashed (the shell told us): every bar shows it.
fn flashed(h: HWND) {
    BARS.with(|b| {
        let Ok(mut b) = b.try_borrow_mut() else { return };
        let Some((main, more)) = b.split_first_mut() else { return };
        if main.on_flash(h) {
            for x in more {
                x.apps.attention = main.apps.attention.clone();
                x.attention_at = main.attention_at;
                x.dirty = true;
            }
        }
    });
}

/// The screens: (handle, device name, the main one).
fn screens() -> Vec<(HMONITOR, String, bool)> {
    unsafe extern "system" fn each(m: HMONITOR, _: HDC, _: *mut RECT, l: LPARAM) -> BOOL {
        unsafe {
            let v = &mut *(l.0 as *mut Vec<(HMONITOR, String, bool)>);
            let mut mi = MONITORINFOEXW::default();
            mi.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            if GetMonitorInfoW(m, &mut mi as *mut _ as *mut MONITORINFO).as_bool() {
                let n = mi.szDevice.iter().position(|&c| c == 0).unwrap_or(0);
                v.push((m, String::from_utf16_lossy(&mi.szDevice[..n]), mi.monitorInfo.dwFlags & 1 != 0));
            }
            TRUE
        }
    }
    let mut v: Vec<(HMONITOR, String, bool)> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(each), LPARAM(&mut v as *mut _ as isize));
    }
    v
}

/// A bar for every other screen (Settings > Taskbar > On every screen), as
/// screens come and go.
fn sync_screens() {
    let on = with_bar(|b| nookly_core::barcfg::other_screens(&b.settings)).unwrap_or(false);
    let want: Vec<String> = if on { screens().into_iter().filter(|s| !s.2).map(|s| s.1).collect() } else { Vec::new() };
    let mut gone = Vec::new();
    BARS.with(|b| {
        if let Ok(mut b) = b.try_borrow_mut() {
            let mut k = 1;
            while k < b.len() {
                if want.contains(&b[k].device) {
                    k += 1;
                } else {
                    gone.push(b.remove(k));
                }
            }
        }
    });
    for mut b in gone {
        b.end_peek();
        b.remove_appbar();
        unsafe {
            for x in [b.pop_b.take(), b.sub_b.take()].into_iter().flatten() {
                let _ = DestroyWindow(x.hwnd);
            }
            let _ = windows::Win32::System::Ole::RevokeDragDrop(b.hwnd);
            let _ = DestroyWindow(b.hwnd);
        }
    }
    let mut have = Vec::new();
    each_bar(|b| have.push(b.device.clone()));
    for d in want {
        if !have.contains(&d) {
            crumb(format!("bar on {d}"));
            if let Ok(mut b) = Bar::new(&d) {
                b.load_settings();
                with_bar(|m| b.apps.copy_from(&m.apps));
                b.faces.reload();
                b.place();
                b.check_fullscreen();
                let hwnd = b.hwnd;
                BARS.with(|v| {
                    if let Ok(mut v) = v.try_borrow_mut() {
                        v.push(b);
                    }
                });
                unsafe {
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                }
                drop::register(hwnd);
            }
        }
    }
}

/// Tell the notch what can be seen of the taskbar (on any screen).
fn tell_shown() {
    let mut now = (false, 0u64);
    each_bar(|b| now = (now.0 || b.out_seen.0, now.1 | b.out_seen.1));
    if TOLD_OUT.replace(now) != now {
        unsafe {
            if let Ok(notch) = FindWindowW(w!("NooklyNotch"), PCWSTR::null()) {
                let msg = RegisterWindowMessageW(nookly_core::surfaces::SHOWN_MSG);
                let _ = PostMessageW(Some(notch), msg, WPARAM(now.0 as usize), LPARAM(now.1 as isize));
            }
        }
    }
}

impl Bar {
    /// A bar on a screen (its device name; empty: the main one).
    fn new(device: &str) -> Result<Bar> {
        unsafe {
            let hinst = GetModuleHandleW(None)?;
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                if device.is_empty() { CLASS } else { MORE_CLASS },
                w!("Nookly Taskbar"),
                WS_POPUP,
                0,
                0,
                100,
                100,
                None,
                None,
                Some(hinst.into()),
                None,
            )?;
            shell::keep_in_peek(hwnd);
            let gfx = Gfx::new(10, 10)?;
            let now = Instant::now();
            Ok(Bar {
                hwnd,
                device: device.to_string(),
                out_seen: (false, 0),
                keys: None,
                key_home: None,
                file_over: None,
                pop_b: None,
                sub_b: None,
                sub_app: None,
                sub_hover: None,
                sub_leave: None,
                thumb_slots: Vec::new(),
                hover_block: None,
                drag: None,
                menu: None,
                peek_hover: None,
                peeked: None,
                peek_left: now,
                menu_keys: false,
                tray_hold: None,
                attention_at: None,
                appbar_rc: RECT::default(),
                strip_at: now,
                strip_tries: 0,
                gfx: Some(gfx),
                settings: Settings::load(),
                mon: RECT::default(),
                scale: 1.0,
                w: 100.0,
                h: 100.0,
                hits: Vec::new(),
                mouse: (-1e4, -1e4),
                inside: false,
                down: None,
                width: Spring::default(),
                depth: Spring::default(),
                last_step: now,
                start: now,
                left_at: None,
                fullscreen: false,
                appbar: false,
                backdrop: None,
                backdrop_gen: 0,
                backdrop_key: 0,
                resting: false,
                drawn_away: None,
                glass_light: false,
                probe: None,
                next_probe: now,
                next_grab: now,
                order: Vec::new(),
                placed: Vec::new(),
                apps: apps::Apps::new(),
                icons: Icons::default(),
                feed: nookly_core::feed::read().unwrap_or_default(),
                faces: Faces::default(),
                scroll: 0.0,
                scroll_to: 0.0,
                shell_open: false,
                strip_clip: None,
                dirty: true,
                drawn_minute: 99,
                plan_fit: layout::Fit { shown: 0, overflow: false },
                plan: None,
                popup: None,
                pop_since: now,
                hover_hit: None,
                hover_since: now,
                pop_leave: None,
            })
        }
    }
}

fn create() -> Result<()> {
    unsafe {
        let hinst = GetModuleHandleW(None)?;
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            lpszClassName: CLASS,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        RegisterClassExW(&wc);
        RegisterClassExW(&WNDCLASSEXW { lpszClassName: bubble::CLASS, ..wc });
        RegisterClassExW(&WNDCLASSEXW { lpszClassName: MORE_CLASS, ..wc });
        let mut bar = Bar::new("")?;
        let hwnd = bar.hwnd;
        BAR_HWND.store(hwnd.0 as isize, std::sync::atomic::Ordering::Relaxed);
        SHELL_HOOK.store(RegisterWindowMessageW(w!("SHELLHOOK")), std::sync::atomic::Ordering::Relaxed);
        let _ = RegisterShellHookWindow(hwnd);
        bar.load_settings();
        shell::hide_windows_taskbar();
        bar.place();
        bar.check_fullscreen();
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        BARS.with(|b| b.borrow_mut().push(bar));
        drop::register(hwnd);
        SCREENS_DIRTY.store(true, std::sync::atomic::Ordering::Relaxed);
        hook_explorer();
        let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
        SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, None, Some(on_window), 0, 0, flags);
        SetWinEventHook(EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND, None, Some(on_window), 0, 0, flags);
        SetWinEventHook(EVENT_OBJECT_CREATE, EVENT_OBJECT_HIDE, None, Some(on_window), 0, 0, flags);
        SetWinEventHook(EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_NAMECHANGE, None, Some(on_window), 0, 0, flags);
        SetWinEventHook(EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED, None, Some(on_window), 0, 0, flags);
        Ok(())
    }
}

/// Frames: one per display refresh while something moves, otherwise sleep
/// until a message arrives or the next scheduled look (clock, glass probe).
fn run_loop() {
    let mut msg = MSG::default();
    let mut last_scan = Instant::now();
    let mut last_look = Instant::now();
    loop {
                unsafe {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    return;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        if SCREENS_DIRTY.swap(false, std::sync::atomic::Ordering::Relaxed) {
            shell::keep_hidden();
            sync_screens();
        }
        // Window events come in bursts: read the list at most every 50 ms.
        // (The main bar reads it, the others take it over.)
        let order = ORDER_DIRTY.swap(false, std::sync::atomic::Ordering::Relaxed);
        if order || (APPS_DIRTY.load(std::sync::atomic::Ordering::Relaxed) && last_scan.elapsed().as_millis() >= 50) {
            APPS_DIRTY.store(false, std::sync::atomic::Ordering::Relaxed);
            last_scan = Instant::now();
            crumb("scan");
            BARS.with(|b| {
                let Ok(mut b) = b.try_borrow_mut() else { return };
                let Some((main, more)) = b.split_first_mut() else { return };
                let st = main.settings.clone();
                if order {
                    main.apps.read_order();
                }
                let before = main.apps.signature();
                main.apps.scan(&st);
                main.apps_read(before);
                for x in more {
                    let before = x.apps.signature();
                    x.apps.copy_from(&main.apps);
                    x.apps_read(before);
                }
            });
        }
        let mut moving = false;
        each_bar(|b| {
            if time::Now::get().min != b.drawn_minute {
                b.dirty = true;
            }
            b.tick_glass();
            let m = if b.at_rest() {
                // (The springs count from now on.)
                b.last_step = Instant::now();
                false
            } else {
                crumb("step");
                b.step()
            };
            b.resting = !m;
            // Slid away, it looks the same whatever its widgets show.
            if !m && b.drawn_away == Some(b.fullscreen) && b.depth.pos < 0.5 {
                b.dirty = false;
                b.drawn_minute = time::Now::get().min;
            }
            if m || b.dirty || (b.shown() > 0.5 && band::busy()) {
                crumb("render");                b.render();
            }
            crumb("idle");
            moving |= m;
        });
        tell_shown();
        unsafe {
            if moving {
                time::wait_refresh();
            } else {
                let mut wait = 1000;
                each_bar(|b| wait = wait.min(b.idle_wait()));
                if APPS_DIRTY.load(std::sync::atomic::Ordering::Relaxed) {
                    wait = wait.min(50);
                }
                MsgWaitForMultipleObjectsEx(None, wait, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
                if last_look.elapsed().as_millis() < 60 {
                    continue;
                }
                last_look = Instant::now();                each_bar(|b| {
                    b.check_fullscreen();
                    b.check_strip();
                    if b.inside || b.popup.is_some() || b.shell_open || b.down.is_some() {
                        b.verify_mouse();
                    }
                    // Auto-hide: notice the cursor at the screen's bottom
                    // edge even where no sliver catches it.
                    if b.autohide() && !b.inside {
                        let mut p = POINT::default();
                        let _ = GetCursorPos(&mut p);
                        if p.y >= b.mon.bottom - 2 && p.y <= b.mon.bottom && p.x >= b.mon.left && p.x < b.mon.right {
                            b.left_at = Some(Instant::now());
                        }
                    }
                });
            }
        }
    }
}

fn main() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let args: Vec<String> = std::env::args().collect();
        // `--restore`: just put the Windows taskbar back (used by the
        // uninstaller and as a manual escape hatch).
        if args.iter().any(|a| a == "--restore") {
            shell::restore_windows_taskbar();
            return;
        }
        // `--icon-test <path> <px>`: what the shell gives for an icon.
        if let Some(i) = args.iter().position(|a| a == "--icon-test") {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let px: u32 = args.get(i + 2).and_then(|v| v.parse().ok()).unwrap_or(32);
            let out = match apps::icon_pixels(args.get(i + 1).map(String::as_str).unwrap_or(""), px) {
                Some((s, d)) => format!("size {s} bytes {} opaque {} max alpha {}", d.len(), d.chunks(4).filter(|p| p[3] > 0).count(), d.chunks(4).map(|p| p[3]).max().unwrap_or(0)),
                None => "none".into(),
            };
            let _ = std::fs::write("icon-test.txt", out);
            return;
        }
        // `--dump-jump <file>`: the jump lists of the apps open now.
        if let Some(i) = args.iter().position(|a| a == "--dump-jump") {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let mut a = apps::Apps::new();
            a.scan(&Settings::load());
            let out: String = a.list.iter().map(|x| format!("{}\n{}", x.key, jump::dump(&x.key, &x.launch, x.target.as_deref()))).collect();
            let _ = std::fs::write(args.get(i + 1).cloned().unwrap_or_else(|| "jump.txt".into()), out);
            return;
        }
        // `--dump-apps <file>`: write the app list (keys, windows) for checking.
        if let Some(i) = args.iter().position(|a| a == "--dump-apps") {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let mut a = apps::Apps::new();
            a.scan(&Settings::load());
            let out: String = a
                .list
                .iter()
                .map(|x| format!("{} | {} | {} | pinned={} | windows={:?}\n", x.key, x.name, x.launch, x.pinned, x.windows.iter().map(|&h| (apps::title(h), apps::debug_ids(h))).collect::<Vec<_>>()))
                .collect();
            let _ = std::fs::write(args.get(i + 1).cloned().unwrap_or_else(|| "apps.txt".into()), out);
            return;
        }
        let mutex = CreateMutexW(None, true, w!("Local\\Nookly.Taskbar"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }
        // (With drag and drop.)
        let _ = windows::Win32::System::Ole::OleInitialize(None);
        // Whatever happens, the Windows taskbar comes back (the notch also
        // puts it back if this process dies).
        windows::Win32::System::Diagnostics::Debug::SetUnhandledExceptionFilter(Some(on_crash));
        std::panic::set_hook(Box::new(|info| {
            peek::stop();
            shell::restore_windows_taskbar();
            // What went wrong, for the next fix (taskbar_crash.txt).
            let at = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
            let msg = info.payload().downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| info.payload().downcast_ref::<String>().cloned()).unwrap_or_default();
            let doing = CRUMB.try_lock().map(|c| c.clone()).unwrap_or_default();
            nookly_core::store::write("taskbar_crash.txt", &format!("{} {at}: {msg} (while: {doing})\n", time::unix_now() as u64));
        }));
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), std::sync::atomic::Ordering::Relaxed);
        SETTINGS_CHANGED.store(RegisterWindowMessageW(w!("Nookly.SettingsChanged")), std::sync::atomic::Ordering::Relaxed);
        FEED_CHANGED.store(RegisterWindowMessageW(nookly_core::feed::FEED_MSG), std::sync::atomic::Ordering::Relaxed);
        FACES_CHANGED.store(RegisterWindowMessageW(nookly_core::surfaces::CHANGED_MSG), std::sync::atomic::Ordering::Relaxed);
        KEYS.store(RegisterWindowMessageW(nookly_core::surfaces::KEYS_MSG), std::sync::atomic::Ordering::Relaxed);
        // Which version this add-on is (the notch replaces one that doesn't
        // match it, after an update).
        nookly_core::store::write("taskbar_version.txt", env!("CARGO_PKG_VERSION"));
        if let Err(e) = create() {
            shell::restore_windows_taskbar();
            let msg = HSTRING::from(format!("Nookly Taskbar failed to start:\n{e}"));
            MessageBoxW(None, &msg, w!("Nookly"), MB_ICONERROR);
            return;
        }
        with_bar(|b| {
            b.faces.reload();
            want_faces(b.glass_light, b.scale);
        });
        run_loop();
        crumb("exit: appbar");
        each_bar(|b| b.remove_appbar());
        crumb("exit: restore");
        shell::restore_windows_taskbar();
        crumb("exit: mutex");
        drop(mutex);
        // End here, everything put back: Windows' own unloading of the shell
        // and DWM libraries raised a crash (0x87a) on the way out otherwise,
        // and the notch counted that as the taskbar failing.
        crumb("exit: process");
        let _ = windows::Win32::System::Threading::TerminateProcess(windows::Win32::System::Threading::GetCurrentProcess(), 0);
        std::process::exit(0);
    }
}
