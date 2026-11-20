//! Each taskbar widget's own full view: what its bubble shows when it has
//! no notch panel about the same thing (Date opens the Calendar, Weather
//! the Weather panel...). Drawn by the notch like the faces (see
//! faces.rs); clicks come back to `action`.
//!
//! One rule: a bubble shows more of the widget's own subject, never
//! someone else's. Buttons do the next thing you'd want (open the right
//! Windows settings page, switch, empty...).

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use crate::faces::*;
use crate::gfx::*;
use crate::modules::Widget;
use crate::store::Settings;
use crate::time::{DAYS, MONTHS, MONTH_NAMES, Now};
use crate::ui::Ui;

const WEEKDAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/// Button ids (each view uses its own few).
const B1: u32 = 1;
const B2: u32 = 2;
const CITY: u32 = 100;
const FX_BASE: u32 = 200;
const FX_QUOTE: u32 = 300;
const MODE_LIGHT: u32 = 10;
const MODE_DARK: u32 = 11;

/// A view's size (DIPs), or None: not drawn here.
pub fn size(id: &str) -> Option<(f32, f32)> {
    Some(match id {
        "clock" => (420.0, 190.0),
        "datetime" => (440.0, 200.0),
        "dayprog" => (420.0, 152.0),
        "yearprog" => (420.0, 156.0),
        "worldclock" => (430.0, 250.0),
        "sun" => (420.0, 190.0),
        "moon" => (380.0, 170.0),
        "quote" => (430.0, 150.0),
        "currency" => (430.0, 226.0),
        "disk" => (400.0, 190.0),
        "uptime" => (360.0, 140.0),
        "battery" => (400.0, 160.0),
        "wifi" => (400.0, 150.0),
        "bluetooth" => (380.0, 140.0),
        "mic" => (360.0, 140.0),
        "lang" => (380.0, 140.0),
        "recycle" => (380.0, 140.0),
        "mode" => (380.0, 150.0),
        _ => return None,
    })
}

/// What a click did, for the notch to finish.
pub enum Act {
    None,
    /// A setting changed (save it, draw the faces again).
    Saved,
    /// Open this notch panel (by title).
    Panel(&'static str),
}

/// The title row: a symbol in the widget's colour, the name, and on the
/// right a short note.
fn header(ui: &mut Ui, r: &mut Rect, icon: &str, title: &str, tint: Color, note: &str) {
    let mut row = r.cut_top(24.0, 10.0);
    let tile = row.cut_left(24.0, 9.0);
    ui.fill_rr_grad(tile, 7.0, tint, tint.lerp(hex(0x000000), 0.35), true);
    ui.icon(icon, tile, 12.0, hex(0xFFFFFF));
    if !note.is_empty() {
        let w = ui.measure(note, 11.0, Weight::Regular) + 4.0;
        ui.text(note, row.cut_right(w, 6.0), 11.0, col::DIM, Weight::Regular, Align::Right);
    }
    ui.text(title, row, 14.0, col::TEXT, Weight::Semi, Align::Left);
}

/// A row of buttons along the bottom: (label, id).
fn buttons(ui: &mut Ui, r: &mut Rect, list: &[(&str, u32)]) {
    let mut row = r.cut_bottom(30.0, 8.0);
    for &(label, id) in list.iter().rev() {
        let w = ui.measure(label, 12.0, Weight::Semi) + 28.0;
        ui.button(row.cut_right(w, 8.0), label, id);
    }
}

/// A stat: a small label over a value.
fn stat(ui: &mut Ui, r: Rect, label: &str, value: &str, c: Color) {
    ui.text(label, Rect::new(r.x, r.y, r.w, 14.0), 10.5, col::DIM, Weight::Regular, Align::Left);
    ui.text(value, Rect::new(r.x, r.y + 14.0, r.w, 18.0), 13.5, c, Weight::Semi, Align::Left);
}

fn value<'a>(values: &'a [Widget], id: &str) -> Option<&'a Widget> {
    values.iter().find(|w| w.id == id)
}

pub fn draw(ui: &mut Ui, r: Rect, id: &str, info: &Info, values: &[Widget], st: &Settings) {
    let r = r.inset(14.0);
    let n = Now::get();
    let _ = st;
    match id {
        "clock" => clock(ui, r, &n, info),
        "datetime" => datetime(ui, r, &n),
        "dayprog" => dayprog(ui, r, &n, info),
        "yearprog" => yearprog(ui, r, &n),
        "worldclock" => world(ui, r, info),
        "sun" => sun(ui, r, &n, info),
        "moon" => moon(ui, r, &n),
        "quote" => quote(ui, r, info),
        "currency" => currency(ui, r, info),
        "disk" => disk(ui, r, info),
        "uptime" => uptime(ui, r, &n),
        "battery" => battery(ui, r, value(values, "battery")),
        "wifi" => wifi(ui, r, value(values, "wifi")),
        "bluetooth" => simple(ui, r, value(values, "bluetooth"), "\u{E702}", "Bluetooth", hex(0x0A84FF), &[("Bluetooth & devices", B1)]),
        "mic" => simple(ui, r, value(values, "mic"), "\u{E720}", "Microphone", col::GREEN, &[("Microphone privacy", B2), ("Sound settings", B1)]),
        "lang" => lang(ui, r, info),
        "recycle" => recycle(ui, r, info),
        "mode" => mode(ui, r, info),
        _ => {}
    }
}

