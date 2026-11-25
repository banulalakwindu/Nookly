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
/// The self-test and the docs renders (not in the released program).
#[cfg(feature = "selftest")]
mod selftest;
#[cfg(feature = "selftest")]
pub use selftest::run_tests;
use settings::*;

pub const WM_APP_WAKE: u32 = WM_APP + 1;
pub const WM_APP_TRAY: u32 = WM_APP + 2;
/// Just draw (a new camera picture): nothing else to look at.
pub const WM_APP_FRAME: u32 = WM_APP + 3;
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
    ("\u{ECA5}", "Live & status", "What the closed notch shows"),
    (glyph::BELL, "Alerts", "Sounds and notifications"),
    ("\u{E765}", "Shortcuts", "Keys and the mouse"),
    ("\u{E7C4}", "Taskbar", "Apps and widgets"),
];
const TOUR_Q_SETTINGS: &str = "Click the gear, right-click the notch or click the tray icon for Settings:";

/// Hit id of a toast's body (clicking the toast itself, not a button).
const TOAST_BODY: u32 = 999;

const TIMER_TICK: usize = 1;
/// A widget's full view open on the taskbar: drawn again this often.
const TIMER_FULL: usize = 10;
/// A taskbar face that moves (music bars while a song plays).
const TIMER_FACES: usize = 11;
const TIMER_WAKE: usize = 14;
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
const GLASS_IDLE_MS: u32 = 600;
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

/// The mouse rests this long on something before its tooltip shows.
const TIP_DELAY: f64 = 0.6;

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
    crate::time::wait_refresh();
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

/// A widget's full view, open in its bubble on the taskbar.
/// Where a widget's full view comes from.
#[derive(Clone, Copy, PartialEq)]
enum FullSrc {
    /// A notch panel about the same thing.
    Panel,
    /// The feature's own view for that widget (Module::draw_full).
    Module,
    /// Drawn here (fullviews.rs).
    BuiltIn,
}

