//! Nookly's own small windows in the notch's style and theme, instead of
//! Windows' grey boxes: a message / question dialog (About, the uninstaller)
//! and the tray icon's menu.
//!
//! Each runs its own little message loop and is gone when it returns: no
//! window, timer or drawing surface is kept afterwards. They draw only when
//! something changes (the mouse moves onto another control, a click).

use std::cell::RefCell;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::gfx::*;
use crate::store::Settings;
use crate::ui::{Hit, Ui};

/// What a button looks like.
#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Plain,
    /// The main answer (filled with the accent colour; Enter picks it).
    Accent,
    /// A destructive answer (red).
    Danger,
}

pub struct Spec {
    pub title: String,
    pub subtitle: String,
    pub body: String,
    /// Switches under the text: (label, explanation, starts on).
    pub checks: Vec<(String, String, bool)>,
    /// (label, id returned, look). Ids must be 10 or more.
    pub buttons: Vec<(String, u32, Kind)>,
    /// Height of the body text area (DIPs).
    pub body_h: f32,
    /// The switches turn red when on (they delete something).
    pub danger_checks: bool,
}

/// One menu row: (icon, label, id, red). An empty label is a separator.
pub type Item = (&'static str, &'static str, u32, bool);

const WM_MOUSELEAVE: u32 = 0x02A3;
const W: f32 = 440.0;
const MENU_W: f32 = 236.0;
const ROW: f32 = 34.0;
const ID_CLOSE: u32 = 1;
const ID_MIN: u32 = 2;
/// + index of the switch (up to 7).
const ID_CHECK: u32 = 3;
/// Menu rows: + index of the row.
const MENU_ROW: u32 = 1000;

struct Dlg {
    hwnd: HWND,
    gfx: Gfx,
    hits: Vec<Hit>,
    mouse: (f32, f32),
    hover: Option<u32>,
    press: Option<u32>,
    tracking: bool,
    pos: (i32, i32),
    scale: f32,
    size: (f32, f32),
    theme: Theme,
    spec: Option<Spec>,
    items: Vec<Item>,
    checked: Vec<bool>,
    result: Option<u32>,
}

thread_local! {
    static DLG: RefCell<Option<Dlg>> = const { RefCell::new(None) };
}

/// The theme the notch uses (Liquid Glass has nothing to blur behind a
/// dialog, so it gets the plain dark one).
pub fn theme(st: &Settings) -> Theme {
    match st.theme.as_str() {
        "custom" => {
            let get = |k: &str, d: u32| st.extra.get(k).and_then(|v| u32::from_str_radix(v, 16).ok()).unwrap_or(d);
            custom_theme(get("custom_bg", 0x1B1B2F), get("custom_text", 0xF5F5F7))
        }
        k => THEMES.iter().find(|t| t.key == k && !t.glass).copied().unwrap_or(THEMES[0]),
    }
}

/// Show a dialog; returns the id of the button picked (0 if it was closed)
/// and whether each switch is on.
pub fn show(spec: Spec, theme: Theme) -> (u32, Vec<bool>) {
    let h = 24.0 + 40.0 + 16.0 + spec.body_h + 16.0 + 34.0 + 22.0 + spec.checks.len() as f32 * 54.0;
    let checked = spec.checks.iter().map(|c| c.2).collect();
    let r = run(W, h, None, theme, Some(spec), Vec::new(), checked);
    (r.0.unwrap_or(0), r.1)
}

/// Show the tray menu at the mouse; returns the item picked.
pub fn menu(items: Vec<Item>, theme: Theme) -> Option<u32> {
    let h = 16.0 + items.iter().map(|it| if it.1.is_empty() { 9.0 } else { ROW }).sum::<f32>();
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    let ids: Vec<u32> = items.iter().map(|it| it.2).collect();
    let picked = run(MENU_W, h, Some(p), theme, None, items, Vec::new()).0?;
    picked.checked_sub(MENU_ROW).and_then(|k| ids.get(k as usize).copied())
}

