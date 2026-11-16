//! The Settings window: a small rounded window of its own, in the notch's
//! style and theme, instead of a big panel inside the notch. Two tabs, each
//! with a master switch: Notch (every setting, in sections, plus the Widgets
//! panel) and Taskbar (coming soon, switched off for now).
//!
//! Kept cheap: the window and its drawing surface exist only while it is
//! open (closing frees both), it draws only when something changes (a click,
//! the mouse moving onto another control), and its only timers run while
//! the Mascot tiles or the Widgets section are on screen.

use super::*;

/// Window size (DIPs).
pub(super) const SW_W: f32 = 760.0;
pub(super) const SW_H: f32 = 560.0;
/// Outer padding, and the header strip you can drag the window by.
const SW_PAD: f32 = 20.0;
const HEAD_H: f32 = 74.0;
const SIDE_W: f32 = 180.0;
/// Height of a setting row (title, a line of explanation, a switch).
const ROW_H: f32 = 46.0;

// Settings action ids (the hits' module index is `SETTINGS`).
pub(super) const S_CLOSE: u32 = 1;
pub(super) const S_TAB: u32 = 2; // + 0 Notch, 1 Taskbar
pub(super) const S_MASTER: u32 = 4;
pub(super) const S_MASTER_TASKBAR: u32 = 5;
pub(super) const S_MINIMIZE: u32 = 6;
pub(super) const S_STYLE: u32 = 10; // + index into NotchStyle::ALL
pub(super) const S_EDGE: u32 = 20; // + index into Edge::ALL
pub(super) const S_CENTER: u32 = 30;
pub(super) const S_MASCOT: u32 = 40; // 0 = clock only, 1.. = Mascot::ALL[i - 1]
pub(super) const S_STARTUP: u32 = 60;
pub(super) const S_VOLUME: u32 = 61;
pub(super) const S_UPDATES: u32 = 62;
pub(super) const S_DATA: u32 = 63;
pub(super) const S_GITHUB: u32 = 64;
pub(super) const S_QUIT: u32 = 65;
pub(super) const S_CLIP_ON: u32 = 66;
pub(super) const S_PRIVACY: u32 = 67;
pub(super) const S_CLIP_MODE: u32 = 70; // + index into ClipMode::ALL
pub(super) const S_SOUND: u32 = 80; // + index into Sound::ALL
pub(super) const S_NOTIF_ON: u32 = 100;
pub(super) const S_NOTIF_TEXT: u32 = 101;
pub(super) const S_NOTIF_BANNERS: u32 = 102;
pub(super) const S_NOTIF_UNMUTE: u32 = 103;
pub(super) const S_POLICY: u32 = 104;
pub(super) const S_MASCOT_ALWAYS: u32 = 105;
pub(super) const S_MASCOT_POS: u32 = 106; // + 0 left, 1 centre, 2 right
pub(super) const S_UPDATE_AUTO: u32 = 109;
pub(super) const S_CLOCK12: u32 = 110;
pub(super) const S_MAKE_ROOM: u32 = 111;
pub(super) const S_COPY_TOAST: u32 = 112;
pub(super) const S_TASKBAR_PET: u32 = 113;
pub(super) const S_TOUR: u32 = 114;
pub(super) const S_GOTO_MASCOT: u32 = 115;
pub(super) const S_TURN_ON_WIDGETS: u32 = 116;
pub(super) const S_THEME: u32 = 200; // + index into THEMES
pub(super) const S_SECONDS: u32 = 210;
pub(super) const S_CUSTOM: u32 = 230;
pub(super) const S_GLOW: u32 = 231;
pub(super) const S_MOTION: u32 = 232;
pub(super) const S_SIZE: u32 = 233; // + 0 smaller, 1 normal, 2 larger
pub(super) const S_CFIX: u32 = 236;
pub(super) const S_CBG: u32 = 240; // + index into CUSTOM_BG
pub(super) const S_CTEXT: u32 = 250; // + index into CUSTOM_TEXT
pub(super) const S_HIDEAPP_ADD: u32 = 260;
pub(super) const S_HIDEAPP: u32 = 261; // + index
pub(super) const S_PROFILE: u32 = 280; // + index into features::PROFILES
pub(super) const S_PROFILE_AUTO: u32 = 285;
pub(super) const S_FEATURE: u32 = 300; // + index into features::FEATURES
pub(super) const S_PRESET: u32 = 330; // + index into features::PRESETS
pub(super) const S_HOTKEY: u32 = 340; // + index into features::HOTKEYS
pub(super) const S_EXPORT: u32 = 350;
pub(super) const S_IMPORT: u32 = 351;
pub(super) const S_IMPORT_OK: u32 = 352;
pub(super) const S_IMPORT_NO: u32 = 353;
pub(super) const S_MOUSE: u32 = 360; // + 0.. wheel, + 10.. middle, + 20.. double-click
pub(super) const S_SECTION: u32 = 400; // + index into SECTIONS
pub(super) const CUSTOM_BG: [u32; 8] = [0x000000, 0x1B1B2F, 0x0F3D3E, 0x3B1F2B, 0x2E2A24, 0xF2F2F2, 0xFFF4E0, 0xE6F0FF];
pub(super) const CUSTOM_TEXT: [u32; 8] = [0xFFFFFF, 0xF5F5F7, 0xFFE7B3, 0xCFFAF0, 0x111111, 0x2B2B2B, 0x3A2E1C, 0x0B2545];

/// The Notch tab's sections: (icon, name, what's in it).
pub(super) const SECTIONS: [(&str, &str, &str); 11] = [
    ("\u{E771}", "Appearance", "Style, place and size of the notch"),
    ("\u{E790}", "Theme", "Colours, the clock, and the song's glow while music plays"),
    ("\u{E76E}", "Mascot", "A little friend for the idle notch, or your taskbar"),
    ("\u{E8FD}", "Panels", "The order of the panels in the notch: drag them where you like"),
    ("\u{E8A9}", "Features", "Switch on what you use. What's off doesn't run at all."),
    ("\u{ECA5}", "Widgets", "What the closed notch shows, and in what order"),
    (glyph::BELL, "Alerts", "Sounds, notifications, and how each alert shows"),
    ("\u{E707}", "Location", "Your place, for the weather and (if you like) prayer times"),
    ("\u{E72E}", "Privacy", "Mic & camera dots, clipboard history, apps to hide for"),
    ("\u{E765}", "Shortcuts", "Keyboard shortcuts anywhere in Windows, and the mouse on the notch"),
    (glyph::SETTINGS, "General", "Startup, updates, moving to another PC, and about"),
];
pub(super) const SEC_THEME: u8 = 1;
pub(super) const SEC_MASCOT: u8 = 2;
pub(super) const SEC_PANELS: u8 = 3;
pub(super) const SEC_FEATURES: u8 = 4;
pub(super) const SEC_WIDGETS: u8 = 5;
pub(super) const SEC_ALERTS: u8 = 6;
pub(super) const SEC_LOCATION: u8 = 7;
pub(super) const SEC_PRIVACY: u8 = 8;
pub(super) const SEC_SHORTCUTS: u8 = 9;
pub(super) const SEC_GENERAL: u8 = 10;
/// Settings > Panels: a panel card (+ its place in the order), and reset.
pub(super) const S_ORDER: u32 = 500;
pub(super) const S_ORDER_RESET: u32 = 470;
/// Settings > Alerts: its two pages (+ 0 sounds & notifications, 1 each alert).
pub(super) const S_ALERT_VIEW: u32 = 480;

const TIMER_ANIM: usize = 1;
/// One-shot redraw (the "Checking..." label goes back after a few seconds).
const TIMER_REDRAW: usize = 2;

pub struct SettingsWin {
    pub hwnd: HWND,
    pub gfx: Gfx,
    pub hits: Vec<Hit>,
    mouse: (f32, f32),
    tracking: bool,
    /// The control pressed, and where.
    press: Option<(Hit, (f32, f32))>,
    /// A drag inside the Widgets panel (its Order list) is running.
    dragging: bool,
    /// Settings > Panels: the card being dragged, and where each card was
    /// drawn (to find where it's dropped).
    order_drag: Option<usize>,
    order_cells: Vec<Rect>,
    /// Settings > Alerts: 0 sounds & notifications, 1 how each alert shows.
    pub alert_view: u8,
    /// Control under the mouse when last drawn (redraw only when it changes).
    hover_id: Option<(usize, u32)>,
    /// Window position on screen (px).
    pos: (i32, i32),
    scale: f32,
    /// 0 Notch, 1 Taskbar.
    pub tab: u8,
    pub section: u8,
    animating: u32,
    /// Self-test: space left at the bottom of each section, and labels that
    /// don't fit their control (only measured in test and docs runs).
    pub room: [f32; SECTIONS.len()],
    pub tight: Vec<String>,
}

impl SettingsWin {
    /// Create the (still hidden) window, centred on the screen with the mouse.
    fn create() -> Option<SettingsWin> {
        unsafe {
            static REG: std::sync::Once = std::sync::Once::new();
            let hinst = GetModuleHandleW(None).ok()?;
            REG.call_once(|| {
                let hi: HINSTANCE = hinst.into();
                // The exe's own icon (stamped by build.ps1), else the tray icon.
                let icon = LoadIconW(Some(hi), PCWSTR(1 as *const u16)).unwrap_or_else(|_| make_tray_icon());
                let wc = WNDCLASSEXW {
                    cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                    lpfnWndProc: Some(settings_proc),
                    hInstance: hi,
                    lpszClassName: w!("NooklySettings"),
                    hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                    hIcon: icon,
                    hIconSm: icon,
                    ..Default::default()
                };
                RegisterClassExW(&wc);
            });
            let mut p = POINT::default();
            let _ = GetCursorPos(&mut p);
            let mon = MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(mon, &mut mi);
            let (mut dx, mut dy) = (96u32, 96u32);
            let _ = GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
            let scale = dx as f32 / 96.0;
            let (pw, ph) = ((SW_W * scale).ceil() as i32, (SW_H * scale).ceil() as i32);
            let r = mi.rcWork;
            let x = r.left + ((r.right - r.left) - pw).max(0) / 2;
            let y = r.top + ((r.bottom - r.top) - ph).max(0) / 2;
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_APPWINDOW,
                w!("NooklySettings"),
                w!("Nookly Settings"),
                WS_POPUP | WS_SYSMENU | WS_MINIMIZEBOX,
                x,
                y,
                pw,
                ph,
                None,
                None,
                Some(hinst.into()),
                None,
            )
            .ok()?;
            let Ok(gfx) = Gfx::new(pw, ph) else {
                let _ = DestroyWindow(hwnd);
                return None;
            };
            Some(SettingsWin {
                hwnd,
                gfx,
                hits: Vec::new(),
                mouse: (-1e4, -1e4),
                tracking: false,
                press: None,
                dragging: false,
                order_drag: None,
                order_cells: Vec::new(),
                alert_view: 0,
                hover_id: None,
                pos: (x, y),
                scale,
                tab: 0,
                section: 0,
                animating: 0,
                room: [0.0; SECTIONS.len()],
                tight: Vec::new(),
            })
        }
    }

    /// Change the drawing scale (DPI change, docs renders).
    pub(super) fn set_scale(&mut self, s: f32) {
        self.scale = s;
        let _ = self.gfx.resize((SW_W * s).ceil() as i32, (SW_H * s).ceil() as i32);
    }

    pub(super) fn hit_at(&self, x: f32, y: f32) -> Option<Hit> {
        self.hits.iter().rev().find(|h| h.rect.contains(x, y)).copied()
    }
}