/// A click in a view: what it does.
pub fn action(id: &str, hit: u32, st: &mut Settings) -> Act {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    let open = |what: &str| open_uri(what);
    match (id, hit) {
        ("clock", B1) => return Act::Panel("Timer"),
        ("datetime", B1) | ("dayprog", B1) | ("yearprog", B1) => return Act::Panel("Calendar"),
        ("worldclock", h) if (CITY..CITY + 32).contains(&h) => {
            if let Some(c) = crate::zones::CITIES.get((h - CITY) as usize) {
                crate::zones::toggle(st, c.0);
                return Act::Saved;
            }
        }
        ("sun", B1) | ("moon", B1) => return Act::Panel("Weather"),
        ("quote", B1) => {
            let k: u32 = st.extra.get("quote_skip").and_then(|v| v.parse().ok()).unwrap_or(0);
            st.extra.insert("quote_skip".into(), (k + 1).to_string());
            return Act::Saved;
        }
        ("currency", h) if (FX_BASE..FX_BASE + 8).contains(&h) => {
            let c = crate::modules::tools::CURRENCIES[(h - FX_BASE) as usize];
            if st.extra.get("fx_quote").map(String::as_str) == Some(c) {
                let b = st.extra.get("fx_base").cloned().unwrap_or_else(|| "USD".into());
                st.extra.insert("fx_quote".into(), b);
            }
            st.extra.insert("fx_base".into(), c.into());
            return Act::Saved;
        }
        ("currency", h) if (FX_QUOTE..FX_QUOTE + 8).contains(&h) => {
            st.extra.insert("fx_quote".into(), crate::modules::tools::CURRENCIES[(h - FX_QUOTE) as usize].into());
            return Act::Saved;
        }
        ("currency", B1) => return Act::Panel("Tools"),
        ("disk", B1) => open("ms-settings:storagesense"),
        ("disk", B2) => open("shell:MyComputerFolder"),
        ("uptime", B1) => crate::shell::press(&[VK_CONTROL, VK_SHIFT, VK_ESCAPE]),
        ("battery", B1) => open("ms-settings:powersleep"),
        ("battery", B2) => open("ms-settings:batterysaver"),
        ("wifi", B1) => open("ms-settings:network-wifi"),
        ("wifi", B2) => crate::shell::open_quick_settings(),
        ("bluetooth", B1) => open("ms-settings:bluetooth"),
        ("mic", B1) => open("ms-settings:sound"),
        ("mic", B2) => open("ms-settings:privacy-microphone"),
        ("lang", B1) => crate::shell::press(&[VK_LWIN, VK_SPACE]),
        ("lang", B2) => open("ms-settings:regionlanguage"),
        ("recycle", B1) => open("shell:RecycleBinFolder"),
        ("recycle", B2) => unsafe {
            // Windows asks first, as always.
            let _ = windows::Win32::UI::Shell::SHEmptyRecycleBinW(None, windows::core::PCWSTR::null(), 0);
        },
        ("mode", MODE_LIGHT) => {
            if dark_now() {
                crate::shell::toggle_dark_mode();
            }
        }
        ("mode", MODE_DARK) => {
            if !dark_now() {
                crate::shell::toggle_dark_mode();
            }
        }
        _ => {}
    }
    Act::None
}

fn dark_now() -> bool {
    let mut i = Info::default();
    i.refresh(crate::time::unix_now());
    i.dark
}

/// Open a settings page, a shell folder or a link.
fn open_uri(what: &str) {
    let w: Vec<u16> = what.encode_utf16().chain(Some(0)).collect();
    unsafe {
        windows::Win32::UI::Shell::ShellExecuteW(None, windows::core::w!("open"), windows::core::PCWSTR(w.as_ptr()), windows::core::PCWSTR::null(), windows::core::PCWSTR::null(), windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL);
    }
}

// ------------------------------------------------------------ time

/// A big clock face: numbers at the quarters, ticks, hands, the second
/// hand in the clock's colour.
fn big_dial(ui: &mut Ui, cx: f32, cy: f32, rad: f32, n: &Now) {
    let tint = hex(0xFF9F0A);
    ui.circle(cx, cy, rad + 3.0, tint.with_a(0.10));
    ui.circle(cx, cy, rad, col::TEXT.with_a(0.06));
    ui.stroke_circle(cx, cy, rad - 0.5, tint.with_a(0.5), 1.2);
    for k in 0..60 {
        let a = k as f32 / 60.0 * TAU;
        let (s, c) = a.sin_cos();
        let (inner, w, cl) = if k % 15 == 0 { (rad - 10.0, 2.0, col::TEXT) } else if k % 5 == 0 { (rad - 7.0, 1.5, col::TEXT.with_a(0.8)) } else { (rad - 4.0, 1.0, col::DIM.with_a(0.6)) };
        ui.line(cx + s * inner, cy - c * inner, cx + s * (rad - 2.0), cy - c * (rad - 2.0), cl, w);
    }
    for (k, num) in [(0, "12"), (3, "3"), (6, "6"), (9, "9")] {
        let a = k as f32 / 12.0 * TAU;
        let (s, c) = a.sin_cos();
        let (x, y) = (cx + s * (rad - 20.0), cy - c * (rad - 20.0));
        ui.text(num, Rect::new(x - 12.0, y - 9.0, 24.0, 18.0), 12.0, col::TEXT, Weight::Semi, Align::Center);
    }
    let m = n.min as f32 + n.sec as f32 / 60.0;
    let hand = |ui: &mut Ui, f: f32, len: f32, w: f32, c: Color| {
        let a = f * TAU;
        ui.line(cx - a.sin() * 6.0, cy + a.cos() * 6.0, cx + a.sin() * len, cy - a.cos() * len, c, w);
    };
    hand(ui, ((n.hour % 12) as f32 + m / 60.0) / 12.0, rad * 0.5, 4.0, col::TEXT);
    hand(ui, m / 60.0, rad * 0.75, 2.6, col::TEXT);
    hand(ui, n.sec as f32 / 60.0, rad * 0.85, 1.2, tint);
    ui.circle(cx, cy, 3.5, tint);
}

