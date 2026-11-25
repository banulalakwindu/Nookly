//! What the bar holds, in order (Settings > Taskbar > Layout), and how each
//! item measures, draws and handles a click; and the pop-ups above the bar
//! (an app's windows as live previews or a list, the apps that didn't fit,
//! a collection's apps).

use nookly_core::gfx::*;
use nookly_core::shell;
use nookly_core::store::Settings;
use nookly_core::ui::Ui;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;

use crate::apps;
use crate::layout::{self, Fit};
use nookly_core::barcfg::{Thing, Zone};
use crate::{Bar, Pop};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Start,
    Search,
    Apps,
    Tray,
    Lang,
    Quick,
    Clock,
    Notifs,
    /// A widget (its place among the widgets, barcfg::placed).
    Widget(usize),
}

impl Item {
    pub const ALL: [Item; 8] = [Item::Start, Item::Search, Item::Apps, Item::Tray, Item::Lang, Item::Quick, Item::Clock, Item::Notifs];

    pub fn key(self) -> &'static str {
        match self {
            Item::Start => "start",
            Item::Search => "search",
            Item::Apps => "apps",
            Item::Tray => "tray",
            Item::Lang => "lang",
            Item::Quick => "quick",
            Item::Clock => "clock",
            Item::Notifs => "notifs",
            Item::Widget(_) => "",
        }
    }

    fn from_key(k: &str) -> Option<Item> {
        Item::ALL.into_iter().find(|i| i.key() == k)
    }
}

/// Click ids.
pub const START: u32 = 1;
pub const SEARCH: u32 = 2;
pub const TRAY: u32 = 3;
pub const NOTIFS: u32 = 5;
pub const LANG: u32 = 6;
pub const QUICK: u32 = 7;
pub const CLOCK: u32 = 8;
pub const OVERFLOW: u32 = 9;
/// The screen's far corner (fixed bar): the desktop.
pub const DESKTOP: u32 = 10;
/// + index into `bar.apps.list`.
pub const APP: u32 = 100;
/// An app's pop-up: + index of the window in the app's list.
pub const POP_ROW: u32 = 1000;
pub const POP_CLOSE: u32 = 1100;
pub const POP_OPEN: u32 = 1200;
/// The "more apps" list: + index into `bar.apps.list`.
pub const POP_APP: u32 = 2000;
/// A collection's tile: + index into `bar.apps.groups`.
pub const GROUP: u32 = 3500;
/// A collection's pop-up: + index of the app in the collection.
pub const POP_MEMBER: u32 = 4000;
/// The windows bubble grown out of a grid (the "more apps" tile's or a
/// collection's): + index of the window; and its "New".
pub const SUB_ROW: u32 = 6000;
pub const SUB_CLOSE: u32 = 6100;
pub const SUB_OPEN: u32 = 6200;
/// A widget: + its place on the bar (barcfg::placed).
pub const WIDGET: u32 = 7000;
/// An open widget's full view (its input goes to the notch).
pub const FULL_VIEW: u32 = 7900;
/// The hidden icons' bubble: + index into tray::icons().
pub const TRAY_ICON: u32 = 8000;

/// Square button size, app tile size and gap, gap between items, padding
/// inside the bar, and the extra space between groups.
const BTN: f32 = 38.0;
pub const TILE: f32 = 40.0;
pub const TILE_GAP: f32 = 2.0;
const GAP: f32 = 4.0;
const PAD: f32 = 10.0;
const SPACE: f32 = 8.0;
/// App icon size (DIPs).
const ICON: f32 = 24.0;
/// A window preview card (title over a live thumbnail).
const CARD_W: f32 = 220.0;
const CARD_H: f32 = 156.0;
const CARD_GAP: f32 = 8.0;

/// What the bar shows, in order, each with its zone (Settings > Taskbar >
/// Layout): the items switched on and every widget.
pub fn order(st: &Settings) -> Vec<(Item, Zone)> {
    let mut w = 0;
    nookly_core::barcfg::layout(st)
        .into_iter()
        .filter_map(|e| match e.thing {
            Thing::Item(k) => Item::from_key(k).filter(|_| e.on).map(|it| (it, e.zone)),
            Thing::Widget(..) => {
                w += 1;
                Some((Item::Widget(w - 1), e.zone))
            }
        })
        .collect()
}

/// What a bar on another screen shows (Settings > Taskbar > Other screens),
/// in the same order.
pub fn other_order(st: &Settings) -> Vec<(Item, Zone)> {
    use nookly_core::barcfg::{layout, other_shows};
    let mut w = 0;
    layout(st)
        .into_iter()
        .filter_map(|e| match e.thing {
            Thing::Item(k) => Item::from_key(k).filter(|_| other_shows(st, k)).map(|it| (it, e.zone)),
            Thing::Widget(id, _) => {
                w += 1;
                other_shows(st, &format!("@{id}")).then_some((Item::Widget(w - 1), e.zone))
            }
        })
        .collect()
}

/// Settings > Taskbar > Apps > most apps to show (0 = as many as fit).
fn max_apps(st: &Settings) -> usize {
    st.extra.get("bar_max_apps").and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// Settings > Taskbar > Apps > hovering an app shows previews (default)
/// or a list of its windows.
pub fn previews(st: &Settings) -> bool {
    st.extra.get("bar_hover_view").is_none_or(|v| v != "list")
}

/// Where everything goes: each item's x (from the body's left) and width,
/// how the apps fit, and the body width. The middle zone scrolls between
/// the pinned ends when it doesn't fit.
pub struct Plan {
    pub slots: Vec<(Item, f32, f32)>,
    pub fit: Fit,
    pub width: f32,
    /// The scrolling strip: (x, width shown, width of all it holds), and
    /// which slots are in it.
    pub strip: Option<(f32, f32, f32)>,
    pub in_strip: Vec<bool>,
}

const WIDGET_GAP: f32 = 3.0;

/// The space between two neighbours: widgets sit close (one row of
/// cards), the small buttons and the right end's items close too (like
/// Windows'), the apps get a little air.
fn gap_between(a: Item, b: Item) -> f32 {
    let w = |it: Item| matches!(it, Item::Widget(_));
    if w(a) && w(b) {
        WIDGET_GAP
    } else if a == Item::Apps || b == Item::Apps {
        GAP + SPACE
    } else if w(a) || w(b) {
        GAP + 4.0
    } else {
        GAP
    }
}

/// The face a right-end item shows (its language, Wi-Fi / sound /
/// battery, clock).
fn sys_face(it: Item) -> Option<&'static str> {
    match it {
        Item::Lang => Some("sys_lang@s"),
        Item::Quick => Some("sys_quick@s"),
        Item::Clock => Some("sys_clock@s"),
        _ => None,
    }
}

/// Width of a thing other than the apps (0: nothing to show yet).
fn item_w(bar: &Bar, _g: &mut Gfx, it: Item) -> f32 {
    match it {
        Item::Start | Item::Search | Item::Notifs => BTN,
        Item::Tray => 28.0,
        Item::Lang | Item::Quick | Item::Clock => sys_face(it).and_then(|f| bar.faces.get(f)).map_or(0.0, |f| f.dip_w),
        Item::Widget(pos) => bar.placed.get(pos).and_then(|p| bar.faces.get(&p.face())).map_or(0.0, |f| f.dip_w),
        Item::Apps => 0.0,
    }
}

/// Width of the collections after the apps (with the space before them).
fn groups_w(n: usize, has_apps: bool) -> f32 {
    if n == 0 {
        0.0
    } else {
        n as f32 * TILE + (n - 1) as f32 * TILE_GAP + if has_apps { SPACE } else { 0.0 }
    }
}

