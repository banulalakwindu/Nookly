//! Right-click menus in the taskbar's own look: a bubble that grows out of
//! the bar, like every other pop-up. For an app, an app in a collection, a
//! collection, or the bar itself.
//!
//! It stays until you pick something, click anywhere else or press Esc (or,
//! in case none of that is heard, until it's been left alone for long). The
//! bar never takes the focus, so while a menu is open it listens for the
//! mouse buttons (raw input: told about them, never in their way) and takes
//! Esc, and the arrows and Enter while the mouse is on it; all of it ends
//! with the menu.

use nookly_core::gfx::*;
use nookly_core::shell;
use nookly_core::surfaces::{COMMAND_AUTOHIDE, COMMAND_MSG, COMMAND_SETTINGS};
use nookly_core::ui::Ui;
use windows::Win32::Foundation::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Input::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use crate::{Bar, Pop, apps, items, jump};

/// A menu row: + its index.
pub const ID: u32 = 9000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cmd {
    New,
    Show,
    Admin,
    Location,
    Pin,
    TakeOut,
    Undo,
    Close,
    End,
    EndNow,
    Settings,
    TaskManager,
    Desktop,
    AutoHide,
    /// One of the app's recent files or tasks.
    Jump(usize),
}

#[derive(Clone)]
pub struct Row {
    icon: &'static str,
    label: String,
    /// None: a line between groups, or (with a label) a heading.
    cmd: Option<Cmd>,
    /// A switch, on or off (a tick).
    on: Option<bool>,
}

fn row(icon: &'static str, label: &str, cmd: Cmd) -> Row {
    Row { icon, label: label.to_string(), cmd: Some(cmd), on: None }
}

fn line() -> Row {
    Row { icon: "", label: String::new(), cmd: None, on: None }
}

fn heading(label: &str) -> Row {
    Row { icon: "", label: label.to_string(), cmd: None, on: None }
}

#[derive(Clone)]
pub struct Menu {
    /// The app it's for (its icon and name on top).
    pub app: Option<apps::App>,
    /// Its collection, when it's for one, or for an app in one.
    group: Option<usize>,
    rows: Vec<Row>,
    /// The row the arrow keys are on.
    sel: Option<usize>,
    /// The app's recent files and tasks (once they're read).
    jump: Vec<jump::Entry>,
}

const W: f32 = 228.0;
const PAD: f32 = 8.0;
const HEAD: f32 = 34.0;
const ROW: f32 = 32.0;
const LINE: f32 = 9.0;
/// A heading, and the width with files in it.
const TITLE: f32 = 24.0;
const WIDE: f32 = 290.0;

impl Row {
    fn height(&self) -> f32 {
        match self.cmd {
            Some(_) => ROW,
            None if self.label.is_empty() => LINE,
            None => TITLE,
        }
    }
}

impl Menu {
    fn for_app(app: apps::App, group: Option<usize>) -> Menu {
        // A permission prompt (or another window Nookly can't look into):
        // only to be shown.
        if app.launch.is_empty() {
            return Menu { app: Some(app), group: None, rows: vec![row("\u{E8A7}", "Show", Cmd::Show)], sel: None, jump: Vec::new() };
        }
        let mut rows = vec![row("\u{E8A7}", if app.windows.is_empty() { "Open" } else { "New window" }, Cmd::New)];
        if apps::exe(&app).is_some() {
            rows.push(row("\u{E7EF}", "Run as administrator", Cmd::Admin));
            rows.push(row(glyph::FOLDER, "Open file location", Cmd::Location));
        }
        rows.push(if app.pinned { row("\u{E77A}", "Unpin from taskbar", Cmd::Pin) } else { row(glyph::PIN, "Pin to taskbar", Cmd::Pin) });
        if group.is_some() {
            rows.push(row(glyph::BACK, "Take out of collection", Cmd::TakeOut));
        }
        if !app.windows.is_empty() {
            rows.push(line());
            rows.push(row(glyph::CLOSE, if app.windows.len() > 1 { "Close all windows" } else { "Close window" }, Cmd::Close));
            if apps::can_end(&app) {
                rows.push(row("\u{E7E8}", "End task", Cmd::End));
            }
        }
        Menu { app: Some(app), group, rows, sel: None, jump: Vec::new() }
    }