fn clock(ui: &mut Ui, r: Rect, n: &Now, info: &Info) {
    let mut r = r;
    let dial = r.cut_left(r.h, 18.0);
    big_dial(ui, dial.cx(), dial.cy(), dial.h / 2.0 - 2.0, n);
    let (t, ampm) = time_parts(n);
    let tw = ui.measure_digits(&t, 38.0);
    ui.digits(&t, Rect::new(r.x, r.y, tw + 4.0, 46.0), 38.0, col::TEXT, Align::Left);
    ui.digits(&format!(":{:02}", n.sec), Rect::new(r.x + tw + 3.0, r.y + 10.0, 50.0, 30.0), 20.0, hex(0xFF9F0A), Align::Left);
    if !ampm.is_empty() {
        ui.text(&ampm, Rect::new(r.x + tw + 4.0, r.y + 2.0, 40.0, 14.0), 11.0, col::DIM, Weight::Bold, Align::Left);
    }
    let mut lines = Rect::new(r.x, r.y + 50.0, r.w, r.h - 50.0);
    let wd = WEEKDAYS[n.dow as usize % 7];
    ui.text(&format!("{wd}, {} {}", n.day, MONTH_NAMES[(n.month as usize).saturating_sub(1) % 12]), lines.cut_top(18.0, 2.0), 13.0, col::TEXT, Weight::Semi, Align::Left);
    ui.text(&format!("Week {}  \u{00B7}  day {} of {}  \u{00B7}  {}", week(n), day_of_year(n), year_days(n.year), utc_offset()), lines.cut_top(16.0, 10.0), 11.0, col::DIM, Weight::Regular, Align::Left);
    if let Some((rise, set)) = sun_today(info, n) {
        let row = lines.cut_top(32.0, 6.0);
        let cols = row.columns(2, 8.0);
        stat(ui, cols[0], "\u{2191} Sunrise", &hm(rise), C_SUN);
        stat(ui, cols[1], "\u{2193} Sunset", &hm(set), C_SUN);
    }
    let mut b = lines;
    buttons(ui, &mut b, &[("Timer & alarms", B1)]);
}

/// A month of days: today ringed, weekends in red.
fn month_grid(ui: &mut Ui, r: Rect, n: &Now) {
    let mut r = r;
    ui.text(&format!("{} {}", MONTH_NAMES[(n.month as usize).saturating_sub(1) % 12], n.year), r.cut_top(18.0, 4.0), 12.5, col::TEXT, Weight::Semi, Align::Left);
    let cols = r.columns(7, 0.0);
    let ch = (r.h / 7.0).floor();
    for (k, l) in ["M", "T", "W", "T", "F", "S", "S"].iter().enumerate() {
        ui.text(l, Rect::new(cols[k].x, r.y, cols[k].w, ch), 10.0, if k >= 5 { hex(0xFF6B5E) } else { col::DIM }, Weight::Semi, Align::Center);
    }
    // The weekday of the 1st (Monday first).
    let first_dow = ((n.dow as i32 - (n.day as i32 - 1)).rem_euclid(7) + 6) % 7;
    let days = crate::time::days_in_month(n.year as i32, n.month as u32) as i32;
    for d in 1..=days {
        let slot = first_dow + d - 1;
        let (cx, cy) = ((slot % 7) as usize, slot / 7);
        let cell = Rect::new(cols[cx].x, r.y + ch * (cy as f32 + 1.0), cols[cx].w, ch);
        let today = d == n.day as i32;
        if today {
            ui.circle(cell.cx(), cell.cy(), ch / 2.0 - 1.0, hex(0xFF453A));
        }
        ui.text(&d.to_string(), cell, 10.5, if today { hex(0xFFFFFF) } else if cx >= 5 { hex(0xFF6B5E).with_a(0.9) } else { col::TEXT }, if today { Weight::Bold } else { Weight::Regular }, Align::Center);
    }
}

fn datetime(ui: &mut Ui, r: Rect, n: &Now) {
    let mut r = r;
    let cal = r.cut_right(190.0, 18.0);
    month_grid(ui, cal, n);
    let (t, ampm) = time_parts(n);
    let tw = ui.measure_digits(&t, 40.0);
    ui.digits(&t, Rect::new(r.x, r.y, tw + 4.0, 48.0), 40.0, col::TEXT, Align::Left);
    ui.digits(&format!(":{:02}", n.sec), Rect::new(r.x + tw + 3.0, r.y + 12.0, 50.0, 30.0), 20.0, hex(0xFF6482), Align::Left);
    if !ampm.is_empty() {
        ui.text(&ampm, Rect::new(r.x + tw + 4.0, r.y + 2.0, 40.0, 14.0), 11.0, col::DIM, Weight::Bold, Align::Left);
    }
    let mut rest = Rect::new(r.x, r.y + 52.0, r.w, r.h - 52.0);
    ui.text(WEEKDAYS[n.dow as usize % 7], rest.cut_top(20.0, 0.0), 15.0, col::TEXT, Weight::Semi, Align::Left);
    ui.text(&format!("{} {} {}  \u{00B7}  week {}", n.day, MONTH_NAMES[(n.month as usize).saturating_sub(1) % 12], n.year, week(n)), rest.cut_top(16.0, 14.0), 11.5, col::DIM, Weight::Regular, Align::Left);
    let f = day_fraction(n);
    ui.text(&format!("{:.0}% of today gone", f * 100.0), rest.cut_top(15.0, 6.0), 10.5, col::DIM, Weight::Regular, Align::Left);
    sky_band(ui, rest.cut_top(6.0, 0.0), f);
}

