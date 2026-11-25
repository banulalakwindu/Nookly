//! The Settings window: a small rounded window of its own, in the notch's
//! style and theme, instead of a big panel inside the notch. Two tabs, each
//! with a master switch: Notch (every setting, in sections, plus the Widgets
//! panel) and Taskbar (the optional taskbar add-on: get it, switch it on,
//! and its own sections).
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
/// Settings > Taskbar.
pub(super) const S_BAR_SECTION: u32 = 700; // + index into BAR_SECTIONS
pub(super) const S_BAR_GET: u32 = 710;
pub(super) const S_BAR_REMOVE: u32 = 711;
pub(super) const S_BAR_AUTOHIDE: u32 = 712;
pub(super) const S_BAR_HOVER: u32 = 713;
pub(super) const S_BAR_FOLLOW: u32 = 714;
pub(super) const S_BAR_ALIGN: u32 = 715; // + 0 centre, 1 left
pub(super) const S_BAR_SCREENS: u32 = 718;
pub(super) const S_BAR_SIZE: u32 = 720; // + 0 smaller, 1 normal, 2 larger
pub(super) const S_BAR_UP: u32 = 730; // + index into barcfg::ITEMS
pub(super) const S_BAR_DOWN: u32 = 740; // + index into barcfg::ITEMS
pub(super) const S_BAR_SHOW: u32 = 750; // + index into barcfg::ITEMS
pub(super) const S_BAR_THEME: u32 = 760; // + index into THEMES
pub(super) const S_BAR_VIEW: u32 = 780; // + 0 previews, 1 list
pub(super) const S_BAR_MAX: u32 = 785; // + 0 Fit, 1 fewer, 2 more
pub(super) const S_BAR_MORE: u32 = 795; // + 0 dots, 1 stacked icons
/// Settings > Taskbar > Widgets: + index into the widget catalog.
pub(super) const S_BAR_WID_UP: u32 = 800;
pub(super) const S_BAR_WID_DOWN: u32 = 850;
pub(super) const S_BAR_WID_SHOW: u32 = 900; // (+ position: take it off)
/// + position x 4 + size (Small, Medium, Large, Wide).
pub(super) const S_BAR_WID_SIZE: u32 = 4000;
/// + index into barcfg::CATALOG.
pub(super) const S_BAR_WID_ADD: u32 = 4300;
/// + index into barcfg::GROUPS (the Add widgets page's filter).
pub(super) const S_BAR_WID_GROUP: u32 = 4400;
/// + position: pin it left, right or not at all (each click moves on).
pub(super) const S_BAR_WID_PIN: u32 = 4600;
/// The grips to drag by: + place in Settings > Taskbar > Layout's list, and
/// + place in Widgets' list.
pub(super) const S_BAR_GRIP: u32 = 4700;
pub(super) const S_WID_GRIP: u32 = 4800;
/// Settings > Taskbar > Apps > most apps shown: Fit (as many as fit, the
/// default) or any number from 1 to 60; leaving Fit starts at 8.
const BAR_MAX_START: usize = 8;
const BAR_MAX_TOP: usize = 60;
/// The Taskbar tab's sections.
/// Settings > Taskbar > Other screens: + index into barcfg::ITEMS, and (from
/// 20) + a widget's place.
pub(super) const S_BAR_OTHER: u32 = 5400;
pub(super) const BAR_OTHER: u8 = 5;
pub(super) const BAR_SECTIONS: [(&str, &str, &str); 6] = [
    ("\u{E771}", "General", "How the taskbar shows and behaves"),
    ("\u{E8FD}", "Layout", "Everything on the taskbar, in order: pin it at an end or let it scroll in the middle"),
    ("\u{ECAA}", "Apps", "Hovering an app, how many show, and collections"),
    ("\u{E790}", "Theme", "The same as the notch, or a theme of the taskbar's own"),
    ("\u{E710}", "Add widgets", "Every widget there is: click one to put it on the taskbar, again to take it off"),
    ("\u{E7F4}", "Other screens", "A taskbar on your other screens, and what it shows there"),
];
/// Settings > Taskbar > Theme: Yours, its colours, and making them readable.
pub(super) const S_BAR_CUSTOM: u32 = 3900;
pub(super) const S_BAR_CBG: u32 = 3910; // + index into CUSTOM_BG
pub(super) const S_BAR_CTEXT: u32 = 3920; // + index into CUSTOM_TEXT
pub(super) const S_BAR_CFIX: u32 = 3930;
/// Settings > Taskbar > Layout: + place in the layout x 3 + zone (left,
/// middle, right).
pub(super) const S_LAY_ZONE: u32 = 5000;
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
    ("\u{ECA5}", "Live & status", "What the closed notch shows: live activities while they happen, and status values"),
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
    /// Settings > Taskbar > Layout or Widgets: the row being dragged by its
    /// grip (which list, its place), and where each row was drawn.
    list_drag: Option<(u32, usize)>,
    list_cells: Vec<Rect>,
    /// What each of those rows is: (place to drop at, zone, a real row).
    list_slots: Vec<(usize, crate::barcfg::Zone, bool)>,
    /// A note under the Layout list (pinning refused) and when it was said.
    bar_note: Option<(&'static str, f64)>,
    /// Settings > Taskbar > Widgets: how far its list is scrolled, and how
    /// far it can go.
    list_scroll: f32,
    list_max: f32,
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
    /// The Taskbar tab's section (index into BAR_SECTIONS).
    pub bar_section: u8,
    /// Settings > Taskbar > Add widgets: which kind it lists.
    pub bar_group: u8,
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
                list_drag: None,
                list_cells: Vec::new(),
                list_slots: Vec::new(),
                bar_note: None,
                list_scroll: 0.0,
                list_max: 0.0,
                alert_view: 0,
                hover_id: None,
                pos: (x, y),
                scale,
                tab: 0,
                section: 0,
                bar_section: 0,
                bar_group: 0,
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

    /// Open Settings on the Taskbar tab.
    pub fn open_settings_taskbar(&mut self) {
        self.open_settings();
        if let Some(sw) = self.swin.as_mut() {
            sw.tab = 1;
        }
        self.sw_render();
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
            i if (S_BAR_SECTION..S_BAR_SECTION + BAR_SECTIONS.len() as u32).contains(&i) => {
                sw.bar_section = (i - S_BAR_SECTION) as u8;
                sw.list_scroll = 0.0;
            }
            i if (S_BAR_WID_GROUP..S_BAR_WID_GROUP + crate::barcfg::GROUPS.len() as u32).contains(&i) => sw.bar_group = (i - S_BAR_WID_GROUP) as u8,
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
                if sw.order_drag.is_some() || sw.list_drag.is_some() {
                    self.sw_render();
                    return Some(LRESULT(0));
                }
                // A grip in Settings > Taskbar that moves: drag its row.
                if let Some((h, p0)) = sw.press {
                    let grip = [S_BAR_GRIP, S_WID_GRIP].into_iter().find(|b| (*b..*b + 64).contains(&h.id));
                    if let Some(base) = grip.filter(|_| h.module == SETTINGS && (xy.1 - p0.1).abs() + (xy.0 - p0.0).abs() > 4.0) {
                        sw.press = None;
                        sw.list_drag = Some((base, (h.id - base) as usize));
                        unsafe {
                            SetCapture(hwnd);
                        }
                        self.sw_render();
                        return Some(LRESULT(0));
                    }
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
                if let Some((base, from)) = sw.list_drag.take() {
                    unsafe {
                        let _ = ReleaseCapture();
                    }
                    // Dropped on the row nearest the mouse (into its zone).
                    let near = sw.list_cells.iter().enumerate().min_by(|a, b| (a.1.cy() - xy.1).abs().partial_cmp(&(b.1.cy() - xy.1).abs()).unwrap_or(std::cmp::Ordering::Equal)).map(|(k, _)| k);
                    if let Some(&(at, zone, row)) = near.and_then(|j| sw.list_slots.get(j)) {
                        let to = if row || at <= from { at } else { at - 1 };
                        let was = crate::barcfg::layout(&self.settings).get(from).map(|e| e.zone);
                        let no_room = zone != crate::barcfg::Zone::Mid && was == Some(crate::barcfg::Zone::Mid) && crate::barcfg::pinned_width(&self.settings, from, zone) > crate::barcfg::pin_room(bar_room(&self.settings));
                        if no_room {
                            sw.bar_note = Some(("No room to pin it there: unpin something, or make a widget smaller, first", crate::time::unix_now()));
                        } else if base == S_BAR_GRIP && (to != from || was != Some(zone)) {
                            crate::barcfg::move_entry(&mut self.settings, from, to, zone);
                            self.settings.save();
                            self.bar_notify();
                        }
                    }
                    self.sw_render();
                    return Some(LRESULT(0));
                }
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
                // Settings > Taskbar > Widgets: its list scrolls.
                if sw.tab == 1 && matches!(sw.bar_section, 1 | BAR_OTHER) && sw.list_max > 0.0 {
                    sw.list_scroll = (sw.list_scroll - d * 82.0).clamp(0.0, sw.list_max);
                    self.sw_render();
                    return Some(LRESULT(0));
                }
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
        let (tab, section, bar_section, bar_group) = (sw.tab, sw.section, sw.bar_section, sw.bar_group);
        let bar_installed = bar_exe().is_some();
        let bar_on = bar_installed && x("taskbar_on");
        let bar_widgets = if tab == 1 && bar_section >= 3 { self.bar_features_off() } else { Vec::new() };
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
        let list_drag = sw.list_drag;
        let mut list_scroll = sw.list_scroll;
        let mut list_max = 0.0f32;
        let mut list_cells: Vec<Rect> = Vec::new();
        let mut list_slots: Vec<(usize, crate::barcfg::Zone, bool)> = Vec::new();
        let bar_note = sw.bar_note.filter(|n| crate::time::unix_now() - n.1 < 5.0).map(|n| n.0);
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
                let badge = 0.0;
                let mut r = cell.centered(18.0 + 7.0 + lw + badge, cell.h);
                mini_screen(&mut ui, r.cut_left(18.0, 7.0).centered(18.0, 13.0), k == 1, if sel { accent } else { fg });
                ui.text(name, r.cut_left(lw + 1.0, 5.0), 12.5, fg, Weight::Semi, Align::Left);
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
                let state = if !bar_installed {
                    "Not installed"
                } else if bar_on {
                    "On"
                } else {
                    "Off: Windows' own taskbar"
                };
                master_card(&mut ui, card, "Taskbar", state, bar_on, bar_installed, S_MASTER_TASKBAR);
            }
            if tab == 1 {
                if bar_installed {
                    for (k, (g, name, _)) in BAR_SECTIONS.iter().enumerate() {
                        let r = list.cut_top(31.0, 2.0);
                        let sel = bar_section == k as u8;
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
                        ui.hit_tip(r, S_BAR_SECTION + k as u32, "");
                    }
                    draw_taskbar_tab(&mut ui, &mut tight, body, &st, bar_section, bar_on, &bar_widgets, bar_group, list_drag, (&mut list_cells, &mut list_slots), (&mut list_scroll, &mut list_max), bar_note);
                } else {
                    ui.text_wrap("An optional add-on: get it on the right if you'd like it.", list.inset_xy(6.0, 4.0), 11.0, col::FAINT, Weight::Regular, Align::Left);
                    draw_taskbar_get(&mut ui, body);
                }
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
                        theme_picker(&mut ui, &mut tight, &mut c, &st, &st.theme, NOTCH_PICK);
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
                            ui.text("Live & status is off", m.cut_top(24.0, 4.0), 15.0, col::TEXT, Weight::Semi, Align::Center);
                            ui.text_wrap("Switch it on to pick what the closed notch shows: songs and timers while they happen, battery, weather, Wi-Fi and more.", m.cut_top(36.0, 10.0), 11.5, col::DIM, Weight::Regular, Align::Center);
                            ui.button_accent(m.cut_top(30.0, 0.0).centered(180.0, 30.0), "Turn it on", S_TURN_ON_WIDGETS, accent);
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
        sw.list_cells = list_cells;
        sw.list_slots = list_slots;
        sw.list_scroll = list_scroll;
        sw.list_max = list_max;
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
            S_MASTER_TASKBAR => {
                if bar_exe().is_some() {
                    let on = crate::barcfg::flag(&self.settings, "taskbar_on", false);
                    self.settings.extra.insert("taskbar_on".into(), if on { "0" } else { "1" }.into());
                    self.bar_fails = 0;
                    self.bar_retry = 0.0;
                }
            }
            S_BAR_AUTOHIDE | S_BAR_HOVER | S_BAR_SCREENS => {
                let (key, default) = match id {
                    S_BAR_AUTOHIDE => ("bar_autohide", false),
                    S_BAR_HOVER => ("bar_hover", true),
                    _ => ("bar_screens", true),
                };
                let on = crate::barcfg::flag(&self.settings, key, default);
                self.settings.extra.insert(key.into(), if on { "0" } else { "1" }.into());
            }
            S_BAR_FOLLOW => {
                let follow = !self.settings.extra.get("bar_theme").is_some_and(|k| !k.is_empty() && k != "follow");
                let v = if follow { self.settings.theme.clone() } else { "follow".to_string() };
                self.settings.extra.insert("bar_theme".into(), v);
            }
            i if (S_BAR_THEME..S_BAR_THEME + crate::gfx::THEMES.len() as u32).contains(&i) => {
                self.settings.extra.insert("bar_theme".into(), crate::gfx::THEMES[(i - S_BAR_THEME) as usize].key.into());
            }
            S_BAR_CUSTOM => {
                // Yours starts from the notch's own colours.
                if !self.settings.extra.contains_key("bar_custom_bg") {
                    let (bg, text) = super::custom_colors(&self.settings);
                    self.settings.extra.insert("bar_custom_bg".into(), format!("{bg:06X}"));
                    self.settings.extra.insert("bar_custom_text".into(), format!("{text:06X}"));
                }
                self.settings.extra.insert("bar_theme".into(), "custom_bar".into());
            }
            i if (S_BAR_CBG..S_BAR_CBG + 8).contains(&i) => {
                self.settings.extra.insert("bar_custom_bg".into(), format!("{:06X}", CUSTOM_BG[(i - S_BAR_CBG) as usize]));
            }
            i if (S_BAR_CTEXT..S_BAR_CTEXT + 8).contains(&i) => {
                self.settings.extra.insert("bar_custom_text".into(), format!("{:06X}", CUSTOM_TEXT[(i - S_BAR_CTEXT) as usize]));
            }
            S_BAR_CFIX => {
                let (bg, _) = super::custom_colors_of(&self.settings, "bar_");
                let w = crate::gfx::contrast(hex(bg), hex(0xFFFFFF));
                let b = crate::gfx::contrast(hex(bg), hex(0x111111));
                self.settings.extra.insert("bar_custom_text".into(), if w >= b { "FFFFFF" } else { "111111" }.into());
            }
            i if (S_BAR_ALIGN..S_BAR_ALIGN + 2).contains(&i) => {
                self.settings.extra.insert("bar_align".into(), if i == S_BAR_ALIGN { "center" } else { "left" }.into());
            }
            i if (S_BAR_WID_SHOW..S_BAR_WID_SHOW + 50).contains(&i) => crate::barcfg::remove_widget(&mut self.settings, (i - S_BAR_WID_SHOW) as usize),
            i if (S_LAY_ZONE..S_LAY_ZONE + 3 * 64).contains(&i) => {
                let (k, z) = (((i - S_LAY_ZONE) / 3) as usize, crate::barcfg::Zone::ALL[((i - S_LAY_ZONE) % 3) as usize]);
                let was = crate::barcfg::layout(&self.settings).get(k).map(|e| e.zone);
                if z != crate::barcfg::Zone::Mid && was == Some(crate::barcfg::Zone::Mid) && crate::barcfg::pinned_width(&self.settings, k, z) > crate::barcfg::pin_room(bar_room(&self.settings)) {
                    if let Some(sw) = self.swin.as_mut() {
                        sw.bar_note = Some(("No room to pin it: unpin something, or make a widget smaller, first", crate::time::unix_now()));
                    }
                } else {
                    crate::barcfg::set_zone(&mut self.settings, k, z);
                }
            }
            i if (S_BAR_WID_PIN..S_BAR_WID_PIN + 50).contains(&i) => {
                use crate::barcfg::Pin;
                let pos = (i - S_BAR_WID_PIN) as usize;
                if let Some(p) = crate::barcfg::placed(&self.settings).get(pos) {
                    let next = match p.pin {
                        Pin::No => Pin::Left,
                        Pin::Left => Pin::Right,
                        Pin::Right => Pin::No,
                    };
                    crate::barcfg::pin_widget(&mut self.settings, pos, next);
                }
            }
            i if (S_BAR_WID_SIZE..S_BAR_WID_SIZE + 256).contains(&i) => {
                let k = i - S_BAR_WID_SIZE;
                crate::barcfg::resize_widget(&mut self.settings, (k / 4) as usize, crate::barcfg::Size::ALL[(k % 4) as usize]);
            }
            i if (S_BAR_WID_ADD..S_BAR_WID_ADD + crate::barcfg::CATALOG.len() as u32).contains(&i) => {
                // Click to put it on; click again to take it off.
                let d = &crate::barcfg::CATALOG[(i - S_BAR_WID_ADD) as usize];
                match crate::barcfg::placed(&self.settings).iter().position(|p| p.id == d.id) {
                    Some(pos) => crate::barcfg::remove_widget(&mut self.settings, pos),
                    None if !self.bar_features_off().contains(&d.feature) => crate::barcfg::add_widget(&mut self.settings, d.id),
                    None => {}
                }
            }
            i if (S_BAR_VIEW..S_BAR_VIEW + 2).contains(&i) => {
                self.settings.extra.insert("bar_hover_view".into(), if i == S_BAR_VIEW { "preview" } else { "list" }.into());
            }
            i if (S_BAR_MAX..S_BAR_MAX + 4).contains(&i) => {
                let now: usize = self.settings.extra.get("bar_max_apps").and_then(|v| v.parse().ok()).unwrap_or(0);
                let n = match i - S_BAR_MAX {
                    0 => 0,
                    1 if now == 0 => BAR_MAX_START,
                    1 => now.saturating_sub(1).max(1),
                    2 if now == 0 => BAR_MAX_START,
                    2 => (now + 1).min(BAR_MAX_TOP),
                    _ if now == 0 => BAR_MAX_START,
                    _ => now,
                };
                self.settings.extra.insert("bar_max_apps".into(), n.to_string());
            }
            i if (S_BAR_MORE..S_BAR_MORE + 2).contains(&i) => {
                self.settings.extra.insert("bar_overflow".into(), if i == S_BAR_MORE { "dots" } else { "stack" }.into());
            }
            i if (S_BAR_SIZE..S_BAR_SIZE + 3).contains(&i) => {
                self.settings.extra.insert("bar_size".into(), ["s", "m", "l"][(i - S_BAR_SIZE) as usize].into());
            }
            i if (S_BAR_SHOW..S_BAR_SHOW + crate::barcfg::ITEMS.len() as u32).contains(&i) => {
                let key = crate::barcfg::ITEMS[(i - S_BAR_SHOW) as usize].0;
                let on = crate::barcfg::order(&self.settings).iter().any(|x| x.0 == key && x.1);
                crate::barcfg::set_shown(&mut self.settings, key, !on);
            }
            i if (S_BAR_OTHER..S_BAR_OTHER + 20).contains(&i) => {
                if let Some(x) = crate::barcfg::ITEMS.get((i - S_BAR_OTHER) as usize) {
                    crate::barcfg::other_toggle(&mut self.settings, x.0);
                }
            }
            i if (S_BAR_OTHER + 20..S_BAR_OTHER + 100).contains(&i) => {
                if let Some(p) = crate::barcfg::placed(&self.settings).get((i - S_BAR_OTHER - 20) as usize).cloned() {
                    crate::barcfg::other_toggle(&mut self.settings, &format!("@{}", p.id));
                }
            }
            S_BAR_GET => self.get_bar(),
            S_BAR_REMOVE => self.remove_bar(),
            S_DATA => open_later(crate::store::dir().display().to_string()),
            S_GITHUB => open_later(format!("https://github.com/{}", crate::modules::update::REPO)),
            S_QUIT => self.deferred.push(Deferred::Quit),
            _ => {}
        }
        self.settings.save();
        self.bar_notify();
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

/// Which theme picker: the notch's or the taskbar's (their ids, the key
/// "Yours" saves as, and where its colours are kept).
#[derive(Clone, Copy)]
struct Pick {
    theme: u32,
    custom: u32,
    bg: u32,
    text: u32,
    fix: u32,
    custom_key: &'static str,
    prefix: &'static str,
}

const NOTCH_PICK: Pick = Pick { theme: S_THEME, custom: S_CUSTOM, bg: S_CBG, text: S_CTEXT, fix: S_CFIX, custom_key: "custom", prefix: "" };
const BAR_PICK: Pick = Pick { theme: S_BAR_THEME, custom: S_BAR_CUSTOM, bg: S_BAR_CBG, text: S_BAR_CTEXT, fix: S_BAR_CFIX, custom_key: "custom_bar", prefix: "bar_" };

/// The theme cards (every theme, then Yours), and with Yours its colours
/// and how readable they are: the same picker for the notch and the taskbar.
fn theme_picker(ui: &mut Ui, tight: &mut Tight, c: &mut Rect, st: &Settings, sel: &str, p: Pick) {
    let accent = col::ACCENT;
    let per = 6;
    let n = crate::gfx::THEMES.len() + 1;
    let n_rows = n.div_ceil(per);
    let grid = c.cut_top(n_rows as f32 * 50.0 + (n_rows - 1) as f32 * 6.0, 12.0);
    let rows = grid.rows(n_rows, 6.0);
    for i in 0..n {
        let cell = rows[i / per].columns(per, 6.0)[i % per];
        theme_tile(ui, tight, cell, i, st, sel, p);
    }
    if sel == p.custom_key {
        let (bg, text) = super::custom_colors_of(st, p.prefix);
        for (label, list, base, cur) in [("Background", &CUSTOM_BG, p.bg, bg), ("Text", &CUSTOM_TEXT, p.text, text)] {
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
            ui.button_c(row.cut_right(70.0, 6.0), "Fix it", p.fix, col::RED.with_a(0.2), col::RED);
        }
        let verdict = if ratio >= 7.0 { "(great)" } else if ratio >= 4.5 { "(readable)" } else { "(too low to read well)" };
        ui.text(&format!("Contrast {ratio:.1}:1 {verdict}"), row, 11.0, if ratio >= 4.5 { col::DIM } else { col::RED }, Weight::Regular, Align::Left);
    }
}

/// Settings > Theme: tile `i` (the last one is your own colours).
fn theme_tile(ui: &mut Ui, tight: &mut Tight, cell: Rect, i: usize, st: &Settings, sel_key: &str, p: Pick) {
    let accent = col::ACCENT;
    let custom = i == crate::gfx::THEMES.len();
    let th = if custom {
        let (bg, text) = super::custom_colors_of(st, p.prefix);
        crate::gfx::custom_theme(bg, text)
    } else {
        crate::gfx::THEMES[i]
    };
    let sel = if custom { sel_key == p.custom_key } else { sel_key == th.key };
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
    ui.hit_tip(cell, if custom { p.custom } else { p.theme + i as u32 }, "");
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

/// A little screen with the notch on top and a Nookly taskbar below.
fn taskbar_picture(ui: &mut Ui, ill: Rect) {
    let accent = col::ACCENT;
    ui.fill_rr_raw(ill, 16.0, hex(0x232327));
    ui.stroke_rr(ill, 16.0, col::TEXT.with_a(0.1), 1.0);
    let notch = Rect::new(ill.cx() - 34.0, ill.y, 68.0, 14.0);
    ui.fill_rr(notch, 7.0, col::NOTCH);
    ui.ellipse(notch.cx() - 6.0, notch.cy(), 1.6, 2.6, col::TEXT);
    ui.ellipse(notch.cx() + 6.0, notch.cy(), 1.6, 2.6, col::TEXT);
    let bar = Rect::new(ill.cx() - 80.0, ill.b() - 22.0, 160.0, 22.0);
    ui.fill_rr(bar, 9.0, col::NOTCH);
    ui.fill_rect(Rect::new(bar.x, bar.b() - 9.0, bar.w, 9.0), col::NOTCH);
    for k in 0..6 {
        let x = bar.x + 14.0 + k as f32 * 23.0;
        let ic = Rect::new(x, bar.cy() - 5.0, 11.0, 11.0);
        ui.fill_rr(ic, 3.0, if k == 0 { col::BLUE } else if k == 3 { accent } else { col::TEXT.with_a(0.35) });
    }
}

/// The Taskbar tab before the add-on is installed: what it is, and a
/// button to get it.
fn draw_taskbar_get(ui: &mut Ui, body: Rect) {
    let accent = col::ACCENT;
    let mut c = body.centered(440.0, 360.0);
    taskbar_picture(ui, c.cut_top(140.0, 22.0).centered(260.0, 140.0));
    ui.text("Nookly Taskbar", c.cut_top(28.0, 8.0), 20.0, col::TEXT, Weight::Semi, Align::Center);
    let badge = c.cut_top(22.0, 12.0).centered(118.0, 22.0);
    ui.fill_rr(badge, 11.0, accent.with_a(0.2));
    ui.text("Optional add-on", badge, 11.0, accent, Weight::Semi, Align::Center);
    ui.text_wrap(
        "A taskbar in the notch's shape and theme: Start, search, your apps, widgets (clock, date, weather, music, system...) and notifications. It's not part of the Nookly download, so Nookly stays small: get it only if you want it. It runs only while it's switched on.",
        c.cut_top(64.0, 14.0),
        12.0,
        col::DIM,
        Weight::Regular,
        Align::Center,
    );
    let b = c.cut_top(34.0, 10.0).centered(200.0, 34.0);
    match crate::modules::update::ADDON.lock().unwrap().clone() {
        crate::modules::update::AddOn::Busy => {
            ui.fill_rr(b, 17.0, accent.with_a(0.2));
            ui.text("Downloading\u{2026}", b, 12.5, accent, Weight::Semi, Align::Center);
        }
        crate::modules::update::AddOn::Failed(why) => {
            ui.button_accent(b, "Try again", S_BAR_GET, accent);
            ui.text(&format!("Couldn't get it: {why}"), c.cut_top(18.0, 0.0), 11.0, col::RED, Weight::Regular, Align::Center);
        }
        crate::modules::update::AddOn::Idle => {
            ui.button_accent(b, "Download taskbar", S_BAR_GET, accent);
            ui.text("Under 1 MB, from this version's GitHub release (checked before it's installed)", c.cut_top(18.0, 0.0), 11.0, col::FAINT, Weight::Regular, Align::Center);
        }
    }
}

/// The Taskbar tab's sections, once the add-on is installed.
/// A row's grip (six dots) to drag it by.
fn grip(ui: &mut Ui, r: Rect, id: u32, active: bool) {
    let c = if active || ui.hovered(r) { col::TEXT } else { col::FAINT };
    for (dx, dy) in [(-2.5, -4.5), (2.5, -4.5), (-2.5, 0.0), (2.5, 0.0), (-2.5, 4.5), (2.5, 4.5)] {
        ui.circle(r.cx() + dx, r.cy() + dy, 1.4, c);
    }
    ui.hit_tip(r, id, "");
}

/// While a row is dragged: where it would land (a line in the accent
/// colour) and the row itself following the mouse, drawn by `row`.
fn drag_ghost(ui: &mut Ui, cells: &[Rect], from: usize, accent: Color, row: impl FnOnce(&mut Ui, Rect)) {
    let my = ui.mouse.1;
    let Some(to) = cells.iter().enumerate().min_by(|a, b| (a.1.cy() - my).abs().partial_cmp(&(b.1.cy() - my).abs()).unwrap_or(std::cmp::Ordering::Equal)).map(|(k, _)| k) else { return };
    let Some(src) = cells.get(from) else { return };
    if to != from {
        let t = cells[to];
        let y = if to > from { t.b() + 2.0 } else { t.y - 3.0 };
        ui.fill_rr(Rect::new(t.x + 4.0, y - 1.0, t.w - 8.0, 2.5), 1.25, accent);
    }
    let r = Rect::new(src.x, (my - src.h / 2.0).clamp(cells[0].y - 10.0, cells[cells.len() - 1].b() - src.h + 10.0), src.w, src.h);
    // Solid underneath (it covers the rows it passes over), with a soft
    // shadow.
    ui.fill_rr(Rect::new(r.x, r.y + 3.0, r.w, r.h), 13.0, hex(0x000000).with_a(0.35));
    ui.fill_rr(r, 11.0, col::NOTCH);
    row(ui, r);
}

fn draw_taskbar_tab(ui: &mut Ui, tight: &mut Tight, body: Rect, st: &Settings, section: u8, on: bool, off: &[&'static str], group_sel: u8, drag: Option<(u32, usize)>, (cells, slots): (&mut Vec<Rect>, &mut Vec<(usize, crate::barcfg::Zone, bool)>), scroll: (&mut f32, &mut f32), note: Option<&str>) {
    let accent = col::ACCENT;
    let mut c = body;
    let (_, title, sub) = BAR_SECTIONS[section as usize % BAR_SECTIONS.len()];
    ui.text(title, c.cut_top(26.0, 0.0), 19.0, col::TEXT, Weight::Semi, Align::Left);
    let subr = c.cut_top(18.0, 14.0);
    if on {
        ui.text(sub, subr, 11.5, col::DIM, Weight::Regular, Align::Left);
    } else {
        ui.text("The taskbar is off. These apply when you turn it on (switch on the left).", subr, 11.5, col::RED, Weight::Regular, Align::Left);
    }
    match section {
        0 => {
            let autohide = crate::barcfg::flag(st, "bar_autohide", false);
            let hover = crate::barcfg::flag(st, "bar_hover", true);
            switch_row(ui, tight, c.cut_top(ROW_H, 8.0), "Auto-hide", "Slides away; touch the bottom edge of the screen to bring it back", autohide, S_BAR_AUTOHIDE);
            switch_row(ui, tight, c.cut_top(ROW_H, 8.0), "Open on hover", "Off: lists and pop-ups open only when you click", hover, S_BAR_HOVER);
            c.cut_top(6.0, 0.0);
            let mut row = c.cut_top(30.0, 10.0);
            let left = row.cut_left((row.w - 12.0) / 2.0, 12.0);
            let align_left = st.extra.get("bar_align").is_some_and(|v| v == "left");
            for (i, (cell, name)) in left.columns(2, 6.0).into_iter().zip(["Centre", "Left"]).enumerate() {
                ui.chip(cell, name, S_BAR_ALIGN + i as u32, align_left == (i == 1), accent);
            }
            let size = match st.extra.get("bar_size").map(String::as_str) {
                Some("s") => 0,
                Some("l") => 2,
                _ => 1,
            };
            for (i, (cell, name)) in row.columns(3, 6.0).into_iter().zip(["Smaller", "Normal", "Larger"]).enumerate() {
                ui.chip(cell, name, S_BAR_SIZE + i as u32, size == i, accent);
            }
            let b = c.cut_bottom(30.0, 0.0);
            let rm = Rect::new(b.r() - 150.0, b.y, 150.0, 30.0);
            ui.button(rm, "Remove taskbar", S_BAR_REMOVE);
            ui.text("The add-on runs only while it's on.", Rect::new(b.x, b.y, b.w - 160.0, 30.0), 11.0, col::FAINT, Weight::Regular, Align::Left);
        }
        3 => {
            // The theme: the notch's, or one of its own (the same picker).
            let own = st.extra.get("bar_theme").filter(|k| !k.is_empty() && *k != "follow").cloned();
            switch_row(ui, tight, c.cut_top(ROW_H, 14.0), "Same theme as the notch", "Off: pick the taskbar's own theme below", own.is_none(), S_BAR_FOLLOW);
            match own {
                Some(k) => {
                    group(ui, &mut c, "Theme  \u{00B7}  the taskbar and its pop-ups");
                    theme_picker(ui, tight, &mut c, st, &k, BAR_PICK);
                }
                None => {
                    let name = if st.theme == "custom" { "Yours" } else { crate::gfx::THEMES.iter().find(|t| t.key == st.theme).map_or("Default", |t| t.name) };
                    ui.text(&format!("Now: {name}, as the notch (Settings > Notch > Theme)."), c.cut_top(20.0, 0.0), 11.5, col::DIM, Weight::Regular, Align::Left);
                }
            }
        }
        BAR_OTHER => {
            use crate::barcfg::{ITEMS, def, other_screens, other_shows, placed};
            let on = other_screens(st);
            switch_row(ui, tight, c.cut_top(ROW_H, 12.0), "Taskbar on other screens", "Off: only on the main screen", on, S_BAR_SCREENS);
            let foot = c.cut_bottom(32.0, 8.0);
            let view = c;
            // Two columns of switches: the taskbar's own things, the widgets.
            let w = placed(st);
            let row_h = 40.0;
            let n_items = ITEMS.len().div_ceil(2) as f32;
            let content = 22.0 + n_items * row_h + 8.0 + 22.0 + (w.len().div_ceil(2).max(1)) as f32 * row_h;
            *scroll.1 = (content - view.h).max(0.0);
            *scroll.0 = scroll.0.clamp(0.0, *scroll.1);
            let hits_from = ui.hits.len();
            ui.push_clip(Rect::new(view.x - 4.0, view.y, view.w + 8.0, view.h));
            let mut y = Rect::new(view.x, view.y - *scroll.0, view.w - 10.0, content);
            let cell = |ui: &mut Ui, r: Rect, icon: &str, name: &str, sel: bool, id: u32| {
                let sel = sel && on;
                let bg = if sel {
                    accent.with_a(0.13)
                } else if ui.hovered(r) {
                    col::BTN_HOVER
                } else {
                    col::BTN
                };
                ui.fill_rr(r, 11.0, bg);
                let mut i = r.inset_xy(12.0, 0.0);
                ui.icon(icon, i.cut_left(20.0, 10.0), 13.0, if sel { accent } else { col::FAINT });
                let s = i.cut_right(36.0, 8.0).centered(36.0, 20.0);
                switch(ui, s, sel, on);
                ui.text(name, i, 12.0, if sel { col::TEXT } else { col::DIM }, Weight::Semi, Align::Left);
                if on {
                    ui.hit_tip(r, id, "");
                }
            };
            group(ui, &mut y, "From the taskbar");
            for (k, chunk) in ITEMS.chunks(2).enumerate() {
                for (j, (r, it)) in y.cut_top(row_h - 6.0, 6.0).columns(2, 6.0).into_iter().zip(chunk).enumerate() {
                    cell(ui, r, it.2, it.1, other_shows(st, it.0), S_BAR_OTHER + (k * 2 + j) as u32);
                }
            }
            y.cut_top(8.0, 0.0);
            group(ui, &mut y, "Widgets on the taskbar");
            if w.is_empty() {
                ui.text("None yet: pick some in Add widgets.", y.cut_top(row_h - 6.0, 6.0), 11.5, col::FAINT, Weight::Regular, Align::Left);
            }
            for (k, chunk) in w.chunks(2).enumerate() {
                for (j, (r, p)) in y.cut_top(row_h - 6.0, 6.0).columns(2, 6.0).into_iter().zip(chunk).enumerate() {
                    let (icon, name) = def(&p.id).map_or(("", ""), |d| (d.icon, d.name));
                    cell(ui, r, icon, name, other_shows(st, &format!("@{}", p.id)), S_BAR_OTHER + 20 + (k * 2 + j) as u32);
                }
            }
            ui.pop_clip();
            for h in ui.hits[hits_from..].iter_mut() {
                let (y0, y1) = (h.rect.y.max(view.y), h.rect.b().min(view.b()));
                h.rect = Rect::new(h.rect.x, y0, h.rect.w, (y1 - y0).max(0.0));
            }
            if *scroll.1 > 0.0 {
                let track = Rect::new(view.r() - 4.0, view.y, 3.0, view.h);
                let th = (view.h * view.h / content).max(24.0);
                let ty = track.y + (*scroll.0 / *scroll.1) * (track.h - th);
                ui.fill_rr(track, 1.5, col::TEXT.with_a(0.06));
                ui.fill_rr(Rect::new(track.x, ty, 3.0, th), 1.5, col::TEXT.with_a(0.3));
            }
            ui.text_wrap("The order and sizes are the main taskbar's (Layout). Start, hidden icons and the rest can be off on the main one and on here, or the other way round.", foot, 11.0, col::FAINT, Weight::Regular, Align::Left);
        }
        4 => {
            // Every widget there is, by kind: click to add.
            use crate::barcfg::{CATALOG, GROUPS};
            let g = GROUPS[group_sel as usize % GROUPS.len()];
            let row = c.cut_top(28.0, 12.0);
            for (k, (cell, name)) in row.columns(GROUPS.len(), 6.0).into_iter().zip(GROUPS).enumerate() {
                ui.chip(cell, name, S_BAR_WID_GROUP + k as u32, name == g, accent);
            }
            let placed = crate::barcfg::placed(st);
            let items: Vec<(usize, &crate::barcfg::Def)> = CATALOG.iter().enumerate().filter(|(_, d)| d.group == g).collect();
            for chunk in items.chunks(2) {
                let row = c.cut_top(46.0, 6.0);
                for (cell, (k, d)) in row.columns(2, 6.0).into_iter().zip(chunk.iter()) {
                    let missing = off.contains(&d.feature);
                    let count = placed.iter().filter(|p| p.id == d.id).count();
                    let hov = ui.hovered(cell);
                    ui.fill_rr(cell, 11.0, if hov && !missing { col::BTN_HOVER } else { col::BTN });
                    let mut inner = cell.inset_xy(12.0, 0.0);
                    ui.icon(d.icon, inner.cut_left(20.0, 10.0), 14.0, if missing { col::FAINT } else { accent });
                    let sizes: String = d.sizes.iter().map(|s| s.letter()).collect::<Vec<_>>().join(" ");
                    let tag = inner.cut_right(if count > 0 { 64.0 } else { 52.0 }, 6.0);
                    if count > 0 {
                        ui.text("\u{2713} added", Rect::new(tag.x, tag.cy() - 15.0, tag.w, 15.0), 10.0, col::GREEN, Weight::Semi, Align::Right);
                    } else if !missing {
                        ui.icon("\u{E710}", Rect::new(tag.r() - 16.0, tag.cy() - 16.0, 16.0, 16.0), 10.0, col::DIM);
                    }
                    ui.text(&sizes, Rect::new(tag.x, tag.cy() + 1.0, tag.w, 14.0), 10.0, col::FAINT, Weight::Semi, Align::Right);
                    let sub = if missing { "Not in this version of Nookly".to_string() } else { d.about.to_string() };
                    ui.text(d.name, Rect::new(inner.x, inner.cy() - 16.0, inner.w, 17.0), 12.5, if missing { col::DIM } else { col::TEXT }, Weight::Semi, Align::Left);
                    ui.text(&sub, Rect::new(inner.x, inner.cy() + 1.0, inner.w, 14.0), 10.5, if missing { col::RED.with_a(0.8) } else { col::DIM }, Weight::Regular, Align::Left);
                    fit(ui, tight, d.about, 10.5, Weight::Regular, inner.w);
                    ui.hit_tip(cell, S_BAR_WID_ADD + *k as u32, "");
                }
            }
            c.cut_top(4.0, 0.0);
            ui.text_wrap("Widgets are the taskbar's. The notch has panels (its tabs), live activities (a song, a timer, while they happen) and status (small values on its sides).", c.cut_top(36.0, 0.0), 11.0, col::FAINT, Weight::Regular, Align::Left);
        }
        2 => {
            group(ui, &mut c, "Hovering an open app shows");
            let list = st.extra.get("bar_hover_view").is_some_and(|v| v == "list");
            for (i, (cell, name)) in c.cut_top(30.0, 14.0).columns(2, 8.0).into_iter().zip(["Previews of its windows", "A list of its windows"]).enumerate() {
                ui.chip(cell, name, S_BAR_VIEW + i as u32, list == (i == 1), accent);
            }
            group(ui, &mut c, "Most apps shown  \u{00B7}  the rest go in a \u{22EF} tile");
            let max: usize = st.extra.get("bar_max_apps").and_then(|v| v.parse().ok()).unwrap_or(0);
            let mut row = c.cut_top(30.0, 14.0);
            ui.chip(row.cut_left(110.0, 12.0), "Fit", S_BAR_MAX, max == 0, accent);
            // Or a number of your own: - n +
            let minus = row.cut_left(30.0, 6.0).centered(30.0, 30.0);
            let val = row.cut_left(70.0, 6.0);
            let plus = row.cut_left(30.0, 0.0).centered(30.0, 30.0);
            ui.icon_btn(minus, "\u{E738}", S_BAR_MAX + 1, 11.0, col::TEXT, Some(col::BTN), "");
            ui.fill_rr(val, 15.0, if max > 0 { accent.with_a(0.16) } else { col::TEXT.with_a(0.05) });
            let shown = if max > 0 { max.to_string() } else { BAR_MAX_START.to_string() };
            ui.text(&shown, val, 13.0, if max > 0 { accent } else { col::FAINT }, Weight::Semi, Align::Center);
            ui.hit_tip(val, S_BAR_MAX + 3, "");
            ui.icon_btn(plus, glyph::ADD, S_BAR_MAX + 2, 11.0, col::TEXT, Some(col::BTN), "");
            group(ui, &mut c, "The \u{22EF} tile looks like");
            let stack = st.extra.get("bar_overflow").is_some_and(|v| v == "stack");
            for (i, (cell, name)) in c.cut_top(30.0, 18.0).columns(2, 8.0).into_iter().zip(["Three dots", "Its apps' icons, stacked"]).enumerate() {
                ui.chip(cell, name, S_BAR_MORE + i as u32, stack == (i == 1), accent);
            }
            group(ui, &mut c, "Collections");
            ui.text_wrap(
                "Drag an app between others to move it (into and out of the \u{22EF} tile too), or onto another to put them in a collection; drag more apps onto it to add them. Collections sit after your apps; hover one to see its apps. Right-click a collection to undo it, or an app in it to take it out.",
                c.cut_top(80.0, 0.0),
                11.5,
                col::DIM,
                Weight::Regular,
                Align::Left,
            );
        }
        _ => {
            use crate::barcfg::{Size, Thing, Zone, def};
            let entries = crate::barcfg::layout(st);
            let dragging = drag.filter(|d| d.0 == S_BAR_GRIP).map(|d| d.1);
            let foot = c.cut_bottom(36.0, 8.0);
            let view = c;
            // Rows: a header per zone, its things (or a place to drop into).
            let mut rows: Vec<(Option<usize>, Zone)> = Vec::new();
            for z in Zone::ALL {
                rows.push((None, z));
                let n = entries.iter().filter(|e| e.zone == z).count();
                for (k, _) in entries.iter().enumerate().filter(|(_, e)| e.zone == z) {
                    rows.push((Some(k), z));
                }
                if n == 0 {
                    rows.push((Some(usize::MAX), z));
                }
            }
            let row_h = |r: &(Option<usize>, Zone)| if r.0.is_none() { 26.0 } else { 38.0 };
            let content: f32 = rows.iter().map(row_h).sum();
            *scroll.1 = (content - view.h).max(0.0);
            *scroll.0 = scroll.0.clamp(0.0, *scroll.1);
            let hits_from = ui.hits.len();
            ui.push_clip(Rect::new(view.x - 4.0, view.y, view.w + 8.0, view.h));
            let mut y = view.y - *scroll.0;
            let mut widget_pos = 0usize;
            let mut ghost: Option<(Rect, usize)> = None;
            for (k, z) in rows.iter().copied() {
                let h = row_h(&(k, z));
                let r = Rect::new(view.x, y, view.w - 10.0, h - 4.0);
                y += h;
                match k {
                    None => {
                        let label = match z {
                            Zone::Left => "Pinned left  \u{00B7}  stays put",
                            Zone::Mid => "In the middle  \u{00B7}  scrolls with the wheel when it doesn't all fit",
                            Zone::Right => "Pinned right  \u{00B7}  stays put",
                        };
                        ui.text(label, Rect::new(r.x + 4.0, r.y + 4.0, r.w, 18.0), 11.5, col::DIM, Weight::Semi, Align::Left);
                    }
                    Some(usize::MAX) => {
                        // An empty zone: somewhere to drop.
                        let start = entries.iter().position(|e| Zone::ALL.iter().position(|x| *x == e.zone) > Zone::ALL.iter().position(|x| *x == z)).unwrap_or(entries.len());
                        cells.push(r);
                        slots.push((start, z, false));
                        ui.stroke_rr(r.inset(0.5), 10.5, col::TEXT.with_a(0.12), 1.0);
                        ui.text("Nothing here: drag something in, or use its Left / Right switch", r, 11.0, col::FAINT, Weight::Regular, Align::Center);
                    }
                    Some(k) => {
                        let e = &entries[k];
                        cells.push(r);
                        slots.push((k, z, true));
                        let wpos = if matches!(e.thing, Thing::Widget(..)) {
                            widget_pos += 1;
                            Some(widget_pos - 1)
                        } else {
                            None
                        };
                        if dragging == Some(k) {
                            ui.stroke_rr(r.inset(0.5), 10.5, col::TEXT.with_a(0.18), 1.0);
                            ghost = Some((r, k));
                            continue;
                        }
                        layout_row(ui, r, e, k, wpos, dragging.is_none(), accent);
                    }
                }
            }
            ui.pop_clip();
            // Hits of rows only partly in sight: just the part that shows.
            for h in ui.hits[hits_from..].iter_mut() {
                let (y0, y1) = (h.rect.y.max(view.y), h.rect.b().min(view.b()));
                h.rect = Rect::new(h.rect.x, y0, h.rect.w, (y1 - y0).max(0.0));
            }
            if *scroll.1 > 0.0 {
                let track = Rect::new(view.r() - 4.0, view.y, 3.0, view.h);
                let th = (view.h * view.h / content).max(24.0);
                let ty = track.y + (*scroll.0 / *scroll.1) * (track.h - th);
                ui.fill_rr(track, 1.5, col::TEXT.with_a(0.06));
                ui.fill_rr(Rect::new(track.x, ty, 3.0, th), 1.5, col::TEXT.with_a(0.3));
            }
            if let Some((src, k)) = ghost {
                let cells_now = cells.clone();
                let from = cells_now.iter().position(|c| c.y == src.y).unwrap_or(0);
                let e = entries[k].clone();
                drag_ghost(ui, &cells_now, from, accent, |ui, r| {
                    ui.fill_rr(r, 11.0, col::TEXT.with_a(0.12));
                    ui.stroke_rr(r.inset(0.5), 10.5, accent.with_a(0.6), 1.0);
                    let mut row = r.inset_xy(6.0, 0.0);
                    grip(ui, row.cut_left(18.0, 6.0), 0, true);
                    let (icon, name) = thing_label(&e.thing);
                    ui.icon(icon, row.cut_left(20.0, 10.0), 13.0, accent);
                    ui.text(name, row, 12.5, col::TEXT, Weight::Semi, Align::Left);
                });
            }
            let _ = (Size::S, def("clock"), tight);
            match note {
                Some(n) => ui.text_wrap(n, foot, 11.5, col::RED, Weight::Semi, Align::Left),
                None => ui.text_wrap("Drag by the grip to reorder, or into another part. Left / Middle / Right moves it at once. Sizes: S about one app wide up to W, the most detail.", foot, 11.0, col::FAINT, Weight::Regular, Align::Left),
            }
        }
    }
}

/// A thing's symbol and name in Settings.
fn thing_label(t: &crate::barcfg::Thing) -> (&'static str, &'static str) {
    match t {
        crate::barcfg::Thing::Item(k) => crate::barcfg::ITEMS.iter().find(|i| i.0 == *k).map_or(("", ""), |i| (i.2, i.1)),
        crate::barcfg::Thing::Widget(id, _) => crate::barcfg::def(id).map_or(("", ""), |d| (d.icon, d.name)),
    }
}

/// One row of Settings > Taskbar > Layout: the grip, what it is, and its
/// controls (sizes for a widget, the zone, on/off or take off).
fn layout_row(ui: &mut Ui, r: Rect, e: &crate::barcfg::Entry, k: usize, wpos: Option<usize>, hoverable: bool, accent: Color) {
    use crate::barcfg::{Size, Thing, Zone};
    let hov = hoverable && ui.hovered(r);
    ui.fill_rr(r, 11.0, col::TEXT.with_a(if hov { 0.075 } else { 0.045 }));
    let mut row = r.inset_xy(6.0, 0.0);
    grip(ui, row.cut_left(18.0, 6.0), S_BAR_GRIP + k as u32, false);
    let (icon, name) = thing_label(&e.thing);
    ui.icon(icon, row.cut_left(20.0, 10.0), 13.0, if e.on { accent } else { col::FAINT });
    // On / off (items) or take off (widgets).
    match (&e.thing, wpos) {
        (Thing::Widget(..), Some(p)) => {
            let rm = row.cut_right(26.0, 8.0).centered(26.0, 26.0);
            ui.icon_btn(rm, "\u{E711}", S_BAR_WID_SHOW + p as u32, 10.0, col::DIM, Some(col::BTN), "");
        }
        (Thing::Item("apps"), _) => {
            ui.text("Always on", row.cut_right(60.0, 8.0), 10.5, col::FAINT, Weight::Regular, Align::Right);
        }
        (Thing::Item(key), _) => {
            let sw = row.cut_right(36.0, 8.0).centered(36.0, 20.0);
            switch(ui, sw, e.on, true);
            if let Some(idx) = crate::barcfg::ITEMS.iter().position(|x| x.0 == *key) {
                ui.hit_tip(sw.inset(-4.0), S_BAR_SHOW + idx as u32, "");
            }
        }
        _ => {}
    }
    // Where it sits.
    let zones = row.cut_right(3.0 * 48.0 + 4.0, 10.0);
    for (z, (cell, label)) in zones.columns(3, 2.0).into_iter().zip(["Left", "Middle", "Right"]).enumerate() {
        let cell = cell.centered(cell.w, 22.0);
        let zone = Zone::ALL[z];
        let sel = e.zone == zone;
        let locked = e.thing == Thing::Item("apps") && zone != Zone::Mid;
        let bg = if sel { accent.with_a(0.28) } else if locked { col::TEXT.with_a(0.03) } else if ui.hovered(cell) { col::BTN_HOVER } else { col::BTN };
        ui.fill_rr(cell, 7.0, bg);
        ui.text(label, cell, 10.0, if sel { col::TEXT } else if locked { col::FAINT.with_a(0.5) } else { col::DIM }, Weight::Semi, Align::Center);
        if !locked && !sel {
            ui.hit_tip(cell, S_LAY_ZONE + k as u32 * 3 + z as u32, "");
        }
    }
    // A widget's sizes.
    if let (Thing::Widget(id, size), Some(p)) = (&e.thing, wpos) {
        let d = crate::barcfg::def(id);
        let chips = row.cut_right(4.0 * 22.0 + 3.0 * 2.0, 10.0);
        for (s, (cell, sz)) in chips.columns(4, 2.0).into_iter().zip(Size::ALL).enumerate() {
            let cell = cell.centered(22.0, 22.0);
            if d.is_some_and(|d| d.sizes.contains(&sz)) {
                let sel = *size == sz;
                let bg = if sel { accent.with_a(0.28) } else if ui.hovered(cell) { col::BTN_HOVER } else { col::BTN };
                ui.fill_rr(cell, 7.0, bg);
                ui.text(sz.letter(), cell, 10.0, if sel { col::TEXT } else { col::DIM }, Weight::Semi, Align::Center);
                ui.hit_tip(cell, S_BAR_WID_SIZE + p as u32 * 4 + s as u32, "");
            } else {
                ui.text(sz.letter(), cell, 10.0, col::FAINT.with_a(0.4), Weight::Regular, Align::Center);
            }
        }
    }
    let sub = match &e.thing {
        Thing::Widget(id, size) => format!("{} widget  \u{00B7}  {}", size.name(), crate::barcfg::def(id).map_or("", |d| d.about)),
        Thing::Item(_) => if e.on { "On the taskbar".into() } else { "Off".into() },
    };
    ui.text(name, Rect::new(row.x, row.cy() - 15.0, row.w, 17.0), 12.5, if e.on { col::TEXT } else { col::DIM }, Weight::Semi, Align::Left);
    ui.text(&sub, Rect::new(row.x, row.cy() + 1.0, row.w, 14.0), 10.5, col::DIM, Weight::Regular, Align::Left);
}

/// How wide the taskbar can get (DIPs, as it draws them): the main
/// screen at the bar's size, less its flares and a margin.
pub(super) fn bar_room(st: &Settings) -> f32 {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN};
    let px = unsafe { GetSystemMetrics(SM_CXSCREEN) } as f32;
    let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForSystem() }.max(96) as f32 / 96.0;
    let size = match st.extra.get("bar_size").map(String::as_str) {
        Some("s") => 0.88,
        Some("l") => 1.12,
        _ => 1.0,
    };
    px / (dpi * size) - 40.0
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
        3 => "Nookly's own taskbar, in the notch's style (an optional add-on)",
        S_MASTER => "Turn the whole notch on or off. Off: it hides and none of its features run",
        S_MASTER_TASKBAR => "Use Nookly's taskbar instead of Windows' own (off: Windows' taskbar comes back)",
        S_BAR_GET => "Download the taskbar add-on (under 1 MB) and install it on this PC",
        S_BAR_REMOVE => "Turn the taskbar off and delete the add-on from this PC",
        S_BAR_AUTOHIDE => "Slide the taskbar away; touch the bottom edge of the screen to bring it back",
        S_BAR_HOVER => "Off: lists and pop-ups open only when you click",
        S_BAR_SCREENS => "A taskbar on every screen, or only on the main one",
        i if (S_BAR_OTHER..S_BAR_OTHER + 100).contains(&i) => "Show it on your other screens' taskbars, or not",
        S_BAR_FOLLOW => "Use the same theme as the notch",
        i if (S_BAR_THEME..S_BAR_THEME + 10).contains(&i) => "Use this theme for the taskbar",
        S_BAR_CUSTOM => "The taskbar's own colours: pick a background and a text colour",
        S_BAR_CFIX => "Make the text readable on this background",
        i if (S_BAR_ALIGN..S_BAR_ALIGN + 2).contains(&i) => "Where the taskbar sits along the bottom edge",
        i if (S_BAR_SIZE..S_BAR_SIZE + 3).contains(&i) => "Make the taskbar a little smaller or larger",
        i if (S_BAR_UP..S_BAR_UP + 7).contains(&i) => "Move it earlier in the taskbar",
        i if (S_BAR_DOWN..S_BAR_DOWN + 7).contains(&i) => "Move it later in the taskbar",
        i if (S_BAR_SHOW..S_BAR_SHOW + 8).contains(&i) => "Show or hide it in the taskbar",
        S_BAR_VIEW => "Hovering an open app shows live previews of its windows",
        i if (S_BAR_WID_UP..S_BAR_WID_UP + 50).contains(&i) => "Move it earlier on the taskbar",
        i if (S_BAR_WID_DOWN..S_BAR_WID_DOWN + 50).contains(&i) => "Move it later on the taskbar",
        i if (S_BAR_WID_SHOW..S_BAR_WID_SHOW + 50).contains(&i) => "Take it off the taskbar",
        i if (S_BAR_GRIP..S_BAR_GRIP + 64).contains(&i) || (S_WID_GRIP..S_WID_GRIP + 64).contains(&i) => "Drag it up or down, or into another part",
        i if (S_LAY_ZONE..S_LAY_ZONE + 3 * 64).contains(&i) => match (i - S_LAY_ZONE) % 3 {
            0 => "Pin it at the left end: it stays put",
            1 => "In the middle: it scrolls with the apps when they don't all fit",
            _ => "Pin it at the right end: it stays put",
        },
        i if (S_BAR_WID_PIN..S_BAR_WID_PIN + 50).contains(&i) => "Pin it left or right of the apps: it stays put when they scroll",
        i if (S_BAR_WID_SIZE..S_BAR_WID_SIZE + 256).contains(&i) => "Show it at this size (S Small, M Medium, L Large, W Wide)",
        i if (S_BAR_WID_ADD..S_BAR_WID_ADD + 100).contains(&i) => "Put it on the taskbar, or take it off (pick its size in Widgets)",
        i if (S_BAR_WID_GROUP..S_BAR_WID_GROUP + 4).contains(&i) => "Show this kind of widget",
        781 => "Hovering an open app lists its windows by title",
        S_BAR_MAX => "As many apps as fit on the screen",
        786 => "Fewer apps on the taskbar; the rest go in the \u{22EF} tile",
        787 => "More apps on the taskbar",
        788 => "At most this many apps on the taskbar; the rest go in the \u{22EF} tile",
        S_BAR_MORE => "The tile for the apps that don't fit shows three dots",
        796 => "The tile for the apps that don't fit shows their icons, stacked",
        S_TURN_ON_WIDGETS => "Switch Live & status on",
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
        i if (S_PROFILE..S_PROFILE + 3).contains(&i) => "Switch profile: its own features, notch sides and theme",
        S_PROFILE_AUTO => "Work on weekdays 9 to 5, Home otherwise",
        i if (S_MOUSE..S_MOUSE + 30).contains(&i) => "What this does on the closed notch",
        S_EXPORT => "Save your settings and data in one file, for another PC",
        S_IMPORT => "Load settings and data saved with Export",
        S_IMPORT_OK => "Replace this PC's settings and data with the file's, and restart",
        S_IMPORT_NO => "Keep everything as it is",
        i if (S_SECTION..S_SECTION + SECTIONS.len() as u32).contains(&i) => SECTIONS[(i - S_SECTION) as usize].2,
        i if (S_BAR_SECTION..S_BAR_SECTION + BAR_SECTIONS.len() as u32).contains(&i) => BAR_SECTIONS[(i - S_BAR_SECTION) as usize].2,
        _ => "",
    }
}