    /// The app's recent files and tasks come on top (the menu grows up: what
    /// it had stays where it was).
    pub fn add_jump(&mut self, list: Vec<jump::Entry>) {
        if list.is_empty() || !self.jump.is_empty() {
            return;
        }
        let mut rows = Vec::new();
        for recent in [true, false] {
            let mut first = true;
            for (i, e) in list.iter().enumerate().filter(|(_, e)| e.recent == recent) {
                if std::mem::take(&mut first) {
                    rows.push(heading(if recent { "Recent" } else { "Tasks" }));
                }
                let icon = match (recent, e.folder) {
                    (true, true) => glyph::FOLDER,
                    (true, false) => glyph::FILE,
                    _ => "\u{E8A7}",
                };
                rows.push(row(icon, &e.label, Cmd::Jump(i)));
            }
        }
        rows.push(line());
        if let Some(s) = self.sel.as_mut() {
            *s += rows.len();
        }
        rows.append(&mut self.rows);
        self.rows = rows;
        self.jump = list;
    }

    fn for_group(g: usize) -> Menu {
        Menu { app: None, group: Some(g), rows: vec![row("\u{E7A7}", "Undo collection", Cmd::Undo)], sel: None, jump: Vec::new() }
    }

    fn for_bar(autohide: bool) -> Menu {
        let mut hide = row("\u{E70D}", "Auto-hide", Cmd::AutoHide);
        hide.on = Some(autohide);
        let rows = vec![
            row(glyph::SETTINGS, "Taskbar settings", Cmd::Settings),
            row("\u{E9D9}", "Task Manager", Cmd::TaskManager),
            row("\u{E7F4}", "Show desktop", Cmd::Desktop),
            line(),
            hide,
        ];
        Menu { app: None, group: None, rows, sel: None, jump: Vec::new() }
    }

    pub fn size(&self) -> (f32, f32) {
        let rows: f32 = self.rows.iter().map(Row::height).sum();
        (if self.jump.is_empty() { W } else { WIDE }, PAD * 2.0 + if self.app.is_some() { HEAD } else { 0.0 } + rows)
    }

    /// The next row the arrows land on (they skip the lines, and go round).
    fn next(&self, down: bool) -> Option<usize> {
        let n = self.rows.len();
        let step = if down { 1 } else { n - 1 };
        let mut i = self.sel.unwrap_or(if down { n - 1 } else { 0 });
        for _ in 0..n {
            i = (i + step) % n;
            if self.rows[i].cmd.is_some() {
                return Some(i);
            }
        }
        None
    }
}

/// The menu for what's under the mouse (hit `id`; None: the bar itself).
pub fn open(bar: &mut Bar, id: Option<u32>, x: f32) {
    let m = match id {
        Some(i) if (items::GROUP..items::GROUP + 100).contains(&i) => Menu::for_group((i - items::GROUP) as usize),
        Some(i) if (items::POP_MEMBER..items::POP_MEMBER + 100).contains(&i) => match (items::grid_app(bar, i), bar.popup) {
            (Some(a), Some(Pop::Group(g, _))) => Menu::for_app(a, Some(g)),
            _ => return,
        },
        Some(i) if (items::APP..items::APP + 900).contains(&i) => match bar.apps.list.get((i - items::APP) as usize) {
            Some(a) => Menu::for_app(a.clone(), None),
            None => return,
        },
        Some(i) if (items::POP_APP..items::POP_APP + 1000).contains(&i) => match items::grid_app(bar, i) {
            Some(a) => Menu::for_app(a, None),
            None => return,
        },
        // Inside a pop-up: nothing of its own.
        Some(0) | Some(items::FULL_VIEW) => return,
        // An app's windows: the app's menu.
        Some(i) if (items::POP_ROW..=items::POP_OPEN).contains(&i) => match bar.popup {
            Some(Pop::App(k, _)) => match bar.apps.list.get(k) {
                Some(a) => Menu::for_app(a.clone(), None),
                None => return,
            },
            _ => return,
        },
        Some(i) if (items::SUB_ROW..=items::SUB_OPEN).contains(&i) => match bar.sub_app.clone() {
            Some((a, _)) => Menu::for_app(a, None),
            None => return,
        },
        // A row of the menu itself.
        Some(i) if (ID..ID + 100).contains(&i) => return,
        _ => Menu::for_bar(bar.autohide()),
    };
    // Over its tile, when it's a tile's.
    let tile = id.is_some_and(|i| i >= items::APP && !(items::WIDGET..items::WIDGET + 500).contains(&i));
    let x = id.filter(|_| tile).and_then(|i| bar.hits.iter().find(|h| h.id == i)).map_or(x, |h| h.rect.cx());
    if let Some(a) = m.app.as_ref().filter(|a| !a.launch.is_empty()) {
        jump::read(a, bar.hwnd);
    }
    bar.menu = Some(m);
    bar.set_pop(Some(Pop::Menu(x)));
}