fn dayprog(ui: &mut Ui, r: Rect, n: &Now, info: &Info) {
    let mut r = r;
    let f = day_fraction(n);
    let left = ((1.0 - f) * 1440.0) as u32;
    let mut top = r.cut_top(30.0, 14.0);
    let head = format!("{:.0}%", f * 100.0);
    let hw = ui.measure(&head, 26.0, Weight::Bold);
    ui.text(&head, top.cut_left(hw + 4.0, 8.0), 26.0, col::TEXT, Weight::Bold, Align::Left);
    ui.text(&format!("of today  \u{00B7}  {} h {} min until midnight", left / 60, left % 60), top, 12.0, col::DIM, Weight::Regular, Align::Left);
    // 24 hours, each in the sky's colour at that hour; sunrise and sunset marked.
    let bar = r.cut_top(22.0, 4.0);
    segments(ui, bar, 24, f, 2.0, |i| sky(i as f32 + 0.5));
    let labels = r.cut_top(14.0, 12.0);
    for h in [0, 6, 12, 18, 24] {
        let x = labels.x + labels.w * h as f32 / 24.0;
        let t = if h == 24 { "24".to_string() } else { format!("{h}") };
        ui.text(&t, Rect::new(x - 12.0, labels.y, 24.0, 14.0), 9.5, col::DIM, Weight::Regular, Align::Center);
    }
    let sunpos = sun_today(info, n);
    if let Some((rise, set)) = sunpos {
        for (m, up) in [(rise, true), (set, false)] {
            let x = bar.x + bar.w * (m as f32 / 1440.0);
            ui.fill_rr(Rect::new(x - 1.0, bar.y - 4.0, 2.0, bar.h + 8.0), 1.0, C_SUN);
            ui.text(if up { "\u{2191}" } else { "\u{2193}" }, Rect::new(x - 8.0, bar.y - 18.0, 16.0, 14.0), 10.0, C_SUN, Weight::Bold, Align::Center);
        }
    }
    let cols = r.cut_top(32.0, 0.0).columns(3, 10.0);
    let mins = n.hour as f32 * 60.0 + n.min as f32;
    let work = ((mins - 540.0) / 480.0).clamp(0.0, 1.0);
    stat(ui, cols[0], "Work day (9 to 5)", &format!("{:.0}% done", work * 100.0), col::TEXT);
    match sunpos {
        Some((rise, set)) => {
            let dl = (set as f32 - mins).max(0.0);
            let txt = if (mins as f64) < rise { format!("Sunrise {}", hm(rise)) } else if dl > 0.0 { format!("{} h {} min", dl as u32 / 60, dl as u32 % 60) } else { "After sunset".into() };
            stat(ui, cols[1], "Daylight left", &txt, C_SUN);
        }
        None => stat(ui, cols[1], "Daylight left", "Set your place", col::DIM),
    }
    stat(ui, cols[2], "Hours gone", &format!("{} of 24", n.hour), col::TEXT);
}

fn yearprog(ui: &mut Ui, r: Rect, n: &Now) {
    let mut r = r;
    let f = year_fraction(n);
    let left = year_days(n.year) - day_of_year(n);
    let mut top = r.cut_top(30.0, 12.0);
    let head = format!("{:.1}%", f * 100.0);
    let hw = ui.measure(&head, 26.0, Weight::Bold);
    ui.text(&head, top.cut_left(hw + 4.0, 8.0), 26.0, col::TEXT, Weight::Bold, Align::Left);
    ui.text(&format!("of {}  \u{00B7}  {left} days left", n.year), top, 12.0, col::DIM, Weight::Regular, Align::Left);
    let months = r.cut_top(40.0, 14.0);
    let cells = months.columns(12, 4.0);
    let mf = n.month as f32 - 1.0 + n.day as f32 / crate::time::days_in_month(n.year as i32, n.month as u32) as f32;
    for (k, cell) in cells.into_iter().enumerate() {
        let c = hue(0.33 - k as f32 / 12.0 * 0.9);
        let fill = (mf - k as f32).clamp(0.0, 1.0);
        let block = Rect::new(cell.x, cell.y, cell.w, 24.0);
        ui.fill_rr(block, 5.0, c.with_a(0.16));
        if fill > 0.0 {
            let h = block.h * fill;
            ui.fill_rr(Rect::new(block.x, block.b() - h, block.w, h), 5.0, c);
        }
        let now = k + 1 == n.month as usize;
        ui.text(&MONTHS[k][..1], Rect::new(cell.x, cell.y + 26.0, cell.w, 14.0), 9.5, if now { col::TEXT } else { col::DIM }, if now { Weight::Bold } else { Weight::Regular }, Align::Center);
    }
    let cols = r.cut_top(32.0, 0.0).columns(3, 10.0);
    let q = (n.month - 1) / 3;
    let qf = (mf - q as f32 * 3.0) / 3.0;
    stat(ui, cols[0], "Week", &format!("{} of {}", week(n), if week(&Now { month: 12, day: 28, ..*n }) == 53 { 53 } else { 52 }), col::TEXT);
    stat(ui, cols[1], &format!("Quarter {}", q + 1), &format!("{:.0}% done", qf * 100.0), col::TEXT);
    stat(ui, cols[2], "Day", &format!("{} of {}", day_of_year(n), year_days(n.year)), col::TEXT);
}