/// Lay the bar out in at most `max_w` DIPs: the left end, the middle, the
/// right end, as Settings > Taskbar > Layout puts them. The ends stay; the
/// middle scrolls when it doesn't fit. The ends may take what leaves the
/// middle room for a few apps: past that (a smaller screen), what's
/// pinned nearest the middle joins it.
pub fn plan(bar: &mut Bar, g: &mut Gfx, max_w: f32) -> Plan {
    let n_groups = bar.apps.groups.len();
    let mut shown: Vec<(Item, Zone, f32)> = bar
        .order
        .clone()
        .into_iter()
        .map(|(it, z)| (it, z, item_w(bar, g, it)))
        .filter(|(it, _, w)| *w > 0.0 || (*it == Item::Apps && (!bar.apps.list.is_empty() || n_groups > 0)))
        .collect();
    let has_apps = shown.iter().any(|x| x.0 == Item::Apps);
    let pinned = |v: &[(Item, Zone, f32)]| v.iter().filter(|x| x.1 != Zone::Mid).map(|x| x.2 + GAP + 4.0).sum::<f32>();
    let room = nookly_core::barcfg::pin_room(max_w);
    while pinned(&shown) > room {
        // The pinned thing nearest the middle joins it.
        let right = shown.iter().position(|x| x.1 == Zone::Right);
        let left = shown.iter().rposition(|x| x.1 == Zone::Left);
        match (left, right) {
            (_, Some(r)) if !left.is_some_and(|l| shown[l].2 > shown[r].2) => shown[r].1 = Zone::Mid,
            (Some(l), _) => shown[l].1 = Zone::Mid,
            _ => break,
        }
    }
    let first = shown.iter().position(|x| x.1 == Zone::Mid);
    let last = shown.iter().rposition(|x| x.1 == Zone::Mid);
    let gap = |i: usize| -> f32 {
        let (a, b) = (&shown[i - 1], &shown[i]);
        // Between an end and the middle: at least a little air (not added
        // to the apps' own: that left a wide hole).
        let g = gap_between(a.0, b.0);
        if a.1 != b.1 { g.max(GAP + SPACE) } else { g }
    };
    // Everything but the apps.
    let others: f32 = shown.iter().filter(|x| x.0 != Item::Apps).map(|x| x.2).sum::<f32>() + PAD * 2.0 + (1..shown.len()).map(gap).sum::<f32>();
    let gw = groups_w(n_groups, !bar.apps.list.is_empty());
    let max = max_apps(&bar.settings);
    let n = bar.apps.list.len();
    let fit = if !has_apps {
        Fit { shown: 0, overflow: false }
    } else if max > 0 {
        // The number you set, whatever the room (the middle scrolls).
        layout::fit_apps(n, max, 1e6, TILE, TILE_GAP)
    } else {
        // As many as fit beside the rest, never fewer than a few.
        layout::fit_apps(n, 0, (max_w - others - gw).max(4.0 * (TILE + TILE_GAP)), TILE, TILE_GAP)
    };
    let apps_w = layout::apps_width(fit, TILE, TILE_GAP) + groups_w(n_groups, fit.shown + fit.overflow as usize > 0);
    let mut x = PAD;
    let mut slots = Vec::new();
    let (mut x0, mut x1) = (0.0, 0.0);
    for i in 0..shown.len() {
        let (it, _, w) = shown[i];
        let w = if it == Item::Apps { apps_w } else { w };
        if i > 0 {
            x += gap(i);
        }
        if Some(i) == first {
            x0 = x;
        }
        slots.push((it, x, w));
        x += w;
        if Some(i) == last {
            x1 = x;
        }
    }
    let in_strip: Vec<bool> = (0..slots.len()).map(|i| first.is_some_and(|f| i >= f) && last.is_some_and(|l| i <= l)).collect();
    let mut width = x + PAD;
    let mut strip = None;
    if width > max_w + 0.5 && first.is_some() {
        let content = x1 - x0;
        let view = (content - (width - max_w)).max(2.0 * TILE);
        for (i, s) in slots.iter_mut().enumerate() {
            if last.is_some_and(|l| i > l) {
                s.1 -= content - view;
            }
        }
        width -= content - view;
        strip = Some((x0, view, content));
    }
    Plan { slots, fit, width: width.max(120.0), strip, in_strip }
}

/// A face the notch drew, centred on `cy`, on whole pixels (it was drawn
/// for them: anything else would blur it).
fn face(ui: &mut Ui, bmp: &ID2D1Bitmap, x: f32, cy: f32, w: f32) {
    let s = ui.scale;
    let h = nookly_core::surfaces::FACE_H;
    let px = (x * s).round() / s;
    let py = ((cy - h / 2.0) * s).round() / s;
    let sz = unsafe { bmp.GetPixelSize() };
    let own = (sz.width as f32 / s, sz.height as f32 / s);
    // (Drawn for the main screen's scale: on a screen with another, at its size.)
    let (bw, bh) = if (own.0 - w).abs() > 1.0 && w > 0.0 { (w, h) } else { own };
    ui.image_rr(bmp, Rect::new(px, py, bw, bh), 0.0);
}

/// A face as the notch drew it last (a change shows at once).
fn draw_face(bar: &mut Bar, ui: &mut Ui, id: &str, x: f32, cy: f32, w: f32) {
    if let Some((bmp, _)) = bar.faces.bitmap(ui.g, id) {
        face(ui, &bmp, x, cy, w);
    }
}

/// A hover/press highlight behind a button.
fn highlight(ui: &mut Ui, r: Rect, id: u32, down: Option<u32>) {
    if down == Some(id) && ui.hovered(r) {
        ui.fill_rr(r, 10.0, col::BTN_HOVER.with_a(0.24));
    } else if ui.hovered(r) {
        ui.fill_rr(r, 10.0, col::BTN_HOVER);
    }
}

/// The Windows logo as four small squares.
fn start_logo(ui: &mut Ui, r: Rect, c: Color) {
    let s = 7.5;
    let gap = 1.6;
    let x0 = r.cx() - s - gap / 2.0;
    let y0 = r.cy() - s - gap / 2.0;
    for (dx, dy) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
        ui.fill_rr(Rect::new(x0 + dx * (s + gap), y0 + dy * (s + gap), s, s), 1.5, c);
    }
}

/// An app's icon (or its first letter while there's none).
pub fn app_icon(bar_icons: &mut crate::Icons, ui: &mut Ui, app: &apps::App, r: Rect, list: &mut apps::Apps) {
    let px = (r.w * ui.scale).round().max(16.0) as u32;
    if let Some(bmp) = bar_icons.get(ui.g, list, app, px) {
        ui.image_rr(&bmp, r, 0.0);
    } else {
        ui.fill_rr(r, 6.0, col::TEXT.with_a(0.14));
        let ch: String = app.name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
        ui.text(&ch, r, 12.0, col::TEXT, Weight::Semi, Align::Center);
    }
}