extern "system" fn settings_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let res = with_app(|a| {
        let r = a.settings_msg(hwnd, msg, wp, lp);
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

impl App {
    /// The notch's master switch (Settings > Notch). Off: the notch is
    /// hidden and none of its features run.
    pub fn notch_on(&self) -> bool {
        self.settings.extra.get("notch_on").map_or(true, |v| v != "0")
    }

    /// Open the Settings window (gear, right-click on the notch or the
    /// taskbar mascot, tray icon), or bring it forward if it is open.
    pub fn open_settings(&mut self) {
        self.open_settings_at(None);
    }

    /// Open Settings on a section of the Notch tab.
    pub fn open_settings_at(&mut self, section: Option<u8>) {
        self.startup_cache = tray::startup_enabled();
        // The notch closes; the mouse may still be on it, so it must not
        // reopen until the mouse has left once.
        if self.onboarding.is_none() {
            if let Some(k) = self.keyboard {
                self.modules[k].blur();
                self.release_keyboard();
            }
            self.expanded = false;
            self.more_open = false;
            self.leave_at = None;
            if self.hover {
                self.hover_block = true;
            }
        }
        if self.swin.is_none() {
            self.swin = SettingsWin::create();
        }
        let Some(sw) = self.swin.as_mut() else { return };
        if let Some(s) = section {
            sw.tab = 0;
            sw.section = s.min(SECTIONS.len() as u8 - 1);
        }
        let h = sw.hwnd;
        self.sw_render();
        // Self-test and docs renders never show it.
        if !crate::modules::agents::ISOLATED.load(std::sync::atomic::Ordering::Relaxed) {
            unsafe {
                let _ = ShowWindow(h, if IsIconic(h).as_bool() { SW_RESTORE } else { SW_SHOW });
                let _ = SetForegroundWindow(h);
            }
            // Now that it shows, the timers can start.
            self.sw_render();
        }
        self.request_frame();
    }

    /// Close the window and free everything it held.
    pub fn close_settings(&mut self) {
        self.sw_blur();
        self.stop_recording();
        if let Some(sw) = self.swin.take() {
            unsafe {
                let _ = KillTimer(Some(sw.hwnd), TIMER_ANIM);
                let _ = KillTimer(Some(sw.hwnd), TIMER_REDRAW);
                let _ = DestroyWindow(sw.hwnd);
            }
        }
    }

    /// The Widgets panel, when Settings shows it.
    fn widgets_index(&self) -> Option<usize> {
        self.modules.iter().position(|m| m.title() == "Widgets")
    }

    /// Settings is showing module `i` (the Widgets panel) right now.
    pub(super) fn sw_hosts(&self, i: usize) -> bool {
        self.swin.as_ref().and_then(sw_widgets_view).is_some() && self.widgets_index() == Some(i)
    }

    /// Move a panel in the tab bar's order (Settings > Panels).
    pub(super) fn move_panel(&mut self, from: usize, to: usize) {
        let mut v: Vec<String> = self.tab_list().iter().map(|&i| self.modules[i].title().to_string()).collect();
        if from >= v.len() {
            return;
        }
        let t = v.remove(from);
        v.insert(to.min(v.len()), t);
        // Panels that are off keep their place after the ones that are on.
        for t in panel_order(&self.settings) {
            if !v.contains(&t) {
                v.push(t);
            }
        }
        self.settings.extra.insert("panel_order".into(), v.join("|"));
        self.settings.save();
        self.request_frame();
    }

    /// The hosted panel loses the keyboard (section change, close).
    fn sw_blur(&mut self) {
        if let Some(k) = self.sw_keyboard.take() {
            self.modules[k].blur();
        }
    }

    /// Draw the window and push it to the screen.
    pub(super) fn sw_render(&mut self) {
        let Some(mut sw) = self.swin.take() else { return };
        self.sw_draw(&mut sw);
        sw.gfx.end_and_present(sw.hwnd, sw.pos.0, sw.pos.1);
        // The mascot tiles blink and watch the cursor, and the Widgets panel
        // shows live values: a slow timer, only while they are on screen.
        let visible = unsafe { IsWindowVisible(sw.hwnd).as_bool() && !IsIconic(sw.hwnd).as_bool() };
        // (20 fps for the mascots; once a second is plenty for widget values.)
        let anim = match (visible && sw.tab == 0, sw.section) {
            (true, SEC_MASCOT) => 50,
            (true, SEC_WIDGETS | SEC_LOCATION) if !sw.dragging => 1000,
            (true, SEC_ALERTS) if sw.alert_view == 1 => 1000,
            _ => 0,
        };
        if anim != sw.animating {
            sw.animating = anim;
            unsafe {
                if anim > 0 {
                    SetTimer(Some(sw.hwnd), TIMER_ANIM, anim, None);
                } else {
                    let _ = KillTimer(Some(sw.hwnd), TIMER_ANIM);
                }
            }
        }
        self.swin = Some(sw);
    }

    /// A click on a control in the window (also used by the self-test).
    pub(super) fn sw_click(&mut self, x: f32, y: f32) {
        let Some(h) = self.swin.as_ref().and_then(|sw| sw.hit_at(x, y)) else { return };
        self.sw_hit(h, x, y);
    }

    /// Self-test and docs: show a tab and section, drawn.
    pub(super) fn sw_view(&mut self, tab: u8, sec: u8) {
        if self.swin.is_none() {
            self.open_settings();
        }
        if let Some(sw) = self.swin.as_mut() {
            sw.tab = tab;
            sw.section = sec;
        }
        self.sw_render();
    }

    /// Self-test: the window's controls as last drawn.
    pub(super) fn sw_hits(&self) -> Vec<Hit> {
        self.swin.as_ref().map(|sw| sw.hits.clone()).unwrap_or_default()
    }

    /// Self-test: the window shows Settings control `id`.
    pub(super) fn sw_has(&self, id: u32) -> bool {
        self.swin.as_ref().map_or(false, |sw| sw.hits.iter().any(|h| h.module == SETTINGS && h.id == id))
    }

    /// Self-test: a line about the last drawn window, like `snap`.
    pub(super) fn sw_snap(&self, label: &str) -> String {
        let Some(sw) = &self.swin else { return format!("FAIL {label}: no window\n") };
        format!(
            "{:<4} {label:<34} size={:>3.0}x{:<3.0} opaque_px={:<7} bright_px={:<6} hits={}\n",
            if sw.gfx.last_err.is_none() && sw.gfx.coverage() > 0 { "OK" } else { "FAIL" },
            SW_W,
            SW_H,
            sw.gfx.coverage(),
            sw.gfx.bright(),
            sw.hits.len()
        )
    }

    /// Self-test: click control `id` on whichever section has it.
    pub(super) fn sw_click_id(&mut self, id: u32) -> bool {
        if self.swin.is_none() {
            self.open_settings();
        }
        let start = self.swin.as_ref().map_or((0, 0), |sw| (sw.tab, sw.section));
        // The tab and section showing first, then every section.
        let views: Vec<(u8, u8)> = std::iter::once(start).chain((0..SECTIONS.len() as u8).map(|s| (0, s))).collect();
        for (tab, sec) in views {
            if let Some(sw) = self.swin.as_mut() {
                sw.tab = tab;
                sw.section = sec;
            }
            self.sw_render();
            if let Some(h) = self.swin.as_ref().and_then(|sw| sw.hits.iter().find(|h| h.module == SETTINGS && h.id == id).copied()) {
                self.sw_click(h.rect.cx(), h.rect.cy());
                return true;
            }
        }
        false
    }

    fn sw_hit(&mut self, h: Hit, x: f32, y: f32) {
        if h.module < self.modules.len() {
            // A control of the hosted Widgets panel.
            let rel = ((x - h.rect.x) / h.rect.w.max(1.0), (y - h.rect.y) / h.rect.h.max(1.0));
            self.call(h.module, |m, c| m.action(h.id, rel, c));
            self.sw_render();
        } else {
            self.sw_action(h.id);
        }
    }

    fn sw_action(&mut self, id: u32) {
        let Some(sw) = self.swin.as_mut() else { return };
        match id {
            S_CLOSE => {
                self.close_settings();
                return;
            }
            S_MINIMIZE => {
                let h = sw.hwnd;
                unsafe {
                    let _ = ShowWindow(h, SW_MINIMIZE);
                }
                // (minimized: its timers stop on the next draw)
            }
            S_TOUR => {
                self.close_settings();
                self.start_onboarding();
                return;
            }
            // The taskbar is coming: its switch stays off for now.
            S_MASTER_TASKBAR => {}
            i if (S_TAB..S_TAB + 2).contains(&i) => sw.tab = (i - S_TAB) as u8,
            i if (S_SECTION..S_SECTION + SECTIONS.len() as u32).contains(&i) => {
                sw.tab = 0;
                sw.section = (i - S_SECTION) as u8;
                self.sw_blur();
                self.stop_recording();
            }
            S_GOTO_MASCOT => {
                sw.tab = 0;
                sw.section = SEC_MASCOT;
            }
            i if (S_ALERT_VIEW..S_ALERT_VIEW + 2).contains(&i) => {
                sw.alert_view = (i - S_ALERT_VIEW) as u8;
                self.sw_blur();
            }
            // A card clicked without dragging: nothing to do.
            i if (S_ORDER..S_ORDER + 64).contains(&i) => {}
            S_TURN_ON_WIDGETS => {
                crate::features::set_on(&mut self.settings, "Widgets", true);
                self.settings.save();
                self.tick();
            }
            _ => {
                if id == S_UPDATES {
                    unsafe {
                        SetTimer(Some(sw.hwnd), TIMER_REDRAW, 4100, None);
                    }
                }
                self.settings_action(id);
            }
        }
        self.sw_render();
    }

    /// Messages for the Settings window.
    pub(super) fn settings_msg(&mut self, hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
        let sw = self.swin.as_mut().filter(|s| s.hwnd == hwnd)?;
        let scale = sw.scale;
        let xy = (((lp.0 & 0xffff) as i16) as f32 / scale, (((lp.0 >> 16) & 0xffff) as i16) as f32 / scale);
        match msg {
            WM_NCHITTEST => {
                // Drag the window by its header (anywhere but a control).
                let (x, y) = (xy.0 - sw.pos.0 as f32 / scale, xy.1 - sw.pos.1 as f32 / scale);
                let caption = y >= 0.0 && y < HEAD_H && sw.hit_at(x, y).is_none();
                Some(LRESULT((if caption { HTCAPTION } else { HTCLIENT }) as isize))
            }
            // No maximizing by double-clicking the header.
            WM_NCLBUTTONDBLCLK => Some(LRESULT(0)),
            WM_MOUSEMOVE => {
                sw.mouse = xy;
                if !sw.tracking {
                    sw.tracking = true;
                    unsafe {
                        let mut tme = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                        let _ = TrackMouseEvent(&mut tme);
                    }
                }
                if sw.dragging {
                    if let Some(w) = self.widgets_index() {
                        self.modules[w].drag_to(xy.0, xy.1);
                    }
                    self.sw_render();
                    return Some(LRESULT(0));
                }
                if sw.order_drag.is_some() {
                    self.sw_render();
                    return Some(LRESULT(0));
                }
                // A panel card in Settings > Panels that moves: drag it.
                if let Some((h, p0)) = sw.press {
                    if h.module == SETTINGS && (S_ORDER..S_ORDER + 64).contains(&h.id) && (xy.0 - p0.0).abs() + (xy.1 - p0.1).abs() > 5.0 {
                        sw.press = None;
                        sw.order_drag = Some((h.id - S_ORDER) as usize);
                        unsafe {
                            SetCapture(hwnd);
                        }
                        self.sw_render();
                        return Some(LRESULT(0));
                    }
                }
                // A press on the Widgets panel's order list that moves: drag it.
                if let Some((h, p0)) = sw.press {
                    if h.module < self.modules.len() && (xy.0 - p0.0).abs() + (xy.1 - p0.1).abs() > 5.0 && self.modules[h.module].drag_begin(h.id, p0.0, p0.1) {
                        self.modules[h.module].drag_to(xy.0, xy.1);
                        if let Some(sw) = self.swin.as_mut() {
                            sw.press = None;
                            sw.dragging = true;
                        }
                        unsafe {
                            SetCapture(hwnd);
                        }
                        self.sw_render();
                        return Some(LRESULT(0));
                    }
                }
                let sw = self.swin.as_mut()?;
                let id = sw.hit_at(xy.0, xy.1).map(|h| (h.module, h.id));
                if id != sw.hover_id {
                    sw.hover_id = id;
                    self.sw_render();
                }
                Some(LRESULT(0))
            }
            WM_MOUSELEAVE => {
                sw.tracking = false;
                if !sw.dragging {
                    sw.mouse = (-1e4, -1e4);
                    sw.press = None;
                }
                if sw.hover_id.take().is_some() {
                    self.sw_render();
                }
                Some(LRESULT(0))
            }
            WM_LBUTTONDOWN => {
                sw.press = sw.hit_at(xy.0, xy.1).map(|h| (h, xy));
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
                if let Some(from) = sw.order_drag.take() {
                    unsafe {
                        let _ = ReleaseCapture();
                    }
                    // Dropped on (nearest to) another card's place.
                    let to = sw
                        .order_cells
                        .iter()
                        .enumerate()
                        .min_by(|a, b| {
                            let d = |r: &Rect| (r.cx() - xy.0).powi(2) + (r.cy() - xy.1).powi(2);
                            d(a.1).partial_cmp(&d(b.1)).unwrap_or(std::cmp::Ordering::Equal)
                        })
                        .map(|(k, _)| k);
                    if let Some(to) = to.filter(|t| *t != from) {
                        self.move_panel(from, to);
                    }
                    self.sw_render();
                    return Some(LRESULT(0));
                }
                if sw.dragging {
                    sw.dragging = false;
                    unsafe {
                        let _ = ReleaseCapture();
                    }
                    if let Some(w) = self.widgets_index() {
                        self.call(w, |m, c| m.drag_drop(xy.0, xy.1, c));
                    }
                    self.sw_render();
                    return Some(LRESULT(0));
                }
                if let Some((h, _)) = sw.press.take().filter(|(h, _)| h.rect.contains(xy.0, xy.1)) {
                    self.sw_hit(h, xy.0, xy.1);
                }
                Some(LRESULT(0))
            }
            WM_MOUSEWHEEL => {
                let d = ((wp.0 >> 16) & 0xffff) as i16 as f32 / 120.0;
                if sw_widgets_view(sw).is_some() {
                    if let Some(w) = self.widgets_index() {
                        if self.modules[w].scroll(d) {
                            self.sw_render();
                        }
                    }
                }
                Some(LRESULT(0))
            }
            WM_KEYDOWN | WM_SYSKEYDOWN if self.recording.is_some() => {
                self.record_key(wp.0 as u32);
                self.sw_render();
                Some(LRESULT(0))
            }
            WM_KEYDOWN => {
                let vk = wp.0 as u16;
                let ctrl = unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0;
                let used = match self.sw_keyboard {
                    Some(k) => self.call(k, |m, c| m.key(vk, ctrl, c)),
                    None => false,
                };
                if !used && vk == VK_ESCAPE.0 {
                    self.close_settings();
                } else {
                    self.sw_render();
                }
                Some(LRESULT(0))
            }
            WM_CHAR => {
                if let (Some(k), Some(c)) = (self.sw_keyboard, char::from_u32(wp.0 as u32)) {
                    if !c.is_control() {
                        self.call(k, |m, cx| m.chr(c, cx));
                        self.sw_render();
                    }
                }
                Some(LRESULT(0))
            }
            WM_CLOSE => {
                self.close_settings();
                Some(LRESULT(0))
            }
            WM_TIMER => {
                if wp.0 == TIMER_REDRAW {
                    unsafe {
                        let _ = KillTimer(Some(hwnd), TIMER_REDRAW);
                    }
                }
                self.sw_render();
                Some(LRESULT(0))
            }
            WM_MOVE => {
                sw.pos = (((lp.0 & 0xffff) as i16) as i32, (((lp.0 >> 16) & 0xffff) as i16) as i32);
                None
            }
            // Coming back to the window (or restoring it): settings may have
            // changed meanwhile (the notch was dragged to another edge).
            WM_ACTIVATE | WM_SIZE => {
                self.sw_render();
                None
            }
            WM_DPICHANGED => {
                let r = unsafe { *(lp.0 as *const RECT) };
                sw.set_scale((wp.0 & 0xffff) as f32 / 96.0);
                sw.pos = (r.left, r.top);
                let (pw, ph) = (sw.gfx.pw, sw.gfx.ph);
                unsafe {
                    let _ = SetWindowPos(hwnd, None, r.left, r.top, pw, ph, SWP_NOZORDER | SWP_NOACTIVATE);
                }
                self.sw_render();
                Some(LRESULT(0))
            }
            _ => None,
        }
    }

    fn sw_draw(&mut self, sw: &mut SettingsWin) {
        use crate::features::{background, current_preset, hotkey, hotkey_label, is_on, FEATURES, GROUPS, HOTKEYS, PRESETS, PROFILES};
        let t = self.t();
        let st = self.settings.clone();
        let startup = self.startup_cache;
        let checking = t - self.updates_clicked < 4.0;
        let muted = crate::modules::notifs::muted(&st.extra).len();
        let on_taskbar = st.extra.get("taskbar_pet").map_or(false, |v| v == "1");
        let notch_on = self.notch_on();
        let x = |k: &str| st.extra.get(k).map_or(false, |v| v == "1");
        let features: Vec<(usize, &'static str, &'static str, u8, bool)> = FEATURES
            .iter()
            .enumerate()
            .filter_map(|(k, f)| {
                let (name, icon) = match background(f.key) {
                    Some(b) => b,
                    None => {
                        let m = self.modules.iter().find(|m| m.title() == f.key && m.has_tab())?;
                        (feature_name(m.title()), m.icon())
                    }
                };
                Some((k, name, icon, f.group, is_on(&st, f.key)))
            })
            .collect();
        let importing = self.import_pending.as_ref().map(|f| f.iter().map(|(n, _)| n.trim_end_matches(".txt")).collect::<Vec<_>>().join(", "));
        let last_front = if crate::time::demo() { String::new() } else { self.last_front.clone() };
        let (recording, fail) = (self.recording, self.hotkey_fail);
        let widgets = self.widgets_index().filter(|&w| self.enabled(w));
        let (tab, section) = (sw.tab, sw.section);
        let alert_view = sw.alert_view;
        // The Widgets panel shows in Widgets (on only), Alerts and Location.
        let view = sw_widgets_view(sw);
        let host = self.widgets_index().filter(|&w| view.is_some() && (view != Some(0) || self.enabled(w)));
        if let (Some(w), Some(v)) = (host, view) {
            self.modules[w].set_view(v);
            self.refresh_shown();
            let cat = self.notch_catalog();
            let showing = self.showing_ids();
            self.modules[w].set_widget_catalog(cat, &st, showing);
        }
        // Settings > Panels: the panels that are on, in the tab bar's order.
        let order: Vec<(&'static str, &'static str, Color)> = self.tab_list().iter().map(|&i| (panel_name(self.modules[i].title()), self.modules[i].icon(), self.modules[i].accent())).collect();
        let order_drag = sw.order_drag;
        let mut order_cells = Vec::new();
        let mut tight: Tight = crate::modules::agents::ISOLATED.load(std::sync::atomic::Ordering::Relaxed).then(Vec::new);
        let accent = col::ACCENT;
        let mouse = sw.mouse;
        let mut room = None;
        // The window wears the notch's theme (Liquid Glass has nothing behind
        // it to blur here, so it uses the plain dark one).
        sw.gfx.theme = if self.gfx.theme.glass { crate::gfx::THEMES[0] } else { self.gfx.theme };
        sw.hits.clear();
        sw.gfx.begin(sw.scale);
        {
            let mut ui = Ui { g: &mut sw.gfx, hits: &mut sw.hits, module: SETTINGS, mouse, t };
            let win = Rect::new(0.0, 0.0, SW_W, SW_H);
            ui.fill_rr(win, 22.0, col::NOTCH);
            ui.stroke_rr(win.inset(0.5), 21.5, col::TEXT.with_a(0.12), 1.0);
            let mut body = win.inset(SW_PAD);

            // Header: logo and name, the two tabs, minimize and close.
            let mut head = body.cut_top(38.0, 16.0);
            let close = head.cut_right(32.0, 8.0).centered(32.0, 32.0);
            let min = head.cut_right(32.0, 0.0).centered(32.0, 32.0);
            ui.icon_btn(min, "\u{E921}", S_MINIMIZE, 10.0, col::DIM, Some(col::BTN), "");
            ui.icon_btn(close, glyph::CLOSE, S_CLOSE, 11.0, col::DIM, Some(col::BTN), "");
            draw_logo(&mut ui, head.cut_left(36.0, 12.0).centered(36.0, 36.0));
            ui.text("Nookly", Rect::new(head.x, head.y, 170.0, 21.0), 16.0, col::TEXT, Weight::Semi, Align::Left);
            ui.text(&format!("Settings  \u{00B7}  {}", crate::modules::update::VERSION), Rect::new(head.x, head.y + 21.0, 170.0, 16.0), 11.5, col::DIM, Weight::Regular, Align::Left);
            let seg = Rect::new(win.cx() - 130.0, head.y + 1.0, 260.0, 36.0);
            ui.fill_rr(seg, 18.0, col::BTN);
            for (k, cell) in seg.inset(3.0).columns(2, 4.0).into_iter().enumerate() {
                let sel = tab == k as u8;
                if sel {
                    ui.fill_rr(cell, cell.h / 2.0, col::TEXT.with_a(0.16));
                } else if ui.hovered(cell) {
                    ui.fill_rr(cell, cell.h / 2.0, col::TEXT.with_a(0.06));
                }
                let fg = if sel { col::TEXT } else { col::DIM };
                let name = if k == 0 { "Notch" } else { "Taskbar" };
                let lw = ui.measure(name, 12.5, Weight::Semi);
                let badge = if k == 1 { 40.0 } else { 0.0 };
                let mut r = cell.centered(18.0 + 7.0 + lw + badge, cell.h);
                mini_screen(&mut ui, r.cut_left(18.0, 7.0).centered(18.0, 13.0), k == 1, if sel { accent } else { fg });
                ui.text(name, r.cut_left(lw + 1.0, 5.0), 12.5, fg, Weight::Semi, Align::Left);
                if k == 1 {
                    let b = r.cut_left(34.0, 0.0).centered(34.0, 16.0);
                    ui.fill_rr(b, 8.0, accent.with_a(0.2));
                    ui.text("Soon", b, 9.5, accent, Weight::Semi, Align::Center);
                }
                ui.hit_tip(cell, S_TAB + k as u32, "");
            }

            // Sidebar: the tab's master switch, then the sections.
            let side = body.cut_left(SIDE_W, 20.0);
            ui.fill_rr(side, 16.0, col::TEXT.with_a(0.035));
            let mut list = side.inset(8.0);
            let card = list.cut_top(56.0, 10.0);
            if tab == 0 {
                master_card(&mut ui, card, "Notch", if notch_on { "On" } else { "Off: hidden, nothing runs" }, notch_on, true, S_MASTER);
            } else {
                master_card(&mut ui, card, "Taskbar", "Coming soon", false, false, S_MASTER_TASKBAR);
            }
            if tab == 1 {
                ui.text_wrap("The taskbar's own settings will live here.", list.inset_xy(6.0, 4.0), 11.0, col::FAINT, Weight::Regular, Align::Left);
                draw_taskbar_soon(&mut ui, body);
            } else {
                for (k, (g, name, _)) in SECTIONS.iter().enumerate() {
                    let r = list.cut_top(31.0, 2.0);
                    let sel = section == k as u8;
                    if sel {
                        ui.fill_rr(r, 11.0, col::TEXT.with_a(0.10));
                    } else if ui.hovered(r) {
                        ui.fill_rr(r, 11.0, col::TEXT.with_a(0.05));
                    }
                    let mut c = r.inset_xy(8.0, 0.0);
                    let ic = c.cut_left(24.0, 9.0);
                    if sel {
                        ui.circle(ic.cx(), ic.cy(), 12.0, accent.with_a(0.18));
                    }
                    ui.icon(g, ic, 12.0, if sel { accent } else { col::DIM });
                    ui.text(name, c, 12.5, if sel { col::TEXT } else { col::DIM }, Weight::Semi, Align::Left);
                    fit(&mut ui, &mut tight, name, 12.5, Weight::Semi, c.w);
                    ui.hit_tip(r, S_SECTION + k as u32, "");
                }

                // The section.
                let mut c = body;
                let (_, title, sub) = SECTIONS[section as usize];
                ui.text(title, c.cut_top(26.0, 0.0), 19.0, col::TEXT, Weight::Semi, Align::Left);
                let subr = c.cut_top(18.0, 14.0);
                if notch_on {
                    ui.text(sub, subr, 11.5, col::DIM, Weight::Regular, Align::Left);
                    fit(&mut ui, &mut tight, sub, 11.5, Weight::Regular, c.w);
                } else {
                    ui.text("The notch is off. These apply when you turn it on (switch on the left).", subr, 11.5, col::RED, Weight::Regular, Align::Left);
                }
                let hint = c.cut_bottom(18.0, 10.0);
                let bottom = c.b();
                let cols = |c: &mut Rect, n: usize, h: f32, gap: f32| c.cut_top(h, gap).columns(n, 8.0);
                match section {
                    0 => {
                        group(&mut ui, &mut c, "Style");
                        let row = c.cut_top(92.0, 14.0);
                        for (i, (cell, s)) in row.columns(NotchStyle::ALL.len(), 8.0).into_iter().zip(NotchStyle::ALL).enumerate() {
                            style_tile(&mut ui, cell, s, st.style == s, S_STYLE + i as u32);
                        }
                        group(&mut ui, &mut c, "Position  \u{00B7}  or right-drag the notch to any edge");
                        let mut row = c.cut_top(30.0, 14.0);
                        let center = row.cut_right(96.0, 8.0);
                        for (i, (cell, e)) in row.columns(4, 8.0).into_iter().zip(Edge::ALL).enumerate() {
                            ui.chip(cell, e.label(), S_EDGE + i as u32, st.edge == e, accent);
                        }
                        ui.button(center, "Center", S_CENTER);
                        group(&mut ui, &mut c, "Size of the notch and panels");
                        let size = st.extra.get("ui_size").cloned().unwrap_or_else(|| "m".into());
                        let row = c.cut_top(30.0, 14.0);
                        for (k, (cell, (key, label))) in row.columns(3, 8.0).into_iter().zip([("s", "Smaller"), ("m", "Normal"), ("l", "Larger")]).enumerate() {
                            ui.chip(cell, label, S_SIZE + k as u32, size == key, accent);
                        }
                        let r = c.cut_top(ROW_H, 0.0).columns(2, 8.0);
                        switch_row(&mut ui, &mut tight, r[0], "Step aside for windows", "Slim, click-through bar over them", st.make_room, S_MAKE_ROOM);
                        switch_row(&mut ui, &mut tight, r[1], "Reduce motion", "No springy animations", x("reduce_motion"), S_MOTION);
                    }
                    1 => {
                        group(&mut ui, &mut c, "Theme  \u{00B7}  the notch and every panel");
                        let per = 6;
                        let n = crate::gfx::THEMES.len() + 1;
                        let n_rows = (n + per - 1) / per;
                        let grid = c.cut_top(n_rows as f32 * 50.0 + (n_rows - 1) as f32 * 6.0, 12.0);
                        let rows = grid.rows(n_rows, 6.0);
                        for i in 0..n {
                            let cell = rows[i / per].columns(per, 6.0)[i % per];
                            theme_tile(&mut ui, &mut tight, cell, i, &st);
                        }
                        if st.theme == "custom" {
                            let (bg, text) = custom_colors(&st);
                            for (label, list, base, cur) in [("Background", &CUSTOM_BG, S_CBG, bg), ("Text", &CUSTOM_TEXT, S_CTEXT, text)] {
                                let mut row = c.cut_top(24.0, 4.0);
                                ui.text(label, row.cut_left(80.0, 4.0), 11.0, col::DIM, Weight::Semi, Align::Left);
                                for (k, cell) in row.columns(list.len(), 4.0).into_iter().enumerate() {
                                    let d = cell.centered(20.0, 20.0);
                                    ui.fill_rr_raw(d, 10.0, hex(list[k]));
                                    ui.stroke_rr(d, 10.0, if cur == list[k] { accent } else { col::TEXT.with_a(0.25) }, if cur == list[k] { 2.0 } else { 1.0 });
                                    ui.hit(cell, base + k as u32);
                                }
                            }
                            let ratio = crate::gfx::contrast(hex(bg), hex(text));
                            let mut row = c.cut_top(24.0, 10.0);
                            if ratio < 4.5 {
                                ui.button_c(row.cut_right(70.0, 6.0), "Fix it", S_CFIX, col::RED.with_a(0.2), col::RED);
                            }
                            let verdict = if ratio >= 7.0 { "(great)" } else if ratio >= 4.5 { "(readable)" } else { "(too low to read well)" };
                            ui.text(&format!("Contrast {ratio:.1}:1 {verdict}"), row, 11.0, if ratio >= 4.5 { col::DIM } else { col::RED }, Weight::Regular, Align::Left);
                        }
                        let r = c.cut_top(ROW_H, 8.0).columns(2, 8.0);
                        switch_row(&mut ui, &mut tight, r[0], "12-hour clock", "4:20 PM instead of 16:20", st.clock12, S_CLOCK12);
                        switch_row(&mut ui, &mut tight, r[1], "Seconds on the clock", "The notch gets a bit wider", st.seconds, S_SECONDS);
                        switch_row(&mut ui, &mut tight, c.cut_top(ROW_H, 0.0), "Song colour around the notch", "The album's colour glows to the beat while music plays", x("album_glow"), S_GLOW);
                    }
                    2 => {
                        group(&mut ui, &mut c, "Character  \u{00B7}  hover one to say hi");
                        let grid = c.cut_top(116.0, 14.0);
                        // Tile 0 is "Clock" (no mascot), then every mascot.
                        let mut k = 0usize;
                        for row in grid.rows(2, 8.0) {
                            for cell in row.columns(5, 8.0) {
                                if k > Mascot::ALL.len() {
                                    break;
                                }
                                mascot_tile(&mut ui, &mut tight, cell, k, &st, t, mouse);
                                k += 1;
                            }
                        }
                        group(&mut ui, &mut c, "Where it sits in the notch (Full style)");
                        let mut row = c.cut_top(30.0, 14.0);
                        for (k, (cell, name)) in row.cut_left(300.0, 0.0).columns(3, 8.0).into_iter().zip(["Left", "Center", "Right"]).enumerate() {
                            ui.chip(cell, name, S_MASCOT_POS + k as u32, st.mascot_pos == k as u8, accent);
                        }
                        switch_row(&mut ui, &mut tight, c.cut_top(ROW_H, 8.0), "Always out", "Show it all the time, even while something plays", st.mascot_always, S_MASCOT_ALWAYS);
                        switch_row(&mut ui, &mut tight, c.cut_top(ROW_H, 0.0), "On the taskbar", "It moves to the taskbar corner; the panel opens above it", on_taskbar, S_TASKBAR_PET);
                    }
                    SEC_PANELS => {
                        let mut foot = c.cut_bottom(30.0, 12.0);
                        ui.button(foot.cut_right(130.0, 0.0), "Reset order", S_ORDER_RESET);
                        ui.text("Drag a panel to its new place. Switch panels on or off in Features.", foot, 11.0, col::DIM, Weight::Regular, Align::Left);
                        let n = order.len();
                        let cols = if n > 21 { 4 } else { 3 };
                        let rows = ((n + cols - 1) / cols).max(1);
                        let pitch = ((c.h + 8.0) / rows as f32).min(50.0);
                        let cw = (c.w - 8.0 * (cols as f32 - 1.0)) / cols as f32;
                        for k in 0..n {
                            order_cells.push(Rect::new(c.x + (k % cols) as f32 * (cw + 8.0), c.y + (k / cols) as f32 * pitch, cw, pitch - 8.0));
                        }
                        // While dragging: where it would land.
                        let target = order_drag.and_then(|_| {
                            order_cells.iter().enumerate().min_by(|a, b| {
                                let d = |r: &Rect| (r.cx() - mouse.0).powi(2) + (r.cy() - mouse.1).powi(2);
                                d(a.1).partial_cmp(&d(b.1)).unwrap_or(std::cmp::Ordering::Equal)
                            }).map(|(k, _)| k)
                        });
                        for (k, &(name, icon, acc)) in order.iter().enumerate() {
                            let cell = order_cells[k];
                            if order_drag == Some(k) {
                                ui.stroke_rr(cell.inset(0.5), 12.0, col::TEXT.with_a(0.18), 1.0);
                            } else {
                                order_card(&mut ui, &mut tight, cell, k + 1, name, icon, acc, 1.0);
                            }
                            if target == Some(k) && order_drag != Some(k) {
                                ui.stroke_rr(cell.inset(-1.0), 13.0, accent, 2.0);
                            }
                            ui.hit_tip(cell, S_ORDER + k as u32, "");
                        }
                        // The dragged card follows the mouse.
                        if let Some(k) = order_drag.filter(|k| *k < n) {
                            let (name, icon, acc) = order[k];
                            let cell = order_cells[k];
                            let r = Rect::new(mouse.0 - cell.w / 2.0, mouse.1 - cell.h / 2.0, cell.w, cell.h);
                            order_card(&mut ui, &mut tight, r, target.map_or(k, |t| t) + 1, name, icon, acc, 0.92);
                        }
                        c.y = c.y.max(c.y + rows as f32 * pitch - 8.0);
                    }
                    SEC_FEATURES => {
                        let preset = current_preset(&st);
                        let mut row = c.cut_top(30.0, 12.0);
                        ui.text("Start from", row.cut_left(70.0, 6.0), 11.5, col::DIM, Weight::Semi, Align::Left);
                        for (k, cell) in row.columns(PRESETS.len(), 6.0).into_iter().enumerate() {
                            ui.chip(cell, PRESETS[k].0, S_PRESET + k as u32, preset == Some(k), accent);
                            fit(&mut ui, &mut tight, PRESETS[k].0, 11.5, Weight::Semi, cell.w - 10.0);
                        }
                        let mut foot = c.cut_bottom(30.0, 12.0);
                        ui.text("Profile", foot.cut_left(52.0, 6.0), 11.5, col::DIM, Weight::Semi, Align::Left);
                        let active = crate::features::active_profile(&st);
                        for (k, name) in PROFILES.iter().enumerate() {
                            ui.chip(foot.cut_left(72.0, 6.0), name, S_PROFILE + k as u32, active == *name, accent);
                        }
                        ui.chip(foot.cut_left(140.0, 0.0), "Auto (work hours)", S_PROFILE_AUTO, x("profile_auto"), accent);
                        let grid = c;
                        for (g, mut col_r) in grid.columns(GROUPS.len(), 8.0).into_iter().enumerate() {
                            ui.text(GROUPS[g], col_r.cut_top(15.0, 4.0), 11.5, col::DIM, Weight::Semi, Align::Left);
                            for &(k, name, icon, _, on) in features.iter().filter(|it| it.3 == g as u8) {
                                let row = col_r.cut_top(24.0, 3.0);
                                feature_row(&mut ui, &mut tight, row, name, icon, on, S_FEATURE + k as u32);
                            }
                            c.y = c.y.max(col_r.y);
                        }
                        c.y = c.y.max(grid.y);
                    }
                    SEC_WIDGETS => match (widgets, self.widgets_index()) {
                        (Some(w), _) => {
                            ui.module = w;
                            self.modules[w].draw(&mut ui, c);
                            ui.module = SETTINGS;
                        }
                        (None, Some(_)) => {
                            let mut m = c.centered(380.0, 110.0);
                            ui.text("Widgets are off", m.cut_top(24.0, 4.0), 15.0, col::TEXT, Weight::Semi, Align::Center);
                            ui.text_wrap("Switch them on to pick what the closed notch shows: battery, weather, Wi-Fi and more.", m.cut_top(36.0, 10.0), 11.5, col::DIM, Weight::Regular, Align::Center);
                            ui.button_accent(m.cut_top(30.0, 0.0).centered(180.0, 30.0), "Turn on widgets", S_TURN_ON_WIDGETS, accent);
                        }
                        _ => {}
                    },
                    SEC_ALERTS => {
                        let tabs = c.cut_top(30.0, 14.0);
                        let mut tabs = tabs;
                        for (k, label) in ["Sounds & notifications", "How each alert shows"].iter().enumerate() {
                            let w = ui.measure(label, 11.5, Weight::Semi) + 32.0;
                            ui.chip(tabs.cut_left(w, 8.0), label, S_ALERT_VIEW + k as u32, alert_view == k as u8, accent);
                        }
                        if alert_view == 1 {
                            if let Some(w) = host {
                                ui.module = w;
                                self.modules[w].draw(&mut ui, c);
                                ui.module = SETTINGS;
                            }
                            c.y = c.b();
                        } else {
                        group(&mut ui, &mut c, "Reminder sound  \u{00B7}  click one to hear it");
                        let row = c.cut_top(30.0, 14.0);
                        for (i, (cell, s)) in row.columns(Sound::ALL.len(), 6.0).into_iter().zip(Sound::ALL).enumerate() {
                            ui.chip(cell, s.label(), S_SOUND + i as u32, st.sound == s, accent);
                            fit(&mut ui, &mut tight, s.label(), 11.5, Weight::Semi, cell.w - 10.0);
                        }
                        switch_row(&mut ui, &mut tight, c.cut_top(ROW_H, 14.0), "Volume overlay", "A bar in the notch when you change the volume", st.volume_overlay, S_VOLUME);
                        group(&mut ui, &mut c, "Windows notifications");
                        switch_row(&mut ui, &mut tight, c.cut_top(ROW_H, 8.0), "Show them in the notch", "Other apps' notifications drop out of the notch", st.notif_on, S_NOTIF_ON);
                        switch_row(&mut ui, &mut tight, c.cut_top(ROW_H, 8.0), "Include message text", "Off: only the app name shows", st.notif_text, S_NOTIF_TEXT);
                        let b = cols(&mut c, 2, 30.0, 0.0);
                        let unmute = if muted == 0 { "No muted apps".to_string() } else { format!("Unmute {muted} app{}", if muted == 1 { "" } else { "s" }) };
                        ui.button(b[0], "Windows banners", S_NOTIF_BANNERS);
                        ui.button(b[1], &unmute, S_NOTIF_UNMUTE);
                        }
                    }
                    SEC_LOCATION => {
                        if let Some(w) = host {
                            ui.module = w;
                            self.modules[w].draw(&mut ui, c);
                            ui.module = SETTINGS;
                        }
                        c.y = c.b();
                    }
                    SEC_PRIVACY => {
                        switch_row(&mut ui, &mut tight, c.cut_top(ROW_H, 14.0), "Mic & camera dots", "A dot beside the notch while an app uses them", st.privacy_dots, S_PRIVACY);
                        group(&mut ui, &mut c, "Clipboard history");
                        switch_row(&mut ui, &mut tight, c.cut_top(ROW_H, 8.0), "Record what I copy", "Search it and paste it again from the Clipboard panel", st.clip_on, S_CLIP_ON);
                        let row = c.cut_top(30.0, 8.0);
                        for (i, (cell, m)) in row.columns(3, 8.0).into_iter().zip(ClipMode::ALL).enumerate() {
                            ui.chip(cell, clip_mode_short(m), S_CLIP_MODE + i as u32, st.clip_mode == m, accent);
                        }
                        switch_row(&mut ui, &mut tight, c.cut_top(ROW_H, 14.0), "Show what I copy", "A quick toast of what you copied; nothing is kept", st.copy_toast, S_COPY_TOAST);
                        group(&mut ui, &mut c, "Hide the notch while these apps are in front");
                        let mut row = c.cut_top(30.0, 0.0);
                        if !last_front.is_empty() && !hidden_apps(&st).iter().any(|a| a.eq_ignore_ascii_case(&last_front)) {
                            let label = format!("+ {}", crate::modules::health::friendly(&last_front));
                            let w = (ui.measure(&label, 12.0, Weight::Semi) + 28.0).min(200.0);
                            ui.button(row.cut_right(w, 8.0), &label, S_HIDEAPP_ADD);
                        }
                        let apps = hidden_apps(&st);
                        for (k, a) in apps.iter().enumerate().take(6) {
                            let name = format!("{}  \u{00D7}", crate::modules::health::friendly(a));
                            let w = ui.measure(&name, 11.5, Weight::Semi) + 26.0;
                            if w > row.w {
                                break;
                            }
                            ui.chip(row.cut_left(w, 6.0), &name, S_HIDEAPP + k as u32, true, accent);
                        }
                        if apps.is_empty() {
                            ui.text("None yet. Switch to an app, then come back and add it.", row, 11.0, col::FAINT, Weight::Regular, Align::Left);
                        }
                    }
                    SEC_SHORTCUTS => {
                        group(&mut ui, &mut c, "Keyboard  \u{00B7}  works anywhere in Windows");
                        for k in 0..HOTKEYS.len() {
                            let mut row = c.cut_top(30.0, 6.0);
                            let chip = row.cut_right(170.0, 0.0);
                            ui.text(HOTKEYS[k].1, row, 12.5, col::TEXT, Weight::Regular, Align::Left);
                            fit(&mut ui, &mut tight, HOTKEYS[k].1, 12.5, Weight::Regular, row.w);
                            let (label, sel) = if recording == Some(k) {
                                ("Press keys...".to_string(), true)
                            } else if fail[k] {
                                ("In use by another app".to_string(), false)
                            } else {
                                (hotkey(&st, k).map_or("Add".to_string(), |(m, v)| hotkey_label(m, v)), false)
                            };
                            ui.chip(chip, &label, S_HOTKEY + k as u32, sel, accent);
                            if fail[k] && recording != Some(k) {
                                ui.stroke_rr(chip, chip.h / 2.0, col::RED.with_a(0.7), 1.0);
                            }
                        }
                        c.cut_top(8.0, 0.0);
                        group(&mut ui, &mut c, "Mouse on the closed notch  \u{00B7}  Shift + scroll is always brightness");
                        let val = |k: &str, d: &str| st.extra.get(k).cloned().unwrap_or_else(|| d.to_string());
                        let rows: [(&str, &str, &str, &[(&str, &str)], u32); 3] = [
                            ("Scroll wheel", "mouse_wheel", "off", &[("off", "Nothing"), ("volume", "Volume"), ("brightness", "Brightness")], S_MOUSE),
                            ("Middle click", "mouse_middle", "play", &[("off", "Nothing"), ("play", "Play / pause")], S_MOUSE + 10),
                            ("Double-click", "mouse_double", "off", &[("off", "Nothing"), ("shot", "Screenshot"), ("clip", "Clipboard"), ("note", "Note")], S_MOUSE + 20),
                        ];
                        for (k, (name, key, def, opts, base)) in rows.into_iter().enumerate() {
                            let mut row = c.cut_top(30.0, if k == 2 { 0.0 } else { 6.0 });
                            ui.text(name, row.cut_left(110.0, 8.0), 12.5, col::TEXT, Weight::Regular, Align::Left);
                            let cur = val(key, def);
                            for (j, (cell, (v, label))) in row.columns(4, 6.0).into_iter().zip(opts.iter()).enumerate() {
                                ui.chip(cell, label, base + j as u32, cur == *v, accent);
                                fit(&mut ui, &mut tight, label, 11.5, Weight::Semi, cell.w - 10.0);
                            }
                        }
                    }
                    _ => {
                        if let Some(names) = importing {
                            group(&mut ui, &mut c, "Import settings");
                            ui.text_wrap("Replace your settings and data on this PC with the ones in the file? Nookly starts again to load them.", c.cut_top(36.0, 4.0), 12.0, col::TEXT, Weight::Regular, Align::Left);
                            ui.text_wrap(&format!("In the file: {names}"), c.cut_top(30.0, 8.0), 10.5, col::DIM, Weight::Regular, Align::Left);
                            let b = cols(&mut c, 2, 30.0, 14.0);
                            ui.button_accent(b[0], "Replace", S_IMPORT_OK, accent);
                            ui.button(b[1], "Cancel", S_IMPORT_NO);
                        } else {
                            let r = c.cut_top(ROW_H, 14.0).columns(2, 8.0);
                            switch_row(&mut ui, &mut tight, r[0], "Start with Windows", "Open Nookly when you sign in", startup, S_STARTUP);
                            switch_row(&mut ui, &mut tight, r[1], "Check for updates", "Twice a day, automatically", st.update_check, S_UPDATE_AUTO);
                            let b = cols(&mut c, 3, 32.0, 8.0);
                            ui.button(b[0], if checking { "Checking..." } else { "Check for updates" }, S_UPDATES);
                            ui.button(b[1], "Open data folder", S_DATA);
                            ui.button(b[2], "Welcome tour", S_TOUR);
                            let b = cols(&mut c, 2, 32.0, 14.0);
                            ui.button(b[0], "Export settings", S_EXPORT);
                            ui.button(b[1], "Import settings", S_IMPORT);
                            for s in ["Check for updates", "Open data folder", "Welcome tour", "Export settings", "Import settings"] {
                                fit(&mut ui, &mut tight, s, 12.0, Weight::Semi, b[0].w.min((c.w - 16.0) / 3.0) - 16.0);
                            }
                        }
                        group(&mut ui, &mut c, "About");
                        let card = c.cut_top(100.0, 0.0);
                        ui.fill_rr(card, 14.0, col::TEXT.with_a(0.045));
                        let mut inner = card.inset(14.0);
                        let buttons = inner.cut_bottom(28.0, 0.0);
                        draw_logo(&mut ui, inner.cut_left(40.0, 12.0).centered(40.0, 40.0));
                        let v = format!("Nookly {}", crate::modules::update::VERSION);
                        ui.text(&v, inner.cut_top(20.0, 0.0), 14.0, col::TEXT, Weight::Semi, Align::Left);
                        ui.text("A living notch for Windows. Free and open source.", inner, 11.5, col::DIM, Weight::Regular, Align::Left);
                        let b = buttons.columns(3, 8.0);
                        ui.button(b[0], "GitHub page", S_GITHUB);
                        ui.button(b[1], "Privacy policy", S_POLICY);
                        ui.button_c(b[2], "Quit Nookly", S_QUIT, col::RED.with_a(0.18), col::RED);
                    }
                }
                room = Some(bottom - c.y);

                // What the control under the mouse does.
                let under = ui.hits.iter().rev().find(|h| h.rect.contains(mouse.0, mouse.1)).copied();
                let tip = under
                    .map(|h| {
                        if !h.tip.is_empty() {
                            h.tip
                        } else if h.module < self.modules.len() {
                            self.modules[h.module].tip(h.id)
                        } else {
                            settings_tip(h.id)
                        }
                    })
                    .filter(|s| !s.is_empty());
                let mut hr = hint;
                ui.icon("\u{E946}", hr.cut_left(14.0, 6.0), 10.5, col::FAINT);
                ui.text(tip.unwrap_or("Changes apply and save straight away."), hr, 11.0, if tip.is_some() { col::DIM } else { col::FAINT }, Weight::Regular, Align::Left);
            }
        }
        if let Some(r) = room {
            sw.room[section as usize] = r;
        }
        sw.order_cells = order_cells;
        for s in tight.unwrap_or_default() {
            if !sw.tight.contains(&s) {
                sw.tight.push(s);
            }
        }
    }

    /// Apply a settings click.
    pub(super) fn settings_action(&mut self, id: u32) {
        match id {
            S_ORDER_RESET => {
                self.settings.extra.remove("panel_order");
            }
            S_MASTER => {
                let on = self.notch_on();
                self.settings.extra.insert("notch_on".into(), if on { "0" } else { "1" }.into());
                self.settings.save();
                self.check_fullscreen();
                self.sync_pet();
            }
            i if (S_STYLE..S_STYLE + NotchStyle::ALL.len() as u32).contains(&i) => {
                self.undock();
                self.settings.style = NotchStyle::ALL[(i - S_STYLE) as usize];
            }
            i if (S_EDGE..S_EDGE + Edge::ALL.len() as u32).contains(&i) => {
                self.set_edge(Edge::ALL[(i - S_EDGE) as usize], false);
            }
            S_CENTER => {
                let e = self.edge();
                self.set_edge(e, true);
            }
            S_MASCOT => self.settings.personality = false,
            i if (S_MASCOT + 1..=S_MASCOT + Mascot::ALL.len() as u32).contains(&i) => {
                self.settings.personality = true;
                self.settings.mascot = Mascot::ALL[(i - S_MASCOT - 1) as usize];
            }
            S_STARTUP => {
                tray::set_startup(!tray::startup_enabled());
                self.startup_cache = tray::startup_enabled();
            }
            S_VOLUME => self.settings.volume_overlay = !self.settings.volume_overlay,
            S_UPDATE_AUTO => self.settings.update_check = !self.settings.update_check,
            S_PRIVACY => self.settings.privacy_dots = !self.settings.privacy_dots,
            i if (S_SOUND..S_SOUND + Sound::ALL.len() as u32).contains(&i) => {
                let s = Sound::ALL[(i - S_SOUND) as usize];
                self.settings.sound = s;
                sound::set(s);
                sound::play(s);
            }
            S_CLIP_ON => self.settings.clip_on = !self.settings.clip_on,
            S_COPY_TOAST => self.settings.copy_toast = !self.settings.copy_toast,
            S_CLOCK12 => {
                self.settings.clock12 = !self.settings.clock12;
                crate::time::H12.store(self.settings.clock12, std::sync::atomic::Ordering::Relaxed);
            }
            S_MAKE_ROOM => {
                self.settings.make_room = !self.settings.make_room;
                self.sync_passthrough();
            }
            i if (S_THEME..S_THEME + crate::gfx::THEMES.len() as u32).contains(&i) => {
                self.settings.theme = crate::gfx::THEMES[(i - S_THEME) as usize].key.to_string();
                self.backdrop = None;
            }
            S_CUSTOM => {
                self.settings.theme = "custom".into();
                self.backdrop = None;
            }
            i if (S_CBG..S_CBG + 8).contains(&i) => {
                self.settings.extra.insert("custom_bg".into(), format!("{:06X}", CUSTOM_BG[(i - S_CBG) as usize]));
            }
            i if (S_CTEXT..S_CTEXT + 8).contains(&i) => {
                self.settings.extra.insert("custom_text".into(), format!("{:06X}", CUSTOM_TEXT[(i - S_CTEXT) as usize]));
            }
            S_CFIX => {
                // The readable one of white and near-black.
                let (bg, _) = custom_colors(&self.settings);
                let w = crate::gfx::contrast(hex(bg), hex(0xFFFFFF));
                let b = crate::gfx::contrast(hex(bg), hex(0x111111));
                self.settings.extra.insert("custom_text".into(), if w >= b { "FFFFFF" } else { "111111" }.into());
            }
            S_GLOW | S_MOTION => {
                let k = if id == S_GLOW { "album_glow" } else { "reduce_motion" };
                let on = self.settings.extra.get(k).map_or(false, |v| v == "1");
                self.settings.extra.insert(k.into(), if on { "0" } else { "1" }.into());
            }
            i if (S_SIZE..S_SIZE + 3).contains(&i) => {
                self.settings.extra.insert("ui_size".into(), ["s", "m", "l"][(i - S_SIZE) as usize].into());
                self.settings.save();
                self.place();
            }
            S_HIDEAPP_ADD => {
                let mut v = hidden_apps(&self.settings);
                if !self.last_front.is_empty() && !v.contains(&self.last_front) {
                    v.push(self.last_front.clone());
                }
                self.settings.extra.insert("hide_apps".into(), v.join("|"));
            }
            i if (S_HIDEAPP..S_HIDEAPP + 10).contains(&i) => {
                let mut v = hidden_apps(&self.settings);
                let k = (i - S_HIDEAPP) as usize;
                if k < v.len() {
                    v.remove(k);
                }
                self.settings.extra.insert("hide_apps".into(), v.join("|"));
            }
            i if (S_PROFILE..S_PROFILE + 3).contains(&i) => {
                crate::features::switch_profile(&mut self.settings, crate::features::PROFILES[(i - S_PROFILE) as usize]);
                crate::time::SECONDS.store(self.settings.seconds, std::sync::atomic::Ordering::Relaxed);
                self.backdrop = None;
            }
            S_PROFILE_AUTO => {
                let on = self.settings.extra.get("profile_auto").map_or(false, |v| v == "1");
                self.settings.extra.insert("profile_auto".into(), if on { "0" } else { "1" }.into());
            }
            S_SECONDS => {
                self.settings.seconds = !self.settings.seconds;
                crate::time::SECONDS.store(self.settings.seconds, std::sync::atomic::Ordering::Relaxed);
            }
            S_MASCOT_ALWAYS => self.settings.mascot_always = !self.settings.mascot_always,
            S_TASKBAR_PET => {
                let on = self.settings.extra.get("taskbar_pet").map_or(false, |v| v == "1");
                self.settings.extra.insert("taskbar_pet".into(), if on { "0" } else { "1" }.into());
                self.sync_pet();
            }
            i if (S_MASCOT_POS..S_MASCOT_POS + 3).contains(&i) => self.settings.mascot_pos = (i - S_MASCOT_POS) as u8,
            i if (S_FEATURE..S_FEATURE + crate::features::FEATURES.len() as u32).contains(&i) => {
                let key = crate::features::FEATURES[(i - S_FEATURE) as usize].key;
                let on = crate::features::is_on(&self.settings, key);
                let panel = crate::features::background(key).is_none() && key != "Widgets";
                // At least one panel always stays.
                if on && panel && self.tab_list().len() <= 1 {
                    return;
                }
                crate::features::set_on(&mut self.settings, key, !on);
            }
            i if (S_PRESET..S_PRESET + crate::features::PRESETS.len() as u32).contains(&i) => {
                crate::features::apply_preset(&mut self.settings, (i - S_PRESET) as usize);
            }
            i if (S_HOTKEY..S_HOTKEY + crate::features::HOTKEYS.len() as u32).contains(&i) => {
                let k = (i - S_HOTKEY) as usize;
                if self.recording == Some(k) {
                    self.stop_recording();
                } else {
                    self.start_recording(k);
                }
                self.request_frame();
                return;
            }
            i if (S_MOUSE..S_MOUSE + 30).contains(&i) => {
                let (key, opts): (&str, &[&str]) = match (i - S_MOUSE) / 10 {
                    0 => ("mouse_wheel", &["off", "volume", "brightness"]),
                    1 => ("mouse_middle", &["off", "play"]),
                    _ => ("mouse_double", &["off", "shot", "clip", "note"]),
                };
                if let Some(v) = opts.get(((i - S_MOUSE) % 10) as usize) {
                    self.settings.extra.insert(key.into(), v.to_string());
                }
            }
            S_EXPORT => self.deferred.push(Deferred::Export),
            S_IMPORT => self.deferred.push(Deferred::Import),
            S_IMPORT_NO => self.import_pending = None,
            S_IMPORT_OK => {
                if let Some(files) = self.import_pending.take() {
                    for (name, body) in files {
                        crate::store::write(&name, &body);
                    }
                    // Start again on the imported data.
                    self.deferred.push(Deferred::Restart);
                    return;
                }
            }
            S_POLICY => open_later("https://github.com/banulalakwindu/Nookly/blob/main/PRIVACY.md"),
            S_NOTIF_ON => self.settings.notif_on = !self.settings.notif_on,
            S_NOTIF_TEXT => self.settings.notif_text = !self.settings.notif_text,
            S_NOTIF_BANNERS => open_later("ms-settings:notifications"),
            S_NOTIF_UNMUTE => {
                self.settings.extra.remove(crate::modules::notifs::MUTED_KEY);
            }
            i if (S_CLIP_MODE..S_CLIP_MODE + ClipMode::ALL.len() as u32).contains(&i) => {
                self.settings.clip_mode = ClipMode::ALL[(i - S_CLIP_MODE) as usize];
            }
            S_UPDATES => {
                self.updates_clicked = self.t();
                if let Some(i) = self.modules.iter().position(|m| m.title() == "Updates") {
                    self.call(i, |m, c| m.action(crate::modules::update::A_CHECK_NOW, (0.0, 0.0), c));
                }
            }
            S_DATA => open_later(crate::store::dir().display().to_string()),
            S_GITHUB => open_later(format!("https://github.com/{}", crate::modules::update::REPO)),
            S_QUIT => self.deferred.push(Deferred::Quit),
            _ => {}
        }
        self.settings.save();
        // Modules that mirror settings (clipboard, privacy dots) pick it up now.
        self.tick();
        self.request_frame();
    }
}

// ------------------------------------------------------------ drawing bits

/// Labels that don't fit (self-test and docs runs only; None = don't measure).
type Tight = Option<Vec<String>>;

fn fit(ui: &mut Ui, tight: &mut Tight, s: &str, size: f32, w: Weight, room: f32) {
    if let Some(v) = tight {
        if ui.measure(s, size, w) > room {
            v.push(s.to_string());
        }
    }
}

/// A small grey heading over a group of controls.
fn group(ui: &mut Ui, c: &mut Rect, label: &str) {
    ui.text(label, c.cut_top(16.0, 6.0), 11.5, col::DIM, Weight::Semi, Align::Left);
}

/// An on/off switch (a dark knob that themes don't recolour).
fn switch(ui: &mut Ui, sw: Rect, on: bool, enabled: bool) {
    let r = sw.h / 2.0;
    if !enabled {
        ui.stroke_rr(sw.inset(0.6), r - 0.6, col::FAINT, 1.2);
        ui.circle(sw.x + r, sw.cy(), r - 4.0, col::FAINT);
    } else if on {
        ui.fill_rr(sw, r, col::ACCENT);
        ui.circle(sw.r() - r, sw.cy(), r - 2.0, hex(0x010101));
    } else {
        ui.fill_rr(sw, r, col::TEXT.with_a(0.18));
        ui.circle(sw.x + r, sw.cy(), r - 2.0, col::TEXT);
    }
}

/// The tab's master switch, at the top of the sidebar.
fn master_card(ui: &mut Ui, r: Rect, name: &str, state: &str, on: bool, enabled: bool, id: u32) {
    let hov = enabled && ui.hovered(r);
    let bg = if on { col::ACCENT.with_a(if hov { 0.2 } else { 0.14 }) } else { col::TEXT.with_a(if hov { 0.08 } else { 0.05 }) };
    ui.fill_rr(r, 12.0, bg);
    let mut c = r.inset_xy(12.0, 0.0);
    let sw = c.cut_right(38.0, 8.0).centered(38.0, 22.0);
    ui.text(name, Rect::new(c.x, r.y + 9.0, c.w, 19.0), 13.5, if enabled { col::TEXT } else { col::DIM }, Weight::Semi, Align::Left);
    ui.text(state, Rect::new(c.x, r.y + 28.0, c.w, 16.0), 10.5, if on { col::ACCENT } else { col::DIM }, Weight::Regular, Align::Left);
    switch(ui, sw, on, enabled);
    ui.hit_tip(r, id, "");
}

/// Tab icon: a tiny screen with a notch on top, or a taskbar along the bottom.
fn mini_screen(ui: &mut Ui, r: Rect, taskbar: bool, c: Color) {
    ui.stroke_rr(r.inset(0.6), 2.5, c, 1.2);
    if taskbar {
        ui.fill_rr(Rect::new(r.x + 2.0, r.b() - 4.5, r.w - 4.0, 2.5), 1.0, c);
    } else {
        ui.fill_rr(Rect::new(r.cx() - 3.5, r.y, 7.0, 3.5), 1.5, c);
    }
}

/// A setting with a line of explanation under it and a switch on the right;
/// the whole row is clickable.
fn switch_row(ui: &mut Ui, tight: &mut Tight, r: Rect, label: &str, desc: &str, on: bool, id: u32) {
    let hov = ui.hovered(r);
    ui.fill_rr(r, 12.0, col::TEXT.with_a(if hov { 0.075 } else { 0.045 }));
    let mut row = r.inset_xy(14.0, 0.0);
    let sw = row.cut_right(36.0, 10.0).centered(36.0, 20.0);
    ui.text(label, Rect::new(row.x, r.y + 5.0, row.w, 19.0), 13.0, col::TEXT, Weight::Semi, Align::Left);
    ui.text(desc, Rect::new(row.x, r.y + 23.0, row.w, 16.0), 11.0, col::DIM, Weight::Regular, Align::Left);
    fit(ui, tight, label, 13.0, Weight::Semi, row.w);
    fit(ui, tight, desc, 11.0, Weight::Regular, row.w);
    switch(ui, sw, on, true);
    ui.hit_tip(r, id, "");
}

/// Settings > Features: one row per feature, its icon in colour while on.
fn feature_row(ui: &mut Ui, tight: &mut Tight, row: Rect, name: &str, icon: &str, on: bool, id: u32) {
    let accent = col::ACCENT;
    let hov = ui.hovered(row);
    let bg = match (on, hov) {
        (true, false) => accent.with_a(0.13),
        (true, true) => accent.with_a(0.2),
        (false, false) => col::BTN,
        (false, true) => col::BTN_HOVER,
    };
    ui.fill_rr(row, 12.0, bg);
    // On: a warm tint, the icon in colour and a ring; off: plain and grey.
    // (No switch: the columns are narrow and the names need the room.)
    if on {
        ui.stroke_rr(row, 12.0, accent.with_a(0.45), 1.0);
    }
    let mut c = row.inset_xy(8.0, 0.0);
    ui.icon(icon, c.cut_left(14.0, 6.0), 11.0, if on { accent } else { col::DIM });
    ui.text(name, c, 11.5, if on { col::TEXT } else { col::DIM }, if on { Weight::Semi } else { Weight::Regular }, Align::Left);
    fit(ui, tight, name, 11.5, Weight::Semi, c.w);
    ui.hit_tip(row, id, "");
}

/// Settings > Theme: tile `i` (the last one is your own colours).
fn theme_tile(ui: &mut Ui, tight: &mut Tight, cell: Rect, i: usize, st: &Settings) {
    let accent = col::ACCENT;
    let custom = i == crate::gfx::THEMES.len();
    let th = if custom {
        let (bg, text) = custom_colors(st);
        crate::gfx::custom_theme(bg, text)
    } else {
        crate::gfx::THEMES[i]
    };
    let sel = st.theme == th.key;
    let saved = ui.theme;
    ui.theme = crate::gfx::THEMES[0];
    let bg = if sel { accent.with_a(0.22) } else if ui.hovered(cell) { col::BTN_HOVER } else { col::BTN };
    ui.fill_rr(cell, 10.0, bg);
    if sel {
        ui.stroke_rr(cell, 10.0, accent.with_a(0.85), 1.4);
    }
    let mut c = cell.inset(5.0);
    let screen = c.cut_top(24.0, 3.0);
    if th.glass {
        for (k, colr) in [0x7B5CFF, 0xFF8A5B, 0x3CC8C8].into_iter().enumerate() {
            let w = screen.w / 3.0;
            ui.fill_rr_raw(Rect::new(screen.x + k as f32 * w, screen.y, w + 0.5, screen.h), 0.0, hex(colr));
        }
    } else {
        ui.fill_rr_raw(screen, 5.0, hex(0x4A4A52));
    }
    let nr = Rect::new(screen.cx() - 18.0, screen.y + 4.0, 36.0, 14.0);
    ui.fill_rr_raw(nr, 7.0, th.bg);
    ui.theme = th;
    ui.text("4:20", nr, 8.0, col::TEXT, Weight::Semi, Align::Center);
    ui.theme = saved;
    let name = if custom { "Yours" } else { th.name };
    ui.text(name, c, 10.0, if sel { col::TEXT } else { col::DIM }, Weight::Semi, Align::Center);
    fit(ui, tight, name, 10.0, Weight::Semi, c.w);
    ui.hit_tip(cell, if custom { S_CUSTOM } else { S_THEME + i as u32 }, "");
}

/// What Settings shows of the Widgets panel now (its view), if anything.
fn sw_widgets_view(sw: &SettingsWin) -> Option<u8> {
    match (sw.tab, sw.section) {
        (0, SEC_WIDGETS) => Some(0),
        (0, SEC_ALERTS) if sw.alert_view == 1 => Some(1),
        (0, SEC_LOCATION) => Some(2),
        _ => None,
    }
}

/// Settings > Panels: a panel's card, with its place in the order.
fn order_card(ui: &mut Ui, tight: &mut Tight, r: Rect, n: usize, name: &str, icon: &str, acc: Color, alpha: f32) {
    let saved = ui.g.alpha;
    ui.g.alpha = saved * alpha;
    let hov = ui.hovered(r);
    ui.fill_rr(r, 12.0, col::TEXT.with_a(if hov { 0.085 } else { 0.05 }));
    if alpha < 1.0 {
        ui.fill_rr(r, 12.0, col::NOTCH.with_a(0.7));
        ui.stroke_rr(r, 12.0, col::ACCENT, 1.5);
    }
    let mut c = r.inset_xy(10.0, 0.0);
    let num = c.cut_left(14.0, 4.0);
    ui.text(&n.to_string(), num, 11.0, col::FAINT, Weight::Semi, Align::Center);
    let ic = c.cut_left(26.0, 8.0).centered(26.0, 26.0);
    ui.circle(ic.cx(), ic.cy(), 13.0, acc.with_a(0.2));
    ui.icon(icon, ic, 12.0, acc);
    ui.icon("\u{E76F}", c.cut_right(14.0, 4.0), 11.0, col::FAINT);
    ui.text(name, c, 12.5, col::TEXT, Weight::Semi, Align::Left);
    fit(ui, tight, name, 12.5, Weight::Semi, c.w);
    ui.g.alpha = saved;
}

/// Settings > Mascot: tile `k` (0 = Clock, then each mascot).
fn mascot_tile(ui: &mut Ui, tight: &mut Tight, cell: Rect, k: usize, st: &Settings, t: f64, mouse: (f32, f32)) {
    let accent = col::ACCENT;
    let selected = if k == 0 { !st.personality } else { st.personality && st.mascot == Mascot::ALL[k - 1] };
    let hov = ui.hovered(cell);
    let bg = if selected {
        accent.with_a(0.22)
    } else if hov {
        col::BTN_HOVER
    } else {
        col::BTN
    };
    ui.fill_rr(cell, 12.0, bg);
    if selected {
        ui.stroke_rr(cell, 12.0, accent.with_a(0.8), 1.2);
    }
    let mut c = cell;
    let name_r = c.cut_bottom(16.0, 0.0);
    let pic = c.inset_xy(6.0, 4.0);
    if k == 0 {
        ui.text(&Now::get().hhmm(), pic, 13.0, col::TEXT, Weight::Semi, Align::Center);
        ui.text("Clock", name_r, 10.5, if selected { col::TEXT } else { col::DIM }, Weight::Regular, Align::Center);
    } else {
        let m = Mascot::ALL[k - 1];
        let mood = if hov { Mood::Wave } else { Mood::Eyes };
        let look = gaze(pic.cx(), pic.cy(), mouse, 80.0);
        ui.push_clip(cell);
        mascot::draw(ui, pic, m, mood, t, look);
        ui.pop_clip();
        ui.text(m.label(), name_r, 10.5, if selected { col::TEXT } else { col::DIM }, Weight::Regular, Align::Center);
        fit(ui, tight, m.label(), 10.5, Weight::Regular, cell.w - 6.0);
    }
    ui.hit_tip(cell, S_MASCOT + k as u32, "");
}

/// A notch-style tile (welcome tour and Settings): a tiny screen showing
/// that style, its name and a few words.
pub(super) fn style_tile(ui: &mut Ui, cell: Rect, s: NotchStyle, sel: bool, id: u32) {
    let accent = col::ACCENT;
    let hov = ui.hovered(cell);
    let bg = if sel { accent.with_a(0.2) } else if hov { col::BTN_HOVER } else { col::BTN };
    ui.fill_rr(cell, 12.0, bg);
    if sel {
        ui.stroke_rr(cell, 12.0, accent.with_a(0.85), 1.4);
    }
    let mut c = cell.inset(7.0);
    let screen = c.cut_top(30.0, 4.0);
    ui.fill_rr(screen, 6.0, hex(0x3A3A40));
    let cx = screen.cx();
    match s {
        NotchStyle::Full => {
            let n = Rect::new(cx - 30.0, screen.y, 60.0, 13.0);
            ui.fill_rr(n, 5.0, col::NOTCH);
            ui.text("12:30", n, 8.0, col::TEXT, Weight::Semi, Align::Center);
        }
        NotchStyle::Slim => {
            let n = Rect::new(cx - 20.0, screen.y, 40.0, 13.0);
            ui.fill_rr(n, 5.0, col::NOTCH);
            ui.text("12:30", n, 8.0, col::TEXT, Weight::Semi, Align::Center);
        }
        NotchStyle::Compact => ui.fill_rr(Rect::new(cx - 17.0, screen.y, 34.0, 8.0), 4.0, col::NOTCH),
        NotchStyle::Pill => ui.fill_rr(Rect::new(cx - 15.0, screen.y + 3.0, 30.0, 3.0), 1.5, col::TEXT.with_a(0.8)),
        NotchStyle::Hidden => {
            for k in 0..5 {
                ui.fill_rect(Rect::new(cx - 18.0 + k as f32 * 8.0, screen.y + 2.0, 4.0, 1.0), col::TEXT.with_a(0.25));
            }
        }
    }
    let (name, desc) = match s {
        NotchStyle::Full => ("Full", "Clock, music, mascot"),
        NotchStyle::Slim => ("Slim", "Full, but narrower"),
        NotchStyle::Compact => ("Compact", "A smaller notch"),
        NotchStyle::Pill => ("Pill", "Just a thin bar"),
        NotchStyle::Hidden => ("Hidden", "Invisible until hovered"),
    };
    ui.text(name, c.cut_top(16.0, 0.0), 12.5, if sel { col::TEXT } else { col::DIM }, Weight::Semi, Align::Center);
    ui.text_wrap(desc, c, 10.0, col::FAINT, Weight::Regular, Align::Center);
    ui.hit_tip(cell, id, "");
}

/// The Taskbar tab: a custom taskbar is on the way.
fn draw_taskbar_soon(ui: &mut Ui, body: Rect) {
    let accent = col::ACCENT;
    let mut c = body.centered(440.0, 330.0);
    // A little screen with the notch on top and a Nookly taskbar below.
    let ill = c.cut_top(140.0, 22.0).centered(260.0, 140.0);
    ui.fill_rr_raw(ill, 16.0, hex(0x232327));
    ui.stroke_rr(ill, 16.0, col::TEXT.with_a(0.1), 1.0);
    let notch = Rect::new(ill.cx() - 34.0, ill.y, 68.0, 14.0);
    ui.fill_rr(notch, 7.0, col::NOTCH);
    ui.ellipse(notch.cx() - 6.0, notch.cy(), 1.6, 2.6, col::TEXT);
    ui.ellipse(notch.cx() + 6.0, notch.cy(), 1.6, 2.6, col::TEXT);
    let bar = Rect::new(ill.x + 14.0, ill.b() - 34.0, ill.w - 28.0, 24.0);
    ui.fill_rr(bar, 12.0, col::NOTCH);
    ui.stroke_rr(bar, 12.0, accent.with_a(0.55), 1.0);
    for k in 0..5 {
        let x = bar.cx() - 2.5 * 20.0 + 4.0 + k as f32 * 20.0;
        let ic = Rect::new(x, bar.cy() - 6.0, 12.0, 12.0);
        ui.fill_rr(ic, 3.5, if k == 2 { accent } else { col::TEXT.with_a(0.35) });
    }
    ui.text("Custom taskbar", c.cut_top(28.0, 8.0), 20.0, col::TEXT, Weight::Semi, Align::Center);
    let badge = c.cut_top(22.0, 12.0).centered(104.0, 22.0);
    ui.fill_rr(badge, 11.0, accent.with_a(0.2));
    ui.text("Coming soon", badge, 11.0, accent, Weight::Semi, Align::Center);
    ui.text_wrap(
        "A Nookly taskbar in the same style as the notch is on the way, with its own on/off switch. Meanwhile, the mascot can already live on your taskbar.",
        c.cut_top(48.0, 14.0),
        12.0,
        col::DIM,
        Weight::Regular,
        Align::Center,
    );
    let b = c.cut_top(32.0, 0.0).centered(220.0, 32.0);
    ui.button(b, "Put the mascot on the taskbar", S_GOTO_MASCOT);
}

/// What each Settings control does (shown at the bottom of the window).
pub(super) fn settings_tip(id: u32) -> &'static str {
    match id {
        S_CLOSE => "Close Settings (Esc)",
        S_MINIMIZE => "Minimize to the taskbar",
        i if (S_ORDER..S_ORDER + 64).contains(&i) => "Drag to change where this panel sits in the notch",
        S_ORDER_RESET => "Back to the original order",
        S_ALERT_VIEW => "Reminder sound, volume bar and Windows notifications",
        481 => "Off, small inside the closed notch, or a pop-up, for each kind of alert",
        S_TAB => "Everything about the notch",
        3 => "A custom taskbar: coming soon",
        S_MASTER => "Turn the whole notch on or off. Off: it hides and none of its features run",
        S_MASTER_TASKBAR => "The custom taskbar is coming soon",
        S_TURN_ON_WIDGETS => "Switch the Widgets feature on",
        S_GOTO_MASCOT => "Settings > Mascot > On the taskbar",
        S_TOUR => "Show the welcome tour again",
        i if (S_THEME..S_THEME + 10).contains(&i) => "Use this theme for the notch and every panel",
        S_SECONDS => "Show seconds on the clock (the notch gets a little wider)",
        S_STYLE => "Clock, music and mascot in a notch",
        11 => "Like Full, but narrower; the open panel is smaller too",
        12 => "A smaller notch",
        13 => "Just a thin bar until you hover",
        14 => "Invisible until you hover the edge",
        i if (S_EDGE..S_EDGE + 4).contains(&i) => "Put the notch on this screen edge",
        S_CENTER => "Move it back to the middle of the edge",
        S_MASCOT => "Show the clock instead of a mascot",
        i if (S_MASCOT + 1..S_MASCOT + 20).contains(&i) => "Use this mascot when the notch is idle",
        S_MASCOT_ALWAYS => "Keep the mascot out all the time (Full style)",
        S_TASKBAR_PET => "Move the mascot to the taskbar corner; the notch hides and its panel opens above the mascot",
        i if (S_MASCOT_POS..S_MASCOT_POS + 3).contains(&i) => "Where the mascot sits in the notch",
        i if (S_SOUND..S_SOUND + 10).contains(&i) => "Use this sound for reminders (plays it once)",
        S_STARTUP => "Start Nookly when you sign in to Windows",
        S_VOLUME => "Show a bar when you change the volume",
        S_UPDATE_AUTO => "Look for new versions twice a day",
        S_UPDATES => "Look for a new version now",
        S_DATA => "Open the folder where your data is kept",
        S_GITHUB => "Source code, releases and issues",
        S_POLICY => "What Nookly keeps and reads",
        S_QUIT => "Close Nookly (start it again from the Start menu)",
        S_NOTIF_ON => "Show other apps' Windows notifications in the notch",
        S_NOTIF_TEXT => "Off: only the app name shows",
        S_NOTIF_BANNERS => "Windows' own notification settings",
        S_NOTIF_UNMUTE => "Show the apps you muted again",
        S_PRIVACY => "A dot while an app uses the mic or camera",
        S_CLIP_ON => "Keep a history of what you copy",
        S_COPY_TOAST => "A small toast shows what you just copied (nothing is kept)",
        S_CLOCK12 => "4:20 PM instead of 16:20",
        S_MAKE_ROOM => "Over a window under it the notch slims to a thin bar you can click through, so browser tabs stay reachable",
        S_CLIP_MODE => "Record copied text only",
        71 => "Record copied text and files",
        72 => "Record text, files and pictures",
        i if (S_FEATURE..S_FEATURE + crate::features::FEATURES.len() as u32).contains(&i) => crate::features::FEATURES[(i - S_FEATURE) as usize].what,
        i if (S_PRESET..S_PRESET + crate::features::PRESETS.len() as u32).contains(&i) => crate::features::PRESETS[(i - S_PRESET) as usize].1,
        i if (S_HOTKEY..S_HOTKEY + 4).contains(&i) => "Click, then press the keys (Backspace removes it, Esc cancels)",
        S_CUSTOM => "Your own theme: pick a background and a text colour",
        i if (S_CBG..S_CBG + 8).contains(&i) => "Background colour",
        i if (S_CTEXT..S_CTEXT + 8).contains(&i) => "Text colour",
        S_CFIX => "Pick a text colour that's easy to read",
        S_GLOW => "The song's colour around the closed notch while music plays",
        S_MOTION => "No springy animations: the notch and panels change at once",
        i if (S_SIZE..S_SIZE + 3).contains(&i) => "Make the notch and panels smaller or larger",
        S_HIDEAPP_ADD => "The notch hides while this app is in front",
        i if (S_HIDEAPP..S_HIDEAPP + 10).contains(&i) => "Stop hiding for this app",
        i if (S_PROFILE..S_PROFILE + 3).contains(&i) => "Switch profile: its own features, widgets and theme",
        S_PROFILE_AUTO => "Work on weekdays 9 to 5, Home otherwise",
        i if (S_MOUSE..S_MOUSE + 30).contains(&i) => "What this does on the closed notch",
        S_EXPORT => "Save your settings and data in one file, for another PC",
        S_IMPORT => "Load settings and data saved with Export",
        S_IMPORT_OK => "Replace this PC's settings and data with the file's, and restart",
        S_IMPORT_NO => "Keep everything as it is",
        i if (S_SECTION..S_SECTION + SECTIONS.len() as u32).contains(&i) => SECTIONS[(i - S_SECTION) as usize].2,
        _ => "",
    }
}