fn world(ui: &mut Ui, r: Rect, info: &Info) {
    let mut r = r;
    header(ui, &mut r, "\u{E909}", "World clock", hex(0x7D7AFF), "Pick up to four cities");
    let list = info.cities.clone();
    let rows_r = r.cut_top(4.0 * 26.0, 10.0);
    if list.is_empty() {
        ui.text("No cities yet: pick some below.", rows_r, 12.0, col::DIM, Weight::Regular, Align::Center);
    }
    for (k, city) in list.iter().enumerate() {
        let Some((h, m, dd, ahead)) = crate::zones::time_in(city) else { continue };
        let row = Rect::new(rows_r.x, rows_r.y + k as f32 * 26.0, rows_r.w, 24.0);
        if k % 2 == 0 {
            ui.fill_rr(row, 7.0, col::TEXT.with_a(0.04));
        }
        let mut row = row.inset_xy(8.0, 0.0);
        let hour = h as f32 + m as f32 / 60.0;
        let dot = row.cut_left(10.0, 8.0);
        ui.circle(dot.cx(), dot.cy(), 5.0, sky(hour).with_a(0.25));
        ui.circle(dot.cx(), dot.cy(), 3.5, sky(hour));
        let t = crate::time::clock_full(h as u32, m as u32);
        let tw = ui.measure(&t, 15.0, Weight::Bold) + 4.0;
        ui.text(&t, row.cut_right(tw, 10.0), 15.0, col::TEXT, Weight::Bold, Align::Right);
        let when = match dd {
            1 => "Tomorrow",
            -1 => "Yesterday",
            _ => "Today",
        };
        let note = format!("{when}, {}", crate::zones::ahead_text(ahead));
        let nw = ui.measure(&note, 10.5, Weight::Regular) + 4.0;
        ui.text(&note, row.cut_right(nw, 8.0), 10.5, col::DIM, Weight::Regular, Align::Right);
        ui.text(city, row, 13.0, col::TEXT, Weight::Semi, Align::Left);
    }
    // Every city there is, to pick from.
    let cities = crate::zones::CITIES;
    let rows = r.rows(4, 4.0);
    for (k, (name, _)) in cities.iter().enumerate() {
        let row = rows[k / 4];
        let cell = row.columns(4, 4.0)[k % 4];
        ui.chip(cell, name, CITY + k as u32, list.contains(name), hex(0x7D7AFF));
    }
}

fn sun(ui: &mut Ui, r: Rect, n: &Now, info: &Info) {
    let mut r = r;
    let Some((rise, set)) = sun_today(info, n) else {
        ui.text("Set your place in Settings > Location to see the sun's times.", r, 12.0, col::DIM, Weight::Regular, Align::Center);
        return;
    };
    let now = n.hour as f64 * 60.0 + n.min as f64;
    let art = r.cut_left(200.0, 16.0);
    // The day's arc, big: night below the horizon, the sun on its path.
    let rad = (art.w / 2.0 - 8.0).min(art.h - 40.0);
    let (cx, base) = (art.cx(), art.y + rad + 10.0);
    ui.fill_rr(Rect::new(art.x, base, art.w, 1.5), 0.75, col::TEXT.with_a(0.35));
    ui.arc(cx, base, rad, -FRAC_PI_2, PI, C_SUN.with_a(0.25), 2.0);
    let f = ((now - rise) / (set - rise)) as f32;
    if (0.0..=1.0).contains(&f) {
        ui.arc(cx, base, rad, -FRAC_PI_2, f * PI, C_SUN, 3.0);
        let a = -FRAC_PI_2 + f * PI;
        let (sx, sy) = (cx + rad * a.sin(), base - rad * a.cos());
        ui.circle(sx, sy, 13.0, C_SUN.with_a(0.18));
        ui.circle(sx, sy, 8.0, C_SUN.with_a(0.4));
        ui.circle(sx, sy, 5.5, hex(0xFFE7A0));
    } else {
        ui.icon("\u{E708}", Rect::new(cx - 12.0, base - rad * 0.6 - 12.0, 24.0, 24.0), 18.0, hex(0xC7B8FF));
    }
    ui.text(&hm(rise), Rect::new(art.x - 10.0, base + 6.0, 70.0, 16.0), 11.0, col::DIM, Weight::Semi, Align::Left);
    ui.text(&hm(set), Rect::new(art.r() - 60.0, base + 6.0, 70.0, 16.0), 11.0, col::DIM, Weight::Semi, Align::Right);
    let length = set - rise;
    ui.text("Daylight", r.cut_top(16.0, 0.0), 11.0, col::DIM, Weight::Regular, Align::Left);
    ui.text(&format!("{} h {} min", length as u32 / 60, length as u32 % 60), r.cut_top(24.0, 10.0), 18.0, col::TEXT, Weight::Bold, Align::Left);
    let rows = r.cut_top(36.0, 6.0).columns(2, 8.0);
    stat(ui, rows[0], "\u{2191} Sunrise", &hm(rise), C_SUN);
    stat(ui, rows[1], "\u{2193} Sunset", &hm(set), C_SUN);
    let rows = r.cut_top(36.0, 0.0).columns(2, 8.0);
    stat(ui, rows[0], "Solar noon", &hm((rise + set) / 2.0), col::TEXT);
    stat(ui, rows[1], "Golden hour", &hm(set - 60.0), hex(0xFFB340));
}