pub fn draw(bar: &mut Bar, ui: &mut Ui, body: Rect, plan: &Plan) {
    let cy = body.y + body.h / 2.0;
    let down = bar.down;
    // The strip (apps and widgets) scrolls inside its window when it
    // doesn't fit: drawn moved by the scroll, clipped, and only what shows
    // can be hovered or clicked.
    let clip = plan.strip.map(|(x0, view, _)| Rect::new(body.x + x0 - 1.0, body.y, view + 2.0, body.h));
    bar.strip_clip = None;
    let mut hits_from = None;
    for (k, &(it, x, w)) in plan.slots.iter().enumerate() {
        let inside = plan.in_strip.get(k).copied().unwrap_or(false);
        if inside && hits_from.is_none() {
            hits_from = Some(ui.hits.len());
            if let Some((c, (_, view, content))) = clip.zip(plan.strip) {
                // Fade out at an edge only where more is hidden past it.
                let room = (content - view).max(0.0);
                let left = if bar.scroll > 0.5 { 26.0 } else { 0.0 };
                let right = if bar.scroll < room - 0.5 { 26.0 } else { 0.0 };
                ui.push_fade(c, left, right);
                bar.strip_clip = Some(c);
            }
        }
        if !inside && hits_from.is_some() && k > 0 && plan.in_strip.get(k - 1).copied().unwrap_or(false) {
            end_strip(ui, clip, hits_from.unwrap_or(0));
            bar.strip_clip = None;
        }
        let x = if inside && clip.is_some() { x - bar.scroll } else { x };
        let r = Rect::new(body.x + x, cy - BTN / 2.0, w, BTN);
        match it {
            Item::Start => {
                highlight(ui, r, START, down);
                start_logo(ui, r, col::BLUE.lerp(col::CYAN, 0.35));
                ui.hit(r, START);
            }
            Item::Search => {
                highlight(ui, r, SEARCH, down);
                ui.icon("\u{E721}", r, 16.0, col::TEXT);
                ui.hit(r, SEARCH);
            }
            Item::Tray => {
                highlight(ui, r, TRAY, down);
                ui.icon("\u{E70E}", r, 12.0, col::DIM);
                ui.hit(r, TRAY);
            }
            Item::Notifs => {
                highlight(ui, r, NOTIFS, down);
                ui.icon(glyph::BELL, r, 15.0, col::TEXT);
                badge(ui, r.centered(TILE, TILE), crate::unread::count() as usize);
                ui.hit(r, NOTIFS);
            }
            Item::Lang | Item::Quick | Item::Clock => {
                let id = match it {
                    Item::Lang => LANG,
                    Item::Quick => QUICK,
                    _ => CLOCK,
                };
                highlight(ui, r, id, down);
                if let Some(f) = sys_face(it) {
                    draw_face(bar, ui, f, r.x, cy, w);
                }
                ui.hit(r, id);
            }
            Item::Apps => draw_apps(bar, ui, Rect::new(body.x + x, cy - TILE / 2.0, w, TILE), plan.fit),
            Item::Widget(pos) => draw_widget(bar, ui, pos, body.x + x, cy, w),
        }
    }
    if plan.in_strip.last().copied().unwrap_or(false) {
        if let Some(from) = hits_from {
            end_strip(ui, clip, from);
        }
    }
    bar.strip_clip = None;
    // Where the strip is scrolled to: a thin line under it, only while the
    // mouse is over the strip (or it's still gliding).
    if let Some((x0, view, content)) = plan.strip.filter(|&(x0, view, _)| ui.hovered(Rect::new(body.x + x0, body.y, view, body.h)) || (bar.scroll - bar.scroll_target()).abs() > 0.3) {
        let room = (content - view).max(1.0);
        let thumb = (view * view / content).max(24.0);
        let tx = body.x + x0 + (bar.scroll / room).clamp(0.0, 1.0) * (view - thumb);
        ui.fill_rr(Rect::new(body.x + x0, body.b() - 3.5, view, 2.0), 1.0, col::TEXT.with_a(0.08));
        ui.fill_rr(Rect::new(tx, body.b() - 3.5, thumb, 2.0), 1.0, col::TEXT.with_a(0.35));
    }
    draw_dragged(bar, ui);
    draw_focus(bar, ui);
}

/// Used from the keyboard: a ring around the thing it's on.
fn draw_focus(bar: &Bar, ui: &mut Ui) {
    let Some(id) = bar.key_on() else { return };
    if let Some(r) = ui.hits.iter().rev().find(|h| h.id == id).map(|h| h.rect) {
        ui.stroke_rr(r.inset(1.0), 10.0, col::ACCENT, 2.0);
    }
}

/// What a thing is called (read out when it's reached from the keyboard).
pub fn name(bar: &Bar, id: u32) -> String {
    let app = |a: &apps::App| match a.windows.len() {
        0 => format!("{}, not open", a.name),
        1 => format!("{}, open", a.name),
        n => format!("{}, {n} windows", a.name),
    };
    let title = |id: u32| pop_window(bar, id).map(apps::title).unwrap_or_default();
    match id {
        START => "Start".into(),
        SEARCH => "Search".into(),
        TRAY => "Hidden icons".into(),
        NOTIFS => match crate::unread::count() {
            0 => "Notifications".into(),
            n => format!("Notifications, {n} new"),
        },
        LANG => "Keyboard language".into(),
        QUICK => "Wi-Fi, sound and battery".into(),
        CLOCK => "Date and time".into(),
        OVERFLOW => format!("More apps, {}", bar.apps.list.len().saturating_sub(bar.plan_fit.shown)),
        i if (APP..APP + 900).contains(&i) => bar.apps.list.get((i - APP) as usize).map(app).unwrap_or_default(),
        i if (POP_APP..POP_APP + 1000).contains(&i) || (POP_MEMBER..POP_MEMBER + 100).contains(&i) => grid_app(bar, i).as_ref().map(app).unwrap_or_default(),
        i if (GROUP..GROUP + 100).contains(&i) => bar.apps.groups.get((i - GROUP) as usize).map(|g| format!("Collection {}", g.name)).unwrap_or_default(),
        i if (WIDGET..WIDGET + 500).contains(&i) => bar.placed.get((i - WIDGET) as usize).and_then(|p| nookly_core::barcfg::def(&p.id)).map(|d| d.name.to_string()).unwrap_or_default(),
        i if (TRAY_ICON..TRAY_ICON + 200).contains(&i) => crate::tray::icons().get((i - TRAY_ICON) as usize).map(|t| t.tip.clone()).unwrap_or_default(),
        POP_OPEN | SUB_OPEN => "New window".into(),
        i if (POP_CLOSE..POP_CLOSE + 100).contains(&i) || (SUB_CLOSE..SUB_CLOSE + 100).contains(&i) => format!("Close {}", title(i)),
        i if (POP_ROW..POP_ROW + 100).contains(&i) || (SUB_ROW..SUB_ROW + 100).contains(&i) => title(i),
        FULL_VIEW => "Widget".into(),
        _ => String::new(),
    }
}

/// Where the arrows go from `from` in a row of things (their rects): the
/// next one along, or (up and down) the nearest in the next row.
pub fn key_step(rects: &[(u32, Rect)], from: Option<u32>, dx: i32, dy: i32) -> Option<u32> {
    if rects.is_empty() {
        return None;
    }
    let mut v: Vec<(u32, Rect)> = rects.to_vec();
    v.sort_by(|a, b| ((a.1.cy() / 12.0).round(), a.1.x).partial_cmp(&((b.1.cy() / 12.0).round(), b.1.x)).unwrap_or(std::cmp::Ordering::Equal));
    let Some(k) = from.and_then(|f| v.iter().position(|x| x.0 == f)) else { return Some(v[0].0) };
    if dy != 0 {
        let c = v[k].1;
        let next = v
            .iter()
            .filter(|x| (x.1.cy() - c.cy()) * dy as f32 > 8.0)
            .min_by(|a, b| {
                let d = |r: &Rect| (r.cy() - c.cy()).abs() * 4.0 + (r.cx() - c.cx()).abs();
                d(&a.1).total_cmp(&d(&b.1))
            })
            .map(|x| x.0);
        return next.or(Some(v[k].0));
    }
    let n = v.len() as i32;
    Some(v[((k as i32 + dx).rem_euclid(n)) as usize].0)
}

/// The app being dragged follows the mouse (drawn by the bar and by the
/// pop-up it came from: each shows its part).
fn draw_dragged(bar: &mut Bar, ui: &mut Ui) {
    let Some(app) = bar.dragging().and_then(|k| bar.apps.list.get(k)).cloned() else { return };
    let (mx, my) = ui.mouse;
    let r = Rect::new(mx - 14.0, my - 14.0, 28.0, 28.0);
    let old = ui.alpha;
    ui.alpha = 0.85;
    ui.fill_rr(r.inset(-4.0), 10.0, col::BTN_HOVER);
    app_icon(&mut bar.icons, ui, &app, r, &mut bar.apps);
    ui.alpha = old;
}