struct FullView {
    id: String,
    src: FullSrc,
    module: usize,
    mouse: (f32, f32),
    down: Option<Hit>,
    hits: Vec<Hit>,
    face: Option<crate::surfaces::Face>,
    gfx: Option<Gfx>,
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
    /// The keyboard is for a widget's view on the taskbar; a call from there
    /// is running.
    bar_keyboard: bool,
    full_call: bool,
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
    /// The mascot as last drawn (see mascot::key).
    mascot_drawn: Option<[i32; 3]>,
    /// The frame was asked for by the once-a-second tick alone.
    tick_frame: bool,
    /// Where the pointer was at the last frame, and when it last moved.
    ptr: (i32, i32),
    ptr_at: f64,
    /// What the closed notch showed when it was last drawn, and how many
    /// ticks' frames in a row looked the same.
    closed_drawn: Option<String>,
    closed_same: u32,
    /// What the last frame asked about the mouse (see ui::note_hovers).
    hovers: Vec<Rect>,
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
    /// When a wake last ticked.
    woke_at: f64,
    glass_probe_ms: u32,
    /// The next grab must cover the whole window (the notch is opening).
    glass_full: bool,
    /// Only the notch's own part was grabbed since the last whole grab (the
    /// rest may be old), and whether the notch was at rest last frame.
    glass_patched: Option<Rect>,
    glass_was_rest: bool,
    excluded: bool,
    /// Hit under the mouse with a tooltip: (module, id, since, suppressed by a click).
    tip: Option<(usize, u32, f64, bool)>,
    pub deferred: Vec<Deferred>,
    /// The mascot on the Windows taskbar (Settings > Mascot > Taskbar).
    pet: Option<crate::taskbar::Pet>,
    /// The taskbar add-on's process while it runs (Settings > Taskbar), how
    /// often it has quit unexpectedly this session, and when to try again.
    bar: Option<std::process::Child>,
    bar_fails: u32,
    bar_retry: f64,
    /// The widget feed shared with the taskbar, and the module behind each
    /// of its widgets (to open its panel when the taskbar asks).
    feed: Option<crate::feed::Writer>,
    feed_mods: Vec<usize>,
    /// The taskbar widgets' pictures (see surfaces.rs): the shared block,
    /// what each face showed when last drawn, the surface they're drawn on,
    /// and whether Liquid Glass is light behind the taskbar (it says so).
    surf: Option<crate::surfaces::Writer>,
    surf_keys: Vec<(String, String)>,
    /// Each face as last drawn, with its key (only faces that changed are
    /// drawn again: a clock with seconds doesn't redraw the rest).
    surf_cache: std::collections::HashMap<String, (String, crate::surfaces::Face)>,
    /// The widget faces last written (the full view is written with them).
    surf_faces: Vec<crate::surfaces::Face>,
    /// The faces are drawn ten times a second (one of them moves).
    faces_fast: bool,
    /// The taskbar add-on is being brought to this version.
    bar_updating: bool,
    /// The add-on update failed this session (it's tried at the next start).
    bar_update_tried: bool,
    /// The add-on is of this version (looked up once).
    bar_ver_ok: bool,
    /// What the taskbar's layout uses: the layout, and for each feature
    /// whether a widget on it comes from there.
    bar_mods: std::cell::RefCell<(String, Vec<bool>)>,
    /// The taskbar was wanted when the shortcuts were last taken.
    bar_keyed: bool,
    /// The taskbar is out (not slid away): moving faces move only then.
    bar_out: bool,
    /// The widgets in sight on the taskbar (a bit per place).
    bar_seen: u64,
    /// A widget's bubble open on the taskbar: its full view, drawn here.
    full: Option<FullView>,
    surf_gfx: Option<Gfx>,
    surf_light: bool,
    /// The scale the taskbar draws at (its DPI times Settings > Size);
    /// 0 until it says.
    surf_scale: f32,
    face_info: crate::faces::Info,
    /// Screen x of the taskbar mascot's centre while the notch lives above
    /// it (taskbar mode), else None.
    dock_x: Option<i32>,
    /// The ⋯ page (panels that don't fit in the tab bar) is open.
    more_open: bool,
    /// Recording a keyboard shortcut (index into features::HOTKEYS).
    recording: Option<usize>,
    /// Shortcuts Windows refused (another app has them).
    hotkey_fail: [bool; 5],
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
            // Quitting Nookly takes its taskbar with it.
            if let Some(app) = g.as_mut() {
                app.stop_bar();
            }
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

/// Have a frame drawn, from a background thread.
pub fn wake_frame() {
    let h = MAIN_HWND.load(std::sync::atomic::Ordering::Relaxed);
    if h != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(h as *mut _)), WM_APP_FRAME, WPARAM(0), LPARAM(0));
        }
    }
}

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
            crate::shell::keep_in_peek(hwnd);

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
                bar_keyboard: false,
                full_call: false,
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
                mascot_drawn: None,
                tick_frame: false,
                ptr: (0, 0),
                ptr_at: 0.0,
                closed_drawn: None,
                closed_same: 0,
                hovers: Vec::new(),
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
                glass_probe_ms: 0,
                woke_at: -1.0,
                glass_full: false,
                glass_patched: None,
                glass_was_rest: false,
                excluded: false,
                tip: None,
                deferred: Vec::new(),
                pet: None,
                bar: None,
                bar_fails: 0,
                bar_retry: 0.0,
                feed: None,
                feed_mods: Vec::new(),
                surf: None,
                surf_keys: Vec::new(),
                surf_cache: std::collections::HashMap::new(),
                surf_faces: Vec::new(),
                faces_fast: false,
                bar_updating: false,
                bar_update_tried: false,
                bar_ver_ok: false,
                bar_mods: Default::default(),
                bar_keyed: false,
                bar_out: true,
                bar_seen: u64::MAX,
                full: None,
                surf_gfx: None,
                surf_light: false,
                surf_scale: 0.0,
                face_info: Default::default(),
                dock_x: None,
                more_open: false,
                recording: None,
                hotkey_fail: [false; 5],
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
            let taskbar = crate::shell::tray();
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
            // In a widget's view on the taskbar: typed there, the notch
            // stays closed.
            Some(true) if self.full_call => self.grab_keyboard_for_bar(i),
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
        // (Its features can run for the taskbar with the notch off: they
        // don't pile up alerts for when it's on again.)
        if !self.notch_on() {
            return;
        }
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

    fn grab_keyboard_for_bar(&mut self, i: usize) {
        let open = self.expanded;
        self.grab_keyboard(i);
        self.expanded = open;
        self.bar_keyboard = true;
    }

    fn release_keyboard(&mut self) {
        self.bar_keyboard = false;
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
        // With the notch switched off (Settings > Notch) nothing runs but
        // what the taskbar shows.
        let on = self.notch_on();
        for i in 0..self.modules.len() {
            if if on { self.runs(i) } else { self.bar_uses(i) } {
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
            self.gfx.forget_text();
            trim_memory();
        }
        crate::time::H12.store(self.settings.clock12, std::sync::atomic::Ordering::Relaxed);
        crate::time::SECONDS.store(self.settings.seconds, std::sync::atomic::Ordering::Relaxed);
        self.update_make_room();
        self.update_mood();
        self.sync_pet();
        self.sync_bar();
        self.push_feed();
        self.push_faces();
        self.sync_fast();
        // Once a second, whatever happened (a frame that would look the same
        // isn't drawn: see frame()).
        let asked = self.frame_interval == VSYNC;
        self.request_frame();
        self.tick_frame = !asked;
    }

    // ---------------------------------------------------- taskbar add-on

    /// The taskbar add-on is installed (Settings > Taskbar > Download) and
    /// switched on.
    pub fn bar_wanted(&self) -> bool {
        self.settings.extra.get("taskbar_on").is_some_and(|v| v == "1") && bar_exe().is_some() && !crate::modules::agents::ISOLATED.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Start, watch or stop the taskbar add-on to match the setting. If it
    /// quits unexpectedly the Windows taskbar comes back at once, and it's
    /// started again after a pause (2, 4, 8 s; then it stays off until the
    /// switch is turned off and on).
    pub fn sync_bar(&mut self) {
        // A test run: the taskbar on screen belongs to the real Nookly.
        if crate::modules::agents::ISOLATED.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let want = self.bar_wanted();
        // Its shortcut is taken only while it's on.
        if want != self.bar_keyed {
            self.bar_keyed = want;
            self.sync_hotkeys();
        }
        let t = self.t();
        if let Some(c) = self.bar.as_mut() {
            if let Ok(Some(_)) = c.try_wait() {
                self.bar = None;
                crate::shell::restore_windows_taskbar();
                if want {
                    self.bar_fails += 1;
                    self.bar_retry = t + 2f64.powi(self.bar_fails as i32);
                }
            }
        }
        // A taskbar already running (started before Nookly restarted, say)
        // is kept, not started twice.
        // An add-on from another version (Nookly updated): the matching one
        // first (the old one keeps working until it's here).
        // (The add-on writes its version when it starts; one from before
        // that, or another version, is replaced.)
        if !self.bar_ver_ok && bar_exe().is_some() {
            self.bar_ver_ok = crate::store::read("taskbar_version.txt").trim() == env!("CARGO_PKG_VERSION");
        }
        if !self.bar_ver_ok && bar_exe().is_some() && !self.bar_updating && !self.bar_update_tried && !cfg!(debug_assertions) {
            self.bar_updating = true;
            if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
                crate::modules::update::get_addon(dir.join("nookly-taskbar.exe"), || {});
            }
        }
        if self.bar_updating && *crate::modules::update::ADDON.lock().unwrap() != crate::modules::update::AddOn::Busy {
            let ok = *crate::modules::update::ADDON.lock().unwrap() == crate::modules::update::AddOn::Idle;
            self.bar_updating = false;
            self.bar_update_tried = true;
            if ok {
                // Recorded only once it's really here (a failed download is
                // tried again at the next start; the old one works meanwhile).
                self.settings.extra.insert("bar_version".into(), env!("CARGO_PKG_VERSION").into());
                self.settings.save();
                if self.bar.is_some() || bar_window().is_some() {
                    // The new one takes over.
                    self.stop_bar();
                }
            }
        }
        let running = self.bar.is_some() || bar_window().is_some();
        let busy = *crate::modules::update::ADDON.lock().unwrap() == crate::modules::update::AddOn::Busy;
        if want && !running && !busy && self.bar_fails <= 3 && t >= self.bar_retry {
            if let Some(exe) = bar_exe() {
                self.bar = std::process::Command::new(exe).arg("--from-notch").spawn().ok();
                self.place();
            }
        } else if !want {
            self.bar_fails = 0;
            if running {
                self.stop_bar();
                self.place();
            } else if crate::shell::hidden_by_us() && bar_window().is_none() {
                // Left hidden by a taskbar that's gone (a crash, a restart).
                crate::shell::restore_windows_taskbar();
            }
        }
    }

    /// Close the taskbar add-on and put the Windows taskbar back.
    pub fn stop_bar(&mut self) {
        // A test run: the taskbar on screen belongs to the real Nookly.
        if crate::modules::agents::ISOLATED.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let child = self.bar.take();
        if let Some(h) = bar_window() {
            unsafe {
                let _ = PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            for _ in 0..30 {
                if bar_window().is_none() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
        if let Some(mut c) = child {
            if !matches!(c.try_wait(), Ok(Some(_))) {
                let _ = c.kill();
                let _ = c.wait();
            }
        }
        crate::shell::restore_windows_taskbar();
    }

    /// Settings > Taskbar > Download taskbar. (The download itself comes
    /// with the release flow; until then a copy built next to nookly.exe is
    /// what counts as installed.)
    /// Settings > Taskbar > Download taskbar: get the add-on of this
    /// version, install it beside Nookly and switch it on.
    pub fn get_bar(&mut self) {
        let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) else { return };
        crate::modules::update::get_addon(dir.join("nookly-taskbar.exe"), || {});
        self.settings.extra.insert("taskbar_on".into(), "1".into());
        self.settings.extra.insert("bar_version".into(), env!("CARGO_PKG_VERSION").into());
    }

    /// Settings > Taskbar > Remove taskbar: off, Windows' taskbar back, and
    /// the add-on deleted.
    pub fn remove_bar(&mut self) {
        self.settings.extra.insert("taskbar_on".into(), "0".into());
        self.stop_bar();
        if let Some(exe) = bar_exe() {
            let _ = std::fs::remove_file(exe);
        }
        forget_bar_exe();
        self.place();
    }

    /// Share every widget's value with the running taskbar (it shows the
    /// ones picked in Settings > Taskbar > Widgets). Only while it runs, and
    /// it's told only when something changed.
    pub fn push_feed(&mut self) {
        let Some(bar) = bar_window() else {
            self.feed = None;
            return;
        };
        let (_, widgets) = self.notch_items();
        self.feed_mods = widgets.iter().map(|(_, m)| *m).collect();
        let items: Vec<crate::feed::Item> = widgets
            .into_iter()
            .map(|(w, _)| crate::feed::Item { id: w.id.to_string(), name: w.name.to_string(), icon: w.icon.to_string(), color: w.color, value: w.value, short: w.short, ring: w.ring, quiet: w.quiet })
            .collect();
        if self.feed.is_none() {
            self.feed = crate::feed::Writer::open();
        }
        let changed = self.feed.as_mut().is_some_and(|f| f.write(&crate::feed::encode(&items)));
        if changed {
            unsafe {
                let msg = RegisterWindowMessageW(crate::feed::FEED_MSG);
                let _ = PostMessageW(Some(bar), msg, WPARAM(0), LPARAM(0));
            }
        }
    }

    /// A face that moves (music bars) is drawn eight times a second, while
    /// the taskbar is out.
    fn sync_fast(&mut self) {
        let fast = bar_window().is_some()
            && self.bar_out
            && crate::barcfg::placed(&self.settings).iter().enumerate().any(|(k, p)| {
                k < 64 && self.bar_seen & (1 << k) != 0 && (0..self.modules.len()).any(|i| self.runs(i) && self.modules[i].faces().contains(&p.id.as_str()) && self.modules[i].face_animates(&p.id))
            });
        if fast != self.faces_fast {
            self.faces_fast = fast;
            unsafe {
                if fast {
                    SetTimer(Some(self.hwnd), TIMER_FACES, 125, None);
                } else {
                    let _ = KillTimer(Some(self.hwnd), TIMER_FACES);
                }
            }
        }
    }

    /// Draw the taskbar's widgets (see faces.rs) into the shared block and
    /// tell the taskbar, when one of them shows something new: each says
    /// what it shows with a key, so at rest nothing is drawn.
    pub fn push_faces(&mut self) {
        let Some(bar) = bar_window() else {
            self.surf = None;
            self.surf_gfx = None;
            self.surf_keys.clear();
            self.surf_cache.clear();
            return;
        };
        // Slid away: nobody sees them (they're drawn when it comes out).
        if !self.bar_out && self.surf.is_some() && !self.surf_keys.is_empty() {
            return;
        }
        let want = crate::barcfg::faces(&self.settings);
        if want.is_empty() && self.surf_keys.is_empty() && self.surf.is_some() {
            return;
        }
        let scale = if self.surf_scale > 0.0 { self.surf_scale } else { unsafe { GetDpiForWindow(bar) }.max(96) as f32 / 96.0 };
        let theme = pick_theme(&self.settings, &crate::barcfg::theme_key(&self.settings), self.surf_light);
        self.face_info.refresh(crate::time::unix_now());
        self.face_info.settings(&self.settings);
        let values = self.bar_values();
        self.face_info.quick = ["wifi", "volume", "battery"].iter().filter_map(|id| values.iter().find(|(w, _)| w.id == *id).map(|(w, _)| w.clone())).collect();
        enum Src {
            BuiltIn,
            Module(usize),
            Value(Widget),
            Empty,
        }
        let mut jobs: Vec<(crate::barcfg::Placed, Src)> = Vec::new();
        let mut keys: Vec<(String, String)> = Vec::new();
        for p in want {
            let src = if crate::faces::BUILT_IN.contains(&p.id.as_str()) {
                Src::BuiltIn
            } else if let Some(m) = (0..self.modules.len()).find(|&i| self.runs(i) && self.modules[i].faces().contains(&p.id.as_str())) {
                Src::Module(m)
            } else if let Some((w, _)) = values.iter().find(|(w, _)| w.id == p.id) {
                Src::Value(w.clone())
            } else {
                // Nothing to show yet (no countdown set, no task...): the
                // widget still holds its place, saying so.
                Src::Empty
            };
            let k = match &src {
                Src::BuiltIn => crate::faces::key(&p.id, p.size, &self.face_info),
                Src::Module(m) => self.modules[*m].face_key(&p.id, p.size),
                Src::Value(w) => format!("{w:?}"),
                Src::Empty => "empty".to_string(),
            };
            keys.push((p.face(), format!("{k}|{scale}|{}|{}", theme.key, self.surf_light)));
            jobs.push((p, src));
        }
        if keys == self.surf_keys && self.surf.is_some() {
            return;
        }
        if self.surf.is_none() {
            self.surf = crate::surfaces::Writer::open();
        }
        let Some(mut g) = self.surf_gfx.take().or_else(|| Gfx::small(1, 1).ok()) else { return };
        let mut faces = Vec::new();
        self.surf_cache.retain(|name, _| keys.iter().any(|(n, _)| n == name));
        for (i, (p, src)) in jobs.iter().enumerate() {
            if let Some((ck, f)) = self.surf_cache.get(&keys[i].0) {
                if *ck == keys[i].1 {
                    faces.push(f.clone());
                    continue;
                }
            }
            let w = p.width();
            let (pw, ph) = ((w * scale).round() as i32, (crate::surfaces::FACE_H * scale).round() as i32);
            if g.resize(pw, ph).is_err() {
                continue;
            }
            g.theme = theme;
            g.begin(scale);
            let mut hits = Vec::new();
            {
                let module = if let Src::Module(m) = src { *m } else { 0 };
                let mut ui = Ui { g: &mut g, hits: &mut hits, module, mouse: (-1e4, -1e4), t: 0.0 };
                let r = Rect::new(0.0, 0.0, pw as f32 / scale, ph as f32 / scale);
                match src {
                    Src::BuiltIn => crate::faces::draw(&mut ui, r, &p.id, p.size, &self.face_info),
                    Src::Module(m) => self.modules[*m].draw_face(&p.id, p.size, &mut ui, r),
                    Src::Value(v) => crate::faces::draw_value(&mut ui, r, v, p.size),
                    Src::Empty => crate::faces::draw_empty(&mut ui, r, &p.id, p.size),
                }
            }
            if g.end() {
                let f = crate::surfaces::Face { id: p.face(), w: pw as u32, h: ph as u32, dip_w: pw as f32 / scale, px: g.pixels() };
                self.surf_cache.insert(keys[i].0.clone(), (keys[i].1.clone(), f.clone()));
                faces.push(f);
            }
        }
        self.surf_gfx = Some(g);
        self.surf_faces = faces;
        if self.write_surfaces() {
            self.surf_keys = keys;
        }
    }

    /// Write the faces (and an open full view) and tell the taskbar.
    fn write_surfaces(&mut self) -> bool {
        let Some(bar) = bar_window() else { return false };
        if self.surf.is_none() {
            self.surf = crate::surfaces::Writer::open();
        }
        let mut all = self.surf_faces.clone();
        if let Some(f) = self.full.as_ref().and_then(|f| f.face.clone()) {
            all.push(f);
        }
        if self.surf.as_mut().is_some_and(|s| s.write(&all)) {
            unsafe {
                let msg = RegisterWindowMessageW(crate::surfaces::CHANGED_MSG);
                let _ = PostMessageW(Some(bar), msg, WPARAM(0), LPARAM(0));
            }
            true
        } else {
            false
        }
    }

    /// A widget's bubble opened on the taskbar (`k`: its place in the
    /// catalog + 1) or closed (0): draw its full view while it's open.
    fn open_full(&mut self, k: usize) {
        let want = k.checked_sub(1).and_then(|k| crate::barcfg::CATALOG.get(k)).and_then(|d| match crate::barcfg::full_for(d.id)? {
            crate::barcfg::Full::Panel(title) => self.modules.iter().position(|m| m.title() == title).map(|m| (d.id.to_string(), FullSrc::Panel, m)),
            crate::barcfg::Full::Own => match self.modules.iter().position(|m| m.full_size(d.id).is_some()) {
                Some(m) => Some((d.id.to_string(), FullSrc::Module, m)),
                None => crate::fullviews::size(d.id).map(|_| (d.id.to_string(), FullSrc::BuiltIn, 0)),
            },
        });
        match want {
            Some((id, src, module)) => {
                if self.full.as_ref().is_some_and(|f| f.id == id) {
                    return;
                }
                if self.bar_keyboard {
                    self.on_blur();
                }
                self.full = Some(FullView { id, src, module, mouse: (-1e4, -1e4), down: None, hits: Vec::new(), face: None, gfx: None });
                // It runs now (even with its notch panel hidden).
                if src != FullSrc::BuiltIn {
                    self.call(module, |m, c| m.tick(c));
                }
                unsafe {
                    SetTimer(Some(self.hwnd), TIMER_FULL, 50, None);
                }
                self.push_full();
            }
            None => {
                if self.bar_keyboard {
                    self.on_blur();
                }
                if self.full.take().is_some() {
                    unsafe {
                        let _ = KillTimer(Some(self.hwnd), TIMER_FULL);
                    }
                    self.write_surfaces();
                }
            }
        }
    }

    /// Draw the open full view (when it looks different, write it).
    fn push_full(&mut self) {
        let Some(mut f) = self.full.take() else { return };
        let Some(bar) = bar_window() else {
            self.full = Some(f);
            return;
        };
        let scale = if self.surf_scale > 0.0 { self.surf_scale } else { unsafe { GetDpiForWindow(bar) }.max(96) as f32 / 96.0 };
        let theme = pick_theme(&self.settings, &crate::barcfg::theme_key(&self.settings), self.surf_light);
        let (w, h) = match f.src {
            FullSrc::Panel => self.modules[f.module].size(),
            FullSrc::Module => self.modules[f.module].full_size(&f.id).unwrap_or((360.0, 180.0)),
            FullSrc::BuiltIn => crate::fullviews::size(&f.id).unwrap_or((360.0, 180.0)),
        };
        let values = if f.src == FullSrc::BuiltIn {
            self.face_info.refresh(crate::time::unix_now());
            self.bar_values().into_iter().map(|x| x.0).collect()
        } else {
            Vec::new()
        };
        let (pw, ph) = ((w * scale).round() as i32, (h * scale).round() as i32);
        let Some(mut g) = f.gfx.take().or_else(|| Gfx::new(pw.max(1), ph.max(1)).ok()) else {
            self.full = Some(f);
            return;
        };
        if g.pw != pw || g.ph != ph {
            let _ = g.resize(pw, ph);
        }
        g.theme = theme;
        g.begin(scale);
        let mut hits = Vec::new();
        {
            let t = self.t();
            let mut ui = Ui { g: &mut g, hits: &mut hits, module: f.module, mouse: f.mouse, t };
            let r = Rect::new(0.0, 0.0, pw as f32 / scale, ph as f32 / scale);
            match f.src {
                FullSrc::Panel => self.modules[f.module].draw(&mut ui, r),
                FullSrc::Module => self.modules[f.module].draw_full(&f.id, &mut ui, r),
                FullSrc::BuiltIn => crate::fullviews::draw(&mut ui, r, &f.id, &self.face_info, &values, &self.settings),
            }
        }
        let mut changed = false;
        if g.end() {
            let face = crate::surfaces::Face { id: crate::surfaces::full_face(&f.id), w: pw as u32, h: ph as u32, dip_w: pw as f32 / scale, px: g.pixels() };
            if f.face.as_ref() != Some(&face) {
                f.face = Some(face);
                changed = true;
            }
        }
        f.hits = hits;
        f.gfx = Some(g);
        self.full = Some(f);
        if changed {
            self.write_surfaces();
        }
    }

    /// The mouse inside the open full view on the taskbar.
    fn full_input(&mut self, wp: usize, lp: isize) {
        let what = wp & 0xFF;
        let x = (lp & 0xFFFF) as i16 as f32 / 4.0;
        let y = ((lp >> 16) & 0xFFFF) as i16 as f32 / 4.0;
        let Some(f) = self.full.as_mut() else { return };
        let m = f.module;
        let (src, id) = (f.src, f.id.clone());
        match what {
            crate::surfaces::INPUT_MOVE => f.mouse = (x, y),
            crate::surfaces::INPUT_LEAVE => {
                f.mouse = (-1e4, -1e4);
                f.down = None;
            }
            crate::surfaces::INPUT_DOWN => {
                f.mouse = (x, y);
                f.down = f.hits.iter().rev().find(|h| h.rect.contains(x, y)).copied();
            }
            crate::surfaces::INPUT_UP => {
                f.mouse = (x, y);
                if let Some(h) = f.down.take().filter(|h| h.rect.contains(x, y)) {
                    let rel = ((x - h.rect.x) / h.rect.w.max(1.0), (y - h.rect.y) / h.rect.h.max(1.0));
                    self.full_call = true;
                    match src {
                        FullSrc::Panel => self.call(m, |md, c| md.action(h.id, rel, c)),
                        FullSrc::Module => self.call(m, |md, c| md.full_action(&id, h.id, c)),
                        FullSrc::BuiltIn => {
                            match crate::fullviews::action(&id, h.id, &mut self.settings) {
                                crate::fullviews::Act::Saved => {
                                    self.settings.save();
                                    self.face_info.settings(&self.settings);
                                    self.surf_keys.clear();
                                    self.push_faces();
                                }
                                crate::fullviews::Act::Panel(title) => {
                                    if let Some(i) = self.modules.iter().position(|m| m.title() == title) {
                                        if self.tab_list().contains(&i) {
                                            self.tab = i;
                                            self.expanded = true;
                                            self.request_frame();
                                        }
                                    }
                                }
                                crate::fullviews::Act::None => {}
                            }
                            self.face_info.stale();
                        }
                    }
                    self.full_call = false;
                }
            }
            crate::surfaces::INPUT_WHEEL => {
                let d = ((wp >> 16) & 0xFFFF) as i16 as f32 / 120.0;
                if src == FullSrc::Panel {
                    self.modules[m].scroll(d);
                }
            }
            _ => {}
        }
        self.push_full();
    }

    /// Features a taskbar widget can't come from at all (not in this build):
    /// their names. A feature switched off for the notch still works for
    /// the taskbar, so that's never a reason.
    pub fn bar_features_off(&self) -> Vec<&'static str> {
        let mut off = Vec::new();
        for d in crate::barcfg::CATALOG {
            if !d.feature.is_empty() && !off.contains(&d.feature) && !self.modules.iter().any(|m| m.title() == d.feature) {
                off.push(d.feature);
            }
        }
        off
    }

    /// The taskbar's widget was clicked: open its feature's panel.
    fn open_from_bar(&mut self, index: usize) {
        if let Some(&m) = self.feed_mods.get(index) {
            if m < self.modules.len() && self.modules[m].has_tab() {
                self.call(m, |_, c| c.open_my_tab());
                self.raise();
                self.request_frame();
            }
        }
    }

    /// Tell the running taskbar to read the settings again.
    pub fn bar_notify(&self) {
        if let Some(h) = bar_window() {
            unsafe {
                let msg = RegisterWindowMessageW(w!("Nookly.SettingsChanged"));
                let _ = PostMessageW(Some(h), msg, WPARAM(0), LPARAM(0));
            }
        }
    }

    // ---------------------------------------------------- taskbar mascot

    fn pet_wanted(&self) -> bool {
        self.settings.extra.get("taskbar_pet").map_or(false, |v| v == "1") && self.notch_on() && !self.bar_wanted() && !crate::modules::agents::ISOLATED.load(std::sync::atomic::Ordering::Relaxed)
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
        } else if self.settings.edge == Edge::Bottom && self.bar_wanted() {
            // The Nookly taskbar has the bottom edge.
            Edge::Top
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

    /// A taskbar widget on the bar comes from this feature (so it runs even
    /// with its notch panel hidden: the notch and the taskbar are separate
    /// places, each with its own on and off).
    fn bar_uses(&self, i: usize) -> bool {
        if !self.bar_wanted() {
            return false;
        }
        if self.full.as_ref().is_some_and(|f| f.module == i) {
            return true;
        }
        // Asked for every feature every second, and the layout rarely
        // changes: worked out once for each layout.
        let layout = self.settings.extra.get("bar_layout").map_or("", String::as_str);
        let mut c = self.bar_mods.borrow_mut();
        if c.0 != layout || layout.is_empty() || c.1.len() != self.modules.len() {
            let placed = crate::barcfg::placed(&self.settings);
            let used = |title: &str| placed.iter().any(|p| crate::barcfg::def(&p.id).is_some_and(|d| d.feature == title) || crate::barcfg::panel_for(&p.id) == Some(title));
            *c = (layout.to_string(), self.modules.iter().map(|m| used(m.title())).collect());
        }
        c.1[i]
    }

    /// The feature does its work: its notch panel is on, or the taskbar
    /// shows one of its widgets. (Its alerts and opening its panel stay
    /// with the notch panel.)
    fn runs(&self, i: usize) -> bool {
        self.enabled(i) || self.bar_uses(i)
    }

    /// The status values of every feature that runs (for the taskbar's
    /// widgets), with the module each comes from.
    fn bar_values(&mut self) -> Vec<(Widget, usize)> {
        let mut out = Vec::new();
        for i in 0..self.modules.len() {
            if self.runs(i) {
                out.extend(self.modules[i].widgets().into_iter().map(|w| (w, i)));
            }
        }
        out
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
        let th = crate::gfx::pick_theme(&self.settings, &self.settings.theme, self.glass_light);
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
    /// part (tiny: a quarter of the pixels each way).
    /// Refreshes at rest grab only the notch's own part (and a margin) and
    /// patch it into the whole grab: a fraction of the work, so it can keep
    /// up with a page scrolling behind.
    /// The glass covers the whole shape, the flares where it meets the
    /// screen edge included.
    fn update_backdrop(&mut self, notch: Rect, goal: Rect, t: f64, dist: f32, ear: f32, edge: Edge) {
        let flared = |n: Rect| if edge.vertical() { Rect::new(n.x, n.y - ear, n.w, n.h + ear * 2.0) } else { Rect::new(n.x - ear, n.y, n.w + ear * 2.0, n.h) };
        let notch = flared(notch);
        let goal = flared(goal);
        let (pw, ph) = (self.gfx.pw, self.gfx.ph);
        let (gw, gh) = ((pw / 4).max(2) as u32, (ph / 4).max(2) as u32);
        // The notch starts changing size (a panel switch) after only its own
        // part was refreshed, and the new shape reaches past that part: the
        // rest of the grab may be old, so grab the whole area once, or the
        // new shape's sides would show old colours. (Shrinking, as when it
        // closes, stays inside the fresh part: no grab, no stall.)
        let inside = |a: Rect, b: Rect| b.x >= a.x - 1.0 && b.y >= a.y - 1.0 && b.r() <= a.r() + 1.0 && b.b() <= a.b() + 1.0;
        let resized = dist > 0.0 && self.glass_was_rest && self.glass_patched.map_or(false, |p| !inside(p, goal));
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
                    self.glass_patched = Some(notch);
                }
            }
        } else if stale {
            self.glass_dirty = false;
            self.glass_full = false;
            self.glass_patched = None;
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
                let t0 = std::time::Instant::now();
                let g = crate::glass::capture_behind(self.hwnd, RECT { left: ox, top: oy, right: ox + pw, bottom: oy + ph });
                dlog(format!("glass grab: whole {pw}x{ph} px, {:.1} ms (dist {dist:.1})", t0.elapsed().as_secs_f32() * 1000.0));
                g
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
        let key = ((x0 as u64) << 48) ^ ((y0 as u64) << 36) ^ ((x1 as u64) << 24) ^ ((y1 as u64) << 12) ^ ((raw.serial as u64) << 56);
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
        crate::glass::tame(&mut px, self.glass_light);
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
                self.glass_probe_ms = GLASS_PROBE_MS;
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
        }
        // Nobody at the keyboard or the mouse: only something playing could
        // change it, so it looks less often.
        let ms = if fast { GLASS_LIVE_MS } else if Self::system_idle_secs() > 3.0 { GLASS_IDLE_MS } else { GLASS_PROBE_MS };
        if ms != self.glass_probe_ms {
            self.glass_probe_ms = ms;
            unsafe {
                SetTimer(Some(self.hwnd), TIMER_GLASS_PROBE, ms, None);
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

    /// Liquid Glass's light, as on the taskbar: a rim that glows from the
    /// very edge and fades inward, and a soft light from above. It's still,
    /// so it costs nothing at rest.
    fn draw_glass_light(&mut self, geo: &windows::Win32::Graphics::Direct2D::ID2D1PathGeometry, notch: Rect, rad: f32, ear: f32) {
        let light = self.glass_light;
        self.gfx.push_layer(Some(geo), 1.0);
        // It follows the outline, flares included, but not the side along
        // the screen edge, where the glass runs on past the edge.
        if let Some(outline) = self.gfx.notch_outline(notch, rad, ear, self.edge()) {
            let deep = if self.edge().vertical() { notch.w } else { notch.h };
            self.gfx.glass_rim(&outline, light, deep);
        }
        let top = Rect::new(notch.x - ear, notch.y, notch.w + ear * 2.0, notch.h * 0.3);
        self.gfx.glass_sheen(top, if light { 0.22 } else { 0.10 });
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
        self.tick_frame = false;
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
        if self.expanded || (self.keyboard.is_some() && !self.bar_keyboard) || self.drag_files || self.onboarding.is_some() {
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
        // Hidden (a game or a video fills the screen, the notch is off):
        // nothing to draw until it's back.
        if self.hidden && self.mode() != Mode::Expanded && !crate::modules::agents::ISOLATED.load(std::sync::atomic::Ordering::Relaxed) {
            self.set_frame_timer(0);
            return;
        }
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

        // A frame nothing asked for but the mascot: drawn only when it would
        // look different (it looks elsewhere, or blinks).
        let asked = matches!(self.frame_interval, 0 | VSYNC);
        let mut p = POINT::default();
        if unsafe { GetCursorPos(&mut p) }.is_ok() && (p.x, p.y) != self.ptr {
            self.ptr = (p.x, p.y);
            self.ptr_at = t;
        }
        let key = self.mascot_key(t);
        let tick = std::mem::take(&mut self.tick_frame);
        let shows = if tick && key.is_some() { self.closed_key() } else { None };
        let mut same = dist <= 0.0 && key.is_some() && key == self.mascot_drawn && (!asked || (tick && !self.glass_dirty && shows.is_some() && shows == self.closed_drawn));
        if asked && same {
            // (Drawn now and then in any case.)
            self.closed_same += 1;
            same = self.closed_same < 10;
        }
        if !same {
            self.render(target, dist);
            self.mascot_drawn = key;
            self.closed_drawn = shows;
            self.closed_same = 0;
        }

        // Tab switch: slide, highlight glide and glow all finish within 0.6 s.
        let moving = dist > 0.0 || t - self.switch_t < 0.6;
        self.perf_note(t, moving);
        let ms = if moving { VSYNC } else { self.ambient_ms(t) };
        self.set_frame_timer(ms);
    }

    /// With nothing in motion: how often to draw (milliseconds, 0: not until
    /// something happens). Only what can be seen counts, and only as often
    /// as it changes: every frame costs the GPU, and the CPU with it.
    /// (Whatever else changes asks for its frame, and there's one a second
    /// in any case.)
    fn ambient_ms(&self, t: f64) -> u32 {
        let mut ms = 0u32;
        let mut want = |m: u32| ms = if ms == 0 { m } else { ms.min(m) };
        let until = |at: f64, most: u32| (((at - t) * 1000.0).ceil().max(8.0) as u32).min(most);
        match self.mode() {
            Mode::Expanded => {
                if self.onboarding.is_some() {
                    want(33);
                }
                for i in [Some(self.tab).filter(|_| !self.more_open), self.keyboard].into_iter().flatten() {
                    if i < self.modules.len() && self.enabled(i) && self.modules[i].animating() {
                        want(self.modules[i].frame_ms());
                    }
                }
            }
            Mode::Toast => want(33),
            Mode::Collapsed => {
                // The activities it shows (at 30 fps at most).
                let shown: Vec<usize> = match &self.notch_shown {
                    Some(Shown::Wide(it)) => vec![it],
                    Some(Shown::Pair(l, r)) => [l.as_ref(), r.as_ref()].into_iter().flatten().collect(),
                    None => Vec::new(),
                }
                .into_iter()
                .filter_map(|it| match it {
                    ShownItem::Live(i) => Some(*i),
                    ShownItem::W(..) => None,
                })
                .chain(self.shown_owner())
                .collect();
                for i in shown {
                    if i < self.modules.len() && self.enabled(i) && self.modules[i].animating() {
                        want(self.modules[i].frame_ms().max(33));
                    }
                }
                match self.mood {
                    Mood::Clock => {}
                    // A sleepy "z" floating up.
                    // (Less often once you have been away a while.)
                    Mood::Doze => want(if t - self.mood_since < 60.0 { 200 } else { 500 }),
                    // It looks at the pointer and blinks: nothing to see while
                    // the pointer is still and no blink is due.
                    Mood::Eyes if self.mascot_drawn.is_some() && t - self.ptr_at > 0.8 => want(crate::mascot::until_blink(t).clamp(16, 90)),
                    Mood::Eyes | Mood::Wave => want(33),
                }
                // Resting on it opens it in a moment.
                if self.hover && self.can_hover_open() {
                    want(until(self.hover_since + self.look().hover_delay(), 33));
                }
            }
        }
        if let Some(at) = self.leave_at {
            want(until(at, 100));
        }
        if self.hover {
            // A tooltip about to show (or fading in); else just now and then.
            match self.tip {
                Some((_, _, since, false)) if t - since < TIP_DELAY => want(until(since + TIP_DELAY, 250)),
                Some((_, _, since, false)) if t - since < TIP_DELAY + 0.2 => want(33),
                _ => {}
            }
            want(500);
        }
        ms
    }

    /// Everything else the closed notch shows right now, when none of it is
    /// drawn by a panel (live activities look as they like).
    fn closed_key(&self) -> Option<String> {
        let live = |it: &ShownItem| matches!(it, ShownItem::Live(_));
        match &self.notch_shown {
            Some(Shown::Wide(it)) if live(it) => return None,
            Some(Shown::Pair(l, r)) if l.iter().chain(r.iter()).any(live) => return None,
            _ => {}
        }
        Some(format!("{} {:?} {:?} {:?}", Now::get().hhmm(), self.notch_shown, self.indicators(), (self.mood, self.mascot_side())))
    }

    /// The closed notch's mascot right now (see mascot::key), when it's all
    /// that could make it look different.
    fn mascot_key(&self, t: f64) -> Option<[i32; 3]> {
        let alone = self.mode() == Mode::Collapsed
            && !self.hover
            && self.queue.is_empty()
            && self.leave_at.is_none()
            // (Fading in.)
            && t - self.mood_since > 0.8
            && !self.settings.extra.get("album_glow").is_some_and(|v| v == "1" && self.shown_owner().is_some())
            && !(0..self.modules.len()).any(|i| self.enabled(i) && self.modules[i].animating());
        if !alone {
            return None;
        }
        let n = self.edge_rect(self.cur.0, self.cur.1);
        let mut p = POINT::default();
        unsafe {
            GetCursorPos(&mut p).ok()?;
        }
        let m = ((p.x - self.origin.0) as f32 / self.scale, (p.y - self.origin.1) as f32 / self.scale);
        if self.mood == Mood::Clock {
            return Some([i32::MIN; 3]);
        }
        crate::mascot::key(self.settings.mascot, self.mood, t, gaze(n.cx(), n.cy(), m, 300.0))
    }

    /// The part of the canvas to draw into: all of it while something moves
    /// or shows around the notch, else the notch and what can be right
    /// beside it (dots, the song's glow, a tooltip below the open panel).
    fn draw_view(&self, notch: Rect, goal: Rect, dist: f32) -> Option<Rect> {
        if dist > 0.0 || self.drag_files || self.dragging || self.onboarding.is_some() {
            return None;
        }
        let (x, y) = match self.mode() {
            // (A tooltip can be about to show.)
            Mode::Expanded if self.tip.is_some() => (120.0, 44.0),
            Mode::Expanded => (16.0, 12.0),
            Mode::Toast => return None,
            Mode::Collapsed if self.hover => return None,
            Mode::Collapsed => (116.0, 84.0),
        };
        let l = notch.x.min(goal.x) - x;
        let t = notch.y.min(goal.y) - y;
        let r = notch.r().max(goal.r()) + x;
        let b = notch.b().max(goal.b()) + y;
        Some(Rect::new(l, t, r - l, b - t))
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
        let view = self.draw_view(notch, goal, dist);
        self.gfx.set_view(view, self.scale);
        self.gfx.begin(self.scale);
        crate::ui::note_hovers(true);
        let ear = (depth / 4.0).min(9.0);
        let barless = matches!(self.look(), NotchStyle::Pill | NotchStyle::Hidden);
        if depth >= 0.5 {
            if let Some(geo) = self.gfx.notch_geometry(notch, rad, ear, edge) {
                let glass = self.gfx.theme.glass;
                if glass {
                    // Liquid Glass: what is behind, blurred (glass.rs),
                    // under a light tint.
                    let goal = self.edge_rect(target.0, target.1);
                    self.update_backdrop(notch, goal, t, dist, ear, edge);
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
                    self.draw_glass_light(&geo, notch, rad, ear);
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
        self.hovers = crate::ui::note_hovers(false);
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
        const DELAY: f64 = TIP_DELAY;
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
        // The taskbar's: it takes the keyboard.
        if id == 1 + crate::features::HOTKEY_BAR {
            if let Some(h) = bar_window() {
                unsafe {
                    let mut pid = 0u32;
                    GetWindowThreadProcessId(h, Some(&mut pid));
                    let _ = AllowSetForegroundWindow(pid);
                    let _ = PostMessageW(Some(h), RegisterWindowMessageW(crate::surfaces::KEYS_MSG), WPARAM(0), LPARAM(0));
                }
            }
            return;
        }
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
                    ("\u{E8A9}", "One hover away", "Music, calendar, notes, timers, live activities and more."),
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
        let (old, entered) = (self.mouse, !self.hover);
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
        // Dragging a slider follows the mouse at the display's pace; hovering
        // needs less.
        if self.press.is_some() || entered {
            self.request_frame();
        } else if self.frame_interval == 0 || self.frame_interval > 25 {
            // (And nothing, while what's under the mouse is the same.)
            let hit = |p: (f32, f32)| self.hit_at(p.0, p.1).map(|h| (h.module, h.id));
            if hit(old) != hit((x, y)) || self.hovers.iter().any(|r| r.contains(old.0, old.1) != r.contains(x, y)) {
                self.set_frame_timer(25);
            }
        }
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
        if msg == unsafe { RegisterWindowMessageW(crate::surfaces::WANT_MSG) } {
            // The taskbar (re)started, or Liquid Glass turned light or dark
            // behind it: draw every face again.
            self.surf_light = wp.0 != 0;
            if lp.0 > 0 {
                self.surf_scale = lp.0 as f32 / 1000.0;
            }
            self.surf_keys.clear();
            self.surf_cache.clear();
            self.push_faces();
            return Some(LRESULT(0));
        }
        if msg == unsafe { RegisterWindowMessageW(crate::surfaces::SHOWN_MSG) } {
            self.bar_out = wp.0 != 0;
            self.bar_seen = lp.0 as u64;
            // Out again: its widgets as they are now.
            if self.bar_out {
                self.push_faces();
            }
            self.sync_fast();
            return Some(LRESULT(0));
        }
        if msg == unsafe { RegisterWindowMessageW(crate::surfaces::OPEN_MSG) } {
            self.open_full(wp.0);
            return Some(LRESULT(0));
        }
        if msg == unsafe { RegisterWindowMessageW(crate::surfaces::COMMAND_MSG) } {
            match wp.0 {
                crate::surfaces::COMMAND_SETTINGS => self.open_settings_taskbar(),
                crate::surfaces::COMMAND_AUTOHIDE => {
                    let on = crate::barcfg::flag(&self.settings, "bar_autohide", false);
                    self.settings.extra.insert("bar_autohide".into(), if on { "0" } else { "1" }.into());
                    self.settings.save();
                    self.bar_notify();
                    self.sw_render();
                }
                _ => {}
            }
            return Some(LRESULT(0));
        }
        if msg == unsafe { RegisterWindowMessageW(crate::surfaces::INPUT_MSG) } {
            self.full_input(wp.0, lp.0);
            return Some(LRESULT(0));
        }
        if msg == unsafe { RegisterWindowMessageW(crate::feed::OPEN_MSG) } {
            self.open_from_bar(wp.0);
            return Some(LRESULT(0));
        }
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
                    TIMER_FULL => self.push_full(),
                    TIMER_FACES => self.push_faces(),
                    TIMER_WAKE => {
                        unsafe {
                            let _ = KillTimer(Some(self.hwnd), TIMER_WAKE);
                        }
                        self.woke_at = self.t();
                        self.tick();
                    }
                    _ => {}
                }
                Some(LRESULT(0))
            }
            WM_APP_WAKE => {
                // They can come in a burst (the volume being dragged): a tick
                // now, and one after it for all the rest.
                let t = self.t();
                if t - self.woke_at < 0.04 {
                    unsafe {
                        SetTimer(Some(self.hwnd), TIMER_WAKE, 40, None);
                    }
                } else {
                    self.woke_at = t;
                    self.tick();
                }
                Some(LRESULT(0))
            }
            WM_APP_FRAME => {
                self.request_frame();
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

// ------------------------------------------------------------- docs renders

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
    custom_colors_of(st, "")
}

/// Your own colours: the notch's (`prefix` "") or the taskbar's ("bar_").
fn custom_colors_of(st: &Settings, prefix: &str) -> (u32, u32) {
    let get = |k: &str, d: u32| st.extra.get(&format!("{prefix}{k}")).and_then(|v| u32::from_str_radix(v, 16).ok()).unwrap_or(d);
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
        // (The key stays "Widgets": widgets are the taskbar's.)
        "Widgets" => "Live & status",
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
    if msg == WM_INPUT {
        unsafe {
            if crate::modules::system::is_switch_key(lp) {
                let _ = PostMessageW(Some(hwnd), WM_APP_WAKE, WPARAM(0), LPARAM(0));
            }
            return DefWindowProcW(hwnd, msg, wp, lp);
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

/// The taskbar add-on's exe, next to ours (Settings > Taskbar > Download).
/// (Looked for once a second at most: it's asked for many times a second.)
pub fn bar_exe() -> Option<std::path::PathBuf> {
    let mut c = BAR_EXE.lock().unwrap();
    if let Some((at, p)) = c.as_ref() {
        if at.elapsed().as_millis() < 900 {
            return p.clone();
        }
    }
    let p = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.join("nookly-taskbar.exe"))).filter(|p| p.exists());
    *c = Some((std::time::Instant::now(), p.clone()));
    p
}

static BAR_EXE: std::sync::Mutex<Option<(std::time::Instant, Option<std::path::PathBuf>)>> = std::sync::Mutex::new(None);

/// It was just installed or removed: look again now.
pub fn forget_bar_exe() {
    *BAR_EXE.lock().unwrap() = None;
}

/// The running taskbar add-on's window.
fn bar_window() -> Option<HWND> {
    unsafe { FindWindowW(w!("NooklyTaskbar"), PCWSTR::null()).ok().filter(|h| !h.is_invalid()) }
}
