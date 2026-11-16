//! The notch window: layout, animation, input routing, toasts and the mascot.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::time::Instant;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::gfx::*;
use crate::modules::{self, side_rects, CatalogItem, Ctx, Module, Toast, Widget};
use crate::mascot::{self, Mood};
use crate::store::{AlertMode, ClipMode, Edge, Mascot, NotchLayout, NotchRow, NotchStyle, Settings, ShowWhen, Sound};
use crate::time::Now;
use crate::ui::{Hit, Ui};
use crate::{dragdrop, sound, tray};

mod settings;
use settings::*;

pub const WM_APP_WAKE: u32 = WM_APP + 1;
pub const WM_APP_TRAY: u32 = WM_APP + 2;
const WM_MOUSELEAVE: u32 = 0x02A3;

/// Logical canvas (DIPs). The window is this big; transparent parts are click-through.
const CANVAS_W: f32 = 780.0;
const CANVAS_H: f32 = 360.0;
const TAB_H: f32 = 30.0;
/// Space between tab icons (tight: there are up to a dozen tabs).
const TAB_GAP: f32 = 2.0;
const PAD: f32 = 16.0;
/// Slim style: a shorter tab bar, tighter padding, and the panel content
/// this much narrower and shorter than Full's.
const SLIM_TAB_H: f32 = 26.0;
/// The tallest a tab's content may make the panel (fits the canvas).
const TALL_PANEL: f32 = 250.0;
const SLIM_PAD: f32 = 12.0;
const SLIM_TRIM: (f32, f32) = (80.0, 24.0);
/// Slim closed notch while something is live: its width (the same for
/// every activity), edge padding, the space between each wing and the
/// clock in the middle, and the strip a side mascot adds.
const SLIM_LIVE_W: f32 = 200.0;
/// With seconds on the clock, the narrow notches grow this much.
const SECONDS_W: f32 = 24.0;
const SLIM_WING_PAD: f32 = 10.0;
const SLIM_GAP: f32 = 6.0;
const SLIM_MASCOT_STRIP: f32 = 62.0;
/// Tab bar hits use this module index.
const APP_HIT: usize = usize::MAX;
/// Toast action hits use this module index; the id is the action slot.
const TOAST: usize = usize::MAX - 1;
/// Settings window hits use this module index (see settings.rs).
const SETTINGS: usize = usize::MAX - 2;
/// Tab bar id of the gear button (other tab ids are module indexes).
const GEAR: u32 = 1000;
/// The ⋯ button for the tabs that don't fit.
const MORE: u32 = 1001;
/// Narrowest tab before the rest go under ⋯.
const TAB_MIN: f32 = 26.0;
/// Welcome tour panel content size, its hits' module index and action ids.
const ONBOARD_SIZE: (f32, f32) = (600.0, 214.0);
const ONBOARD: usize = usize::MAX - 3;
const O_NEXT: u32 = 1;
/// Last tour step: finish and open Settings.
const O_SETTINGS: u32 = 4;
const O_BACK: u32 = 2;
const O_SKIP: u32 = 3;
const O_STYLE: u32 = 10; // + index into NotchStyle::ALL
const O_EDGE: u32 = 20; // + index into Edge::ALL
const O_PRESET: u32 = 30; // + index into features::PRESETS
const ONBOARD_STEPS: u8 = 5;
/// Raise this when the welcome tour gains something new: everyone sees it
/// once more after updating.
const TOUR_VERSION: u32 = 3;
/// Tour step 3: how to use it.
const TOUR_TIPS: [(&str, &str, &str); 4] = [
    (glyph::PLAY, "Hover to open", "Move the mouse away and it tucks itself back."),
    ("\u{E962}", "Scroll to switch", "Roll the wheel on the open notch to change panels."),
    ("\u{E765}", "Win+Alt+N", "Opens it from anywhere, no mouse needed."),
    (glyph::SETTINGS, "Gear or right-click", "Settings, in a window of its own. Right-drag moves it."),
];
/// Tour step 3: themes to start with (indices into gfx::THEMES).
const TOUR_THEMES: [usize; 6] = [0, 2, 3, 6, 8, 9];
const O_THEME: u32 = 40; // + index into TOUR_THEMES
/// What's in the Settings window, at a glance: its sections (the Notch
/// tab), then the Taskbar tab.
const TOUR_SETTINGS: [(&str, &str, &str); 8] = [
    ("\u{E771}", "Appearance", "Style, edge and size"),
    ("\u{E790}", "Theme", "Colours, clock, song glow"),
    ("\u{E76E}", "Mascot", "In the notch or taskbar"),
    ("\u{E8A9}", "Features", "Switch on what you use"),
    ("\u{ECA5}", "Widgets", "What the closed notch shows"),
    (glyph::BELL, "Alerts", "Sounds and notifications"),
    ("\u{E765}", "Shortcuts", "Keys and the mouse"),
    ("\u{E7C4}", "Taskbar", "Coming soon"),
];
const TOUR_Q_SETTINGS: &str = "Click the gear, right-click the notch or click the tray icon for Settings:";

/// Hit id of a toast's body (clicking the toast itself, not a button).
const TOAST_BODY: u32 = 999;

const TIMER_TICK: usize = 1;
const TIMER_FRAME: usize = 2;
/// Watches the cursor while the notch is click-through ("make room").
const TIMER_POLL: usize = 3;
/// Re-checks the windows under the notch while one is being dragged.
const TIMER_MOVE: usize = 7;
/// Liquid Glass: grab the background again once a window that changed
/// under the notch has finished animating.
const TIMER_GLASS: usize = 8;
/// Liquid Glass: looks at a thin ring of pixels just around the notch a few
/// times a second, to notice what's behind it changing with no window event
/// (scrolling, a video, a page loading).
const TIMER_GLASS_PROBE: usize = 9;
/// How often the ring is checked (ms): at rest, and while what's behind is
/// moving (until it has been still for GLASS_LIVE seconds).
const GLASS_PROBE_MS: u32 = 200;
const GLASS_LIVE_MS: u32 = 33;
const GLASS_LIVE: f64 = 1.0;
/// Margin (in grab pixels, 4 screen pixels each) kept around the notch when
/// only its own part is grabbed again: wider than the blur reaches, so the
/// patch's edge never shows inside the notch.
const GLASS_PATCH_MARGIN: i32 = 7;
/// Smart hide: after a window comes under the notch (or you switch to it),
/// the notch stays out of the way this long; it also stays away until this
/// long after the mouse leaves the area around it.
const SMART_SETTLE: f64 = 2.5;
const SMART_NEAR: f64 = 2.0;
/// Self-test: seconds since the last input, or -1 for the real value (build
/// servers have had no mouse movement for ages, so the mascot would doze).
pub static IDLE_OVERRIDE: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(-1);

/// Self-test: behave as if no full-screen app were running (the tests run
/// on the user's PC, maybe while a game is open).
pub static FORCE_VISIBLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// "Is a maximized window under the notch?" override: 0 = ask Windows,
/// 1 = yes, 2 = no. Self-test and docs renders use 2 (his PC may have a
/// maximized window open) and 1 for the make-room checks.
pub static MAXIMIZED_OVERRIDE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// `frame_interval` value meaning "draw on every display refresh".
pub const VSYNC: u32 = 1;

/// How the main loop should schedule the next frame.
pub enum Pacing {
    /// Nothing is moving: sleep until a message arrives.
    Idle,
    /// A transition is running: draw, then wait for the next display refresh.
    Refresh,
    /// A slow ambient animation: draw after this many seconds.
    After(f64),
}

/// Sleep until just after the display's next refresh, so transitions draw
/// once per refresh at any monitor rate. The refresh phase and period come
/// from the compositor's timing info. (Neither DwmFlush nor DXGI's
/// WaitForVBlank is used: measured on a 144 Hz desktop, DwmFlush waited
/// ~250 ms whenever nothing else on screen changed, and WaitForVBlank
/// returned at once.) Falls back to ~120 fps if the timing isn't available.
pub fn wait_refresh(_hwnd: HWND) {
    use windows::Win32::Graphics::Dwm::{DwmGetCompositionTimingInfo, DWM_TIMING_INFO};
    use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
    let mut secs = 1.0 / 120.0;
    unsafe {
        let mut ti = DWM_TIMING_INFO { cbSize: std::mem::size_of::<DWM_TIMING_INFO>() as u32, ..Default::default() };
        let (mut now, mut freq) = (0i64, 0i64);
        if DwmGetCompositionTimingInfo(HWND::default(), &mut ti).is_ok() && QueryPerformanceCounter(&mut now).is_ok() && QueryPerformanceFrequency(&mut freq).is_ok() {
            let (period, vblank) = (ti.qpcRefreshPeriod as i64, ti.qpcVBlank as i64);
            // Sane refresh rates only (24..500 Hz).
            if freq > 0 && period > freq / 500 && period < freq / 24 {
                let next = vblank + ((now - vblank).div_euclid(period) + 1) * period;
                secs = (next - now) as f64 / freq as f64 + 0.0005;
            }
        }
    }
    std::thread::sleep(std::time::Duration::from_secs_f64(secs));
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Collapsed,
    Toast,
    Expanded,
}

pub enum Deferred {
    TrayMenu,
    Quit,
    /// Settings > General: save settings and data to a file / load them.
    Export,
    Import,
    /// Files dropped on Share: Windows' Share window.
    Share(Vec<String>),
    /// Launcher: pick apps and files (false) or folders (true) to pin.
    PickToPin(bool),
    /// Calendar: pick an .ics file.
    PickIcs,
    /// Start a fresh Nookly (after an import) and quit this one.
    Restart,
    DragOut(Vec<String>),
}

/// Liquid Glass: what's behind the whole window, a quarter size, blurred.
struct GlassRaw {
    w: u32,
    h: u32,
    px: Vec<u8>,
    /// When it was grabbed, where the window was then, and a counter that
    /// tells the cropped bitmap it needs remaking.
    at: f64,
    origin: (i32, i32),
    serial: u32,
}

pub struct App {
    pub hwnd: HWND,
    gfx: Gfx,
    pub modules: Vec<Box<dyn Module>>,
    pub settings: Settings,
    hits: Vec<Hit>,
    tab: usize,
    expanded: bool,
    hover: bool,
    /// When the mouse entered the window (for the per-style hover delay).
    hover_since: f64,
    last_leave: f64,
    mouse: (f32, f32),
    press: Option<(Hit, (f32, f32))>,
    pub drag_files: bool,
    pub dragging_out: bool,
    leave_at: Option<f64>,
    keyboard: Option<usize>,
    queue: VecDeque<Toast>,
    toast: Option<(Toast, f64)>,
    cur: (f32, f32, f32),
    /// Spring velocity of `cur`.
    vel: (f32, f32, f32),
    /// Tab (and settings view) drawn last frame, and when it last changed,
    /// for the cross-fade when switching.
    shown: (usize, bool),
    switch_t: f64,
    /// The Settings window while it is open (see settings.rs).
    swin: Option<settings::SettingsWin>,
    /// The panel shown in the Settings window (Widgets) that is typing.
    sw_keyboard: Option<usize>,
    /// "Start with Windows" (registry), read when Settings opens.
    startup_cache: bool,
    /// When "Check for updates" was clicked in Settings.
    updates_clicked: f64,
    /// Last header layout: (title shown, nothing overlapped). For the self-test.
    header_fit: (bool, bool),
    /// What was showing before the current tab switch, and which way it went.
    prev_shown: (usize, bool),
    switch_dir: f32,
    /// Tab highlight pill offset in the tab bar, and when it was last drawn.
    pill_off: f32,
    pill_last: f64,
    /// Mouse wheel over the open notch: amount not yet turned into a tab
    /// switch, and when the last switch happened.
    wheel_acc: f32,
    wheel_t: f64,
    wheel_last: f64,
    /// The tab bar's rect in the last drawn frame, and one tab's width.
    tab_bar: Rect,
    tab_slot: f32,
    /// Where the closed notch drew the mascot last (for the self-test).
    mascot_rect: Option<Rect>,
    /// What the closed notch shows now (from the Widgets tab's order).
    notch_shown: Option<Shown>,
    /// Each widget's last value and when it changed ("On change").
    widget_seen: std::collections::HashMap<&'static str, (String, f64)>,
    /// An alert set to Small: shown inside the closed notch until then.
    small_alert: Option<(Widget, f64)>,
    /// A drag inside a panel (Widgets tab order list): that module.
    inner_drag: Option<usize>,
    /// Welcome tour step while it is showing (first run, or from the tray).
    onboarding: Option<u8>,
    /// The current toast's picture as a bitmap: (image address, gfx generation).
    toast_bmp: Option<(usize, u32, windows::Win32::Graphics::Direct2D::ID2D1Bitmap)>,
    /// Current transition's (start, last frame, frames, worst gap) for the
    /// debug log.
    perf: Option<(f64, f64, u32, f64)>,
    start: Instant,
    /// Virtual clock for the scripted demo video (`--render-demo`).
    virt: Option<f64>,
    last_frame: f64,
    frame_interval: u32,
    scale: f32,
    origin: (i32, i32),
    /// Centre of the notch along its edge, in canvas DIPs.
    anchor: f32,
    /// Right-button press point (screen px) and whether it became a drag.
    rdrag: Option<POINT>,
    dragging: bool,
    hidden: bool,
    mood: Mood,
    mood_since: f64,
    last_activity: f64,
    /// Set after clicking a toast button: hovering must not reopen the panel
    /// until the mouse has left the notch once.
    hover_block: bool,
    /// A maximized window is under the notch (checked every second).
    covered: bool,
    /// Smart hide: when the window under the notch came (or you switched to
    /// one), until when the mouse counts as near, a window being dragged,
    /// and when the windows were last checked.
    covered_since: f64,
    near_until: f64,
    moving: bool,
    /// The window being dragged (it only counts while it's over the notch).
    moving_hwnd: Option<isize>,
    under_at: f64,
    was_yielding: bool,
    /// The window is click-through right now (WS_EX_TRANSPARENT).
    passthrough: bool,
    /// While click-through: when the cursor reached the hover zone.
    poll_since: Option<f64>,
    /// Liquid Glass: the blurred backdrop (bitmap, gfx generation, when),
    /// whether it is bright (light glass), and whether the notch is kept
    /// out of screen captures.
    /// (The notch's part of the grab, bent at the edges: bitmap, gfx
    /// generation, what it was made from, where it's drawn.)
    backdrop: Option<(windows::Win32::Graphics::Direct2D::ID2D1Bitmap, u32, u64, Rect)>,
    /// The glass grab of the whole window area (see update_backdrop).
    glass_raw: Option<GlassRaw>,
    /// What's behind the notch changed (a window switched, moved, opened or
    /// closed, or the notch is opening): grab it again on the next frame.
    glass_dirty: bool,
    /// The notch was open (or showing a toast) last frame.
    glass_was_open: bool,
    glass_light: bool,
    /// The notch's shape last drawn over glass (window DIPs) while at rest,
    /// the last look at the ring around it, and whether the probe runs.
    glass_shape: Option<Rect>,
    glass_probe: u64,
    glass_probing: bool,
    /// What's behind is moving: the ring is checked fast until this time.
    glass_live_until: f64,
    glass_fast: bool,
    /// The next grab must cover the whole window (the notch is opening).
    glass_full: bool,
    /// Only the notch's own part was grabbed since the last whole grab (the
    /// rest may be old), and whether the notch was at rest last frame.
    glass_patched: bool,
    glass_was_rest: bool,
    excluded: bool,
    /// Hit under the mouse with a tooltip: (module, id, since, suppressed by a click).
    tip: Option<(usize, u32, f64, bool)>,
    pub deferred: Vec<Deferred>,
    /// The mascot on the Windows taskbar (Settings > Mascot > Taskbar).
    pet: Option<crate::taskbar::Pet>,
    /// Screen x of the taskbar mascot's centre while the notch lives above
    /// it (taskbar mode), else None.
    dock_x: Option<i32>,
    /// The ⋯ page (panels that don't fit in the tab bar) is open.
    more_open: bool,
    /// Recording a keyboard shortcut (index into features::HOTKEYS).
    recording: Option<usize>,
    /// Shortcuts Windows refused (another app has them).
    hotkey_fail: [bool; 4],
    /// An import waiting for "Replace" (the files in it).
    import_pending: Option<Vec<(String, String)>>,
    /// Hovered feature on the Features page (its description shows).
    feature_hover: Option<usize>,
    /// Audio meter of the speakers (for the album glow) and when it was
    /// opened; the smoothed level.
    meter: Option<(windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation, f64)>,
    beat: f32,
    /// The app in front (pid, exe) and the last one that wasn't Nookly.
    front_cache: (u32, String),
    last_front: String,
    /// Last plain click on the closed notch (for double-click).
    last_notch_click: f64,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// Handle of the single-instance mutex (also the installer's AppMutex).
pub static SINGLE_INSTANCE: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Release the single-instance mutex early, so an installer started by the
/// updater doesn't see Nookly as still running while we shut down.
pub fn release_single_instance() {
    let h = SINGLE_INSTANCE.swap(0, std::sync::atomic::Ordering::SeqCst);
    if h != 0 {
        unsafe {
            let _ = windows::Win32::System::Threading::ReleaseMutex(HANDLE(h as *mut _));
            let _ = CloseHandle(HANDLE(h as *mut _));
        }
    }
}

/// Leak the app at exit: dropping D2D/COM/WinRT objects from a thread-local
/// destructor during process teardown is unsafe and corrupts the exit code.
pub fn shutdown() {
    APP.with(|a| {
        if let Ok(mut g) = a.try_borrow_mut() {
            std::mem::forget(g.take());
        }
    });
}

thread_local! {
    static OPEN_LATER: RefCell<Vec<String>> = RefCell::new(Vec::new());
}

/// Open a file, folder or link once the current message is handled. The
/// shell keeps Windows messages flowing while it starts things; inside a
/// click Nookly is busy and would miss them (a lost "mouse left" kept the
/// notch open for good).
/// The notch window, for background threads that need to wake it.
pub static MAIN_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Wake the app from a background thread (it ticks right away).
pub fn wake() {
    let h = MAIN_HWND.load(std::sync::atomic::Ordering::Relaxed);
    if h != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(h as *mut _)), WM_APP_WAKE, WPARAM(0), LPARAM(0));
        }
    }
}

/// A brightness change is running on a worker thread.
static BRIGHT_BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Its result, for the overlay: Some(Some(level)) set, Some(None) failed.
static BRIGHT_RESULT: std::sync::Mutex<Option<Option<u32>>> = std::sync::Mutex::new(None);

pub fn open_later(target: impl Into<String>) {
    OPEN_LATER.with(|q| q.borrow_mut().push(target.into()));
}

fn run_open_later() {
    let list = OPEN_LATER.with(|q| std::mem::take(&mut *q.borrow_mut()));
    for t in list {
        let h = HSTRING::from(t.as_str());
        unsafe {
            ShellExecuteW(None, w!("open"), &h, None, None, SW_SHOWNORMAL);
        }
    }
}

/// Run `f` against the app if it exists and is not already borrowed.
pub fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

/// Append a line to debug.log in the data folder when NOOKLY_DEBUG=1.
pub fn dlog(msg: impl AsRef<str>) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ON.get_or_init(|| std::env::var("NOOKLY_DEBUG").map_or(false, |v| v == "1")) {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(crate::store::dir().join("debug.log")) {
            let _ = writeln!(f, "{:.3} {}", crate::time::unix_now(), msg.as_ref());
        }
    }
}

fn lerp(a: f32, b: f32, k: f32) -> f32 {
    a + (b - a) * k
}

impl App {
    pub fn create(visible: bool) -> Result<()> {
        unsafe {
            let hinst = GetModuleHandleW(None)?;
            // Self-test and docs renders use their own class, so agent hooks
            // (and the uninstaller) only ever find the real notch.
            let class = if crate::modules::agents::ISOLATED.load(std::sync::atomic::Ordering::Relaxed) { w!("NooklyNotchTest") } else { w!("NooklyNotch") };
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(wndproc),
                hInstance: hinst.into(),
                lpszClassName: class,
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                ..Default::default()
            };
            RegisterClassExW(&wc);
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class,
                w!("Nookly"),
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

            let gfx = Gfx::new(10, 10)?;
            let modules = modules::all(hwnd);
            let tab = modules.iter().position(|m| m.has_tab()).unwrap_or(0);
            let mut app = App {
                hwnd,
                gfx,
                modules,
                settings: Settings::load(),
                hits: Vec::new(),
                tab,
                expanded: false,
                hover: false,
                hover_since: 0.0,
                last_leave: -1.0,
                mouse: (-1e4, -1e4),
                press: None,
                drag_files: false,
                dragging_out: false,
                leave_at: None,
                keyboard: None,
                queue: VecDeque::new(),
                toast: None,
                cur: (200.0, 32.0, 12.0),
                vel: (0.0, 0.0, 0.0),
                shown: (tab, false),
                switch_t: -10.0,
                swin: None,
                sw_keyboard: None,
                startup_cache: false,
                updates_clicked: -10.0,
                header_fit: (true, true),
                prev_shown: (tab, false),
                switch_dir: 1.0,
                pill_off: 0.0,
                pill_last: -10.0,
                wheel_acc: 0.0,
                wheel_t: -10.0,
                wheel_last: -10.0,
                tab_bar: Rect::default(),
                tab_slot: TAB_H + TAB_GAP,
                mascot_rect: None,
                notch_shown: None,
                widget_seen: std::collections::HashMap::new(),
                small_alert: None,
                inner_drag: None,
                onboarding: None,
                toast_bmp: None,
                perf: None,
                start: Instant::now(),
                virt: None,
                last_frame: 0.0,
                frame_interval: 0,
                scale: 1.0,
                origin: (0, 0),
                anchor: CANVAS_W / 2.0,
                rdrag: None,
                dragging: false,
                hidden: false,
                mood: Mood::Clock,
                mood_since: 0.0,
                last_activity: 0.0,
                hover_block: false,
                covered: false,
                covered_since: -10.0,
                near_until: -10.0,
                moving: false,
                moving_hwnd: None,
                under_at: -10.0,
                was_yielding: false,
                passthrough: false,
                poll_since: None,
                backdrop: None,
                glass_raw: None,
                glass_dirty: false,
                glass_was_open: false,
                glass_light: false,
                glass_shape: None,
                glass_probe: 0,
                glass_probing: false,
                glass_live_until: 0.0,
                glass_fast: false,
                glass_full: false,
                glass_patched: false,
                glass_was_rest: false,
                excluded: false,
                tip: None,
                deferred: Vec::new(),
                pet: None,
                dock_x: None,
                more_open: false,
                recording: None,
                hotkey_fail: [false; 4],
                import_pending: None,
                feature_hover: None,
                front_cache: (0, String::new()),
                meter: None,
                beat: 0.0,
                last_front: String::new(),
                last_notch_click: 0.0,
            };
            app.place();
            sound::set(app.settings.sound);
            let seen = app.settings.extra.get("tour_version").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
            if app.settings.onboarded && seen < TOUR_VERSION && visible {
                // Updated, and the tour has news: show it once more.
                app.start_onboarding();
            } else if !app.settings.onboarded {
                // A brand-new install starts small; the tour offers the rest.
                if visible && crate::store::read("settings.txt").trim().is_empty() {
                    crate::features::apply_preset(&mut app.settings, 0);
                    app.settings.save();
                }
                app.start_onboarding();
            }
            MAIN_HWND.store(hwnd.0 as isize, std::sync::atomic::Ordering::Relaxed);
            if visible {
                app.hotkey_fail = crate::features::register(hwnd, &app.settings);
                use windows::Win32::UI::Accessibility::SetWinEventHook;
                for (lo, hi) in [(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND), (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND)] {
                    let _ = SetWinEventHook(lo, hi, None, Some(on_foreground), 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS);
                }
                // A window starts or stops being dragged or resized.
                let _ = SetWinEventHook(EVENT_SYSTEM_MOVESIZESTART, EVENT_SYSTEM_MOVESIZEEND, None, Some(on_movesize), 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS);
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                SetTimer(Some(hwnd), TIMER_TICK, 1000, None);
                tray::add(hwnd);
                dragdrop::register(hwnd);
            }
            app.tick();
            app.request_frame();
            APP.with(|a| *a.borrow_mut() = Some(app));
            with_app(|a| a.frame());
        }
        Ok(())
    }

    fn t(&self) -> f64 {
        self.virt.unwrap_or_else(|| self.start.elapsed().as_secs_f64())
    }

    /// Size and position the window on the chosen edge/monitor. The window is
    /// clamped inside the monitor's work area; the notch itself can sit
    /// anywhere along the edge (`anchor` is its centre inside the canvas).
    fn place(&mut self) {
        unsafe {
            // Docked to the taskbar mascot: the taskbar's screen, bottom edge.
            let taskbar = FindWindowW(w!("Shell_TrayWnd"), None).ok();
            let mon = match (self.dock_x, taskbar, self.settings.monitor) {
                (Some(_), Some(tb), _) => MonitorFromWindow(tb, MONITOR_DEFAULTTOPRIMARY),
                (_, _, Some((x, y))) => MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST),
                _ => MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY),
            };
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(mon, &mut mi);
            let (mut dx, mut dy) = (96u32, 96u32);
            let _ = GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
            self.scale = dx as f32 / 96.0 * ui_size(&self.settings);
            let pw = (CANVAS_W * self.scale).ceil() as i32;
            let ph = (CANVAS_H * self.scale).ceil() as i32;
            let r = mi.rcWork;
            let pos = self.settings.pos.clamp(0.0, 1.0);
            let (x, y, anchor_px) = match self.edge() {
                Edge::Top | Edge::Bottom => {
                    let c = self.dock_x.unwrap_or(r.left + ((r.right - r.left) as f32 * pos) as i32);
                    let x = (c - pw / 2).clamp(r.left, (r.right - pw).max(r.left));
                    let y = if self.edge() == Edge::Top { r.top } else { r.bottom - ph };
                    (x, y, c - x)
                }
                Edge::Left | Edge::Right => {
                    let c = r.top + ((r.bottom - r.top) as f32 * pos) as i32;
                    let y = (c - ph / 2).clamp(r.top, (r.bottom - ph).max(r.top));
                    let x = if self.edge() == Edge::Left { r.left } else { r.right - pw };
                    (x, y, c - y)
                }
            };
            self.origin = (x, y);
            self.anchor = anchor_px as f32 / self.scale;
            let _ = self.gfx.resize(pw, ph);
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                self.origin.0,
                self.origin.1,
                pw,
                ph,
                SWP_NOACTIVATE,
            );
        }
    }

    // ------------------------------------------------------------- ctx glue

    /// Call into module `i` with a fresh `Ctx`, then apply what it asked for.
    pub fn call<R>(&mut self, i: usize, f: impl FnOnce(&mut dyn Module, &mut Ctx) -> R) -> R {
        let mut toasts = Vec::new();
        let mut open_tab = None;
        let mut keyboard = None;
        let mut close = false;
        let r = {
            let mut ctx = Ctx {
                now: Now::get(),
                hwnd: self.hwnd,
                hidden: self.hidden,
                settings: &mut self.settings,
                module: i,
                toasts: &mut toasts,
                open_tab: &mut open_tab,
                keyboard: &mut keyboard,
                close: &mut close,
            };
            f(self.modules[i].as_mut(), &mut ctx)
        };
        let on = self.enabled(i);
        for t in toasts {
            if on {
                self.push_toast(t);
            }
        }
        if let Some(m) = open_tab.filter(|_| on) {
            self.tab = m;
            self.expanded = true;
            self.leave_at = Some(self.t() + 2.5);
        }
        if crate::modules::calendar::PICK_ICS.swap(false, std::sync::atomic::Ordering::Relaxed) {
            self.deferred.push(Deferred::PickIcs);
        }
        // A module asked for the file picker (Launcher): right away.
        match crate::modules::launcher::PICK.swap(0, std::sync::atomic::Ordering::Relaxed) {
            1 => self.deferred.push(Deferred::PickToPin(false)),
            2 => self.deferred.push(Deferred::PickToPin(true)),
            _ => {}
        }
        match keyboard {
            // The Widgets panel inside the Settings window types there.
            Some(true) if self.sw_hosts(i) => self.sw_keyboard = Some(i),
            Some(false) if self.sw_keyboard == Some(i) => self.sw_keyboard = None,
            Some(true) => self.grab_keyboard(i),
            Some(false) => self.release_keyboard(),
            None => {}
        }
        if close {
            self.release_keyboard();
            self.expanded = false;
            self.leave_at = None;
            // The cursor is still on the button: don't reopen under it
            // (Screenshot and Pick from screen need the notch out of the way).
            self.hover_block = true;
        }
        self.request_frame();
        r
    }

    /// Queue a toast. Its chime plays when it actually appears on screen
    /// (see `frame`), never while the notch is hidden behind a full-screen app.
    fn push_toast(&mut self, t: Toast) {
        match self.settings.alert_mode(t.key) {
            AlertMode::Off => return,
            // Small: a few seconds inside the closed notch (not for alerts
            // with buttons).
            AlertMode::Small if t.actions.is_empty() => {
                let until = self.t() + (t.secs as f64).clamp(1.5, 3.0);
                self.small_alert = Some((alert_widget(&t), until));
                self.request_frame();
                return;
            }
            _ => {}
        }
        let now = self.t();
        let hidden = self.hidden;
        if let Some((cur, end)) = &mut self.toast {
            if cur.key == t.key && !hidden {
                if t.chime {
                    sound::chime();
                }
                *end = now + t.secs as f64;
                *cur = t;
                return;
            }
        }
        if let Some(q) = self.queue.iter_mut().find(|q| q.key == t.key) {
            *q = t;
        } else {
            self.queue.push_back(t);
        }
    }

    fn grab_keyboard(&mut self, i: usize) {
        self.keyboard = Some(i);
        self.expanded = true;
        unsafe {
            let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, ex & !(WS_EX_NOACTIVATE.0 as isize));
            let _ = SetForegroundWindow(self.hwnd);
            let _ = SetFocus(Some(self.hwnd));
        }
    }

    fn release_keyboard(&mut self) {
        if self.keyboard.take().is_some() {
            unsafe {
                let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE.0 as isize);
            }
            if !self.hover {
                self.leave_at = Some(self.t() + 0.4);
            }
        }
    }

    // ------------------------------------------------------------ periodic

    pub fn tick(&mut self) {
        // A lost "mouse left" message must never keep the notch open: check
        // where the mouse really is.
        let testing = crate::modules::agents::ISOLATED.load(std::sync::atomic::Ordering::Relaxed);
        if self.hover && !testing && self.inner_drag.is_none() && self.rdrag.is_none() && !self.drag_files {
            let mut p = POINT::default();
            if unsafe { GetCursorPos(&mut p) }.is_ok() && unsafe { WindowFromPoint(p) } != self.hwnd {
                self.on_mouse_leave();
            }
        }
        match crate::modules::launcher::PICK.swap(0, std::sync::atomic::Ordering::Relaxed) {
            1 => self.deferred.push(Deferred::PickToPin(false)),
            2 => self.deferred.push(Deferred::PickToPin(true)),
            _ => {}
        }
        if let Some(res) = BRIGHT_RESULT.lock().unwrap().take() {
            let t = match res {
                Some(v) => Toast::new("brightness", "\u{E706}", col::TEXT, "Brightness", "Screen").right(format!("{v}%")).progress(v as f32 / 100.0),
                None => Toast::new("brightness", "\u{E706}", col::DIM, "Brightness", "This screen doesn't let apps change it"),
            };
            self.push_toast(t.secs(1.5));
        }
        let now = Now::get();
        if now.sec == 0 {
            if let Some(p) = crate::features::auto_profile(&self.settings, crate::time::weekday(now.year as i32, now.month as u32, now.day as u32), now.hour as u32) {
                crate::features::switch_profile(&mut self.settings, p);
                self.settings.save();
            }
        }
        // A panel switched off in Settings > Features does no work.
        // With the notch switched off (Settings > Notch) nothing runs.
        let on = self.notch_on();
        for i in 0..self.modules.len() {
            if on && self.enabled(i) {
                self.call(i, |m, c| m.tick(c));
            } else {
                self.call(i, |m, c| m.tick_off(c));
                if on {
                    self.call(i, |m, c| m.background(c));
                }
            }
        }
        let tabs = self.tab_list();
        if !tabs.contains(&self.tab) {
            self.tab = tabs.first().copied().unwrap_or(0);
        }
        self.check_fullscreen();
        self.sync_glass_probe();
        self.keep_on_top();
        // Once a minute while closed and untouched, give back memory that
        // drawing and grabbing used and no longer need.
        if self.mode() == Mode::Collapsed && !self.hover && now.sec == 30 {
            trim_memory();
        }
        crate::time::H12.store(self.settings.clock12, std::sync::atomic::Ordering::Relaxed);
        crate::time::SECONDS.store(self.settings.seconds, std::sync::atomic::Ordering::Relaxed);
        self.update_make_room();
        self.update_mood();
        self.sync_pet();
        self.request_frame();
    }

    // ---------------------------------------------------- taskbar mascot

    fn pet_wanted(&self) -> bool {
        self.settings.extra.get("taskbar_pet").map_or(false, |v| v == "1") && self.notch_on() && !crate::modules::agents::ISOLATED.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Create or remove the taskbar mascot to match the setting, and hide
    /// it while a full-screen app runs.
    pub fn sync_pet(&mut self) {
        let want = self.pet_wanted();
        if want && self.pet.is_none() {
            self.pet = crate::taskbar::Pet::create();
            self.pet_place();
        } else if !want {
            if let Some(p) = self.pet.take() {
                p.destroy();
            }
            if self.dock_x.take().is_some() {
                self.place();
                self.request_frame();
            }
        }
    }

    fn pet_place(&mut self) {
        let hidden = self.hidden;
        let mut dock = None;
        if let Some(p) = self.pet.as_mut() {
            let place = if hidden { None } else { crate::taskbar::locate() };
            p.set_place(place);
            p.raise();
            // The notch moves above the mascot (if the mascot can't show,
            // e.g. an auto-hidden taskbar, the notch goes back to its place).
            dock = place.map(|pl| {
                let half = (crate::taskbar::MASCOT_W * pl.scale / 2.0) as i32;
                if pl.hint_left { pl.x + pl.w - half } else { pl.x + half }
            });
        }
        if dock != self.dock_x && !(hidden && self.dock_x.is_some()) {
            self.dock_x = dock;
            self.place();
            self.request_frame();
        }
    }

    /// Turn taskbar mode off (the user picked a place for the notch).
    fn undock(&mut self) {
        if self.pet_wanted() {
            self.settings.extra.insert("taskbar_pet".into(), "0".into());
            self.sync_pet();
        }
    }

    pub fn pet_raise(&mut self) {
        if let Some(p) = &self.pet {
            p.raise();
        }
    }

    fn pet_frame(&mut self) {
        let Some(mut pet) = self.pet.take() else { return };
        if let (Some(pl), true) = (pet.place, pet.shown) {
            let t = self.t();
            let s = pl.scale;
            let (w, h) = (pl.w as f32 / s, pl.h as f32 / s);
            let mbox = if pl.hint_left {
                Rect::new(w - crate::taskbar::MASCOT_W, 3.0, crate::taskbar::MASCOT_W, h - 6.0)
            } else {
                Rect::new(0.0, 3.0, crate::taskbar::MASCOT_W, h - 6.0)
            };
            if self.dock_x.is_some() {
                if pet.hover && !self.expanded && t - pet.hover_since >= 0.25 && self.can_hover_open() {
                    self.open_from_hover();
                }
                if pet.hover && self.mode() == Mode::Expanded && !self.hover {
                    self.leave_at = Some(t + 0.6);
                }
            }
            let idle = Self::system_idle_secs();
            let mood = if pet.hover {
                Mood::Wave
            } else if idle > 120.0 {
                Mood::Doze
            } else {
                Mood::Eyes
            };
            let mut hits = Vec::new();
            pet.gfx.begin(s);
            {
                let mut ui = Ui { g: &mut pet.gfx, hits: &mut hits, module: usize::MAX - 9, mouse: (-1e4, -1e4), t };
                // Nearly invisible, but it makes the whole box clickable.
                ui.fill_rr(mbox, 8.0, col::NOTCH.with_a(0.012));
                if pet.hover {
                    ui.fill_rr(mbox, 8.0, col::TEXT.with_a(0.08));
                }
                draw_mascot(&mut ui, mbox.inset_xy(2.0, 2.0), self.settings.mascot, mood, t, (pl.x, pl.y), s);
                if pet.hover && t - pet.hover_since > 0.5 && self.dock_x.is_none() {
                    let hint = "Click: open  \u{00B7}  Right-click: settings";
                    let tw = ui.measure(hint, 11.5, Weight::Regular) + 20.0;
                    let hr = if pl.hint_left {
                        Rect::new(mbox.x - tw - 6.0, h / 2.0 - 12.0, tw, 24.0)
                    } else {
                        Rect::new(mbox.r() + 6.0, h / 2.0 - 12.0, tw, 24.0)
                    };
                    ui.fill_rr(hr, 12.0, col::NOTCH.with_a(0.92));
                    ui.text(hint, hr, 11.5, col::TEXT, Weight::Regular, Align::Center);
                }
            }
            pet.gfx.end_and_present(pet.hwnd, pl.x, pl.y);
        }
        self.pet = Some(pet);
    }

    /// Messages for the taskbar mascot's window.
    pub fn pet_msg(&mut self, hwnd: HWND, msg: u32, wp: WPARAM, _lp: LPARAM) -> Option<LRESULT> {
        if self.pet.as_ref().map_or(true, |p| p.hwnd != hwnd) {
            return None;
        }
        match msg {
            WM_TIMER => {
                match wp.0 {
                    crate::taskbar::TIMER_FRAME => self.pet_frame(),
                    crate::taskbar::TIMER_PLACE => self.pet_place(),
                    _ => {}
                }
                Some(LRESULT(0))
            }
            WM_MOUSEMOVE => {
                let t = self.t();
                if let Some(p) = self.pet.as_mut() {
                    if !p.hover {
                        p.hover = true;
                        p.hover_since = t;
                        unsafe {
                            let mut tme = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                            let _ = TrackMouseEvent(&mut tme);
                        }
                    }
                }
                Some(LRESULT(0))
            }
            WM_MOUSELEAVE => {
                if let Some(p) = self.pet.as_mut() {
                    p.hover = false;
                }
                // Left the mascot without going up into the panel.
                if self.dock_x.is_some() && !self.hover && self.mode() == Mode::Expanded && self.keyboard.is_none() {
                    self.leave_at = Some(self.t() + 0.5);
                }
                self.hover_block = false;
                Some(LRESULT(0))
            }
            WM_LBUTTONDOWN | WM_RBUTTONDOWN => Some(LRESULT(0)),
            WM_LBUTTONUP => {
                // Open the notch; the mouse is down here, so give it time.
                self.expanded = true;
                self.raise();
                self.leave_at = Some(self.t() + 4.0);
                self.request_frame();
                Some(LRESULT(0))
            }
            WM_RBUTTONUP => {
                self.open_settings();
                self.raise();
                Some(LRESULT(0))
            }
            _ => None,
        }
    }

    // ------------------------------------------------------- make room

    /// The edge the notch uses right now (the bottom while it lives above
    /// the taskbar mascot).
    fn edge(&self) -> Edge {
        if self.dock_x.is_some() {
            Edge::Bottom
        } else {
            self.settings.edge
        }
    }

    /// The look the closed notch uses right now: a thin bar while it is
    /// making room for a maximized window (Full and Compact only); nothing
    /// at all while the taskbar mascot stands in for it.
    fn look(&self) -> NotchStyle {
        if self.dock_x.is_some() {
            return NotchStyle::Hidden;
        }
        match self.settings.style {
            NotchStyle::Full | NotchStyle::Slim | NotchStyle::Compact if self.yielding() => NotchStyle::Pill,
            s => s,
        }
    }

    /// The open panel uses Slim's smaller size (not for Settings or the
    /// welcome tour, which keep theirs).
    fn slim_panel(&self) -> bool {
        self.settings.style == NotchStyle::Slim && self.onboarding.is_none()
    }

    /// Live activities (id, module, priority now) and widgets (with the
    /// module they come from) of the panels that are on.
    fn notch_items(&mut self) -> (Vec<(&'static str, usize, Option<u8>)>, Vec<(Widget, usize)>) {
        let mut live = Vec::new();
        let mut widgets = Vec::new();
        for i in 0..self.modules.len() {
            if !self.enabled(i) {
                continue;
            }
            let id = self.modules[i].live_id();
            if !id.is_empty() {
                live.push((id, i, self.modules[i].live()));
            }
            widgets.extend(self.modules[i].widgets().into_iter().map(|w| (w, i)));
        }
        (live, widgets)
    }

    /// The rows the notch reads: your order (Custom order), or Nookly's:
    /// what is running, most urgent first (an agent that needs you goes
    /// above music), then the widgets two per row, in the order switched on.
    fn effective_rows(&self, live: &[(&'static str, usize, Option<u8>)]) -> Vec<NotchRow> {
        let l = &self.settings.layout;
        if l.custom {
            return l.rows.clone();
        }
        auto_rows(l, live)
    }

    /// Work out what the closed notch shows.
    fn refresh_shown(&mut self) {
        let (live, widgets) = self.notch_items();
        let now = self.t();
        // When each widget's value last changed ("On change").
        for (w, _) in &widgets {
            match self.widget_seen.get_mut(w.id) {
                Some(v) if v.0 != w.value => {
                    v.0 = w.value.clone();
                    v.1 = now;
                }
                Some(_) => {}
                None => {
                    self.widget_seen.insert(w.id, (w.value.clone(), f64::NEG_INFINITY));
                }
            }
        }
        let rows = self.effective_rows(&live);
        let mut shown = {
            let layout = &self.settings.layout;
            let seen = &self.widget_seen;
            let active = |id: &str| {
                live.iter().any(|(l, _, p)| *l == id && p.is_some())
                    || widgets.iter().any(|(w, _)| {
                        w.id == id
                            && match layout.when_of(id) {
                                ShowWhen::Always => true,
                                ShowWhen::Matters => !w.quiet,
                                ShowWhen::Change => seen.get(w.id).map_or(false, |v| now - v.1 < 3.0),
                            }
                    })
            };
            let item = |id: &str| -> Option<ShownItem> {
                if id.is_empty() {
                    return None;
                }
                match live.iter().find(|(l, _, _)| *l == id) {
                    Some((_, i, _)) => Some(ShownItem::Live(*i)),
                    None => widgets.iter().find(|(w, _)| w.id == id).map(|(w, i)| ShownItem::W(w.clone(), *i)),
                }
            };
            pick_layout(&rows, &active).and_then(|(wide, l, r)| if wide { item(&l).map(Shown::Wide) } else { Some(Shown::Pair(item(&l), item(&r))) })
        };
        // A small alert takes the right side (or the whole notch).
        if let Some((w, until)) = &self.small_alert {
            if now < *until {
                let a = ShownItem::W(w.clone(), usize::MAX);
                shown = Some(match shown {
                    None => Shown::Wide(a),
                    Some(Shown::Wide(it)) => Shown::Pair(Some(it), Some(a)),
                    Some(Shown::Pair(l, r)) => Shown::Pair(l.or(r), Some(a)),
                });
            }
        }
        self.notch_shown = shown;
    }

    /// The panel of what the closed notch shows first (clicking opens it).
    fn shown_owner(&self) -> Option<usize> {
        let it = match self.notch_shown.as_ref()? {
            Shown::Wide(it) => it,
            Shown::Pair(l, r) => l.as_ref().or(r.as_ref())?,
        };
        let m = match it {
            ShownItem::Live(i) => *i,
            ShownItem::W(_, i) => *i,
        };
        (m < self.modules.len() && self.modules[m].has_tab() && self.enabled(m)).then_some(m)
    }

    /// Ids of what the closed notch shows now.
    fn showing_ids(&self) -> Vec<String> {
        let id = |it: &ShownItem| match it {
            ShownItem::Live(i) => self.modules[*i].live_id().to_string(),
            ShownItem::W(w, _) => w.id.to_string(),
        };
        match &self.notch_shown {
            Some(Shown::Wide(it)) => vec![id(it)],
            Some(Shown::Pair(l, r)) => l.iter().chain(r.iter()).map(id).collect(),
            None => Vec::new(),
        }
    }

    /// Everything the Widgets tab can put on the notch.
    fn notch_catalog(&mut self) -> Vec<CatalogItem> {
        let (live, widgets) = self.notch_items();
        let mut out: Vec<CatalogItem> = live
            .iter()
            .map(|(id, i, _)| {
                let m = &self.modules[*i];
                let (name, detail) = live_names(id);
                CatalogItem { id: id.to_string(), name: name.into(), icon: m.icon(), color: m.accent(), ring: None, detail: detail.into(), live: true, group: 0 }
            })
            .collect();
        out.extend(widgets.into_iter().map(|(w, i)| CatalogItem {
            id: w.id.to_string(),
            name: w.name.to_string(),
            icon: w.icon,
            color: w.color,
            ring: w.ring,
            detail: w.detail,
            live: false,
            // System widgets come from the Widgets tab itself.
            group: if self.modules[i].title() == "Widgets" || self.modules[i].title() == "System Monitor" { 1 } else { 2 },
        }));
        out
    }

    /// Draw what the closed notch shows into its two sides.
    fn draw_shown(&mut self, shown: &Shown, left: Rect, right: Rect, vertical: bool, compact: bool, t: f64) {
        let mouse = self.mouse;
        let one = |s: &Shown| match s {
            Shown::Wide(it) => Some(it.clone()),
            Shown::Pair(l, r) => l.clone().or_else(|| r.clone()),
        };
        // Wide, or (side edges) the first one only.
        let whole = match shown {
            Shown::Wide(_) => one(shown),
            Shown::Pair(..) if vertical => one(shown),
            _ => None,
        };
        if let Some(it) = whole {
            match it {
                ShownItem::Live(i) => {
                    let mut ui = Ui { g: &mut self.gfx, hits: &mut self.hits, module: i, mouse, t };
                    self.modules[i].draw_live(&mut ui, left, right);
                }
                ShownItem::W(w, _) => {
                    let mut ui = Ui { g: &mut self.gfx, hits: &mut self.hits, module: APP_HIT, mouse, t };
                    draw_widget_wide(&mut ui, &w, left, right, vertical, compact);
                }
            }
            return;
        }
        if let Shown::Pair(l, r) = shown {
            for (it, rect, side) in [(l, left, false), (r, right, true)] {
                if rect.w < 4.0 {
                    continue;
                }
                match it {
                    Some(ShownItem::Live(i)) => {
                        let mut ui = Ui { g: &mut self.gfx, hits: &mut self.hits, module: *i, mouse, t };
                        self.modules[*i].draw_live_side(&mut ui, rect, side);
                    }
                    Some(ShownItem::W(w, _)) => {
                        let mut ui = Ui { g: &mut self.gfx, hits: &mut self.hits, module: APP_HIT, mouse, t };
                        draw_widget_side(&mut ui, w, rect, side);
                    }
                    None => {}
                }
            }
        }
    }

    /// The theme to draw with (Liquid Glass picks light or dark glass from
    /// how bright the backdrop is), and keep the notch out of screen
    /// captures while Liquid Glass needs to see behind itself.
    fn apply_theme(&mut self) {
        let th = match self.settings.theme.as_str() {
            "glass" if self.glass_light => crate::gfx::GLASS_LIGHT,
            "custom" => {
                let (bg, text) = custom_colors(&self.settings);
                crate::gfx::custom_theme(bg, text)
            }
            k => crate::gfx::THEMES.iter().find(|t| t.key == k).copied().unwrap_or(crate::gfx::THEMES[0]),
        };
        self.gfx.theme = th;
        // Never left excluded from capture (screen recorders refuse to run
        // then); Liquid Glass hides the notch only while grabbing its
        // backdrop.
        if self.excluded {
            crate::glass::exclude_from_capture(self.hwnd, false);
            self.excluded = false;
        }
        if !th.glass {
            // Not glass: let go of the grab too.
            self.backdrop = None;
            self.glass_raw = None;
        }
    }

    /// The glass backdrop. One grab covers the whole window area around the
    /// notch, aligned to the screen, so whatever size or shape the notch has
    /// (opening, switching panels, a toast) it shows exactly what's behind
    /// that part, at once. The grab is taken when there is none or the
    /// window moved, then refreshed only at rest (every 1.5 s while frames
    /// are drawn), never while the notch moves; a resting closed notch draws
    /// no frames, so it costs nothing. Each frame just crops the notch's
    /// part and bends its edges (both tiny: a quarter of the pixels).
    /// Refreshes at rest grab only the notch's own part (and a margin) and
    /// patch it into the whole grab: a fraction of the work, so it can keep
    /// up with a page scrolling behind.
    fn update_backdrop(&mut self, notch: Rect, t: f64, dist: f32, rad: f32) {
        let (pw, ph) = (self.gfx.pw, self.gfx.ph);
        let (gw, gh) = ((pw / 4).max(2) as u32, (ph / 4).max(2) as u32);
        // The notch starts changing size (a panel switch) after only its own
        // part was refreshed: the rest of the grab may be old, so grab the
        // whole area once, or the new shape's sides would show old colours.
        let resized = dist > 0.0 && self.glass_was_rest && self.glass_patched;
        self.glass_was_rest = dist <= 0.0;
        let whole = match &self.glass_raw {
            None => true,
            Some(r) => self.glass_full || resized || r.origin != self.origin || (r.w, r.h) != (gw, gh),
        };
        let stale = whole || self.glass_dirty || self.glass_raw.as_ref().map_or(false, |r| dist <= 0.0 && t - r.at > 1.0);
        let testing = crate::modules::agents::ISOLATED.load(std::sync::atomic::Ordering::Relaxed);
        if stale && !whole && !testing {
            self.glass_dirty = false;
            if let Some(raw) = self.glass_raw.as_mut() {
                // The notch's part of the grab, with a margin, in grab pixels.
                let k = raw.w as f32 / (pw as f32 / self.scale);
                let m = GLASS_PATCH_MARGIN;
                let bx0 = ((notch.x * k).floor() as i32 - m).clamp(0, raw.w as i32);
                let by0 = ((notch.y * k).floor() as i32 - m).clamp(0, raw.h as i32);
                let bx1 = ((notch.r() * k).ceil() as i32 + m).clamp(0, raw.w as i32);
                let by1 = ((notch.b() * k).ceil() as i32 + m).clamp(0, raw.h as i32);
                let (ox, oy) = self.origin;
                let r = RECT { left: ox + bx0 * 4, top: oy + by0 * 4, right: ox + bx1 * 4, bottom: oy + by1 * 4 };
                let t0 = std::time::Instant::now();
                let grab = crate::glass::capture_behind(self.hwnd, r);
                dlog(format!("glass grab: notch part {}x{} px, {:.1} ms", r.right - r.left, r.bottom - r.top, t0.elapsed().as_secs_f32() * 1000.0));
                if let Some((w, h, px, _)) = grab {
                    let (w, h) = (w.min((bx1 - bx0) as u32) as usize, h.min((by1 - by0) as u32) as usize);
                    for y in 0..h {
                        let d = ((by0 as usize + y) * raw.w as usize + bx0 as usize) * 4;
                        let s = y * w * 4;
                        raw.px[d..d + w * 4].copy_from_slice(&px[s..s + w * 4]);
                    }
                    raw.at = t;
                    raw.serial = raw.serial.wrapping_add(1);
                    self.glass_patched = true;
                }
            }
        } else if stale {
            self.glass_dirty = false;
            self.glass_full = false;
            self.glass_patched = false;
            let testing = crate::modules::agents::ISOLATED.load(std::sync::atomic::Ordering::Relaxed);
            let data = if testing {
                // Renders and tests: a made-up background (light, dark, colour).
                let kind = std::env::var("NOOKLY_RENDER_BACKDROP").unwrap_or_else(|_| "color".into());
                // (or a scene image, for the docs: a path to a .raw file)
                if kind.ends_with(".raw") {
                    crate::glass::from_raw_file(&kind, gw, gh)
                } else {
                    Some(crate::glass::sample(&kind, gw, gh))
                }
            } else {
                let (ox, oy) = self.origin;
                crate::glass::capture_behind(self.hwnd, RECT { left: ox, top: oy, right: ox + pw, bottom: oy + ph })
            };
            if let Some((w, h, px, _)) = data {
                let serial = self.glass_raw.as_ref().map_or(0, |r| r.serial.wrapping_add(1));
                self.glass_raw = Some(GlassRaw { w, h, px, at: t, origin: self.origin, serial });
            }
        }
        let Some(raw) = &self.glass_raw else { return };
        // The notch's part of the grab, in its pixels.
        let k = raw.w as f32 / (pw as f32 / self.scale);
        let clampx = |v: f32| (v.max(0.0) as u32).min(raw.w);
        let clampy = |v: f32| (v.max(0.0) as u32).min(raw.h);
        let (x0, y0) = (clampx((notch.x * k).floor()), clampy((notch.y * k).floor()));
        let (x1, y1) = (clampx((notch.r() * k).ceil()), clampy((notch.b() * k).ceil()));
        if x1 <= x0 + 1 || y1 <= y0 + 1 {
            return;
        }
        let key = ((x0 as u64) << 48) ^ ((y0 as u64) << 36) ^ ((x1 as u64) << 24) ^ ((y1 as u64) << 12) ^ ((rad * k) as u64) ^ ((raw.serial as u64) << 56);
        if self.backdrop.as_ref().map_or(false, |b| b.1 == self.gfx.generation && b.2 == key) {
            return;
        }
        let (cw, ch) = (x1 - x0, y1 - y0);
        let mut px = Vec::with_capacity((cw * ch * 4) as usize);
        for y in y0..y1 {
            let s = ((y * raw.w + x0) * 4) as usize;
            px.extend_from_slice(&raw.px[s..s + (cw * 4) as usize]);
        }
        // Light glass over bright backgrounds, dark glass over dark ones,
        // from what's behind this part (a little hysteresis: no flicker).
        let lum = px.chunks_exact(4).map(|p| 0.0722 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.2126 * p[2] as f32).sum::<f32>() / (cw * ch) as f32 / 255.0;
        if lum > 0.62 {
            self.glass_light = true;
        } else if lum < 0.5 {
            self.glass_light = false;
        }
        // Refraction: the edges bend what's behind, like thick glass.
        // (not along the screen edge: the glass runs on past it).
        let e = self.edge();
        let open = [e == Edge::Left, e == Edge::Top, e == Edge::Right, e == Edge::Bottom];
        crate::glass::lens(&mut px, cw, ch, rad * k, open);
        let at = Rect::new(x0 as f32 / k, y0 as f32 / k, cw as f32 / k, ch as f32 / k);
        // Same size as last time: refill the bitmap rather than make a new one.
        if let Some(b) = self.backdrop.as_mut().filter(|b| b.1 == self.gfx.generation) {
            if crate::gfx::refill_bitmap(&b.0, cw, ch, &px) {
                b.2 = key;
                b.3 = at;
                return;
            }
        }
        if let Some(b) = self.gfx.make_bitmap(cw, ch, &px) {
            self.backdrop = Some((b, self.gfx.generation, key, at));
        }
    }

    /// Something changed behind the notch (a window came to the front,
    /// moved, opened or closed): with Liquid Glass, grab the background
    /// again now, and once more when the window has finished animating.
    /// Nothing runs between such events.
    fn glass_changed(&mut self) {
        if self.settings.theme != "glass" || self.hidden {
            return;
        }
        self.glass_dirty = true;
        unsafe {
            SetTimer(Some(self.hwnd), TIMER_GLASS, 350, None);
        }
        self.request_frame();
    }

    /// Run the ring probe only while it can matter: Liquid Glass on and the
    /// notch showing.
    fn sync_glass_probe(&mut self) {
        let testing = crate::modules::agents::ISOLATED.load(std::sync::atomic::Ordering::Relaxed);
        let want = self.settings.theme == "glass" && !self.hidden && !testing;
        if want == self.glass_probing {
            return;
        }
        self.glass_probing = want;
        self.glass_probe = 0;
        self.glass_fast = false;
        unsafe {
            if want {
                SetTimer(Some(self.hwnd), TIMER_GLASS_PROBE, GLASS_PROBE_MS, None);
            } else {
                let _ = KillTimer(Some(self.hwnd), TIMER_GLASS_PROBE);
            }
        }
    }

    /// The ring around the resting notch changed (a page scrolled under it,
    /// a video plays behind it): what's behind the notch changed too, so
    /// grab it again. The ring sits a little way out, clear of the notch's
    /// own shadow and dots, so looking at it needs no hiding of the notch.
    fn glass_probe_tick(&mut self) {
        let Some(n) = self.glass_shape else { return };
        if self.hidden || self.settings.theme != "glass" {
            return;
        }
        let (ox, oy, k) = (self.origin.0 as f32, self.origin.1 as f32, self.scale);
        let px = |x: f32, y: f32, w: f32, h: f32| RECT { left: (ox + x * k) as i32, top: (oy + y * k) as i32, right: (ox + (x + w) * k) as i32, bottom: (oy + (y + h) * k) as i32 };
        let (gap, th) = (16.0, 4.0);
        let ring = [
            px(n.x - gap - th, n.y, th, n.h),
            px(n.r() + gap, n.y, th, n.h),
            px(n.x - gap, n.b() + gap, n.w + gap * 2.0, th),
        ];
        let Some(h) = crate::glass::probe(&ring) else { return };
        let before = std::mem::replace(&mut self.glass_probe, h);
        let t = self.t();
        if before != 0 && before != h {
            self.glass_dirty = true;
            self.glass_live_until = t + GLASS_LIVE;
            self.request_frame();
        }
        // Moving behind: look again every frame or two; still: slow down.
        let fast = t < self.glass_live_until;
        if fast != self.glass_fast {
            self.glass_fast = fast;
            if !fast {
                // A burst of grabs just ended: hand the memory it used back.
                trim_memory();
            }
            unsafe {
                SetTimer(Some(self.hwnd), TIMER_GLASS_PROBE, if fast { GLASS_LIVE_MS } else { GLASS_PROBE_MS }, None);
            }
        }
    }

    /// Stay on top: another always-on-top window (or one that took the top
    /// for itself) may have come over the notch, or Windows may have let go
    /// of the notch's always-on-top flag. Put it back, without fighting
    /// menus, tooltips or the shell's own pop-ups.
    fn keep_on_top(&mut self) {
        if self.hidden {
            return;
        }
        unsafe {
            let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            let lost = ex & WS_EX_TOPMOST.0 as isize == 0;
            if lost || window_above(self.hwnd, self.notch_area()) {
                dlog(format!("back on top (flag lost: {lost})"));
                self.raise();
            }
        }
    }

    /// Liquid Glass light, no outline: a rim that glows from the very edge
    /// and fades inward (brighter on the top, where the light comes from),
    /// and two soft glints that drift slowly, like light moving through
    /// liquid. They move only while frames are drawn (the notch is open,
    /// hovered or moving), so a resting notch costs nothing.
    fn draw_glass_light(&mut self, geo: &windows::Win32::Graphics::Direct2D::ID2D1PathGeometry, notch: Rect, t: f64) {
        let light = self.glass_light;
        let a = if light { 1.0 } else { 0.8 };
        let white = hex(0xFFFFFF);
        self.gfx.push_layer(Some(geo), 1.0);
        // Fresnel rim: half of each stroke falls outside and is clipped. It
        // runs round the body only, as if the glass went on past the screen
        // edge: no rim along that edge or the flared corners beside it, so
        // the corners read as glass, not as a rounded cut-out.
        let (len, dep) = if self.edge().vertical() { (notch.h, notch.w) } else { (notch.w, notch.h) };
        let rad = self.cur.2.min(dep / 2.0).min(len / 2.0).max(0.0);
        let body = match self.edge() {
            Edge::Top => Rect::new(notch.x, notch.y - 60.0, notch.w, notch.h + 60.0),
            Edge::Bottom => Rect::new(notch.x, notch.y, notch.w, notch.h + 60.0),
            Edge::Left => Rect::new(notch.x - 60.0, notch.y, notch.w + 60.0, notch.h),
            Edge::Right => Rect::new(notch.x, notch.y, notch.w + 60.0, notch.h),
        };
        for (w, k) in [(2.0, 0.32), (7.0, 0.10), (16.0, 0.05), (30.0, 0.022)] {
            self.gfx.stroke_rr_raw(body, rad, white.with_a(k * a), w);
        }
        // Light from above: a soft band along the top inside edge.
        for k in 0..6 {
            let f = k as f32 / 6.0;
            let band = Rect::new(notch.x, notch.y + notch.h * 0.28 * f, notch.w, notch.h * 0.28 / 6.0 + 0.5);
            self.gfx.fill_rr_raw(band, 0.0, white.with_a(0.06 * a * (1.0 - f)));
        }
        // Liquid glints, nudged by the notch's own motion.
        let p = if reduce_motion(&self.settings) { 0.0 } else { t as f32 };
        let push = (self.vel.0 * 0.004).clamp(-0.12, 0.12);
        for (i, (sx, sy, size)) in [(0.37f32, 0.53f32, 0.30f32), (0.29, 0.41, 0.22)].into_iter().enumerate() {
            let off = i as f32 * 2.4;
            let cx = notch.x + notch.w * (0.5 + 0.36 * (p * sx + off).sin() + push);
            let cy = notch.y + notch.h * (0.42 + 0.24 * (p * sy + off * 1.3).cos());
            let (rx, ry) = (notch.w * size * 0.5, notch.h * size * 1.4);
            for k in 0..7 {
                let f = 1.0 - k as f32 / 7.0;
                self.gfx.ellipse_raw(cx, cy, rx * f, ry * f, white.with_a(0.016 * a));
            }
        }
        self.gfx.pop_layer();
    }

    /// Slim's width while something shows (wider with seconds).
    fn slim_live_w(&self) -> f32 {
        SLIM_LIVE_W + if self.settings.seconds { SECONDS_W } else { 0.0 }
    }

    /// Tab bar height and panel padding.
    fn panel_metrics(&self) -> (f32, f32) {
        if self.slim_panel() {
            (SLIM_TAB_H, SLIM_PAD)
        } else {
            (TAB_H, PAD)
        }
    }

    /// Making room: the setting is on and a maximized window is under us.
    /// Smart hide is stepping aside right now: a window's top (title bar,
    /// tabs, address bar) is under the notch, and it just came (or you
    /// switched to it), or the mouse is near the notch but not on it, or a
    /// window is being dragged.
    fn yielding(&self) -> bool {
        if !self.settings.make_room || !self.covered {
            return false;
        }
        let t = self.t();
        t - self.covered_since < SMART_SETTLE || t < self.near_until || self.moving
    }

    /// True when the foreground window is maximized on the notch's screen.
    /// Is any app window under the notch's place (its top area: title bar,
    /// tabs, address bar)? Walks the top-level windows once; only called on
    /// window events, while a window is dragged, and once a second.
    fn window_under(&self) -> bool {
        match MAXIMIZED_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed) {
            1 => return true,
            2 => return false,
            _ => {}
        }
        if self.hidden || self.dock_x.is_some() {
            return false;
        }
        let mut s = UnderSearch { area: self.notch_area(), me: self.hwnd, pid: std::process::id(), found: false };
        unsafe {
            let _ = EnumWindows(Some(under_cb), LPARAM(&mut s as *mut UnderSearch as isize));
        }
        s.found
    }

    /// The notch's place on screen (pixels), a little wider and deeper.
    fn notch_area(&self) -> RECT {
        let r = self.edge_rect(340.0, 46.0);
        let (ox, oy, k) = (self.origin.0 as f32, self.origin.1 as f32, self.scale);
        RECT { left: (ox + r.x * k) as i32, top: (oy + r.y * k) as i32, right: (ox + r.r() * k) as i32, bottom: (oy + r.b() * k) as i32 }
    }

    /// Is this one window under the notch? (A window on another screen, or
    /// lower down, isn't.)
    fn window_covers(&self, h: HWND) -> bool {
        match MAXIMIZED_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed) {
            1 => return !h.0.is_null(),
            2 => return false,
            _ => {}
        }
        !h.0.is_null() && unsafe { covers(h, self.notch_area(), self.hwnd, std::process::id()) }
    }

    pub fn update_make_room(&mut self) {
        let m = self.window_under();
        self.under_at = self.t();
        if m != self.covered {
            dlog(format!("window under the notch: {m}"));
            self.covered = m;
            if m {
                self.covered_since = self.t();
            }
            self.request_frame();
        }
        self.sync_yield();
        self.sync_passthrough();
    }

    /// Redraw when smart hide steps aside or comes back.
    fn sync_yield(&mut self) {
        let y = self.yielding();
        if y != self.was_yielding {
            self.was_yielding = y;
            self.request_frame();
        }
    }

    /// You switched windows: if you switched to one under the notch, step
    /// aside again (not for a window on another screen or lower down).
    pub fn window_switched(&mut self, h: HWND) {
        self.update_make_room();
        if self.covered && self.window_covers(h) {
            self.covered_since = self.t();
            self.sync_yield();
        }
    }

    /// A window started (or stopped) being dragged or resized. It only
    /// counts while it is over the notch.
    pub fn window_moving(&mut self, on: bool, h: HWND) {
        unsafe {
            if on {
                SetTimer(Some(self.hwnd), TIMER_MOVE, 100, None);
            } else {
                let _ = KillTimer(Some(self.hwnd), TIMER_MOVE);
            }
        }
        let over = self.window_covers(h);
        if on {
            self.moving_hwnd = Some(h.0 as isize);
            self.moving = over;
        } else {
            self.moving_hwnd = None;
            self.moving = false;
            // Dropped where it covers the notch: stay out of the way a moment.
            if over {
                self.covered_since = self.t();
            }
        }
        self.update_make_room();
    }

    /// While a window is dragged: is it over the notch right now?
    fn drag_tick(&mut self) {
        if let Some(h) = self.moving_hwnd {
            self.moving = self.window_covers(HWND(h as *mut core::ffi::c_void));
        }
        self.update_make_room();
    }

    /// Click-through while the closed notch makes room, so the tabs under
    /// it stay clickable; a cursor poll opens it on hover instead.
    fn sync_passthrough(&mut self) {
        let want = self.settings.make_room && self.covered && !self.hidden && self.mode() == Mode::Collapsed && self.queue.is_empty() && !self.dragging && self.rdrag.is_none();
        if want == self.passthrough {
            return;
        }
        self.passthrough = want;
        self.poll_since = None;
        unsafe {
            let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            let t = WS_EX_TRANSPARENT.0 as isize;
            SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, if want { ex | t } else { ex & !t });
            if want {
                SetTimer(Some(self.hwnd), TIMER_POLL, 90, None);
            } else {
                let _ = KillTimer(Some(self.hwnd), TIMER_POLL);
            }
        }
        if want {
            // No more mouse messages until it opens again.
            self.hover = false;
            self.tip = None;
        }
    }

    fn poll_cursor(&mut self) {
        if !self.passthrough {
            return;
        }
        let mut p = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut p);
        }
        let x = (p.x - self.origin.0) as f32 / self.scale;
        let y = (p.y - self.origin.1) as f32 / self.scale;
        self.poll_at(x, y);
        // The window moved away: notice within half a second.
        if self.t() - self.under_at >= 0.5 {
            self.update_make_room();
        }
        self.sync_yield();
    }

    /// Cursor at canvas (x, y) while click-through: rest on the bar to open.
    pub fn poll_at(&mut self, x: f32, y: f32) {
        // The notch's own place (even while it steps aside): resting there
        // opens it. Around it (the address bar, tabs): it steps aside.
        let zone = if self.yielding() { self.edge_rect(240.0, 16.0) } else { self.edge_rect(240.0, 38.0) };
        if !zone.contains(x, y) {
            self.poll_since = None;
            self.hover_block = false;
            if self.edge_rect(460.0, 120.0).contains(x, y) {
                self.near_until = self.t() + SMART_NEAR;
                self.sync_yield();
            }
            return;
        }
        if !self.can_hover_open() {
            return;
        }
        let t = self.t();
        let since = *self.poll_since.get_or_insert(t);
        if t - since >= NotchStyle::Pill.hover_delay() {
            self.poll_since = None;
            self.passthrough = false;
            unsafe {
                let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, ex & !(WS_EX_TRANSPARENT.0 as isize));
                let _ = KillTimer(Some(self.hwnd), TIMER_POLL);
            }
            self.on_mouse_move(x, y);
            self.open_from_hover();
        }
    }

    /// Hide while a game, video or presentation fills the notch's screen.
    /// Windows' own "busy" state (QUNS_BUSY) is not used: it also reports
    /// busy while the desktop itself has focus and for some ordinary
    /// windows, which made the notch vanish until the Start menu was opened.
    pub fn check_fullscreen(&mut self) {
        let busy = !FORCE_VISIBLE.load(std::sync::atomic::Ordering::Relaxed) && unsafe {
            matches!(SHQueryUserNotificationState(), Ok(QUNS_RUNNING_D3D_FULL_SCREEN) | Ok(QUNS_PRESENTATION_MODE)) || self.foreground_fullscreen()
        } || self.hide_for_front_app()
            || !self.notch_on();
        if busy != self.hidden {
            dlog(format!("fullscreen {busy}"));
            self.hidden = busy;
            unsafe {
                let _ = ShowWindow(self.hwnd, if busy { SW_HIDE } else { SW_SHOWNOACTIVATE });
            }
            if !busy {
                self.raise();
            }
            self.sync_glass_probe();
            self.request_frame();
        }
    }

    /// The app in front is one you chose to hide the notch for (Settings >
    /// Look). Also remembers the last app in front that isn't Nookly.
    fn hide_for_front_app(&mut self) -> bool {
        let mut cache = std::mem::take(&mut self.front_cache);
        let app = crate::modules::health::front_app(&mut cache);
        self.front_cache = cache;
        let Some(app) = app else { return false };
        let l = app.to_lowercase();
        if l != "nookly" && l != "explorer" && l != "shellexperiencehost" && l != "searchhost" && l != "startmenuexperiencehost" {
            self.last_front = app.clone();
        }
        hidden_apps(&self.settings).iter().any(|a| a.eq_ignore_ascii_case(&app))
    }

    /// True when the foreground window really is full screen on our monitor.
    fn foreground_fullscreen(&self) -> bool {
        unsafe {
            let fg = GetForegroundWindow();
            if fg.0.is_null() || fg == self.hwnd || IsIconic(fg).as_bool() || !IsWindowVisible(fg).as_bool() {
                return false;
            }
            let mut buf = [0u16; 128];
            let n = GetClassNameW(fg, &mut buf) as usize;
            let class = String::from_utf16_lossy(&buf[..n.min(buf.len())]);
            let mut wr = RECT::default();
            if GetWindowRect(fg, &mut wr).is_err() {
                return false;
            }
            let mon = MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(mon, &mut mi);
            is_fullscreen(wr, mi.rcMonitor, IsZoomed(fg).as_bool(), &class)
        }
    }

    /// Put the notch back on top of other always-on-top windows.
    fn raise(&self) {
        unsafe {
            let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }

    fn system_idle_secs() -> f64 {
        let o = IDLE_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed);
        if o >= 0 {
            return o as f64;
        }
        unsafe {
            let mut lii = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
            if GetLastInputInfo(&mut lii).as_bool() {
                GetTickCount().wrapping_sub(lii.dwTime) as f64 / 1000.0
            } else {
                0.0
            }
        }
    }

    fn live_module(&self) -> Option<usize> {
        let mut best: Option<(u8, usize)> = None;
        for (i, m) in self.modules.iter().enumerate() {
            if !self.enabled(i) {
                continue;
            }
            if let Some(p) = m.live() {
                if best.map_or(true, |(bp, _)| p > bp) {
                    best = Some((p, i));
                }
            }
        }
        best.map(|(_, i)| i)
    }

    /// Module `i` is on (its panel isn't hidden in Settings > Panels).
    /// Modules without a tab are always on.
    fn enabled(&self, i: usize) -> bool {
        let m = &self.modules[i];
        !m.has_tab() || !self.settings.panel_hidden(m.title())
    }

    /// The tabs in the tab bar, in order.
    /// (Widgets is set up in the Settings window, not in the notch.)
    fn tab_list(&self) -> Vec<usize> {
        let mut v: Vec<usize> = (0..self.modules.len()).filter(|&i| self.modules[i].has_tab() && self.enabled(i) && self.modules[i].title() != "Widgets").collect();
        // Your own order (Settings > Panels); panels not in it keep theirs, after.
        let order = panel_order(&self.settings);
        if !order.is_empty() {
            v.sort_by_key(|&i| (order.iter().position(|t| t == self.modules[i].title()).unwrap_or(usize::MAX), i));
        }
        v
    }

    /// The mascot shares the closed notch with a live activity / the clock,
    /// at the side picked in Settings (top and bottom edges, Full style).
    fn mascot_side(&self) -> Option<u8> {
        let st = &self.settings;
        (st.style.full_like() && !st.edge.vertical() && self.mood != Mood::Clock).then_some(st.mascot_pos)
    }

    /// Mascot state machine: clock -> eyes (notch idle) -> doze (user away)
    /// -> wave (user back) -> eyes. With "Always" the clock step is skipped.
    fn update_mood(&mut self) {
        let t = self.t();
        let busy = self.expanded || self.toast.is_some() || self.live_module().is_some();
        if busy {
            self.last_activity = t;
        }
        let idle = Self::system_idle_secs();
        let always = self.settings.mascot_always;
        let next = if !self.settings.personality || (busy && !always) || !self.look().full_like() {
            Mood::Clock
        } else {
            match self.mood {
                Mood::Doze if idle < 2.0 => Mood::Wave,
                Mood::Wave if t - self.mood_since > 3.0 => Mood::Eyes,
                Mood::Wave => Mood::Wave,
                _ if idle > 120.0 => Mood::Doze,
                _ if always => Mood::Eyes,
                _ if t - self.last_activity > 45.0 => Mood::Eyes,
                Mood::Clock => Mood::Clock,
                m => m,
            }
        };
        if next != self.mood {
            self.mood = next;
            self.mood_since = t;
        }
    }

    // ---------------------------------------------------------- animation

    pub fn request_frame(&mut self) {
        self.set_frame_timer(VSYNC);
    }

    /// Choose the frame rate: 0 = none, `VSYNC` = every display refresh,
    /// otherwise every `ms`. The main loop (main.rs) does the pacing; the
    /// Windows timer only keeps things moving inside modal loops (menus,
    /// drag and drop), where the main loop doesn't run.
    fn set_frame_timer(&mut self, ms: u32) {
        if self.frame_interval != ms {
            self.frame_interval = ms;
            unsafe {
                if ms == 0 {
                    let _ = KillTimer(Some(self.hwnd), TIMER_FRAME);
                } else {
                    SetTimer(Some(self.hwnd), TIMER_FRAME, if ms == VSYNC { 16 } else { ms }, None);
                }
            }
        }
    }

    /// When the main loop should draw next.
    pub fn pacing(&self) -> Pacing {
        match self.frame_interval {
            0 => Pacing::Idle,
            VSYNC => Pacing::Refresh,
            ms => Pacing::After((self.last_frame + ms as f64 / 1000.0 - self.t()).max(0.0)),
        }
    }

    /// The fallback timer fired: draw only if the main loop hasn't (it is
    /// blocked in a modal loop), so frames are never doubled.
    fn timer_frame(&mut self) {
        let due = if self.frame_interval == VSYNC { 0.03 } else { self.frame_interval as f64 / 1000.0 * 1.5 };
        if self.t() - self.last_frame >= due {
            self.frame();
        }
    }

    fn mode(&self) -> Mode {
        if self.expanded || self.keyboard.is_some() || self.drag_files || self.onboarding.is_some() {
            Mode::Expanded
        } else if self.toast.is_some() {
            Mode::Toast
        } else {
            Mode::Collapsed
        }
    }

    /// Target size in edge-local terms: (along the edge, away from it, radius).
    fn target(&mut self) -> (f32, f32, f32) {
        let vertical = self.edge().vertical();
        // Panels keep their upright (w, h); on side edges "along" is the height.
        let panel = |w: f32, h: f32, rad: f32| if vertical { (h, w, rad) } else { (w, h, rad) };
        match self.mode() {
            Mode::Expanded => {
                // Every tab shares one panel size so the tab bar never moves;
                // only an extra-wide view (Music lyrics) grows sideways.
                let (w, h) = if self.onboarding.is_some() {
                    ONBOARD_SIZE
                } else if self.slim_panel() {
                    let m = &self.modules[self.tab];
                    let (mw, mh) = m.slim_min();
                    let h = if m.fit_height() { m.size().1.min(TALL_PANEL) } else { (modules::PANEL.1 - SLIM_TRIM.1).max(mh).max(m.size().1.min(TALL_PANEL)) };
                    ((m.size().0.max(modules::PANEL.0) - SLIM_TRIM.0).max(mw), h)
                } else {
                    // A tab may be taller (the Widgets lists), never shorter,
                    // unless it fits its own content (the Volume list).
                    let m = &self.modules[self.tab];
                    let low = if m.fit_height() { 60.0 } else { modules::PANEL.1 };
                    (m.size().0.max(modules::PANEL.0), m.size().1.clamp(low, TALL_PANEL))
                };
                let (tab_h, pad) = self.panel_metrics();
                let w = (w + 2.0 * pad).clamp(380.0, CANVAS_W - 40.0);
                let h = (h + tab_h + pad + 8.0).clamp(80.0, CANVAS_H - 20.0);
                panel(w, h, 26.0)
            }
            Mode::Toast => {
                let has_actions = self.toast.as_ref().map_or(false, |t| !t.0.actions.is_empty());
                let h = if has_actions { 104.0 } else { 66.0 };
                panel(if self.settings.style == NotchStyle::Slim { 340.0 } else { 380.0 }, h, 24.0)
            }
            Mode::Collapsed => {
                self.refresh_shown();
                let live = self.notch_shown.is_some();
                let side_mascot = matches!(self.mascot_side(), Some(0 | 2));
                // Seconds on the clock: the narrow notches grow a little.
                let sec = if self.settings.seconds { SECONDS_W } else { 0.0 };
                match (self.look(), vertical) {
                    (NotchStyle::Full, false) if live && side_mascot => (364.0, 34.0, 13.0),
                    (NotchStyle::Full, false) if live => (300.0, 34.0, 13.0),
                    (NotchStyle::Full, false) => (200.0, 32.0, 12.0),
                    // Side edges, live: room for the stacked clock between
                    // the top and bottom items (taller with seconds).
                    (NotchStyle::Full, true) if live => (130.0 + sec, 40.0, 14.0),
                    (NotchStyle::Full, true) => (100.0, 40.0, 14.0),
                    // Slim: Full's height, narrower.
                    // Live: one width for every activity, plus a side mascot's strip.
                    // Live: one width for everything. A side mascot takes one
                    // side; only an item across both sides makes room for it.
                    (NotchStyle::Slim, false) if live => {
                        let wide = matches!(self.notch_shown, Some(Shown::Wide(_)));
                        (self.slim_live_w() + if side_mascot && wide { SLIM_MASCOT_STRIP } else { 0.0 }, 34.0, 13.0)
                    }
                    (NotchStyle::Slim, false) if side_mascot => (150.0 + sec, 32.0, 12.0),
                    (NotchStyle::Slim, false) => (120.0 + sec, 32.0, 12.0),
                    (NotchStyle::Slim, true) if live => (114.0 + sec, 40.0, 14.0),
                    (NotchStyle::Slim, true) => (80.0, 40.0, 14.0),
                    (NotchStyle::Compact, false) if live => (190.0 + sec, 22.0, 9.0),
                    (NotchStyle::Compact, false) => (96.0 + sec, 20.0, 8.0),
                    (NotchStyle::Compact, true) if live => (72.0 + sec, 26.0, 10.0),
                    (NotchStyle::Compact, true) => (60.0, 26.0, 10.0),
                    // No black notch at all; the bar is drawn separately.
                    (NotchStyle::Pill | NotchStyle::Hidden, _) => (if live { 120.0 } else { 84.0 }, 0.0, 0.0),
                }
            }
        }
    }

    /// Screen rect (canvas DIPs) of a notch `along` x `depth` on the current
    /// edge, centred on `anchor` but kept inside the canvas.
    fn edge_rect(&self, along: f32, depth: f32) -> Rect {
        let edge = self.edge();
        let span = if edge.vertical() { CANVAS_H } else { CANVAS_W };
        let c = self.anchor.clamp(along / 2.0, (span - along / 2.0).max(along / 2.0));
        match edge {
            Edge::Top => Rect::new(c - along / 2.0, 0.0, along, depth),
            Edge::Bottom => Rect::new(c - along / 2.0, CANVAS_H - depth, along, depth),
            Edge::Left => Rect::new(0.0, c - along / 2.0, depth, along),
            Edge::Right => Rect::new(CANVAS_W - depth, c - along / 2.0, depth, along),
        }
    }

    /// Inner content rect with `near` padding on the screen-edge side, `far`
    /// on the opposite side and `side` padding left/right.
    fn content_rect(&self, goal: Rect, near: f32, far: f32, side: f32) -> Rect {
        match self.edge() {
            Edge::Top => Rect::new(goal.x + side, goal.y + near, goal.w - 2.0 * side, goal.h - near - far),
            Edge::Bottom => Rect::new(goal.x + side, goal.y + far, goal.w - 2.0 * side, goal.h - near - far),
            Edge::Left | Edge::Right => {
                let v = (near + far) / 2.0;
                Rect::new(goal.x + side, goal.y + v, goal.w - 2.0 * side, goal.h - 2.0 * v)
            }
        }
    }

    pub fn frame(&mut self) {
        let t = self.t();
        let dt = (t - self.last_frame).clamp(0.0, 0.1) as f32;
        self.last_frame = t;
        // A toast or an opened panel must take clicks again at once.
        self.sync_passthrough();

        if self.mode() == Mode::Collapsed {
            if let Some(i) = self.remembered_tab() {
                if i != self.tab {
                    self.tab = i;
                    self.shown = (i, false);
                }
            }
        }

        // Open once the mouse has rested on the collapsed notch long enough.
        if self.hover && self.can_hover_open() && t - self.hover_since >= self.look().hover_delay() {
            self.open_from_hover();
        }
        // Collapse after the mouse has left for a moment.
        if let Some(at) = self.leave_at {
            if t >= at && !self.hover && self.keyboard.is_none() && !self.drag_files && self.onboarding.is_none() {
                self.expanded = false;
                self.leave_at = None;
            }
        }
        // Toast lifecycle (paused while the panel is open, and while the
        // notch is hidden so nothing expires or chimes unseen).
        if self.mode() != Mode::Expanded && !self.hidden {
            if let Some((_, end)) = &self.toast {
                if t >= *end && !self.hover {
                    self.toast = None;
                }
            }
            // Reminders (the ones that chime) wait while you're on a call.
            let in_call = crate::modules::privacy::IN_CALL.load(std::sync::atomic::Ordering::Relaxed);
            let front = self.queue.front().map_or(false, |q| q.chime && in_call && q.key != "meeting");
            if self.toast.is_none() && !front {
                if let Some(next) = self.queue.pop_front() {
                    let end = t + next.secs as f64;
                    if next.chime {
                        sound::chime();
                    }
                    self.toast = Some((next, end));
                    // Another always-on-top window may have covered us.
                    self.raise();
                }
            }
        }

        // Switching tabs cross-fades the content.
        let shown = (self.tab, false);
        if shown != self.shown {
            if self.mode() == Mode::Expanded {
                self.switch_t = t;
                self.prev_shown = self.shown;
                // Slide toward the side of the newly picked tab.
                let slot = |s: (usize, bool)| if s.1 { usize::MAX } else { s.0 };
                self.switch_dir = if slot(shown) >= slot(self.shown) { 1.0 } else { -1.0 };
            }
            self.shown = shown;
        }
        // Liquid Glass: opening (or a toast dropping down) grabs what's
        // behind right now, not what was there when it last looked.
        let open = self.mode() != Mode::Collapsed;
        if open && !self.glass_was_open {
            self.glass_dirty = true;
            self.glass_full = true;
        }
        self.glass_was_open = open;

        let target = self.target();
        let dist = self.step_anim(target, dt);

        self.render(target, dist);

        let ambient = (0..self.modules.len()).any(|i| self.enabled(i) && self.modules[i].animating())
            || matches!(self.mood, Mood::Eyes | Mood::Doze | Mood::Wave)
            || self.toast.is_some()
            || self.leave_at.is_some()
            || self.hover;
        // Tab switch: slide, highlight glide and glow all finish within 0.6 s.
        let moving = dist > 0.0 || t - self.switch_t < 0.6;
        self.perf_note(t, moving);
        if moving {
            self.set_frame_timer(VSYNC);
        } else if ambient {
            // Precise timer (see main.rs): 60 fps for live content in the
            // open panel (lyrics, progress, stopwatch), 30 fps otherwise.
            let live = self.mode() == Mode::Expanded && (0..self.modules.len()).any(|i| self.enabled(i) && self.modules[i].animating());
            self.set_frame_timer(if live { 16 } else { 33 });
        } else {
            self.set_frame_timer(0);
        }
    }

    /// With NOOKLY_DEBUG=1, log each transition's frame rate and worst gap
    /// to debug.log, so a "looks choppy" report comes with numbers.
    fn perf_note(&mut self, t: f64, moving: bool) {
        if moving {
            let p = self.perf.get_or_insert((t, t, 0, 0.0));
            p.3 = p.3.max(t - p.1);
            p.1 = t;
            p.2 += 1;
        } else if let Some((s, e, n, worst)) = self.perf.take() {
            if n > 2 {
                dlog(format!("anim {n} frames in {:.0} ms: {:.0} fps, worst gap {:.1} ms", (e - s) * 1000.0, (n - 1) as f64 / (e - s).max(1e-6), worst * 1000.0));
            }
        }
    }

    /// Move the notch size toward `target` with a lightly under-damped spring
    /// (a small, soft overshoot like a phone's island), in fixed sub-steps so
    /// it behaves the same at any frame rate. Returns the remaining distance
    /// (0 once settled).
    fn step_anim(&mut self, target: (f32, f32, f32), dt: f32) -> f32 {
        // Reduce motion: straight to the new size.
        if reduce_motion(&self.settings) {
            self.cur = target;
            self.vel = (0.0, 0.0, 0.0);
            return 0.0;
        }
        // Liquid: the width is springy (about 12% overshoot and a small
        // second wobble) and the height follows a little behind, so the
        // shape stretches as it opens and settles like a drop of water.
        // (stiffness in rad/s, damping ratio) for width, height, corners.
        const SPRING: [(f32, f32); 3] = [(19.0, 0.55), (16.0, 0.62), (22.0, 0.7)];
        let n = ((dt * 240.0).ceil() as usize).clamp(1, 32);
        let h = dt / n as f32;
        let tg = [target.0, target.1, target.2];
        let mut x = [self.cur.0, self.cur.1, self.cur.2];
        let mut v = [self.vel.0, self.vel.1, self.vel.2];
        for _ in 0..n {
            for i in 0..3 {
                let (w, z) = SPRING[i];
                let a = w * w * (tg[i] - x[i]) - 2.0 * z * w * v[i];
                v[i] += a * h;
                x[i] += v[i] * h;
            }
        }
        for xi in x.iter_mut() {
            *xi = xi.max(0.0);
        }
        self.cur = (x[0], x[1], x[2]);
        self.vel = (v[0], v[1], v[2]);
        let dist = (x[0] - tg[0]).abs() + (x[1] - tg[1]).abs();
        let speed = v[0].abs() + v[1].abs();
        if dist < 0.3 && speed < 6.0 {
            self.cur = target;
            self.vel = (0.0, 0.0, 0.0);
            return 0.0;
        }
        dist.max(0.31)
    }

    /// Progress (0..1) of the tab-switch transition.
    fn switch_progress(&self, t: f64) -> f32 {
        if reduce_motion(&self.settings) {
            return 1.0;
        }
        ((t - self.switch_t) / 0.32).clamp(0.0, 1.0) as f32
    }

    /// Opacity of the incoming content during a tab switch (0..1, eased).
    fn switch_alpha(&self, t: f64) -> f32 {
        let k = ((self.switch_progress(t) - 0.15) / 0.85).clamp(0.0, 1.0);
        k * k * (3.0 - 2.0 * k)
    }

    fn render(&mut self, target: (f32, f32, f32), dist: f32) {
        let t = self.t();
        let (along, depth, rad) = self.cur;
        // Liquid squash and stretch: growing fast one way, it gives a
        // little the other way (at most 6%), like a drop changing shape.
        let (along, depth) = if reduce_motion(&self.settings) {
            (along, depth)
        } else {
            let k = |v: f32| (v * 0.00004).clamp(-0.06, 0.06);
            (along * (1.0 - k(self.vel.1)), depth * (1.0 - k(self.vel.0)))
        };
        let notch = self.edge_rect(along, depth);
        let goal = self.edge_rect(target.0, target.1);
        let edge = self.edge();
        let mode = self.mode();
        // While switching tabs the panel stays solid and only the content
        // cross-fades; otherwise content fades in as the notch opens.
        let switching = mode == Mode::Expanded && t - self.switch_t < 0.4;
        let content_alpha = if switching { 1.0 } else { (1.0 - dist / 50.0).clamp(0.0, 1.0) };

        self.hits.clear();
        crate::ui::clear_text_areas();
        self.apply_theme();
        self.gfx.begin(self.scale);
        // The flares where the notch meets the screen edge; Liquid Glass has
        // none: its sides run straight into the edge, like a sheet of glass.
        let ear = if self.gfx.theme.glass { 0.0 } else { (depth / 4.0).min(9.0) };
        let barless = matches!(self.look(), NotchStyle::Pill | NotchStyle::Hidden);
        if depth >= 0.5 {
            if let Some(geo) = self.gfx.notch_geometry(notch, rad, ear, edge) {
                let glass = self.gfx.theme.glass;
                if glass {
                    // Liquid Glass: what is behind, blurred and bent near
                    // the edges like a lens (glass.rs), under a light tint.
                    self.update_backdrop(notch, t, dist, rad);
                    self.glass_shape = if dist <= 0.0 { Some(notch) } else { None };
                    if let Some((b, _, _, at)) = self.backdrop.clone() {
                        self.gfx.push_layer(Some(&geo), 1.0);
                        self.gfx.image_rr(&b, at, 0.0);
                        self.gfx.pop_layer();
                    }
                }
                // The song's colour as a soft glow around the closed notch,
                // breathing with the music (drawn first: the notch covers
                // its inner half, so only the feathered outside shows).
                if mode == Mode::Collapsed && self.settings.extra.get("album_glow").map_or(false, |v| v == "1") {
                    let music = self.modules.iter().position(|m| m.live_id() == "music");
                    if music.is_some() && self.shown_owner() == music {
                        if let Some(c) = *crate::modules::music::ART_COLOR.lock().unwrap() {
                            let level = self.beat_level(t);
                            let strength = 0.5 + 0.5 * level;
                            // Around the notch's rounded body (stretched past the
                            // screen edge), so nothing glows along the edge.
                            let body = match edge {
                                Edge::Top => Rect::new(notch.x, notch.y - 60.0, notch.w, notch.h + 60.0),
                                Edge::Bottom => Rect::new(notch.x, notch.y, notch.w, notch.h + 60.0),
                                Edge::Left => Rect::new(notch.x - 60.0, notch.y, notch.w + 60.0, notch.h),
                                Edge::Right => Rect::new(notch.x, notch.y, notch.w + 60.0, notch.h),
                            };
                            for k in 0..8 {
                                let f = k as f32 / 8.0;
                                let fade = (1.0 - f) * (1.0 - f);
                                self.gfx.stroke_rr_raw(body, rad, c.with_a(strength * 0.2 * fade), 3.0 + k as f32 * 7.0 + level * 12.0);
                            }
                        }
                    }
                }
                self.gfx.fill_geometry(&geo, col::NOTCH);
                if glass {
                    self.draw_glass_light(&geo, notch, t);
                }
                self.gfx.push_layer(Some(&geo), content_alpha);
                match mode {
                    Mode::Expanded if self.onboarding.is_some() => self.draw_onboarding(goal, t),
                    Mode::Expanded => self.draw_expanded(notch, goal, t),
                    Mode::Toast => self.draw_toast(goal, t),
                    Mode::Collapsed if !barless => self.draw_collapsed(goal, t),
                    Mode::Collapsed => {}
                }
                self.gfx.pop_layer();
                // Keep the whole silhouette clickable even while content is fading.
                self.hits.insert(0, Hit { rect: notch, module: APP_HIT, id: u32::MAX, tip: "" });
            }
        }
        if barless {
            self.draw_bar(along, depth);
        }
        if mode != Mode::Expanded {
            let dots = self.indicators();
            if !dots.is_empty() {
                self.draw_dots(&dots, notch);
            }
        }
        if dist <= 0.0 {
            self.draw_tooltip(t);
        }
        self.gfx.end_and_present(self.hwnd, self.origin.0, self.origin.1);
    }

    /// Tooltip for the hit under the mouse after it has rested there for a
    /// moment. Drawn outside the notch (on the side away from the screen
    /// edge) and kept inside the canvas.
    /// The tooltip of a hit: its own, or the owner's text for that id.
    fn tip_for(&self, h: &crate::ui::Hit) -> &'static str {
        if !h.tip.is_empty() {
            return h.tip;
        }
        match h.module {
            SETTINGS => settings_tip(h.id),
            ONBOARD => onboard_tip(h.id),
            m if m < self.modules.len() => self.modules[m].tip(h.id),
            _ => "",
        }
    }

    fn draw_tooltip(&mut self, t: f64) {
        const DELAY: f64 = 0.6;
        let hit = if self.press.is_none() && !self.dragging { self.hit_at(self.mouse.0, self.mouse.1) } else { None };
        let Some(mut h) = hit else {
            self.tip = None;
            return;
        };
        h.tip = self.tip_for(&h);
        if h.tip.is_empty() {
            self.tip = None;
            return;
        }
        match self.tip {
            Some((m, id, _, _)) if m == h.module && id == h.id => {}
            _ => self.tip = Some((h.module, h.id, t, false)),
        }
        let Some((_, _, since, suppressed)) = self.tip else { return };
        if suppressed || t - since < DELAY {
            return;
        }
        let size = 11.5;
        let w = self.gfx.measure(h.tip, size, Weight::Regular) + 16.0;
        let ht = 22.0;
        let r = h.rect;
        let gap = 6.0;
        let (mut x, mut y) = match self.edge() {
            Edge::Top => (r.cx() - w / 2.0, r.b() + gap),
            Edge::Bottom => (r.cx() - w / 2.0, r.y - gap - ht),
            Edge::Left => (r.r() + gap, r.cy() - ht / 2.0),
            Edge::Right => (r.x - gap - w, r.cy() - ht / 2.0),
        };
        // Flip to the other side if it would leave the canvas.
        if y + ht > CANVAS_H {
            y = r.y - gap - ht;
        }
        if y < 0.0 {
            y = r.b() + gap;
        }
        x = x.clamp(2.0, (CANVAS_W - w - 2.0).max(2.0));
        y = y.clamp(2.0, (CANVAS_H - ht - 2.0).max(2.0));
        let fade = ((t - since - DELAY) / 0.12).clamp(0.0, 1.0) as f32;
        let tr = Rect::new(x, y, w, ht);
        self.gfx.alpha = fade;
        self.gfx.fill_rr(tr, 7.0, hex(0x2A2A2E));
        self.gfx.stroke_rr(tr, 7.0, col::TEXT.with_a(0.14), 1.0);
        self.gfx.text(h.tip, tr, size, col::TEXT, Weight::Regular, Align::Center);
        self.gfx.alpha = 1.0;
    }

    /// The open panel: tab bar (laid out in the animated `notch` rect so it
    /// glides with the shape) and the module or settings content (laid out
    /// in the final `goal` rect so it doesn't reflow while animating).
    fn draw_expanded(&mut self, notch: Rect, goal: Rect, t: f64) {
        let (tab_h, pad) = self.panel_metrics();
        let r = self.content_rect(goal, 6.0, pad, pad);
        let r = Rect::new(r.x, r.y + tab_h + 8.0, r.w, r.h - tab_h - 8.0);
        let mut bar = self.content_rect(notch, 6.0, pad, pad).cut_top(tab_h, 8.0);
        let now = Now::get();
        let mouse = self.mouse;
        // With many tabs the icons get a little narrower, so the panel's
        // name still fits beside the clock; past that the rest go under ⋯.
        let clock_w = self.gfx.measure(&now.hhmm(), 14.0, Weight::Semi) + 10.0;
        let room = bar.w - (tab_h + 4.0 + TAB_GAP) - clock_w - 84.0;
        // How many fit is decided on the usual panel width, so the bar
        // stays the same on wider panels (Widgets, Settings).
        let (tabs, more) = self.bar_tabs(room.min(bar_room(tab_h)));
        let slots = tabs.len() + more as usize;
        let tw = (room / slots.max(1) as f32 - TAB_GAP).clamp(TAB_MIN, tab_h).floor();
        let slot = tw + TAB_GAP;
        self.tab_bar = bar;
        self.tab_slot = slot;
        if !more {
            self.more_open = false;
        }
        // The highlight pill glides to the selected tab (or ⋯).
        let sel_off = if self.more_open {
            tabs.len() as f32 * slot
        } else {
            tabs.iter().position(|&i| i == self.tab).unwrap_or(0) as f32 * slot
        };
        let gap = t - self.pill_last;
        self.pill_last = t;
        if gap > 0.15 {
            self.pill_off = sel_off;
        } else {
            let k = 1.0 - (-(gap as f32) * 16.0).exp();
            self.pill_off += (sel_off - self.pill_off) * k;
            if (self.pill_off - sel_off).abs() < 0.3 {
                self.pill_off = sel_off;
            }
        }
        // Just-selected tab: a glow in its colour and a little pop.
        let gp = ((t - self.switch_t) / 0.55).clamp(0.0, 1.0) as f32;
        let accent = self.modules[self.tab].accent();
        let indicators = self.indicators();
        {
            let mut ui = Ui { g: &mut self.gfx, hits: &mut self.hits, module: APP_HIT, mouse, t };
            let pill = Rect::new(bar.x + self.pill_off, bar.y, tw, tab_h);
            if gp < 1.0 {
                let fade = (1.0 - gp) * (1.0 - gp);
                let (cx, cy) = (bar.x + sel_off + tw / 2.0, bar.cy());
                for k in 0..5 {
                    let rad = tab_h * 0.5 + 1.0 + k as f32 * 3.0 + gp * 6.0;
                    ui.circle(cx, cy, rad, accent.with_a(0.16 * fade * (1.0 - k as f32 / 5.0)));
                }
            }
            let pill_c = col::TEXT.with_a(0.16).lerp(accent.with_a(0.42), (1.0 - gp) * 0.8);
            ui.fill_rr(pill, tw.min(tab_h) / 2.0, pill_c);
            for &i in &tabs {
                let tr = bar.cut_left(tw, TAB_GAP);
                let m = &self.modules[i];
                let sel = i == self.tab && !self.more_open;
                let fg = if sel && gp < 1.0 {
                    m.accent().lerp(col::TEXT, gp)
                } else if sel {
                    col::TEXT
                } else {
                    col::DIM
                };
                // The icon stays still (no size change or hop): only its colour
                // fades from the tab's accent to white.
                ui.icon_btn(tr, m.icon(), i as u32, 14.0, fg, None, m.title());
            }
            if more {
                let tr = bar.cut_left(tw, TAB_GAP);
                let fg = if self.more_open { col::TEXT } else { col::DIM };
                ui.icon_btn(tr, glyph::MORE, MORE, 14.0, fg, None, "More panels");
            }
            // Settings gear, set slightly apart from the tabs: it opens the
            // Settings window.
            bar.cut_left(4.0, 0.0);
            let gr = bar.cut_left(tw, TAB_GAP);
            ui.icon_btn(gr, glyph::SETTINGS, GEAR, 14.0, col::DIM, None, "Settings");
            // Title of the current tab + date/time on the right. With many
            // tabs on a narrow panel, drop the date first, then the title
            // (tab tooltips still name them), so nothing overlaps.
            let time = now.hhmm();
            let tw = ui.measure(&time, 14.0, Weight::Semi);
            let tr = bar.cut_right(tw + 2.0, 8.0);
            ui.text(&time, tr, 14.0, col::TEXT, Weight::Semi, Align::Right);
            // Mic / camera in-use dots sit just left of the clock.
            for c in indicators.iter().rev() {
                let d = bar.cut_right(10.0, 2.0);
                ui.circle(d.cx(), d.cy(), 3.5, *c);
            }
            let title = if self.more_open {
                "More panels"
            } else {
                panel_name(self.modules[self.tab].title())
            };
            let date = now.short_date();
            let title_w = ui.measure(title, 13.0, Weight::Semi) + 10.0;
            let date_w = ui.measure(&date, 12.0, Weight::Regular) + 8.0;
            if bar.w >= title_w + date_w {
                let dr = bar.cut_right(date_w, 0.0);
                ui.text(&date, dr, 12.0, col::DIM, Weight::Regular, Align::Right);
            }
            let show_title = bar.w >= title_w;
            if show_title {
                bar.x += 6.0;
                bar.w -= 6.0;
                ui.text(title, bar, 13.0, col::TEXT, Weight::Semi, Align::Left);
            }
            self.header_fit = (show_title, bar.w >= 0.0);
        }

        // Content: on a tab switch the old content slides out and fades
        // while the new slides in from the side you moved toward.
        let p = self.switch_progress(t);
        let base = self.gfx.alpha;
        self.gfx.push_clip(r.inset(-2.0));
        let old = self.prev_shown;
        let sliding = p < 1.0 && !old.1 && old.0 != self.tab && old.0 < self.modules.len();
        let travel = 40.0 * self.switch_dir;
        let e = 1.0 - (1.0 - p).powi(3);
        if sliding {
            let oa = (1.0 - p / 0.55).clamp(0.0, 1.0);
            if oa > 0.0 {
                let n = self.hits.len();
                self.gfx.alpha = base * oa;
                let rr = shift(self.module_rect(old.0, r), -travel * e);
                let mut ui = Ui { g: &mut self.gfx, hits: &mut self.hits, module: old.0, mouse: (-1e4, -1e4), t };
                self.modules[old.0].draw(&mut ui, rr);
                // The outgoing content is display-only.
                self.hits.truncate(n);
            }
        }
        self.gfx.alpha = base * self.switch_alpha(t);
        if self.more_open {
            self.draw_more(r, t);
        } else {
            let i = self.tab;
            if self.modules[i].title() == "Widgets" {
                self.refresh_shown();
                let cat = self.notch_catalog();
                let showing = self.showing_ids();
                let st = self.settings.clone();
                self.modules[i].set_widget_catalog(cat, &st, showing);
            }
            let dx = if sliding { travel * (1.0 - e) } else { 0.0 };
            let rr = shift(self.module_rect(i, r), dx);
            let mut ui = Ui { g: &mut self.gfx, hits: &mut self.hits, module: i, mouse, t };
            self.modules[i].draw(&mut ui, rr);
        }
        self.gfx.pop_clip();
        self.gfx.alpha = base;
    }

    /// Mic / camera dots just beside the closed notch (or toast), on the
    /// screen edge, like a phone's privacy dots. A dark rim keeps them
    /// visible on white windows.
    fn draw_dots(&mut self, dots: &[Color], notch: Rect) {
        let edge = self.edge();
        let n = dots.len() as f32;
        for (k, c) in dots.iter().enumerate() {
            let off = 10.0 + k as f32 * 12.0;
            let (mut x, mut y) = match edge {
                Edge::Top => (notch.r() + off, 7.0),
                Edge::Bottom => (notch.r() + off, CANVAS_H - 7.0),
                Edge::Left => (7.0, notch.b() + off),
                Edge::Right => (CANVAS_W - 7.0, notch.b() + off),
            };
            // Near the canvas end: put them on the other side instead.
            if x > CANVAS_W - 8.0 {
                x = notch.x - 10.0 - (n - 1.0 - k as f32) * 12.0;
            }
            if y > CANVAS_H - 8.0 && edge.vertical() {
                y = notch.y - 10.0 - (n - 1.0 - k as f32) * 12.0;
            }
            self.gfx.circle(x, y, 5.4, col::NOTCH.with_a(0.55));
            self.gfx.circle(x, y, 4.0, *c);
        }
    }

    /// The tabs that fit in the bar (the open one always among them), and
    /// whether the rest go under ⋯.
    fn bar_tabs(&self, room: f32) -> (Vec<usize>, bool) {
        let all = self.tab_list();
        let cap = ((room + TAB_GAP) / (TAB_MIN + TAB_GAP)).floor().max(3.0) as usize;
        if all.len() <= cap {
            return (all, false);
        }
        let mut shown: Vec<usize> = all[..cap - 1].to_vec();
        if !shown.contains(&self.tab) && all.contains(&self.tab) {
            shown[cap - 2] = self.tab;
        }
        (shown, true)
    }

    /// The panels under ⋯, as big tiles.
    fn draw_more(&mut self, r: Rect, t: f64) {
        let (tab_h, _) = self.panel_metrics();
        let clock_w = self.gfx.measure(&Now::get().hhmm(), 14.0, Weight::Semi) + 10.0;
        let room = (self.tab_bar.w - (tab_h + 4.0 + TAB_GAP) - clock_w - 84.0).min(bar_room(tab_h));
        let (shown, _) = self.bar_tabs(room);
        let rest: Vec<usize> = self.tab_list().into_iter().filter(|i| !shown.contains(i)).collect();
        let mouse = self.mouse;
        let tiles: Vec<(usize, &'static str, &'static str, Color)> =
            rest.iter().map(|&i| (i, panel_name(self.modules[i].title()), self.modules[i].icon(), self.modules[i].accent())).collect();
        let mut ui = Ui { g: &mut self.gfx, hits: &mut self.hits, module: APP_HIT, mouse, t };
        // Icon and name side by side; as many to a row as it takes to fit
        // the panel's height.
        let (row_h, gap) = (34.0, 6.0);
        let max_rows = (((r.h + gap) / (row_h + gap)).floor() as usize).max(1);
        let per = ((tiles.len() + max_rows - 1) / max_rows).max(4).min(tiles.len().max(1));
        let rows = (tiles.len() + per - 1) / per;
        let grid = r.centered(r.w.min(130.0 * per as f32), rows as f32 * (row_h + gap) - gap);
        for (k, (i, name, icon, accent)) in tiles.into_iter().enumerate() {
            let row = Rect::new(grid.x, grid.y + (k / per) as f32 * (row_h + gap), grid.w, row_h);
            let cell = row.columns(per, gap)[k % per];
            let hov = ui.hovered(cell);
            ui.fill_rr(cell, 10.0, if hov { col::BTN_HOVER } else { col::BTN });
            let mut c = cell.inset_xy(8.0, 0.0);
            let ic = c.cut_left(24.0, 8.0);
            ui.circle(ic.cx(), ic.cy(), 12.0, accent.with_a(0.18));
            ui.icon(icon, ic, 11.5, accent);
            ui.text(name, c, 12.0, col::TEXT, Weight::Semi, Align::Left);
            ui.hit_tip(cell, i as u32, name);
        }
    }

    /// Where module `i` draws inside the panel content rect `r`: its natural
    /// size, centred.
    fn module_rect(&self, i: usize, r: Rect) -> Rect {
        let (nw, nh) = self.modules[i].size();
        let (w, h) = (nw.min(r.w), nh.min(r.h));
        Rect::new(r.x + (r.w - w) / 2.0, r.y + (r.h - h) / 2.0, w, h)
    }

    /// Privacy dots (mic / camera in use) from every module.
    fn indicators(&self) -> Vec<Color> {
        if !self.settings.privacy_dots {
            return Vec::new();
        }
        self.modules.iter().flat_map(|m| m.indicator()).collect()
    }


    /// Record a shortcut: the window takes the keyboard until a key is
    /// pressed (the old shortcut is off meanwhile, so it can be pressed).
    fn start_recording(&mut self, k: usize) {
        self.recording = Some(k);
        unsafe {
            for i in 0..crate::features::HOTKEYS.len() {
                let _ = UnregisterHotKey(Some(self.hwnd), 1 + i as i32);
            }
            // The Settings window already has the keyboard.
            if self.swin.is_some() {
                return;
            }
            let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, ex & !(WS_EX_NOACTIVATE.0 as isize));
            let _ = SetForegroundWindow(self.hwnd);
            let _ = SetFocus(Some(self.hwnd));
        }
    }

    fn stop_recording(&mut self) {
        if self.recording.take().is_some() {
            if self.keyboard.is_none() {
                unsafe {
                    let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
                    SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE.0 as isize);
                }
            }
            self.sync_hotkeys();
            self.request_frame();
        }
    }

    fn sync_hotkeys(&mut self) {
        if crate::modules::agents::ISOLATED.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        self.hotkey_fail = crate::features::register(self.hwnd, &self.settings);
    }

    /// A key while recording a shortcut: Esc cancels, Backspace / Delete
    /// removes it, a key with Ctrl, Alt or Win (or an F key) sets it.
    fn record_key(&mut self, vk: u32) {
        let Some(k) = self.recording else { return };
        let down = |v: VIRTUAL_KEY| unsafe { GetKeyState(v.0 as i32) } < 0;
        let mods = (down(VK_MENU) as u32) | (down(VK_CONTROL) as u32) << 1 | (down(VK_SHIFT) as u32) << 2 | ((down(VK_LWIN) || down(VK_RWIN)) as u32) << 3;
        match vk {
            0x1B => {}
            0x08 | 0x2E if mods == 0 => {
                crate::features::set_hotkey(&mut self.settings, k, None);
                self.settings.save();
            }
            _ => match crate::features::recorded(mods, vk) {
                Some(key) => {
                    crate::features::set_hotkey(&mut self.settings, k, Some(key));
                    self.settings.save();
                }
                None => return,
            },
        }
        self.stop_recording();
    }

    /// A shortcut was pressed anywhere in Windows.
    fn on_hotkey(&mut self, id: usize) {
        let t = self.t();
        // The notch is switched off (Settings > Notch): its shortcuts rest too.
        if !self.notch_on() {
            return;
        }
        match id {
            1 => {
                if self.expanded && self.onboarding.is_none() {
                    self.expanded = false;
                    self.more_open = false;
                    self.leave_at = None;
                    if let Some(k) = self.keyboard {
                        self.modules[k].blur();
                        self.release_keyboard();
                    }
                } else {
                    self.expanded = true;
                    if !self.hover {
                        self.leave_at = Some(t + 8.0);
                    }
                }
            }
            2 | 3 | 4 => {
                let (title, action) = match id {
                    2 => ("Clipboard", crate::modules::clipboard::A_SEARCH),
                    3 => ("Shelf", crate::modules::shelf::A_SNIP),
                    _ => ("Notes", crate::modules::notes::A_FOCUS),
                };
                let Some(i) = self.modules.iter().position(|m| m.title() == title) else { return };
                if !self.enabled(i) {
                    self.push_toast(Toast::new("feature_off", glyph::SETTINGS, col::ACCENT, &format!("{} is off", feature_name(title)), "Switch it on in Settings > Notch > Features").secs(4.0));
                } else if id == 2 || id == 4 {
                    self.tab = i;
                    self.more_open = false;
                    self.expanded = true;
                    self.call(i, |m, c| m.action(action, (0.0, 0.0), c));
                } else {
                    self.call(i, |m, c| m.action(action, (0.0, 0.0), c));
                }
            }
            _ => {}
        }
        self.request_frame();
    }

    /// Mouse wheel over the closed notch: volume or brightness, as set in
    /// Settings > Shortcuts (Shift: brightness).
    fn wheel_closed(&mut self, delta: f32) {
        let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
        let what = self.settings.extra.get("mouse_wheel").cloned().unwrap_or_else(|| "off".into());
        let what = if shift { "brightness".to_string() } else { what };
        let up = delta > 0.0;
        match what.as_str() {
            "volume" => {
                use windows::Win32::Media::Audio::*;
                use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
                unsafe {
                    let ep: Option<windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume> = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
                        .ok()
                        .and_then(|e| e.GetDefaultAudioEndpoint(eRender, eConsole).ok())
                        .and_then(|d| d.Activate(CLSCTX_ALL, None).ok());
                    if let Some(ep) = ep {
                        let _ = if up { ep.VolumeStepUp(std::ptr::null()) } else { ep.VolumeStepDown(std::ptr::null()) };
                    }
                }
            }
            "brightness" => {
                // Brightness calls can take a moment (WMI): off the UI thread.
                if BRIGHT_BUSY.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                let h = self.hwnd.0 as isize;
                std::thread::spawn(move || {
                    let hwnd = HWND(h as *mut _);
                    let now = crate::brightness::get(hwnd).map(|b| (b as i32 + if up { 5 } else { -5 }).clamp(0, 100) as u32);
                    let set = now.and_then(|v| crate::brightness::set(hwnd, v));
                    *BRIGHT_RESULT.lock().unwrap() = Some(set);
                    BRIGHT_BUSY.store(false, std::sync::atomic::Ordering::SeqCst);
                    crate::app::wake();
                });
            }
            _ => {}
        }
    }

    /// How loud the music is right now (0..1), from Windows' audio meter,
    /// quick to rise and slow to fall like a VU meter. Only read while the
    /// glow is drawn.
    fn beat_level(&mut self, t: f64) -> f32 {
        use windows::Win32::Media::Audio::*;
        use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
        if self.meter.as_ref().map_or(true, |m| t - m.1 > 30.0) {
            // (Re)open now and then: the output device may have changed.
            let m: Option<windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation> = unsafe {
                CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
                    .ok()
                    .and_then(|e| e.GetDefaultAudioEndpoint(eRender, eConsole).ok())
                    .and_then(|d| d.Activate(CLSCTX_ALL, None).ok())
            };
            self.meter = m.map(|m| (m, t));
        }
        let peak = self.meter.as_ref().and_then(|m| unsafe { m.0.GetPeakValue().ok() }).unwrap_or(0.0).clamp(0.0, 1.0);
        // Loudness feels logarithmic: lift the quiet part.
        let peak = peak.sqrt();
        self.beat = if peak > self.beat { self.beat + (peak - self.beat) * 0.6 } else { self.beat * 0.9 + peak * 0.1 };
        self.beat
    }

    /// Middle click on the closed notch.
    fn middle_click(&mut self) {
        if self.mode() == Mode::Expanded {
            return;
        }
        if self.settings.extra.get("mouse_middle").map_or(true, |v| v == "play") {
            if let Some(i) = self.modules.iter().position(|m| m.live_id() == "music") {
                self.call(i, |m, c| m.action(1, (0.0, 0.0), c));
            }
        }
    }

    /// A second click on the closed notch soon after the first.
    fn double_click(&mut self) -> bool {
        let what = self.settings.extra.get("mouse_double").cloned().unwrap_or_else(|| "off".into());
        let id = match what.as_str() {
            "shot" => 3,
            "clip" => 2,
            "note" => 4,
            _ => return false,
        };
        self.expanded = false;
        self.on_hotkey(id);
        true
    }

    /// Show the welcome tour (first run, or "Welcome tour" in the tray menu).
    pub fn start_onboarding(&mut self) {
        self.onboarding = Some(0);
        self.expanded = true;
        self.toast = None;
        self.request_frame();
    }

    fn finish_onboarding(&mut self) {
        self.onboarding = None;
        self.settings.onboarded = true;
        self.settings.extra.insert("tour_version".into(), TOUR_VERSION.to_string());
        self.settings.save();
        self.expanded = false;
        self.leave_at = None;
        self.push_toast(
            Toast::new("welcome", glyph::SETTINGS, col::ACCENT, "You're all set", "Settings anytime: click the gear or right-click the notch")
                .secs(7.0),
        );
        self.request_frame();
    }

    fn onboarding_action(&mut self, id: u32) {
        let Some(step) = self.onboarding else { return };
        match id {
            O_NEXT if step + 1 >= ONBOARD_STEPS => self.finish_onboarding(),
            O_NEXT => self.onboarding = Some(step + 1),
            O_SETTINGS => {
                self.finish_onboarding();
                self.open_settings();
            }
            O_BACK => self.onboarding = Some(step.saturating_sub(1)),
            O_SKIP => self.finish_onboarding(),
            i if (O_STYLE..O_STYLE + NotchStyle::ALL.len() as u32).contains(&i) => {
                self.settings.style = NotchStyle::ALL[(i - O_STYLE) as usize];
                self.settings.save();
            }
            i if (O_EDGE..O_EDGE + Edge::ALL.len() as u32).contains(&i) => {
                self.set_edge(Edge::ALL[(i - O_EDGE) as usize], true);
            }
            i if (O_THEME..O_THEME + TOUR_THEMES.len() as u32).contains(&i) => {
                self.settings.theme = crate::gfx::THEMES[TOUR_THEMES[(i - O_THEME) as usize]].key.to_string();
                self.backdrop = None;
                self.settings.save();
            }
            i if (O_PRESET..O_PRESET + crate::features::PRESETS.len() as u32).contains(&i) => {
                crate::features::apply_preset(&mut self.settings, (i - O_PRESET) as usize);
                self.settings.save();
            }
            _ => {}
        }
        self.request_frame();
    }

    /// The welcome tour: 1. how the closed notch looks, 2. where it sits,
    /// 3. how to use it. Every choice applies straight away.
    fn draw_onboarding(&mut self, goal: Rect, t: f64) {
        let Some(step) = self.onboarding else { return };
        let mouse = self.mouse;
        let st = self.settings.clone();
        let accent = col::ACCENT;
        let mut r = self.content_rect(goal, 10.0, PAD, PAD + 4.0);
        let tour_icons: Vec<(&'static str, &'static str)> = self.modules.iter().filter(|m| m.has_tab()).map(|m| (m.title(), m.icon())).collect();
        let mut ui = Ui { g: &mut self.gfx, hits: &mut self.hits, module: ONBOARD, mouse, t };

        // Header: title, step dots, Skip.
        let mut head = r.cut_top(24.0, 4.0);
        ui.button_c(head.cut_right(60.0, 0.0), "Skip", O_SKIP, col::BTN, col::DIM);
        let dots = head.cut_right(12.0 * ONBOARD_STEPS as f32 + 18.0, 8.0);
        for k in 0..ONBOARD_STEPS {
            let on = k == step;
            let c = if on { accent } else { col::TEXT.with_a(0.25) };
            let w = if on { 16.0 } else { 6.0 };
            let x = dots.x + k as f32 * 12.0 + if k > step { 10.0 } else { 0.0 };
            ui.fill_rr(Rect::new(x, dots.cy() - 3.0, w, 6.0), 3.0, c);
        }
        ui.text("Welcome to Nookly", head, 15.0, col::TEXT, Weight::Semi, Align::Left);
        let (question, hint) = match step {
            0 => ("Hi! I'm Nookly, your new notch.", "Let's make it yours. It takes about 30 seconds."),
            1 => ("How should it look, and where should it sit?", "Right-drag it to any edge of any screen later too."),
            2 => ("Pick a colour.", "Ten themes, Liquid Glass and your own in Settings > Theme."),
            3 => ("What do you want in it?", "Only what you pick runs. Change it anytime in Settings > Features."),
            _ => ("Four things to know, and you're done.", "Enjoy! Everything is in Settings."),
        };
        ui.text(question, r.cut_top(20.0, 2.0), 12.5, col::DIM, Weight::Regular, Align::Left);
        let mut foot = r.cut_bottom(28.0, 0.0);
        let next = if step + 1 >= ONBOARD_STEPS { "Start using Nookly" } else { "Next" };
        let nw = ui.measure(next, 12.0, Weight::Semi) + 40.0;
        ui.button_accent(foot.cut_right(nw, 8.0), next, O_NEXT, accent);
        if step + 1 >= ONBOARD_STEPS {
            ui.button(foot.cut_right(110.0, 8.0), "Open Settings", O_SETTINGS);
        }
        if step > 0 {
            ui.button(foot.cut_right(70.0, 8.0), "Back", O_BACK);
        }
        ui.text(hint, foot, 11.0, col::FAINT, Weight::Regular, Align::Left);
        let body = r.inset_xy(0.0, 6.0);

        match step {
            0 => {
                // Hello: the mark, and what it's about.
                let mut b = body;
                let mark = b.cut_left(150.0, 24.0).centered(104.0, 104.0);
                draw_logo(&mut ui, mark);
                let points: [(&str, &str, &str); 3] = [
                    ("\u{E8A9}", "One hover away", "Music, calendar, notes, timers, widgets and more."),
                    ("\u{E945}", "Light and quick", "Only what you switch on runs, so it costs almost nothing."),
                    ("\u{E72E}", "Yours alone", "No account, no ads. Your data stays on this PC."),
                ];
                for (cell, (g, title, sub)) in b.rows(3, 8.0).into_iter().zip(points) {
                    let mut c = cell;
                    let ic = c.cut_left(30.0, 10.0);
                    ui.circle(ic.cx(), ic.cy(), 15.0, accent.with_a(0.18));
                    ui.icon(g, ic, 12.0, accent);
                    ui.text(title, c.cut_top(c.h / 2.0, 0.0), 13.0, col::TEXT, Weight::Semi, Align::Left);
                    ui.text(sub, c, 11.0, col::DIM, Weight::Regular, Align::Left);
                }
            }
            2 => {
                // A tile per theme, with a little notch in it.
                for (k, cell) in body.centered(body.w, 74.0).columns(TOUR_THEMES.len(), 8.0).into_iter().enumerate() {
                    let th = crate::gfx::THEMES[TOUR_THEMES[k]];
                    let sel = st.theme == th.key;
                    let saved = ui.theme;
                    ui.theme = crate::gfx::THEMES[0];
                    let hov = ui.hovered(cell);
                    ui.fill_rr(cell, 12.0, if sel { accent.with_a(0.2) } else if hov { col::BTN_HOVER } else { col::BTN });
                    if sel {
                        ui.stroke_rr(cell, 12.0, accent.with_a(0.85), 1.4);
                    }
                    let mut c = cell.inset(7.0);
                    let screen = c.cut_top(38.0, 4.0);
                    if th.glass {
                        for (j, colr) in [0x7B5CFF, 0xFF8A5B, 0x3CC8C8].into_iter().enumerate() {
                            let w = screen.w / 3.0;
                            ui.fill_rr_raw(Rect::new(screen.x + j as f32 * w, screen.y, w + 0.5, screen.h), 0.0, hex(colr));
                        }
                    } else {
                        ui.fill_rr_raw(screen, 6.0, hex(0x4A4A52));
                    }
                    let n = Rect::new(screen.cx() - 22.0, screen.y + 4.0, 44.0, 15.0);
                    ui.fill_rr_raw(n, 7.0, th.bg);
                    ui.theme = th;
                    ui.text("4:20", n, 8.5, col::TEXT, Weight::Semi, Align::Center);
                    ui.theme = saved;
                    ui.text(th.name, c, 11.0, if sel { col::TEXT } else { col::DIM }, Weight::Semi, Align::Center);
                    ui.hit_tip(cell, O_THEME + k as u32, "");
                }
            }
            1 => {
                // A tile per style, each with a tiny screen showing it, and
                // the edge to put it on underneath.
                let mut body = body;
                let edges = body.cut_bottom(26.0, 8.0);
                for (i, (cell, e)) in edges.columns(4, 8.0).into_iter().zip(Edge::ALL).enumerate() {
                    ui.chip(cell, e.label(), O_EDGE + i as u32, st.edge == e, accent);
                }
                for (i, (cell, s)) in body.columns(NotchStyle::ALL.len(), 10.0).into_iter().zip(NotchStyle::ALL).enumerate() {
                    style_tile(&mut ui, cell, s, st.style == s, O_STYLE + i as u32);
                }
            }
            3 => {
                // A card per preset, with the icons of what it switches on.
                let picked = crate::features::current_preset(&st);
                for (k, cell) in body.columns(crate::features::PRESETS.len(), 10.0).into_iter().enumerate() {
                    let (name, what, list) = crate::features::PRESETS[k];
                    let sel = picked == Some(k);
                    let hov = ui.hovered(cell);
                    ui.fill_rr(cell, 12.0, if sel { accent.with_a(0.2) } else if hov { col::BTN_HOVER } else { col::BTN });
                    if sel {
                        ui.stroke_rr(cell, 12.0, accent.with_a(0.85), 1.4);
                    }
                    let mut c = cell.inset(10.0);
                    ui.text(name, c.cut_top(18.0, 2.0), 13.0, if sel { col::TEXT } else { col::DIM }, Weight::Semi, Align::Left);
                    let mut icons = c.cut_bottom(16.0, 0.0);
                    for &(key, icon) in &tour_icons {
                        if list.contains(&key) && icons.w >= 16.0 {
                            ui.icon(icon, icons.cut_left(14.0, 3.0), 10.5, if sel { accent } else { col::DIM });
                        }
                    }
                    ui.text_wrap(what, c, 10.5, col::DIM, Weight::Regular, Align::Left);
                    ui.hit_tip(cell, O_PRESET + k as u32, "");
                }
            }
            _ => {
                let cards: &[(&str, &str, &str)] = &TOUR_TIPS;
                for (cell, &(g, title, sub)) in body.columns(cards.len(), 10.0).into_iter().zip(cards) {
                    ui.fill_rr(cell, 12.0, col::BTN);
                    let mut c = cell.inset(10.0);
                    let ic = c.cut_top(26.0, 6.0);
                    ui.circle(ic.x + 13.0, ic.cy(), 13.0, accent.with_a(0.18));
                    ui.icon(g, Rect::new(ic.x, ic.y, 26.0, 26.0), 12.0, accent);
                    ui.text(title, c.cut_top(18.0, 2.0), 12.5, col::TEXT, Weight::Semi, Align::Left);
                    ui.text_wrap(sub, c, 11.0, col::DIM, Weight::Regular, Align::Left);
                }
            }
        }
    }

    fn draw_toast(&mut self, goal: Rect, t: f64) {
        let Some((toast, _)) = self.toast.clone() else { return };
        let mouse = self.mouse;
        let mut r = self.content_rect(goal, 8.0, 10.0, 18.0);
        // App icon (e.g. of a Windows notification), cached as a bitmap.
        let bmp = toast.image.as_ref().and_then(|img| {
            let key = std::sync::Arc::as_ptr(img) as usize;
            match &self.toast_bmp {
                Some((k, g, b)) if *k == key && *g == self.gfx.generation => Some(b.clone()),
                _ => {
                    let b = self.gfx.make_bitmap(img.0, img.1, &img.2)?;
                    self.toast_bmp = Some((key, self.gfx.generation, b.clone()));
                    Some(b)
                }
            }
        });
        let mut ui = Ui { g: &mut self.gfx, hits: &mut self.hits, module: TOAST, mouse, t };
        let mut top = r.cut_top(44.0, 6.0);
        if toast.click.is_some() {
            ui.hit_tip(top, TOAST_BODY, "");
        }
        let ic = top.cut_left(36.0, 12.0);
        match &bmp {
            Some(b) => ui.g.image_rr(b, ic.centered(34.0, 34.0), 9.0),
            None => {
                ui.circle(ic.cx(), ic.cy(), 18.0, toast.color.with_a(0.18));
                ui.icon(toast.icon, ic, 16.0, toast.color);
            }
        }
        if !toast.right.is_empty() {
            let rw = ui.measure(&toast.right, 16.0, Weight::Semi) + 4.0;
            let rr = top.cut_right(rw, 8.0);
            ui.text(&toast.right, rr, 16.0, toast.color, Weight::Semi, Align::Right);
        }
        let mut txt = top;
        if let Some(p) = toast.progress {
            let pr = txt.cut_bottom(8.0, 0.0);
            ui.progress(pr.inset_xy(0.0, 2.0), p, toast.color, None);
        }
        let tt = txt.cut_top(txt.h * 0.55, 0.0);
        ui.text(&toast.title, tt, 14.0, col::TEXT, Weight::Semi, Align::Left);
        ui.text(&toast.sub, txt, 12.0, col::DIM, Weight::Regular, Align::Left);
        if !toast.actions.is_empty() {
            let row = r.cut_top(28.0, 0.0);
            let n = toast.actions.len();
            for (k, (cell, (label, _))) in row.columns(n, 8.0).into_iter().zip(toast.actions.iter()).enumerate() {
                if k == 0 {
                    ui.button_accent(cell, label, k as u32, toast.color);
                } else {
                    ui.button(cell, label, k as u32);
                }
            }
        }
    }

    /// Pill and Hidden styles: an invisible hover strip along the edge, plus
    /// (Pill) a thin bar that fades out as the notch opens.
    fn draw_bar(&mut self, along: f32, depth: f32) {
        let pill = self.look() == NotchStyle::Pill;
        // Alpha of ~1% is invisible but still catches the mouse
        // (fully transparent layered-window pixels are click-through).
        if self.dock_x.is_some() {
            return;
        }
        let (za, zd) = if pill { (200.0, 14.0) } else { (280.0, 3.0) };
        let zone = self.edge_rect(za, zd);
        self.gfx.fill_rect(zone, col::NOTCH.with_a(0.012));
        self.hits.insert(0, Hit { rect: zone, module: APP_HIT, id: u32::MAX, tip: "" });
        let fade = (1.0 - depth / 18.0).clamp(0.0, 1.0);
        if !pill || fade <= 0.0 {
            return;
        }
        let first = match &self.notch_shown {
            Some(Shown::Wide(it)) => Some(it.clone()),
            Some(Shown::Pair(l, r)) => l.clone().or_else(|| r.clone()),
            None => None,
        };
        let color = match first {
            Some(ShownItem::Live(i)) => self.modules[i].live_color(),
            Some(ShownItem::W(w, _)) => w.color,
            None => col::TEXT.with_a(0.6),
        };
        // A 5 px bar, 4 px away from the edge.
        let slot = self.edge_rect(along, 9.0);
        let bar = match self.edge() {
            Edge::Top => Rect::new(slot.x, slot.y + 4.0, slot.w, 5.0),
            Edge::Bottom => Rect::new(slot.x, slot.y, slot.w, 5.0),
            Edge::Left => Rect::new(slot.x + 4.0, slot.y, 5.0, slot.h),
            Edge::Right => Rect::new(slot.x, slot.y, 5.0, slot.h),
        };
        self.gfx.alpha = fade;
        // Faint dark rim so the bar also shows on white title bars.
        self.gfx.fill_rr(bar.inset(-1.0), 3.5, col::NOTCH.with_a(0.35));
        self.gfx.fill_rr(bar, 2.5, color);
        self.gfx.alpha = 1.0;
    }

    fn draw_collapsed(&mut self, goal: Rect, t: f64) {
        let mouse = self.mouse;
        let compact = self.look() == NotchStyle::Compact;
        let vertical = self.edge().vertical();
        let side = self.mascot_side();
        let mood_t = (t - self.mood_since) as f32;
        let fade = (mood_t / 0.6).clamp(0.0, 1.0);
        if let Some(mut shown) = self.notch_shown.clone() {
            // Slim: the wings fill what the clock leaves (a centred mascot
            // takes the clock's place), so the clock stays in the exact
            // middle and the sides keep their room.
            let slim_mid = (self.look() == NotchStyle::Slim && !vertical).then(|| self.gfx.measure(&Now::get().hhmm(), 14.0, Weight::Semi) + 2.0);
            // Slim with a side mascot and two one-side items: the mascot
            // takes a side instead of widening the notch.
            let in_wing = slim_mid.is_some() && matches!(side, Some(0 | 2)) && matches!(shown, Shown::Pair(..));
            // Otherwise a side mascot takes a strip of the (wider) notch; a
            // centred one sits between the two wings.
            let mut goal = goal;
            let mut mascot_r = None;
            if let Some(s) = side.filter(|s| *s != 1 && !in_wing) {
                let strip = SLIM_MASCOT_STRIP;
                mascot_r = Some(if s == 0 {
                    Rect::new(goal.x + 10.0, goal.y + 2.0, strip - 4.0, goal.h - 4.0)
                } else {
                    Rect::new(goal.r() - strip - 6.0, goal.y + 2.0, strip - 4.0, goal.h - 4.0)
                });
                goal = Rect::new(goal.x + if s == 0 { strip } else { 0.0 }, goal.y, (goal.w - strip).max(0.0), goal.h);
            }
            let (left, right) = if vertical {
                // Icon/art on top, time below (Compact: icon only).
                let s = if compact { 16.0 } else { 20.0 };
                let top = Rect::new(goal.cx() - s / 2.0, goal.y + 10.0, s, s);
                let bottom = if compact { Rect::default() } else { Rect::new(goal.x + 2.0, goal.b() - 30.0, goal.w - 4.0, 18.0) };
                (top, bottom)
            } else if let Some(mid) = slim_mid {
                let w = (goal.w / 2.0 - mid / 2.0 - SLIM_GAP - SLIM_WING_PAD).max(0.0);
                (
                    Rect::new(goal.x + SLIM_WING_PAD, goal.y + 2.0, w, goal.h - 4.0),
                    Rect::new(goal.r() - SLIM_WING_PAD - w, goal.y + 2.0, w, goal.h - 4.0),
                )
            } else {
                let slim = self.look() == NotchStyle::Slim;
                let (wing, pad) = if compact { (68.0, 10.0) } else if slim { (72.0, 10.0) } else { (92.0, 12.0) };
                (
                    Rect::new(goal.x + pad, goal.y + 2.0, wing, goal.h - 4.0),
                    Rect::new(goal.r() - pad - wing, goal.y + 2.0, wing, goal.h - 4.0),
                )
            };
            if side == Some(1) && !vertical && right.x - left.r() > 30.0 {
                mascot_r = Some(Rect::new(left.r() + 2.0, goal.y + 2.0, right.x - left.r() - 4.0, goal.h - 4.0));
            }
            if in_wing {
                let on_left = side == Some(0);
                mascot_r = Some(if on_left { left } else { right });
                if let Shown::Pair(l, r) = &mut shown {
                    // The item on the mascot's side moves to the free side,
                    // or (both taken) the second one waits.
                    let (keep, other) = if on_left { (l.take(), r.take()) } else { (r.take(), l.take()) };
                    let item = other.or(keep);
                    if on_left {
                        *r = item;
                    } else {
                        *l = item;
                    }
                }
            }
            // Live wings are display-only; clicking anywhere expands.
            let n_hits = self.hits.len();
            self.draw_shown(&shown, left, right, vertical, compact, t);
            self.hits.truncate(n_hits);
            let mut ui = Ui { g: &mut self.gfx, hits: &mut self.hits, module: APP_HIT, mouse, t };
            // Clock in the middle, as on the idle notch (music: art left,
            // equaliser right; a timer: ring left, time left right), when
            // there is room between the wings.
            let size = if compact { 11.0 } else { 14.0 };
            let time = Now::get().hhmm();
            // A centred mascot takes the middle instead.
            let centred = side == Some(1) && mascot_r.is_some();
            let tw = ui.measure(&time, size, Weight::Semi) + 4.0;
            if !centred && !vertical && right.x - left.r() >= tw {
                // Always the middle of the notch (the wings are symmetric).
                let cx = goal.cx();
                let mid = Rect::new(cx - tw / 2.0, goal.y + 2.0, tw, goal.h - 4.0);
                ui.text(&time, mid, size, col::TEXT, Weight::Semi, Align::Center);
            }
            if vertical {
                // Side edges: hours over minutes between the top item (art,
                // ring, icon) and the bottom one (wave, time left, value).
                let bottom = if right.h > 0.0 { right.y } else { goal.b() - 6.0 };
                let mid = Rect::new(goal.x + 2.0, left.b() + 2.0, goal.w - 4.0, (bottom - left.b() - 4.0).max(0.0));
                stacked_clock(&mut ui, mid, size, self.settings.seconds);
            }
            if let Some(mr) = mascot_r {
                ui.alpha = fade;
                draw_mascot(&mut ui, mr, self.settings.mascot, self.mood, t, self.origin, self.scale);
                ui.alpha = 1.0;
            }
            self.mascot_rect = mascot_r;
            return;
        }
        let mut ui = Ui { g: &mut self.gfx, hits: &mut self.hits, module: APP_HIT, mouse, t };
        let area = if vertical { goal.inset_xy(2.0, 14.0) } else { goal.inset_xy(14.0, 2.0) };
        self.mascot_rect = None;
        // Mascot at the left or right with the clock beside it.
        if let Some(s) = side.filter(|s| *s != 1) {
            let mut rest = area;
            let mr = if s == 0 { rest.cut_left(64.0, 4.0) } else { rest.cut_right(64.0, 4.0) };
            let size = if compact { 11.0 } else { 14.0 };
            ui.text(&Now::get().hhmm(), rest, size, col::TEXT, Weight::Semi, Align::Center);
            ui.alpha = fade;
            draw_mascot(&mut ui, mr, self.settings.mascot, self.mood, t, self.origin, self.scale);
            ui.alpha = 1.0;
            self.mascot_rect = Some(mr);
            return;
        }
        match self.mood {
            Mood::Clock => {
                let now = Now::get();
                let size = if compact { 11.0 } else { 14.0 };
                if vertical {
                    stacked_clock(&mut ui, area, size, self.settings.seconds);
                } else {
                    ui.text(&now.hhmm(), area, size, col::TEXT, Weight::Semi, Align::Center);
                }
            }
            m => {
                ui.alpha = fade;
                draw_mascot(&mut ui, area, self.settings.mascot, m, t, self.origin, self.scale);
                ui.alpha = 1.0;
                self.mascot_rect = Some(area);
            }
        }
    }

    // --------------------------------------------------------------- input

    fn hit_at(&self, x: f32, y: f32) -> Option<Hit> {
        self.hits.iter().rev().find(|h| h.rect.contains(x, y)).copied()
    }

    fn on_mouse_move(&mut self, x: f32, y: f32) {
        self.mouse = (x, y);
        if !self.hover {
            self.hover = true;
            unsafe {
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: self.hwnd,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut tme);
            }
            // A leave/enter blip (cursor jittering on a thin edge strip) keeps
            // the hover timer running instead of restarting it.
            if self.t() - self.last_leave > 0.2 {
                self.hover_since = self.t();
            }
            if self.can_hover_open() && self.look().hover_delay() <= 0.0 {
                self.open_from_hover();
            }
        }
        self.leave_at = None;
        // Dragging inside a panel (the Widgets tab's order list)?
        if let Some(m) = self.inner_drag {
            self.modules[m].drag_to(x, y);
            self.request_frame();
            return;
        }
        // Dragging something out of a module?
        if let Some((hit, p0)) = self.press {
            let moved = (x - p0.0).abs() + (y - p0.1).abs();
            if moved > 5.0 && hit.module < self.modules.len() && self.modules[hit.module].drag_begin(hit.id, p0.0, p0.1) {
                self.modules[hit.module].drag_to(x, y);
                self.press = None;
                self.inner_drag = Some(hit.module);
                unsafe {
                    SetCapture(self.hwnd);
                }
                self.request_frame();
                return;
            }
            if moved > 6.0 && hit.module < self.modules.len() {
                if let Some(paths) = self.modules[hit.module].drag_out(hit.id) {
                    self.press = None;
                    self.deferred.push(Deferred::DragOut(paths));
                }
            }
        }
        self.request_frame();
    }

    /// Hovering opens the panel unless that would cover something the user is
    /// about to click: a toast with buttons stays put while hovered, and after
    /// a toast button was clicked the panel waits until the mouse leaves once.
    fn can_hover_open(&self) -> bool {
        let toast_buttons = self.toast.as_ref().map_or(false, |(t, _)| !t.actions.is_empty())
            || (self.toast.is_none() && self.queue.front().map_or(false, |t| !t.actions.is_empty()));
        !self.expanded && !self.dragging && !self.hover_block && !toast_buttons
    }

    fn open_from_hover(&mut self) {
        self.expanded = true;
        self.raise();
        self.last_activity = self.t();
        if self.mood != Mood::Clock {
            self.mood = Mood::Clock;
            self.mood_since = self.t();
        }
        self.request_frame();
    }

    fn on_mouse_leave(&mut self) {
        if self.inner_drag.is_some() {
            return; // the mouse is captured until the drop
        }
        dlog("leave");
        self.hover = false;
        self.hover_block = false;
        self.tip = None;
        self.last_leave = self.t();
        self.mouse = (-1e4, -1e4);
        self.press = None;
        self.leave_at = Some(self.t() + 0.35);
        self.request_frame();
    }

    fn on_down(&mut self, x: f32, y: f32) {
        self.press = self.hit_at(x, y).map(|h| (h, (x, y)));
        // A click hides the tooltip until the mouse moves to another hit.
        if let Some(tip) = &mut self.tip {
            tip.3 = true;
        }
        dlog(format!(
            "down ({x:.0},{y:.0}) mode={} tab={} hits={} -> {:?}",
            self.mode() as u8,
            self.tab,
            self.hits.len(),
            self.press.map(|(h, _)| (h.module, h.id))
        ));
    }

    fn on_up(&mut self, x: f32, y: f32) {
        if let Some(m) = self.inner_drag.take() {
            unsafe {
                let _ = ReleaseCapture();
            }
            self.call(m, |md, c| md.drag_drop(x, y, c));
            self.request_frame();
            return;
        }
        dlog(format!("up ({x:.0},{y:.0}) press={:?}", self.press.map(|(h, _)| (h.module, h.id))));
        let Some((hit, _)) = self.press.take() else { return };
        if !hit.rect.contains(x, y) {
            return;
        }
        let rel = ((x - hit.rect.x) / hit.rect.w.max(1.0), (y - hit.rect.y) / hit.rect.h.max(1.0));
        match hit.module {
            APP_HIT => {
                if hit.id == u32::MAX {
                    // Background click: expand (on the panel of what the
                    // closed notch shows), or drop keyboard focus.
                    let now = self.t();
                    let quick = now - self.last_notch_click < 0.35;
                    self.last_notch_click = if quick { 0.0 } else { now };
                    if quick && self.double_click() {
                        return;
                    }
                    if !self.expanded {
                        if let Some(m) = self.shown_owner() {
                            if self.tab_list().contains(&m) {
                                self.tab = m;
                            }
                        }
                    }
                    self.expanded = true;
                    if let Some(k) = self.keyboard {
                        self.modules[k].blur();
                        self.release_keyboard();
                    }
                } else if hit.id == GEAR {
                    self.open_settings();
                } else if hit.id == MORE {
                    self.more_open = !self.more_open;
                } else {
                    self.tab = hit.id as usize;
                    self.remember_tab();
                    self.more_open = false;
                    if let Some(k) = self.keyboard {
                        self.modules[k].blur();
                        self.release_keyboard();
                    }
                }
            }
            SETTINGS => self.settings_action(hit.id),
            ONBOARD => self.onboarding_action(hit.id),
            TOAST => {
                // The mouse is still on the notch: don't let hover turn the
                // collapsing toast straight into the full panel.
                self.hover_block = true;
                self.expanded = false;
                if let Some((toast, _)) = self.toast.take() {
                    if hit.id == TOAST_BODY {
                        if let Some(id) = toast.click {
                            self.call(toast.module, |m, c| m.action(id, (0.0, 0.0), c));
                        }
                    } else if let Some((_, id)) = toast.actions.get(hit.id as usize) {
                        let id = *id;
                        self.call(toast.module, |m, c| m.action(id, (0.0, 0.0), c));
                    }
                }
            }
            i if i < self.modules.len() => {
                self.call(i, |m, c| m.action(hit.id, rel, c));
            }
            _ => {}
        }
        self.request_frame();
    }

    /// Mouse wheel over the open notch: a list that can scroll scrolls;
    /// anywhere else (and always over the tab bar) it switches panels, one
    /// per notch of the wheel. Down / toward you = the next tab.
    fn on_wheel(&mut self, delta: f32) {
        if self.mode() != Mode::Expanded && self.onboarding.is_none() {
            self.wheel_closed(delta);
            return;
        }
        if self.onboarding.is_some() {
            return;
        }
        self.request_frame();
        let (mx, my) = self.mouse;
        let over_bar = self.tab_bar.inset_xy(-8.0, -6.0).contains(mx, my);
        // Typing in a panel: the wheel scrolls it (Notes), never switches.
        if let Some(k) = self.keyboard {
            self.modules[k].scroll(delta);
            return;
        }
        if !over_bar && self.modules[self.tab].scroll(delta) {
            self.wheel_acc = 0.0;
            return;
        }
        let t = self.t();
        // A new gesture starts fresh; a fast flick moves one tab at a time
        // (the rest of the flick is dropped).
        if t - self.wheel_last > 0.6 {
            self.wheel_acc = 0.0;
        }
        self.wheel_last = t;
        if t - self.wheel_t < 0.18 {
            self.wheel_acc = 0.0;
            return;
        }
        self.wheel_acc += delta;
        if self.wheel_acc.abs() < 0.99 {
            return;
        }
        let step = if self.wheel_acc < 0.0 { 1 } else { -1 };
        self.wheel_acc = 0.0;
        self.wheel_t = t;
        self.step_tab(step);
    }

    /// Save the panel you picked, so the notch opens on it next time (also
    /// after a restart).
    fn remember_tab(&mut self) {
        let title = self.modules[self.tab].title();
        if self.settings.last_tab != title {
            self.settings.last_tab = title.to_string();
            self.settings.save();
        }
    }

    /// The remembered panel, if it still has a visible tab.
    fn remembered_tab(&self) -> Option<usize> {
        let tabs = self.tab_list();
        tabs.iter().copied().find(|&i| self.modules[i].title() == self.settings.last_tab)
    }

    /// Move to the next (+1) or previous (-1) visible tab; stops at the ends.
    fn step_tab(&mut self, step: i32) {
        let tabs = self.tab_list();
        let Some(pos) = tabs.iter().position(|&i| i == self.tab) else { return };
        let next = pos as i32 + step;
        if next >= 0 && (next as usize) < tabs.len() {
            let i = tabs[next as usize];
            self.modules[self.tab].blur();
            self.more_open = false;
            self.tab = i;
            self.remember_tab();
            self.leave_at = None;
        }
    }

    fn on_key(&mut self, vk: u16) -> bool {
        let Some(k) = self.keyboard else { return false };
        let ctrl = unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0;
        self.call(k, |m, c| m.key(vk, ctrl, c))
    }

    fn on_char(&mut self, ch: char) -> bool {
        let Some(k) = self.keyboard else { return false };
        self.call(k, |m, c| m.chr(ch, c))
    }

    fn on_blur(&mut self) {
        self.stop_recording();
        if let Some(k) = self.keyboard {
            self.modules[k].blur();
            self.release_keyboard();
            self.request_frame();
        }
    }

    // --------------------------------------------------- drag & drop (in)

    pub fn drag_enter(&mut self) {
        if self.dragging_out {
            return;
        }
        self.drag_files = true;
        // An open tab that takes drops (Launcher) keeps them; otherwise
        // the first one that's on (Shelf).
        let keep = self.expanded && self.modules[self.tab].wants_drops() && self.enabled(self.tab);
        if !keep {
            if let Some(i) = (0..self.modules.len()).find(|&i| self.modules[i].wants_drops() && self.modules[i].has_tab() && self.enabled(i)) {
                self.tab = i;
            }
        }
        for m in self.modules.iter_mut() {
            m.drag_hover(true);
        }
        self.request_frame();
    }

    /// Files are being dragged over the notch at screen point (x, y).
    pub fn drag_over(&mut self, x: i32, y: i32) {
        let m = ((x - self.origin.0) as f32 / self.scale, (y - self.origin.1) as f32 / self.scale);
        if m != self.mouse {
            self.mouse = m;
            self.request_frame();
        }
    }

    pub fn drag_leave(&mut self) {
        self.drag_files = false;
        for m in self.modules.iter_mut() {
            m.drag_hover(false);
        }
        self.leave_at = Some(self.t() + 0.5);
        self.request_frame();
    }

    pub fn drop_paths(&mut self, paths: Vec<String>) {
        self.drag_leave();
        if self.dragging_out || paths.is_empty() {
            return;
        }
        *crate::modules::DROP_AT.lock().unwrap() = self.mouse;
        let order: Vec<usize> = std::iter::once(self.tab).chain(0..self.modules.len()).collect();
        for i in order {
            if self.modules[i].wants_drops() || i == self.tab {
                let p = paths.clone();
                if self.call(i, |m, c| m.drop_files(p, c)) {
                    let share = std::mem::take(&mut *crate::modules::shelf::SHARE_NOW.lock().unwrap());
                    if !share.is_empty() {
                        self.deferred.push(Deferred::Share(share));
                        break;
                    }
                    self.tab = i;
                    self.expanded = true;
                    self.leave_at = Some(self.t() + 2.0);
                    break;
                }
            }
        }
    }

    // ------------------------------------------------------- move the notch

    /// Right-drag: snap to the nearest edge of whichever monitor the cursor
    /// is on, and follow the cursor along it.
    fn on_right_drag(&mut self) {
        let Some(p0) = self.rdrag else { return };
        let mut p = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut p);
        }
        if !self.dragging {
            if (p.x - p0.x).abs() + (p.y - p0.y).abs() < 8 {
                return;
            }
            self.dragging = true;
            self.expanded = false;
            self.toast = None;
        }
        self.move_to_point(p);
    }

    pub fn move_to_point(&mut self, p: POINT) {
        self.undock();
        unsafe {
            let mon = MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(mon, &mut mi);
            let r = mi.rcWork;
            let (w, h) = ((r.right - r.left).max(1) as f32, (r.bottom - r.top).max(1) as f32);
            let fx = ((p.x - r.left) as f32 / w).clamp(0.0, 1.0);
            let fy = ((p.y - r.top) as f32 / h).clamp(0.0, 1.0);
            // Nearest edge, measured in pixels.
            let d = [
                (Edge::Top, (p.y - r.top) as f32),
                (Edge::Bottom, (r.bottom - p.y) as f32),
                (Edge::Left, (p.x - r.left) as f32),
                (Edge::Right, (r.right - p.x) as f32),
            ];
            let edge = d.iter().min_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).map(|e| e.0).unwrap_or(Edge::Top);
            self.settings.edge = edge;
            self.settings.pos = if edge.vertical() { fy } else { fx };
            let mc = mi.rcMonitor;
            self.settings.monitor = Some(((mc.left + mc.right) / 2, (mc.top + mc.bottom) / 2));
        }
        self.place();
        self.request_frame();
    }

    fn end_drag(&mut self) {
        self.dragging = false;
        self.settings.save();
        dlog(format!("moved to {} at {:.3}", self.settings.edge.key(), self.settings.pos));
        self.request_frame();
    }

    /// Tray "Position" menu: jump to an edge (keeping the position along it).
    pub fn set_edge(&mut self, edge: Edge, center: bool) {
        self.undock();
        self.settings.edge = edge;
        if center {
            self.settings.pos = 0.5;
        }
        self.settings.save();
        self.place();
        self.request_frame();
    }

    // ------------------------------------------------------------ messages

    fn handle(&mut self, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        let xy = || {
            let x = (lp.0 & 0xffff) as i16 as f32;
            let y = ((lp.0 >> 16) & 0xffff) as i16 as f32;
            (x / self.scale, y / self.scale)
        };
        match msg {
            WM_TIMER => {
                match wp.0 {
                    TIMER_TICK => self.tick(),
                    TIMER_FRAME => self.timer_frame(),
                    TIMER_POLL => self.poll_cursor(),
                    TIMER_GLASS => {
                        unsafe {
                            let _ = KillTimer(Some(self.hwnd), TIMER_GLASS);
                        }
                        if self.settings.theme == "glass" {
                            self.glass_dirty = true;
                            self.request_frame();
                        }
                    }
                    TIMER_MOVE => self.drag_tick(),
                    TIMER_GLASS_PROBE => self.glass_probe_tick(),
                    _ => {}
                }
                Some(LRESULT(0))
            }
            WM_APP_WAKE => {
                self.tick();
                Some(LRESULT(0))
            }

            WM_MOUSEMOVE => {
                if self.rdrag.is_some() {
                    self.on_right_drag();
                    return Some(LRESULT(0));
                }
                let (x, y) = xy();
                self.on_mouse_move(x, y);
                Some(LRESULT(0))
            }
            WM_RBUTTONDOWN => {
                let mut p = POINT::default();
                unsafe {
                    let _ = GetCursorPos(&mut p);
                    SetCapture(self.hwnd);
                }
                self.rdrag = Some(p);
                self.dragging = false;
                Some(LRESULT(0))
            }
            WM_CAPTURECHANGED => {
                if self.rdrag.take().is_some() && self.dragging {
                    self.end_drag();
                }
                // Lost the mouse mid-drag (Alt+Tab...): cancel, so the panel
                // doesn't stay open waiting for a drop that never comes.
                if let Some(m) = self.inner_drag.take() {
                    self.call(m, |md, c| md.drag_drop(-1e4, -1e4, c));
                    self.request_frame();
                }
                None
            }
            WM_MOUSELEAVE => {
                self.on_mouse_leave();
                Some(LRESULT(0))
            }
            WM_LBUTTONDOWN => {
                let (x, y) = xy();
                self.on_down(x, y);
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
                let (x, y) = xy();
                self.on_up(x, y);
                Some(LRESULT(0))
            }
            WM_RBUTTONUP => {
                // Plain right-click opens the menu; right-drag moves the notch.
                let was_drag = self.dragging;
                self.rdrag = None;
                unsafe {
                    let _ = ReleaseCapture();
                }
                if was_drag {
                    self.end_drag();
                } else {
                    // Plain right-click on the notch opens the settings panel.
                    self.open_settings();
                }
                Some(LRESULT(0))
            }
            WM_MOUSEWHEEL => {
                let d = ((wp.0 >> 16) & 0xffff) as i16 as f32 / 120.0;
                self.on_wheel(d);
                Some(LRESULT(0))
            }
            WM_SETCURSOR if (lp.0 & 0xffff) as u32 == HTCLIENT && crate::ui::over_text(self.mouse.0, self.mouse.1) => unsafe {
                if let Ok(c) = LoadCursorW(None, IDC_IBEAM) {
                    SetCursor(Some(c));
                }
                Some(LRESULT(1))
            },
            WM_MBUTTONUP => {
                self.middle_click();
                Some(LRESULT(0))
            }
            WM_HOTKEY => {
                self.on_hotkey(wp.0);
                Some(LRESULT(0))
            }
            WM_KEYDOWN | WM_SYSKEYDOWN if self.recording.is_some() => {
                self.record_key(wp.0 as u32);
                Some(LRESULT(0))
            }
            WM_KEYDOWN => {
                if self.on_key(wp.0 as u16) {
                    Some(LRESULT(0))
                } else {
                    None
                }
            }
            WM_CHAR => {
                if let Some(c) = char::from_u32(wp.0 as u32) {
                    if !c.is_control() && self.on_char(c) {
                        return Some(LRESULT(0));
                    }
                }
                None
            }
            WM_KILLFOCUS => {
                self.on_blur();
                None
            }
            WM_MOUSEACTIVATE => {
                if self.keyboard.is_some() {
                    None
                } else {
                    Some(LRESULT(MA_NOACTIVATE as isize))
                }
            }
            WM_DISPLAYCHANGE | WM_DPICHANGED | WM_SETTINGCHANGE => {
                self.place();
                self.request_frame();
                None
            }
            WM_APP_TRAY => {
                let ev = (lp.0 & 0xffff) as u32;
                if ev == WM_LBUTTONUP {
                    self.open_settings();
                } else if ev == WM_RBUTTONUP || ev == WM_CONTEXTMENU {
                    self.deferred.push(Deferred::TrayMenu);
                }
                Some(LRESULT(0))
            }
            WM_COMMAND => {
                tray::command(self, (wp.0 & 0xffff) as u32);
                Some(LRESULT(0))
            }
            m if m == tray::taskbar_created() => {
                tray::add(self.hwnd);
                None
            }
            _ => None,
        }
    }
}

// ---------------------------------------------------------------- self-test

fn snap(a: &mut App, label: &str) -> String {
    let tgt = a.target();
    a.cur = tgt;
    a.render(tgt, 0.0);
    let ok = a.gfx.last_err.is_none();
    format!(
        "{:<4} {label:<34} size={:>3.0}x{:<3.0} opaque_px={:<7} bright_px={:<6} hits={}\n",
        if ok { "OK" } else { "FAIL" },
        tgt.0,
        tgt.1,
        a.gfx.coverage(),
        a.gfx.bright(),
        a.hits.len()
    )
}

fn find(a: &App, title: &str) -> Option<usize> {
    a.modules.iter().position(|m| m.title() == title)
}

/// Exercise every view and the main flows without showing the window.
pub fn selftest() -> String {
    // Let the media thread find the current session first.
    std::thread::sleep(std::time::Duration::from_secs(3));
    with_app(|a| {
        let mut out = String::new();
        // The self-test runs on the user's PC: no chimes.
        sound::mute(true);
        // A fresh data folder is a first run: the welcome tour must be up.
        let fresh_tour = a.onboarding == Some(0) && a.mode() == Mode::Expanded;
        out += &format!("{:<4} first run opens the welcome tour\n", if fresh_tour { "OK" } else { "FAIL" });
        a.onboarding = None;
        a.tick();
        a.settings.personality = true;
        a.settings.style = NotchStyle::Full;
        a.queue.clear();
        a.toast = None;

        a.expanded = false;
        a.mood = Mood::Clock;
        let live = a.live_module().map(|i| a.modules[i].title()).unwrap_or("none");
        out += &snap(a, &format!("collapsed (live: {live})"));
        for (m, n) in [(Mood::Eyes, "mascot eyes"), (Mood::Doze, "mascot doze"), (Mood::Wave, "mascot wave")] {
            a.mood = m;
            a.mood_since = a.t() - 5.0;
            if a.live_module().is_none() {
                out += &snap(a, n);
            } else {
                out += &format!("SKIP {n:<34} (a live activity is showing)\n");
            }
        }
        a.mood = Mood::Clock;
        for s in [NotchStyle::Compact, NotchStyle::Pill, NotchStyle::Hidden] {
            a.settings.style = s;
            out += &snap(a, &format!("collapsed style {}", s.key()));
        }
        a.settings.style = NotchStyle::Full;

        // Edges: the drawn shape must touch the chosen screen side, and
        // panels must stay upright (wider than tall) on every edge.
        for e in Edge::ALL {
            a.settings.edge = e;
            a.anchor = if e.vertical() { CANVAS_H / 2.0 } else { CANVAS_W / 2.0 };
            for (style, open) in [(NotchStyle::Full, false), (NotchStyle::Pill, false), (NotchStyle::Full, true)] {
                a.settings.style = style;
                a.expanded = open;
                let label = format!("edge {} {}{}", e.key(), style.key(), if open { " open" } else { "" });
                let tgt = a.target();
                a.cur = tgt;
                a.render(tgt, 0.0);
                let (pw, ph) = (a.gfx.pw, a.gfx.ph);
                let ok = match a.gfx.bbox() {
                    Some((x0, y0, x1, y1)) => {
                        let touches = match e {
                            Edge::Top => y0 == 0,
                            Edge::Bottom => y1 == ph - 1,
                            Edge::Left => x0 == 0,
                            Edge::Right => x1 == pw - 1,
                        };
                        let upright = !open || (x1 - x0) > (y1 - y0);
                        touches && upright && a.gfx.last_err.is_none()
                    }
                    None => false,
                };
                out += &format!("{:<4} {label:<34} bbox={:?}\n", if ok { "OK" } else { "FAIL" }, a.gfx.bbox());
            }
        }
        a.expanded = false;
        a.settings.style = NotchStyle::Full;
        // Right-drag logic: a point near the middle of the primary monitor's
        // left side must snap to the left edge at about half height.
        unsafe {
            let mon = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(mon, &mut mi);
            let r = mi.rcWork;
            a.move_to_point(POINT { x: r.left + 5, y: (r.top + r.bottom) / 2 });
            let ok = a.settings.edge == Edge::Left && (a.settings.pos - 0.5).abs() < 0.02;
            out += &format!("{:<4} drag snaps to nearest edge (left, pos {:.2})\n", if ok { "OK" } else { "FAIL" }, a.settings.pos);
            a.move_to_point(POINT { x: r.right - 300, y: r.bottom - 2 });
            let ok = a.settings.edge == Edge::Bottom && a.settings.pos > 0.7;
            out += &format!("{:<4} drag snaps to bottom near right corner (pos {:.2})\n", if ok { "OK" } else { "FAIL" }, a.settings.pos);
        }
        a.settings.edge = Edge::Top;
        a.settings.pos = 0.5;
        a.settings.monitor = None;
        a.place();

        for i in 0..a.modules.len() {
            if !a.modules[i].has_tab() {
                continue;
            }
            a.tab = i;
            a.expanded = true;
            let t = a.modules[i].title();
            out += &snap(a, &format!("tab: {t}"));
        }

        // To-do: add a task through the real editor + keyboard path.
        if let Some(i) = find(a, "TodoList") {
            a.tab = i;
            a.call(i, |m, c| m.action(1, (0.0, 0.0), c));
            out += &snap(a, "todo editor open");
            a.call(i, |m, c| {
                m.action(10, (0.0, 0.0), c); // +15m
                for ch in "Selftest task".chars() {
                    m.chr(ch, c);
                }
                m.key(0x0D, false, c);
            });
            let saved = crate::store::read("todo.txt");
            let ok = saved.lines().any(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                f.len() > 4 && f[4] == "Selftest task" && f[2].parse::<f64>().unwrap_or(0.0) > 0.0
            });
            out += &format!("{:<4} todo saved to disk with due time\n", if ok { "OK" } else { "FAIL" });
            out += &snap(a, "todo list with task");
        }
        // Water: drink + settings page.
        if let Some(i) = find(a, "Water Reminder") {
            a.tab = i;
            a.call(i, |m, c| m.action(1, (0.0, 0.0), c));
            let saved = crate::store::read("water.txt");
            out += &format!("{:<4} water drink logged\n", if saved.contains("log ") { "OK" } else { "FAIL" });
            a.call(i, |m, c| m.action(2, (0.0, 0.0), c));
            out += &snap(a, "water settings page");
            a.call(i, |m, c| m.action(3, (0.0, 0.0), c));
        }
        // Shelf: drop a real file.
        if let Some(i) = find(a, "Shelf") {
            a.tab = i;
            let p = std::env::current_dir().unwrap_or_default().join("Cargo.toml").display().to_string();
            a.drop_paths(vec![p]);
            let saved = crate::store::read("shelf.txt");
            out += &format!("{:<4} shelf item stored\n", if saved.contains("Cargo.toml") { "OK" } else { "FAIL" });
            a.tab = i;
            a.expanded = true;
            out += &snap(a, "shelf with item");
            let drag = a.modules[i].drag_out(1 << 24);
            out += &format!("{:<4} shelf drag-out provides path\n", if drag.map_or(false, |d| d.len() == 1) { "OK" } else { "FAIL" });
        }
        // Clipboard history: limits, de-duplication, pins that persist, clear.
        if let Some(i) = find(a, "Clipboard") {
            use crate::modules::clipboard::{self as cb, Content};
            let field = |s: &str, k: &str| -> String {
                s.split_whitespace().find_map(|w| w.strip_prefix(&format!("{k}="))).unwrap_or("").to_string()
            };
            a.call(i, |m, c| m.action(cb::A_DEMO, (0.0, 0.0), c));
            a.call(i, |m, c| m.action(cb::A_DEMO, (0.0, 0.0), c));
            let st = a.modules[i].debug_state();
            let ok = field(&st, "unpinned") == "30" && field(&st, "dupes") == "0" && field(&st, "images") == "1";
            out += &format!("{:<4} clipboard keeps the newest 30, no duplicates ({st})\n", if ok { "OK" } else { "FAIL" });
            out += &format!("{:<4} clipboard history is off by default\n", if field(&st, "on") == "false" && field(&st, "mode") == "0" { "OK" } else { "FAIL" });
            let img = cb::image_from_dib(cb::make_test_dib(64, 32));
            let ok = img.as_ref().map_or(false, |im| im.w == 64 && im.h == 32 && im.thumb.0 == 64 && im.thumb.1 == 32 && im.thumb.2.len() == 64 * 32 * 4);
            out += &format!("{:<4} copied images get a thumbnail ({:?})\n", if ok { "OK" } else { "FAIL" }, img.map(|i| (i.thumb.0, i.thumb.1)));
            let big = cb::image_from_dib(cb::make_test_dib(400, 100)).map(|i| (i.thumb.0, i.thumb.1));
            out += &format!("{:<4} big images shrink to fit the thumbnail {:?}\n", if big == Some((80, 20)) { "OK" } else { "FAIL" }, big);
            let top: u32 = field(&st, "top_id").parse().unwrap_or(0);
            a.call(i, |m, c| m.action((2 << 24) | top, (0.0, 0.0), c)); // pin
            let saved = crate::store::read("clipboard.txt");
            let fresh = cb::Clipboard::new(a.hwnd).debug_state();
            let ok = saved.lines().count() == 1 && field(&fresh, "pinned") == "1";
            out += &format!("{:<4} pinned clipboard entry survives a restart\n", if ok { "OK" } else { "FAIL" });
            a.call(i, |m, c| m.action(1, (0.0, 0.0), c)); // clear
            let st = a.modules[i].debug_state();
            out += &format!("{:<4} clear keeps pinned entries ({})\n", if field(&st, "items") == "1" && field(&st, "pinned") == "1" { "OK" } else { "FAIL" }, field(&st, "items"));
            a.tab = i;
            a.expanded = true;
            out += &snap(a, "clipboard with a pinned entry");
            a.call(i, |m, c| m.action((3 << 24) | top, (0.0, 0.0), c)); // delete
            let ok = a.modules[i].debug_state().contains("items=0") && crate::store::read("clipboard.txt").trim().is_empty();
            out += &format!("{:<4} deleting a pinned entry removes it from disk\n", if ok { "OK" } else { "FAIL" });
            a.expanded = false;

            // Live Windows clipboard (opt-in: it briefly replaces what you copied,
            // then puts it back).
            if std::env::var("NOOKLY_TEST_CLIPBOARD").is_ok() {
                let hwnd = a.hwnd;
                let saved = cb::current(hwnd);
                a.settings.clip_on = true;
                a.settings.clip_mode = ClipMode::All;
                a.call(i, |m, c| m.tick(c));
                let wait = |a: &mut App| {
                    std::thread::sleep(std::time::Duration::from_millis(400));
                    a.call(i, |m, c| m.tick(c));
                };
                let tag = format!("Nookly selftest {}", crate::time::unix_now() as u64);
                cb::set_clipboard(hwnd, &Content::Text(tag.clone()));
                wait(a);
                let st = a.modules[i].debug_state();
                out += &format!("{:<4} copying text is recorded instantly\n", if st.ends_with(&format!("top={tag}")) { "OK" } else { "FAIL" });
                let older: u32 = field(&st, "top_id").parse().unwrap_or(0);
                cb::set_private_for_test(hwnd, "secret-password-123");
                wait(a);
                let st = a.modules[i].debug_state();
                out += &format!("{:<4} password-manager copies are not recorded\n", if !st.contains("secret-password") { "OK" } else { "FAIL" });
                let file = std::env::current_dir().unwrap_or_default().join("Cargo.toml").display().to_string();
                cb::set_clipboard(hwnd, &Content::Files(vec![file]));
                wait(a);
                let st = a.modules[i].debug_state();
                out += &format!("{:<4} copying a file is recorded\n", if st.contains("top=Cargo.toml") { "OK" } else { "FAIL" });
                if let Some(im) = cb::image_from_dib(cb::make_test_dib(64, 32)) {
                    cb::set_clipboard(hwnd, &Content::Image(std::sync::Arc::new(im)));
                }
                wait(a);
                let st = a.modules[i].debug_state();
                out += &format!("{:<4} copying an image is recorded (Everything mode)\n", if st.contains("top=Image  64 x 32") { "OK" } else { "FAIL" });
                a.settings.clip_mode = ClipMode::Text;
                a.call(i, |m, c| m.tick(c));
                let file2 = std::env::current_dir().unwrap_or_default().join("README.md").display().to_string();
                cb::set_clipboard(hwnd, &Content::Files(vec![file2]));
                wait(a);
                let st = a.modules[i].debug_state();
                out += &format!("{:<4} Text-only mode skips copied files\n", if !st.contains("README") { "OK" } else { "FAIL" });
                a.settings.clip_mode = ClipMode::All;
                a.call(i, |m, c| m.tick(c));
                a.call(i, |m, c| m.action((1 << 24) | older, (0.0, 0.0), c)); // copy the text again
                let back = cb::current(hwnd);
                out += &format!("{:<4} clicking an entry puts it back on the clipboard\n", if back == Some(Content::Text(tag.clone())) { "OK" } else { "FAIL" });
                // Clicking the row pastes into the app you were using; the copy
                // icon (shown on hover) only copies.
                {
                    use std::sync::atomic::Ordering::Relaxed;
                    cb::PASTE_DRY_RUN.store(true, Relaxed);
                    a.expanded = true;
                    a.tab = i;
                    let tgt = a.target();
                    a.cur = tgt;
                    a.render(tgt, 0.0);
                    // The second entry (the first one shows "Copied" right now).
                    let row = a.hits.iter().filter(|h| h.module == i && h.id >> 24 == 4).nth(1).copied();
                    let mut copy_icon = None;
                    if let Some(h) = row {
                        a.on_mouse_move(h.rect.x + 40.0, h.rect.cy());
                        a.render(tgt, 0.0);
                        copy_icon = a.hits.iter().find(|x| x.module == i && x.id >> 24 == 1 && (x.rect.cy() - h.rect.cy()).abs() < 2.0).copied();
                    }
                    // A plain desktop-style window to paste into (not Nookly).
                    let shell = unsafe { GetShellWindow() };
                    cb::TEST_TARGET.store(shell.0 as isize, Relaxed);
                    let entry = row.map_or(0, |h| h.id & 0xFF_FFFF);
                    cb::set_clipboard(hwnd, &Content::Text("other".into()));
                    a.call(i, |m, c| m.action((4 << 24) | entry, (0.0, 0.0), c));
                    let st = a.modules[i].debug_state();
                    let pasted = st.contains("paste=Some(Sent)") && cb::current(hwnd).map_or(false, |c| c != Content::Text("other".into()));
                    cb::set_clipboard(hwnd, &Content::Text("other".into()));
                    a.call(i, |m, c| m.action((1 << 24) | entry, (0.0, 0.0), c));
                    let st2 = a.modules[i].debug_state();
                    let copied = st2.contains("paste=None") && cb::current(hwnd).map_or(false, |c| c != Content::Text("other".into()));
                    // Never into Nookly itself or the taskbar.
                    cb::TEST_TARGET.store(hwnd.0 as isize, Relaxed);
                    a.call(i, |m, c| m.action((4 << 24) | entry, (0.0, 0.0), c));
                    let own = a.modules[i].debug_state().contains("paste=Some(NoTarget)");
                    let tray = unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }.unwrap_or_default();
                    let no_tray = tray.0.is_null() || !cb::paste_target_ok(tray, hwnd);
                    out += &format!(
                        "{:<4} clicking an entry pastes it, the copy icon only copies, never into Nookly or the taskbar {:?}\n",
                        if row.is_some() && copy_icon.is_some() && pasted && copied && own && no_tray { "OK" } else { "FAIL" },
                        (row.is_some(), copy_icon.is_some(), pasted, copied, own, no_tray)
                    );
                    out += &snap(a, "clipboard hover with copy icon");
                    // Real Ctrl+V into a text box (opt-in: it takes focus).
                    if std::env::var("NOOKLY_TEST_PASTE").is_ok() {
                        cb::PASTE_DRY_RUN.store(false, Relaxed);
                        unsafe {
                            let hinst = GetModuleHandleW(None).unwrap_or_default();
                            let top = CreateWindowExW(WINDOW_EX_STYLE(0), w!("STATIC"), w!("Nookly paste test"), WS_OVERLAPPEDWINDOW | WS_VISIBLE, 100, 100, 400, 120, None, None, Some(hinst.into()), None).unwrap_or_default();
                            // An Edit can't be a top-level parent for focus tests; host it.
                            let edit = CreateWindowExW(WINDOW_EX_STYLE(0), w!("EDIT"), w!(""), WS_CHILD | WS_VISIBLE | WS_BORDER, 10, 10, 360, 30, Some(top), None, Some(hinst.into()), None).unwrap_or_default();
                            // Windows only lets the app that got the last input take
                            // the foreground: send it an empty key event first.
                            let nothing = INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0), wScan: 0, dwFlags: KEYEVENTF_KEYUP, time: 0, dwExtraInfo: 0 } } };
                            SendInput(&[nothing], std::mem::size_of::<INPUT>() as i32);
                            let _ = SetForegroundWindow(top);
                            let _ = SetFocus(Some(edit));
                            let front = GetForegroundWindow() == top;
                            let want = format!("Nookly paste {}", crate::time::unix_now() as u64);
                            cb::set_clipboard(hwnd, &Content::Text(want.clone()));
                            wait(a);
                            cb::TEST_TARGET.store(top.0 as isize, Relaxed);
                            let st = a.modules[i].debug_state();
                            let id: u32 = field(&st, "top_id").parse().unwrap_or(0);
                            let _ = SetFocus(Some(edit));
                            a.call(i, |m, c| m.action((4 << 24) | id, (0.0, 0.0), c));
                            let mut got = String::new();
                            for _ in 0..40 {
                                let mut msg = MSG::default();
                                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                                    let _ = TranslateMessage(&msg);
                                    DispatchMessageW(&msg);
                                }
                                let mut b = [0u16; 128];
                                let n = GetWindowTextW(edit, &mut b);
                                got = String::from_utf16_lossy(&b[..n as usize]);
                                if got == want {
                                    break;
                                }
                                std::thread::sleep(std::time::Duration::from_millis(25));
                            }
                            let _ = DestroyWindow(top);
                            out += &format!("{:<4} clicking an entry really pastes into a text box ({got:?}, test window in front: {front})\n", if got == want { "OK" } else { "FAIL" });
                        }
                    }
                    cb::PASTE_DRY_RUN.store(false, Relaxed);
                    cb::TEST_TARGET.store(0, Relaxed);
                    a.on_mouse_leave();
                }
                if let Some(s) = saved {
                    cb::set_clipboard(hwnd, &s);
                }
                wait(a);
                a.call(i, |m, c| m.action(1, (0.0, 0.0), c));
                a.settings.clip_on = false;
                a.settings.clip_mode = ClipMode::Text;
                a.call(i, |m, c| m.tick(c));
            }
        }
        // Mic / camera dots.
        if let Some(i) = find(a, "Privacy") {
            use crate::modules::privacy::*;
            let names = [
                app_name("C:#Program Files#Zoom#bin#Zoom.exe"),
                app_name("Microsoft.WindowsSoundRecorder_8wekyb3d8bbwe"),
                app_name("MSTeams_8wekyb3d8bbwe"),
            ];
            let ok = names == ["Zoom".to_string(), "Windows Sound Recorder".to_string(), "MS Teams".to_string()];
            out += &format!("{:<4} mic/camera app names from the registry keys {:?}\n", if ok { "OK" } else { "FAIL" }, names);
            a.queue.clear();
            a.toast = None;
            a.call(i, |m, c| m.tick(c));
            a.call(i, |m, c| m.action(A_DEMO, (0.0, 0.0), c));
            a.call(i, |m, c| m.tick(c));
            let dots = a.indicators();
            let toast = a.queue.iter().any(|t| t.key == "privacy" && t.title == "Microphone in use" && t.sub.contains("Demo Recorder"));
            out += &format!("{:<4} mic in use shows an orange dot and names the app\n", if dots == vec![MIC] && toast { "OK" } else { "FAIL" });
            a.queue.clear();
            a.expanded = false;
            a.settings.style = NotchStyle::Full;
            let mut shapes = Vec::new();
            for on in [true, false] {
                a.settings.privacy_dots = on;
                let tgt = a.target();
                a.cur = tgt;
                a.render(tgt, 0.0);
                shapes.push(a.gfx.coverage());
            }
            a.settings.privacy_dots = true;
            out += &format!("{:<4} dot is drawn beside the closed notch ({} px vs {} px without)\n", if shapes[0] > shapes[1] + 40 { "OK" } else { "FAIL" }, shapes[0], shapes[1]);
            a.call(i, |m, c| m.action(A_DEMO, (0.0, 0.0), c));
            a.call(i, |m, c| m.tick(c));
            out += &format!("{:<4} dot goes away when the mic is released\n", if a.indicators().is_empty() { "OK" } else { "FAIL" });
            a.queue.clear();
        }
        // Pomodoro: start -> live activity, skip -> toast.
        if let Some(i) = find(a, "Pomodoro") {
            a.call(i, |m, c| m.action(1, (0.0, 0.0), c));
            a.expanded = false;
            a.keyboard = None;
            out += &format!("{:<4} pomodoro live activity while running\n", if a.modules[i].live().is_some() { "OK" } else { "FAIL" });
            out += &snap(a, "collapsed with pomodoro");
            for s in [NotchStyle::Compact, NotchStyle::Pill] {
                a.settings.style = s;
                out += &snap(a, &format!("pomodoro live, style {}", s.key()));
            }
            a.settings.style = NotchStyle::Full;
            a.call(i, |m, c| m.action(3, (0.0, 0.0), c));
            a.toast = a.queue.pop_front().map(|t| (t, a.t() + 10.0));
            out += &format!("{:<4} pomodoro skip produced toast\n", if a.toast.is_some() { "OK" } else { "FAIL" });
            out += &snap(a, "toast with actions");
        }
        a.toast = None;
        // Timer: preset, start -> live, expire -> toast, restart; stopwatch laps.
        if let Some(i) = find(a, "Timer") {
            if let Some(p) = find(a, "Pomodoro") {
                a.call(p, |m, c| m.action(2, (0.0, 0.0), c)); // reset pomodoro so the timer is live
            }
            a.tab = i;
            a.expanded = true;
            a.call(i, |m, c| m.action(12, (0.0, 0.0), c)); // 5 min preset
            out += &snap(a, "tab: Timer (countdown)");
            a.call(i, |m, c| m.action(3, (0.0, 0.0), c)); // start
            a.expanded = false;
            let live = a.live_module().map(|k| a.modules[k].title()).unwrap_or("none");
            out += &format!("{:<4} timer running shows in notch (live: {live})\n", if live == "Timer" { "OK" } else { "FAIL" });
            out += &snap(a, "collapsed with timer");
            a.queue.clear();
            a.call(i, |m, c| m.action(crate::modules::timer::A_DEBUG_EXPIRE, (0.0, 0.0), c));
            a.call(i, |m, c| m.tick(c));
            let ok = a.queue.iter().any(|t| t.key == "timer" && t.actions.len() == 2);
            out += &format!("{:<4} finished timer pops toast with Restart/Dismiss\n", if ok { "OK" } else { "FAIL" });
            a.queue.clear();
            a.call(i, |m, c| m.action(20, (0.0, 0.0), c)); // Restart from toast
            out += &format!("{:<4} restart from toast runs again\n", if a.modules[i].live().is_some() { "OK" } else { "FAIL" });
            a.call(i, |m, c| m.action(4, (0.0, 0.0), c)); // reset
            a.call(i, |m, c| {
                m.action(2, (0.0, 0.0), c); // stopwatch view
                m.action(3, (0.0, 0.0), c); // start
            });
            std::thread::sleep(std::time::Duration::from_millis(60));
            a.call(i, |m, c| {
                m.action(7, (0.0, 0.0), c); // lap
                m.action(7, (0.0, 0.0), c); // lap
            });
            a.expanded = true;
            out += &snap(a, "tab: Timer (stopwatch)");
            let st = a.modules[i].debug_state();
            out += &format!("{:<4} stopwatch running with 2 laps\n", if st.contains("running=true laps=2") { "OK" } else { "FAIL" });
            a.call(i, |m, c| {
                m.action(4, (0.0, 0.0), c);
                m.action(1, (0.0, 0.0), c);
            });
            a.expanded = false;
        }
        // Hovering a toast that has buttons must not swap it for the panel,
        // and clicking a toast button must not reopen the panel under the
        // still-hovering mouse.
        if let Some(ti) = find(a, "Timer") {
            a.expanded = false;
            a.queue.clear();
            let mut tt = Toast::new("selftest", glyph::TIMER, col::PINK, "Selftest", "toast").action("Reset", 4).action("Reset", 4);
            tt.module = ti;
            a.toast = Some((tt, a.t() + 30.0));
            a.hover = true;
            a.hover_since = a.t() - 5.0;
            a.frame();
            let ok = !a.expanded && a.mode() == Mode::Toast;
            out += &format!("{:<4} hovering a toast with buttons keeps the toast\n", if ok { "OK" } else { "FAIL" });
            let tgt = a.target();
            a.cur = tgt;
            a.render(tgt, 0.0);
            let btn = a.hits.iter().find(|h| h.module == TOAST && h.id == 0).copied();
            let ok = match btn {
                Some(b) => {
                    a.on_down(b.rect.cx(), b.rect.cy());
                    a.on_up(b.rect.cx(), b.rect.cy());
                    a.hover_since = a.t() - 5.0;
                    a.frame();
                    a.toast.is_none() && !a.expanded
                }
                None => false,
            };
            out += &format!("{:<4} toast button click does not reopen the panel while hovered\n", if ok { "OK" } else { "FAIL" });
            a.on_mouse_leave();
            out += &format!("{:<4} hover opens again after the mouse left\n", if a.can_hover_open() { "OK" } else { "FAIL" });
            a.hover = false;
            a.leave_at = None;
            a.toast = None;
            a.queue.clear();
        }
        // Tooltips: resting on an icon-only tab shows its name.
        {
            a.expanded = true;
            a.tab = a.modules.iter().position(|m| m.has_tab()).unwrap_or(0);
            let tgt = a.target();
            a.cur = tgt;
            a.render(tgt, 0.0);
            let tab = a.hits.iter().find(|h| h.module == APP_HIT && h.id != u32::MAX && !h.tip.is_empty()).copied();
            let ok = match tab {
                Some(h) => {
                    let base = a.gfx.bright();
                    a.mouse = (h.rect.cx(), h.rect.cy());
                    a.render(tgt, 0.0);
                    if let Some(tip) = &mut a.tip {
                        tip.2 -= 2.0;
                    }
                    a.render(tgt, 0.0);
                    let shown = a.gfx.bright() > base + 20;
                    a.mouse = (-1e4, -1e4);
                    a.tip = None;
                    shown
                }
                None => false,
            };
            out += &format!("{:<4} tooltip shows on a resting tab icon\n", if ok { "OK" } else { "FAIL" });
            let mut untipped: Vec<u32> = Vec::new();
            for i in 0..a.modules.len() {
                if !a.modules[i].has_tab() {
                    continue;
                }
                a.tab = i;
                let tgt = a.target();
                a.cur = tgt;
                a.render(tgt, 0.0);
                untipped.extend(a.hits.iter().filter(|h| h.module == APP_HIT && h.id != u32::MAX && h.tip.is_empty()).map(|h| h.id));
            }
            untipped.dedup();
            let mut no_title = Vec::new();
            for i in 0..a.modules.len() {
                if !a.modules[i].has_tab() {
                    continue;
                }
                a.tab = i;
                let tgt = a.target();
                a.cur = tgt;
                a.render(tgt, 0.0);
                if !a.header_fit.0 || !a.header_fit.1 {
                    no_title.push(a.modules[i].title());
                }
            }
            out += &format!("{:<4} every tab's header fits with its title {:?}\n", if no_title.is_empty() { "OK" } else { "FAIL" }, no_title);
            // Every clickable thing explains itself on hover.
            let mut missing: Vec<String> = Vec::new();
            let mut audit = |a: &mut App, view: &str| {
                let tgt = a.target();
                a.cur = tgt;
                a.render(tgt, 0.0);
                let hits: Vec<crate::ui::Hit> = a.hits.clone();
                for h in hits {
                    if h.id == u32::MAX || h.module == APP_HIT {
                        continue;
                    }
                    if a.tip_for(&h).is_empty() {
                        let s = format!("{view}:{}", h.id);
                        if !missing.contains(&s) {
                            missing.push(s);
                        }
                    }
                }
            };
            for i in 0..a.modules.len() {
                if !a.modules[i].has_tab() {
                    continue;
                }
                a.tab = i;
                let title = a.modules[i].title();
                audit(a, title);
                // Other views of the same panel.
                let views: &[u32] = match title {
                    "Tools" => &[2, 3, 4, 77, 73, 1],
                    "Water Reminder" => &[2, 3],
                    "Prayer Times" => &[1, 3, 4, 5, 2],
                    "AI Agents" => &[990, 3],
                    "Clipboard" => &[991],
                    "Widgets" => &[6, 3, 5, 1, 6],
                    _ => &[],
                };
                for &v in views {
                    a.call(i, |m, c| m.action(v, (0.0, 0.0), c));
                    a.release_keyboard();
                    audit(a, title);
                }
            }
            // The Settings window: every control explains itself, every
            // section fits, and every label fits its control.
            let views: Vec<(u8, u8)> = (0..SECTIONS.len() as u8).map(|s| (0u8, s)).chain([(1u8, 0u8)]).collect();
            for (tab, sec) in views {
                a.sw_view(tab, sec);
                for h in a.sw_hits() {
                    let tip = if !h.tip.is_empty() {
                        h.tip
                    } else if h.module < a.modules.len() {
                        a.modules[h.module].tip(h.id)
                    } else {
                        settings_tip(h.id)
                    };
                    if tip.is_empty() {
                        let s = format!("settings{tab}.{sec}:{}", h.id);
                        if !missing.contains(&s) {
                            missing.push(s);
                        }
                    }
                }
            }
            let (room, tight) = a.swin.as_ref().map(|sw| (sw.room, sw.tight.clone())).unwrap_or_default();
            a.close_settings();
            out += &format!("{:<4} every button, switch, chip and row has a tooltip {:?}\n", if missing.is_empty() { "OK" } else { "FAIL" }, missing);
            out += &format!("{:<4} every Settings section fits (room left {:?})\n", if room.iter().all(|r| *r >= 0.0) { "OK" } else { "FAIL" }, room);
            out += &format!("{:<4} settings labels fit their controls {:?}\n", if tight.is_empty() { "OK" } else { "FAIL" }, tight);
            out += &format!("{:<4} every tab icon has a tooltip {:?}\n", if untipped.is_empty() { "OK" } else { "FAIL" }, untipped);
            a.expanded = false;
        }
        // Mascots: every character renders in each mood, draws something
        // visible, and looks different from the others.
        {
            use std::hash::{Hash, Hasher};
            let mut hashes = Vec::new();
            for m in Mascot::ALL {
                let mut ok = true;
                let mut bright = 0;
                for (mood, mi) in [(Mood::Eyes, 0), (Mood::Doze, 1), (Mood::Wave, 2)] {
                    a.hits.clear();
                    a.gfx.begin(a.scale);
                    {
                        let mut ui = Ui { g: &mut a.gfx, hits: &mut a.hits, module: APP_HIT, mouse: (-1e4, -1e4), t: 1.0 };
                        ui.fill_rect(Rect::new(0.0, 0.0, 220.0, 48.0), col::NOTCH);
                        mascot::draw(&mut ui, Rect::new(14.0, 2.0, 172.0, 28.0), m, mood, 1.0, (1.0, 0.5));
                    }
                    a.gfx.end_and_present(a.hwnd, a.origin.0, a.origin.1);
                    ok &= a.gfx.last_err.is_none();
                    if mi == 0 {
                        bright = a.gfx.bright();
                        let mut h = std::collections::hash_map::DefaultHasher::new();
                        a.gfx.pixels().hash(&mut h);
                        hashes.push(h.finish());
                    }
                }
                let ok = ok && bright > 30;
                out += &format!("{:<4} mascot {:<8} renders (bright_px={bright})\n", if ok { "OK" } else { "FAIL" }, m.key());
            }
            let mut uniq = hashes.clone();
            uniq.sort();
            uniq.dedup();
            out += &format!("{:<4} every mascot looks different ({} of {})\n", if uniq.len() == hashes.len() { "OK" } else { "FAIL" }, uniq.len(), hashes.len());
        }
        // Sounds: every built-in sound is a valid, audible, distinct WAV;
        // the choice is saved and "Off" plays nothing.
        {
            let mut ok = true;
            let mut seen = Vec::new();
            let mut info = Vec::new();
            for s in Sound::ALL {
                if let Some(w) = sound::wav(s) {
                    let pk = sound::peak(w);
                    let secs = (w.len() - 44) as f32 / 2.0 / 44100.0;
                    ok &= &w[..4] == b"RIFF" && pk > 0.15 && pk < 1.0 && secs > 0.2 && secs < 2.0 && !seen.contains(w);
                    seen.push(w.clone());
                    info.push(format!("{} {secs:.2}s peak {pk:.2}", s.key()));
                }
            }
            out += &format!("{:<4} {} built-in sounds are valid and distinct ({})\n", if ok && seen.len() == 4 { "OK" } else { "FAIL" }, seen.len(), info.join(", "));
            sound::mute(false);
            sound::set(Sound::Off);
            let silent = !sound::chime();
            sound::mute(true);
            sound::set(a.settings.sound);
            out += &format!("{:<4} sound Off plays nothing\n", if silent { "OK" } else { "FAIL" });
        }
        // Full-screen detection: only a real full-screen window hides the
        // notch; the desktop, Start, taskbar and maximized windows never do.
        {
            let mon = RECT { left: 0, top: 0, right: 1920, bottom: 1080 };
            let cases = [
                ("F11 browser / video", RECT { left: 0, top: 0, right: 1920, bottom: 1080 }, false, "Chrome_WidgetWin_1", true),
                ("borderless game", RECT { left: -1, top: -1, right: 1921, bottom: 1081 }, false, "UnityWndClass", true),
                ("desktop focused", RECT { left: 0, top: 0, right: 1920, bottom: 1080 }, false, "WorkerW", false),
                ("desktop (Progman)", RECT { left: 0, top: 0, right: 1920, bottom: 1080 }, false, "Progman", false),
                ("Start menu", RECT { left: 0, top: 0, right: 1920, bottom: 1080 }, false, "Windows.UI.Core.CoreWindow", false),
                ("maximized window", RECT { left: -8, top: -8, right: 1928, bottom: 1040 }, true, "Chrome_WidgetWin_1", false),
                ("maximized, auto-hide taskbar", RECT { left: -8, top: -8, right: 1928, bottom: 1088 }, true, "SunAwtFrame", false),
                ("normal window", RECT { left: 100, top: 80, right: 1500, bottom: 900 }, false, "Notepad", false),
                ("full screen on other monitor", RECT { left: 1920, top: 0, right: 3840, bottom: 1080 }, false, "Chrome_WidgetWin_1", false),
            ];
            let wrong: Vec<&str> = cases.iter().filter(|c| is_fullscreen(c.1, mon, c.2, c.3) != c.4).map(|c| c.0).collect();
            out += &format!("{:<4} only real full-screen windows hide the notch {:?}\n", if wrong.is_empty() { "OK" } else { "FAIL" }, wrong);
            out += &format!("INFO full screen right now: {}\n", a.foreground_fullscreen());
            // While hidden, a reminder waits (no expiry, no chime) and shows
            // as soon as the notch is visible again.
            a.queue.clear();
            a.toast = None;
            a.expanded = false;
            a.hidden = true;
            a.push_toast(Toast::new("selftest-hidden", glyph::DROP, col::CYAN, "Time to Drink", "test").secs(0.01));
            a.frame();
            std::thread::sleep(std::time::Duration::from_millis(30));
            a.frame();
            let waited = a.toast.is_none() && a.queue.len() == 1;
            a.hidden = false;
            a.frame();
            let shown = a.toast.as_ref().map_or(false, |t| t.0.key == "selftest-hidden");
            out += &format!("{:<4} a reminder waits while the notch is hidden, then shows\n", if waited && shown { "OK" } else { "FAIL" });
            a.toast = None;
            a.queue.clear();
        }
        // Frame pacing, measured on this PC: a tab switch must draw at the
        // display's refresh rate with no long gaps, and each frame must be
        // cheap. Also reports what the old Windows-timer pacing delivered.
        {
            let probe = |ms: u32| -> f64 {
                unsafe {
                    let id = SetTimer(None, 0, ms, None);
                    let (s, mut last, mut gaps) = (std::time::Instant::now(), None, Vec::new());
                    let mut m = MSG::default();
                    while s.elapsed().as_secs_f64() < 0.5 {
                        if PeekMessageW(&mut m, None, WM_TIMER, WM_TIMER, PM_REMOVE).as_bool() {
                            if m.wParam.0 != id {
                                continue;
                            }
                            let n = s.elapsed().as_secs_f64();
                            if let Some(l) = last {
                                gaps.push(n - l);
                            }
                            last = Some(n);
                        } else {
                            std::thread::yield_now();
                        }
                    }
                    let _ = KillTimer(None, id);
                    if gaps.is_empty() { 0.0 } else { gaps.len() as f64 / gaps.iter().sum::<f64>() }
                }
            };
            let (old15, old33) = (probe(15), probe(33));
            a.queue.clear();
            a.toast = None;
            a.onboarding = None;
            a.hidden = false;
            let tabs: Vec<usize> = (0..a.modules.len()).filter(|&i| a.modules[i].has_tab()).collect();
            a.tab = tabs[0];
            a.expanded = true;
            a.hover = true;
            a.leave_at = None;
            a.request_frame();
            let s0 = a.t();
            while matches!(a.pacing(), Pacing::Refresh) && a.t() - s0 < 2.0 {
                a.frame();
                wait_refresh(a.hwnd);
            }
            a.tab = tabs[tabs.len().min(4) - 1];
            a.request_frame();
            let (mut gaps, mut costs) = (Vec::new(), Vec::new());
            let mut last: Option<f64> = None;
            let mut refresh_all = true;
            let s1 = a.t();
            loop {
                let st = a.t();
                if let Some(l) = last {
                    gaps.push(st - l);
                }
                last = Some(st);
                a.frame();
                costs.push(a.t() - st);
                if !matches!(a.pacing(), Pacing::Refresh) {
                    break;
                }
                if a.t() - s1 > 2.0 {
                    refresh_all = false;
                    break;
                }
                wait_refresh(a.hwnd);
            }
            let ambient_after = matches!(a.pacing(), Pacing::After(_));
            let n = gaps.len().max(1) as f64;
            let fps = n / gaps.iter().sum::<f64>().max(1e-6);
            let worst = gaps.iter().cloned().fold(0.0, f64::max) * 1000.0;
            let avg_cost = costs.iter().sum::<f64>() / costs.len().max(1) as f64 * 1000.0;
            let max_cost = costs.iter().cloned().fold(0.0, f64::max) * 1000.0;
            let ok = refresh_all && ambient_after && fps >= 55.0 && worst < 40.0 && avg_cost < 4.0;
            out += &format!(
                "{:<4} tab switch frame rate {fps:.0} fps over {} frames, worst gap {worst:.1} ms, draw {avg_cost:.2} ms avg / {max_cost:.2} ms max\n",
                if ok { "OK" } else { "FAIL" },
                costs.len()
            );
            out += &format!("INFO old Windows-timer pacing here: 15 ms timer = {old15:.0} fps, 33 ms timer = {old33:.0} fps (old tab switches used the 33 ms one)\n");
            a.hover = false;
            a.expanded = false;
        }
        // Welcome tour: style is step one, choices apply and save, the panel
        // stays open without hover, and finishing says where to change it.
        {
            let click = |a: &mut App, id: u32| -> bool {
                let tgt = a.target();
                a.cur = tgt;
                a.render(tgt, 0.0);
                match a.hits.iter().find(|h| h.module == ONBOARD && h.id == id).copied() {
                    Some(h) => {
                        a.on_down(h.rect.cx(), h.rect.cy());
                        a.on_up(h.rect.cx(), h.rect.cy());
                        true
                    }
                    None => false,
                }
            };
            let before = (a.settings.style, a.settings.edge);
            a.queue.clear();
            a.toast = None;
            a.start_onboarding();
            a.hover = false;
            a.leave_at = Some(a.t() - 1.0);
            a.frame();
            out += &format!("{:<4} welcome tour stays open without the mouse\n", if a.mode() == Mode::Expanded && a.onboarding == Some(0) { "OK" } else { "FAIL" });
            for (k, name) in ["hello", "look and place", "colour", "what's in it", "tips"].iter().enumerate() {
                a.onboarding = Some(k as u8);
                out += &snap(a, &format!("welcome tour step {} ({name})", k + 1));
            }
            a.onboarding = Some(1);
            let tiles = {
                let tgt = a.target();
                a.cur = tgt;
                a.render(tgt, 0.0);
                a.hits.iter().filter(|h| h.module == ONBOARD && (O_STYLE..O_STYLE + NotchStyle::ALL.len() as u32).contains(&h.id)).count()
            };
            let ok = tiles == 5 && click(a, O_STYLE + 3) && a.settings.style == NotchStyle::Pill && crate::store::read("settings.txt").contains("style=pill");
            out += &format!("{:<4} step 1 offers the 5 styles and picking Pill applies it\n", if ok { "OK" } else { "FAIL" });
            let ok = click(a, O_EDGE + 1) && a.settings.edge == Edge::Bottom && a.mode() == Mode::Expanded && a.onboarding == Some(1);
            out += &format!("{:<4} the same step moves the notch and the tour stays open\n", if ok { "OK" } else { "FAIL" });
            let ok = click(a, O_NEXT) && a.onboarding == Some(2) && click(a, O_THEME + 1) && a.settings.theme == "midnight";
            out += &format!("{:<4} step 3 picks a colour theme ({})\n", if ok { "OK" } else { "FAIL" }, a.settings.theme);
            a.settings.theme = "default".into();
            let ok = click(a, O_NEXT) && a.onboarding == Some(3);
            out += &snap(a, "welcome tour presets");
            let ok = ok && a.hits.iter().any(|h| h.module == ONBOARD && h.id == O_PRESET);
            out += &format!("{:<4} step 4 offers the presets\n", if ok { "OK" } else { "FAIL" });
            let ok = ok && click(a, O_NEXT) && a.onboarding == Some(4);
            out += &format!("{:<4} the last step shows {} tips\n", if ok { "OK" } else { "FAIL" }, TOUR_TIPS.len());
            let ok = ok && click(a, O_NEXT) && a.onboarding.is_none();
            let saved = crate::store::read("settings.txt").contains("onboarded=1");
            let told = a.queue.iter().any(|t| t.key == "welcome" && t.sub.contains("right-click"));
            out += &format!("{:<4} finishing saves it and says how to change things later\n", if ok && saved && told { "OK" } else { "FAIL" });
            // Updating from a version without the tour (settings but no
            // "onboarded" key) shows it once; once finished it never returns.
            let saved_settings = crate::store::read("settings.txt");
            crate::store::write("settings.txt", "edge=top\nstyle=pill\nvolume_osd=1\n");
            let old_user = crate::store::Settings::load();
            crate::store::write("settings.txt", &saved_settings);
            let ok = !old_user.onboarded && old_user.style == NotchStyle::Pill && crate::store::Settings::load().onboarded;
            out += &format!("{:<4} people updating from a version without the tour get it once\n", if ok { "OK" } else { "FAIL" });
            // Tour text fits: style tiles, tip cards, hints beside the buttons.
            let mut tight = Vec::new();
            // Five tiles; the line under each style may wrap to two lines.
            let tile_w = (ONBOARD_SIZE.0 - 8.0 - 40.0) / 5.0 - 16.0;
            for s in ["Clock, music, mascot", "Full, but narrower", "A smaller notch", "Just a thin bar", "Invisible until hovered"] {
                if a.gfx.measure(s, 10.0, Weight::Regular) > tile_w * 1.8 {
                    tight.push(s);
                }
            }
            let card_w = (ONBOARD_SIZE.0 - 8.0 - 10.0 * (TOUR_TIPS.len() as f32 - 1.0)) / TOUR_TIPS.len() as f32 - 20.0;
            let preset_w = (ONBOARD_SIZE.0 - 8.0 - 10.0 * 3.0) / 4.0 - 20.0;
            for (s, sub, _) in crate::features::PRESETS {
                if a.gfx.measure(s, 13.0, Weight::Semi) > preset_w || a.gfx.measure(sub, 10.5, Weight::Regular) > preset_w * 3.6 {
                    tight.push(s);
                }
            }
            for (_, s, sub) in TOUR_TIPS {
                if a.gfx.measure(s, 12.5, Weight::Semi) > card_w {
                    tight.push(s);
                }
                // The tip wraps onto at most four lines.
                if a.gfx.measure(sub, 11.0, Weight::Regular) > card_w * 3.6 {
                    tight.push(sub);
                }
            }
            // Settings at a glance: title on one line, description on two.
            let glance_w = (ONBOARD_SIZE.0 - 8.0 - 24.0) / 4.0 - 16.0 - 30.0;
            for (_, s, sub) in TOUR_SETTINGS {
                if a.gfx.measure(s, 12.0, Weight::Semi) > glance_w {
                    tight.push(s);
                }
                if a.gfx.measure(sub, 10.5, Weight::Regular) > glance_w * 1.7 {
                    tight.push(sub);
                }
            }
            if a.gfx.measure(TOUR_Q_SETTINGS, 12.5, Weight::Regular) > ONBOARD_SIZE.0 - 8.0 {
                tight.push("settings question");
            }
            let foot_w = ONBOARD_SIZE.0 - 8.0 - (a.gfx.measure("Next", 12.0, Weight::Semi) + 48.0) - 78.0;
            for s in ["Pick one. You can change it anytime in Settings.", "Or right-drag the notch to any edge, on any screen."] {
                if a.gfx.measure(s, 11.0, Weight::Regular) > foot_w {
                    tight.push(s);
                }
            }
            let last_w = ONBOARD_SIZE.0 - 8.0 - (a.gfx.measure("Start using Nookly", 12.0, Weight::Semi) + 48.0) - 118.0 - 78.0;
            if a.gfx.measure("That's it. Enjoy!", 11.0, Weight::Regular) > last_w || a.gfx.measure("Open Settings", 12.0, Weight::Semi) > 110.0 - 16.0 {
                tight.push("last step footer");
            }
            if a.gfx.measure("First, how should the notch look when it's closed?", 12.5, Weight::Regular) > ONBOARD_SIZE.0 {
                tight.push("question");
            }
            out += &format!("{:<4} welcome tour text fits {:?}\n", if tight.is_empty() { "OK" } else { "FAIL" }, tight);
            a.queue.clear();
            a.settings.style = before.0;
            a.set_edge(before.1, true);
            a.settings.edge = Edge::Top;
            a.settings.save();
            a.expanded = false;
        }
        // Settings window: the gear, a right-click and the tray icon open it
        // (and close the notch); its clicks apply and save.
        {
            a.toast = None;
            a.queue.clear();
            a.expanded = true;
            a.tab = a.tab_list().first().copied().unwrap_or(0);
            let tgt = a.target();
            a.cur = tgt;
            a.render(tgt, 0.0);
            let gear = a.hits.iter().find(|h| h.module == APP_HIT && h.id == GEAR).copied();
            if let Some(g) = gear {
                a.on_down(g.rect.cx(), g.rect.cy());
                a.on_up(g.rect.cx(), g.rect.cy());
            }
            let opened = gear.is_some() && a.swin.is_some() && !a.expanded;
            out += &format!("{:<4} gear opens the Settings window and closes the notch\n", if opened { "OK" } else { "FAIL" });
            a.close_settings();
            a.hover_block = false;
            a.handle(WM_RBUTTONDOWN, WPARAM(0), LPARAM(0));
            a.handle(WM_RBUTTONUP, WPARAM(0), LPARAM(0));
            let rclick = a.swin.is_some();
            a.close_settings();
            a.handle(WM_APP_TRAY, WPARAM(0), LPARAM(WM_LBUTTONUP as isize));
            let tray = a.swin.is_some();
            out += &format!("{:<4} right-click on the notch and the tray icon open the same window\n", if rclick && tray { "OK" } else { "FAIL" });
            a.sw_view(0, 0);
            out += &a.sw_snap("settings window");
            let sections = a.sw_hits().iter().filter(|h| (S_SECTION..S_SECTION + 20).contains(&h.id)).count();
            let notch_bar = !a.tab_list().iter().any(|&i| a.modules[i].title() == "Widgets");
            out += &format!("{:<4} Settings has {sections} sections; Widgets moved there from the notch's tab bar\n", if sections == SECTIONS.len() && notch_bar { "OK" } else { "FAIL" });
            a.sw_view(0, SEC_MASCOT);
            let tiles = a.sw_hits().iter().filter(|h| h.module == SETTINGS && (S_MASCOT..=S_MASCOT + Mascot::ALL.len() as u32).contains(&h.id)).count();
            out += &format!("{:<4} settings shows {tiles} mascot tiles (clock + {})\n", if tiles == Mascot::ALL.len() + 1 { "OK" } else { "FAIL" }, Mascot::ALL.len());
            a.sw_view(0, SEC_GENERAL);
            let general = [S_POLICY, S_QUIT, S_TOUR, S_EXPORT, S_IMPORT, S_UPDATES].iter().all(|id| a.sw_has(*id));
            out += &format!("{:<4} General has updates, export / import, the tour, Privacy policy and Quit\n", if general { "OK" } else { "FAIL" });
            // The Widgets section hosts the Widgets panel, and its clicks work.
            if let Some(w) = find(a, "Widgets") {
                a.sw_view(0, SEC_WIDGETS);
                out += &a.sw_snap("settings widgets section");
                let hosted = a.sw_hits().iter().filter(|h| h.module == w).count();
                // Widgets has Show (and Order) only; alerts and the place
                // moved to their own sections.
                let only_show = !a.sw_hits().iter().any(|h| h.module == w && h.id == crate::modules::widgets::A_ALERTS);
                // Alerts > How each alert shows: the alerts list, one place.
                a.sw_view(0, SEC_ALERTS);
                let views = a.sw_has(S_ALERT_VIEW) && a.sw_has(S_ALERT_VIEW + 1) && a.sw_has(S_NOTIF_ON);
                a.sw_click_id(S_ALERT_VIEW + 1);
                let alerts = a.sw_hits().iter().any(|h| h.module == w && h.id == crate::modules::widgets::A_ALERT_RESET) && !a.sw_has(S_NOTIF_ON);
                out += &a.sw_snap("settings alerts: each alert");
                if let Some(sw) = a.swin.as_mut() {
                    sw.alert_view = 0;
                }
                // Location: the place picker, a general setting of its own.
                a.sw_view(0, SEC_LOCATION);
                out += &a.sw_snap("settings location");
                let place = a.sw_hits().iter().filter(|h| h.module == w).count() > 2 && !a.sw_hits().iter().any(|h| h.module == w && h.id == crate::modules::widgets::A_SHOW);
                out += &format!(
                    "{:<4} Widgets ({hosted} controls), Alerts (two clear pages) and Location (its own section) are separate {:?}\n",
                    if hosted > 5 && only_show && views && alerts && place { "OK" } else { "FAIL" },
                    (only_show, views, alerts, place)
                );
            }
            // Settings > Panels: drag a panel to a new place; the notch's
            // tab bar follows, and Reset puts the order back.
            {
                let before = a.tab_list();
                a.sw_view(0, SEC_PANELS);
                out += &a.sw_snap("settings panels order");
                let cards = a.sw_hits().iter().filter(|h| (S_ORDER..S_ORDER + 64).contains(&h.id)).count();
                let first = a.modules[before[0]].title();
                a.move_panel(0, 2);
                let after = a.tab_list();
                let moved = after.len() == before.len() && a.modules[after[2]].title() == first && crate::store::read("settings.txt").contains("panel_order=");
                a.sw_click_id(S_ORDER_RESET);
                let reset = a.tab_list() == before;
                out += &format!("{:<4} Settings > Panels lists {cards} panels; dragging reorders the tab bar and saves; Reset restores it\n", if cards == before.len() && moved && reset { "OK" } else { "FAIL" });
            }
            // The master switch: off hides the notch and stops its features.
            a.sw_view(0, 0);
            let master = a.sw_click_id(S_MASTER) && !a.notch_on() && a.hidden && crate::store::read("settings.txt").contains("notch_on=0");
            a.tick();
            let hidden = a.hidden;
            let back = a.sw_click_id(S_MASTER) && a.notch_on();
            a.tick();
            out += &format!("{:<4} the Notch master switch hides the notch and brings it back, and saves\n", if master && hidden && back && !a.hidden { "OK" } else { "FAIL" });
            // Taskbar tab: its switch stays off (coming soon); a button leads to Mascot.
            a.sw_view(1, 0);
            let off = a.sw_has(S_MASTER_TASKBAR) && a.sw_has(S_GOTO_MASCOT);
            if let Some(h) = a.sw_hits().iter().find(|h| h.id == S_MASTER_TASKBAR).copied() {
                a.sw_click(h.rect.cx(), h.rect.cy());
            }
            let still = !a.settings.extra.contains_key("taskbar_on");
            if let Some(h) = a.sw_hits().iter().find(|h| h.id == S_GOTO_MASCOT).copied() {
                a.sw_click(h.rect.cx(), h.rect.cy());
            }
            let lands = a.swin.as_ref().map_or(false, |sw| sw.tab == 0 && sw.section == SEC_MASCOT);
            out += &format!("{:<4} Taskbar tab: coming soon, its switch stays off, and it points to the taskbar mascot\n", if off && still && lands { "OK" } else { "FAIL" });
            let ok = a.sw_click_id(S_TOUR) && a.swin.is_none() && a.onboarding == Some(0);
            a.onboarding = None;
            a.expanded = false;
            out += &format!("{:<4} Welcome tour button restarts the tour\n", if ok { "OK" } else { "FAIL" });
            a.open_settings();
            let ok = a.sw_click_id(S_CLOSE) && a.swin.is_none();
            out += &format!("{:<4} close button closes the window\n", if ok { "OK" } else { "FAIL" });
            a.open_settings();
            let before = (a.settings.style, a.settings.mascot, a.settings.personality);
            let click = |a: &mut App, id: u32| -> bool { a.sw_click_id(id) };
            let robot = S_MASCOT + 1 + Mascot::ALL.iter().position(|m| *m == Mascot::Robot).unwrap_or(0) as u32;
            let ok = click(a, robot) && a.settings.mascot == Mascot::Robot && a.settings.personality && crate::store::read("settings.txt").contains("mascot=robot");
            out += &format!("{:<4} picking a mascot applies and saves\n", if ok { "OK" } else { "FAIL" });
            let ok = click(a, S_MASCOT) && !a.settings.personality;
            out += &format!("{:<4} picking Clock turns the mascot off\n", if ok { "OK" } else { "FAIL" });
            let ok = click(a, S_STYLE + 2) && a.settings.style == NotchStyle::Compact && crate::store::read("settings.txt").contains("style=compact")
                && click(a, S_STYLE + 1) && a.settings.style == NotchStyle::Slim && crate::store::read("settings.txt").contains("style=slim");
            out += &format!("{:<4} notch style chip applies and saves\n", if ok { "OK" } else { "FAIL" });
            // Slim: narrower than Full, same closed height; the open panel is
            // narrower and shorter, except Tools, which keeps Full's size.
            {
                let saved = (a.expanded, a.tab, a.settings.edge);
                a.settings.edge = Edge::Top;
                let size = |a: &mut App, s: NotchStyle, open: bool, tab: usize| {
                    a.settings.style = s;
                    a.expanded = open;
                    a.tab = tab;
                    a.target()
                };
                let music = find(a, "Now Playing").unwrap_or(0);
                let tools = find(a, "Tools").unwrap_or(0);
                let (fc, sc) = (size(a, NotchStyle::Full, false, music), size(a, NotchStyle::Slim, false, music));
                let (fo, so) = (size(a, NotchStyle::Full, true, music), size(a, NotchStyle::Slim, true, music));
                let (ft, st) = (size(a, NotchStyle::Full, true, tools), size(a, NotchStyle::Slim, true, tools));
                let ok = sc.0 < fc.0 && sc.1 == fc.1 && so.0 < fo.0 && so.1 < fo.1 && st.1 - SLIM_TAB_H - SLIM_PAD >= ft.1 - TAB_H - PAD - 1.0 && NotchStyle::from_key("slim") == Some(NotchStyle::Slim);
                out += &format!(
                    "{:<4} Slim style: closed {:.0}x{:.0} (Full {:.0}x{:.0}), open {:.0}x{:.0} (Full {:.0}x{:.0}), Tools keeps its height\n",
                    if ok { "OK" } else { "FAIL" }, sc.0, sc.1, fc.0, fc.1, so.0, so.1, fo.0, fo.1
                );
                (a.expanded, a.tab, a.settings.edge) = saved;
                // With the widest clock (12:58), what each live view shows
                // still fits beside it: ring + label, icon + text, countdown.
                let mid = a.gfx.measure("12:58", 14.0, Weight::Semi) + 2.0;
                let wing = SLIM_LIVE_W / 2.0 - mid / 2.0 - SLIM_GAP - SLIM_WING_PAD;
                let mut tight = Vec::new();
                for (box_w, gap, label) in [(18.0, 5.0, "Timer"), (18.0, 5.0, "Watch"), (18.0, 5.0, "Focus"), (18.0, 5.0, "Break"), (20.0, 4.0, "Due"), (20.0, 4.0, "OK?"), (20.0, 2.0, "100%"), (0.0, 0.0, "Maghrib")] {
                    if box_w + gap + a.gfx.measure(label, 12.0, Weight::Semi) > wing {
                        tight.push(label);
                    }
                }
                for d in ["59:59", "1:00:00"] {
                    if a.gfx.measure_digits(d, 12.0) > wing {
                        tight.push(d);
                    }
                }
                out += &format!("{:<4} Slim live notch: {SLIM_LIVE_W} wide, wings {wing:.0} px beside a 12:58 clock, labels fit {tight:?}\n", if tight.is_empty() { "OK" } else { "FAIL" });
            }
            out += &widget_tests(a);
            a.settings.style = before.0;
            a.settings.mascot = before.1;
            a.settings.personality = before.2;
            a.settings.save();
            let ok = click(a, S_VOLUME) && click(a, S_VOLUME);
            out += &format!("{:<4} settings switches respond\n", if ok { "OK" } else { "FAIL" });
            let ok = a.settings.update_check
                && click(a, S_UPDATE_AUTO)
                && !a.settings.update_check
                && crate::store::read("settings.txt").lines().any(|l| l.trim() == "update_check=0")
                && click(a, S_UPDATE_AUTO)
                && a.settings.update_check;
            out += &format!("{:<4} automatic update checks can be turned off (on by default) and it saves\n", if ok { "OK" } else { "FAIL" });
            let bell = S_SOUND + Sound::ALL.iter().position(|s| *s == Sound::Bell).unwrap_or(0) as u32;
            let ok = click(a, bell) && a.settings.sound == Sound::Bell && crate::store::read("settings.txt").contains("sound=bell");
            out += &format!("{:<4} picking a sound applies and saves\n", if ok { "OK" } else { "FAIL" });
            // Alerts and Privacy: notifications start off; the switches save.
            let n0 = a.settings.notif_on;
            a.sw_view(0, SEC_ALERTS);
            let alerts = [S_NOTIF_ON, S_NOTIF_TEXT, S_NOTIF_BANNERS, S_NOTIF_UNMUTE, S_VOLUME].iter().all(|id| a.sw_has(*id));
            a.sw_view(0, SEC_PRIVACY);
            let privacy = [S_PRIVACY, S_CLIP_ON, S_CLIP_MODE, S_COPY_TOAST].iter().all(|id| a.sw_has(*id));
            let t0 = a.settings.notif_text;
            let toggled = click(a, S_NOTIF_TEXT) && a.settings.notif_text != t0 && crate::store::read("settings.txt").contains(&format!("notif_text={}", (!t0) as u8));
            click(a, S_NOTIF_TEXT);
            out += &format!(
                "{:<4} Alerts and Privacy hold the notification, mic-dot and clipboard switches (notifications start {})\n",
                if alerts && privacy && toggled && !n0 { "OK" } else { "FAIL" },
                if n0 { "ON (should be off)" } else { "off" }
            );
            a.close_settings();
            a.settings.sound = Sound::Chime;
            a.settings.save();
            a.expanded = false;
        }
        // Prayer times: the maths matches a reference (aladhan.com, same
        // method) within a minute, across seasons and cities.
        {
            use crate::modules::prayer::{self, Calc, Isha};
            let cases: [(i32, i32, i32, f64, f64, f64, Isha, [&str; 6]); 5] = [
                (2026, 10, 27, 23.8103, 90.4125, 2.0, Isha::Angle(18.0), ["04:45", "06:01", "11:42", "15:47", "17:23", "18:39"]),
                (2026, 6, 21, 23.8103, 90.4125, 2.0, Isha::Angle(18.0), ["03:44", "05:12", "12:00", "16:41", "18:48", "20:16"]),
                (2026, 1, 15, 24.8949, 91.8687, 2.0, Isha::Angle(18.0), ["05:19", "06:39", "12:02", "15:49", "17:25", "18:45"]),
                (2026, 3, 21, 22.3569, 91.7832, 2.0, Isha::Angle(18.0), ["04:42", "05:56", "12:00", "16:22", "18:04", "19:19"]),
                (2026, 10, 27, 23.8103, 90.4125, 1.0, Isha::Angle(17.0), ["04:45", "06:01", "11:42", "14:58", "17:23", "18:35"]),
            ];
            let (mut worst, mut bad) = (0i64, Vec::new());
            for (y, m, d, lat, lon, asr, isha, want) in cases {
                let got = prayer::times(&Calc { lat, lon, tz: 6.0, fajr: 18.0, isha, asr }, y, m, d);
                for k in 0..6 {
                    let w: Vec<i64> = want[k].split(':').map(|x| x.parse().unwrap_or(0)).collect();
                    let diff = ((got[k] * 60.0).round() as i64 - (w[0] * 60 + w[1])).abs();
                    worst = worst.max(diff);
                    if diff > 1 {
                        bad.push(format!("{y}-{m}-{d} {} {} vs {}", prayer::NAMES[k], prayer::hhmm24(got[k]), want[k]));
                    }
                }
            }
            out += &format!("{:<4} prayer times match the reference within a minute (30 checks, worst {worst} min) {:?}\n", if bad.is_empty() { "OK" } else { "FAIL" }, bad);
            out += &format!("INFO this PC's UTC offset {:+} h\n", prayer::local_tz());
        }
        // Prayer panel: pages, picks saved, reminder, countdown in the notch.
        if let Some(pi) = find(a, "Prayer Times") {
            use crate::modules::prayer::{A_TEST_DUE, A_TEST_RELOAD, A_TEST_SOON};
            a.toast = None;
            a.queue.clear();
            a.expanded = true;
            a.tab = pi;
            a.switch_t = -10.0;
            out += &snap(a, "prayer panel");
            let bells = a.hits.iter().filter(|h| h.module == pi && (90..96).contains(&h.id)).count();
            out += &format!("{:<4} prayer panel lists six times with a reminder bell each ({bells})\n", if bells == 6 { "OK" } else { "FAIL" });
            let clickm = |a: &mut App, id: u32| -> bool {
                let tgt = a.target();
                a.cur = tgt;
                a.render(tgt, 0.0);
                match a.hits.iter().find(|h| h.module == pi && h.id == id).copied() {
                    Some(h) => {
                        a.on_down(h.rect.cx(), h.rect.cy());
                        a.on_up(h.rect.cx(), h.rect.cy());
                        true
                    }
                    None => false,
                }
            };
            let saved = |k: &str| crate::store::read("prayer.txt").lines().any(|l| l.trim() == k);
            let city = clickm(a, 1) && clickm(a, 14) && saved("city=Sylhet");
            out += &snap(a, "prayer location page");
            let method = clickm(a, 4) && clickm(a, 51) && saved("method=mwl") && clickm(a, 61) && clickm(a, 60) && saved("asr=shafi");
            out += &snap(a, "prayer method page");
            clickm(a, 50);
            clickm(a, 61);
            let adjust = clickm(a, 5) && clickm(a, 113) && saved("off_asr=1") && clickm(a, 103) && saved("off_asr=0");
            out += &snap(a, "prayer adjust page");
            // Custom latitude, longitude typed in.
            clickm(a, 3);
            let mut custom = clickm(a, 40) && a.keyboard == Some(pi);
            for _ in 0..40 {
                a.on_key(0x08);
            }
            for ch in "21.43, 92.01".chars() {
                a.on_char(ch);
            }
            out += &snap(a, "prayer custom location typing");
            a.on_key(0x0D);
            custom = custom && saved("city=custom") && saved("lat=21.4300") && saved("lon=92.0100") && a.keyboard.is_none();
            clickm(a, 10);
            let back = clickm(a, 2) && saved("city=Dhaka") && saved("method=karachi") && saved("asr=hanafi");
            out += &format!(
                "{:<4} prayer settings: city, method, Asr school, minute nudges and typed coordinates save {:?}\n",
                if city && method && adjust && custom && back { "OK" } else { "FAIL" },
                (city, method, adjust, custom, back)
            );
            // Tight labels on its pages.
            let mut tight = Vec::new();
            let w4 = (modules::PANEL.0 - 18.0) / 4.0 - 8.0;
            for (c, _, _) in crate::modules::prayer::CITIES {
                if a.gfx.measure(c, 11.5, Weight::Semi) > w4 {
                    tight.push(c);
                }
            }
            let wm = (modules::PANEL.0 - 74.0 - 24.0) / 5.0 - 8.0;
            for (_, m, _, _) in crate::modules::prayer::METHODS {
                if a.gfx.measure(m, 11.5, Weight::Semi) > wm {
                    tight.push(m);
                }
            }
            if a.gfx.measure("15 min before", 11.5, Weight::Semi) > (modules::PANEL.0 - 74.0 - 18.0) / 4.0 - 8.0 {
                tight.push("15 min before");
            }
            out += &format!("{:<4} prayer panel labels fit {:?}\n", if tight.is_empty() { "OK" } else { "FAIL" }, tight);
            // A reminder at prayer time, once.
            a.expanded = false;
            a.toast = None;
            a.queue.clear();
            a.call(pi, |m, c| m.action(A_TEST_DUE, (0.0, 0.0), c));
            a.tick();
            let first = a.toast.as_ref().map(|t| t.0.title.clone()).or_else(|| a.queue.front().map(|t| t.title.clone())).unwrap_or_default();
            a.toast = None;
            a.queue.clear();
            a.tick();
            let again = a.toast.is_some() || !a.queue.is_empty();
            out += &format!("{:<4} prayer reminder at the time, once ({first})\n", if first.starts_with("Time for ") && !again { "OK" } else { "FAIL" });
            // Ten minutes before: the countdown takes the closed notch.
            a.toast = None;
            a.queue.clear();
            a.call(pi, |m, c| m.action(A_TEST_SOON, (0.0, 0.0), c));
            let live = a.live_module() == Some(pi);
            out += &snap(a, "prayer countdown in the notch");
            out += &format!("{:<4} prayer countdown shows in the closed notch 10 min before\n", if live { "OK" } else { "FAIL" });
            a.call(pi, |m, c| m.action(A_TEST_RELOAD, (0.0, 0.0), c));
            let off = a.live_module() != Some(pi);
            out += &format!("{:<4} prayer reminders and countdown are off until turned on\n", if off { "OK" } else { "FAIL" });
        } else {
            out += "FAIL prayer panel missing\n";
        }
        // Settings > Panels: hide a panel; its tab and alerts go; one stays.
        {
            a.toast = None;
            a.queue.clear();
            let wi = find(a, "Water Reminder").unwrap_or(0);
            a.sw_view(0, SEC_FEATURES);
            out += &a.sw_snap("settings features section");
            let fk = crate::features::FEATURES.iter().position(|f| f.key == "Water Reminder").unwrap_or(0) as u32;
            let chips = a.sw_hits().iter().filter(|h| h.module == SETTINGS && (S_FEATURE..S_FEATURE + crate::features::FEATURES.len() as u32).contains(&h.id)).count();
            let fits = true;
            let room = a.swin.as_ref().map_or(false, |sw| sw.room[SEC_FEATURES as usize] >= 0.0);
            if let Some(h) = a.sw_hits().iter().find(|h| h.module == SETTINGS && h.id == S_FEATURE + fk).copied() {
                a.sw_click(h.rect.cx(), h.rect.cy());
            }
            a.close_settings();
            a.expanded = true;
            let saved = crate::store::read("settings.txt").lines().any(|l| l.trim() == "hidden_panels=Water Reminder");
            let tgt = a.target();
            a.cur = tgt;
            a.render(tgt, 0.0);
            let no_tab = !a.hits.iter().any(|h| h.module == APP_HIT && h.id == wi as u32);
            a.call(wi, |_, c| c.toast(Toast::new("water", glyph::DROP, col::CYAN, "Time to drink", "test")));
            let quiet = a.toast.is_none() && a.queue.is_empty();
            out += &format!(
                "{:<4} switching a feature off ({chips} listed) removes its tab and silences it, and saves {:?}\n",
                if chips == crate::features::FEATURES.len() && fits && room && saved && no_tab && quiet { "OK" } else { "FAIL" },
                (fits, room, saved, no_tab, quiet)
            );
            for (k, f) in crate::features::FEATURES.iter().enumerate() {
                if crate::features::background(f.key).is_none() && crate::features::is_on(&a.settings, f.key) {
                    a.settings_action(S_FEATURE + k as u32);
                }
            }
            let one = a.tab_list().len() == 1 && a.tab_list().contains(&a.tab);
            out += &format!("{:<4} the last panel that's on can't be switched off\n", if one { "OK" } else { "FAIL" });
            // Presets switch on exactly their set and are recognised after.
            let mut ok_p = true;
            for k in 0..crate::features::PRESETS.len() {
                a.settings_action(S_PRESET + k as u32);
                let names: Vec<&str> = a.tab_list().iter().map(|&i| a.modules[i].title()).collect();
                // (Widgets is on too, but it lives in Settings, not the tab bar.)
                let want: Vec<&str> = crate::features::PRESETS[k].2.iter().copied().filter(|t| crate::features::background(t).is_none() && *t != "Widgets").collect();
                ok_p &= crate::features::current_preset(&a.settings) == Some(k) && names.len() == want.len() && want.iter().all(|t| names.contains(t));
            }
            out += &format!("{:<4} presets switch on exactly their panels and are recognised\n", if ok_p { "OK" } else { "FAIL" });
            // A panel that's off isn't ticked: the clipboard records nothing.
            let ci = find(a, "Clipboard").unwrap_or(0);
            crate::features::set_on(&mut a.settings, "Clipboard", false);
            a.settings.clip_on = true;
            a.tick();
            let clip_quiet = crate::modules::clipboard::recording() == Some(false);
            crate::features::set_on(&mut a.settings, "Clipboard", true);
            a.tick();
            let clip_back = crate::modules::clipboard::recording() == Some(true);
            a.settings.clip_on = false;
            let _ = ci;
            out += &format!("{:<4} a switched-off panel does no work (clipboard stops recording, starts again)\n", if clip_quiet && clip_back { "OK" } else { "FAIL" });
            a.settings.hidden_panels.clear();
            a.settings.save();
            a.tick();
        }
        // Mouse wheel on the open notch switches panels; lists still scroll.
        {
            a.toast = None;
            a.queue.clear();
            a.expanded = true;
            a.release_keyboard();
            let tabs = a.tab_list();
            let wi = find(a, "Water Reminder").unwrap_or(0);
            let pos = tabs.iter().position(|&i| i == wi).unwrap_or(0);
            a.tab = wi;
            let tgt = a.target();
            a.cur = tgt;
            a.render(tgt, 0.0);
            let content = (a.tab_bar.cx(), a.tab_bar.b() + 70.0);
            let bar = (a.tab_bar.cx(), a.tab_bar.cy());
            a.mouse = content;
            a.wheel_t = -10.0;
            a.on_wheel(-1.0);
            let down = a.tab == tabs[pos + 1];
            a.wheel_t = -10.0;
            a.on_wheel(1.0);
            let up = a.tab == wi;
            // A fast flick moves one panel, not several.
            a.wheel_t = -10.0;
            for _ in 0..4 {
                a.on_wheel(-1.0);
            }
            let flick = a.tab == tabs[pos + 1];
            a.tab = *tabs.last().unwrap_or(&0);
            a.wheel_t = -10.0;
            a.on_wheel(-1.0);
            let end = a.tab == *tabs.last().unwrap_or(&0);
            // Small touchpad steps add up to one switch.
            a.tab = wi;
            a.wheel_t = -10.0;
            a.on_wheel(-0.4);
            let half = a.tab == wi;
            a.on_wheel(-0.4);
            a.on_wheel(-0.4);
            let pad = half && a.tab == tabs[pos + 1];
            // A to-do list long enough to scroll keeps the wheel; the tab bar
            // still switches.
            let ti = find(a, "TodoList").unwrap_or(0);
            for k in 0..8 {
                a.call(ti, |m, c| {
                    m.action(1, (0.0, 0.0), c);
                    for ch in format!("Wheel test {k}").chars() {
                        m.chr(ch, c);
                    }
                    m.key(0x0D, false, c);
                });
            }
            a.release_keyboard();
            a.tab = ti;
            let tgt = a.target();
            a.cur = tgt;
            a.render(tgt, 0.0);
            a.mouse = content;
            a.wheel_t = -10.0;
            a.on_wheel(-1.0);
            let list = a.tab == ti;
            a.mouse = bar;
            a.wheel_t = -10.0;
            a.on_wheel(-1.0);
            let tpos = tabs.iter().position(|&i| i == ti).unwrap_or(0);
            let over_bar = tabs.get(tpos + 1).map_or(a.tab == ti, |&n| a.tab == n);
            out += &format!(
                "{:<4} mouse wheel switches panels (down, up, one per flick, stops at the end, touchpad), scrolls lists, tab bar always switches {:?}\n",
                if down && up && flick && end && pad && list && over_bar { "OK" } else { "FAIL" },
                (down, up, flick, end, pad, list, over_bar)
            );
            a.expanded = false;
        }
        // Mascot: "Always" and left / centre / right, with and without a live activity.
        {
            let before = (a.settings.mascot_always, a.settings.mascot_pos, a.settings.personality, a.settings.style, a.settings.edge);
            IDLE_OVERRIDE.store(0, std::sync::atomic::Ordering::Relaxed);
            a.settings.personality = true;
            a.settings.style = NotchStyle::Full;
            a.settings.edge = Edge::Top;
            a.expanded = false;
            a.toast = None;
            a.queue.clear();
            a.settings.mascot_always = false;
            a.last_activity = a.t();
            a.update_mood();
            let clock_first = a.mood == Mood::Clock;
            a.settings.mascot_always = true;
            a.update_mood();
            let always = a.mood == Mood::Eyes;
            let mut where_ok = Vec::new();
            for pos in 0..3u8 {
                a.settings.mascot_pos = pos;
                let tgt = a.target();
                a.cur = tgt;
                a.render(tgt, 0.0);
                let notch = a.edge_rect(tgt.0, tgt.1);
                let m = a.mascot_rect.unwrap_or_default();
                let ok = match pos {
                    0 => m.cx() < notch.cx() - 30.0,
                    1 => (m.cx() - notch.cx()).abs() < 4.0,
                    _ => m.cx() > notch.cx() + 30.0,
                };
                where_ok.push(ok && a.mascot_rect.is_some());
            }
            out += &snap(a, "mascot always, right, clock beside it");
            // With music (or any live activity) in the closed notch.
            let pi = find(a, "Prayer Times").unwrap_or(0);
            a.call(pi, |m, c| m.action(crate::modules::prayer::A_TEST_SOON, (0.0, 0.0), c));
            a.update_mood();
            let mut live_ok = Vec::new();
            for pos in 0..3u8 {
                a.settings.mascot_pos = pos;
                let tgt = a.target();
                a.cur = tgt;
                a.render(tgt, 0.0);
                let notch = a.edge_rect(tgt.0, tgt.1);
                let m = a.mascot_rect.unwrap_or_default();
                let wide = if pos == 1 { tgt.0 == 300.0 } else { tgt.0 == 364.0 };
                let side = match pos {
                    0 => m.cx() < notch.x + 50.0,
                    1 => (m.cx() - notch.cx()).abs() < 4.0,
                    _ => m.cx() > notch.r() - 50.0,
                };
                live_ok.push(a.mascot_rect.is_some() && wide && side && notch.contains(m.cx(), m.cy()));
                if pos == 0 {
                    out += &snap(a, "mascot always, left, with countdown");
                }
            }
            a.call(pi, |m, c| m.action(crate::modules::prayer::A_TEST_RELOAD, (0.0, 0.0), c));
            // Off again: the clock is back once the notch is used.
            a.settings.mascot_always = false;
            a.expanded = true;
            a.update_mood();
            let back = a.mood == Mood::Clock;
            a.expanded = false;
            out += &format!(
                "{:<4} mascot \"Always\" shows it at once, at left / center / right, beside the clock and beside a live activity {:?}\n",
                if clock_first && always && back && where_ok.iter().all(|x| *x) && live_ok.iter().all(|x| *x) { "OK" } else { "FAIL" },
                (clock_first, always, back, where_ok, live_ok)
            );
            (a.settings.mascot_always, a.settings.mascot_pos, a.settings.personality, a.settings.style, a.settings.edge) = before;
            IDLE_OVERRIDE.store(-1, std::sync::atomic::Ordering::Relaxed);
            a.settings.save();
            a.update_mood();
        }
        // The notch remembers the panel you picked (also after a restart),
        // and returns to it after a reminder opened another one.
        {
            a.toast = None;
            a.queue.clear();
            a.release_keyboard();
            let tabs = a.tab_list();
            let (water, todo) = (find(a, "Water Reminder").unwrap_or(tabs[1]), find(a, "TodoList").unwrap_or(tabs[0]));
            a.expanded = true;
            a.tab = tabs[0];
            let tgt = a.target();
            a.cur = tgt;
            a.render(tgt, 0.0);
            if let Some(h) = a.hits.iter().find(|h| h.module == APP_HIT && h.id == water as u32).copied() {
                a.on_down(h.rect.cx(), h.rect.cy());
                a.on_up(h.rect.cx(), h.rect.cy());
            }
            let saved = crate::store::read("settings.txt").lines().any(|l| l.trim() == "last_tab=Water Reminder");
            a.expanded = false;
            a.frame();
            let kept = a.tab == water;
            // A reminder opens To-do for a moment...
            a.call(todo, |_, c| c.open_my_tab());
            let jumped = a.tab == todo && a.expanded;
            a.expanded = false;
            a.leave_at = None;
            a.frame();
            let back = a.tab == water;
            // ...and a restart opens Water again.
            let reloaded = crate::store::Settings::load().last_tab == "Water Reminder";
            out += &format!(
                "{:<4} the notch remembers the last panel you picked, across restarts and after a reminder {:?}\n",
                if saved && kept && jumped && back && reloaded { "OK" } else { "FAIL" },
                (saved, kept, jumped, back, reloaded)
            );
        }
        // AI agents: hook events -> states, toasts and the live wing; the
        // hook process itself; connecting the agents in a scratch home.
        // 12-hour clock (default) and 24-hour.
        {
            use crate::time::{clock, clock_full, fmt_mod, H12};
            let o = std::sync::atomic::Ordering::Relaxed;
            let was = H12.load(o);
            H12.store(true, o);
            let t12 = clock(16, 20) == "4:20"
                && clock_full(16, 20) == "4:20 PM"
                && clock_full(0, 5) == "12:05 AM"
                && clock_full(12, 0) == "12:00 PM"
                && fmt_mod(9 * 60 + 30) == "9:30 AM"
                && crate::modules::prayer::hhmm(15.0 + 47.0 / 60.0) == "3:47 PM";
            H12.store(false, o);
            let t24 = clock(16, 20) == "16:20" && clock_full(9, 5) == "09:05" && fmt_mod(21 * 60) == "21:00" && crate::modules::prayer::hhmm(15.0 + 47.0 / 60.0) == "15:47";
            H12.store(was, o);
            let default_on = crate::store::Settings::load().clock12 || crate::store::read("settings.txt").contains("clock12=1");
            out += &format!("{:<4} clock: 12-hour by default (4:20 PM), 24-hour when switched off ({t12}, {t24}, {default_on})\n", if t12 && t24 && default_on { "OK" } else { "FAIL" });
            let before = a.settings.clock12;
            a.settings_action(S_CLOCK12);
            let off = !a.settings.clock12 && !crate::time::h12() && crate::store::read("settings.txt").contains("clock12=0");
            a.settings_action(S_CLOCK12);
            let on = a.settings.clock12 == !before || a.settings.clock12 == before;
            a.settings.clock12 = before;
            H12.store(before, o);
            a.settings.save();
            out += &format!("{:<4} Settings > General: 12-hour clock switch applies at once and saves\n", if off && on { "OK" } else { "FAIL" });
        }
        // Make room: over a maximized window the Full notch becomes a thin
        // click-through bar; resting on it opens the notch.
        {
            let o = std::sync::atomic::Ordering::Relaxed;
            let saved = (a.settings.style, a.settings.make_room, a.expanded, a.hover_block);
            a.settings.style = NotchStyle::Full;
            a.settings.make_room = true;
            a.expanded = false;
            a.toast = None;
            a.queue.clear();
            a.hover_block = false;
            let full = a.target();
            MAXIMIZED_OVERRIDE.store(1, o);
            a.update_make_room();
            let slim = a.target();
            let ex = unsafe { GetWindowLongPtrW(a.hwnd, GWL_EXSTYLE) } & WS_EX_TRANSPARENT.0 as isize != 0;
            let yielded = a.look() == NotchStyle::Pill && slim.1 == 0.0 && full.1 > 0.0 && a.passthrough && ex;
            // A toast must be clickable.
            a.push_toast(Toast::new("t", glyph::BELL, col::GREEN, "Test", "toast"));
            a.sync_passthrough();
            let toast_clicks = !a.passthrough;
            a.toast = None;
            a.queue.clear();
            a.sync_passthrough();
            // Rest the cursor on the bar: opens after the hover delay.
            let zone = a.edge_rect(220.0, 16.0);
            a.poll_at(zone.cx(), zone.cy());
            let not_yet = !a.expanded;
            std::thread::sleep(std::time::Duration::from_millis(360));
            a.poll_at(zone.cx(), zone.cy());
            let opened = a.expanded && !a.passthrough;
            a.expanded = false;
            a.sync_passthrough();
            // Away from the bar nothing opens.
            a.poll_at(zone.cx(), zone.b() + 200.0);
            std::thread::sleep(std::time::Duration::from_millis(360));
            a.poll_at(zone.cx(), zone.b() + 200.0);
            let stays = !a.expanded;
            // Setting off: back to the full notch, clickable.
            a.settings_action(S_MAKE_ROOM);
            let off = a.look() == NotchStyle::Full && !a.passthrough && crate::store::read("settings.txt").contains("make_room=0");
            a.settings_action(S_MAKE_ROOM);
            MAXIMIZED_OVERRIDE.store(2, o);
            a.update_make_room();
            let back = a.look() == NotchStyle::Full && !a.passthrough && unsafe { GetWindowLongPtrW(a.hwnd, GWL_EXSTYLE) } & WS_EX_TRANSPARENT.0 as isize == 0;
            (a.settings.style, a.settings.make_room, a.expanded, a.hover_block) = saved;
            a.settings.save();
            out += &format!(
                "{:<4} make room: thin click-through bar over a maximized window, toasts still clickable, rest on it to open, setting off restores {:?}\n",
                if yielded && toast_clicks && not_yet && opened && stays && off && back { "OK" } else { "FAIL" },
                (yielded, toast_clicks, not_yet, opened, stays, off, back)
            );
            let real = unsafe {
                let fg = GetForegroundWindow();
                !fg.0.is_null() && IsZoomed(fg).as_bool()
            };
            out += &format!("INFO foreground window maximized right now: {real}\n");
        }
        // Taskbar mascot placement (pure layout; the window itself needs his taskbar).
        out += &format!("{:<4} taskbar mascot: left corner, right of the weather, beside the tray, hidden when it can't fit\n", if crate::taskbar::self_check() { "OK" } else { "FAIL" });
        // Music: a refused prev/next falls back to the media key.
        out += &format!("{:<4} music: prev/next/play fall back to the media keys when the player refuses\n", if crate::modules::music::test_fallback() { "OK" } else { "FAIL" });
        // Copy toast ("Show what I copy"): off by default; text, picture and
        // files previews; a repeat isn't shown twice; nothing is recorded.
        if let Some(ci) = find(a, "Clipboard") {
            use crate::modules::clipboard as cb;
            let saved = (a.settings.copy_toast, a.settings.clip_on);
            let default_off = !crate::store::Settings::load().copy_toast || !crate::store::read("settings.txt").contains("copy_toast=1");
            let items0 = a.modules[ci].debug_state();
            let feed = |a: &mut App, id: u32| {
                a.toast = None;
                a.queue.clear();
                a.call(ci, |m, c| m.action(id, (0.0, 0.0), c));
                a.call(ci, |m, c| m.tick(c));
            };
            let shown = |a: &App| a.toast.as_ref().map(|t| t.0.clone()).or_else(|| a.queue.front().cloned()).filter(|t| t.key == "copy");
            a.settings.copy_toast = false;
            feed(a, cb::A_TEST_COPY_TEXT);
            let quiet_off = shown(a).is_none();
            a.settings.copy_toast = true;
            a.settings.clip_on = false;
            feed(a, cb::A_TEST_COPY_TEXT);
            let text = shown(a).map_or(false, |t| t.title == "Copied" && t.sub.starts_with("Meeting moved to 4:30 pm") && t.right == "2 lines");
            feed(a, cb::A_TEST_COPY_TEXT);
            let no_repeat = shown(a).is_none();
            feed(a, cb::A_TEST_COPY_IMAGE);
            let pic = shown(a).map_or(false, |t| t.title == "Copied a picture" && t.sub == "1600 x 1000" && t.image.as_ref().map_or(false, |i| i.0 == i.1 && i.0 == 40));
            feed(a, cb::A_TEST_COPY_FILES);
            let files = shown(a).map_or(false, |t| t.title == "Copied 2 files" && t.sub.contains("Report final.docx") && t.sub.contains("Logo.png"));
            let kept_nothing = a.modules[ci].debug_state().split(' ').next() == items0.split(' ').next();
            out += &snap(a, "copy toast");
            a.toast = None;
            a.queue.clear();
            (a.settings.copy_toast, a.settings.clip_on) = saved;
            a.settings.save();
            out += &format!(
                "{:<4} copy toast: off by default, previews text (2 lines), a picture and files, no repeat, keeps nothing {:?}\n",
                if default_off && quiet_off && text && no_repeat && pic && files && kept_nothing { "OK" } else { "FAIL" },
                (default_off, quiet_off, text, no_repeat, pic, files, kept_nothing)
            );
        }
        // Screenshot / Pick from screen: the notch window is hidden while
        // the screen is frozen and comes back after (a tiny off-screen
        // stand-in window plays the notch).
        {
            use crate::modules::tools::picker;
            let stand_in = unsafe {
                CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE, w!("STATIC"), w!(""), WS_POPUP, -32000, -32000, 4, 4, None, None, None, None).unwrap_or_default()
            };
            unsafe {
                let _ = ShowWindow(stand_in, SW_SHOWNOACTIVATE);
            }
            let shown_before = unsafe { IsWindowVisible(stand_in).as_bool() };
            let snap = picker::capture_without(stand_in);
            let hidden_during = unsafe { !IsWindowVisible(stand_in).as_bool() };
            picker::show_app(stand_in);
            let back = unsafe { IsWindowVisible(stand_in).as_bool() };
            // Not hidden by us: left alone.
            unsafe {
                let _ = ShowWindow(stand_in, SW_HIDE);
            }
            picker::hide_app(stand_in);
            picker::show_app(stand_in);
            let left_alone = unsafe { !IsWindowVisible(stand_in).as_bool() };
            unsafe {
                let _ = DestroyWindow(stand_in);
            }
            out += &format!(
                "{:<4} screenshot and color picker hide the notch while the screen is frozen, then bring it back {:?}\n",
                if shown_before && snap.is_some() && hidden_during && back && left_alone { "OK" } else { "FAIL" },
                (shown_before, snap.is_some(), hidden_during, back, left_alone)
            );
            // Clicking Screenshot closes the panel and it stays closed with
            // the cursor still on the button.
            if let Some(sh) = find(a, "Shelf") {
                a.hover_block = false;
                a.expanded = true;
                a.tab = sh;
                a.call(sh, |m, c| m.action(crate::modules::shelf::A_TEST_NO_OVERLAY, (0.0, 0.0), c));
                a.call(sh, |m, c| m.action(crate::modules::shelf::A_SNIP, (0.0, 0.0), c));
                let blocked = !a.expanded && a.hover_block && !a.can_hover_open();
                a.hover_block = false;
                out += &format!("{:<4} Screenshot closes the panel and it doesn't reopen under the cursor\n", if blocked { "OK" } else { "FAIL" });
            }
        }
        if let Some(ai) = find(a, "AI Agents") {
            use crate::modules::agents::{self as ag, Link};
            use serde_json::json;
            let toasted = |a: &App, f: &dyn Fn(&crate::modules::Toast) -> bool| a.toast.as_ref().map_or(false, |t| f(&t.0)) || a.queue.iter().any(|t| f(t));
            let clear = |a: &mut App| {
                a.toast = None;
                a.queue.clear();
            };
            let feed = |a: &mut App, j: serde_json::Value| {
                ag::push_event(j.to_string());
                a.call(ai, |m, c| m.tick(c));
            };
            clear(a);
            out += &snap(a, "agents: nothing connected");
            feed(a, json!({"agent": "claude", "event": "SessionStart", "session": "t1", "cwd": "C:\\Projects\\shop"}));
            let ready = a.modules[ai].debug_state().contains("claude:shop:Ready") && a.modules[ai].live().is_none();
            feed(a, json!({"agent": "claude", "event": "UserPromptSubmit", "session": "t1", "cwd": "C:\\Projects\\shop"}));
            let working = a.modules[ai].debug_state().contains("claude:shop:Working") && a.modules[ai].live() == Some(45);
            feed(a, json!({"agent": "claude", "event": "Notification", "session": "t1", "message": "Claude needs your permission to use Bash"}));
            let asked = a.modules[ai].debug_state().contains("NeedsYou") && a.modules[ai].live() == Some(75) && toasted(a, &|t| t.key == "agent" && t.title == "Claude Code needs you" && t.sub.contains("Bash"));
            feed(a, json!({"agent": "claude", "event": "PostToolUse", "session": "t1", "tool": "Bash"}));
            let back = a.modules[ai].debug_state().contains("claude:shop:Working");
            clear(a);
            feed(a, json!({"agent": "claude", "event": "Stop", "session": "t1"}));
            let done = a.modules[ai].debug_state().contains("claude:shop:Done") && a.modules[ai].live() == Some(50) && toasted(a, &|t| t.title == "Claude Code finished");
            clear(a);
            feed(a, json!({"agent": "claude", "event": "Notification", "session": "t1", "message": "Claude is waiting for your input"}));
            let quiet = a.toast.is_none() && a.queue.is_empty() && a.modules[ai].debug_state().contains("claude:shop:Done");
            feed(a, json!({"agent": "codex", "event": "Stop", "session": "c1", "cwd": "D:/work/api"}));
            let codex = a.modules[ai].debug_state().contains("codex:api:Done") && toasted(a, &|t| t.title == "Codex finished");
            clear(a);
            feed(a, json!({"agent": "claude", "event": "SessionEnd", "session": "t1"}));
            let ended = !a.modules[ai].debug_state().contains("claude:shop");
            out += &format!(
                "{:<4} agents: ready, working, needs you (toast), back to work, done (toast), idle nudge ignored, Codex done, session end {:?}\n",
                if ready && working && asked && back && done && quiet && codex && ended { "OK" } else { "FAIL" },
                (ready, working, asked, back, done, quiet, codex, ended)
            );
            // Hermes: stream start = working, a question/approval = needs you,
            // turn end = done, Esc = stopped, session closed = gone.
            feed(a, json!({"agent": "hermes", "event": "on_stream_start", "session": "h1", "cwd": "C:\\Users\\Dev", "surface": "desktop", "model": "anthropic/claude-opus"}));
            let h_work = a.modules[ai].debug_state().contains("hermes:desktop:Working") && a.modules[ai].live() == Some(45);
            feed(a, json!({"agent": "hermes", "event": "on_human_input_request", "session": "h1", "kind": "approval"}));
            let h_ask = a.modules[ai].debug_state().contains("hermes:desktop:NeedsYou") && toasted(a, &|t| t.title == "Hermes needs you" && t.sub.contains("OK"));
            feed(a, json!({"agent": "hermes", "event": "on_human_input_resolved", "session": "h1"}));
            let h_back = a.modules[ai].debug_state().contains("hermes:desktop:Working");
            clear(a);
            feed(a, json!({"agent": "hermes", "event": "on_session_end", "session": "h1", "interrupted": true}));
            let h_stop = a.modules[ai].debug_state().contains("hermes:desktop:Ready") && a.toast.is_none() && a.queue.is_empty();
            feed(a, json!({"agent": "hermes", "event": "on_stream_start", "session": "h1"}));
            feed(a, json!({"agent": "hermes", "event": "on_session_end", "session": "h1"}));
            let h_done = a.modules[ai].debug_state().contains("hermes:desktop:Done") && toasted(a, &|t| t.title == "Hermes finished");
            feed(a, json!({"agent": "hermes", "event": "on_session_finalize", "session": "h1"}));
            let h_gone = !a.modules[ai].debug_state().contains("hermes:desktop");
            clear(a);
            out += &format!(
                "{:<4} Hermes: working, needs you (toast), back to work, Esc = stopped (no toast), done (toast), closed {:?}\n",
                if h_work && h_ask && h_back && h_stop && h_done && h_gone { "OK" } else { "FAIL" },
                (h_work, h_ask, h_back, h_stop, h_done, h_gone)
            );
            let hp = json!({"hook_event_name": "on_human_input_request", "tool_name": null, "tool_input": null, "session_id": "s9", "cwd": "C:/Users/x", "profile": "default", "extra": {"kind": "approval", "prompt": "rm -rf [REDACTED]", "conversation_history": ["SECRET"], "user_message": "SECRET"}});
            let hs = ag::slim("hermes", &hp).to_string();
            out += &format!("{:<4} Hermes hook payload slimmed ({hs})\n", if !hs.contains("SECRET") && hs.contains("approval") && hs.contains("s9") { "OK" } else { "FAIL" });
            // Hermes hooks merge: keeps the user's own hooks and other sections.
            let cur = json!({"pre_tool_call": [{"command": "mine.sh", "matcher": "terminal"}], "outbound": [{"url": "https://x"}]});
            let cmd = ag::hook_command("hermes");
            let on = ag::merge_hermes_hooks(&cur, &cmd, true);
            let twice = ag::merge_hermes_hooks(&on, &cmd, true);
            let off = ag::merge_hermes_hooks(&on, &cmd, false);
            let ok = on == twice && off == cur && ag::HERMES_EVENTS.iter().all(|e| on[*e].as_array().map_or(false, |l| l.len() == 1)) && on["pre_tool_call"] == cur["pre_tool_call"];
            out += &format!("{:<4} Hermes hooks: added once per event, the user's hooks and outbound kept, removal restores\n", if ok { "OK" } else { "FAIL" });
            if std::env::var("NOOKLY_TEST_HERMES").is_ok() {
                // The real hermes CLI against a scratch HERMES_HOME (never the user's).
                let hh = crate::store::dir().join("hermes-home");
                let _ = std::fs::remove_dir_all(&hh);
                let _ = std::fs::create_dir_all(&hh);
                let _ = std::fs::write(hh.join("config.yaml"), "hooks:\n  pre_tool_call:\n  - command: mine.sh\n    matcher: terminal\n");
                // Safe: the self-test and doc renders run on one thread.
                unsafe { std::env::set_var("NOOKLY_HERMES_HOME", &hh) };
                let t0 = std::time::Instant::now();
                let c1 = ag::hermes_connect(true);
                let cfg = std::fs::read_to_string(hh.join("config.yaml")).unwrap_or_default();
                let allow = std::fs::read_to_string(hh.join("shell-hooks-allowlist.json")).unwrap_or_default();
                let linked = ag::hermes_link() == Link::On;
                let c2 = ag::hermes_connect(false);
                let cfg2 = std::fs::read_to_string(hh.join("config.yaml")).unwrap_or_default();
                let allow2 = std::fs::read_to_string(hh.join("shell-hooks-allowlist.json")).unwrap_or_default();
                let ok = c1.is_ok() && c2.is_ok() && linked && cfg.contains("on_stream_start") && cfg.contains("mine.sh") && allow.matches("--agent-event hermes").count() == 6 && !cfg2.contains("agent-event") && cfg2.contains("mine.sh") && !allow2.contains("agent-event") && ag::hermes_link() == Link::Off;
                out += &format!("{:<4} real hermes CLI: connect writes 6 hooks + consent, keeps the user's hook, disconnect removes them ({:.1} s) {:?}\n", if ok { "OK" } else { "FAIL" }, t0.elapsed().as_secs_f32(), (c1, c2, linked));
                // Safe: the self-test and doc renders run on one thread.
                unsafe { std::env::remove_var("NOOKLY_HERMES_HOME") };
            }
            // The hook keeps only status fields: no prompt, no file contents.
            let big = json!({"hook_event_name": "PostToolUse", "session_id": "s", "cwd": "C:/x/y", "tool_name": "Read", "prompt": "SECRET-PROMPT", "tool_input": {"file_path": "C:/x/.env"}, "tool_response": {"content": "SECRET-FILE"}});
            let slim = ag::slim("claude", &big).to_string();
            out += &format!("{:<4} the hook passes on only event, session, folder, tool and notice ({slim})\n", if !slim.contains("SECRET") && !slim.contains(".env") && slim.contains("Read") { "OK" } else { "FAIL" });
            // The real hook process, as Claude Code and Codex run it.
            let exe = std::env::current_exe().unwrap_or_default();
            let mut timings = Vec::new();
            for (agent, payload) in [
                ("claude", json!({"hook_event_name": "UserPromptSubmit", "session_id": "rt1", "cwd": "C:/Projects/roundtrip", "prompt": "SECRET-PROMPT"}).to_string()),
                ("codex", json!({"type": "agent-turn-complete", "thread-id": "rt2", "cwd": "C:/Projects/codexrt", "last-assistant-message": "SECRET"}).to_string()),
            ] {
                use std::io::Write;
                let t0 = std::time::Instant::now();
                let mut cmd = std::process::Command::new(&exe);
                cmd.args(["--agent-event", agent]).env("NOOKLY_AGENT_HWND", (a.hwnd.0 as isize).to_string()).stdin(std::process::Stdio::piped());
                if agent == "codex" {
                    cmd.arg(&payload);
                }
                if let Ok(mut child) = cmd.spawn() {
                    if let Some(mut si) = child.stdin.take() {
                        if agent == "claude" {
                            let _ = si.write_all(payload.as_bytes());
                        }
                    }
                    for _ in 0..500 {
                        unsafe {
                            let mut msg = MSG::default();
                            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                                let _ = TranslateMessage(&msg);
                                DispatchMessageW(&msg);
                            }
                        }
                        if child.try_wait().ok().flatten().is_some() {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                }
                timings.push(t0.elapsed().as_millis());
            }
            a.call(ai, |m, c| m.tick(c));
            let st = a.modules[ai].debug_state();
            let rt = st.contains("claude:roundtrip:Working") && st.contains("codex:codexrt:Done") && !st.contains("SECRET");
            out += &format!("{:<4} hook processes reach the notch (Claude via stdin, Codex via argument) in {:?} ms\n", if rt && timings.iter().all(|t| *t < 3000) { "OK" } else { "FAIL" }, timings);
            clear(a);
            // Connecting, in a scratch home (never the real ~/.claude).
            let home = crate::store::dir().join("agent-home");
            let _ = std::fs::remove_dir_all(&home);
            let _ = std::fs::create_dir_all(&home);
            // Safe: the self-test and doc renders run on one thread.
            unsafe { std::env::set_var("NOOKLY_AGENT_HOME", &home) };
            let missing = ag::claude_link() == Link::Missing && ag::claude_connect(true).is_err() && ag::codex_link() == Link::Missing;
            let _ = std::fs::create_dir_all(home.join(".claude"));
            let original = "{\n  \"model\": \"opus\",\n  \"hooks\": {\n    \"Stop\": [\n      {\n        \"hooks\": [\n          {\n            \"type\": \"command\",\n            \"command\": \"echo mine\"\n          }\n        ]\n      }\n    ]\n  },\n  \"zzz\": 1\n}\n";
            let _ = std::fs::write(home.join(".claude/settings.json"), original);
            let c1 = ag::claude_connect(true).is_ok() && ag::claude_connect(true).is_ok() && ag::claude_link() == Link::On;
            let text = std::fs::read_to_string(home.join(".claude/settings.json")).unwrap_or_default();
            let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
            let ours = |ev: &str| v["hooks"][ev].as_array().map_or(0, |l| l.iter().filter(|e| e.to_string().contains("--agent-event")).count());
            let all_once = ag::CLAUDE_EVENTS.iter().all(|e| ours(e) == 1);
            let kept = text.contains("echo mine") && text.find("\"model\"") < text.find("\"hooks\"") && text.find("\"hooks\"") < text.find("\"zzz\"");
            let c2 = ag::claude_connect(false).is_ok() && ag::claude_link() == Link::Off;
            let restored = std::fs::read_to_string(home.join(".claude/settings.json")).unwrap_or_default() == original;
            let backup = home.join(".claude/settings.json.nookly-backup").exists();
            out += &format!(
                "{:<4} Claude Code connect adds 6 hooks once, keeps the user's settings and order, disconnect restores the file exactly {:?}\n",
                if missing && c1 && all_once && kept && c2 && restored && backup { "OK" } else { "FAIL" },
                (missing, c1, all_once, kept, c2, restored, backup)
            );
            let _ = std::fs::create_dir_all(home.join(".codex"));
            let conf = "model = \"o3\"\n\n[tui]\nnotify = true\n";
            let _ = std::fs::write(home.join(".codex/config.toml"), conf);
            let x1 = ag::codex_connect(true).is_ok() && ag::codex_link() == Link::On;
            let t = std::fs::read_to_string(home.join(".codex/config.toml")).unwrap_or_default();
            let top = t.lines().nth(1).map_or(false, |l| l.starts_with("notify = [") && l.ends_with("\"--agent-event\", \"codex\"]"));
            let x2 = ag::codex_connect(false).is_ok() && std::fs::read_to_string(home.join(".codex/config.toml")).unwrap_or_default() == conf;
            let theirs = "notify = [\"my-notifier\"]\nmodel = \"o3\"\n";
            let _ = std::fs::write(home.join(".codex/config.toml"), theirs);
            let x3 = ag::codex_connect(true).is_err() && std::fs::read_to_string(home.join(".codex/config.toml")).unwrap_or_default() == theirs;
            out += &format!("{:<4} Codex connect adds a top-level notify line, disconnect restores, an existing notify is left alone {:?}\n", if x1 && top && x2 && x3 { "OK" } else { "FAIL" }, (x1, top, x2, x3));
            // Uninstall unhooks both.
            let _ = std::fs::write(home.join(".codex/config.toml"), conf);
            let _ = ag::claude_connect(true);
            let _ = ag::codex_connect(true);
            ag::disconnect_all();
            out += &format!("{:<4} uninstall disconnects both agents\n", if ag::claude_link() == Link::Off && ag::codex_link() == Link::Off { "OK" } else { "FAIL" });
            // Never the real Hermes during tests.
            // Safe: the self-test and doc renders run on one thread.
            unsafe { std::env::remove_var("NOOKLY_HERMES_HOME") };
            out += &format!("{:<4} self-test can't reach the real Hermes or agent settings ({:?}, {:?})\n", if ag::hermes_home().is_none() && ag::hermes_link() == Link::Missing { "OK" } else { "FAIL" }, ag::hermes_home(), ag::hermes_link());
            // Safe: the self-test and doc renders run on one thread.
            unsafe { std::env::remove_var("NOOKLY_AGENT_HOME") };
            let _ = std::fs::remove_dir_all(&home);
            a.call(ai, |m, c| m.action(ag::A_DEMO, (0.0, 0.0), c));
            a.expanded = true;
            a.tab = ai;
            out += &snap(a, "agents: three sessions");
            let rows = a.hits.iter().filter(|h| h.module == ai && h.id > (1 << 24)).count();
            out += &format!("{:<4} agents panel lists the sessions ({rows} rows)\n", if rows == 3 { "OK" } else { "FAIL" });
            // The three switches fit their labels.
            let mut tight = Vec::new();
            for (name, w) in ["Hermes", "Claude Code", "Codex"].iter().zip(ag::TOGGLE_W) {
                if a.gfx.measure(name, 12.5, Weight::Regular) + 58.0 > w {
                    tight.push(*name);
                }
            }
            let left = modules::PANEL.0 - ag::TOGGLE_W.iter().sum::<f32>() - 22.0;
            if a.gfx.measure("No agents running", 12.0, Weight::Regular) > left {
                tight.push("status");
            }
            out += &format!("{:<4} agents header switches fit {:?}\n", if tight.is_empty() { "OK" } else { "FAIL" }, tight);
            a.call(ai, |m, c| m.action(3, (0.0, 0.0), c));
            clear(a);
            a.expanded = false;
        } else {
            out += "FAIL AI Agents panel missing\n";
        }

        // System monitor: Windows counters, formatting, the closed-notch readout.
        if let Some(mi) = find(a, "System Monitor") {
            use crate::modules::monitor as mo;
            let c1 = mo::cpu_times();
            std::thread::sleep(std::time::Duration::from_millis(300));
            let c2 = mo::cpu_times();
            let cpu = matches!((c1, c2), (Some((i1, t1)), Some((i2, t2))) if t2 > t1 && i2 >= i1 && i2 - i1 <= t2 - t1);
            let (used, total) = mo::memory();
            let mem = total > (1u64 << 30) && used > 0 && used < total;
            let net = mo::net_bytes();
            let (free, dt) = mo::disk();
            let disk = dt > 0 && free <= dt;
            out += &format!("{:<4} system counters: CPU times, memory {}, network {:?}, disk {} free of {}\n", if cpu && mem && net.is_some() && disk { "OK" } else { "FAIL" }, mo::gb(total), net, mo::gb(free), mo::gb(dt));
            let fm = [mo::speed(0.0), mo::speed(1536.0), mo::speed(5.5 * 1048576.0), mo::gb(17_179_869_184), mo::gb(511_000_000_000)];
            out += &format!("{:<4} speeds and sizes read well {:?}\n", if fm == ["0 B/s", "2 KB/s", "5.5 MB/s", "16.0 GB", "476 GB"] { "OK" } else { "FAIL" }, fm);
            std::thread::sleep(std::time::Duration::from_millis(1000));
            a.call(mi, |m, c| m.tick(c));
            let st = a.modules[mi].debug_state();
            let sampled = st.starts_with("cpu ") && !st.contains("samples 0");
            let ids: Vec<&str> = a.modules[mi].widgets().iter().map(|w| w.id).collect();
            let ok = sampled && a.modules[mi].live().is_none() && ids == ["cpu", "ram", "net"];
            out += &format!("{:<4} monitor samples once a second and offers CPU, memory and network widgets {ids:?} ({st})\n", if ok { "OK" } else { "FAIL" });
            a.expanded = true;
            a.tab = mi;
            out += &snap(a, "monitor panel");
            a.expanded = false;
        } else {
            out += "FAIL System Monitor panel missing\n";
        }

        // Clipboard: snippets and search (no clipboard writes).
        if let Some(ci) = find(a, "Clipboard") {
            use crate::modules::clipboard as cb;
            let field = |s: &str, k: &str| -> String { s.split_whitespace().find_map(|w| w.strip_prefix(&format!("{k}="))).unwrap_or("").to_string() };
            let head_w = 500.0 - 156.0 - 68.0 - 30.0 - 34.0;
            let mut tight = Vec::new();
            for (on, mode) in [(true, 0u8), (true, 1), (true, 2), (false, 0)] {
                let s = cb::status_line(on, mode);
                if a.gfx.measure(&s, 12.0, Weight::Regular) > head_w {
                    tight.push(s);
                }
            }
            out += &format!("{:<4} clipboard header text fits beside the buttons {:?}\n", if tight.is_empty() { "OK" } else { "FAIL" }, tight);
            a.expanded = true;
            a.tab = ci;
            a.call(ci, |m, c| m.action(cb::A_SNIPPET, (0.0, 0.0), c));
            let kb = a.keyboard == Some(ci);
            for ch in "Road 12, Dhanmondi".chars() {
                a.on_char(ch);
            }
            out += &snap(a, "clipboard: typing a snippet");
            a.on_key(0x0D);
            let saved = crate::store::read("clipboard.txt").contains("Road 12, Dhanmondi") && a.keyboard.is_none();
            a.call(ci, |m, c| m.action(cb::A_DEMO, (0.0, 0.0), c));
            a.call(ci, |m, c| m.action(cb::A_SEARCH, (0.0, 0.0), c));
            for ch in "ENTRY 3".chars() {
                a.on_char(ch);
            }
            out += &snap(a, "clipboard: searching");
            let shown = field(&a.modules[ci].debug_state(), "shown");
            a.on_key(0x1B);
            let cleared = field(&a.modules[ci].debug_state(), "shown") != shown && a.keyboard == Some(ci);
            a.on_key(0x1B);
            let closed = a.keyboard.is_none();
            out += &format!(
                "{:<4} clipboard snippet saved as a pin; search filters (\"entry 3\" shows {shown}), Esc clears then closes {:?}\n",
                if kb && saved && shown == "6" && cleared && closed { "OK" } else { "FAIL" },
                (kb, saved, cleared, closed)
            );
            a.call(ci, |m, c| m.action(1, (0.0, 0.0), c));
            a.expanded = false;
        }

        // Shelf screenshot: closes the notch, saves a PNG, adds it, reopens.
        if let Some(si) = find(a, "Shelf") {
            use crate::modules::{shelf, snip};
            a.call(si, |m, c| m.action(shelf::A_TEST_NO_OVERLAY, (0.0, 0.0), c));
            a.expanded = true;
            a.tab = si;
            let tgt = a.target();
            a.cur = tgt;
            a.render(tgt, 0.0);
            let button = a.hits.iter().find(|h| h.module == si && h.id == shelf::A_SNIP).copied();
            if let Some(h) = button {
                a.on_down(h.rect.cx(), h.rect.cy());
                a.on_up(h.rect.cx(), h.rect.cy());
            }
            let closed = !a.expanded;
            let px: Vec<u8> = (0..40 * 20).flat_map(|i| [(i % 40 * 6) as u8, (i / 40 * 12) as u8, 200, 255]).collect();
            *snip::RESULT.lock().unwrap() = Some(Some(snip::Shot { w: 40, h: 20, bgra: px }));
            a.tick();
            let back = a.expanded && a.tab == si;
            let st = a.modules[si].debug_state();
            let dir = snip::folder();
            let file = std::fs::read_dir(&dir).ok().and_then(|d| d.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| p.extension().map_or(false, |x| x == "png")));
            let bytes = file.as_ref().and_then(|f| std::fs::read(f).ok()).unwrap_or_default();
            let png = bytes.starts_with(&[0x89, b'P', b'N', b'G']) && unsafe { crate::modules::music::decode_square(&bytes, 20) }.is_some();
            let dib = snip::dib32(&snip::Shot { w: 3, h: 2, bgra: vec![0; 24] }).len() == 40 + 24;
            out += &format!(
                "{:<4} Shelf screenshot: closes the notch, saves a real PNG, adds it to the Shelf, reopens ({}) {:?}\n",
                if button.is_some() && closed && back && st.contains("Screenshot ") && png && dib { "OK" } else { "FAIL" },
                st,
                (button.is_some(), closed, back, png, dib)
            );
            out += &snap(a, "shelf with a screenshot");
            if std::env::var("NOOKLY_TEST_PICKER").is_ok() {
                let up = snip::start(a.hwnd);
                let dragged = snip::test_drag(10, 10, 110, 60);
                let got = snip::RESULT.lock().unwrap().take();
                let size = got.as_ref().and_then(|g| g.as_ref().map(|s| (s.w, s.h)));
                out += &format!("{:<4} screenshot window opens and captures the dragged area ({:?})\n", if up && dragged && size == Some((101, 51)) && !snip::active() { "OK" } else { "FAIL" }, size);
            }
            if let Some(f) = &file {
                let _ = std::fs::remove_file(f);
            }
            a.call(si, |m, c| m.action(1, (0.0, 0.0), c));
            a.expanded = false;
        }

        // Self-uninstall: what it would do for this exe.
        {
            let plan = crate::uninstall::plan(false).join(" | ");
            let ok = plan.contains("disconnect Hermes, Claude Code and Codex") && plan.contains("delete data folder") && (plan.contains("unins000.exe") || plan.contains("if it is empty")) && !crate::uninstall::plan(true).join(" ").contains("delete data folder");
            out += &format!("{:<4} uninstall plan ({plan})\n", if ok { "OK" } else { "FAIL" });
        }

        // Tools tab: calculator, converter, time zones, colours, QR, picker.
        {
            use crate::modules::tools::{self, convert, eval, fmt_num, tz};
            let cases: [(&str, f64); 24] = [
                ("1250*15%", 187.5),
                ("200+10%", 220.0),
                ("200-10%", 180.0),
                ("100*(1+5%)", 105.0),
                ("50%", 0.5),
                ("2^10", 1024.0),
                ("-2^2", -4.0),
                ("2^-1", 0.5),
                ("(1+2)*3", 9.0),
                ("2(3+4)", 14.0),
                ("3x4", 12.0),
                ("12 × 3 ÷ 4", 9.0),
                ("sqrt(16)+2", 6.0),
                ("sin(30)", 0.5),
                ("cos(60)", 0.5),
                ("log(1000)", 3.0),
                ("0xFF+1", 256.0),
                ("0b1010", 10.0),
                ("10/4", 2.5),
                ("5!", 120.0),
                ("1,250+250", 1500.0),
                ("(2+3", 5.0),
                ("2e3", 2000.0),
                ("2pi", std::f64::consts::TAU),
            ];
            let mut bad = Vec::new();
            for (e, want) in cases {
                match eval(e) {
                    Ok(v) if (v - want).abs() <= 1e-9 * want.abs().max(1.0) => {}
                    r => bad.push(format!("{e} -> {r:?}")),
                }
            }
            for e in ["1/0", "2+", "foo(3)", "(1+2))", "3!!!!"] {
                if eval(e).is_ok() && e != "3!!!!" {
                    bad.push(format!("{e} should be an error"));
                }
            }
            for (v, want) in [(1234567.891, "1,234,567.891"), (0.1 + 0.2, "0.3"), (-1500.0, "-1,500"), (2.0f64.sqrt(), "1.414213562"), (0.0, "0")] {
                if fmt_num(v) != want {
                    bad.push(format!("fmt {v} -> {}", fmt_num(v)));
                }
            }
            out += &format!("{:<4} calculator: 24 sums incl. 15%, +10%, 2^10, sqrt, sin, hex, 5!, and errors {:?}\n", if bad.is_empty() { "OK" } else { "FAIL" }, bad);

            let mut bad = Vec::new();
            for (cat, from, to, v, want, what) in [
                (0usize, 0usize, 4usize, 1.0, 0.621371192, "1 km in mi"),
                (2, 0, 1, 100.0, 212.0, "100 C in F"),
                (2, 1, 0, -40.0, -40.0, "-40 F in C"),
                (2, 0, 2, 0.0, 273.15, "0 C in K"),
                (3, 6, 1, 5.0, 3600.0, "5 katha in ft2"),
                (3, 7, 6, 1.0, 20.0, "1 bigha in katha"),
                (3, 4, 5, 1.0, 100.0, "1 acre in decimal"),
                (1, 5, 6, 1.0, 40.0, "1 maund in seer"),
                (1, 6, 7, 1.0, 80.0, "1 seer in tola"),
                (1, 0, 2, 1.0, 2.20462262, "1 kg in lb"),
                (6, 3, 2, 1.0, 1024.0, "1 GB in MB"),
                (6, 6, 2, 100.0, 11.920928955, "100 Mbit in MB"),
                (5, 0, 2, 100.0, 62.137119224, "100 km/h in mph"),
                (4, 3, 0, 1.0, 3.785411784, "1 gal in L"),
            ] {
                let got = convert(cat, from, to, v);
                if (got - want).abs() > 1e-4 * want.abs().max(1.0) {
                    bad.push(format!("{what} = {got}"));
                }
            }
            out += &format!("{:<4} converter: 14 conversions incl. katha, bigha, decimal, maund, seer, tola, Mbit {:?}\n", if bad.is_empty() { "OK" } else { "FAIL" }, bad);

            let mut bad: Vec<String> = tools::CITIES.iter().filter(|(_, k)| !tz::known(k)).map(|(c, _)| c.to_string()).collect();
            let at = |y: u16, m: u16, d: u16, h: u16| tz::ft(&windows::Win32::Foundation::SYSTEMTIME { wYear: y, wMonth: m, wDay: d, wHour: h, ..Default::default() });
            for (utc, key, want) in [
                (at(2026, 1, 15, 6), "Bangladesh Standard Time", 12),
                (at(2026, 1, 15, 6), "GMT Standard Time", 6),
                (at(2026, 1, 15, 6), "Eastern Standard Time", 1),
                (at(2026, 1, 15, 6), "AUS Eastern Standard Time", 17),
                (at(2026, 7, 15, 6), "GMT Standard Time", 7),
                (at(2026, 7, 15, 6), "Eastern Standard Time", 2),
                (at(2026, 7, 15, 6), "AUS Eastern Standard Time", 16),
                (at(2026, 7, 15, 6), "Arabian Standard Time", 10),
            ] {
                match tz::local(Some(key), utc) {
                    Some(t) if t.wHour == want => {}
                    t => bad.push(format!("{key}: {:?}", t.map(|t| t.wHour))),
                }
            }
            out += &format!("{:<4} time zones: all 12 cities known, daylight saving right in January and July {:?}\n", if bad.is_empty() { "OK" } else { "FAIL" }, bad);
            out += &format!("INFO time zones: {}\n", tz::debug());

            let c = [30u8, 144, 255];
            let fmt = (tools::hex_of(c), tools::rgb_of(c), tools::hsl_of(c));
            let ok = fmt == ("#1E90FF".into(), "rgb(30, 144, 255)".into(), "hsl(210, 100%, 56%)".into()) && (tools::contrast([255, 255, 255], [0, 0, 0]) - 21.0).abs() < 0.01;
            out += &format!("{:<4} color formats HEX / RGB / HSL and contrast {:?}\n", if ok { "OK" } else { "FAIL" }, fmt);

            if let Some(ti) = find(a, "Tools") {
                a.toast = None;
                a.queue.clear();
                a.expanded = true;
                a.tab = ti;
                a.switch_t = -10.0;
                let qr = crate::modules::tools::qr_check("https://github.com/banulalakwindu/Nookly");
                let long = crate::modules::tools::qr_check(&"x".repeat(3500));
                out += &format!("{:<4} QR code for a link ({:?}), image copy size right, too-long text refused\n", if matches!(qr, Some((s, true)) if s >= 21) && long.is_none() { "OK" } else { "FAIL" }, qr);
                let names = ["color", "calculator", "QR code", "converter"];
                let mut missing = Vec::new();
                for (k, id) in [(0usize, 10u32), (1, 40), (2, 62), (3, 100)] {
                    a.call(ti, |m, c| m.action(1 + k as u32, (0.0, 0.0), c));
                    a.release_keyboard();
                    out += &snap(a, &format!("tools: {}", names[k]));
                    if !a.hits.iter().any(|h| h.module == ti && h.id == id) {
                        missing.push(names[k]);
                    }
                }
                a.call(ti, |m, c| m.action(77, (0.0, 0.0), c));
                out += &snap(a, "tools: time zones");
                let cities = a.hits.iter().filter(|h| h.module == ti && (110..122).contains(&h.id)).count();
                a.call(ti, |m, c| m.action(73, (0.0, 0.0), c));
                out += &format!("{:<4} Tools tab shows all four tools and 12 time-zone cities {:?}\n", if missing.is_empty() && cities == 12 { "OK" } else { "FAIL" }, (missing, cities));
                // Typing a sum and pressing Enter keeps it in the history.
                a.call(ti, |m, c| m.action(2, (0.0, 0.0), c));
                let kb = a.keyboard == Some(ti);
                for ch in "1250*15%".chars() {
                    a.on_char(ch);
                }
                a.on_key(0x0D);
                let hist = crate::modules::tools::test_last(a.modules[ti].debug_state());
                a.on_key(0x1B);
                a.on_key(0x1B);
                a.release_keyboard();
                out += &format!("{:<4} calculator takes typing, Enter keeps 1250*15% = 187.5 ({hist})\n", if kb && hist.contains("187.5") { "OK" } else { "FAIL" });
                // Labels fit.
                let mut tight = Vec::new();
                let gfx = &mut a.gfx as *mut Gfx;
                for (cat, room) in tools::unit_room(|s| unsafe { (*gfx).measure(s, 11.5, Weight::Semi) }, modules::PANEL.0 - 118.0 - 8.0) {
                    if room < 0.0 {
                        tight.push(cat);
                    }
                }
                let cat_w = (modules::PANEL.0 - 5.0 * 7.0) / 8.0 - 6.0;
                for c in tools::CATS.iter().map(|c| c.0).chain(["Zones"]) {
                    if a.gfx.measure(c, 11.5, Weight::Semi) > cat_w {
                        tight.push(c);
                    }
                }
                let city_w = (modules::PANEL.0 - 30.0) / 6.0 - 16.0;
                for (c, _) in tools::CITIES {
                    if a.gfx.measure(c, 10.5, Weight::Regular) > city_w {
                        tight.push(c);
                    }
                }
                for (_, n, _) in [("", "Calculator", ""), ("", "Converter", ""), ("", "QR code", ""), ("", "Color", "")] {
                    if a.gfx.measure(n, 11.5, Weight::Semi) + 20.0 > (modules::PANEL.0 - 18.0) / 4.0 - 12.0 {
                        tight.push(n);
                    }
                }
                for (s, w) in [("Pick from screen", 140.0 - 16.0), ("Copy image", 110.0 - 16.0), ("Esc cancels, arrows nudge", 149.5)] {
                    if a.gfx.measure(s, if w > 149.0 { 10.0 } else { 12.0 }, Weight::Semi) > w {
                        tight.push(s);
                    }
                }
                out += &format!("{:<4} Tools labels fit {:?}\n", if tight.is_empty() { "OK" } else { "FAIL" }, tight);
                // "Pick from screen" closes the notch first, then the result
                // reopens the Tools tab with the colour.
                a.call(ti, |m, c| m.action(tools::A_TEST_NO_OVERLAY, (0.0, 0.0), c));
                a.call(ti, |m, c| m.action(1, (0.0, 0.0), c));
                let tgt = a.target();
                a.cur = tgt;
                a.render(tgt, 0.0);
                if let Some(h) = a.hits.iter().find(|h| h.module == ti && h.id == 10).copied() {
                    a.on_down(h.rect.cx(), h.rect.cy());
                    a.on_up(h.rect.cx(), h.rect.cy());
                }
                let closed = !a.expanded;
                *tools::picker::RESULT.lock().unwrap() = Some(Some([0x12, 0x34, 0x56]));
                a.tick();
                let back = a.expanded && a.tab == ti && a.modules[ti].debug_state().contains("colors");
                out += &format!("{:<4} Pick from screen closes the notch, and the picked color reopens Tools ({})\n", if closed && back { "OK" } else { "FAIL" }, a.modules[ti].debug_state());
                // The screenshot the picker uses matches the real screen.
                match tools::picker::capture() {
                    Some(s) => {
                        let screen = unsafe { GetDC(None) };
                        let mut same = 0;
                        for (fx, fy) in [(0.1f32, 0.1f32), (0.5, 0.5), (0.9, 0.2), (0.3, 0.8), (0.7, 0.7)] {
                            let (x, y) = ((s.w as f32 * fx) as i32, (s.h as f32 * fy) as i32);
                            let p = unsafe { GetPixel(screen, s.x + x, s.y + y) };
                            let real = [(p.0 & 0xff) as u8, ((p.0 >> 8) & 0xff) as u8, ((p.0 >> 16) & 0xff) as u8];
                            if s.at(x, y) == Some(real) {
                                same += 1;
                            }
                        }
                        unsafe { ReleaseDC(None, screen) };
                        out += &format!("{:<4} color picker screenshot {}x{} matches the screen ({same}/5 points)\n", if same >= 3 { "OK" } else { "INFO" }, s.w, s.h);
                        if std::env::var("NOOKLY_TEST_PICKER").is_ok() {
                            *tools::picker::RESULT.lock().unwrap() = None;
                            let up = tools::picker::start(a.hwnd);
                            let clicked = tools::picker::test_click(12, 12);
                            let got = tools::picker::RESULT.lock().unwrap().take();
                            out += &format!("{:<4} color picker window opens and picks where clicked ({:?})\n", if up && clicked && got.is_some() && !tools::picker::active() { "OK" } else { "FAIL" }, got);
                        }
                    }
                    None => out += "INFO color picker: couldn't take a screenshot (no desktop?)\n",
                }
                a.call(ti, |m, c| m.action(1, (0.0, 0.0), c));
                a.expanded = false;
            } else {
                out += "FAIL Tools tab missing\n";
            }
        }
        // Resize animation: soft spring that settles quickly with a small
        // overshoot, and content that fades in after a tab switch.
        {
            let from = (476.0f32, 150.0f32, 26.0f32);
            let to = (from.0 + 280.0, from.1 + 66.0, from.2);
            a.cur = from;
            a.vel = (0.0, 0.0, 0.0);
            let (mut steps, mut over) = (0u32, 0.0f32);
            while steps < 180 {
                let d = a.step_anim(to, 1.0 / 60.0);
                over = over.max(a.cur.0 - to.0);
                steps += 1;
                if d == 0.0 {
                    break;
                }
            }
            let secs = steps as f32 / 60.0;
            // Liquid on purpose: a clear overshoot and wobble, settled
            // within a second.
            let ok = secs <= 0.9 && over > 0.04 * 280.0 && over < 0.16 * 280.0;
            out += &format!("{:<4} resize spring (liquid) settles in {secs:.2}s, overshoot {over:.1}px of 280\n", if ok { "OK" } else { "FAIL" });
            // Same result at a choppy 20 fps.
            a.cur = from;
            a.vel = (0.0, 0.0, 0.0);
            let mut s2 = 0u32;
            while s2 < 60 && a.step_anim(to, 0.05) != 0.0 {
                s2 += 1;
            }
            out += &format!("{:<4} resize spring is frame-rate independent ({:.2}s at 20 fps)\n", if (s2 as f32 * 0.05 - secs).abs() < 0.12 { "OK" } else { "FAIL" }, s2 as f32 * 0.05);
            let now = a.t();
            a.switch_t = now;
            let (a0, a1) = (a.switch_alpha(now), a.switch_alpha(now + 0.35));
            out += &format!("{:<4} tab switch fades content in ({a0:.2} -> {a1:.2})\n", if a0 < 0.05 && a1 > 0.99 { "OK" } else { "FAIL" });
            a.switch_t = -10.0;
            // Every tab opens to the same panel (only Music lyrics may widen).
            let mut sizes = Vec::new();
            for i in 0..a.modules.len() {
                if a.modules[i].has_tab() {
                    a.tab = i;
                    a.expanded = true;
                    let t = a.target();
                    sizes.push((a.modules[i].title(), t.0 as i32, t.1 as i32));
                }
            }
            // Wider, taller or fitted on purpose.
            sizes.retain(|s| !["Widgets", "Weather", "Volume"].contains(&s.0));
            let same = sizes.windows(2).all(|w| (w[0].1, w[0].2) == (w[1].1, w[1].2));
            out += &format!("{:<4} every tab opens to the same panel size {:?}\n", if same { "OK" } else { "FAIL" }, sizes.first().map(|s| (s.1, s.2)));

            // Switching tabs: old content slides out while the new slides in;
            // only the new content is clickable mid-way.
            let tabs: Vec<usize> = (0..a.modules.len()).filter(|&i| a.modules[i].has_tab()).collect();
            let (t0, t1) = (tabs[0], tabs[3.min(tabs.len() - 1)]);
            a.tab = t0;
            a.shown = (t0, false);
            let tgt = a.target();
            a.cur = tgt;
            a.render(tgt, 0.0);
            a.tab = t1;
            a.frame();
            let mid = a.t() - 0.12;
            a.switch_t = mid;
            a.render(tgt, 0.0);
            let old_hits = a.hits.iter().filter(|h| h.module == t0).count();
            let new_hits = a.hits.iter().filter(|h| h.module == t1).count();
            let ok = a.prev_shown.0 == t0 && a.switch_dir > 0.0 && old_hits == 0 && new_hits > 0 && a.gfx.last_err.is_none();
            out += &format!("{:<4} tab switch slides old content out, new in (old hits {old_hits}, new {new_hits})\n", if ok { "OK" } else { "FAIL" });
            // The highlight pill glides to the new tab instead of jumping.
            let slot = a.tab_slot;
            let target_off = tabs.iter().position(|&i| i == t1).unwrap_or(0) as f32 * slot;
            a.pill_off = 0.0;
            a.pill_last = a.t() - 0.016;
            a.render(tgt, 0.0);
            let first = a.pill_off;
            for _ in 0..40 {
                a.pill_last = a.t() - 0.016;
                a.render(tgt, 0.0);
            }
            let pill_ok = first > 0.0 && first < target_off && (a.pill_off - target_off).abs() < 0.5;
            out += &format!("{:<4} highlight pill glides to the new tab ({first:.1} -> {:.1} of {target_off:.0})\n", if pill_ok { "OK" } else { "FAIL" }, a.pill_off);
            a.switch_t = -10.0;
            a.expanded = false;
        }
        // Notifications: text rules, colours, muting, Do Not Disturb, and a
        // real Windows notification travelling into the notch.
        {
            use crate::modules::notifs::{self, Note};
            let ni = find(a, "Notifications").unwrap_or(0);
            let n = Note { app: "Messages".into(), title: "Amaya".into(), body: "See you\n at 7".into(), ..Default::default() };
            let ok = notifs::toast_text(&n, true) == ("Amaya".to_string(), "See you at 7".to_string())
                && notifs::toast_text(&n, false) == ("Messages".to_string(), "New notification".to_string())
                && notifs::toast_text(&Note { app: "Mail".into(), ..Default::default() }, true).0 == "Mail";
            out += &format!("{:<4} notification text (and hidden-text mode shows only the app)\n", if ok { "OK" } else { "FAIL" });
            let red: Vec<u8> = (0..64 * 64).flat_map(|_| [20u8, 20, 200, 255]).collect();
            let grey: Vec<u8> = (0..64 * 64).flat_map(|_| [128u8, 128, 128, 255]).collect();
            let c = notifs::icon_color(&red);
            let ok = c.map_or(false, |c| c.r > 0.8 && c.g < 0.5 && c.b < 0.5) && notifs::icon_color(&grey).map_or(false, |c| c.r == col::TEXT.r);
            out += &format!("{:<4} notification colour comes from the app icon\n", if ok { "OK" } else { "FAIL" });
            a.toast = None;
            a.queue.clear();
            a.expanded = false;
            a.hidden = false;
            out += &format!("INFO Do Not Disturb / focus on this PC right now: {} (tests below force it either way)\n", notifs::quiet());
            notifs::QUIET_OVERRIDE.store(2, std::sync::atomic::Ordering::Relaxed);
            FORCE_VISIBLE.store(true, std::sync::atomic::Ordering::Relaxed);
            let was_quiet = notifs::quiet();
            a.call(ni, |m, c| m.action(notifs::A_DEMO, (0.0, 0.0), c));
            a.frame();
            let shown = a.toast.as_ref().map_or(false, |t| t.0.key == "notif" && t.0.title == "Amaya" && t.0.click == Some(notifs::A_OPEN) && t.0.actions.len() == 2);
            if !was_quiet {
                out += &format!("{:<4} a notification shows in the notch with Open / Mute\n", if shown { "OK" } else { "FAIL" });
                out += &snap(a, "notification toast");
                // Mute: the app's next notification is dropped, and Settings counts it.
                a.toast = None;
                a.call(ni, |m, c| m.action(notifs::A_MUTE, (0.0, 0.0), c));
                a.toast = None;
                a.queue.clear();
                a.call(ni, |m, c| m.action(notifs::A_DEMO, (0.0, 0.0), c));
                a.frame();
                let silent = a.toast.is_none() && a.queue.is_empty();
                let counted = notifs::muted(&a.settings.extra) == vec!["Nookly.Demo".to_string()];
                a.settings.extra.remove(notifs::MUTED_KEY);
                a.settings.save();
                out += &format!("{:<4} muting an app silences it\n", if silent && counted { "OK" } else { "FAIL" });
            } else {
                out += "INFO Do Not Disturb is on: notification display tests skipped\n";
            }
            out += &format!("INFO focus/Do Not Disturb state: {:?} (0 = off)\n", notifs::focus_profile());
            // Read what's in Notification Center now, step by step, on a
            // background (MTA) thread like the watcher, with a time limit, so
            // a hanging step is named instead of freezing the self-test.
            {
                let trace = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
                let tr = trace.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    unsafe {
                        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
                    }
                    let log = |s: String| tr.lock().unwrap().push(s);
                    let t0 = std::time::Instant::now();
                    let ms = |t: std::time::Instant| t.elapsed().as_secs_f64() * 1000.0;
                    let list = windows::UI::Notifications::Management::UserNotificationListener::Current()
                        .and_then(|l| l.GetNotificationsAsync(windows::UI::Notifications::NotificationKinds::Toast))
                        .and_then(|op| op.join());
                    let Ok(list) = list else {
                        log(format!("list failed: {:?}", list.err()));
                        let _ = tx.send(());
                        return;
                    };
                    log(format!("list: {} in {:.0} ms", list.Size().unwrap_or(0), ms(t0)));
                    let mut done: Vec<String> = Vec::new();
                    for n in list {
                        let info = n.AppInfo().ok();
                        let app = info.as_ref().and_then(|i| i.DisplayInfo().ok()).and_then(|d| d.DisplayName().ok()).map(|s| s.to_string()).unwrap_or_default();
                        if done.contains(&app) {
                            continue;
                        }
                        done.push(app.clone());
                        let t = std::time::Instant::now();
                        log(format!("{app}: reading..."));
                        let note = notifs::read_note(&n);
                        log(format!(
                            "{app}: read in {:.0} ms total, icon {}, title {} chars, body {} chars",
                            ms(t),
                            if note.icon.is_some() { "yes" } else { "no" },
                            note.title.chars().count(),
                            note.body.chars().count()
                        ));
                    }
                    let _ = tx.send(());
                });
                let finished = rx.recv_timeout(std::time::Duration::from_secs(15)).is_ok();
                for line in trace.lock().unwrap().iter() {
                    out += &format!("INFO center: {line}\n");
                }
                out += &format!("{:<4} reading Notification Center finishes (15 s limit)\n", if finished { "OK" } else { "FAIL" });
            }
            // Live: a silent test notification (straight to Notification
            // Center, no banner, no sound) must reach the notch.
            a.toast = None;
            a.queue.clear();
            let had = a.settings.notif_on;
            a.settings.notif_on = true;
            a.tick();
            let mut ready = false;
            for _ in 0..30 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                if notifs::ACCESS.load(std::sync::atomic::Ordering::Relaxed) == 1 {
                    ready = true;
                    break;
                }
            }
            // Let the watcher take its first look (it never replays old ones).
            std::thread::sleep(std::time::Duration::from_millis(900));
            let aumid = HSTRING::from(r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe");
            let sent = (|| -> windows::core::Result<std::time::Instant> {
                use windows::Data::Xml::Dom::XmlDocument;
                use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
                let xml = XmlDocument::new()?;
                xml.LoadXml(h!("<toast><visual><binding template='ToastGeneric'><text>Nookly self-test</text><text>This test notification removes itself</text></binding></visual><audio silent='true'/></toast>"))?;
                let toast = ToastNotification::CreateToastNotification(&xml)?;
                toast.SetTag(h!("wl-selftest"))?;
                toast.SetGroup(h!("wl"))?;
                toast.SetSuppressPopup(true)?;
                ToastNotificationManager::CreateToastNotifierWithId(&aumid)?.Show(&toast)?;
                Ok(std::time::Instant::now())
            })();
            let mut got = None;
            if let Ok(t0) = sent {
                while t0.elapsed().as_secs_f64() < 5.0 && got.is_none() {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    a.tick();
                    let hit = |t: &Toast| t.key == "notif" && t.title == "Nookly self-test";
                    if a.toast.as_ref().map_or(false, |t| hit(&t.0)) || a.queue.iter().any(|t| hit(t)) {
                        got = Some(t0.elapsed().as_secs_f64() * 1000.0);
                    }
                }
                let _ = windows::UI::Notifications::ToastNotificationManager::History().and_then(|h| h.RemoveGroupedTagWithId(h!("wl-selftest"), h!("wl"), &aumid));
            }
            let quiet = notifs::quiet();
            match (ready, &sent, got) {
                (true, Ok(_), Some(ms)) => out += &format!("OK   a real Windows notification reached the notch in {ms:.0} ms\n"),
                (_, _, None) if quiet => out += "INFO live notification test skipped: Do Not Disturb is on\n",
                (false, _, _) if std::env::var("CI").is_ok() => out += "INFO notification access: not available on this build server\n",
                (false, _, _) => out += "FAIL notification access: Windows didn't allow the listener\n",
                (_, Err(e), _) => out += &format!("INFO live notification test: couldn't send a test notification ({e})\n"),
                _ => out += "FAIL a real Windows notification never reached the notch (5 s)\n",
            }
            // Three arrive during a full-screen game: nothing shows until the
            // notch is back, then the newest with "+2 more". (Do Not Disturb
            // holds them the same way, so this needs it off.)
            if !quiet {
                a.toast = None;
                a.queue.clear();
                a.hidden = true;
                for _ in 0..3 {
                    a.call(ni, |m, c| m.action(notifs::A_DEMO_INBOX, (0.0, 0.0), c));
                }
                a.tick();
                let held = a.toast.is_none() && a.queue.iter().all(|t| t.key != "notif");
                a.hidden = false;
                a.tick();
                a.frame();
                let sub = a.toast.as_ref().map(|t| t.0.sub.clone()).unwrap_or_default();
                out += &format!("{:<4} notifications during full screen wait, then show as one ({sub})\n", if held && sub.ends_with("(+2 more)") { "OK" } else { "FAIL" });
            }
            // Do Not Disturb holds notifications too (never drops them).
            {
                a.toast = None;
                a.queue.clear();
                notifs::QUIET_OVERRIDE.store(1, std::sync::atomic::Ordering::Relaxed);
                for _ in 0..2 {
                    a.call(ni, |m, c| m.action(notifs::A_DEMO_INBOX, (0.0, 0.0), c));
                }
                a.tick();
                let held = a.toast.is_none() && a.queue.iter().all(|t| t.key != "notif");
                notifs::QUIET_OVERRIDE.store(2, std::sync::atomic::Ordering::Relaxed);
                a.tick();
                a.frame();
                let sub = a.toast.as_ref().map(|t| t.0.sub.clone()).unwrap_or_default();
                out += &format!("{:<4} notifications during Do Not Disturb wait, then show ({sub})\n", if held && sub.ends_with("(+1 more)") { "OK" } else { "FAIL" });
            }
            notifs::QUIET_OVERRIDE.store(0, std::sync::atomic::Ordering::Relaxed);
            FORCE_VISIBLE.store(false, std::sync::atomic::Ordering::Relaxed);
            a.settings.notif_on = had;
            a.settings.save();
            a.tick();
            a.toast = None;
            a.queue.clear();
        }
        // Updates: version maths, release parsing, and a live GitHub check.
        {
            use crate::modules::update::*;
            let ok = is_newer("v0.1.10", "0.1.9") && !is_newer("0.1.4", "0.1.4") && !is_newer("v0.1.3", "0.1.4") && is_newer("1.0.0", "0.9.9");
            out += &format!("{:<4} update version comparison\n", if ok { "OK" } else { "FAIL" });
            let json = br#"{"tag_name":"v9.9.9","html_url":"https://x","draft":false,"prerelease":false,
                "assets":[{"name":"NooklySetup-9.9.9.exe","browser_download_url":"https://github.com/a/b.exe","size":123}]}"#;
            let rel = parse_release(json);
            let ok = rel.as_ref().map_or(false, |r| r.version == "9.9.9" && r.installer.as_ref().map_or(false, |i| i.size == Some(123)));
            out += &format!("{:<4} update release JSON parsing\n", if ok { "OK" } else { "FAIL" });
            let ok = sha256_hex(b"abc") == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
            out += &format!("{:<4} SHA-256 matches the standard test vector\n", if ok { "OK" } else { "FAIL" });
            let html = format!(
                r#"<a href="/{REPO}/releases/download/v9.9.9/Nookly-src.zip">x</a><clipboard-copy value="sha256:{a}">
                <a href="/{REPO}/releases/download/v9.9.9/NooklySetup-9.9.9.exe" rel="nofollow">x</a>
                <clipboard-copy value="sha256:{b}"></clipboard-copy><span>2.3 MB</span>"#,
                a = "1".repeat(64),
                b = "AB".repeat(32)
            );
            let inst = parse_assets_html(&html, "v9.9.9");
            let ok = inst.as_ref().map_or(false, |i| {
                i.url == format!("https://github.com/{REPO}/releases/download/v9.9.9/NooklySetup-9.9.9.exe") && i.sha256.as_deref() == Some(&"ab".repeat(32)[..])
            });
            out += &format!("{:<4} release page parsing finds the installer and its SHA-256\n", if ok { "OK" } else { "FAIL" });
            // Live: both routes must agree on the latest release.
            let api = fetch_api();
            let web = fetch_web();
            let desc = |r: &std::result::Result<Release, String>| match r {
                Ok(r) => format!(
                    "{} (installer {}, sha256 {})",
                    r.version,
                    r.installer.as_ref().map_or("missing".into(), |i| i.url.rsplit('/').next().unwrap_or("").to_string()),
                    r.installer.as_ref().and_then(|i| i.sha256.clone()).map_or("none".into(), |h| h[..12].to_string())
                ),
                Err(e) => format!("failed ({e})"),
            };
            out += &format!("INFO GitHub API: {}\n", desc(&api));
            let web_ok = web.as_ref().map_or(false, |r| r.installer.as_ref().map_or(false, |i| i.sha256.is_some()));
            let agree = match (&api, &web) {
                (Ok(a), Ok(w)) => a.version == w.version,
                _ => true,
            };
            out += &format!("{:<4} release pages (no API limit): {}{}\n", if web_ok && agree { "OK" } else { "FAIL" }, desc(&web), if agree { "" } else { " DISAGREES with the API" });
            if let Ok(r) = fetch_latest() {
                out += &format!("INFO latest release {} -> this build {VERSION} is {}\n", r.version, if is_newer(&r.version, VERSION) { "behind (update offered)" } else { "up to date" });
            }
            if std::env::var("NOOKLY_TEST_DOWNLOAD").is_ok() {
                for (route, r) in [("release pages", web), ("API", api)] {
                    if let Ok(r) = r {
                        match download_installer(&r) {
                            Ok(p) => {
                                let n = std::fs::metadata(&p).map_or(0, |m| m.len());
                                out += &format!("OK   installer download via {route}, {n} bytes, checksum verified\n");
                                let _ = std::fs::remove_file(p);
                            }
                            Err(e) => out += &format!("FAIL installer download via {route}: {e}\n"),
                        }
                    }
                }
                // A tampered file must be refused.
                if let Ok(Some(mut i)) = fetch_web().map(|r| r.installer) {
                    i.sha256 = Some("0".repeat(64));
                    let bad = Release { version: "0.0.0".into(), page: String::new(), installer: Some(i) };
                    let refused = matches!(download_installer(&bad), Err(e) if e.contains("doesn't match"));
                    out += &format!("{:<4} a download that doesn't match its checksum is refused\n", if refused { "OK" } else { "FAIL" });
                }
            }
        }
        for i in 0..a.modules.len() {
            let s = a.modules[i].debug_state();
            if !s.is_empty() {
                out += &format!("INFO {}: {s}\n", a.modules[i].title());
            }
        }
        out
    })
    .unwrap_or_else(|| "app not running".into())
}

// ------------------------------------------------------------- docs renders

/// Write the current frame as `w:u32 h:u32` + premultiplied BGRA rows,
/// cropped to `crop` (inclusive pixel box) or to the drawn bounding box.
fn write_raw(g: &Gfx, path: &std::path::Path, crop: Option<(i32, i32, i32, i32)>) {
    let px = g.pixels();
    let (pw, ph) = (g.pw, g.ph);
    let Some((x0, y0, x1, y1)) = crop.or_else(|| g.bbox()) else { return };
    let (x1, y1) = (x1.min(pw - 1), y1.min(ph - 1));
    let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
    let mut out = Vec::with_capacity(8 + (w * h * 4) as usize);
    out.extend_from_slice(&(w as u32).to_le_bytes());
    out.extend_from_slice(&(h as u32).to_le_bytes());
    for y in y0..=y1 {
        let s = ((y * pw + x0) * 4) as usize;
        out.extend_from_slice(&px[s..s + (w * 4) as usize]);
    }
    let _ = std::fs::write(path, out);
}

fn shot(a: &mut App, dir: &std::path::Path, name: &str) {
    let tgt = a.target();
    a.cur = tgt;
    // A few frames so smoothed things (lyric scroll) settle.
    for _ in 0..60 {
        a.render(tgt, 0.0);
    }
    write_raw(&a.gfx, &dir.join(format!("{name}.raw")), None);
}

/// Animate toward the current target, writing one raw frame per step.
fn animate(a: &mut App, dir: &std::path::Path, n: &mut u32, steps: u32, crop: (i32, i32, i32, i32)) {
    for _ in 0..steps {
        let tgt = a.target();
        let k = 1.0 - (-(1.0f32 / 30.0) * 16.0).exp();
        a.cur = (lerp(a.cur.0, tgt.0, k), lerp(a.cur.1, tgt.1, k), lerp(a.cur.2, tgt.2, k));
        let dist = (a.cur.0 - tgt.0).abs() + (a.cur.1 - tgt.1).abs();
        a.render(tgt, dist);
        write_raw(&a.gfx, &dir.join(format!("anim_{:03}.raw", *n)), Some(crop));
        *n += 1;
    }
}

/// `nookly --render-docs <dir>`: render every view with demo data for the
/// README/user guide. Run with `NOOKLY_DATA_DIR` pointing at seeded demo data.
pub fn render_docs(dir: &std::path::Path) -> String {
    let _ = std::fs::create_dir_all(dir);
    with_app(|a| {
        let mut log = String::new();
        let scale = 2.0;
        a.scale = scale;
        let _ = a.gfx.resize((CANVAS_W * scale) as i32, (CANVAS_H * scale) as i32);
        // NOOKLY_RENDER_STYLE=slim renders every shot in another style (for checking layouts).
        let base = std::env::var("NOOKLY_RENDER_STYLE").ok().and_then(|s| NotchStyle::from_key(&s)).unwrap_or(NotchStyle::Full);
        a.settings.style = base;
        // NOOKLY_RENDER_THEME / NOOKLY_RENDER_SECONDS: renders in a theme, with seconds.
        if let Ok(t) = std::env::var("NOOKLY_RENDER_THEME") {
            a.settings.theme = t;
        }
        a.settings.seconds = std::env::var("NOOKLY_RENDER_SECONDS").is_ok();
        crate::time::SECONDS.store(a.settings.seconds, std::sync::atomic::Ordering::Relaxed);
        a.settings.edge = Edge::Top;
        a.settings.personality = true;
        a.anchor = CANVAS_W / 2.0;
        a.queue.clear();
        a.toast = None;
        a.expanded = false;
        a.mood = Mood::Clock;
        let music = find(a, "Now Playing").unwrap_or(0);
        let pomo = find(a, "Pomodoro").unwrap_or(0);
        let timer = find(a, "Timer").unwrap_or(0);
        let water = find(a, "Water Reminder").unwrap_or(0);
        let todo = find(a, "TodoList").unwrap_or(0);
        let shelf = find(a, "Shelf").unwrap_or(0);
        let act = |a: &mut App, i: usize, id: u32| a.call(i, |m, c| m.action(id, (0.0, 0.0), c));
        let tick = |a: &mut App, i: usize| a.call(i, |m, c| m.tick(c));
        tick(a, music);

        // Collapsed looks.
        shot(a, dir, "collapsed_full_music");
        for (s, n) in [(NotchStyle::Slim, "collapsed_slim_music"), (NotchStyle::Compact, "collapsed_compact_music"), (NotchStyle::Pill, "collapsed_pill_music")] {
            a.settings.style = s;
            shot(a, dir, n);
        }
        act(a, music, 1); // pause
        tick(a, music);
        shot(a, dir, "collapsed_pill_idle");
        a.settings.style = NotchStyle::Compact;
        shot(a, dir, "collapsed_compact_clock");
        a.settings.style = base;
        shot(a, dir, "collapsed_full_clock");
        for (m, n) in [(Mood::Eyes, "mascot_eyes"), (Mood::Doze, "mascot_doze"), (Mood::Wave, "mascot_wave")] {
            a.mood = m;
            a.mood_since = a.t() - 5.0;
            shot(a, dir, n);
        }
        for m in Mascot::ALL {
            a.settings.mascot = m;
            a.mood = Mood::Eyes;
            a.mood_since = a.t() - 5.0;
            shot(a, dir, &format!("mascot_{}", m.key()));
        }
        a.settings.mascot = Mascot::Eyes;
        a.mood = Mood::Clock;
        act(a, music, 1); // play
        tick(a, music);

        // Panels.
        a.expanded = true;
        a.tab = music;
        shot(a, dir, "panel_music");
        act(a, music, crate::modules::music::A_DEMO_LYRICS);
        shot(a, dir, "panel_lyrics");
        act(a, music, 5); // lyrics off
        a.tab = pomo;
        act(a, pomo, crate::modules::pomodoro::A_DEMO);
        shot(a, dir, "panel_pomodoro");
        a.expanded = false;
        shot(a, dir, "collapsed_full_pomodoro");
        act(a, pomo, 2); // reset
        a.expanded = true;
        a.tab = timer;
        act(a, timer, crate::modules::timer::A_DEMO_TIMER);
        shot(a, dir, "panel_timer");
        a.expanded = false;
        shot(a, dir, "collapsed_full_timer");
        a.settings.style = NotchStyle::Pill;
        shot(a, dir, "collapsed_pill_timer");
        a.settings.style = base;
        a.expanded = true;
        act(a, timer, crate::modules::timer::A_DEMO_WATCH);
        shot(a, dir, "panel_stopwatch");
        act(a, timer, 1); // countdown view
        act(a, timer, 4); // reset it
        a.tab = water;
        shot(a, dir, "panel_water");
        act(a, water, 2);
        shot(a, dir, "panel_water_settings");
        act(a, water, 3);
        a.tab = todo;
        shot(a, dir, "panel_todo");
        act(a, todo, 1); // add task
        a.call(todo, |m, c| {
            for ch in "Call the dentist".chars() {
                m.chr(ch, c);
            }
            m.action(11, (0.0, 0.0), c); // +30m
        });
        shot(a, dir, "panel_todo_editor");
        act(a, todo, 3); // cancel
        a.keyboard = None;
        a.tab = shelf;
        shot(a, dir, "panel_shelf");
        a.drag_files = true;
        for m in a.modules.iter_mut() {
            m.drag_hover(true);
        }
        shot(a, dir, "panel_shelf_drop");
        a.drag_files = false;
        for m in a.modules.iter_mut() {
            m.drag_hover(false);
        }
        if let Some(clip) = find(a, "Clipboard") {
            a.call(clip, |m, c| m.action(crate::modules::clipboard::A_DEMO_DOCS, (0.0, 0.0), c));
            a.tab = clip;
            shot(a, dir, "panel_clipboard");
        }
        if let Some(p) = find(a, "Prayer Times") {
            for k in [0u32, 2, 3, 4, 5] {
                a.call(p, |m, c| m.action(90 + k, (0.0, 0.0), c));
            }
            a.tab = p;
            shot(a, dir, "panel_prayer");
            a.call(p, |m, c| m.action(crate::modules::prayer::A_TEST_RELOAD, (0.0, 0.0), c));
        }
        if let Some(t) = find(a, "Tools") {
            a.call(t, |m, c| m.action(crate::modules::tools::A_DEMO, (0.0, 0.0), c));
            a.tab = t;
            for (k, name) in [(1u32, "panel_tools_color"), (2, "panel_tools_calc"), (3, "panel_tools_qr"), (4, "panel_tools_convert")] {
                a.call(t, |m, c| m.action(k, (0.0, 0.0), c));
                a.release_keyboard();
                shot(a, dir, name);
            }
            a.call(t, |m, c| m.action(77, (0.0, 0.0), c));
            shot(a, dir, "panel_tools_zones");
            a.call(t, |m, c| m.action(73, (0.0, 0.0), c));
            a.call(t, |m, c| m.action(1, (0.0, 0.0), c));
        }
        if let Some(g) = find(a, "AI Agents") {
            a.call(g, |m, c| m.action(crate::modules::agents::A_DEMO, (0.0, 0.0), c));
            a.tab = g;
            shot(a, dir, "panel_agents");
        }
        if let Some(m) = find(a, "System Monitor") {
            a.call(m, |x, c| x.action(crate::modules::monitor::A_DEMO, (0.0, 0.0), c));
            a.tab = m;
            shot(a, dir, "panel_monitor");
        }
        if let Some(c) = find(a, "Calendar") {
            a.tab = c;
            shot(a, dir, "panel_calendar");
        }
        if let Some(w) = find(a, "Widgets") {
            a.call(w, |x, c| x.action(crate::modules::widgets::A_DEMO, (0.0, 0.0), c));
            let layout = a.settings.layout.clone();
            a.settings.layout = crate::store::NotchLayout::custom_rows(vec![NotchRow::wide("music"), NotchRow::pair("agents", "timer"), NotchRow::pair("bluetooth", "battery"), NotchRow::pair("cpu", "")], true);
            a.tab = w;
            a.call(w, |x, c| x.action(1, (0.0, 0.0), c));
            shot(a, dir, "panel_widgets");
            a.call(w, |x, c| x.action(2, (0.0, 0.0), c));
            shot(a, dir, "panel_widgets_order");
            a.call(w, |x, c| x.action(3, (0.0, 0.0), c));
            a.settings.set_alert_mode("volume", AlertMode::Small);
            a.settings.set_alert_mode("caps", AlertMode::Off);
            shot(a, dir, "panel_widgets_alerts");
            a.call(w, |x, c| x.action(5, (0.0, 0.0), c));
            shot(a, dir, "panel_widgets_place");
            a.call(w, |x, c| x.action(1, (0.0, 0.0), c));
            a.settings.set_alert_mode("volume", AlertMode::Popup);
            a.settings.set_alert_mode("caps", AlertMode::Popup);
            // A side mascot and a centred one with two widgets, and a small
            // alert beside a widget.
            a.expanded = false;
            a.settings.layout = crate::store::NotchLayout::custom_rows(vec![NotchRow::pair("date", "bluetooth")], false);
            let m = (a.mood, a.settings.mascot_pos, a.settings.personality);
            a.settings.personality = true;
            a.mood = Mood::Eyes;
            for (pos, n) in [(0u8, "widgets_mascot_left"), (1, "widgets_mascot_center")] {
                a.settings.mascot_pos = pos;
                shot(a, dir, n);
            }
            (a.mood, a.settings.mascot_pos, a.settings.personality) = m;
            a.push_toast(Toast::new("volume", glyph::VOLUME, col::TEXT, "Volume", "").right("60%").progress(0.6).secs(3.0));
            a.small_alert = a.small_alert.take().or_else(|| Some((alert_widget(&Toast::new("volume", glyph::VOLUME, col::TEXT, "Volume", "").right("60%").progress(0.6)), f64::MAX)));
            if let Some(s) = &mut a.small_alert {
                s.1 = f64::MAX;
            }
            a.queue.clear();
            shot(a, dir, "widgets_small_alert");
            a.small_alert = None;
            a.expanded = true;
            a.settings.layout = layout;
        }
        // Every live activity on the closed notch, one at a time (the other
        // panels hidden), then music with a mascot beside and between.
        {
            let saved = (a.settings.hidden_panels.clone(), a.mood, a.settings.mascot_pos);
            a.expanded = false;
            let tabs: Vec<usize> = (0..a.modules.len()).filter(|&j| a.modules[j].has_tab()).collect();
            for &i in &tabs {
                a.settings.hidden_panels = tabs.iter().filter(|&&j| j != i).map(|&j| a.modules[j].title().to_string()).collect();
                if a.live_module() == Some(i) {
                    shot(a, dir, &format!("live_{}", a.modules[i].title().replace(' ', "_").to_lowercase()));
                }
            }
            a.settings.hidden_panels = tabs.iter().filter(|&&j| j != music).map(|&j| a.modules[j].title().to_string()).collect();
            a.mood = Mood::Eyes;
            for (pos, n) in [(0u8, "live_music_mascot_left"), (1, "live_music_mascot_center")] {
                a.settings.mascot_pos = pos;
                shot(a, dir, n);
            }
            // Widgets on the resting notch, and music beside an agent.
            a.settings.hidden_panels.clear();
            a.mood = Mood::Clock;
            let layout = a.settings.layout.clone();
            if let Some(w) = find(a, "Widgets") {
                a.call(w, |x, c| x.action(crate::modules::widgets::A_DEMO, (0.0, 0.0), c));
            }
            let only = |a: &mut App, keep: &[&str]| {
                let tabs: Vec<usize> = (0..a.modules.len()).filter(|&j| a.modules[j].has_tab()).collect();
                a.settings.hidden_panels = tabs.iter().filter(|&&j| !keep.contains(&a.modules[j].title())).map(|&j| a.modules[j].title().to_string()).collect();
            };
            only(a, &["Widgets", "System Monitor"]);
            for (rows, n) in [
                (vec![NotchRow::pair("bluetooth", "cpu")], "widgets_pair"),
                (vec![NotchRow::wide("bluetooth")], "widgets_wide"),
                (vec![NotchRow::pair("cpu", "net")], "widgets_monitor"),
                (vec![NotchRow::pair("weather", "wifi")], "widgets_weather_wifi"),
                (vec![NotchRow::wide("weather")], "widgets_weather_wide"),
                (vec![NotchRow::pair("volume", "mic")], "widgets_sound"),
            ] {
                a.settings.layout = crate::store::NotchLayout::custom_rows(rows, false);
                shot(a, dir, n);
            }
            only(a, &["Now Playing", "AI Agents"]);
            a.settings.layout = crate::store::NotchLayout::custom_rows(vec![NotchRow::pair("music", "agents")], false);
            shot(a, dir, "live_music_and_agents");
            a.settings.layout = layout;
            (a.settings.hidden_panels, a.mood, a.settings.mascot_pos) = saved;
            a.expanded = true;
        }
        // Liquid Glass over a colourful and a light background.
        if let Some(m) = find(a, "Now Playing") {
            let (theme, tab) = (a.settings.theme.clone(), a.tab);
            a.settings.theme = "glass".into();
            a.tab = m;
            a.expanded = true;
            // Over a detailed scene (NOOKLY_GLASS_SCENE: a photo and an app
            // window, from make_images.py) the whole canvas is written, so
            // the picture can be laid on that same scene, lined up.
            let scene = std::env::var("NOOKLY_GLASS_SCENE").ok().filter(|p| std::path::Path::new(p).exists());
            let kinds: Vec<(String, &str)> = scene.into_iter().map(|p| (p, "glass_scene")).chain([("color".to_string(), "glass_color"), ("light".to_string(), "glass_light")]).collect();
            for (kind, name) in kinds {
                // Safe: the self-test and doc renders run on one thread.
                unsafe { std::env::set_var("NOOKLY_RENDER_BACKDROP", &kind) };
                a.backdrop = None;
                a.glass_raw = None;
                let tgt = a.target();
                a.cur = tgt;
                for _ in 0..30 {
                    a.render(tgt, 0.0);
                }
                let full = kind.ends_with(".raw").then(|| (0, 0, a.gfx.pw - 1, a.gfx.ph - 1));
                write_raw(&a.gfx, &dir.join(format!("{name}.raw")), full);
            }
            // Safe: the self-test and doc renders run on one thread.
            unsafe { std::env::remove_var("NOOKLY_RENDER_BACKDROP") };
            (a.settings.theme, a.tab) = (theme, tab);
            a.backdrop = None;
            a.glass_raw = None;
        }
        // The Settings window, at the same 2x scale.
        for (name, tab, sec) in [("settings_general", 0u8, 0u8), ("settings_privacy", 0, SEC_PRIVACY), ("settings_look", 0, SEC_THEME), ("settings_features", 0, SEC_FEATURES), ("settings_shortcuts", 0, SEC_SHORTCUTS), ("settings_widgets", 0, SEC_WIDGETS), ("settings_panels", 0, SEC_PANELS), ("settings_location", 0, SEC_LOCATION), ("settings_taskbar", 1, 0)] {
            a.sw_view(tab, sec);
            if let Some(sw) = a.swin.as_mut() {
                sw.set_scale(scale);
            }
            a.sw_render();
            if let Some(sw) = &a.swin {
                write_raw(&sw.gfx, &dir.join(format!("{name}.raw")), None);
            }
        }
        a.close_settings();
        // Nookly's own installer, uninstaller, About and tray menu (with a
        // neutral install folder, not this PC's).
        let theme = crate::gfx::THEMES[0];
        let folder = std::path::Path::new(r"C:\Users\you\AppData\Local\Programs\Nookly");
        let dialogs = [
            ("dlg_setup", Some(crate::setup::install_spec(folder, false, true, false, true)), None),
            ("dlg_uninstall", Some(crate::uninstall::ask_spec(false)), None),
            ("dlg_about", Some(crate::tray::about_spec()), None),
            ("dlg_menu", None, Some(0)),
        ];
        for (name, spec, hover) in dialogs {
            let items = if spec.is_none() { crate::tray::menu_items() } else { Vec::new() };
            if let Some(g) = crate::dialog::snapshot(spec, items, theme, scale, hover) {
                write_raw(&g, &dir.join(format!("{name}.raw")), None);
            }
        }
        // Opening Settings closed the notch; the panel shots below need it open.
        a.expanded = true;
        // The new panels.
        {
            let hidden = std::mem::take(&mut a.settings.hidden_panels);
            let tab = a.tab;
            for (title, name) in [("Notes", "panel_notes"), ("Countdowns", "panel_countdowns"), ("Launcher", "panel_launcher"), ("Toggles", "panel_toggles"), ("Health", "panel_health"), ("Mirror", "panel_mirror"), ("Commands", "panel_commands"), ("Weather", "panel_weather"), ("Volume", "panel_volume")] {
                if let Some(i) = a.modules.iter().position(|m| m.title() == title) {
                    a.tab = i;
                    shot(a, dir, name);
                }
            }
            // Everything on: the panels that don't fit, under the ⋯.
            a.tab = a.modules.iter().position(|m| m.title() == "Now Playing").unwrap_or(0);
            a.more_open = true;
            shot(a, dir, "panel_more_all");
            a.more_open = false;
            // The song's colour glowing around the closed notch.
            a.settings.extra.insert("album_glow".into(), "1".into());
            a.expanded = false;
            a.tick();
            *crate::modules::music::ART_COLOR.lock().unwrap() = Some(hex(0xFF7A3D));
            shot(a, dir, "notch_glow");
            a.settings.extra.remove("album_glow");
            a.expanded = true;
            (a.settings.hidden_panels, a.tab) = (hidden, tab);
        }
        a.open_settings();
        a.start_onboarding();
        for (k, name) in ["tour_hello", "tour_style", "tour_colour", "tour_presets", "tour_tips"].into_iter().enumerate() {
            a.onboarding = Some(k as u8);
            shot(a, dir, name);
        }
        a.onboarding = None;
        a.expanded = true;
        // Every panel on: the ones that don't fit go under ⋯.
        {
            let hidden = std::mem::take(&mut a.settings.hidden_panels);
            let tab = a.tab;
            a.tab = a.modules.iter().position(|m| m.title() == "Calendar").unwrap_or(0);
            shot(a, dir, "tabs_overflow");
            a.more_open = true;
            shot(a, dir, "panel_more");
            a.more_open = false;
            (a.settings.hidden_panels, a.tab) = (hidden, tab);
        }

        // Toasts.
        a.expanded = false;
        let toasts = [
            ("toast_timer", Toast::new("timer", "\u{E121}", col::PINK, "Time's up!", "10 minute timer finished").action("Restart", 0).action("Dismiss", 0)),
            ("toast_water", Toast::new("water", glyph::DROP, col::CYAN, "Time to Drink", "Stay hydrated! 750 ml to go").action("Drank", 0).action("Snooze", 0)),
            ("toast_todo", Toast::new("todo", glyph::ALARM, col::YELLOW, "Send the weekly update", "Task due now").action("Done", 0).action("Snooze 10m", 0)),
            ("toast_pomodoro", Toast::new("pomodoro", glyph::TIMER, col::GREEN, "Break Time!", "5 minute short break").action("Start", 0).action("Later", 0)),
            ("toast_charging", Toast::new("power", glyph::CHARGING, col::GREEN, "Charging", "Charger connected").right("70%").progress(0.7)),
            ("toast_volume", Toast::new("volume", glyph::VOLUME, col::TEXT, "Volume", "Output").right("45%").progress(0.45)),
            ("toast_caps", Toast::new("caps", glyph::CAPS, col::ACCENT, "Caps Lock", "On")),
            ("toast_bt", Toast::new("bt", glyph::HEADPHONE, col::BLUE, "Earbuds", "Connected")),
            ("toast_lang", Toast::new("lang", glyph::KEYBOARD, col::BLUE, "Bangla (Bangladesh)", "Keyboard language").right("BN")),
            ("toast_copy", crate::modules::clipboard::copy_toast(&crate::modules::clipboard::Content::Text("Meeting moved to 4:30 pm, same room.\nBring the printed slides.".into()))),
            ("toast_agent", Toast::new("agent", crate::modules::agents::ICON, col::ACCENT, "Hermes needs you", "desktop \u{00B7} Wants your OK to run a command")),
            ("toast_agent_done", Toast::new("agent", crate::modules::agents::ICON, col::GREEN, "Claude Code finished", "Nookly \u{00B7} took 4m 12s")),
            (
                "toast_notification",
                Toast::new("notif", glyph::BELL, col::GREEN, "Amaya", "Lunch at 1? I booked the table by the window")
                    .image(std::sync::Arc::new(crate::modules::notifs::demo_icon()))
                    .click(1)
                    .action("Open", 0)
                    .action("Mute app", 0),
            ),
            ("toast_mic", Toast::new("privacy", glyph::MIC, crate::modules::privacy::MIC, "Microphone in use", "Recorder")),
        ];
        for (name, t) in toasts {
            let end = a.t() + 100.0;
            a.toast = Some((t, end));
            shot(a, dir, name);
        }
        a.toast = None;

        // Other edges (music live).
        for (e, s, n) in [
            (Edge::Left, NotchStyle::Full, "edge_left_full"),
            (Edge::Right, NotchStyle::Compact, "edge_right_compact"),
            (Edge::Bottom, NotchStyle::Pill, "edge_bottom_pill"),
        ] {
            a.settings.edge = e;
            a.settings.style = s;
            a.anchor = if e.vertical() { CANVAS_H / 2.0 } else { CANVAS_W / 2.0 };
            shot(a, dir, n);
        }
        a.settings.edge = Edge::Left;
        a.settings.style = base;
        a.anchor = CANVAS_H / 2.0;
        a.expanded = true;
        a.tab = music;
        shot(a, dir, "edge_left_panel");
        // Taskbar mode: the panel opens up from the bottom-left corner.
        a.settings.edge = Edge::Bottom;
        a.anchor = 0.0;
        shot(a, dir, "edge_bottom_panel");
        a.expanded = false;
        a.settings.edge = Edge::Top;
        a.anchor = CANVAS_W / 2.0;
        // The taskbar mascot on its own (no notch around it).
        for (k, m) in [("cat", Mascot::Cat), ("eyes", Mascot::Eyes)] {
            a.gfx.begin(a.scale);
            {
                let mut hits = Vec::new();
                let mut ui = Ui { g: &mut a.gfx, hits: &mut hits, module: usize::MAX - 9, mouse: (-1e4, -1e4), t: 1.0 };
                mascot::draw(&mut ui, Rect::new(20.0, 20.0, 58.0, 42.0), m, Mood::Eyes, 1.0, (0.6, 0.2));
            }
            a.gfx.end_and_present(a.hwnd, a.origin.0, a.origin.1);
            write_raw(&a.gfx, &dir.join(format!("taskbar_mascot_{k}.raw")), None);
        }

        // Hover-to-open animation at 1x for the README GIF.
        a.scale = 1.0;
        let _ = a.gfx.resize(CANVAS_W as i32, CANVAS_H as i32);
        let crop = (0, 0, CANVAS_W as i32 - 1, 259);
        let mut n = 0u32;
        a.tab = music;
        a.expanded = false;
        a.cur = a.target();
        animate(a, dir, &mut n, 1, crop); // hold: collapsed
        a.expanded = true;
        animate(a, dir, &mut n, 18, crop); // open
        animate(a, dir, &mut n, 1, crop); // hold: player
        act(a, music, crate::modules::music::A_DEMO_LYRICS);
        animate(a, dir, &mut n, 18, crop); // split into lyrics
        animate(a, dir, &mut n, 1, crop); // hold: lyrics
        act(a, music, 5);
        a.tab = timer;
        act(a, timer, crate::modules::timer::A_DEMO_TIMER);
        animate(a, dir, &mut n, 18, crop); // switch to timer
        animate(a, dir, &mut n, 1, crop); // hold: timer
        a.expanded = false;
        animate(a, dir, &mut n, 18, crop); // close (timer live)
        animate(a, dir, &mut n, 1, crop); // hold
        log += &format!("rendered to {} ({n} animation frames)\n", dir.display());
        log
    })
    .unwrap_or_else(|| "app not running".into())
}

/// `nookly --render-demo <dir>`: the scripted demo for the README video.
/// Drives the real UI (hover, clicks, tabs, toasts, settings) on a virtual
/// 30 fps clock with a scripted cursor, and writes every frame as a raw
/// 2x canvas plus `demo.log` (cursor position, clicks and scene markers)
/// for `docs/make_demo.py` to composite.
pub fn render_demo(dir: &std::path::Path) -> String {
    let _ = std::fs::create_dir_all(dir);
    with_app(|a| {
        struct D<'a> {
            a: &'a mut App,
            dir: std::path::PathBuf,
            n: u32,
            t: f64,
            cur: (f32, f32),
            inside: bool,
            click: u8,
            log: String,
        }
        impl D<'_> {
            fn step(&mut self) {
                self.t += 1.0 / 30.0;
                self.a.virt = Some(self.t);
                if self.n % 30 == 0 {
                    self.a.tick();
                }
                let (x, y) = self.cur;
                // Layered windows only get the mouse over drawn pixels.
                let over = self.a.hit_at(x, y).is_some();
                if over {
                    self.a.on_mouse_move(x, y);
                } else if self.inside {
                    self.a.on_mouse_leave();
                }
                self.inside = over;
                self.a.frame();
                write_raw(&self.a.gfx, &self.dir.join(format!("f{:04}.raw", self.n)), Some((0, 0, self.a.gfx.pw - 1, self.a.gfx.ph - 1)));
                self.log += &format!("f {} {:.1} {:.1} {}\n", self.n, x * self.a.scale, y * self.a.scale, self.click);
                self.click = 0;
                self.n += 1;
            }
            fn wait(&mut self, secs: f64) {
                for _ in 0..(secs * 30.0).round() as u32 {
                    self.step();
                }
            }
            fn scene(&mut self, name: &str) {
                self.log += &format!("scene {} {name}\n", self.n);
            }
            /// Caption subtitle change (shown by make_demo.py).
            fn sub(&mut self, text: &str) {
                self.log += &format!("sub {} {text}\n", self.n);
            }
            /// Move to a hit and show a click there without sending it.
            fn point(&mut self, module: usize, id: u32, secs: f64) -> bool {
                let Some((x, y)) = self.hit(module, id) else { return false };
                self.move_to(x, y, secs);
                self.wait(0.12);
                self.click = 1;
                self.step();
                true
            }
            /// Glide the cursor to (x, y) with an ease-in-out curve.
            fn move_to(&mut self, x: f32, y: f32, secs: f64) {
                let (x0, y0) = self.cur;
                let k = (secs * 30.0).round().max(1.0) as u32;
                for i in 1..=k {
                    let p = i as f32 / k as f32;
                    let e = if p < 0.5 { 4.0 * p * p * p } else { 1.0 - (-2.0 * p + 2.0).powi(3) / 2.0 };
                    self.cur = (x0 + (x - x0) * e, y0 + (y - y0) * e);
                    self.step();
                }
            }
            /// Centre of a hit currently on screen.
            fn hit(&self, module: usize, id: u32) -> Option<(f32, f32)> {
                self.a.hits.iter().rev().find(|h| h.module == module && h.id == id).map(|h| (h.rect.cx(), h.rect.cy()))
            }
            fn click_at(&mut self, module: usize, id: u32, secs: f64) {
                let Some((x, y)) = self.hit(module, id) else {
                    self.log += &format!("missing hit {module} {id}\n");
                    return;
                };
                self.move_to(x, y, secs);
                self.wait(0.12);
                self.a.on_down(x, y);
                self.click = 1;
                self.step();
                self.step();
                self.a.on_up(x, y);
                self.step();
            }
        }

        let scale = 2.0;
        a.scale = scale;
        let _ = a.gfx.resize((CANVAS_W * scale) as i32, (CANVAS_H * scale) as i32);
        a.settings.style = NotchStyle::Full;
        a.settings.edge = Edge::Top;
        a.settings.personality = true;
        a.settings.mascot = Mascot::Eyes;
        a.anchor = CANVAS_W / 2.0;
        a.queue.clear();
        a.toast = None;
        a.expanded = false;
        a.onboarding = None;
        a.mood = Mood::Clock;
        sound::mute(true);
        let music = find(a, "Now Playing").unwrap_or(0);
        let pomo = find(a, "Pomodoro").unwrap_or(0);
        let timer = find(a, "Timer").unwrap_or(0);
        let water = find(a, "Water Reminder").unwrap_or(0);
        let todo = find(a, "TodoList").unwrap_or(0);
        let shelf = find(a, "Shelf").unwrap_or(0);
        let clip = find(a, "Clipboard").unwrap_or(0);
        let notes = find(a, "Notifications").unwrap_or(0);
        let privacy = find(a, "Privacy").unwrap_or(usize::MAX);
        let prayer = find(a, "Prayer Times").unwrap_or(usize::MAX);
        let tools = find(a, "Tools").unwrap_or(usize::MAX);
        let agents = find(a, "AI Agents").unwrap_or(usize::MAX);
        let monitor = find(a, "System Monitor").unwrap_or(usize::MAX);
        a.call(clip, |m, c| m.action(crate::modules::clipboard::A_DEMO_DOCS, (0.0, 0.0), c));
        if tools != usize::MAX {
            a.call(tools, |m, c| m.action(crate::modules::tools::A_DEMO, (0.0, 0.0), c));
        }
        if agents != usize::MAX {
            a.call(agents, |m, c| m.action(crate::modules::agents::A_DEMO, (0.0, 0.0), c));
        }
        if monitor != usize::MAX {
            a.call(monitor, |m, c| m.action(crate::modules::monitor::A_DEMO, (0.0, 0.0), c));
        }
        a.tab = music;
        a.tick();
        a.cur = a.target();
        let mut d = D { a, dir: dir.to_path_buf(), n: 0, t: 1.0, cur: (CANVAS_W * 0.82, CANVAS_H + 40.0), inside: false, click: 0, log: String::new() };

        d.scene("intro");
        d.sub("A Dynamic Island for Windows. Free and open source.");
        d.wait(2.6);

        // The welcome tour's last two steps: the basics, and Settings at a glance.
        d.scene("tour");
        d.sub("A short welcome tour shows the basics");
        d.a.onboarding = Some(2);
        d.wait(2.4);
        d.sub("the panels worth knowing");
        d.a.onboarding = Some(3);
        d.wait(2.4);
        d.sub("and what you can change in Settings");
        d.a.onboarding = Some(4);
        d.wait(2.8);
        d.a.onboarding = None;
        d.wait(0.5);

        d.scene("styles");
        for (s, text) in [
            (NotchStyle::Compact, "Compact: a smaller notch"),
            (NotchStyle::Pill, "Pill: just a thin bar"),
            (NotchStyle::Hidden, "Hidden: invisible until you hover"),
            (NotchStyle::Full, "Full: clock, music and a mascot"),
        ] {
            d.wait(0.25);
            d.a.settings.style = s;
            d.sub(text);
            d.wait(1.05);
        }

        d.scene("open");
        d.sub("Music controls, with synced lyrics");
        let notch = d.a.edge_rect(CANVAS_W / 2.0, 20.0);
        d.move_to(notch.cx() + 30.0, notch.y + 14.0, 0.9);
        d.wait(1.0);
        if d.point(music, 5, 0.6) {
            d.a.call(music, |m, c| m.action(crate::modules::music::A_DEMO_LYRICS, (0.0, 0.0), c));
        }
        d.wait(1.8);
        d.click_at(music, 5, 0.4);
        d.wait(0.3);

        d.scene("tabs");
        for (i, text) in [
            (timer, "Timer and stopwatch"),
            (pomo, "Pomodoro focus sessions"),
            (todo, "To-do list with reminders"),
            (water, "Water reminder"),
            (shelf, "A shelf for files you're working on"),
            (clip, "Clipboard history: click to paste"),
            (prayer, "Prayer times, worked out on your PC"),
            (tools, "Tools: a color picker for anything on screen"),
            (agents, "AI agents: is Hermes or Claude Code working, or waiting for you?"),
            (monitor, "System monitor: CPU, memory and network"),
        ] {
            if i == usize::MAX {
                continue;
            }
            if i == prayer {
                // Reminders on for the five prayers (bells lit).
                for k in [0u32, 2, 3, 4, 5] {
                    d.a.call(prayer, |m, c| m.action(90 + k, (0.0, 0.0), c));
                }
            }
            // Start the timers only now, so music is the closed notch's
            // live activity everywhere else in the video.
            if i == timer {
                d.a.call(timer, |m, c| m.action(crate::modules::timer::A_DEMO_TIMER, (0.0, 0.0), c));
            } else if i == pomo {
                d.a.call(pomo, |m, c| m.action(crate::modules::pomodoro::A_DEMO, (0.0, 0.0), c));
            }
            d.click_at(APP_HIT, i as u32, 0.35);
            d.sub(text);
            d.wait(0.95);
            if i == shelf {
                d.point(shelf, crate::modules::shelf::A_SNIP, 0.5);
                d.sub("Screenshots go straight onto the Shelf");
                d.wait(1.2);
            }
            if i == clip {
                d.click_at(clip, crate::modules::clipboard::A_SEARCH, 0.4);
                d.sub("Search it, or save snippets you paste often");
                for ch in "meet".chars() {
                    d.wait(0.12);
                    d.a.on_char(ch);
                }
                d.wait(1.1);
                d.a.on_key(0x1B);
                d.a.on_key(0x1B);
                d.a.release_keyboard();
            }
            if i == tools {
                for (k, text) in [(2u32, "a calculator that answers as you type"), (3, "QR codes of whatever you copy"), (4, "and a converter, with katha and bigha")] {
                    d.click_at(tools, k, 0.35);
                    d.a.release_keyboard();
                    d.sub(text);
                    d.wait(1.0);
                }
                d.click_at(tools, 77, 0.35);
                d.sub("and world time zones");
                d.wait(1.1);
                d.a.call(tools, |m, c| m.action(73, (0.0, 0.0), c));
                d.click_at(tools, 1, 0.3);
            }
        }
        // The mouse wheel walks back through the panels.
        d.sub("Or roll the mouse wheel to switch panels");
        // Over the header (not a list, which would scroll instead).
        let content = (d.a.tab_bar.r() - 70.0, d.a.tab_bar.cy());
        d.move_to(content.0, content.1, 0.4);
        for _ in 0..3 {
            d.wait(0.35);
            d.log += &format!("wheel {}\n", d.n);
            d.a.on_wheel(1.0);
            d.wait(0.35);
        }
        d.a.call(timer, |m, c| m.action(4, (0.0, 0.0), c)); // reset
        d.a.call(pomo, |m, c| m.action(2, (0.0, 0.0), c)); // reset

        d.scene("alerts");
        d.move_to(CANVAS_W * 0.85, CANVAS_H + 60.0, 0.7);
        d.wait(0.4);
        d.sub("Your Windows notifications, in the island");
        d.a.call(notes, |m, c| m.action(crate::modules::notifs::A_DEMO, (0.0, 0.0), c));
        d.wait(2.6);
        d.a.toast = None;
        if agents != usize::MAX {
            d.sub("Hermes needs your OK? The island tells you");
            d.a.call(agents, |m, c| m.action(crate::modules::agents::A_DEMO_ASK, (0.0, 0.0), c));
            d.wait(2.4);
            d.a.toast = None;
            d.a.queue.clear();
            d.sub("and when it's done");
            d.a.call(agents, |m, c| m.action(crate::modules::agents::A_DEMO_DONE, (0.0, 0.0), c));
            d.wait(2.2);
            d.a.toast = None;
            d.a.queue.clear();
        }
        if prayer != usize::MAX {
            d.sub("A reminder at each prayer time");
            d.a.call(prayer, |m, c| m.action(crate::modules::prayer::A_DEMO, (0.0, 0.0), c));
            d.wait(2.4);
            d.a.toast = None;
        }
        if let Some(ci) = find(&d.a, "Clipboard") {
            d.sub("Copy something: a small preview shows what you copied (optional)");
            let was = d.a.settings.copy_toast;
            d.a.settings.copy_toast = true;
            d.a.call(ci, |m, c| m.action(crate::modules::clipboard::A_TEST_COPY_TEXT, (0.0, 0.0), c));
            d.a.call(ci, |m, c| m.tick(c));
            d.wait(2.2);
            d.a.toast = None;
            d.a.queue.clear();
            d.a.settings.copy_toast = was;
        }
        d.sub("Over a maximized window it slims down, so your tabs stay clickable");
        MAXIMIZED_OVERRIDE.store(1, std::sync::atomic::Ordering::Relaxed);
        d.a.update_make_room();
        d.wait(2.0);
        MAXIMIZED_OVERRIDE.store(2, std::sync::atomic::Ordering::Relaxed);
        d.a.update_make_room();
        d.wait(0.5);
        d.sub("Charging, battery, volume, Wi-Fi and Bluetooth");
        d.a.push_toast(Toast::new("power", glyph::CHARGING, col::GREEN, "Charging", "Charger connected").right("70%").progress(0.7).secs(1.8));
        d.wait(2.2);
        d.sub("A dot while an app uses your mic or camera");
        if privacy != usize::MAX {
            d.a.call(privacy, |m, c| m.action(crate::modules::privacy::A_DEMO, (0.0, 0.0), c));
            d.a.tick();
        }
        d.wait(2.4);

        d.wait(0.3);
        if privacy != usize::MAX {
            d.a.call(privacy, |m, c| m.action(crate::modules::privacy::A_DEMO, (0.0, 0.0), c));
        }

        d.scene("outro");
        d.sub("github.com/banulalakwindu/Nookly");
        d.move_to(CANVAS_W * 0.9, CANVAS_H + 60.0, 0.8);
        d.wait(3.2);
        d.log += &format!("end {}\n", d.n);
        let _ = std::fs::write(dir.join("demo.log"), &d.log);
        format!("{} frames", d.n)
    })
    .unwrap_or_else(|| "app not running".into())
}

/// The idle companion: two eyes that follow the cursor and blink, close when
/// you're away, and smile when you come back.
fn draw_mascot(ui: &mut Ui, area: Rect, kind: Mascot, mood: Mood, t: f64, origin: (i32, i32), scale: f32) {
    // Look toward the cursor (straight ahead in docs renders).
    let mut look = (0.0f32, 0.0f32);
    if !crate::time::demo() {
        unsafe {
            let mut p = POINT::default();
            if GetCursorPos(&mut p).is_ok() {
                let mx = (p.x - origin.0) as f32 / scale;
                let my = (p.y - origin.1) as f32 / scale;
                look = gaze(area.cx(), area.cy(), (mx, my), 300.0);
            }
        }
    }
    let mood = if mood == Mood::Clock { Mood::Eyes } else { mood };
    mascot::draw(ui, area, kind, mood, t, look);
}

/// Tooltips for welcome-tour hits (by id).
fn onboard_tip(id: u32) -> &'static str {
    match id {
        O_NEXT => "Next step",
        O_BACK => "Previous step",
        O_SKIP => "Skip the tour (Welcome tour in the tray menu brings it back)",
        O_SETTINGS => "Finish and open Settings",
        i if (O_STYLE..O_STYLE + 5).contains(&i) => "Use this look for the closed notch",
        i if (O_EDGE..O_EDGE + 4).contains(&i) => "Put the notch on this screen edge",
        i if (O_PRESET..O_PRESET + 4).contains(&i) => "Switch on these panels",
        i if (O_THEME..O_THEME + 6).contains(&i) => "Use this colour theme",
        _ => "",
    }
}

/// Short names for Settings > Panels and the panel header.
/// What the closed notch shows: one item across both sides, or one item
/// on each side (either may be empty).
#[derive(Clone, Debug)]
enum Shown {
    Wide(ShownItem),
    Pair(Option<ShownItem>, Option<ShownItem>),
}

#[derive(Clone, Debug)]
enum ShownItem {
    /// A module's live activity (drawn by the module).
    Live(usize),
    /// A widget and the module it comes from (usize::MAX: an alert).
    W(Widget, usize),
}

/// Nookly's own order (Custom order off): what is switched on and
/// running, most urgent first, then the widgets two per row in the order
/// they were switched on.
pub(crate) fn auto_rows(l: &NotchLayout, live: &[(&'static str, usize, Option<u8>)]) -> Vec<NotchRow> {
    let mut lv: Vec<(&str, u8, usize)> = live.iter().enumerate().filter(|(_, (id, _, _))| l.contains(id)).map(|(k, (id, _, p))| (*id, p.unwrap_or(0), k)).collect();
    lv.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)));
    let mut rows: Vec<NotchRow> = lv.iter().map(|(id, _, _)| NotchRow::wide(id)).collect();
    let ws: Vec<&String> = l.picked.iter().filter(|p| !live.iter().any(|(id, _, _)| id == p)).collect();
    for c in ws.chunks(2) {
        rows.push(NotchRow::pair(c[0], c.get(1).map_or("", |s| s.as_str())));
    }
    rows
}

/// An alert as a widget, for Small alerts inside the closed notch.
fn alert_widget(t: &Toast) -> Widget {
    let value = if !t.right.is_empty() { t.right.clone() } else { t.sub.clone() };
    let mut w = Widget::new("alert", "Alert", t.icon, t.color, value).label(t.title.clone()).short(if !t.right.is_empty() { t.right.clone() } else { t.title.clone() }).detail(t.sub.clone());
    if let Some(p) = t.progress {
        w = w.ring(p);
    }
    w
}

/// The first row with something to show wins: (both sides, left, right).
/// A one-side row with an empty or idle side borrows the next one-side
/// item below it that has something to show.
pub(crate) fn pick_layout(rows: &[NotchRow], active: &dyn Fn(&str) -> bool) -> Option<(bool, String, String)> {
    let on = |id: &String| !id.is_empty() && active(id);
    for (k, row) in rows.iter().enumerate() {
        if row.wide {
            if on(&row.left) {
                return Some((true, row.left.clone(), String::new()));
            }
            continue;
        }
        let mut l = if on(&row.left) { row.left.clone() } else { String::new() };
        let mut r = if on(&row.right) { row.right.clone() } else { String::new() };
        if l.is_empty() && r.is_empty() {
            continue;
        }
        'fill: for below in rows[k + 1..].iter().filter(|b| !b.wide) {
            for id in below.ids() {
                if l.is_empty() && r.is_empty() {
                    break 'fill;
                }
                if (!l.is_empty() && !r.is_empty()) || *id == l || *id == r || !on(id) {
                    continue;
                }
                if l.is_empty() {
                    l = id.clone();
                } else {
                    r = id.clone();
                }
            }
        }
        return Some((false, l, r));
    }
    None
}

/// Self-test: the closed notch's order list, drag and drop, widgets.
fn widget_tests(a: &mut App) -> String {
    use crate::store::NotchLayout;
    let mut out = String::new();
    let ok = |b: bool| if b { "OK" } else { "FAIL" };
    // The list round-trips through settings text.
    let l = NotchLayout::custom_rows(vec![NotchRow::wide("music"), NotchRow::pair("agents", "battery"), NotchRow::pair("", "cpu")], true);
    let text = l.to_text();
    let back = NotchLayout::from_text(&text);
    out += &format!("{:<4} widget order saves and loads ({text})\n", ok(back == l.rows && text == "*music;agents,battery;,cpu"));
    let d = NotchLayout::default();
    let ids: Vec<String> = d.rows.iter().map(|r| r.left.clone()).collect();
    out += &format!("{:<4} default order: live activities only, each across both sides {ids:?}\n", ok(ids == ["pomodoro", "timer", "prayer", "music", "agents", "due"] && d.rows.iter().all(|r| r.wide)));
    // Switching a live activity on gives it both sides, above the widgets.
    let mut l2 = NotchLayout::custom_rows(vec![NotchRow::wide("music"), NotchRow::pair("battery", "")], true);
    l2.add("agents");
    l2.add("cpu");
    out += &format!("{:<4} a live activity switched on goes across both sides, above the widgets ({})\n", ok(l2.to_text() == "*music;*agents;battery,cpu"), l2.to_text());
    // Picking what shows.
    let pick = |rows: &[NotchRow], on: &[&str]| pick_layout(rows, &|id: &str| on.contains(&id));
    let s = |w: bool, l: &str, r: &str| Some((w, l.to_string(), r.to_string()));
    let music_first = vec![NotchRow::wide("music"), NotchRow::wide("agents")];
    let agents_first = vec![NotchRow::wide("agents"), NotchRow::wide("music")];
    let side_by_side = vec![NotchRow::pair("music", "agents")];
    let borrow = vec![NotchRow::pair("music", "agents"), NotchRow::wide("timer"), NotchRow::pair("battery", "cpu")];
    let cases = [
        pick(&music_first, &["music", "agents"]) == s(true, "music", ""),
        pick(&music_first, &["agents"]) == s(true, "agents", ""),
        pick(&agents_first, &["music", "agents"]) == s(true, "agents", ""),
        pick(&side_by_side, &["music", "agents"]) == s(false, "music", "agents"),
        pick(&side_by_side, &["agents"]) == s(false, "", "agents"),
        // An idle side borrows the next one-side item below (not a wide one).
        pick(&borrow, &["music", "timer", "cpu"]) == s(false, "music", "cpu"),
        pick(&borrow, &["timer", "cpu"]) == s(true, "timer", ""),
        pick(&borrow, &["battery"]) == s(false, "battery", ""),
        pick(&borrow, &[]).is_none(),
    ];
    out += &format!("{:<4} the first row with something to show wins; an empty side borrows from below {cases:?}\n", ok(cases.iter().all(|c| *c)));
    // Drag and drop.
    use crate::modules::widgets::{test_drop, test_toggle_size};
    let mut r = vec![NotchRow::wide("music"), NotchRow::pair("battery", ""), NotchRow::pair("cpu", "ram")];
    test_drop(&mut r, Some((2, true)), None, Some((1, true)), None); // ram -> beside battery
    let a1 = r == vec![NotchRow::wide("music"), NotchRow::pair("battery", "ram"), NotchRow::pair("cpu", "")];
    test_drop(&mut r, Some((1, false)), None, Some((1, true)), None); // swap sides
    let a2 = r[1] == NotchRow::pair("ram", "battery");
    test_drop(&mut r, Some((2, false)), None, None, Some(0)); // cpu to a new first row
    let a3 = r == vec![NotchRow::pair("cpu", ""), NotchRow::wide("music"), NotchRow::pair("ram", "battery")];
    test_drop(&mut r, None, Some(2), None, Some(0)); // row to the top
    let a4 = r[0] == NotchRow::pair("ram", "battery") && r.len() == 3;
    test_toggle_size(&mut r, 0); // two one-side items -> both sides: battery gets a row
    let a5 = r[0] == NotchRow::wide("ram") && r[1] == NotchRow::pair("battery", "");
    test_toggle_size(&mut r, 0);
    let a6 = r[0] == NotchRow::pair("ram", "");
    test_drop(&mut r, Some((0, false)), None, Some((3, false)), None); // onto the wide music row: swap
    let a7 = r.iter().any(|x| *x == NotchRow::wide("ram")) && r.iter().any(|x| x.left == "music" && !x.wide);
    out += &format!("{:<4} order list: pair, swap sides, new row, move row, both/one side {:?}\n", ok(a1 && a2 && a3 && a4 && a5 && a6 && a7), [a1, a2, a3, a4, a5, a6, a7]);
    // AirPods broadcast: 60 % left, 70 % right, case 40 %, not charging.
    let mut pkt = [0u8; 27];
    pkt[0] = 0x07;
    pkt[1] = 0x19;
    pkt[5] = 0x00;
    pkt[6] = 0x67;
    pkt[7] = 0x04;
    let p = crate::modules::widgets::decode_pods(&pkt);
    let good = p.map_or(false, |p| p.left == Some(60) && p.right == Some(70) && p.case == Some(40) && !p.charging);
    pkt[6] = 0xF7;
    let unknown = crate::modules::widgets::decode_pods(&pkt).map_or(false, |p| p.left.is_none() && p.right == Some(70));
    out += &format!("{:<4} AirPods battery broadcast decodes (L 60 R 70 case 40; unknown bud) {p:?}\n", ok(good && unknown && crate::modules::widgets::decode_pods(&pkt[..20]).is_none()));
    // Every live panel has an id, and the Widgets tab lists everything.
    let saved = (a.expanded, a.tab, a.settings.layout.clone());
    let cat = a.notch_catalog();
    let names: Vec<&str> = cat.iter().map(|c| c.id.as_str()).collect();
    let want = ["music", "timer", "pomodoro", "prayer", "due", "agents", "cpu", "ram", "net", "bluetooth", "date", "weather", "volume", "mic"];
    let missing: Vec<&&str> = want.iter().filter(|w| !names.contains(w)).collect();
    out += &format!("{:<4} the Widgets tab lists live activities and widgets ({} items, missing {missing:?})\n", ok(missing.is_empty()), cat.len());
    // Phase 2: weather (online only while it's on), Wi-Fi, sound.
    {
        use crate::modules::widgets as wg;
        let path = wg::weather_path(23.8103, 90.4125, false);
        let pf = wg::weather_path(40.71, -74.01, true);
        let parsed = wg::parse_weather(r#"{"current":{"time":"2026-11-11T10:00","temperature_2m":28.6,"weather_code":61,"is_day":1}}"#);
        let kinds = [wg::weather_kind(0, true).0, wg::weather_kind(3, true).0, wg::weather_kind(63, true).0, wg::weather_kind(95, false).0];
        let ok_w = path == "/v1/forecast?latitude=23.8&longitude=90.4&current=temperature_2m,weather_code,is_day"
            && pf.ends_with("&temperature_unit=fahrenheit")
            && parsed == Some((28.6, 61, true))
            && wg::parse_weather("{}").is_none()
            && kinds == ["Clear", "Cloudy", "Rain", "Storm"];
        out += &format!("{:<4} weather: asks Open-Meteo for the place rounded to 0.1°, reads the answer, names the sky {kinds:?}\n", ok(ok_w));
        let off = !a.settings.layout.contains("weather") && !wg::weather_started();
        out += &format!("{:<4} weather stays offline while its widget is off\n", ok(off || a.settings.layout.contains("weather")));
        let wifi = wg::wifi();
        out += &format!("INFO Wi-Fi now: {wifi:?}\n");
        let w: Vec<String> = cat.iter().filter(|c| ["wifi", "volume", "mic"].contains(&c.id.as_str())).map(|c| format!("{} {}", c.id, c.detail)).collect();
        out += &format!("INFO sound and Wi-Fi widgets: {w:?}\n");
        if std::env::var("NOOKLY_SELFTEST_ONLINE").is_ok() {
            let (lat, lon, place) = crate::modules::prayer::location();
            let r = crate::http::get("api.open-meteo.com", &wg::weather_path(lat, lon, false));
            let got = r.filter(|r| r.status == 200).and_then(|r| wg::parse_weather(&String::from_utf8_lossy(&r.body)));
            out += &format!("{:<4} live weather for {place}: {got:?}\n", ok(got.is_some()));
        }
    }
    // A widget row shows on the resting notch at the live width.
    a.settings.layout = NotchLayout::custom_rows(vec![NotchRow::pair("cpu", "date")], true);
    a.expanded = false;
    a.queue.clear();
    a.toast = None;
    let tgt = a.target();
    let shows = a.showing_ids();
    let full_w = tgt.0;
    out += &format!("{:<4} widgets show when nothing is live: {shows:?}, {:.0} wide\n", ok(shows == ["cpu", "date"] && full_w >= 190.0), full_w);
    // On a side edge the clock (hours over minutes) stays in the middle,
    // between the top and bottom items, in every style, with or without
    // seconds.
    {
        let before = (a.settings.edge, a.settings.style, a.settings.seconds, a.anchor);
        let mut blank = Vec::new();
        for style in [NotchStyle::Full, NotchStyle::Slim, NotchStyle::Compact] {
            for secs in [false, true] {
                a.settings.edge = Edge::Left;
                a.settings.style = style;
                a.settings.seconds = secs;
                a.anchor = CANVAS_H / 2.0;
                let tgt = a.target();
                a.cur = tgt;
                a.render(tgt, 0.0);
                let g = a.edge_rect(tgt.0, tgt.1);
                // The middle third of the notch, where the clock goes.
                let (s, px) = (a.scale, a.gfx.pixels());
                let (y0, y1) = (((g.y + g.h * 0.36) * s) as i32, ((g.b() - g.h * 0.36) * s) as i32);
                let (x0, x1) = ((g.x * s) as i32, (g.r() * s) as i32);
                let mut bright = 0;
                for y in y0.max(0)..y1.min(a.gfx.ph) {
                    for x in x0.max(0)..x1.min(a.gfx.pw) {
                        let i = ((y * a.gfx.pw + x) * 4) as usize;
                        if px[i] as u32 + px[i + 1] as u32 + px[i + 2] as u32 > 300 {
                            bright += 1;
                        }
                    }
                }
                if bright < 15 {
                    blank.push(format!("{style:?}{}", if secs { "+s" } else { "" }));
                }
            }
        }
        (a.settings.edge, a.settings.style, a.settings.seconds, a.anchor) = before;
        out += &format!("{:<4} side edges keep the clock between the live items {blank:?}\n", ok(blank.is_empty() && shows == ["cpu", "date"]));
    }
    a.settings.layout = NotchLayout::custom_rows(Vec::new(), true);
    let tgt2 = a.target();
    out += &format!("{:<4} an empty list leaves just the clock ({:.0} wide)\n", ok(a.showing_ids().is_empty() && tgt2.0 < full_w), tgt2.0);
    (a.expanded, a.tab, a.settings.layout) = saved;
    out += &widget_tests_2(a);
    out += &smart_hide_tests(a);
    out += &look_tests(a);
    out += &feature_tests(a);
    out += &phase_tests(a);
    out
}

/// Self-test: the everyday helpers, health, Ramadan, money, look, profiles.
fn phase_tests(a: &mut App) -> String {
    use crate::modules::{countdown, meetings};
    let ok = |b: bool| if b { "OK" } else { "FAIL" };
    let mut out = String::new();
    let start = (a.expanded, a.more_open, a.pill_off, a.tab, a.settings.clone());

    // Countdowns: dates in many forms, years roll over, yearly repeats.
    let today = (2026, 11, 12);
    let p = |t: &str| countdown::parse(t, today).map(|c| (c.name, c.y, c.m, c.d));
    let parsed = [p("Exam 2026-12-20"), p("Mum's birthday, 3 Mar"), p("Eid Jun 6"), p("Trip 20/12"), p("Party 5 Nov"), p("nonsense")];
    let want = [
        Some(("Exam".to_string(), 2026, 12, 20)),
        Some(("Mum's birthday".to_string(), 2027, 3, 3)),
        Some(("Eid".to_string(), 2027, 6, 6)),
        Some(("Trip".to_string(), 2026, 12, 20)),
        Some(("Party".to_string(), 2027, 11, 5)),
        None,
    ];
    let yearly = countdown::Countdown { y: 2000, m: 11, d: 10, yearly: true, name: "B".into() };
    let days = (countdown::days_left(&countdown::Countdown { y: 2026, m: 12, d: 20, yearly: false, name: "E".into() }, today), countdown::days_left(&yearly, today));
    out += &format!("{:<4} countdowns read 6 date forms and count days {:?}\n", ok(parsed == want && days == (38, 363)), days);

    // Calendar link: events, a weekly repeat with an exception, Join links.
    let base = meetings::local_to_unix(2026, 11, 9, 0, 0, 0);
    let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART:20261109T093000\r\nDTEND:20261109T094500\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR;COUNT=6\r\nEXDATE:20261111T093000\r\nSUMMARY:Standup\r\nDESCRIPTION:Join: https://teams.microsoft.com/l/meetup-join/abc\r\n  more\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20261112\r\nSUMMARY:Holiday\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART:20261110T100000Z\r\nSUMMARY:Cancelled thing\r\nSTATUS:CANCELLED\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let ev = meetings::parse_ics(ics, base - 86400.0, base + 30.0 * 86400.0);
    let standups: Vec<f64> = ev.iter().filter(|e| e.title == "Standup").map(|e| ((e.start - base) / 86400.0 * 10.0).round() / 10.0).collect();
    let link = ev.iter().find(|e| e.title == "Standup").and_then(|e| e.link.clone());
    let ics_ok = standups.len() == 5 && standups[0] == 0.4 && ev.iter().any(|e| e.title == "Holiday" && e.all_day) && !ev.iter().any(|e| e.title.starts_with("Cancelled")) && link.as_deref() == Some("https://teams.microsoft.com/l/meetup-join/abc");
    out += &format!("{:<4} calendar link: weekly repeat with an exception ({} standups), all-day events, cancelled skipped, Teams link found\n", ok(ics_ok), standups.len());

    // Currency rates from the API's JSON.
    let json = r#"{"result":"success","rates":{"USD":1,"EUR":0.92,"GBP":0.79,"LKR":298.5,"BDT":121.2,"INR":84.1,"SAR":3.75,"AED":3.6725}}"#;
    let rates = crate::modules::tools::parse_rates(json);
    out += &format!("{:<4} currency rates read from the API ({:?})\n", ok(rates.map_or(false, |r| r[3] == 298.5 && r[0] == 1.0)), rates.map(|r| r[3]));

    // Ramadan: the Hijri date (tabular): 1 March 2026 is in Ramadan 1447.
    let h = crate::modules::prayer::hijri(2026, 3, 1, 0);
    let h2 = crate::modules::prayer::hijri(2026, 5, 1, 0);
    out += &format!("{:<4} Hijri dates: 1 Mar 2026 = {h:?}, 1 May 2026 = {h2:?} (Ramadan mode keys off month 9)\n", ok(h.0 == 1447 && h.1 == 9 && h2.1 != 9));

    // Your own theme: readable, and Fix picks a readable text colour.
    let th = crate::gfx::custom_theme(0x1B1B2F, 0xF5F5F7);
    let bad = crate::gfx::contrast(hex(0xF2F2F2), hex(0xFFE7B3));
    a.settings.extra.insert("custom_bg".into(), "F2F2F2".into());
    a.settings.extra.insert("custom_text".into(), "FFE7B3".into());
    a.settings_action(S_CFIX);
    let fixed = custom_colors(&a.settings);
    out += &format!("{:<4} your own theme: contrast {:.1}:1; a {:.1}:1 pair is fixed to {:06X}\n", ok(crate::gfx::contrast(th.bg, th.text) >= 7.0 && bad < 4.5 && crate::gfx::contrast(hex(fixed.0), hex(fixed.1)) >= 7.0), crate::gfx::contrast(th.bg, th.text), bad, fixed.1);

    // Profiles keep their own features and theme.
    let mut st = a.settings.clone();
    st.extra.remove("profile");
    st.theme = "rose".into();
    st.hidden_panels = vec!["Tools".into()];
    crate::features::switch_profile(&mut st, "Work");
    st.theme = "ocean".into();
    st.hidden_panels = vec!["Notes".into()];
    crate::features::switch_profile(&mut st, "Home");
    let home = (st.theme.clone(), st.hidden_panels.clone());
    crate::features::switch_profile(&mut st, "Work");
    let work = (st.theme.clone(), st.hidden_panels.clone());
    st.extra.insert("profile_auto".into(), "1".into());
    let auto = (crate::features::auto_profile(&st, 2, 10), crate::features::auto_profile(&st, 6, 10));
    out += &format!("{:<4} profiles keep their own features and theme; Auto picks Work on weekdays 9-5 {:?}\n", ok(home.0 == "rose" && home.1 == ["Tools"] && work.0 == "ocean" && work.1 == ["Notes"] && auto == (None, Some("Home"))), auto);

    // Downloads: a finished file shows; a half-downloaded one doesn't.
    let dir = std::env::temp_dir().join(format!("nookly-dl-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let since = std::time::SystemTime::now() - std::time::Duration::from_secs(2);
    let _ = std::fs::write(dir.join("movie.mp4.crdownload"), b"partial");
    let half = crate::modules::downloads::newest_after(&dir, since).is_none();
    let _ = std::fs::write(dir.join("report.pdf"), b"%PDF done");
    let done = crate::modules::downloads::newest_after(&dir, since).map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned());
    let _ = std::fs::remove_dir_all(&dir);
    out += &format!("{:<4} downloads: a finished file is seen ({done:?}), a .crdownload isn't\n", ok(half && done.as_deref() == Some("report.pdf")));

    // Every new panel draws and fits.
    a.settings.hidden_panels.clear();
    a.expanded = true;
    let mut drawn = Vec::new();
    for title in ["Notes", "Countdowns", "Launcher", "Toggles", "Mirror", "Health"] {
        if let Some(i) = a.modules.iter().position(|m| m.title() == title) {
            a.tab = i;
            let tgt = a.target();
            a.cur = tgt;
            a.render(tgt, 0.0);
            out += &snap(a, &format!("panel {title}"));
            if a.gfx.last_err.is_none() {
                drawn.push(title);
            }
        }
    }
    out += &format!("{:<4} new panels draw: {drawn:?}\n", ok(drawn.len() == 6));
    // The Look page fits.
    a.settings.theme = "custom".into();
    a.sw_view(0, SEC_THEME);
    out += &a.sw_snap("theme section (custom theme)");
    let themes = a.sw_hits().iter().filter(|h| (S_THEME..S_THEME + 10).contains(&h.id) || h.id == S_CUSTOM).count();
    let look_ok = themes == 11 && a.sw_has(S_CBG) && a.sw_has(S_GLOW) && a.sw_has(S_SECONDS);
    a.sw_view(0, 0);
    let look_ok = look_ok && a.sw_has(S_MOTION) && a.sw_has(S_SIZE) && a.sw_has(S_MAKE_ROOM);
    let left = a.swin.as_ref().map_or(-1.0, |sw| sw.room[SEC_THEME as usize].min(sw.room[0]));
    a.close_settings();
    out += &format!("{:<4} Theme and Appearance: 11 themes, your own colours, size, motion and glow fit ({:.0} DIPs left)\n", ok(look_ok && left >= 0.0), left);

    let (e, so, sp, tb, st0) = start;
    (a.expanded, a.more_open, a.pill_off, a.tab) = (e, so, sp, tb);
    a.settings = st0;
    a.settings.save();
    a.tick();
    out
}

/// Self-test: tab overflow, shortcuts, export and import, the tour's presets.
fn feature_tests(a: &mut App) -> String {
    use crate::features::*;
    let ok = |b: bool| if b { "OK" } else { "FAIL" };
    let mut out = String::new();
    let start = (a.expanded, a.more_open, a.pill_off, a.tab, a.settings.hidden_panels.clone(), a.settings.extra.clone(), a.onboarding);
    let draw = |a: &mut App| {
        let tgt = a.target();
        a.cur = tgt;
        a.render(tgt, 0.0);
    };
    // Every panel on: what doesn't fit goes under ⋯, nothing is lost, and
    // the open panel is always in the bar.
    a.settings.hidden_panels.clear();
    a.onboarding = None;
    a.expanded = true;
    let all = a.tab_list();
    let last = *all.last().unwrap_or(&0);
    a.tab = last;
    a.more_open = false;
    draw(a);
    let bar_ids: Vec<u32> = a.hits.iter().filter(|h| h.module == APP_HIT && (h.id as usize) < a.modules.len()).map(|h| h.id).collect();
    let more = a.hits.iter().find(|h| h.module == APP_HIT && h.id == MORE).copied();
    let open_in_bar = bar_ids.contains(&(last as u32));
    let mut overflow_ok = true;
    if let Some(m) = more {
        a.on_down(m.rect.cx(), m.rect.cy());
        a.on_up(m.rect.cx(), m.rect.cy());
        draw(a);
        out += &snap(a, "more panels");
        let tiles: Vec<u32> = a.hits.iter().filter(|h| h.module == APP_HIT && (h.id as usize) < a.modules.len()).map(|h| h.id).collect();
        // The bar plus the tiles hold every panel once.
        let mut seen: Vec<u32> = tiles.clone();
        seen.sort();
        seen.dedup();
        overflow_ok = a.more_open && all.iter().all(|i| seen.contains(&(*i as u32)));
        // Picking a tile opens it and closes the ⋯ page.
        if let Some(h) = a.hits.iter().rev().find(|h| h.module == APP_HIT && (h.id as usize) < a.modules.len() && !bar_ids.contains(&h.id)).copied() {
            a.on_down(h.rect.cx(), h.rect.cy());
            a.on_up(h.rect.cx(), h.rect.cy());
            overflow_ok &= a.tab == h.id as usize && !a.more_open;
            draw(a);
            overflow_ok &= a.hits.iter().any(|x| x.module == APP_HIT && x.id == h.id && x.rect.y < a.tab_bar.b());
        } else {
            overflow_ok = false;
        }
    }
    out += &format!(
        "{:<4} {} panels: {} in the tab bar{}, the open one always there\n",
        ok(open_in_bar && overflow_ok && (more.is_some() || bar_ids.len() == all.len())),
        all.len(),
        bar_ids.len(),
        if more.is_some() { ", the rest under ⋯" } else { "" }
    );
    // Minimal: no ⋯.
    apply_preset(&mut a.settings, 0);
    a.tab = a.tab_list()[0];
    draw(a);
    let no_more = !a.hits.iter().any(|h| h.module == APP_HIT && h.id == MORE);
    out += &format!("{:<4} a few panels: no ⋯\n", ok(no_more));

    // Shortcuts.
    let def = hotkey(&a.settings, 0).map(|(m, v)| hotkey_label(m, v));
    let rec = [recorded(9, 0x4E), recorded(4, 0x41), recorded(0, 0x41), recorded(0, 0x71), recorded(2, 0x11)];
    let labels = (hotkey_label(2 | 4, 0x56), hotkey_label(0, 0x7B));
    let keys_ok = def.as_deref() == Some("Win+Alt+N") && rec == [Some((9, 0x4E)), None, None, Some((0, 0x71)), None] && labels == ("Ctrl+Shift+V".to_string(), "F12".to_string());
    out += &format!("{:<4} shortcuts: Win+Alt+N by default; Shift alone or a bare letter isn't one ({:?})\n", ok(keys_ok), labels);
    set_hotkey(&mut a.settings, 1, Some((2 | 4, 0x56)));
    set_hotkey(&mut a.settings, 0, None);
    let set_ok = hotkey(&a.settings, 1) == Some((6, 0x56)) && hotkey(&a.settings, 0).is_none();
    a.settings.save();
    let back = crate::store::Settings::load();
    out += &format!("{:<4} a changed or removed shortcut saves and loads\n", ok(set_ok && hotkey(&back, 1) == Some((6, 0x56)) && hotkey(&back, 0).is_none()));
    // Recording: the click starts it, Esc cancels, a combination sets it.
    a.sw_view(0, SEC_SHORTCUTS);
    a.recording = Some(2);
    a.record_key(0x1B);
    let esc = a.recording.is_none() && hotkey(&a.settings, 2).is_none();
    a.recording = Some(2);
    a.record_key(0x41); // a bare letter: still waiting
    let waits = a.recording == Some(2);
    a.record_key(0x73); // F4
    let f4 = a.recording.is_none() && hotkey(&a.settings, 2) == Some((0, 0x73));
    out += &format!("{:<4} recording a shortcut: Esc cancels, a bare letter waits, F4 sets it\n", ok(esc && waits && f4));
    a.sw_view(0, SEC_FEATURES);
    out += &a.sw_snap("features section");
    let rows_fit = a.swin.as_ref().map_or(false, |sw| sw.room[SEC_FEATURES as usize] >= 0.0) && a.sw_hits().iter().filter(|h| (S_FEATURE..S_FEATURE + 64).contains(&h.id)).count() > 20;
    a.sw_view(0, SEC_SHORTCUTS);
    out += &a.sw_snap("shortcuts section");
    let chips = a.sw_hits().iter().filter(|h| (S_HOTKEY..S_HOTKEY + 4).contains(&h.id)).count();
    let mouse_chips = a.sw_hits().iter().filter(|h| (S_MOUSE..S_MOUSE + 30).contains(&h.id)).count();
    let in_panel = a.swin.as_ref().map_or(false, |sw| sw.room[SEC_SHORTCUTS as usize] >= 0.0);
    // Recording from the window: a click starts it, Esc cancels.
    let rec = a.sw_click_id(S_HOTKEY) && a.recording == Some(0);
    a.record_key(0x1B);
    out += &format!("{:<4} Features section fits every switch; Shortcuts has {chips} keyboard and {mouse_chips} mouse choices and records\n", ok(rows_fit && in_panel && chips == 4 && mouse_chips == 9 && rec && a.recording.is_none()));

    // Export and import.
    let text = bundle();
    let files = unbundle(&text);
    let round = files.as_ref().map_or(false, |f| f.iter().any(|(n, b)| n == "settings.txt" && b.contains("hotkey_clip=6:86")));
    let junk = unbundle("hello\nworld").is_none() && unbundle("Nookly settings export 1\n=== Nookly file: ../evil.txt ===\nx\n").is_none();
    out += &format!("{:<4} export holds the data files; import takes only Nookly's own files\n", ok(round && junk));
    a.import_pending = files;
    a.sw_view(0, SEC_GENERAL);
    out += &a.sw_snap("import confirm");
    let asks = a.sw_has(S_IMPORT_OK) && a.sw_has(S_IMPORT_NO);
    a.settings_action(S_IMPORT_NO);
    out += &format!("{:<4} import asks before replacing anything; Cancel drops it\n", ok(asks && a.import_pending.is_none()));
    // General page: the export row fits.
    a.sw_view(0, SEC_GENERAL);
    let spare = a.swin.as_ref().map_or(-1.0, |sw| sw.room[SEC_GENERAL as usize]);
    let gen_ok = spare >= 0.0 && a.sw_has(S_EXPORT);
    a.close_settings();
    out += &format!("{:<4} General fits Export / Import ({:.0} DIPs to spare)\n", ok(gen_ok), spare);

    // The tour's presets.
    a.onboarding = Some(3);
    draw(a);
    out += &snap(a, "tour presets");
    let cards = a.hits.iter().filter(|h| h.module == ONBOARD && (O_PRESET..O_PRESET + 4).contains(&h.id)).count();
    a.onboarding_action(O_PRESET + 1);
    let student = current_preset(&a.settings) == Some(1);
    out += &format!("{:<4} the tour offers {cards} presets; picking Student switches it on\n", ok(cards == 4 && student));

    (a.expanded, a.more_open, a.pill_off, a.tab, a.settings.hidden_panels, a.settings.extra, a.onboarding) = start;
    a.close_settings();
    a.settings.save();
    a.more_open = false;
    a.tick();
    out
}

/// Self-test: themes are readable, seconds widen the clock, the calendar.
fn look_tests(a: &mut App) -> String {
    use crate::gfx::{Theme, GLASS_LIGHT, THEMES};
    let ok = |b: bool| if b { "OK" } else { "FAIL" };
    let mut out = String::new();
    let start = (a.expanded, a.more_open, a.tab, a.settings.layout.clone(), a.settings.style, a.settings.theme.clone());
    let lum = |c: Color| {
        let f = |v: f32| if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
        0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b)
    };
    let contrast = |x: Color, y: Color| {
        let (l1, l2) = (lum(x), lum(y));
        (l1.max(l2) + 0.05) / (l1.min(l2) + 0.05)
    };
    let over = |top: Color, under: Color| Color { r: top.r * top.a + under.r * (1.0 - top.a), g: top.g * top.a + under.g * (1.0 - top.a), b: top.b * top.a + under.b * (1.0 - top.a), a: 1.0 };
    // Solid themes: text, dim text, and a button over the background.
    let mut weak = Vec::new();
    for t in THEMES.iter().filter(|t| !t.glass) {
        let bg = Color { a: 1.0, ..t.bg };
        let btn = over(Color { a: 0.09, ..t.over }, bg);
        let (ct, cd, cb) = (contrast(t.text, bg), contrast(t.dim, bg), contrast(t.text, btn));
        if ct < 7.0 || cd < 3.0 || cb < 7.0 {
            weak.push(format!("{} {ct:.1}/{cd:.1}/{cb:.1}", t.key));
        }
    }
    // Liquid Glass over light, dark and colourful backdrops, every pixel:
    // the glass picks light or dark from the backdrop's brightness.
    let mut worst = Vec::new();
    for kind in ["light", "dark", "color"] {
        let (w, h, px, l) = crate::glass::sample(kind, 60, 20);
        let _ = (w, h);
        let t: Theme = if l > 0.62 { GLASS_LIGHT } else { THEMES.iter().copied().find(|t| t.glass).unwrap_or(THEMES[0]) };
        let mut min = f32::MAX;
        for p in px.chunks_exact(4) {
            let back = Color { r: p[2] as f32 / 255.0, g: p[1] as f32 / 255.0, b: p[0] as f32 / 255.0, a: 1.0 };
            min = min.min(contrast(t.text, over(t.bg, back)));
        }
        worst.push(format!("{kind} {:.1}", min));
        // Glass is see-through on purpose, so it can't reach the solid
        // themes' 4.5:1; it keeps at least 3:1 (WCAG's large-text level).
        if min < 3.0 {
            weak.push(format!("glass on {kind} {min:.1}"));
        }
    }
    // Coloured labels (orange, yellow, green, red, blue, cyan) on each
    // light theme, as the theme darkens them.
    let mut acc = Vec::new();
    for t in THEMES.iter().filter(|t| t.tint < 1.0) {
        let bg = Color { a: 1.0, ..t.bg };
        let min = [col::ACCENT, col::YELLOW, col::GREEN, col::RED, col::BLUE, col::CYAN]
            .iter()
            .map(|c| contrast(Color { r: c.r * t.tint, g: c.g * t.tint, b: c.b * t.tint, a: 1.0 }, bg))
            .fold(f32::MAX, f32::min);
        acc.push(format!("{} {min:.1}", t.key));
        if min < 3.0 {
            weak.push(format!("{} accents {min:.1}", t.key));
        }
    }
    out += &format!("{:<4} coloured labels on the light themes reach 3:1 (icons and bold labels) {acc:?}
", ok(acc.iter().all(|a| !weak.iter().any(|w| w.starts_with(a.split(' ').next().unwrap_or(""))))));
    out += &format!("{:<4} every theme's text is readable (WCAG AA or better), Liquid Glass on light, dark and colour backdrops {worst:?} {weak:?}
", ok(weak.is_empty()));
    // Each theme draws: the panel and the closed notch render without errors.
    let saved = (a.settings.theme.clone(), a.settings.style, a.expanded, a.tab, a.more_open);
    let mut bad = Vec::new();
    for t in THEMES.iter() {
        a.settings.theme = t.key.to_string();
        for open in [false, true] {
            a.expanded = open;
            let tgt = a.target();
            a.cur = tgt;
            a.render(tgt, 0.0);
            if a.gfx.last_err.is_some() || a.gfx.bright() == 0 && t.key != "light" {
                bad.push(format!("{} open {open}", t.key));
            }
        }
    }
    let glass_drawn = a.backdrop.is_some() || THEMES.iter().all(|t| !t.glass);
    out += &format!("{:<4} every theme draws the notch and panels; Liquid Glass has its backdrop {bad:?} {glass_drawn}
", ok(bad.is_empty() && glass_drawn));
    // Liquid Glass follows the notch at once: while it changes size (a
    // panel switch) it shows the matching part of one grab, with no new
    // grab until it rests, and the glass covers exactly the new shape.
    {
        a.settings.theme = "glass".into();
        a.backdrop = None;
        a.glass_raw = None;
        a.expanded = true;
        let tabs = a.tab_list();
        let small = tabs.iter().copied().min_by(|&x, &y| a.modules[x].size().0.partial_cmp(&a.modules[y].size().0).unwrap());
        let wide = tabs.iter().copied().max_by(|&x, &y| a.modules[x].size().0.partial_cmp(&a.modules[y].size().0).unwrap());
        if let (Some(s), Some(w)) = (small, wide) {
            a.tab = s;
            let tgt = a.target();
            a.cur = tgt;
            a.render(tgt, 0.0);
            let first = a.glass_raw.as_ref().map(|r| r.serial);
            // Switch to the widest panel and draw a few in-between frames.
            a.tab = w;
            let mut covers = true;
            for k in 1..=4 {
                let tgt = a.target();
                let f = k as f32 / 4.0;
                a.cur = (lerp(a.cur.0, tgt.0, f), lerp(a.cur.1, tgt.1, f), lerp(a.cur.2, tgt.2, f));
                a.render(tgt, if k < 4 { 20.0 } else { 0.0 });
                let notch = a.edge_rect(a.cur.0, a.cur.1);
                if let Some((_, _, _, at)) = &a.backdrop {
                    covers &= (at.x - notch.x).abs() <= 4.0 / a.scale + 1.0 && (at.r() - notch.r()).abs() <= 4.0 / a.scale + 1.0 && (at.b() - notch.b()).abs() <= 4.0 / a.scale + 1.0;
                } else {
                    covers = false;
                }
            }
            let one_grab = first.is_some() && a.glass_raw.as_ref().map(|r| r.serial) == first;
            out += &format!("{:<4} Liquid Glass shows what's behind the new shape at once, from one grab, while it changes size {:?}\n", ok(covers && one_grab), (covers, one_grab));
            // A window changing behind it (switch, move, open, close) is
            // picked up on the very next frame, even mid-animation; so is
            // the notch starting to open.
            let s0 = a.glass_raw.as_ref().map(|r| r.serial);
            a.glass_changed();
            let tgt = a.target();
            a.render(tgt, 20.0);
            let after_change = a.glass_raw.as_ref().map(|r| r.serial) != s0;
            a.expanded = false;
            a.toast = None;
            a.frame();
            let s1 = a.glass_raw.as_ref().map(|r| r.serial);
            a.expanded = true;
            a.frame();
            let on_open = a.glass_raw.as_ref().map(|r| r.serial) != s1;
            out += &format!("{:<4} Liquid Glass grabs again when a window changes behind it and when the notch opens {:?}\n", ok(after_change && on_open), (after_change, on_open));
        }
    }
    (a.settings.theme, a.settings.style, a.expanded, a.tab, a.more_open) = saved;
    a.backdrop = None;
    a.glass_raw = None;
    // Seconds: the clock face and the narrow notches grow.
    let o = std::sync::atomic::Ordering::Relaxed;
    let style = a.settings.style;
    a.expanded = false;
    a.settings.layout = crate::store::NotchLayout::custom_rows(Vec::new(), true);
    let mut widths = Vec::new();
    for s in [NotchStyle::Slim, NotchStyle::Compact] {
        a.settings.style = s;
        a.settings.seconds = false;
        crate::time::SECONDS.store(false, o);
        let w0 = a.target().0;
        a.settings.seconds = true;
        crate::time::SECONDS.store(true, o);
        let w1 = a.target().0;
        widths.push((w0, w1));
    }
    let face = Now::get().hhmm();
    let fits = {
        let mid = a.gfx.measure("12:58:59", 14.0, Weight::Semi) + 2.0;
        let wing = a.slim_live_w() / 2.0 - mid / 2.0 - SLIM_GAP - SLIM_WING_PAD;
        let room = a.gfx.measure("Timer", 12.0, Weight::Semi) + 18.0 + 5.0;
        wing >= room
    };
    a.settings.seconds = false;
    crate::time::SECONDS.store(false, o);
    a.settings.style = style;
    let grow = widths.iter().all(|(w0, w1)| w1 > w0);
    out += &format!("{:<4} seconds: the clock shows them ({face}), Slim and Compact grow {widths:?}, labels still fit beside 12:58:59 ({fits})
", ok(grow && face.matches(':').count() == 2 && fits));
    // Calendar: the month grid.
    use crate::modules::calendar::month_grid;
    let g = month_grid(2026, 11, 1);
    let nov = g.len() == 42 && g[0] == (2026, 10, 26) && g.iter().filter(|d| d.1 == 11).count() == 30 && g.contains(&(2026, 11, 30));
    let feb = month_grid(2024, 2, 0).iter().filter(|d| d.1 == 2).count() == 29;
    let sun = month_grid(2026, 11, 0)[0] == (2026, 11, 1);
    let wd = crate::time::weekday(2026, 11, 11) == 3;
    out += &format!("{:<4} calendar: month grid (Monday or Sunday first), leap February, weekdays ({nov}, {feb}, {sun}, {wd})
", ok(nov && feb && sun && wd));
    (a.expanded, a.more_open, a.tab, a.settings.layout, a.settings.style, a.settings.theme) = start;
    out
}

/// Self-test: stepping aside for a window under the notch.
fn smart_hide_tests(a: &mut App) -> String {
    let o = std::sync::atomic::Ordering::Relaxed;
    let ok = |b: bool| if b { "OK" } else { "FAIL" };
    let mut out = String::new();
    let saved = (a.settings.style, a.settings.make_room, a.expanded, a.hover_block, a.more_open);
    a.settings.style = NotchStyle::Full;
    a.settings.make_room = true;
    a.expanded = false;
    a.toast = None;
    a.queue.clear();
    a.hover_block = false;
    a.near_until = -10.0;
    // A window comes under the notch: it steps aside at once...
    MAXIMIZED_OVERRIDE.store(1, o);
    a.covered = false;
    a.update_make_room();
    let aside = a.yielding() && a.look() == NotchStyle::Pill && a.passthrough;
    // ...and comes back after a moment, still click-through.
    a.covered_since = a.t() - SMART_SETTLE - 0.1;
    a.sync_yield();
    let back = !a.yielding() && a.look() == NotchStyle::Full && a.passthrough;
    // The mouse near it (the address bar below): steps aside again.
    let zone = a.edge_rect(240.0, 38.0);
    a.poll_at(zone.cx() + 150.0, zone.b() + 40.0);
    let near = a.yielding();
    // The mouse gone for a while: back.
    a.poll_at(zone.cx(), zone.b() + 300.0);
    a.near_until = a.t() - 0.1;
    a.sync_yield();
    let gone = !a.yielding();
    // Resting on the notch itself opens it.
    a.poll_at(zone.cx(), zone.cy());
    std::thread::sleep(std::time::Duration::from_millis(360));
    a.poll_at(zone.cx(), zone.cy());
    let opened = a.expanded && !a.passthrough;
    a.expanded = false;
    a.sync_passthrough();
    // Switching to a window under it, or dragging one: steps aside.
    a.covered_since = a.t() - 10.0;
    a.window_switched(a.hwnd);
    let switched = a.yielding();
    a.covered_since = a.t() - 10.0;
    a.moving = true;
    let dragging = a.yielding();
    a.moving = false;
    // A window on the other screen (not under the notch): switching to it
    // or dragging it doesn't make the notch step aside.
    a.covered_since = a.t() - 10.0;
    a.window_switched(HWND(std::ptr::null_mut()));
    let other_switch = !a.yielding();
    a.window_moving(true, HWND(std::ptr::null_mut()));
    let other_drag = !a.yielding();
    a.window_moving(false, HWND(std::ptr::null_mut()));
    let other_drop = !a.yielding();
    out += &format!(
        "{:<4} smart hide: a window on another screen doesn't move it (switch, drag, drop) {:?}\n",
        ok(other_switch && other_drag && other_drop),
        (other_switch, other_drag, other_drop)
    );
    out += &format!(
        "{:<4} smart hide: steps aside when a window comes under it, comes back, steps aside when the mouse is near, opens when you rest on it, steps aside on switch and drag {:?}
",
        ok(aside && back && near && gone && opened && switched && dragging),
        (aside, back, near, gone, opened, switched, dragging)
    );
    // No window under it: the normal notch.
    MAXIMIZED_OVERRIDE.store(2, o);
    a.update_make_room();
    let normal = !a.yielding() && !a.passthrough && a.look() == NotchStyle::Full;
    let r = |l, t, rr, b| RECT { left: l, top: t, right: rr, bottom: b };
    let area = r(400, 0, 740, 46);
    let rects = under_rect(r(0, 0, 1920, 1040), area) && under_rect(r(500, 20, 900, 600), area) && !under_rect(r(500, 46, 900, 600), area) && !under_rect(r(0, 0, 400, 300), area);
    out += &format!("{:<4} smart hide: normal notch with nothing under it; overlap rules ({normal}, {rects})
", ok(normal && rects));
    MAXIMIZED_OVERRIDE.store(0, o);
    let real = a.window_under();
    MAXIMIZED_OVERRIDE.store(2, o);
    out += &format!("INFO a window under the notch on this screen right now: {real}
");
    (a.settings.style, a.settings.make_room, a.expanded, a.hover_block, a.more_open) = saved;
    a.update_make_room();
    out
}

/// Self-test: simple and custom order, when-to-show, alerts, place, mascot.
fn widget_tests_2(a: &mut App) -> String {
    use crate::store::{Place, PlaceMode};
    let mut out = String::new();
    let ok = |b: bool| if b { "OK" } else { "FAIL" };
    let saved = a.settings.clone();
    let view = (a.expanded, a.more_open, a.tab, a.mood);
    // Everything new saves and loads.
    let mut st = a.settings.clone();
    st.layout = NotchLayout::default();
    st.layout.custom = true;
    st.layout.add("weather");
    st.layout.when.insert("battery".into(), ShowWhen::Change);
    st.layout.alerts.insert("caps".into(), AlertMode::Small);
    st.place = Place { mode: PlaceMode::Manual, lat: 6.9271, lon: 79.8612, name: "Colombo, Sri Lanka".into(), prayer: true, fahrenheit: true };
    st.save();
    let back = Settings::load();
    out += &format!("{:<4} custom order, picks, when-to-show, alert modes and place save and load\n", ok(back.layout == st.layout && back.place == st.place));
    // Settings from the last build: an edited order stays custom.
    crate::store::write("settings.txt", "onboarded=1\nnotch_layout=*music;cpu,\n");
    let old = Settings::load();
    crate::store::write("settings.txt", "onboarded=1\nnotch_layout=*pomodoro;*timer;*prayer;*music;*agents;*due\n");
    let plain = Settings::load();
    let up = old.layout.custom && old.layout.picked == ["music", "cpu"] && !plain.layout.custom;
    out += &format!("{:<4} an order edited in the last build stays a custom order; the default one becomes simple\n", ok(up));
    a.settings = saved.clone();
    a.settings.save();
    // Simple mode orders by urgency: an agent that needs you above music.
    let mut l = NotchLayout::default();
    l.add("battery");
    l.add("bluetooth");
    let live = [("pomodoro", 0usize, None), ("music", 1, Some(50u8)), ("agents", 2, Some(75)), ("timer", 3, None)];
    let rows = auto_rows(&l, &live);
    let first: Vec<&str> = rows.iter().take(2).map(|r| r.left.as_str()).collect();
    let last = rows.last().cloned().unwrap_or_default();
    out += &format!("{:<4} simple mode: most urgent first {first:?}, widgets two per row\n", ok(first == ["agents", "music"] && last == NotchRow::pair("battery", "bluetooth")));
    // "On change": shows for a few seconds after the value changes.
    a.expanded = false;
    a.toast = None;
    a.queue.clear();
    let mut l = NotchLayout::custom_rows(vec![NotchRow::pair("date", "")], true);
    l.when.insert("date".into(), ShowWhen::Change);
    a.settings.layout = l;
    a.widget_seen.clear();
    a.refresh_shown();
    let quiet_first = a.showing_ids().is_empty();
    a.widget_seen.insert("date", ("changed".into(), f64::NEG_INFINITY));
    a.refresh_shown();
    let shows = a.showing_ids() == ["date"];
    let earlier = a.t() - 5.0;
    if let Some(v) = a.widget_seen.get_mut("date") {
        v.1 = earlier;
    }
    a.refresh_shown();
    let gone = a.showing_ids().is_empty();
    out += &format!("{:<4} \"On change\" shows a widget for a few seconds after it changes ({quiet_first}, {shows}, {gone})\n", ok(quiet_first && shows && gone));
    // Alerts: Small goes inside the closed notch, Off goes nowhere.
    a.settings.layout = NotchLayout::custom_rows(vec![NotchRow::pair("cpu", "")], false);
    a.settings.set_alert_mode("caps", AlertMode::Small);
    a.small_alert = None;
    a.push_toast(Toast::new("caps", glyph::CAPS, col::GREEN, "Caps Lock", "On").secs(1.6));
    a.refresh_shown();
    let small = a.queue.is_empty() && a.showing_ids() == ["cpu", "alert"];
    a.small_alert = None;
    a.settings.set_alert_mode("caps", AlertMode::Off);
    a.push_toast(Toast::new("caps", glyph::CAPS, col::GREEN, "Caps Lock", "Off").secs(1.6));
    let off = a.queue.is_empty() && a.small_alert.is_none();
    a.settings.set_alert_mode("caps", AlertMode::Popup);
    a.push_toast(Toast::new("caps", glyph::CAPS, col::GREEN, "Caps Lock", "On").secs(1.6));
    let pop = a.queue.len() == 1;
    a.queue.clear();
    out += &format!("{:<4} alerts: Small shows beside what the notch shows, Off nothing, Pop-up the card ({small}, {off}, {pop})\n", ok(small && off && pop));
    // The Settings switches for volume and Copied are the same setting.
    a.settings.volume_overlay = false;
    let v_off = a.settings.alert_mode("volume") == AlertMode::Off;
    a.settings.set_alert_mode("volume", AlertMode::Small);
    let v_on = a.settings.volume_overlay && a.settings.alert_mode("volume") == AlertMode::Small;
    a.settings.copy_toast = true;
    let c_on = a.settings.alert_mode("copy") == AlertMode::Popup;
    out += &format!("{:<4} Volume overlay and Show what I copy are the same as their alert modes\n", ok(v_off && v_on && c_on));
    // Slim with a side mascot: it takes a side and the notch stays 200
    // (it grows only for an item across both sides).
    a.settings.style = NotchStyle::Slim;
    a.settings.edge = Edge::Top;
    a.settings.personality = true;
    a.settings.mascot_pos = 0;
    a.mood = Mood::Eyes;
    a.settings.layout = NotchLayout::custom_rows(vec![NotchRow::pair("cpu", "date")], false);
    let tgt = a.target();
    a.cur = tgt;
    a.render(tgt, 0.0);
    let notch = a.edge_rect(tgt.0, tgt.1);
    let inside = a.mascot_rect.map_or(false, |m| notch.contains(m.cx(), m.cy()));
    let pair_w = tgt.0;
    a.settings.layout = NotchLayout::custom_rows(vec![NotchRow::wide("date")], false);
    let wide_w = a.target().0;
    a.settings.mascot_pos = 1;
    a.settings.layout = NotchLayout::custom_rows(vec![NotchRow::pair("cpu", "date")], false);
    let tgt = a.target();
    a.cur = tgt;
    a.render(tgt, 0.0);
    let centre_w = tgt.0;
    let small_mascot = a.mascot_rect.map_or(false, |m| m.w <= 60.0);
    out += &format!(
        "{:<4} Slim with a mascot: side mascot keeps 200 ({pair_w:.0}, mascot inside {inside}), grows only beside a both-sides item ({wide_w:.0}); centred stays 200 and small ({centre_w:.0}, {small_mascot})\n",
        ok(pair_w == SLIM_LIVE_W && inside && wide_w > SLIM_LIVE_W && centre_w == SLIM_LIVE_W && small_mascot)
    );
    // Clicking the closed notch opens the panel of what it shows.
    a.mood = Mood::Clock;
    a.settings.personality = false;
    a.settings.layout = NotchLayout::custom_rows(vec![NotchRow::pair("cpu", "")], false);
    a.expanded = false;
    a.tab = 0;
    let tgt = a.target();
    a.cur = tgt;
    a.render(tgt, 0.0);
    let bg = a.hits.iter().find(|h| h.module == APP_HIT && h.id == u32::MAX).map(|h| h.rect);
    if let Some(r) = bg {
        a.on_down(r.cx(), r.cy());
        a.on_up(r.cx(), r.cy());
    }
    let mon = find(a, "System Monitor");
    // A drag that loses the mouse is cancelled.
    let w = find(a, "Widgets").unwrap_or(0);
    a.inner_drag = Some(w);
    a.handle(WM_CAPTURECHANGED, WPARAM(0), LPARAM(0));
    out += &format!("{:<4} a drag that loses the mouse (Alt+Tab) is cancelled
", ok(a.inner_drag.is_none()));
    // Opening a folder or link waits until the click is handled.
    open_later("about:blank");
    let queued = OPEN_LATER.with(|q| q.borrow().len()) == 1;
    OPEN_LATER.with(|q| q.borrow_mut().clear());
    out += &format!("{:<4} folders and links open after the click is handled (no lost mouse messages)
", ok(queued));
    out += &format!("{:<4} clicking the closed notch opens what it shows (tab {})\n", ok(a.expanded && Some(a.tab) == mon), a.modules[a.tab].title());
    a.expanded = false;
    // Place helpers.
    use crate::modules::widgets as wg;
    let tz = wg::tz_city("Sri Lanka Standard Time").map(|p| p.2);
    let cities = wg::parse_cities(r#"{"results":[{"name":"Colombo","latitude":6.93,"longitude":79.85,"country":"Sri Lanka"},{"name":"Kandy","latitude":7.29,"longitude":80.63}]}"#);
    let near = wg::near_name(6.90, 79.86);
    let here = wg::tz_place();
    let ok_p = tz.as_deref() == Some("Colombo") && cities.len() == 2 && cities[0].0 == "Colombo, Sri Lanka" && cities[1].0 == "Kandy" && near == "Near Colombo";
    out += &format!("{:<4} place: time-zone city, city search results, \"near\" names ({near}; this PC's time zone: {here:?})\n", ok(ok_p));
    a.settings = saved;
    a.settings.save();
    (a.expanded, a.more_open, a.tab, a.mood) = view;
    a.small_alert = None;
    out
}

/// Name and Pick-page line of each live activity.
fn live_names(id: &str) -> (&'static str, &'static str) {
    match id {
        "music" => ("Music", "While a song plays"),
        "timer" => ("Timer", "While a timer or stopwatch runs"),
        "pomodoro" => ("Pomodoro", "During a focus session"),
        "prayer" => ("Prayer countdown", "15 min before a prayer"),
        "due" => ("Due tasks", "When a task is due"),
        "agents" => ("AI agents", "While an agent works or needs you"),
        "meeting" => ("Next meeting", "15 min before a meeting"),
        "downloads" => ("Downloads", "When a download finishes"),
        _ => ("Live activity", ""),
    }
}

/// Icon, or a progress ring, of a widget.
fn widget_lead(ui: &mut Ui, r: Rect, w: &Widget) {
    match w.ring {
        Some(f) => {
            ui.stroke_circle(r.cx(), r.cy(), 6.5, w.color.with_a(0.25), 2.0);
            ui.arc(r.cx(), r.cy(), 6.5, 0.0, std::f32::consts::TAU * f, w.color, 2.0);
        }
        None => ui.icon(w.icon, r, 12.0, w.color),
    }
}

/// A widget on one side of the clock: icon (or ring) and value.
fn draw_widget_side(ui: &mut Ui, w: &Widget, r: Rect, right: bool) {
    // Step down until it fits: icon and value, icon and short value, the
    // value alone (no "..." in the notch).
    let size = if r.h < 22.0 { 11.0 } else { 13.0 };
    let lead_w = r.h.min(18.0);
    let fits = |ui: &mut Ui, s: &str, lead: bool| ui.measure(s, size, Weight::Semi) + if lead { lead_w + 5.0 } else { 0.0 } <= r.w;
    let (text, lead) = if fits(ui, &w.value, true) {
        (w.value.as_str(), true)
    } else if fits(ui, &w.short, true) {
        (w.short.as_str(), true)
    } else if fits(ui, &w.value, false) {
        (w.value.as_str(), false)
    } else {
        (w.short.as_str(), false)
    };
    let tw = ui.measure(text, size, Weight::Semi);
    if lead {
        let (lr, tr) = side_rects(r, right, lead_w, tw, 5.0);
        widget_lead(ui, lr, w);
        ui.text(text, tr, size, col::TEXT, Weight::Semi, Align::Left);
    } else {
        ui.text(text, r, size, col::TEXT, Weight::Semi, if right { Align::Right } else { Align::Left });
    }
}

/// A widget across both sides: icon and name left, value right (side
/// edges: icon on top, value below).
fn draw_widget_wide(ui: &mut Ui, w: &Widget, left: Rect, right: Rect, vertical: bool, compact: bool) {
    if vertical {
        widget_lead(ui, left, w);
        if !compact {
            ui.text(&w.value, right, 12.0, col::TEXT, Weight::Semi, Align::Center);
        }
        return;
    }
    let small = left.h < 22.0;
    let lead = Rect::new(left.x, left.y, left.h.min(18.0), left.h);
    widget_lead(ui, lead, w);
    let room = left.w - lead.w - 5.0;
    if !small && ui.measure(&w.label, 12.0, Weight::Semi) <= room {
        ui.text(&w.label, Rect::new(lead.r() + 5.0, left.y, room, left.h), 12.0, w.color, Weight::Semi, Align::Left);
    }
    let vs = if small { 12.0 } else { 14.0 };
    let v = if ui.measure(&w.value, vs, Weight::Semi) <= right.w { &w.value } else { &w.short };
    ui.text(v, right, vs, col::TEXT, Weight::Semi, Align::Right);
}

/// Room for tabs on the usual panel width, with a fixed allowance for the
/// clock (so the ⋯ doesn't come and go as the time gets wider).
fn bar_room(tab_h: f32) -> f32 {
    modules::PANEL.0 - (tab_h + 4.0 + TAB_GAP) - 70.0 - 84.0
}

thread_local! {
    /// The logo's pixels, made once per size.
    static LOGO: std::cell::RefCell<Option<(u32, Vec<u8>)>> = const { std::cell::RefCell::new(None) };
}

/// The clock on a side edge: hours over minutes (and seconds), centred in
/// `area` and shrunk to fit it.
fn stacked_clock(ui: &mut Ui, area: Rect, size: f32, seconds: bool) {
    let now = Now::get();
    let lines = if seconds { 3.0 } else { 2.0 };
    if area.h < 2.0 * size {
        return;
    }
    let line = area.h.min(size * 1.3 * lines) / lines;
    let size = size.min(line / 1.15);
    let y0 = area.cy() - line * lines / 2.0;
    let hour = if crate::time::H12.load(std::sync::atomic::Ordering::Relaxed) { (now.hour + 11) % 12 + 1 } else { now.hour };
    ui.text(&format!("{:02}", hour), Rect::new(area.x, y0, area.w, line), size, col::TEXT, Weight::Semi, Align::Center);
    ui.text(&format!("{:02}", now.min), Rect::new(area.x, y0 + line, area.w, line), size, col::DIM, Weight::Semi, Align::Center);
    if seconds {
        ui.text(&format!("{:02}", now.sec), Rect::new(area.x, y0 + 2.0 * line, area.w, line), size * 0.85, col::FAINT, Weight::Semi, Align::Center);
    }
}

/// Nookly's mark, the very same pixels as the app icon (gfx::logo_pixels).
fn draw_logo(ui: &mut Ui, r: Rect) {
    let n = (r.w * 2.0).round() as u32;
    let px = LOGO.with(|l| {
        let mut l = l.borrow_mut();
        if l.as_ref().map_or(true, |(s, _)| *s != n) {
            *l = Some((n, crate::gfx::logo_pixels(n, true)));
        }
        l.as_ref().unwrap().1.clone()
    });
    if let Some(b) = ui.make_bitmap(n, n, &px) {
        ui.image_rr(&b, r, 0.0);
    }
}

/// Your own theme's colours: (background, text).
fn custom_colors(st: &Settings) -> (u32, u32) {
    let get = |k: &str, d: u32| st.extra.get(k).and_then(|v| u32::from_str_radix(v, 16).ok()).unwrap_or(d);
    (get("custom_bg", 0x1B1B2F), get("custom_text", 0xF5F5F7))
}

/// Settings > Appearance > Size: the notch and panels a little smaller or larger.
fn ui_size(st: &Settings) -> f32 {
    match st.extra.get("ui_size").map(String::as_str) {
        Some("s") => 0.9,
        Some("l") => 1.15,
        _ => 1.0,
    }
}

fn reduce_motion(st: &Settings) -> bool {
    st.extra.get("reduce_motion").map_or(false, |v| v == "1")
}

/// Settings > Panels: the tab bar's order (panel titles), if you set one.
fn panel_order(st: &Settings) -> Vec<String> {
    st.extra.get("panel_order").map_or(Vec::new(), |v| v.split('|').filter(|s| !s.is_empty()).map(String::from).collect())
}

/// Apps (exe names) the notch hides for.
fn hidden_apps(st: &Settings) -> Vec<String> {
    st.extra.get("hide_apps").map_or(Vec::new(), |v| v.split('|').filter(|s| !s.is_empty()).map(String::from).collect())
}

/// Names on the Features page (and in "... is off").
fn feature_name(title: &'static str) -> &'static str {
    match title {
        "AI Agents" => "AI agents",
        "Prayer Times" => "Prayer times",
        "Clipboard" => "Clipboard",
        t => panel_name(t),
    }
}

fn panel_name(title: &'static str) -> &'static str {
    match title {
        "Now Playing" => "Music",
        "TodoList" => "To-do",
        "Water Reminder" => "Water",
        "Prayer Times" => "Prayer",
        "AI Agents" => "Agents",
        "System Monitor" => "Monitor",
        t => t,
    }
}

/// Short chip labels for the clipboard capture setting.
fn clip_mode_short(m: ClipMode) -> &'static str {
    match m {
        ClipMode::Text => "Text",
        ClipMode::TextFiles => "Text + files",
        ClipMode::All => "Everything",
    }
}

/// Shell windows never count as full screen (the desktop, taskbar, Start
/// and search all cover the screen or sit on top of it).
const SHELL_CLASSES: [&str; 7] = [
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "Windows.UI.Core.CoreWindow",
    "XamlExplorerHostIslandWindow",
    "TopLevelWindowForOverflowXamlIsland",
];

/// A window is full screen when it covers the whole monitor (taskbar area
/// included) without being merely maximized, and isn't part of the shell.
/// A maximized window stops at the taskbar, or is "zoomed" when the taskbar
/// auto-hides, so it never counts.
fn is_fullscreen(win: RECT, mon: RECT, zoomed: bool, class: &str) -> bool {
    if zoomed || SHELL_CLASSES.contains(&class) {
        return false;
    }
    win.left <= mon.left && win.top <= mon.top && win.right >= mon.right && win.bottom >= mon.bottom
}

/// Re-check full screen the moment another window takes the foreground or
/// is minimized/restored (the 1 s tick covers everything else).
extern "system" fn on_foreground(_h: windows::Win32::UI::Accessibility::HWINEVENTHOOK, e: u32, w: HWND, _o: i32, _c: i32, _t: u32, _ms: u32) {
    with_app(|a| {
        a.check_fullscreen();
        a.keep_on_top();
        a.glass_changed();
        if e == EVENT_SYSTEM_FOREGROUND {
            a.window_switched(w);
        } else {
            a.update_make_room();
        }
    });
}

extern "system" fn on_movesize(_h: windows::Win32::UI::Accessibility::HWINEVENTHOOK, e: u32, w: HWND, _o: i32, _c: i32, _t: u32, _ms: u32) {
    with_app(|a| {
        a.window_moving(e == EVENT_SYSTEM_MOVESIZESTART, w);
        if e == EVENT_SYSTEM_MOVESIZEEND {
            a.glass_changed();
        }
    });
}

/// Give memory that's no longer in use back to Windows: free heap pages,
/// and the working set (pages still needed come back on their own).
pub fn trim_memory() {
    unsafe {
        use windows::Win32::System::Memory::{GetProcessHeap, HeapCompact, HEAP_FLAGS};
        use windows::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};
        if let Ok(h) = GetProcessHeap() {
            HeapCompact(h, HEAP_FLAGS(0));
        }
        let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

/// Is a visible window above the notch in the z-order and over its place?
/// Menus, tooltips, the shell's own pop-ups and see-through overlays don't
/// count: those come and go on their own.
unsafe fn window_above(me: HWND, area: RECT) -> bool { unsafe {
    const SKIP: &[&str] = &["#32768", "tooltips_class32", "Xaml_WindowedPopupClass", "Shell_TrayWnd", "Shell_SecondaryTrayWnd", "NotifyIconOverflowWindow", "Windows.UI.Core.CoreWindow", "XamlExplorerHostIslandWindow", "TopLevelWindowForOverflowXamlIsland", "ForegroundStaging", "SysShadow", "IME", "MSCTFIME UI"];
    let mut h = GetWindow(me, GW_HWNDPREV).unwrap_or_default();
    let mut n = 0;
    while !h.0.is_null() && n < 256 {
        n += 1;
        if IsWindowVisible(h).as_bool() && !IsIconic(h).as_bool() {
            let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
            let see_through = ex & WS_EX_TRANSPARENT.0 as isize != 0;
            let mut cloaked = 0u32;
            let _ = windows::Win32::Graphics::Dwm::DwmGetWindowAttribute(h, windows::Win32::Graphics::Dwm::DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut _, 4);
            let mut r = RECT::default();
            if !see_through && cloaked == 0 && GetWindowRect(h, &mut r).is_ok() {
                let overlaps = r.left < area.right && r.right > area.left && r.top < area.bottom && r.bottom > area.top;
                if overlaps && r.right - r.left > 1 && r.bottom - r.top > 1 {
                    let mut buf = [0u16; 128];
                    let len = GetClassNameW(h, &mut buf) as usize;
                    let class = String::from_utf16_lossy(&buf[..len.min(buf.len())]);
                    if !SKIP.contains(&class.as_str()) {
                        return true;
                    }
                }
            }
        }
        h = GetWindow(h, GW_HWNDPREV).unwrap_or_default();
    }
    false
}}

/// Looking for a window under the notch (EnumWindows).
struct UnderSearch {
    area: RECT,
    me: HWND,
    pid: u32,
    found: bool,
}

/// Does this window sit under the notch's area? Skips our own windows,
/// hidden, minimized and cloaked ones, tool windows, click-through overlays,
/// the desktop and the taskbar.
unsafe extern "system" fn under_cb(h: HWND, lp: LPARAM) -> BOOL { unsafe {
    let s = &mut *(lp.0 as *mut UnderSearch);
    if covers(h, s.area, s.me, s.pid) {
        s.found = true;
        return FALSE;
    }
    TRUE
}}

/// Is this window under `area`? False for our own windows, hidden,
/// minimized and cloaked ones, tool windows, click-through overlays, the
/// desktop, the taskbar and small popups.
unsafe fn covers(h: HWND, area: RECT, me: HWND, me_pid: u32) -> bool { unsafe {
    if h == me || !IsWindowVisible(h).as_bool() || IsIconic(h).as_bool() {
        return false;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(h, Some(&mut pid));
    if pid == me_pid {
        return false;
    }
    let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
    if ex & WS_EX_TOOLWINDOW.0 as isize != 0 || (ex & WS_EX_TRANSPARENT.0 as isize != 0 && ex & WS_EX_LAYERED.0 as isize != 0) {
        return false;
    }
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
    let mut cloaked = 0u32;
    let _ = DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut core::ffi::c_void, 4);
    if cloaked != 0 {
        return false;
    }
    let mut buf = [0u16; 64];
    let n = GetClassNameW(h, &mut buf) as usize;
    let class = String::from_utf16_lossy(&buf[..n.min(buf.len())]);
    if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
        return false;
    }
    let mut wr = RECT::default();
    if DwmGetWindowAttribute(h, DWMWA_EXTENDED_FRAME_BOUNDS, &mut wr as *mut RECT as *mut core::ffi::c_void, std::mem::size_of::<RECT>() as u32).is_err() {
        let _ = GetWindowRect(h, &mut wr);
    }
    // Small popups (menus, tooltips) don't count.
    if wr.right - wr.left < 80 || wr.bottom - wr.top < 40 {
        return false;
    }
    under_rect(wr, area)
}}

/// Two screen rectangles overlap.
pub(crate) fn under_rect(w: RECT, area: RECT) -> bool {
    w.left < area.right && w.right > area.left && w.top < area.bottom && w.bottom > area.top
}

fn shift(r: Rect, dx: f32) -> Rect {
    Rect::new(r.x + dx, r.y, r.w, r.h)
}

/// Gaze offset toward `mouse` from (cx, cy); full strength beyond `range` DIPs.
fn gaze(cx: f32, cy: f32, mouse: (f32, f32), range: f32) -> (f32, f32) {
    if mouse.0 < -1e3 {
        return (0.0, 0.0);
    }
    let (dx, dy) = (mouse.0 - cx, mouse.1 - cy);
    let d = (dx * dx + dy * dy).sqrt().max(1.0);
    let k = (d / range).min(1.0);
    (dx / d * 2.5 * k, dy / d * 1.5 * k)
}

fn run_deferred(list: Vec<Deferred>) {
    for d in list {
        match d {
            Deferred::TrayMenu => tray::show_menu(),
            Deferred::Quit => {
                if let Some(h) = with_app(|a| a.hwnd) {
                    unsafe {
                        let _ = DestroyWindow(h);
                    }
                }
            }
            Deferred::Export => {
                let Some(h) = with_app(|a| a.hwnd) else { continue };
                if let Some(path) = crate::features::save_dialog(h) {
                    let ok = std::fs::write(&path, crate::features::bundle()).is_ok();
                    with_app(|a| {
                        let name = std::path::Path::new(&path).file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
                        let t = if ok {
                            Toast::new("export", glyph::CHECK, col::GREEN, "Settings exported", &name)
                        } else {
                            Toast::new("export", glyph::SETTINGS, col::RED, "Couldn't save the file", &name)
                        };
                        a.push_toast(t.secs(4.0));
                        a.request_frame();
                    });
                }
            }
            Deferred::Import => {
                let Some(h) = with_app(|a| a.hwnd) else { continue };
                if let Some(path) = crate::features::open_dialog(h) {
                    let files = std::fs::read(&path).ok().and_then(|b| crate::features::unbundle(&String::from_utf8_lossy(&b)));
                    with_app(|a| {
                        match files {
                            Some(f) => {
                                a.import_pending = Some(f);
                                a.open_settings_at(Some(SEC_GENERAL));
                            }
                            None => a.push_toast(Toast::new("import", glyph::SETTINGS, col::RED, "Not a Nookly settings file", "Pick a file saved with Export").secs(5.0)),
                        }
                        a.request_frame();
                    });
                }
            }
            Deferred::PickIcs => {
                let Some(h) = with_app(|a| a.hwnd) else { continue };
                if let Some(path) = crate::features::pick_ics(h) {
                    with_app(|a| {
                        if let Some(i) = a.modules.iter().position(|m| m.title() == "Calendar") {
                            a.call(i, |m, c| m.picked(vec![path], c));
                            a.settings.save();
                        }
                        a.request_frame();
                    });
                }
            }
            Deferred::PickToPin(folder) => {
                let Some(h) = with_app(|a| a.hwnd) else { continue };
                let paths = crate::features::pick_paths(h, folder);
                if !paths.is_empty() {
                    with_app(|a| {
                        if let Some(i) = a.modules.iter().position(|m| m.title() == "Launcher") {
                            a.call(i, |m, c| m.picked(paths, c));
                            a.tab = i;
                            a.expanded = true;
                        }
                        a.request_frame();
                    });
                }
            }
            Deferred::Share(paths) => {
                if let Some(h) = with_app(|a| a.hwnd) {
                    if !crate::share::share(h, paths) {
                        with_app(|a| a.push_toast(Toast::new("share", "\u{E72D}", col::RED, "Couldn't open Share", "Windows' Share window isn't available")));
                    }
                }
            }
            Deferred::Restart => {
                if let Ok(exe) = std::env::current_exe() {
                    release_single_instance();
                    let _ = std::process::Command::new(exe).spawn();
                }
                if let Some(h) = with_app(|a| a.hwnd) {
                    unsafe {
                        let _ = DestroyWindow(h);
                    }
                }
            }
            Deferred::DragOut(paths) => {
                with_app(|a| a.dragging_out = true);
                dragdrop::drag_out(&paths);
                with_app(|a| {
                    a.dragging_out = false;
                    a.request_frame();
                });
            }
        }
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_COPYDATA {
        if let Some(r) = crate::modules::agents::on_copydata(hwnd, lp) {
            return r;
        }
    }
    if msg == WM_DESTROY {
        tray::remove(hwnd);
        unsafe { PostQuitMessage(0) };
        return LRESULT(0);
    }
    let res = with_app(|a| {
        let r = a.handle(msg, wp, lp);
        (r, std::mem::take(&mut a.deferred))
    });
    if let Some((r, deferred)) = res {
        run_deferred(deferred);
        run_open_later();
        if let Some(r) = r {
            return r;
        }
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}