/// A line where a dragged app would go (centre x, the tiles' row).
fn drop_line(ui: &mut Ui, x: f32, y: f32) {
    ui.fill_rr(Rect::new(x - 1.5, y + 5.0, 3.0, TILE - 10.0), 1.5, col::ACCENT);
}

/// The strip's end: stop clipping, and keep only the parts of its hits
/// that show (a tile scrolled out of sight can't be hovered or clicked).
fn end_strip(ui: &mut Ui, clip: Option<Rect>, from: usize) {
    let Some(c) = clip else { return };
    ui.pop_layer();
    let mut k = from;
    while k < ui.hits.len() {
        let r = ui.hits[k].rect;
        let (x0, x1) = (r.x.max(c.x), r.r().min(c.r()));
        if x1 - x0 < 1.0 {
            ui.hits.remove(k);
        } else {
            ui.hits[k].rect = Rect::new(x0, r.y, x1 - x0, r.h);
            k += 1;
        }
    }
}

/// A widget: the picture the notch drew, at its size.
fn draw_widget(bar: &mut Bar, ui: &mut Ui, pos: usize, x: f32, cy: f32, w: f32) {
    let Some(p) = bar.placed.get(pos).cloned() else { return };
    let r = Rect::new(x, cy - nookly_core::surfaces::FACE_H / 2.0, w, nookly_core::surfaces::FACE_H);
    let id = WIDGET + pos as u32;
    // The card is in the picture: the hover only lifts it a little.
    if ui.hovered(r) {
        ui.fill_rr(Rect::new(r.x + 1.0, r.y + 3.0, r.w - 2.0, r.h - 6.0), 11.0, col::TEXT.with_a(if bar.down == Some(id) { 0.03 } else { 0.07 }));
    }
    draw_face(bar, ui, &p.face(), r.x, cy, w);
    ui.hit(r, id);
}

/// The app tile under a drag that it would be dropped on (to make or
/// grow a collection).
fn drop_target(bar: &Bar, id: u32) -> bool {
    matches!(bar.drop_at(), Some(crate::Drop::Onto(i)) if i == id)
}

fn tile_state(bar: &Bar, ui: &mut Ui, t: Rect, id: u32, open: bool) {
    if drop_target(bar, id) {
        ui.fill_rr(t, 10.0, col::ACCENT.with_a(0.22));
        ui.stroke_rr(t.inset(0.5), 9.5, col::ACCENT, 1.5);
    } else if open {
        ui.fill_rr(t, 10.0, col::BTN_HOVER);
    } else {
        highlight(ui, t, id, bar.down);
    }
}

/// A number in a solid accent pill at a tile's top right (0: none).
fn badge(ui: &mut Ui, t: Rect, n: usize) {
    if n == 0 {
        return;
    }
    let label = if n > 99 { "99+".to_string() } else { n.to_string() };
    let bw = (ui.measure(&label, 9.5, Weight::Bold) + 8.0).max(15.0);
    let b = Rect::new(t.r() - bw + 1.0, t.y + 1.0, bw, 15.0);
    ui.fill_rr(b.inset(-1.5), 9.0, col::NOTCH.with_a(0.55));
    ui.fill_rr(b, 7.5, col::ACCENT);
    ui.text(&label, b, 9.5, hex(0x1A1A1A), Weight::Bold, Align::Center);
}

/// An app that wants to be looked at: a red glow behind its tile (a few
/// beats, then steady), and its dot a red bar.
fn attention_glow(bar: &Bar, ui: &mut Ui, t: Rect) {
    ui.fill_rr(t, 10.0, col::RED.with_a(0.16 + 0.22 * bar.attention_beat()));
}

/// The line under an app's icon: a dot while it's open, an accent bar while
/// it's in front, a red one when it wants to be looked at.
fn app_mark(bar: &mut Bar, ui: &mut Ui, t: Rect, app: &apps::App) {
    if app.windows.is_empty() {
        return;
    }
    // A badge the app put on its icon.
    if let Some((ver, icon)) = crate::band::badge(&app.windows) {
        if let Some(b) = bar.icons.badge(ui.g, ver, &icon) {
            let z = 15.0;
            let r = Rect::new(t.cx() + 4.0, t.cy() + 2.0, z, z);
            ui.image_rr(&b, r, 0.0);
        }
    }
    // Its progress, in the place of the line.
    if draw_progress(ui, t, &app.windows) {
        return;
    }
    let front = app.windows.contains(&bar.apps.active);
    let wants = bar.apps.wants(app);
    let w = if front || wants { 14.0 } else { 5.0 };
    let c = if wants {
        col::RED
    } else if front {
        col::ACCENT
    } else {
        col::DIM
    };
    ui.fill_rr(Rect::new(t.cx() - w / 2.0, t.b() - 3.5, w, 3.0), 1.5, c);
}

/// The progress of these windows along a tile's bottom, if there is any.
fn draw_progress(ui: &mut Ui, t: Rect, windows: &[HWND]) -> bool {
    if let Some((state, v)) = crate::band::progress(windows) {
        use crate::band::State;
        let track = Rect::new(t.x + 6.0, t.b() - 4.5, t.w - 12.0, 4.0);
        ui.fill_rr(track, 2.0, col::TEXT.with_a(0.18));
        let c = match state {
            State::Error => col::RED,
            State::Paused => col::YELLOW,
            _ => col::GREEN,
        };
        let fill = match state {
            State::Busy => {
                // Not known how much: a part of it goes to and fro.
                let k = ((ui.t * 2.4).sin() * 0.5 + 0.5) as f32;
                Rect::new(track.x + k * (track.w - 10.0), track.y, 10.0, track.h)
            }
            _ => Rect::new(track.x, track.y, (track.w * v).max(4.0), track.h),
        };
        ui.fill_rr(fill, 2.0, c);
        return true;
    }
    false
}