fn run(w: f32, h: f32, at: Option<POINT>, theme: Theme, spec: Option<Spec>, items: Vec<Item>, checked: Vec<bool>) -> (Option<u32>, Vec<bool>) {
    // One at a time.
    if DLG.with(|d| d.borrow().is_some()) {
        return (None, checked);
    }
    let is_menu = at.is_some();
    unsafe {
        static REG: std::sync::Once = std::sync::Once::new();
        let Ok(hinst) = GetModuleHandleW(None) else { return (None, checked) };
        REG.call_once(|| {
            let hi: HINSTANCE = hinst.into();
            let icon = LoadIconW(Some(hi), PCWSTR(1 as *const u16)).unwrap_or_else(|_| make_tray_icon());
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(proc_),
                hInstance: hi,
                lpszClassName: w!("NooklyDialog"),
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
        let (pw, ph) = ((w * scale).ceil() as i32, (h * scale).ceil() as i32);
        let r = mi.rcWork;
        let (x, y) = match at {
            // The menu opens from the mouse, inside the screen (above the
            // taskbar when the tray is at the bottom).
            Some(a) => {
                let x = (a.x - pw + 8).clamp(r.left, (r.right - pw).max(r.left));
                let y = if a.y + ph > r.bottom { a.y - ph } else { a.y };
                (x, y.clamp(r.top, (r.bottom - ph).max(r.top)))
            }
            None => (r.left + ((r.right - r.left) - pw).max(0) / 2, r.top + ((r.bottom - r.top) - ph).max(0) / 2),
        };
        let (ex, style) = if is_menu {
            (WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST, WS_POPUP)
        } else {
            (WS_EX_LAYERED | WS_EX_APPWINDOW | WS_EX_TOPMOST, WS_POPUP | WS_SYSMENU | WS_MINIMIZEBOX)
        };
        let title = spec.as_ref().map_or("Nookly".to_string(), |s| s.title.clone());
        let Ok(hwnd) = CreateWindowExW(ex, w!("NooklyDialog"), &HSTRING::from(title), style, x, y, pw, ph, None, None, Some(hinst.into()), None) else {
            return (None, checked);
        };
        let Ok(gfx) = Gfx::new(pw, ph) else {
            let _ = DestroyWindow(hwnd);
            return (None, checked);
        };
        DLG.with(|d| {
            *d.borrow_mut() = Some(Dlg {
                hwnd,
                gfx,
                hits: Vec::new(),
                mouse: (-1e4, -1e4),
                hover: None,
                press: None,
                tracking: false,
                pos: (x, y),
                scale,
                size: (w, h),
                theme,
                spec,
                items,
                checked: checked.clone(),
                result: None,
            })
        });
        render();
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let mut msg = MSG::default();
        loop {
            let done = DLG.with(|d| d.borrow().as_ref().map_or(true, |d| d.result.is_some()));
            if done || !IsWindow(Some(hwnd)).as_bool() {
                break;
            }
            if !GetMessageW(&mut msg, None, 0, 0).as_bool() {
                // Quitting: let the outer loop see it too.
                PostQuitMessage(msg.wParam.0 as i32);
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let d = DLG.with(|d| d.borrow_mut().take());
        let _ = DestroyWindow(hwnd);
        let checked = d.as_ref().map_or(checked, |d| d.checked.clone());
        (d.and_then(|d| d.result).filter(|r| *r != 0), checked)
    }
}

fn finish(id: u32) {
    DLG.with(|d| {
        if let Some(d) = d.borrow_mut().as_mut() {
            d.result = Some(id);
        }
    });
}

extern "system" fn proc_(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let xy = |d: &Dlg| (((lp.0 & 0xffff) as i16) as f32 / d.scale, (((lp.0 >> 16) & 0xffff) as i16) as f32 / d.scale);
    let is_menu = DLG.with(|d| d.borrow().as_ref().map_or(false, |d| d.spec.is_none()));
    match msg {
        WM_NCHITTEST => {
            // Drag a dialog by its header (anywhere but a control).
            let caption = DLG.with(|d| {
                let d = d.borrow();
                let Some(d) = d.as_ref() else { return false };
                let (sx, sy) = xy(d);
                let (x, y) = (sx - d.pos.0 as f32 / d.scale, sy - d.pos.1 as f32 / d.scale);
                d.spec.is_some() && y < 64.0 && !d.hits.iter().any(|h| h.rect.contains(x, y))
            });
            if caption {
                return LRESULT(HTCAPTION as isize);
            }
        }
        WM_NCLBUTTONDBLCLK => return LRESULT(0),
        WM_MOVE => {
            DLG.with(|d| {
                if let Some(d) = d.borrow_mut().as_mut() {
                    d.pos = (((lp.0 & 0xffff) as i16) as i32, (((lp.0 >> 16) & 0xffff) as i16) as i32);
                }
            });
        }
        WM_MOUSEMOVE => {
            let changed = DLG.with(|d| {
                let mut d = d.borrow_mut();
                let Some(d) = d.as_mut() else { return false };
                d.mouse = xy(d);
                if !d.tracking {
                    d.tracking = true;
                    unsafe {
                        let mut tme = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                        let _ = TrackMouseEvent(&mut tme);
                    }
                }
                let id = d.hits.iter().rev().find(|h| h.rect.contains(d.mouse.0, d.mouse.1)).map(|h| h.id);
                let changed = id != d.hover;
                d.hover = id;
                changed
            });
            if changed {
                render();
            }
            return LRESULT(0);
        }
        WM_MOUSELEAVE => {
            DLG.with(|d| {
                if let Some(d) = d.borrow_mut().as_mut() {
                    d.tracking = false;
                    d.mouse = (-1e4, -1e4);
                    d.hover = None;
                    d.press = None;
                }
            });
            render();
            return LRESULT(0);
        }
        WM_LBUTTONDOWN => {
            DLG.with(|d| {
                if let Some(d) = d.borrow_mut().as_mut() {
                    let (x, y) = xy(d);
                    d.press = d.hits.iter().rev().find(|h| h.rect.contains(x, y)).map(|h| h.id);
                }
            });
            return LRESULT(0);
        }
        WM_LBUTTONUP => {
            let id = DLG.with(|d| {
                let mut d = d.borrow_mut();
                let d = d.as_mut()?;
                let (x, y) = xy(d);
                let under = d.hits.iter().rev().find(|h| h.rect.contains(x, y)).map(|h| h.id);
                let p = d.press.take();
                (p == under).then_some(p).flatten()
            });
            match id {
                Some(ID_CLOSE) => finish(0),
                Some(ID_MIN) => unsafe {
                    let _ = ShowWindow(hwnd, SW_MINIMIZE);
                },
                Some(i) if (ID_CHECK..ID_CHECK + 7).contains(&i) => {
                    DLG.with(|d| {
                        if let Some(d) = d.borrow_mut().as_mut() {
                            if let Some(c) = d.checked.get_mut((i - ID_CHECK) as usize) {
                                *c = !*c;
                            }
                        }
                    });
                    render();
                }
                Some(i) => finish(i),
                None => {}
            }
            return LRESULT(0);
        }
        WM_KEYDOWN => {
            let vk = wp.0 as u16;
            if vk == VK_ESCAPE.0 {
                finish(0);
            } else if vk == VK_RETURN.0 {
                // Enter: the accent button.
                let id = DLG.with(|d| d.borrow().as_ref().and_then(|d| d.spec.as_ref()).and_then(|s| s.buttons.iter().find(|b| b.2 == Kind::Accent).map(|b| b.1)));
                if let Some(id) = id {
                    finish(id);
                }
            }
            return LRESULT(0);
        }
        // A menu closes when you click anywhere else.
        WM_ACTIVATE if is_menu && (wp.0 & 0xffff) as u32 == WA_INACTIVE => finish(0),
        WM_CLOSE => {
            finish(0);
            return LRESULT(0);
        }
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

fn render() {
    DLG.with(|d| {
        let mut d = d.borrow_mut();
        let Some(d) = d.as_mut() else { return };
        d.hits.clear();
        d.gfx.theme = d.theme;
        d.gfx.begin(d.scale);
        let (w, h) = d.size;
        let mouse = d.mouse;
        let checked = d.checked.clone();
        let spec = d.spec.take();
        let items = std::mem::take(&mut d.items);
        paint(&mut d.gfx, &mut d.hits, (w, h), &spec, &items, &checked, mouse);
        d.spec = spec;
        d.items = items;
        let (hwnd, pos) = (d.hwnd, d.pos);
        d.gfx.end_and_present(hwnd, pos.0, pos.1);
    });
}

fn paint(g: &mut Gfx, hits: &mut Vec<Hit>, (w, h): (f32, f32), spec: &Option<Spec>, items: &[Item], checked: &[bool], mouse: (f32, f32)) {
    let mut ui = Ui { g, hits, module: 0, mouse, t: 0.0 };
    let win = Rect::new(0.0, 0.0, w, h);
    ui.fill_rr(win, if spec.is_some() { 20.0 } else { 14.0 }, col::NOTCH);
    ui.stroke_rr(win.inset(0.5), if spec.is_some() { 19.5 } else { 13.5 }, col::TEXT.with_a(0.14), 1.0);
    match spec {
        Some(s) => draw_dialog(&mut ui, win, s, checked),
        None => draw_menu(&mut ui, win, items),
    }
}

/// Docs renders: a dialog (or, with no spec, the menu with row `hover`
/// highlighted) drawn off screen at `scale`.
pub fn snapshot(spec: Option<Spec>, items: Vec<Item>, theme: Theme, scale: f32, hover: Option<usize>) -> Option<Gfx> {
    let (w, h) = match &spec {
        Some(s) => (W, 24.0 + 40.0 + 16.0 + s.body_h + 16.0 + 34.0 + 22.0 + s.checks.len() as f32 * 54.0),
        None => (MENU_W, 16.0 + items.iter().map(|it| if it.1.is_empty() { 9.0 } else { ROW }).sum::<f32>()),
    };
    // The mouse over a menu row (rows start 8 DIPs down).
    let mouse = hover.map_or((-1e4, -1e4), |k| {
        let y: f32 = 8.0 + items.iter().take(k).map(|it| if it.1.is_empty() { 9.0 } else { ROW }).sum::<f32>();
        (40.0, y + ROW / 2.0)
    });
    let checked: Vec<bool> = spec.as_ref().map_or(Vec::new(), |s| s.checks.iter().map(|c| c.2).collect());
    let mut g = Gfx::new((w * scale).ceil() as i32, (h * scale).ceil() as i32).ok()?;
    g.theme = theme;
    g.begin(scale);
    let mut hits = Vec::new();
    paint(&mut g, &mut hits, (w, h), &spec, &items, &checked, mouse);
    // No window: the frame just stays in the drawing surface.
    g.end_and_present(HWND::default(), 0, 0);
    Some(g)
}

/// Nookly's mark (the app icon's own pixels).
pub fn draw_logo(ui: &mut Ui, r: Rect) {
    let n = (r.w * 2.0).round().max(8.0) as u32;
    let px = logo_pixels(n, true);
    if let Some(b) = ui.make_bitmap(n, n, &px) {
        ui.image_rr(&b, r, 0.0);
    }
}

fn draw_dialog(ui: &mut Ui, win: Rect, s: &Spec, checked: &[bool]) {
    let accent = col::ACCENT;
    let mut c = win.inset(22.0);
    // Header: logo, title and subtitle, minimize and close.
    let mut head = c.cut_top(40.0, 16.0);
    let close = head.cut_right(30.0, 6.0).centered(30.0, 30.0);
    let min = head.cut_right(30.0, 10.0).centered(30.0, 30.0);
    ui.icon_btn(min, "\u{E921}", ID_MIN, 10.0, col::DIM, Some(col::BTN), "Minimize");
    ui.icon_btn(close, glyph::CLOSE, ID_CLOSE, 10.0, col::DIM, Some(col::BTN), "Close");
    draw_logo(ui, head.cut_left(40.0, 12.0).centered(40.0, 40.0));
    ui.text(&s.title, Rect::new(head.x, head.y + 1.0, head.w, 21.0), 16.0, col::TEXT, Weight::Semi, Align::Left);
    ui.text(&s.subtitle, Rect::new(head.x, head.y + 22.0, head.w, 16.0), 11.5, col::DIM, Weight::Regular, Align::Left);
    // Buttons, right-aligned at the bottom.
    let mut foot = c.cut_bottom(34.0, 16.0);
    for (label, id, kind) in s.buttons.iter().rev() {
        let bw = ui.measure(label, 12.0, Weight::Semi) + 36.0;
        let r = foot.cut_right(bw.max(90.0), 8.0);
        match kind {
            Kind::Accent => ui.button_accent(r, label, *id, accent),
            Kind::Danger => ui.button_c(r, label, *id, col::RED.with_a(0.2), col::RED),
            Kind::Plain => ui.button(r, label, *id),
        }
    }
    for (k, (label, desc, _)) in s.checks.iter().enumerate().rev() {
        let checked = checked.get(k).copied().unwrap_or(false);
        let r = c.cut_bottom(46.0, 8.0);
        let hov = ui.hovered(r);
        ui.fill_rr(r, 12.0, col::TEXT.with_a(if hov { 0.075 } else { 0.045 }));
        let mut row = r.inset_xy(14.0, 0.0);
        let sw = row.cut_right(36.0, 10.0).centered(36.0, 20.0);
        ui.text(label, Rect::new(row.x, r.y + 5.0, row.w, 19.0), 13.0, col::TEXT, Weight::Semi, Align::Left);
        ui.text(desc, Rect::new(row.x, r.y + 23.0, row.w, 16.0), 11.0, col::DIM, Weight::Regular, Align::Left);
        if checked {
            ui.fill_rr(sw, 10.0, if s.danger_checks { col::RED } else { col::ACCENT });
            ui.circle(sw.r() - 10.0, sw.cy(), 8.0, hex(0x010101));
        } else {
            ui.fill_rr(sw, 10.0, col::TEXT.with_a(0.18));
            ui.circle(sw.x + 10.0, sw.cy(), 8.0, col::TEXT);
        }
        ui.hit(r, ID_CHECK + k as u32);
    }
    ui.text_wrap(&s.body, c, 12.5, col::TEXT.with_a(0.85), Weight::Regular, Align::Left);
}

fn draw_menu(ui: &mut Ui, win: Rect, items: &[Item]) {
    let mut c = win.inset(8.0);
    for (k, &(icon, label, _, red)) in items.iter().enumerate() {
        if label.is_empty() {
            let r = c.cut_top(9.0, 0.0);
            ui.fill_rect(Rect::new(r.x + 10.0, r.cy(), r.w - 20.0, 1.0), col::TEXT.with_a(0.12));
            continue;
        }
        let r = c.cut_top(ROW, 0.0);
        let hov = ui.hovered(r);
        if hov {
            ui.fill_rr(r, 10.0, if red { col::RED.with_a(0.16) } else { col::TEXT.with_a(0.09) });
        }
        let fg = if red { col::RED } else { col::TEXT };
        let mut row = r.inset_xy(10.0, 0.0);
        ui.icon(icon, row.cut_left(18.0, 10.0), 12.0, if red { col::RED } else if hov { col::ACCENT } else { col::DIM });
        ui.text(label, row, 12.5, fg, Weight::Regular, Align::Left);
        // Rows have their own ids (the caller's could clash with close,
        // minimize and the switches); `menu` maps them back.
        ui.hit(r, MENU_ROW + k as u32);
    }
}