fn moon(ui: &mut Ui, r: Rect, n: &Now) {
    let mut r = r;
    let t = crate::time::unix_now();
    let age = crate::astro::moon_age(t);
    let art = r.cut_left(r.h, 18.0);
    crate::faces::moon_disc(ui, art.cx(), art.cy(), art.h / 2.0 - 6.0, age);
    ui.text(crate::astro::moon_name(age), r.cut_top(26.0, 2.0), 19.0, col::TEXT, Weight::Bold, Align::Left);
    ui.text(&format!("{:.0}% lit  \u{00B7}  {:.1} days old", crate::astro::moon_lit(age) * 100.0, age), r.cut_top(16.0, 14.0), 11.5, col::DIM, Weight::Regular, Align::Left);
    let (full, new) = crate::astro::moon_next(age);
    let day = |d: f64| {
        let k = d.round() as i32;
        let dow = (n.dow as i32 + k).rem_euclid(7) as usize;
        let date = date_add(n, k);
        format!("{} {date}", DAYS[dow])
    };
    let cols = r.cut_top(34.0, 6.0).columns(2, 8.0);
    stat(ui, cols[0], "Full moon", &if full < 1.0 { "Tonight".into() } else { day(full) }, hex(0xF2EEFF));
    stat(ui, cols[1], "New moon", &if new < 1.0 { "Tonight".into() } else { day(new) }, col::TEXT);
}

fn quote(ui: &mut Ui, r: Rect, info: &Info) {
    let mut r = r;
    let (q, who) = QUOTES[info.quote % QUOTES.len()];
    let mut b = r;
    buttons(ui, &mut b, &[("Another one", B1)]);
    r.h -= 38.0;
    let mark = r.cut_left(34.0, 6.0);
    ui.text("\u{201C}", Rect::new(mark.x, mark.y - 10.0, mark.w, 60.0), 54.0, hex(0xFF8FA3), Weight::Bold, Align::Left);
    ui.text_wrap(q, r.cut_top(48.0, 6.0), 17.0, col::TEXT, Weight::Semi, Align::Left);
    ui.text(&format!("\u{2014} {who}"), r.cut_top(18.0, 0.0), 12.0, hex(0xFF8FA3), Weight::Semi, Align::Left);
}

fn currency(ui: &mut Ui, r: Rect, info: &Info) {
    let mut r = r;
    let cur = crate::modules::tools::CURRENCIES;
    let Some((day, rates, prev)) = crate::modules::tools::rates_for_widget() else {
        header(ui, &mut r, "\u{E8C7}", "Currency", C_MONEY, "");
        ui.text("Getting today's rates from open.er-api.com…", r, 12.0, col::DIM, Weight::Regular, Align::Center);
        return;
    };
    let (b, q) = info.fx;
    header(ui, &mut r, "\u{E8C7}", &format!("1 {} =", cur[b]), C_MONEY, &format!("Rates of {day}"));
    // The base: tap one.
    let chips = r.cut_top(24.0, 10.0).columns(8, 4.0);
    for (k, cell) in chips.into_iter().enumerate() {
        ui.chip(cell, cur[k], FX_BASE + k as u32, k == b, C_MONEY);
    }
    // Every other currency at today's rate; tap one for the widget.
    let grid = r.cut_top(4.0 * 28.0, 6.0);
    let others: Vec<usize> = (0..cur.len()).filter(|&k| k != b).collect();
    for (j, &k) in others.iter().enumerate() {
        let cell = Rect::new(grid.x + (j % 2) as f32 * (grid.w / 2.0 + 3.0), grid.y + (j / 2) as f32 * 28.0, grid.w / 2.0 - 3.0, 25.0);
        let sel = k == q;
        let hov = ui.hovered(cell);
        ui.fill_rr(cell, 8.0, if sel { C_MONEY.with_a(0.22) } else if hov { col::BTN_HOVER } else { col::TEXT.with_a(0.05) });
        if sel {
            ui.stroke_rr(cell.inset(0.5), 7.5, C_MONEY, 1.2);
        }
        let mut row = cell.inset_xy(10.0, 0.0);
        ui.text(cur[k], row.cut_left(36.0, 4.0), 12.0, if sel { C_MONEY } else { col::DIM }, Weight::Bold, Align::Left);
        let v = rates[k] / rates[b];
        let ch = prev.map(|p| (v / (p[k] / p[b]) - 1.0) * 100.0);
        if let Some(ch) = ch {
            let (t, c) = if ch.abs() < 0.005 { ("0.00%".to_string(), col::DIM) } else if ch > 0.0 { (format!("\u{25B2}{ch:.2}%"), col::GREEN) } else { (format!("\u{25BC}{:.2}%", ch.abs()), col::RED) };
            let w = ui.measure(&t, 10.0, Weight::Semi) + 4.0;
            ui.text(&t, row.cut_right(w, 6.0), 10.0, c, Weight::Semi, Align::Right);
        }
        ui.text(&money(v), row, 13.0, col::TEXT, Weight::Semi, Align::Left);
        ui.hit(cell, FX_QUOTE + k as u32);
    }
}