fn draw_apps(bar: &mut Bar, ui: &mut Ui, r: Rect, fit: Fit) {
    let list = bar.apps.list.clone();
    let dragging = bar.dragging();
    // The app whose menu is open stays lit.
    let menu_key = bar.menu_app();
    for (i, app) in list.iter().take(fit.shown).enumerate() {
        let t = Rect::new(r.x + i as f32 * (TILE + TILE_GAP), r.y, TILE, TILE);
        let id = APP + i as u32;
        let open = matches!(bar.popup, Some(Pop::App(k, _)) if k == i) || menu_key.as_deref() == Some(app.key.as_str());
        if bar.apps.wants(app) {
            attention_glow(bar, ui, t);
        }
        tile_state(bar, ui, t, id, open);
        let old = ui.alpha;
        if dragging == Some(i) {
            ui.alpha = old * 0.35;
        }
        app_icon(&mut bar.icons, ui, app, t.centered(ICON, ICON), &mut bar.apps);
        ui.alpha = old;
        app_mark(bar, ui, t, app);
        ui.alpha = old;
        ui.hit(t, id);
    }
    // Where a dragged app would go: a line between the tiles.
    if let Some(crate::Drop::Between(k, true)) = bar.drop_at() {
        drop_line(ui, r.x + k as f32 * (TILE + TILE_GAP) - TILE_GAP / 2.0, r.y);
    }
    let mut x = r.x + fit.shown as f32 * (TILE + TILE_GAP);
    if fit.overflow {
        let t = Rect::new(x, r.y, TILE, TILE);
        x += TILE + TILE_GAP;
        let open = matches!(bar.popup, Some(Pop::Overflow(_)));
        let rest = &list[fit.shown.min(list.len())..];
        if rest.iter().any(|a| bar.apps.wants(a)) {
            attention_glow(bar, ui, t);
        }
        tile_state(bar, ui, t, OVERFLOW, open);
        if bar.settings.extra.get("bar_overflow").is_some_and(|v| v == "stack") && !rest.is_empty() {
            // Up to three of the hidden apps' icons, stacked, the group
            // centred in the tile.
            let k = rest.len().min(3);
            let (sz, dx, dy) = (18.0, 4.0, 1.5);
            let (gw, gh) = (sz + dx * (k - 1) as f32, sz + dy * (k - 1) as f32);
            let (x0, y0) = (t.cx() - gw / 2.0, t.cy() - gh / 2.0 + dy * (k - 1) as f32);
            for (j, app) in rest.iter().take(3).enumerate().rev() {
                app_icon(&mut bar.icons, ui, app, Rect::new(x0 + j as f32 * dx, y0 - j as f32 * dy, sz, sz), &mut bar.apps);
            }
        } else {
            // Three dots, drawn (a font glyph sits off centre).
            for k in -1..=1 {
                ui.circle(t.cx() + k as f32 * 5.5, t.cy(), 1.9, col::TEXT);
            }
        }
        // How many are hidden, and how far what they're doing is.
        badge(ui, t, rest.len());
        let windows: Vec<HWND> = rest.iter().flat_map(|a| a.windows.iter().copied()).collect();
        draw_progress(ui, t, &windows);
        ui.hit(t, OVERFLOW);
    }
    // Collections, after the apps: a tile with up to four of their icons.
    let groups = bar.apps.groups.clone();
    if !groups.is_empty() && fit.shown + fit.overflow as usize > 0 {
        x += SPACE - TILE_GAP;
    }
    for (g, grp) in groups.iter().enumerate() {
        let t = Rect::new(x + g as f32 * (TILE + TILE_GAP), r.y, TILE, TILE);
        let id = GROUP + g as u32;
        let open = matches!(bar.popup, Some(Pop::Group(k, _)) if k == g);
        // (Its open apps also have tiles of their own: they do the asking.)
        tile_state(bar, ui, t, id, open);
        let inner = t.centered(28.0, 28.0);
        ui.fill_rr(inner, 8.0, col::TEXT.with_a(0.10));
        for (k, app) in grp.members.iter().take(4).enumerate() {
            let (cx, cy) = ((k % 2) as f32, (k / 2) as f32);
            app_icon(&mut bar.icons, ui, app, Rect::new(inner.x + 3.0 + cx * 11.5, inner.y + 3.0 + cy * 11.5, 10.5, 10.5), &mut bar.apps);
        }
        if grp.members.iter().any(|a| !a.windows.is_empty()) {
            ui.fill_rr(Rect::new(t.cx() - 2.5, t.b() - 3.5, 5.0, 3.0), 1.5, col::DIM);
        }
        ui.hit(t, id);
    }
}

/// The apps in the "more apps" grid.
pub fn more_apps(bar: &Bar) -> Vec<(usize, apps::App)> {
    bar.apps.list.iter().cloned().enumerate().skip(bar.plan_fit.shown).take(36).collect()
}

/// Previews (live thumbnails) for this app's windows, or a list: the
/// setting, and the list when there are more windows than fit side by side.
fn use_previews(bar: &Bar, app: &apps::App) -> bool {
    // Room inside the widest bar, with space for a bubble inside a bubble.
    let room = bar.max_bar_w() - 4.0 * crate::HOLD - 20.0;
    let fits = ((room + CARD_GAP) / (CARD_W + CARD_GAP)).floor().max(1.0) as usize;
    previews(&bar.settings) && !app.windows.is_empty() && app.windows.len() <= fits.min(6)
}

/// Size of an app's windows pop-up: a header, then previews or a list.
pub fn windows_size(bar: &Bar, app: &apps::App) -> (f32, f32) {
    if use_previews(bar, app) {
        let n = app.windows.len() as f32;
        ((n * (CARD_W + CARD_GAP) - CARD_GAP + 20.0).max(240.0), 20.0 + 34.0 + CARD_H)
    } else {
        (300.0, 20.0 + 34.0 + app.windows.len().min(8).max(1) as f32 * 36.0)
    }
}

/// A strip of `n` app icons above the bar, like a second row of it: one
/// row as wide as the bar (less room for the flares), wrapping onto more
/// rows only when there are more apps than that.
fn grid(bar: &Bar, n: usize) -> (usize, usize) {
    let room = bar.content_w() - 2.0 * crate::HOLD - 20.0;
    let fit = ((room / CELL_W).floor() as usize).max(1);
    let cols = n.max(1).min(fit);
    (cols, n.max(1).div_ceil(cols))
}
/// Grid cells: the same tiles as on the bar (icon only, no names).
const CELL_W: f32 = TILE + 6.0;
const CELL_H: f32 = TILE + 6.0;

/// How wide a grid's bubble is from the start so the windows bubble of any
/// of its apps stands on it whole: it never has to widen while open (its
/// cells would slide under the mouse onto another app).
fn room_for_windows(bar: &Bar, list: &[apps::App]) -> f32 {
    list.iter().map(|a| windows_size(bar, a).0 + 2.0 * crate::HOLD).fold(0.0, f32::max)
}

/// A pop-up's size (DIPs).
pub fn pop_size(bar: &Bar, pop: Pop) -> (f32, f32) {
    match pop {
        Pop::App(i, _) => bar.apps.list.get(i).map_or((240.0, 60.0), |a| windows_size(bar, a)),
        Pop::Overflow(_) => {
            let list: Vec<apps::App> = more_apps(bar).into_iter().map(|x| x.1).collect();
            let (c, r) = grid(bar, list.len());
            ((c as f32 * CELL_W + 20.0).max(room_for_windows(bar, &list)), r as f32 * CELL_H + 20.0)
        }
        Pop::Menu(_) => bar.menu.as_ref().map_or((228.0, 60.0), |m| m.size()),
        Pop::Tray(_) => {
            let n = crate::tray::icons().len();
            let (c, r) = tray_grid(n);
            ((c as f32 * TRAY_CELL + 20.0).max(180.0), r as f32 * TRAY_CELL + 20.0 + TRAY_TIP)
        }
        Pop::Widget(..) => match full_face(bar).and_then(|n| bar.faces.get(&n).cloned()) {
            Some(f) => (f.dip_w + 2.0 * FULL_PAD, f.h as f32 * f.dip_w / f.w.max(1) as f32 + 2.0 * FULL_PAD),
            None => (240.0, 80.0),
        },
        Pop::Group(g, _) => {
            let list: Vec<apps::App> = bar.apps.groups.get(g).map_or(Vec::new(), |x| x.members.clone());
            let (c, r) = grid(bar, list.len().max(1));
            ((c as f32 * CELL_W + 20.0).max(170.0).max(room_for_windows(bar, &list)), 30.0 + r as f32 * CELL_H + 20.0)
        }
    }
}

/// Ids for one window list: rows, close buttons and "New".
#[derive(Clone, Copy)]
pub struct Ids {
    pub row: u32,
    pub close: u32,
    pub open: u32,
}
pub const POP_IDS: Ids = Ids { row: POP_ROW, close: POP_CLOSE, open: POP_OPEN };
pub const SUB_IDS: Ids = Ids { row: SUB_ROW, close: SUB_CLOSE, open: SUB_OPEN };

/// A window's title, and a close button while hovered. (The row's own hit
/// goes first, so the close button, registered after it, wins.)
fn title_row(bar: &Bar, ui: &mut Ui, row: Rect, h: HWND, close_id: u32, size: f32) {
    let front = h == bar.apps.active;
    let mut inner = row.inset_xy(8.0, 0.0);
    if ui.hovered(row) {
        let x = inner.cut_right(22.0, 6.0).centered(22.0, 22.0);
        ui.icon_btn(x, glyph::CLOSE, close_id, 8.5, col::DIM, Some(col::BTN), "");
    }
    ui.text(&apps::title(h), inner, size, if front { col::TEXT } else { col::DIM }, Weight::Regular, Align::Left);
}