pub fn draw(bar: &mut Bar, ui: &mut Ui, r: Rect) {
    let Some(m) = bar.menu.clone() else { return };
    let mut c = r.inset(PAD);
    if let Some(app) = &m.app {
        let mut head = c.cut_top(HEAD, 0.0).inset_xy(8.0, 0.0);
        head.h -= 4.0;
        items::app_icon(&mut bar.icons, ui, app, head.cut_left(18.0, 10.0).centered(18.0, 18.0), &mut bar.apps);
        let name = if app.name.is_empty() { "App" } else { app.name.as_str() };
        ui.text(name, head, 12.5, col::TEXT, Weight::Semi, Align::Left);
    }
    for (i, row) in m.rows.iter().enumerate() {
        let Some(cmd) = row.cmd else {
            let l = c.cut_top(row.height(), 0.0);
            if !row.label.is_empty() {
                ui.text(&row.label, l.inset_xy(8.0, 0.0), 10.5, col::FAINT, Weight::Semi, Align::Left);
                continue;
            }
            ui.fill_rr(Rect::new(l.x + 8.0, l.cy() - 0.5, l.w - 16.0, 1.0), 0.5, col::TEXT.with_a(0.10));
            continue;
        };
        let cell = c.cut_top(ROW, 0.0);
        let id = ID + i as u32;
        let hot = cmd == Cmd::EndNow;
        if ui.hovered(cell) || m.sel == Some(i) {
            let bg = if hot {
                col::RED.with_a(0.30)
            } else if bar.down == Some(id) {
                col::BTN_HOVER.with_a(0.24)
            } else {
                col::BTN_HOVER
            };
            ui.fill_rr(cell, 9.0, bg);
        } else if hot {
            ui.fill_rr(cell, 9.0, col::RED.with_a(0.18));
        }
        let mut inner = cell.inset_xy(8.0, 0.0);
        let ic = inner.cut_left(18.0, 10.0);
        ui.icon(row.icon, ic, 13.0, if hot { col::RED } else { col::DIM });
        if row.on == Some(true) {
            let t = inner.cut_right(16.0, 4.0);
            ui.icon(glyph::CHECK, t, 11.0, col::ACCENT);
        }
        ui.text(&row.label, inner, 12.0, if hot { col::RED } else { col::TEXT }, Weight::Regular, Align::Left);
        ui.hit(cell, id);
    }
}

fn pick(bar: &mut Bar, i: usize) {
    let Some(m) = bar.menu.clone() else { return };
    let Some(cmd) = m.rows.get(i).and_then(|r| r.cmd) else { return };
    // Ending an app loses what isn't saved: asked once more, in place.
    if cmd == Cmd::End {
        if let Some(menu) = bar.menu.as_mut() {
            menu.rows[i] = row("\u{E7BA}", "End it? Unsaved work is lost", Cmd::EndNow);
        }
        bar.dirty = true;
        return;
    }
    bar.set_pop(None);
    let rescan = |bar: &mut Bar| {
        let st = bar.settings.clone();
        bar.apps.scan(&st);
        bar.dirty = true;
    };
    match (cmd, &m.app) {
        (Cmd::New, Some(a)) => apps::launch(&a.launch),
        (Cmd::Show, Some(a)) => {
            if let Some(&h) = a.windows.first() {
                apps::activate(h);
            }
        }
        (Cmd::Admin, Some(a)) => apps::launch_admin(a),
        (Cmd::Location, Some(a)) => apps::open_location(a),
        (Cmd::Pin, Some(a)) => {
            apps::set_pinned(a, !a.pinned);
            rescan(bar);
        }
        (Cmd::TakeOut, Some(a)) => {
            if let Some(g) = m.group {
                apps::group_remove(g, &a.key);
                rescan(bar);
            }
        }
        (Cmd::Undo, _) => {
            if let Some(g) = m.group {
                apps::group_delete(g);
                rescan(bar);
            }
        }
        (Cmd::Close, Some(a)) => a.windows.iter().for_each(|&h| apps::close(h)),
        (Cmd::EndNow, Some(a)) => apps::end(a),
        (Cmd::Jump(i), _) => {
            if let Some(e) = m.jump.get(i) {
                jump::open(e);
            }
        }
        (Cmd::Settings, _) => command(COMMAND_SETTINGS),
        (Cmd::AutoHide, _) => command(COMMAND_AUTOHIDE),
        (Cmd::TaskManager, _) => apps::launch("taskmgr.exe"),
        (Cmd::Desktop, _) => show_desktop(),
        _ => {}
    }
}