// ------------------------------------------------------------ system

fn disk(ui: &mut Ui, r: Rect, info: &Info) {
    let mut r = r;
    let total_free: u64 = info.drives.iter().map(|d| d.2).sum();
    header(ui, &mut r, "\u{EDA2}", "Drives", hex(0xAC8E68), &format!("{} free in all", size_text(total_free)));
    let mut b = r;
    buttons(ui, &mut b, &[("This PC", B2), ("Storage settings", B1)]);
    r.h -= 38.0;
    for (letter, name, free, total) in info.drives.iter().take(3) {
        let mut row = r.cut_top(32.0, 6.0);
        let used = 1.0 - *free as f32 / (*total).max(1) as f32;
        let c = if used > 0.9 { col::RED } else if used > 0.75 { col::YELLOW } else { col::GREEN };
        let tile = row.cut_left(30.0, 10.0).centered(30.0, 30.0);
        ui.fill_rr(tile, 8.0, col::TEXT.with_a(0.08));
        ui.text(letter, tile, 12.0, col::TEXT, Weight::Bold, Align::Center);
        let note = format!("{} free of {}", size_text(*free), size_text(*total));
        let nw = ui.measure(&note, 11.0, Weight::Regular) + 4.0;
        ui.text(name, Rect::new(row.x, row.y, row.w - nw, 15.0), 12.0, col::TEXT, Weight::Semi, Align::Left);
        ui.text(&note, Rect::new(row.r() - nw, row.y, nw, 15.0), 11.0, col::DIM, Weight::Regular, Align::Right);
        grad_bar(ui, Rect::new(row.x, row.y + 20.0, row.w, 7.0), used, col::GREEN, c);
    }
}

fn uptime(ui: &mut Ui, r: Rect, n: &Now) {
    let mut r = r;
    let up = unsafe { windows::Win32::System::SystemInformation::GetTickCount64() } / 1000;
    header(ui, &mut r, "\u{E7E8}", "Uptime", hex(0x40C8E0), "");
    let mut b = r;
    buttons(ui, &mut b, &[("Task Manager", B1)]);
    let (d, h, m) = (up / 86400, up / 3600 % 24, up / 60 % 60);
    let big = if d > 0 { format!("{d} d {h} h {m} min") } else { format!("{h} h {m} min") };
    ui.text(&big, r.cut_top(28.0, 4.0), 22.0, col::TEXT, Weight::Bold, Align::Left);
    let started = crate::time::unix_now() - up as f64;
    let (_, mo, da, mins) = crate::time::local_time(started);
    let _ = n;
    ui.text(&format!("Since {} {}, {}", da, MONTHS[(mo as usize).saturating_sub(1) % 12], crate::time::clock_full(mins / 60, mins % 60)), r.cut_top(16.0, 0.0), 11.5, col::DIM, Weight::Regular, Align::Left);
}

fn battery(ui: &mut Ui, r: Rect, w: Option<&Widget>) {
    let mut r = r;
    let Some(w) = w else {
        ui.text("No battery on this PC", r, 12.0, col::DIM, Weight::Regular, Align::Center);
        return;
    };
    let f = w.ring.unwrap_or(0.0);
    let mut b = r;
    buttons(ui, &mut b, &[("Battery saver", B2), ("Power & battery", B1)]);
    r.h -= 38.0;
    let art = r.cut_left(130.0, 18.0).centered(130.0, 58.0);
    battery_shape(ui, art, f, w.color, w.label == "Charging");
    ui.text(&w.value, Rect::new(art.x, art.y, art.w - 12.0, art.h), 20.0, hex(0xFFFFFF), Weight::Bold, Align::Center);
    ui.text(&w.label, r.cut_top(24.0, 4.0), 18.0, col::TEXT, Weight::Bold, Align::Left);
    let rest = w.detail.split(" · ").nth(1).unwrap_or("").to_string();
    ui.text(if rest.is_empty() { &w.detail } else { &rest }, r.cut_top(16.0, 10.0), 12.0, col::DIM, Weight::Regular, Align::Left);
    segments(ui, r.cut_top(10.0, 0.0), 10, f, 3.0, |_| w.color);
}

fn wifi(ui: &mut Ui, r: Rect, w: Option<&Widget>) {
    let mut r = r;
    let mut b = r;
    buttons(ui, &mut b, &[("Quick settings", B2), ("Wi-Fi settings", B1)]);
    r.h -= 38.0;
    let Some(w) = w else {
        ui.text("Wi-Fi status isn't known yet", r, 12.0, col::DIM, Weight::Regular, Align::Center);
        return;
    };
    let f = w.ring.unwrap_or(0.0);
    let art = r.cut_left(110.0, 18.0).centered(110.0, 56.0);
    staircase(ui, art, 7, f, w.color);
    ui.text(&w.label, r.cut_top(24.0, 4.0), 18.0, col::TEXT, Weight::Bold, Align::Left);
    let q = if f > 0.75 { "Excellent signal" } else if f > 0.5 { "Good signal" } else if f > 0.25 { "Fair signal" } else if f > 0.0 { "Weak signal" } else { "Not connected" };
    ui.text(&format!("{q}  \u{00B7}  {}", w.value), r.cut_top(16.0, 0.0), 12.0, col::DIM, Weight::Regular, Align::Left);
}