/// An app's windows: its name and "New", then a live preview of each
/// window (or a list of them).
pub fn draw_windows(bar: &mut Bar, ui: &mut Ui, app: &apps::App, r: Rect, ids: Ids) {
    let mut c = r.inset(10.0);
    let mut head = c.cut_top(30.0, 4.0).inset_xy(4.0, 0.0);
    app_icon(&mut bar.icons, ui, app, head.cut_left(18.0, 10.0).centered(18.0, 18.0), &mut bar.apps);
    let name = if app.name.is_empty() { "App" } else { app.name.as_str() };
    if app.launch.is_empty() {
        ui.text(name, head, 12.5, col::TEXT, Weight::Semi, Align::Left);
    } else {
        let open = head.cut_right(60.0, 0.0).centered(60.0, 24.0);
        ui.text(name, head, 12.5, col::TEXT, Weight::Semi, Align::Left);
        ui.button(open, "New", ids.open);
    }
    if app.windows.is_empty() {
        ui.text("Not open", c.cut_top(30.0, 0.0).inset_xy(6.0, 0.0), 12.0, col::DIM, Weight::Regular, Align::Left);
    } else if use_previews(bar, app) {
        for (j, &h) in app.windows.iter().enumerate() {
            let card = Rect::new(c.x + j as f32 * (CARD_W + CARD_GAP), c.y, CARD_W, CARD_H);
            let front = h == bar.apps.active;
            if ui.hovered(card) {
                ui.fill_rr(card, 12.0, col::BTN_HOVER);
            } else if front {
                ui.fill_rr(card, 12.0, col::BTN);
            }
            ui.hit(card, ids.row + j as u32);
            let mut inner = card.inset(6.0);
            title_row(bar, ui, inner.cut_top(24.0, 4.0), h, ids.close + j as u32, 11.5);
            let slot = inner;
            let iconic = unsafe { windows::Win32::UI::WindowsAndMessaging::IsIconic(h).as_bool() };
            if iconic {
                // Minimized windows have no picture: the app's icon.
                ui.fill_rr(slot, 8.0, col::TEXT.with_a(0.05));
                app_icon(&mut bar.icons, ui, app, slot.centered(36.0, 36.0), &mut bar.apps);
                ui.text("Minimized", Rect::new(slot.x, slot.b() - 22.0, slot.w, 18.0), 10.0, col::FAINT, Weight::Regular, Align::Center);
            } else {
                bar.thumb_slots.push((h, slot));
            }
        }
    } else {
        for (j, &h) in app.windows.iter().take(8).enumerate() {
            let row = c.cut_top(34.0, 2.0);
            let front = h == bar.apps.active;
            if ui.hovered(row) {
                ui.fill_rr(row, 10.0, col::BTN_HOVER);
            } else if front {
                ui.fill_rr(row, 10.0, col::BTN);
            }
            ui.hit(row, ids.row + j as u32);
            title_row(bar, ui, row.inset_xy(2.0, 0.0), h, ids.close + j as u32, 12.0);
        }
    }
}

/// A grid of app icons (the "more apps" tile's, or a collection's).
fn draw_grid(bar: &mut Bar, ui: &mut Ui, list: &[(u32, apps::App)], c: Rect) {
    let (cols, _) = grid(bar, list.len());
    // Centred (the bubble may be wider, holding a bubble grown out of it).
    let gw = cols as f32 * CELL_W;
    let c = Rect::new(c.x + ((c.w - gw) / 2.0).max(0.0), c.y, gw, c.h);
    let hot = bar.sub_app.as_ref().map(|(a, _)| a.key.clone());
    let dragging = bar.dragging().map(|k| POP_APP + k as u32);
    let drop = bar.drop_at();
    for (k, (id, app)) in list.iter().enumerate() {
        let (cx, cy) = ((k % cols) as f32, (k / cols) as f32);
        let t = Rect::new(c.x + cx * CELL_W, c.y + cy * CELL_H, CELL_W, CELL_H).centered(TILE, TILE);
        if bar.apps.wants(app) {
            attention_glow(bar, ui, t);
        }
        if drop == Some(crate::Drop::Onto(*id)) {
            ui.fill_rr(t, 10.0, col::ACCENT.with_a(0.22));
            ui.stroke_rr(t.inset(0.5), 9.5, col::ACCENT, 1.5);
        } else if ui.hovered(t) || hot.as_deref() == Some(app.key.as_str()) {
            ui.fill_rr(t, 10.0, col::BTN_HOVER);
        }
        let old = ui.alpha;
        if dragging == Some(*id) {
            ui.alpha = old * 0.35;
        }
        app_icon(&mut bar.icons, ui, app, t.centered(ICON, ICON), &mut bar.apps);
        ui.alpha = old;
        // Where a dragged app would go: a line on this cell's side.
        if let Some(crate::Drop::Between(n, false)) = drop {
            let here = *id == POP_APP + n as u32;
            let after = *id + 1 == POP_APP + n as u32 && k + 1 == list.len();
            if here || after {
                let side = (CELL_W - TILE) / 2.0;
                drop_line(ui, if here { t.x - side } else { t.r() + side }, t.y);
            }
        }
        app_mark(bar, ui, t, app);
        ui.hit_tip(t, *id, "");
    }
}

/// A pop-up's content, in `r` (the bubble draws its shape around it).
pub fn draw_pop(bar: &mut Bar, ui: &mut Ui, pop: Pop, r: Rect) {
    bar.thumb_slots.clear();
    match pop {
        Pop::App(i, _) => {
            if let Some(app) = bar.apps.list.get(i).cloned() {
                draw_windows(bar, ui, &app, r, POP_IDS);
            }
        }
        Pop::Overflow(_) => {
            let list: Vec<(u32, apps::App)> = more_apps(bar).into_iter().map(|(i, a)| (POP_APP + i as u32, a)).collect();
            draw_grid(bar, ui, &list, r.inset(10.0));
            draw_dragged(bar, ui);
        }
        Pop::Group(g, _) => {
            let Some(grp) = bar.apps.groups.get(g).cloned() else { return };
            let mut c = r.inset(10.0);
            ui.text(&grp.name, c.cut_top(26.0, 4.0).inset_xy(6.0, 0.0), 12.5, col::TEXT, Weight::Semi, Align::Left);
            let list: Vec<(u32, apps::App)> = grp.members.iter().cloned().enumerate().map(|(k, a)| (POP_MEMBER + k as u32, a)).collect();
            draw_grid(bar, ui, &list, c);
        }
        Pop::Tray(_) => draw_tray(bar, ui, r),
        Pop::Menu(_) => crate::menu::draw(bar, ui, r),
        Pop::Widget(..) => {
            // The full view the notch drew, on whole pixels.
            let Some(name) = full_face(bar) else { return };
            match bar.faces.bitmap(ui.g, &name) {
                Some((bmp, w)) => {
                    let f = bar.faces.get(&name).cloned().unwrap_or_default();
                    let h = f.h as f32 * w / f.w.max(1) as f32;
                    let s = ui.scale;
                    let at = Rect::new(((r.cx() - w / 2.0) * s).round() / s, ((r.cy() - h / 2.0) * s).round() / s, w, h);
                    ui.image_rr(&bmp, at, 0.0);
                    ui.hit(at, FULL_VIEW);
                }
                None => ui.text("\u{2026}", r, 14.0, col::DIM, Weight::Semi, Align::Center),
            }
        }
    }
    draw_focus(bar, ui);
}