pub fn show_desktop() {
    shell::press(&[VK_LWIN, VK_D]);
}

/// The settings are the notch's: it does it.
fn command(what: usize) {
    unsafe {
        if let Ok(notch) = FindWindowW(w!("NooklyNotch"), PCWSTR::null()) {
            let msg = RegisterWindowMessageW(COMMAND_MSG);
            // Its Settings window may come to the front.
            let mut pid = 0u32;
            GetWindowThreadProcessId(notch, Some(&mut pid));
            let _ = AllowSetForegroundWindow(pid);
            let _ = PostMessageW(Some(notch), msg, WPARAM(what), LPARAM(0));
        }
    }
}

pub fn action(bar: &mut Bar, id: u32) {
    pick(bar, (id - ID) as usize);
}

const ESC: (i32, VIRTUAL_KEY) = (1, VK_ESCAPE);
const KEYS: [(i32, VIRTUAL_KEY); 3] = [(2, VK_UP), (3, VK_DOWN), (4, VK_RETURN)];

/// A key while the menu is open (WM_HOTKEY).
pub fn key(bar: &mut Bar, id: i32) {
    if !matches!(bar.popup, Some(Pop::Menu(_))) {
        return;
    }
    bar.dirty = true;
    bar.pop_leave = None;
    match id {
        1 => bar.set_pop(None),
        2 | 3 => {
            if let Some(m) = bar.menu.as_mut() {
                m.sel = m.next(id == 3);
            }
        }
        _ => {
            if let Some(i) = bar.menu.as_ref().and_then(|m| m.sel) {
                pick(bar, i);
            }
        }
    }
}

fn hotkey(hwnd: HWND, (id, vk): (i32, VIRTUAL_KEY), on: bool) {
    unsafe {
        if on {
            let _ = RegisterHotKey(Some(hwnd), id, HOT_KEY_MODIFIERS(0), vk.0 as u32);
        } else {
            let _ = UnregisterHotKey(Some(hwnd), id);
        }
    }
}

/// Take the arrows and Enter (while the mouse is on the menu or the bar: away
/// from them they're for what you're typing in).
pub fn take_keys(hwnd: HWND, on: bool) {
    for k in KEYS {
        hotkey(hwnd, k, on);
    }
}

/// Listen (or stop listening) for what closes the menu.
pub fn listen(hwnd: HWND, on: bool) {
    unsafe {
        hotkey(hwnd, ESC, on);
        let dev = RAWINPUTDEVICE { usUsagePage: 1, usUsage: 2, dwFlags: if on { RIDEV_INPUTSINK } else { RIDEV_REMOVE }, hwndTarget: if on { hwnd } else { HWND::default() } };
        let _ = RegisterRawInputDevices(&[dev], std::mem::size_of::<RAWINPUTDEVICE>() as u32);
    }
}

/// Raw input: was a mouse button just pressed?
pub fn pressed(lp: LPARAM) -> bool {
    let mut raw = RAWINPUT::default();
    let mut n = std::mem::size_of::<RAWINPUT>() as u32;
    unsafe {
        let got = GetRawInputData(HRAWINPUT(lp.0 as *mut _), RID_INPUT, Some(&mut raw as *mut _ as *mut _), &mut n, std::mem::size_of::<RAWINPUTHEADER>() as u32);
        if got == u32::MAX || got == 0 || raw.header.dwType != RIM_TYPEMOUSE.0 {
            return false;
        }
        // Left, right or middle down.
        raw.data.mouse.Anonymous.Anonymous.usButtonFlags & (0x1 | 0x4 | 0x10) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_skip_lines_and_go_round() {
        let mut m = Menu::for_bar(false);
        assert_eq!(m.next(true), Some(0));
        m.sel = Some(2);
        assert_eq!(m.next(true), Some(4));
        m.sel = Some(4);
        assert_eq!(m.next(true), Some(0));
        assert_eq!(m.next(false), Some(2));
        m.sel = None;
        assert_eq!(m.next(false), Some(4));
    }

    #[test]
    fn height_counts_rows_and_lines() {
        let m = Menu::for_bar(true);
        assert_eq!(m.size().1, PAD * 2.0 + 4.0 * ROW + LINE);
    }
}