fn simple(ui: &mut Ui, r: Rect, w: Option<&Widget>, icon: &str, title: &str, tint: Color, btns: &[(&str, u32)]) {
    let mut r = r;
    let mut b = r;
    buttons(ui, &mut b, btns);
    r.h -= 38.0;
    let art = r.cut_left(60.0, 16.0).centered(60.0, 60.0);
    let c = w.map_or(tint, |w| if w.color == col::DIM { tint } else { w.color });
    ui.fill_rr_grad(art, 18.0, c, c.lerp(hex(0x000000), 0.4), true);
    ui.icon(w.map_or(icon, |w| w.icon), art, 26.0, hex(0xFFFFFF));
    if let Some(f) = w.and_then(|w| w.ring) {
        ring_w(ui, art.cx(), art.cy(), 36.0, f, c, 3.0);
    }
    match w {
        Some(w) => {
            let big = if w.label.is_empty() || w.label == w.name || w.label == "Mic" { w.value.clone() } else { format!("{}  \u{00B7}  {}", w.label, w.value) };
            ui.text(&big, r.cut_top(26.0, 4.0), 18.0, col::TEXT, Weight::Bold, Align::Left);
            ui.text(&w.detail, r.cut_top(16.0, 0.0), 12.0, col::DIM, Weight::Regular, Align::Left);
        }
        None => {
            ui.text(title, r.cut_top(26.0, 4.0), 18.0, col::TEXT, Weight::Bold, Align::Left);
        }
    }
}

fn lang(ui: &mut Ui, r: Rect, info: &Info) {
    let mut r = r;
    let (code, region, name) = &info.lang;
    let mut b = r;
    buttons(ui, &mut b, &[("Language settings", B2), ("Switch language", B1)]);
    r.h -= 38.0;
    let art = r.cut_left(70.0, 16.0).centered(70.0, 52.0);
    ui.fill_rr_grad(art, 12.0, hex(0x0A84FF), hex(0x5E5CE6), false);
    ui.text(code, art, 22.0, hex(0xFFFFFF), Weight::Bold, Align::Center);
    ui.text(name, r.cut_top(26.0, 4.0), 18.0, col::TEXT, Weight::Bold, Align::Left);
    ui.text(&format!("Keyboard: {code}-{region}  \u{00B7}  Win + Space switches"), r.cut_top(16.0, 0.0), 12.0, col::DIM, Weight::Regular, Align::Left);
}

fn recycle(ui: &mut Ui, r: Rect, info: &Info) {
    let mut r = r;
    let (items, bytes) = info.recycle;
    let mut b = r;
    buttons(ui, &mut b, &[("Empty", B2), ("Open", B1)]);
    r.h -= 38.0;
    let art = r.cut_left(60.0, 16.0).centered(60.0, 60.0);
    let c = if items > 0 { col::GREEN } else { hex(0x8E8E93) };
    ui.fill_rr_grad(art, 18.0, c, c.lerp(hex(0x000000), 0.4), true);
    ui.icon("\u{E74D}", art, 26.0, hex(0xFFFFFF));
    let big = if items == 0 { "Empty".to_string() } else if items == 1 { "1 item".to_string() } else { format!("{items} items") };
    ui.text(&big, r.cut_top(26.0, 4.0), 18.0, col::TEXT, Weight::Bold, Align::Left);
    ui.text(&if items > 0 { format!("{} you could get back", size_text(bytes)) } else { "Nothing to empty".into() }, r.cut_top(16.0, 0.0), 12.0, col::DIM, Weight::Regular, Align::Left);
}

fn mode(ui: &mut Ui, r: Rect, info: &Info) {
    let mut r = r;
    header(ui, &mut r, "\u{E793}", "Light or dark", if info.dark { col::PURPLE } else { col::YELLOW }, "For Windows and apps");
    for (k, cell) in r.columns(2, 12.0).into_iter().enumerate() {
        let dark = k == 1;
        let on = info.dark == dark;
        // A small window in that mode.
        let (bg, fg) = if dark { (hex(0x1F1F24), hex(0xF5F5F7)) } else { (hex(0xF4F4F6), hex(0x1F1F24)) };
        let mut c = cell;
        let label = c.cut_bottom(18.0, 4.0);
        ui.fill_rr_raw(c, 10.0, bg);
        ui.fill_rr_raw(Rect::new(c.x + 10.0, c.y + 10.0, c.w * 0.45, 6.0), 3.0, fg.with_a(0.8));
        ui.fill_rr_raw(Rect::new(c.x + 10.0, c.y + 22.0, c.w * 0.7, 5.0), 2.5, fg.with_a(0.35));
        ui.fill_rr_raw(Rect::new(c.x + 10.0, c.y + 32.0, c.w * 0.55, 5.0), 2.5, fg.with_a(0.35));
        if on {
            ui.stroke_rr_raw(c.inset(-2.0), 12.0, col::ACCENT, 2.0);
        } else if ui.hovered(c) {
            ui.stroke_rr_raw(c.inset(-2.0), 12.0, hex(0xFFFFFF).with_a(0.3), 1.0);
        }
        ui.text(if dark { "Dark" } else { "Light" }, label, 12.0, if on { col::TEXT } else { col::DIM }, Weight::Semi, Align::Center);
        ui.hit(cell, if dark { MODE_DARK } else { MODE_LIGHT });
    }
}