/// The app behind a grid cell id (the "more apps" grid or a collection).
pub fn grid_app(bar: &Bar, id: u32) -> Option<apps::App> {
    match id {
        i if (POP_APP..POP_APP + 1000).contains(&i) => bar.apps.list.get((i - POP_APP) as usize).cloned(),
        i if (POP_MEMBER..POP_MEMBER + 100).contains(&i) => match bar.popup {
            Some(Pop::Group(g, _)) => bar.apps.groups.get(g)?.members.get((i - POP_MEMBER) as usize).cloned(),
            _ => None,
        },
        _ => None,
    }
}

/// Open or bring forward an app: start it, bring its front window
/// forward, or (already in front) minimize it.
fn open_app(app: &apps::App, _cached: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{GA_ROOTOWNER, GetAncestor, GetForegroundWindow, IsIconic};
    match app.front.as_ref().or(app.windows.first()) {
        None => apps::launch(&app.launch),
        Some(&h) => {
            // The window in front right now (not the one seen at the last
            // scan, which can be out of date): if it's one of this app's
            // (or a dialog of one), minimize it; else bring the app forward.
            let front = unsafe {
                let f = GetForegroundWindow();
                let root = GetAncestor(f, GA_ROOTOWNER);
                app.windows.iter().copied().find(|&w| w == f || w == root).filter(|&w| !IsIconic(w).as_bool())
            };
            crate::dbg_log(&format!("open {}: first {:?} front {:?}", app.name, h.0, front.map(|w| w.0)));
            match front {
                Some(w) => apps::minimize(w),
                None => apps::activate(h),
            }
        }
    }
}

/// Click an app's tile. With several windows and click-to-open, list them
/// instead.
fn click_app(bar: &mut Bar, i: usize, x: f32) {
    let Some(app) = bar.apps.list.get(i).cloned() else { return };
    if app.windows.len() > 1 && !bar.hover_opens() {
        bar.toggle_pop(Pop::App(i, x));
        return;
    }
    bar.set_pop(None);
    open_app(&app, bar.apps.active);
}

/// The window behind a row or close-button id (the app's pop-up, or the
/// windows bubble grown out of a grid).
pub fn pop_window(bar: &Bar, id: u32) -> Option<HWND> {
    let (app, base) = if (SUB_ROW..SUB_OPEN).contains(&id) {
        (bar.sub_app.as_ref()?.0.clone(), if id >= SUB_CLOSE { SUB_CLOSE } else { SUB_ROW })
    } else {
        match bar.popup? {
            Pop::App(k, _) => (bar.apps.list.get(k)?.clone(), if id >= POP_CLOSE { POP_CLOSE } else { POP_ROW }),
            _ => return None,
        }
    };
    app.windows.get((id - base) as usize).copied()
}

pub fn action(bar: &mut Bar, id: u32, x: f32) {
    match id {
        START => shell::open_start(),
        SEARCH => shell::open_search(),
        TRAY => {
            if crate::tray::running() {
                let p = Pop::Tray(x);
                if bar.hover_opens() && Pop::same(bar.popup, p) {
                    bar.hover_block = None;
                } else {
                    bar.toggle_pop(p);
                }
            } else {
                shell::open_hidden_icons();
            }
        }
        i if (TRAY_ICON..TRAY_ICON + 200).contains(&i) => tray_click(bar, i, false),
        NOTIFS => {
            crate::unread::mark_seen();
            shell::open_notifications();
        }
        LANG => shell::press(&[windows::Win32::UI::Input::KeyboardAndMouse::VK_LWIN, windows::Win32::UI::Input::KeyboardAndMouse::VK_SPACE]),
        QUICK => shell::open_quick_settings(),
        CLOCK => {
            crate::unread::mark_seen();
            shell::open_notifications();
        }
        DESKTOP => crate::menu::show_desktop(),
        i if (crate::menu::ID..crate::menu::ID + 100).contains(&i) => crate::menu::action(bar, i),
        OVERFLOW => bar.toggle_pop(Pop::Overflow(x)),
        POP_OPEN => {
            if let Some(Pop::App(i, _)) = bar.popup {
                if let Some(a) = bar.apps.list.get(i) {
                    apps::launch(&a.launch);
                }
            }
            bar.set_pop(None);
        }
        SUB_OPEN => {
            if let Some((a, _)) = bar.sub_app.clone() {
                apps::launch(&a.launch);
            }
            bar.set_pop(None);
        }
        i if (POP_ROW..POP_ROW + 100).contains(&i) || (SUB_ROW..SUB_ROW + 100).contains(&i) => {
            if let Some(h) = pop_window(bar, i) {
                apps::activate(h);
            }
            bar.set_pop(None);
        }
        i if (POP_CLOSE..POP_CLOSE + 100).contains(&i) || (SUB_CLOSE..SUB_CLOSE + 100).contains(&i) => {
            if let Some(h) = pop_window(bar, i) {
                apps::close(h);
            }
        }
        i if (POP_APP..POP_APP + 1000).contains(&i) => {
            // A "more apps" row: open it (its windows list under it).
            if let Some(app) = bar.apps.list.get((i - POP_APP) as usize).cloned() {
                bar.set_pop(None);
                open_app(&app, bar.apps.active);
            }
        }
        i if (POP_MEMBER..POP_MEMBER + 100).contains(&i) => {
            if let Some(Pop::Group(g, _)) = bar.popup {
                if let Some(app) = bar.apps.groups.get(g).and_then(|grp| grp.members.get((i - POP_MEMBER) as usize)).cloned() {
                    bar.set_pop(None);
                    open_app(&app, bar.apps.active);
                }
            }
        }
        i if (GROUP..GROUP + 100).contains(&i) => bar.toggle_pop(Pop::Group((i - GROUP) as usize, x)),
        i if (WIDGET..WIDGET + 500).contains(&i) => {
            let pos = (i - WIDGET) as usize;
            if widget_panel(bar, pos) {
                // Its bubble: a click opens it (and keeps a hover-opened one).
                let p = Pop::Widget(pos, x);
                if bar.hover_opens() && Pop::same(bar.popup, p) {
                    bar.hover_block = None;
                } else {
                    bar.toggle_pop(p);
                }
            } else if let Some(p) = nookly_core::barcfg::placed(&bar.settings).get(pos) {
                widget_click(&p.id);
            }
        }
        i if (APP..APP + 900).contains(&i) => click_app(bar, (i - APP) as usize, x),
        _ => {}
    }
}

/// The widget at this place on the bar opens its feature's full view in a
/// bubble (the others do their one thing on a click).
pub fn widget_panel(bar: &Bar, pos: usize) -> bool {
    nookly_core::barcfg::placed(&bar.settings).get(pos).is_some_and(|p| nookly_core::barcfg::full_for(&p.id).is_some())
}

/// Tell the notch which widget's full view is open (None: none).
pub fn tell_full(bar: &Bar, pos: Option<usize>) {
    // (Of several bars, the one that opened it closes it.)
    static OWNER: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
    let me = bar.hwnd.0 as isize;
    if pos.is_some() {
        OWNER.store(me, std::sync::atomic::Ordering::Relaxed);
    } else if OWNER.load(std::sync::atomic::Ordering::Relaxed) != me {
        return;
    }
    let k = pos
        .and_then(|p| nookly_core::barcfg::placed(&bar.settings).get(p).cloned())
        .and_then(|p| nookly_core::barcfg::CATALOG.iter().position(|d| d.id == p.id))
        .map_or(0, |k| k + 1);
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, RegisterWindowMessageW};
        if let Ok(notch) = FindWindowW(windows::core::w!("NooklyNotch"), windows::core::PCWSTR::null()) {
            let msg = RegisterWindowMessageW(nookly_core::surfaces::OPEN_MSG);
            let _ = PostMessageW(Some(notch), msg, windows::Win32::Foundation::WPARAM(k), windows::Win32::Foundation::LPARAM(0));
        }
    }
}

/// The open widget's full view: its face name.
fn full_face(bar: &Bar) -> Option<String> {
    match bar.popup {
        Some(Pop::Widget(k, _)) => nookly_core::barcfg::placed(&bar.settings).get(k).map(|p| nookly_core::surfaces::full_face(&p.id)),
        _ => None,
    }
}

/// Room around a full view inside its bubble.
const FULL_PAD: f32 = 12.0;

/// Where the open full view is (bar coordinates), once the notch drew it.
pub fn full_rect(bar: &Bar) -> Option<Rect> {
    let f = bar.faces.get(&full_face(bar)?)?;
    let c = bar.pop_content()?;
    let h = f.h as f32 * f.dip_w / f.w.max(1) as f32;
    Some(Rect::new(c.cx() - f.dip_w / 2.0, c.cy() - h / 2.0, f.dip_w, h))
}

/// The hidden icons' grid: a cell, and the line under it for the name of
/// the icon under the mouse.
const TRAY_CELL: f32 = 36.0;
const TRAY_TIP: f32 = 24.0;

/// Columns and rows for `n` icons.
fn tray_grid(n: usize) -> (usize, usize) {
    let cols = n.clamp(1, 8);
    (cols, n.max(1).div_ceil(cols))
}

thread_local! {
    /// Tray icons as bitmaps: key -> (icon version, surface generation, bitmap).
    static TRAY_BMPS: std::cell::RefCell<std::collections::HashMap<String, (u32, u32, usize, ID2D1Bitmap)>> = Default::default();
}

/// The hidden icons: every app's tray icon; the name of the one under the
/// mouse below them.
fn draw_tray(bar: &mut Bar, ui: &mut Ui, r: Rect) {
    let list = crate::tray::icons();
    if list.is_empty() {
        ui.text("No icons", r, 12.0, col::DIM, Weight::Regular, Align::Center);
        return;
    }
    let (cols, _) = tray_grid(list.len());
    // The name of the icon under the mouse on top (the icons sit right
    // above the bar, like Windows').
    let mut c = r.inset(10.0);
    let tip_r = c.cut_top(TRAY_TIP - 4.0, 4.0);
    let gw = cols as f32 * TRAY_CELL;
    let c = Rect::new(c.x + ((c.w - gw) / 2.0).max(0.0), c.y, gw, c.h);
    let mut tip = String::new();
    let s = ui.scale;
    for (k, icon) in list.iter().enumerate() {
        let cell = Rect::new(c.x + (k % cols) as f32 * TRAY_CELL, c.y + (k / cols) as f32 * TRAY_CELL, TRAY_CELL, TRAY_CELL).centered(TRAY_CELL - 4.0, TRAY_CELL - 4.0);
        let id = TRAY_ICON + k as u32;
        if ui.hovered(cell) {
            ui.fill_rr(cell, 9.0, if bar.down == Some(id) { col::BTN_HOVER.with_a(0.24) } else { col::BTN_HOVER });
            tip = icon.tip.lines().next().unwrap_or("").to_string();
        }
        if let Some((size, px)) = &icon.icon {
            let key = icon.key();
            let surf_gen = ui.g.generation;
            let gid = ui.g.id as usize;
            let cached = TRAY_BMPS.with(|m| m.borrow().get(&key).filter(|e| e.0 == icon.icon_ver && e.1 == surf_gen && e.2 == gid).map(|e| e.3.clone()));
            let bmp = cached.or_else(|| {
                let b = ui.g.make_bitmap(*size, *size, px)?;
                TRAY_BMPS.with(|m| m.borrow_mut().insert(key.clone(), (icon.icon_ver, surf_gen, gid, b.clone())));
                Some(b)
            });
            if let Some(b) = bmp {
                let z = 18.0;
                let at = Rect::new(((cell.cx() - z / 2.0) * s).round() / s, ((cell.cy() - z / 2.0) * s).round() / s, z, z);
                ui.image_rr(&b, at, 0.0);
            }
        }
        ui.hit(cell, id);
    }
    let (tip, c) = if tip.is_empty() { ("Hidden icons".to_string(), col::FAINT) } else { (tip, col::TEXT) };
    ui.text(&tip, tip_r, 11.5, c, Weight::Semi, Align::Center);
}

/// A click on a hidden icon: to its app, as Windows sends it (the app shows
/// its window, or its menu).
pub fn tray_click(bar: &mut Bar, id: u32, right: bool) {
    let list = crate::tray::icons();
    let Some(icon) = list.get((id - TRAY_ICON) as usize) else { return };
    // Where it is on the screen (apps put their menu there).
    let at = bar.hits.iter().find(|h| h.id == id).map(|h| bar.to_screen(h.rect.cx(), h.rect.y)).unwrap_or((0, 0));
    // Its menu (or window) comes: the bar waits for it, and with a menu so
    // do the icons.
    bar.hold_for_tray(icon.pid);
    if !right {
        bar.set_pop(None);
    }
    crate::tray::click(icon, right, at);
}

/// A click on a widget without a full view: what Windows has for it.
fn widget_click(id: &str) {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    match id {
        "wifi" | "volume" | "battery" | "bluetooth" | "mic" => shell::open_quick_settings(),
        "clock" | "date" | "datetime" | "dayprog" | "yearprog" => {
            crate::unread::mark_seen();
            shell::open_notifications();
        }
        "media" => shell::press(&[VK_MEDIA_PLAY_PAUSE]),
        "lang" => shell::press(&[VK_LWIN, VK_SPACE]),
        "cpu" | "ram" | "net" | "disk" | "uptime" => shell::press(&[VK_CONTROL, VK_SHIFT, VK_ESCAPE]),
        "recycle" => apps::launch("shell:RecycleBinFolder"),
        "mode" => shell::toggle_dark_mode(),
        _ => {}
    }
}

/// Middle click: a new window of the app (on Now playing: play / pause).
pub fn middle(bar: &mut Bar, id: u32) {
    if (WIDGET..WIDGET + 500).contains(&id) {
        if nookly_core::barcfg::placed(&bar.settings).get((id - WIDGET) as usize).is_some_and(|p| p.id == "media") {
            shell::press(&[windows::Win32::UI::Input::KeyboardAndMouse::VK_MEDIA_PLAY_PAUSE]);
        }
        return;
    }
    let app = match id {
        i if (APP..APP + 900).contains(&i) => bar.apps.list.get((i - APP) as usize),
        i if (POP_APP..POP_APP + 1000).contains(&i) => bar.apps.list.get((i - POP_APP) as usize),
        i if (POP_MEMBER..POP_MEMBER + 100).contains(&i) => match bar.popup {
            Some(Pop::Group(g, _)) => bar.apps.groups.get(g).and_then(|grp| grp.members.get((i - POP_MEMBER) as usize)),
            _ => None,
        },
        _ => None,
    };
    if let Some(a) = app {
        apps::launch(&a.launch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_move_along_and_between_rows() {
        let r = |x: f32, y: f32| Rect::new(x, y, 10.0, 10.0);
        // Two rows: 1 2 3 / 4 5.
        let v = [(2, r(20.0, 0.0)), (1, r(0.0, 0.0)), (3, r(40.0, 1.0)), (4, r(0.0, 40.0)), (5, r(22.0, 40.0))];
        assert_eq!(key_step(&v, None, 1, 0), Some(1));
        assert_eq!(key_step(&v, Some(1), 1, 0), Some(2));
        assert_eq!(key_step(&v, Some(3), 1, 0), Some(4));
        assert_eq!(key_step(&v, Some(1), -1, 0), Some(5));
        assert_eq!(key_step(&v, Some(2), 0, 1), Some(5));
        assert_eq!(key_step(&v, Some(5), 0, -1), Some(2));
        assert_eq!(key_step(&v, Some(1), 0, -1), Some(1));
        // Gone meanwhile: from the start.
        assert_eq!(key_step(&v, Some(99), 1, 0), Some(1));
        assert_eq!(key_step(&[], Some(1), 1, 0), None);
    }
}
