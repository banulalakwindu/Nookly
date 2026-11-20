//! The taskbar's widgets, drawn by the notch (the taskbar shows the
//! pictures: see surfaces.rs and barcfg.rs for the catalog and sizes).
//!
//! Every widget is a card about ONE thing, in its own colour; a bigger size
//! shows more of it, and each size has its own design:
//!   Small  one glance: a ring, a gauge, a symbol or a number
//!   Medium the value, big
//!   Large  the value and what it is, or a picture of it (a dial, a bar)
//!   Wide   all of that and the details: a week, a day's sky, a meter
//!
//! Faces are drawn again only when what they show changes (at most once a
//! second); the taskbar fades each new picture in over the old one, so a
//! ticking second or a filling bar moves smoothly.
//!
//! Any status value a feature offers (`Module::widgets`) gets a card from
//! `draw_value`; a feature with a richer design draws its own
//! (`Module::draw_face`), and the ones that belong to no feature (the
//! clock, the date...) are drawn here. So are the taskbar's right end
//! (keyboard language, Wi-Fi / sound / battery, clock), which look like
//! Windows' own: no card.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use crate::barcfg::Size;
use crate::gfx::*;
use crate::modules::Widget;
use crate::time::{DAYS, MONTHS, MONTH_NAMES, Now};
use crate::ui::Ui;

const WEEKDAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/// Each built-in widget's colour.
const C_CLOCK: Color = hex(0xFF9F0A);
const C_DATE: Color = hex(0xFF453A);
const C_DATETIME: Color = hex(0xFF6482);
const C_DAY: Color = hex(0x64D2FF);
const C_YEAR: Color = hex(0x30D158);
const C_LANG: Color = hex(0x0A84FF);
const C_BIN: Color = hex(0x8E8E93);
const C_WORLD: Color = hex(0x7D7AFF);
pub const C_SUN: Color = hex(0xFFB340);
const C_MOON: Color = hex(0xB9A9FF);
const C_QUOTE: Color = hex(0xFF8FA3);
pub const C_MONEY: Color = hex(0x30D158);

// ------------------------------------------------------------ pieces

/// The card behind every widget: a soft wash of its colour, lit from the
/// left, with a fine rim.
pub fn card(ui: &mut Ui, r: Rect, tint: Color) {
    let c = card_rect(r);
    ui.fill_rr(c, 11.0, col::TEXT.with_a(0.05));
    ui.fill_rr_grad(c, 11.0, tint.with_a(0.30), tint.with_a(0.04), false);
    ui.stroke_rr(c.inset(0.5), 10.5, tint.with_a(0.22), 1.0);
}

/// The card's area inside a face (the face is the bar's full height).
pub fn card_rect(r: Rect) -> Rect {
    Rect::new(r.x + 1.0, r.y + 3.0, r.w - 2.0, r.h - 6.0)
}

/// A progress ring with an icon (or nothing) inside.
pub fn ring(ui: &mut Ui, cx: f32, cy: f32, rad: f32, f: f32, c: Color, icon: &str, icon_size: f32) {
    ring_w(ui, cx, cy, rad, f, c, 2.0);
    if !icon.is_empty() {
        ui.icon(icon, Rect::new(cx - rad, cy - rad, rad * 2.0, rad * 2.0), icon_size, c);
    }
}

/// A progress ring of width `w`, starting at the top.
pub fn ring_w(ui: &mut Ui, cx: f32, cy: f32, rad: f32, f: f32, c: Color, w: f32) {
    ui.stroke_circle(cx, cy, rad, c.with_a(0.20), w);
    if f > 0.004 {
        ui.arc(cx, cy, rad, 0.0, f.clamp(0.0, 1.0) * TAU, c, w);
    }
}

/// A round badge in the widget's colour with a symbol on it.
pub fn disc(ui: &mut Ui, cx: f32, cy: f32, rad: f32, c: Color, icon: &str) {
    ui.circle(cx, cy, rad, c.with_a(0.24));
    ui.icon(icon, Rect::new(cx - rad, cy - rad, rad * 2.0, rad * 2.0), rad * 1.05, c);
}

/// A small line graph of `vals` (0..1) filling `r`, with a fill under it
/// that fades out downwards.
pub fn spark(ui: &mut Ui, r: Rect, vals: &[f32], c: Color) {
    if vals.len() < 2 {
        return;
    }
    let n = vals.len();
    let pt = |i: usize| (r.x + r.w * i as f32 / (n - 1) as f32, r.b() - r.h * vals[i].clamp(0.0, 1.0));
    let top = vals.iter().fold(0.0f32, |a, b| a.max(*b)).clamp(0.0, 1.0);
    for band in 0..3 {
        // Three layers, each a little lower and fainter: a soft gradient.
        let k = band as f32 / 3.0;
        let mut poly: Vec<(f32, f32)> = (0..n).map(|i| {
            let (x, y) = pt(i);
            (x, y + (r.b() - y) * k)
        }).collect();
        poly.push((r.r(), r.b()));
        poly.push((r.x, r.b()));
        ui.polygon(&poly, c.with_a(0.10 * (1.0 - k * 0.5)));
    }
    let _ = top;
    for i in 1..n {
        let (a, b) = (pt(i - 1), pt(i));
        ui.line(a.0, a.1, b.0, b.1, c, 1.5);
    }
    let last = pt(n - 1);
    ui.circle(last.0, last.1, 3.2, c.with_a(0.30));
    ui.circle(last.0, last.1, 1.8, c);
}

/// A thin progress bar.
pub fn bar(ui: &mut Ui, r: Rect, f: f32, c: Color) {
    ui.fill_rr(r, r.h / 2.0, col::TEXT.with_a(0.14));
    if f > 0.0 {
        ui.fill_rr(Rect::new(r.x, r.y, (r.w * f.clamp(0.0, 1.0)).max(r.h), r.h), r.h / 2.0, c);
    }
}

/// A progress bar that blends from `a` to `b` as it fills.
pub fn grad_bar(ui: &mut Ui, r: Rect, f: f32, a: Color, b: Color) {
    ui.fill_rr(r, r.h / 2.0, col::TEXT.with_a(0.13));
    if f > 0.0 {
        let w = (r.w * f.clamp(0.0, 1.0)).max(r.h);
        ui.fill_rr_grad(Rect::new(r.x, r.y, w, r.h), r.h / 2.0, a, a.lerp(b, f.clamp(0.0, 1.0)), false);
    }
}

/// `n` blocks in a row, the first `f` of them lit (the last one partly);
/// each block's colour comes from `color(i)`.
pub fn segments(ui: &mut Ui, r: Rect, n: usize, f: f32, gap: f32, color: impl Fn(usize) -> Color) {
    let w = (r.w - gap * (n - 1) as f32) / n as f32;
    let lit = f.clamp(0.0, 1.0) * n as f32;
    for i in 0..n {
        let b = Rect::new(r.x + i as f32 * (w + gap), r.y, w, r.h);
        let c = color(i);
        let rad = (r.h / 2.0).min(2.5);
        ui.fill_rr(b, rad, c.with_a(0.16));
        let part = (lit - i as f32).clamp(0.0, 1.0);
        if part > 0.0 {
            ui.fill_rr(Rect::new(b.x, b.y, (b.w * part).max(rad * 2.0).min(b.w), b.h), rad, c);
        }
    }
}

/// Bars rising left to right (a volume or signal meter), lit up to `f`,
/// green to yellow to red.
pub fn staircase(ui: &mut Ui, r: Rect, n: usize, f: f32, tint: Color) {
    let gap = 2.0;
    let w = (r.w - gap * (n - 1) as f32) / n as f32;
    for i in 0..n {
        let k = (i + 1) as f32 / n as f32;
        let h = r.h * (0.25 + 0.75 * k);
        let b = Rect::new(r.x + i as f32 * (w + gap), r.b() - h, w, h);
        let lit = k <= f + 0.5 / n as f32 && f > 0.0;
        let c = if k < 0.6 { tint } else if k < 0.85 { col::YELLOW } else { col::RED };
        ui.fill_rr(b, (w / 2.0).min(1.5), if lit { c } else { col::TEXT.with_a(0.12) });
    }
}

/// A little calendar page: the weekday on a red band over the day.
pub fn calendar_page(ui: &mut Ui, r: Rect, now: &Now) {
    ui.fill_rr(r, 7.0, col::TEXT.with_a(0.12));
    let top_h = (r.h * 0.36).round();
    ui.push_clip(Rect::new(r.x, r.y, r.w, top_h));
    ui.fill_rr_grad(r, 7.0, hex(0xFF6B5E), hex(0xE5352B), true);
    ui.pop_clip();
    let dow = DAYS[now.dow as usize % 7].to_uppercase();
    ui.text(&dow, Rect::new(r.x, r.y, r.w, top_h), (top_h * 0.66).min(8.5), hex(0xFFFFFF), Weight::Bold, Align::Center);
    ui.text(&now.day.to_string(), Rect::new(r.x, r.y + top_h - 0.5, r.w, r.h - top_h), ((r.h - top_h) * 0.66).min(15.0), col::TEXT, Weight::Bold, Align::Center);
}

/// Two lines: the big one and the small one under it.
pub fn two_lines(ui: &mut Ui, r: Rect, big: &str, big_size: f32, small: &str, big_c: Color) {
    let cy = r.cy();
    if small.is_empty() {
        ui.text(big, Rect::new(r.x, cy - 10.0, r.w, 20.0), big_size, big_c, Weight::Semi, Align::Left);
    } else {
        ui.text(big, Rect::new(r.x, cy - 15.5, r.w, 18.0), big_size, big_c, Weight::Semi, Align::Left);
        ui.text(small, Rect::new(r.x, cy + 1.5, r.w, 14.0), 10.5, col::DIM, Weight::Regular, Align::Left);
    }
}

/// The text that fits `room` at `size`: `long`, else `short`.
fn fit<'a>(ui: &mut Ui, long: &'a str, short: &'a str, size: f32, room: f32) -> &'a str {
    if short.is_empty() || ui.measure(long, size, Weight::Bold) <= room { long } else { short }
}

/// The biggest size up to `size` (down to 10) at which `s` fits `room`.
pub fn fit_size(ui: &mut Ui, s: &str, size: f32, room: f32) -> f32 {
    let mut z = size;
    while z > 10.0 && ui.measure(s, z, Weight::Bold) > room {
        z -= 0.5;
    }
    z
}

/// A widget with nothing to show yet: its symbol, faint, and what it's
/// waiting for.
pub fn draw_empty(ui: &mut Ui, r: Rect, id: &str, size: Size) {
    let Some(d) = crate::barcfg::def(id) else { return };
    card(ui, r, col::DIM);
    let c = card_rect(r);
    let what = match id {
        "countdown" => "No countdown",
        "tasks" => "No tasks",
        "next_prayer" => "Set your place",
        "media" => "Not playing",
        "weather" => "No weather yet",
        _ => "Nothing yet",
    };
    match size {
        Size::S => disc(ui, c.cx(), c.cy(), 11.0, col::DIM, d.icon),
        _ => {
            let mut row = c.inset_xy(7.0, 0.0);
            let ic = row.cut_left(24.0, 7.0);
            disc(ui, ic.cx(), ic.cy(), 11.0, col::DIM, d.icon);
            if size == Size::M {
                ui.text("None yet", row, 11.5, col::DIM, Weight::Semi, Align::Left);
            } else {
                two_lines(ui, row, what, 12.5, d.name, col::DIM);
            }
        }
    }
}

/// A colour of the rainbow (0..1 around it), bright and a little soft.
pub fn hue(t: f32) -> Color {
    let h = (t.rem_euclid(1.0)) * 6.0;
    let (s, v) = (0.62, 1.0);
    let c = v * s;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    Color { r: r + m, g: g + m, b: b + m, a: 1.0 }
}

/// The sky's colour at `h` o'clock: night blue, dawn orange, day gold,
/// dusk red.
pub fn sky(h: f32) -> Color {
    const K: [(f32, u32); 9] = [(0.0, 0x2C2F6B), (5.0, 0x3D3A8C), (6.5, 0xFF8A65), (9.0, 0xFFC94A), (13.0, 0xFFD60A), (17.0, 0xFFB340), (18.8, 0xFF5E62), (20.5, 0x5B4FC0), (24.0, 0x2C2F6B)];
    let h = h.rem_euclid(24.0);
    for w in K.windows(2) {
        if h <= w[1].0 {
            let t = (h - w[0].0) / (w[1].0 - w[0].0);
            return hex(w[0].1).lerp(hex(w[1].1), t);
        }
    }
    hex(K[0].1)
}

/// The day as a band of sky (midnight to midnight), with a marker at `f`.
pub fn sky_band(ui: &mut Ui, r: Rect, f: f32) {
    let stops: Vec<(f32, Color)> = (0..=12).map(|i| (i as f32 / 12.0, sky(i as f32 * 2.0))).collect();
    ui.fill_rr_stops(r, r.h / 2.0, &stops, false);
    // What's still to come is dimmed.
    let x = r.x + r.w * f.clamp(0.0, 1.0);
    ui.push_clip(Rect::new(x, r.y - 1.0, r.r() - x + 1.0, r.h + 2.0));
    ui.fill_rr(r, r.h / 2.0, col::NOTCH.with_a(0.55));
    ui.pop_clip();
    ui.circle(x, r.cy(), r.h * 0.9 + 1.5, col::NOTCH.with_a(0.35));
    ui.circle(x, r.cy(), r.h * 0.9, hex(0xFFFFFF));
}

/// Where the sun is: an arc from sunrise (6:00) to sunset (18:00) over a
/// horizon; the sun on it by day, the moon over it by night.
pub fn sun_arc(ui: &mut Ui, r: Rect, n: &Now) {
    let h = n.hour as f32 + n.min as f32 / 60.0;
    let rad = (r.w / 2.0 - 3.0).min(r.h - 8.0);
    let (cx, base) = (r.cx(), r.cy() + rad / 2.0 + 1.0);
    ui.line(r.x + 1.0, base, r.r() - 1.0, base, col::TEXT.with_a(0.25), 1.0);
    ui.arc(cx, base, rad, -FRAC_PI_2, PI, col::TEXT.with_a(0.18), 1.2);
    let day = (h - 6.0) / 12.0;
    if (0.0..=1.0).contains(&day) {
        ui.arc(cx, base, rad, -FRAC_PI_2, day * PI, sky(h), 1.8);
        let a = -FRAC_PI_2 + day * PI;
        let (sx, sy) = (cx + rad * a.sin(), base - rad * a.cos());
        ui.circle(sx, sy, 5.5, sky(h).with_a(0.30));
        ui.circle(sx, sy, 3.3, sky(h).lerp(hex(0xFFFFFF), 0.25));
    } else {
        ui.icon("\u{E708}", Rect::new(cx - 7.0, base - rad * 0.75 - 7.0, 14.0, 14.0), 10.0, hex(0xC7B8FF));
        for (dx, dy) in [(-0.62, -0.28), (0.55, -0.42), (0.18, -1.05)] {
            ui.circle(cx + dx * rad, base + dy * rad, 0.9, hex(0xFFFFFF).with_a(0.8));
        }
    }
}

// ------------------------------------------------------------ any value

/// A card for a feature's status value at `size`. Battery, sound and
/// Wi-Fi get their own pictures; anything else a ring or a badge.
pub fn draw_value(ui: &mut Ui, r: Rect, w: &Widget, size: Size) {
    // ("Quiet" is for the notch, which hides calm values; a widget the
    // taskbar shows keeps its colour.) A few grey ones get one of their own.
    let tint = match w.id {
        "uptime" => hex(0x40C8E0),
        "bluetooth" if w.color == col::DIM => hex(0x0A84FF).lerp(col::DIM, 0.5),
        "mic" if w.value == "On" => col::GREEN,
        _ => w.color,
    };
    card(ui, r, tint);
    let c = card_rect(r);
    match w.id {
        "battery" => return battery_face(ui, c, w, size, tint),
        "volume" | "wifi" if w.ring.is_some() && size != Size::S => return meter_face(ui, c, w, size, tint),
        _ => {}
    }
    if value_face(ui, c, w, size, tint) {
        return;
    }
    let text_c = col::TEXT;
    let mark = |ui: &mut Ui, cx: f32, cy: f32, rad: f32| match w.ring {
        Some(f) => {
            ring_w(ui, cx, cy, rad, f, tint, 2.4);
            ui.icon(&w.icon, Rect::new(cx - rad, cy - rad, rad * 2.0, rad * 2.0), rad * 0.95, tint);
        }
        None => disc(ui, cx, cy, rad, tint, &w.icon),
    };
    match size {
        Size::S => {
            if w.short.is_empty() || w.short.chars().count() > 4 {
                mark(ui, c.cx(), c.cy(), 11.0);
            } else {
                mark(ui, c.cx(), c.y + 11.5, 8.0);
                ui.text(&w.short, Rect::new(c.x, c.y + 20.0, c.w, 13.0), 10.0, text_c, Weight::Bold, Align::Center);
            }
        }
        Size::M => {
            let mut row = c.inset_xy(7.0, 0.0);
            let ic = row.cut_left(24.0, 7.0);
            mark(ui, ic.cx(), ic.cy(), 11.0);
            let t = fit(ui, &w.value, &w.short, 15.0, row.w).to_string();
            let z = fit_size(ui, &t, 15.0, row.w);
            ui.text(&t, row, z, text_c, Weight::Bold, Align::Left);
        }
        Size::L => {
            let mut row = c.inset_xy(8.0, 0.0);
            let ic = row.cut_left(26.0, 8.0);
            mark(ui, ic.cx(), ic.cy(), 12.0);
            let small = if w.label != w.value { w.label.clone() } else { w.name.to_string() };
            let big = fit(ui, &w.value, &w.short, 15.0, row.w).to_string();
            let z = fit_size(ui, &big, 15.0, row.w);
            two_lines(ui, row, &big, z, &small, text_c);
        }
        Size::W => {
            let mut row = c.inset_xy(8.0, 0.0);
            let ic = row.cut_left(26.0, 8.0);
            mark(ui, ic.cx(), ic.cy(), 12.0);
            // A big faint symbol at the far end, for a little depth.
            let wm = Rect::new(c.r() - 40.0, c.y - 4.0, 44.0, c.h + 8.0);
            ui.push_clip(c);
            ui.icon(&w.icon, wm, 30.0, tint.with_a(0.16));
            ui.pop_clip();
            let small = if w.detail != w.value { w.detail.clone() } else if w.label != w.value { w.label.clone() } else { w.name.to_string() };
            let big = fit(ui, &w.value, &w.short, 15.0, row.w - 20.0).to_string();
            match w.ring {
                Some(f) => {
                    let bw = ui.measure(&big, 15.0, Weight::Bold);
                    ui.text(&big, Rect::new(row.x, c.cy() - 16.0, row.w, 18.0), 15.0, text_c, Weight::Bold, Align::Left);
                    ui.text(&small, Rect::new(row.x + bw + 7.0, c.cy() - 14.0, row.w - bw - 30.0, 15.0), 10.0, col::DIM, Weight::Regular, Align::Left);
                    grad_bar(ui, Rect::new(row.x, c.cy() + 6.0, row.w - 22.0, 4.0), f, tint.with_a(0.65), tint);
                }
                None => two_lines(ui, Rect::new(row.x, row.y, row.w - 20.0, row.h), &big, 15.0, &small, text_c),
            }
        }
    }
}

/// A battery drawn as one: its outline, the charge inside in its colour,
/// a bolt while charging.
pub fn battery_shape(ui: &mut Ui, r: Rect, f: f32, tint: Color, charging: bool) {
    let nub = (r.w * 0.08).max(2.0);
    let body = Rect::new(r.x, r.y, r.w - nub - 1.0, r.h);
    ui.stroke_rr(body.inset(0.6), r.h * 0.28, col::TEXT.with_a(0.55), 1.2);
    ui.fill_rr(Rect::new(body.r() + 1.0, r.cy() - r.h * 0.18, nub, r.h * 0.36), 1.0, col::TEXT.with_a(0.55));
    let inner = body.inset(2.4);
    if f > 0.0 {
        ui.fill_rr_grad(Rect::new(inner.x, inner.y, (inner.w * f.clamp(0.0, 1.0)).max(2.0), inner.h), (r.h * 0.18).max(1.0), tint.with_a(0.75), tint, false);
    }
    if charging {
        ui.icon("\u{E945}", body, r.h * 0.8, hex(0xFFFFFF));
    }
}

fn battery_face(ui: &mut Ui, c: Rect, w: &Widget, size: Size, tint: Color) {
    let f = w.ring.unwrap_or(0.0);
    let charging = w.label == "Charging";
    match size {
        Size::S => {
            battery_shape(ui, Rect::new(c.cx() - 13.0, c.y + 5.0, 26.0, 12.0), f, tint, charging);
            ui.text(&w.value, Rect::new(c.x, c.y + 19.0, c.w, 13.0), 10.0, col::TEXT, Weight::Bold, Align::Center);
        }
        Size::M => {
            let mut row = c.inset_xy(8.0, 0.0);
            let b = row.cut_left(30.0, 8.0);
            battery_shape(ui, Rect::new(b.x, b.cy() - 7.5, 30.0, 15.0), f, tint, charging);
            ui.text(&w.value, row, 15.0, col::TEXT, Weight::Bold, Align::Left);
        }
        Size::L => {
            let mut row = c.inset_xy(8.0, 0.0);
            let b = row.cut_left(34.0, 9.0);
            battery_shape(ui, Rect::new(b.x, b.cy() - 8.5, 34.0, 17.0), f, tint, charging);
            two_lines(ui, row, &w.value, 15.0, &w.label, col::TEXT);
        }
        Size::W => {
            let mut row = c.inset_xy(8.0, 0.0);
            let b = row.cut_left(34.0, 9.0);
            battery_shape(ui, Rect::new(b.x, b.cy() - 8.5, 34.0, 17.0), f, tint, charging);
            let meter = row.cut_right(62.0, 8.0);
            segments(ui, Rect::new(meter.x, meter.cy() - 5.0, meter.w, 10.0), 10, f, 2.0, |_| tint);
            let small = w.detail.split(" · ").nth(1).map(str::to_string).unwrap_or_else(|| w.label.clone());
            two_lines(ui, row, &w.value, 15.0, &small, col::TEXT);
        }
    }
}

/// Sound and Wi-Fi: a symbol and a rising meter of their level.
fn meter_face(ui: &mut Ui, c: Rect, w: &Widget, size: Size, tint: Color) {
    let f = w.ring.unwrap_or(0.0);
    let mut row = c.inset_xy(8.0, 0.0);
    if size == Size::M {
        // Small room: the meter is the symbol.
        let m = row.cut_left(30.0, 8.0);
        staircase(ui, Rect::new(m.x, m.cy() - 9.0, m.w, 18.0), 5, f, tint);
        let z = fit_size(ui, &w.value, 15.0, row.w);
        ui.text(&w.value, row, z, col::TEXT, Weight::Bold, Align::Left);
        return;
    }
    let ic = row.cut_left(24.0, 7.0);
    disc(ui, ic.cx(), ic.cy(), 11.0, tint, &w.icon);
    match size {
        Size::L => {
            let m = row.cut_right(40.0, 6.0);
            staircase(ui, Rect::new(m.x, m.cy() - 9.0, m.w, 18.0), 6, f, tint);
            two_lines(ui, row, &w.value, 15.0, &w.label, col::TEXT);
        }
        _ => {
            let m = row.cut_right(84.0, 8.0);
            staircase(ui, Rect::new(m.x, m.cy() - 10.0, m.w, 20.0), 12, f, tint);
            let small = if w.detail != w.value { w.detail.clone() } else { w.label.clone() };
            two_lines(ui, row, &w.value, 15.0, &small, col::TEXT);
        }
    }
}

// ------------------------------------------------------------ built in

/// Widgets that belong to no feature: drawn here (and the right end's
/// items, sys_*).
pub const BUILT_IN: [&str; 16] = ["clock", "date", "datetime", "dayprog", "yearprog", "lang", "recycle", "mode", "worldclock", "sun", "moon", "quote", "currency", "sys_lang", "sys_quick", "sys_clock"];

/// What a built-in widget needs that takes a moment to read (refreshed
/// every few seconds, not every frame).
#[derive(Default)]
pub struct Info {
    pub recycle: (u64, u64),
    recycle_at: f64,
    pub lang: (String, String, String),
    pub dark: bool,
    /// Wi-Fi, sound and battery, for the right end's group.
    pub quick: Vec<Widget>,
    /// World clock cities, where you are (lat, lon), the currency pair
    /// (places in tools::CURRENCIES) and the quote you're on.
    pub cities: Vec<&'static str>,
    pub place: Option<(f64, f64)>,
    pub fx: (usize, usize),
    pub quote: usize,
    /// Fixed drives: (letter, name, free, total).
    pub drives: Vec<(String, String, u64, u64)>,
    drives_at: f64,
    read_at: f64,
}

impl Info {
    /// Read again what can change (called once a second).
    pub fn refresh(&mut self, now: f64) {
        // (Faces that move are drawn ten times a second: read at most once.)
        if (now - self.read_at).abs() < 0.9 {
            return;
        }
        self.read_at = now;
        self.lang = keyboard_language();
        self.dark = dark_mode();
        if now - self.recycle_at > 5.0 {
            self.recycle_at = now;
            self.recycle = recycle_bin();
        }
        if now - self.drives_at > 30.0 {
            self.drives_at = now;
            self.drives = drives();
        }
    }

    /// Read again right away (after a click changed something).
    pub fn stale(&mut self) {
        self.recycle_at = 0.0;
        self.drives_at = 0.0;
        self.read_at = 0.0;
        self.refresh(crate::time::unix_now());
    }

    /// What your settings pick (cities, currencies, place, quote).
    pub fn settings(&mut self, st: &crate::store::Settings) {
        self.cities = crate::zones::picked(st);
        let cur = |k: &str, d: &str| {
            let v = st.extra.get(k).map(String::as_str).unwrap_or(d);
            crate::modules::tools::CURRENCIES.iter().position(|c| *c == v).unwrap_or(0)
        };
        self.fx = (cur("fx_base", "USD"), cur("fx_quote", "LKR"));
        self.place = crate::modules::widgets::PLACE.lock().ok().and_then(|p| p.clone()).or_else(crate::modules::widgets::tz_place).map(|p| (p.0, p.1));
        let skip: usize = st.extra.get("quote_skip").and_then(|v| v.parse().ok()).unwrap_or(0);
        let n = Now::get();
        self.quote = (n.year as usize * 366 + day_of_year(&n) as usize + skip) % QUOTES.len();
    }
}

/// Your fixed drives: (letter, name, free, total).
pub fn drives() -> Vec<(String, String, u64, u64)> {
    use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW};
    let mut out = Vec::new();
    let mask = unsafe { GetLogicalDrives() };
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = format!("{}:", (b'A' + i as u8) as char);
        let root: Vec<u16> = format!("{letter}\\").encode_utf16().chain(Some(0)).collect();
        let p = windows::core::PCWSTR(root.as_ptr());
        if unsafe { GetDriveTypeW(p) } != 3 {
            continue; // fixed drives only
        }
        let (mut free, mut total) = (0u64, 0u64);
        if unsafe { GetDiskFreeSpaceExW(p, None, Some(&mut total), Some(&mut free)) }.is_err() || total == 0 {
            continue;
        }
        let mut name = [0u16; 64];
        let _ = unsafe { GetVolumeInformationW(p, Some(&mut name), None, None, None, None) };
        let name = String::from_utf16_lossy(&name[..name.iter().position(|&c| c == 0).unwrap_or(0)]);
        out.push((letter, if name.is_empty() { "Local Disk".into() } else { name }, free, total));
    }
    out
}

/// Your offset from UTC now, in minutes.
pub fn tz_minutes() -> f64 {
    use windows::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
    let mut tz = TIME_ZONE_INFORMATION::default();
    let r = unsafe { GetTimeZoneInformation(&mut tz) };
    -(tz.Bias + if r == 2 { tz.DaylightBias } else { 0 }) as f64
}

/// "6:02 AM" for minutes after midnight.
pub fn hm(min: f64) -> String {
    let m = min.rem_euclid(1440.0) as u32;
    crate::time::clock_full(m / 60, m % 60)
}

/// Quotes of the day (one a day, the same for everyone).
pub const QUOTES: [(&str, &str); 36] = [
    ("Well begun is half done.", "Aristotle"),
    ("The secret of getting ahead is getting started.", "Mark Twain"),
    ("It always seems impossible until it's done.", "Nelson Mandela"),
    ("Simplicity is the ultimate sophistication.", "Leonardo da Vinci"),
    ("Whatever you are, be a good one.", "Abraham Lincoln"),
    ("Energy and persistence conquer all things.", "Benjamin Franklin"),
    ("Do what you can, with what you have, where you are.", "Theodore Roosevelt"),
    ("Knowing is not enough; we must apply.", "Goethe"),
    ("The best way out is always through.", "Robert Frost"),
    ("Little by little, one travels far.", "J.R.R. Tolkien"),
    ("Quality is not an act, it is a habit.", "Aristotle"),
    ("Action is the foundational key to all success.", "Pablo Picasso"),
    ("Done is better than perfect.", "Sheryl Sandberg"),
    ("Make each day your masterpiece.", "John Wooden"),
    ("Stay hungry, stay foolish.", "Stewart Brand"),
    ("The journey of a thousand miles begins with one step.", "Lao Tzu"),
    ("Happiness depends upon ourselves.", "Aristotle"),
    ("Fortune favors the bold.", "Virgil"),
    ("He who has a why can bear almost any how.", "Friedrich Nietzsche"),
    ("Act as if what you do makes a difference. It does.", "William James"),
    ("Turn your wounds into wisdom.", "Oprah Winfrey"),
    ("Dream big and dare to fail.", "Norman Vaughan"),
    ("What we think, we become.", "Buddha"),
    ("Be yourself; everyone else is already taken.", "Oscar Wilde"),
    ("Doubt kills more dreams than failure ever will.", "Suzy Kassem"),
    ("Small deeds done are better than great deeds planned.", "Peter Marshall"),
    ("Focus on being productive instead of busy.", "Tim Ferriss"),
    ("Great things are done by a series of small things.", "Vincent van Gogh"),
    ("Nothing will work unless you do.", "Maya Angelou"),
    ("If you want to lift yourself up, lift up someone else.", "Booker T. Washington"),
    ("The harder I work, the luckier I get.", "Samuel Goldwyn"),
    ("Life is short, and it is up to you to make it sweet.", "Sarah Louise Delany"),
    ("Patience is bitter, but its fruit is sweet.", "Aristotle"),
    ("Where there is love there is life.", "Mahatma Gandhi"),
    ("Keep your face always toward the sunshine.", "Walt Whitman"),
    ("Every moment is a fresh beginning.", "T.S. Eliot"),
];

/// What a built-in face shows now (a change draws it again).
pub fn key(id: &str, size: Size, info: &Info) -> String {
    let n = Now::get();
    let date = format!("{}-{}-{}", n.year, n.month, n.day);
    match id {
        "clock" => format!("{}:{}:{}", n.hour, n.min, n.sec),
        "date" => date,
        "datetime" => format!("{date} {}:{}:{}", n.hour, n.min, if size == Size::W { n.sec } else { 0 }),
        "dayprog" => format!("{}:{}", n.hour, if size == Size::S { 0 } else { n.min }),
        "yearprog" => format!("{date} {}", (year_fraction(&n) * 1000.0) as u32),
        "lang" | "sys_lang" => format!("{:?}", info.lang),
        "recycle" => format!("{:?}", info.recycle),
        "mode" => format!("{}", info.dark),
        "sys_clock" => format!("{date} {}:{} {}", n.hour, n.min, crate::time::h12()),
        "worldclock" => format!("{:?} {}:{}", info.cities, n.hour, n.min),
        "sun" => format!("{date} {}:{} {:?}", n.hour, n.min, info.place.map(|p| ((p.0 * 100.0) as i32, (p.1 * 100.0) as i32))),
        "moon" => format!("{date} {}", n.hour),
        "quote" => format!("{date} {}", info.quote),
        "currency" => format!("{:?} {:?}", info.fx, crate::modules::tools::rates_for_widget().map(|r| (r.0, r.1.map(|x| (x * 1e4) as i64)))),
        "sys_quick" => info.quick.iter().map(|w| format!("{}{}{:?}", w.icon, w.value, w.ring.map(|f| (f * 20.0) as u32))).collect(),
        _ => String::new(),
    }
}

pub fn day_fraction(n: &Now) -> f32 {
    (n.hour as f32 * 3600.0 + n.min as f32 * 60.0 + n.sec as f32) / 86400.0
}

pub fn day_of_year(n: &Now) -> u32 {
    (1..n.month as u32).map(|m| crate::time::days_in_month(n.year as i32, m)).sum::<u32>() + n.day as u32
}

pub fn year_days(y: u16) -> u32 {
    if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 { 366 } else { 365 }
}

pub fn year_fraction(n: &Now) -> f32 {
    ((day_of_year(n) - 1) as f32 + day_fraction(n)) / year_days(n.year) as f32
}

/// ISO week number.
pub fn week(n: &Now) -> u32 {
    let weeks_in = |y: i32| {
        let p = |y: i32| (y + y / 4 - y / 100 + y / 400) % 7;
        if p(y) == 4 || p(y - 1) == 3 { 53 } else { 52 }
    };
    let doy = day_of_year(n) as i32;
    let dow = if n.dow == 0 { 7 } else { n.dow as i32 };
    let w = (doy - dow + 10) / 7;
    if w < 1 {
        weeks_in(n.year as i32 - 1)
    } else if w > weeks_in(n.year as i32) as i32 {
        1
    } else {
        w as u32
    }
}

/// "11:45" and "AM" (empty on a 24-hour clock).
pub fn time_parts(n: &Now) -> (String, String) {
    let full = crate::time::clock_full(n.hour as u32, n.min as u32);
    match full.rsplit_once(' ') {
        Some((t, ampm)) => (t.to_string(), ampm.to_string()),
        None => (full, String::new()),
    }
}

pub fn draw(ui: &mut Ui, r: Rect, id: &str, size: Size, info: &Info) {
    let n = Now::get();
    // The right end's items: like Windows' own, no card.
    match id {
        "sys_lang" => return sys_lang(ui, r, info),
        "sys_quick" => return sys_quick(ui, r, info),
        "sys_clock" => return sys_clock(ui, r, &n),
        _ => {}
    }
    let tint = match id {
        "clock" => C_CLOCK,
        "date" => C_DATE,
        "datetime" => C_DATETIME,
        "dayprog" => sky(n.hour as f32 + n.min as f32 / 60.0).lerp(C_DAY, 0.35),
        "yearprog" => C_YEAR,
        "lang" => C_LANG,
        "recycle" => if info.recycle.0 > 0 { col::GREEN } else { C_BIN },
        "mode" => if info.dark { col::PURPLE } else { col::YELLOW },
        "worldclock" => C_WORLD,
        "sun" => C_SUN,
        "moon" => C_MOON,
        "quote" => C_QUOTE,
        "currency" => C_MONEY,
        _ => col::DIM,
    };
    card(ui, r, tint);
    let c = card_rect(r);
    match id {
        "clock" => draw_clock(ui, c, size, &n),
        "date" => draw_date(ui, c, size, &n),
        "datetime" => draw_datetime(ui, c, size, &n),
        "dayprog" => draw_day(ui, c, size, &n),
        "worldclock" => draw_world(ui, c, size, info),
        "sun" => draw_sun(ui, c, size, info, &n),
        "moon" => draw_moon_face(ui, c, size),
        "quote" => draw_quote(ui, c, size, info),
        "currency" => draw_currency(ui, c, size, info),
        "yearprog" => draw_year(ui, c, size, &n),
        "lang" => {
            let (code, region, name) = &info.lang;
            match size {
                Size::S => {
                    let b = Rect::new(c.cx() - 14.0, c.y + 3.0, 28.0, 17.0);
                    ui.fill_rr_grad(b, 5.0, C_LANG, hex(0x5E5CE6), false);
                    ui.text(code, b, 10.5, hex(0xFFFFFF), Weight::Bold, Align::Center);
                    ui.text(region, Rect::new(c.x, c.y + 20.0, c.w, 13.0), 9.5, col::DIM, Weight::Semi, Align::Center);
                }
                _ => {
                    let mut row = c.inset_xy(8.0, 0.0);
                    let badge = row.cut_left(30.0, 8.0).centered(30.0, 22.0);
                    ui.fill_rr_grad(badge, 6.0, C_LANG, hex(0x5E5CE6), false);
                    ui.text(code, badge, 11.0, hex(0xFFFFFF), Weight::Bold, Align::Center);
                    two_lines(ui, row, name, 13.0, region, col::TEXT);
                }
            }
        }
        "recycle" => {
            let (items, bytes) = info.recycle;
            let icon = "\u{E74D}";
            let count = if items == 0 { "Empty".to_string() } else if items == 1 { "1 item".to_string() } else { format!("{items} items") };
            match size {
                Size::S => {
                    disc(ui, c.cx(), c.cy(), 11.0, tint, icon);
                    if items > 0 {
                        let t = items.min(99).to_string();
                        let bw = (ui.measure(&t, 8.5, Weight::Bold) + 6.0).max(13.0);
                        let b = Rect::new(c.cx() + 3.0, c.y + 1.0, bw, 12.0);
                        ui.fill_rr(b, 6.0, col::GREEN);
                        ui.text(&t, b, 8.5, hex(0x0B2A12), Weight::Bold, Align::Center);
                    }
                }
                Size::M => {
                    let mut row = c.inset_xy(7.0, 0.0);
                    let ic = row.cut_left(24.0, 7.0);
                    disc(ui, ic.cx(), ic.cy(), 11.0, tint, icon);
                    ui.text(&count, row, 13.0, col::TEXT, Weight::Bold, Align::Left);
                }
                _ => {
                    let mut row = c.inset_xy(8.0, 0.0);
                    let ic = row.cut_left(26.0, 8.0);
                    disc(ui, ic.cx(), ic.cy(), 12.0, tint, icon);
                    if size == Size::W {
                        // How full: against 1 GB, a gentle scale.
                        let m = row.cut_right(70.0, 8.0);
                        grad_bar(ui, Rect::new(m.x, m.cy() - 2.5, m.w, 5.0), (bytes as f32 / 1073741824.0).sqrt().min(1.0), col::GREEN, col::YELLOW);
                    }
                    two_lines(ui, row, &count, 14.0, &if items > 0 { size_text(bytes) } else { "Recycle Bin".into() }, col::TEXT);
                }
            }
        }
        "mode" => {
            let word = if info.dark { "Dark" } else { "Light" };
            match size {
                Size::S => mode_switch(ui, c.centered(34.0, 20.0), info.dark),
                _ => {
                    let mut row = c.inset_xy(8.0, 0.0);
                    mode_switch(ui, row.cut_left(36.0, 9.0).centered(36.0, 20.0), info.dark);
                    if size == Size::M {
                        ui.text(word, row, 13.5, col::TEXT, Weight::Bold, Align::Left);
                    } else {
                        two_lines(ui, row, word, 13.5, "Windows mode", col::TEXT);
                    }
                }
            }
        }
        _ => {}
    }
}

/// Light / dark as a little switch: a sunny or starry track and a knob
/// with the sun or the moon.
pub fn mode_switch(ui: &mut Ui, r: Rect, dark: bool) {
    if dark {
        ui.fill_rr_grad(r, r.h / 2.0, hex(0x2C2F6B), hex(0x5B4FC0), false);
        for (dx, dy) in [(0.18, 0.3), (0.32, 0.68), (0.45, 0.4)] {
            ui.circle(r.x + r.w * dx, r.y + r.h * dy, 0.9, hex(0xFFFFFF).with_a(0.85));
        }
    } else {
        ui.fill_rr_grad(r, r.h / 2.0, hex(0x64D2FF), hex(0xFFD60A), false);
    }
    let k = r.h - 4.0;
    let kx = if dark { r.r() - 2.0 - k } else { r.x + 2.0 };
    let knob = Rect::new(kx, r.y + 2.0, k, k);
    ui.fill_rr(knob, k / 2.0, hex(0xFFFFFF));
    ui.icon(if dark { "\u{E708}" } else { "\u{E706}" }, knob, k * 0.62, if dark { hex(0x5B4FC0) } else { hex(0xFF9F0A) });
}

fn draw_clock(ui: &mut Ui, c: Rect, size: Size, n: &Now) {
    let (t, ampm) = time_parts(n);
    let secs = n.sec as f32 / 60.0;
    match size {
        Size::S => {
            ui.digits(&t, Rect::new(c.x, c.y + 3.0, c.w, 20.0), 12.5, col::TEXT, Align::Center);
            // The minute going by: a line that fills.
            grad_bar(ui, Rect::new(c.x + 8.0, c.b() - 9.0, c.w - 16.0, 2.5), secs, C_CLOCK.with_a(0.7), C_CLOCK);
        }
        Size::M => {
            let tw = ui.measure_digits(&t, 17.0);
            let aw = if ampm.is_empty() { 0.0 } else { ui.measure(&ampm, 9.5, Weight::Bold) + 3.0 };
            let x = c.cx() - (tw + aw) / 2.0;
            ui.digits(&t, Rect::new(x, c.y, tw + 2.0, c.h - 5.0), 17.0, col::TEXT, Align::Left);
            if !ampm.is_empty() {
                ui.text(&ampm, Rect::new(x + tw + 3.0, c.cy() - 9.0, aw, 11.0), 9.5, C_CLOCK, Weight::Bold, Align::Left);
            }
            grad_bar(ui, Rect::new(c.x + 12.0, c.b() - 6.5, c.w - 24.0, 2.5), secs, C_CLOCK.with_a(0.7), C_CLOCK);
        }
        Size::L => {
            let mut row = c.inset_xy(7.0, 0.0);
            let dial = row.cut_left(30.0, 9.0);
            analog(ui, dial.cx(), dial.cy(), 14.0, n);
            let tw = ui.measure_digits(&t, 18.0);
            ui.digits(&t, Rect::new(row.x, c.y, tw + 2.0, c.h), 18.0, col::TEXT, Align::Left);
            let side = Rect::new(row.x + tw + 4.0, c.y, 30.0, c.h);
            if !ampm.is_empty() {
                ui.text(&ampm, Rect::new(side.x, side.cy() - 13.0, side.w, 12.0), 9.0, col::DIM, Weight::Bold, Align::Left);
            }
            ui.digits(&format!("{:02}", n.sec), Rect::new(side.x, side.cy() - 1.5, side.w, 13.0), 11.0, C_CLOCK, Align::Left);
        }
        Size::W => {
            let mut row = c.inset_xy(7.0, 0.0);
            let dial = row.cut_left(30.0, 9.0);
            analog(ui, dial.cx(), dial.cy(), 14.0, n);
            let arc = row.cut_right(54.0, 6.0);
            sun_arc(ui, arc, n);
            let tw = ui.measure_digits(&t, 17.0);
            ui.digits(&t, Rect::new(row.x, c.cy() - 17.0, tw + 2.0, 21.0), 17.0, col::TEXT, Align::Left);
            ui.digits(&format!(":{:02}", n.sec), Rect::new(row.x + tw + 1.0, c.cy() - 14.0, 26.0, 17.0), 11.0, C_CLOCK, Align::Left);
            let under = if ampm.is_empty() { utc_offset() } else { format!("{ampm}  \u{00B7}  {}", utc_offset()) };
            ui.text(&under, Rect::new(row.x, c.cy() + 3.0, row.w, 13.0), 9.5, col::DIM, Weight::Regular, Align::Left);
        }
    }
}

/// A small clock face with hands, in the clock's colour.
pub fn analog(ui: &mut Ui, cx: f32, cy: f32, rad: f32, n: &Now) {
    ui.circle(cx, cy, rad, C_CLOCK.with_a(0.16));
    ui.stroke_circle(cx, cy, rad - 0.5, C_CLOCK.with_a(0.45), 1.0);
    for k in 0..12 {
        let a = k as f32 / 12.0 * TAU;
        let (s, co) = a.sin_cos();
        let inner = if k % 3 == 0 { rad - 4.0 } else { rad - 2.6 };
        ui.line(cx + s * inner, cy - co * inner, cx + s * (rad - 1.5), cy - co * (rad - 1.5), if k % 3 == 0 { col::TEXT } else { col::DIM }, 1.0);
    }
    let hand = |ui: &mut Ui, f: f32, len: f32, w: f32, c: Color| {
        let a = f * TAU;
        ui.line(cx - a.sin() * 2.0, cy + a.cos() * 2.0, cx + a.sin() * len, cy - a.cos() * len, c, w);
    };
    let m = n.min as f32 + n.sec as f32 / 60.0;
    hand(ui, ((n.hour % 12) as f32 + m / 60.0) / 12.0, rad * 0.5, 2.0, col::TEXT);
    hand(ui, m / 60.0, rad * 0.76, 1.5, col::TEXT);
    hand(ui, n.sec as f32 / 60.0, rad * 0.84, 0.9, C_CLOCK);
    ui.circle(cx, cy, 1.7, C_CLOCK);
}

/// "UTC+5:30".
pub fn utc_offset() -> String {
    use windows::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
    let mut tz = TIME_ZONE_INFORMATION::default();
    let r = unsafe { GetTimeZoneInformation(&mut tz) };
    let bias = -(tz.Bias + if r == 2 { tz.DaylightBias } else { 0 });
    let (h, m) = (bias.abs() / 60, bias.abs() % 60);
    let sign = if bias < 0 { '-' } else { '+' };
    if m == 0 { format!("UTC{sign}{h}") } else { format!("UTC{sign}{h}:{m:02}") }
}

/// The week around today as seven little day cells (Monday first), today
/// lit in `tint`.
pub fn week_strip(ui: &mut Ui, r: Rect, n: &Now, tint: Color) {
    let today = if n.dow == 0 { 6 } else { n.dow as i32 - 1 };
    let w = r.w / 7.0;
    for k in 0..7i32 {
        let cell = Rect::new(r.x + k as f32 * w, r.y, w, r.h);
        let d = date_add(n, k - today);
        let letter = ["M", "T", "W", "T", "F", "S", "S"][k as usize];
        if k == today {
            ui.fill_rr_grad(cell.inset_xy(1.0, 0.0), 7.0, tint, tint.lerp(hex(0xFF9F0A), 0.5), true);
        }
        let (lc, dc) = if k == today { (hex(0xFFFFFF).with_a(0.85), hex(0xFFFFFF)) } else if k >= 5 { (C_DATE.with_a(0.8), col::TEXT) } else { (col::DIM, col::TEXT) };
        ui.text(letter, Rect::new(cell.x, cell.y + 2.0, cell.w, 11.0), 8.5, lc, Weight::Semi, Align::Center);
        ui.text(&d.to_string(), Rect::new(cell.x, cell.y + 12.5, cell.w, cell.h - 13.0), 11.5, if k < today { dc.with_a(0.55) } else { dc }, Weight::Bold, Align::Center);
    }
}

/// The day of the month `k` days from today.
pub fn date_add(n: &Now, k: i32) -> u32 {
    let mut d = n.day as i32 + k;
    let (mut y, mut m) = (n.year as i32, n.month as i32);
    loop {
        if d < 1 {
            m -= 1;
            if m < 1 {
                m = 12;
                y -= 1;
            }
            d += crate::time::days_in_month(y, m as u32) as i32;
        } else if d > crate::time::days_in_month(y, m as u32) as i32 {
            d -= crate::time::days_in_month(y, m as u32) as i32;
            m += 1;
            if m > 12 {
                m = 1;
                y += 1;
            }
        } else {
            return d as u32;
        }
    }
}

fn draw_date(ui: &mut Ui, c: Rect, size: Size, n: &Now) {
    let wd_full = WEEKDAYS[n.dow as usize % 7];
    let mon = MONTHS[(n.month as usize).saturating_sub(1) % 12];
    let mon_full = MONTH_NAMES[(n.month as usize).saturating_sub(1) % 12];
    match size {
        Size::S => calendar_page(ui, c.centered(28.0, 30.0), n),
        Size::M => {
            let mut row = c.inset_xy(7.0, 0.0);
            calendar_page(ui, row.cut_left(25.0, 8.0).centered(25.0, 27.0), n);
            let day = if ui.measure(wd_full, 10.5, Weight::Regular) <= row.w { wd_full } else { DAYS[n.dow as usize % 7] };
            two_lines(ui, row, &format!("{} {mon}", n.day), 14.0, day, col::TEXT);
        }
        Size::L => {
            let mut row = c.inset_xy(7.0, 0.0);
            calendar_page(ui, row.cut_left(28.0, 9.0).centered(28.0, 30.0), n);
            two_lines(ui, row, wd_full, 14.0, &format!("{} {mon_full}  \u{00B7}  W{}", n.day, week(n)), col::TEXT);
        }
        Size::W => {
            let mut row = c.inset_xy(7.0, 0.0);
            let left = row.cut_left(44.0, 6.0);
            ui.text(&mon.to_uppercase(), Rect::new(left.x, c.y + 3.0, left.w, 12.0), 9.0, C_DATE, Weight::Bold, Align::Left);
            ui.text(&format!("Week {}", week(n)), Rect::new(left.x, c.y + 17.0, left.w, 12.0), 9.0, col::DIM, Weight::Semi, Align::Left);
            week_strip(ui, Rect::new(row.x, c.y + 2.0, row.w, c.h - 4.0), n, C_DATE);
        }
    }
}

fn draw_datetime(ui: &mut Ui, c: Rect, size: Size, n: &Now) {
    let wd = DAYS[n.dow as usize % 7];
    let wd_full = WEEKDAYS[n.dow as usize % 7];
    let mon = MONTHS[(n.month as usize).saturating_sub(1) % 12];
    let mon_full = MONTH_NAMES[(n.month as usize).saturating_sub(1) % 12];
    let time = crate::time::clock_full(n.hour as u32, n.min as u32);
    match size {
        Size::S | Size::M => {
            ui.text(&time, Rect::new(c.x, c.cy() - 16.0, c.w, 19.0), 14.5, col::TEXT, Weight::Bold, Align::Center);
            ui.text(&format!("{wd} {} {mon}", n.day), Rect::new(c.x, c.cy() + 2.0, c.w, 13.0), 10.0, C_DATETIME, Weight::Semi, Align::Center);
        }
        Size::L => {
            let mut row = c.inset_xy(7.0, 0.0);
            calendar_page(ui, row.cut_left(28.0, 9.0).centered(28.0, 30.0), n);
            ui.text(&time, Rect::new(row.x, c.cy() - 16.5, row.w, 19.0), 15.5, col::TEXT, Weight::Bold, Align::Left);
            ui.text(&format!("{wd_full}, {mon} {}", n.day), Rect::new(row.x, c.cy() + 2.0, row.w, 13.0), 10.0, col::DIM, Weight::Regular, Align::Left);
        }
        Size::W => {
            let mut row = c.inset_xy(7.0, 0.0);
            calendar_page(ui, row.cut_left(28.0, 9.0).centered(28.0, 30.0), n);
            let (t, ampm) = time_parts(n);
            let tw = ui.measure_digits(&t, 17.0);
            ui.digits(&t, Rect::new(row.x, c.y + 1.0, tw + 2.0, 20.0), 17.0, col::TEXT, Align::Left);
            ui.digits(&format!(":{:02}", n.sec), Rect::new(row.x + tw + 1.0, c.y + 4.0, 24.0, 16.0), 10.5, C_DATETIME, Align::Left);
            if !ampm.is_empty() {
                ui.text(&ampm, Rect::new(row.x + tw + 23.0, c.y + 5.0, 22.0, 15.0), 9.0, col::DIM, Weight::Bold, Align::Left);
            }
            let _ = (wd_full, mon_full);
            let date = format!("{wd}, {} {mon}", n.day);
            let dw = ui.measure(&date, 10.0, Weight::Semi);
            ui.text(&date, Rect::new(row.r() - dw - 1.0, c.y + 5.0, dw + 2.0, 14.0), 10.0, col::DIM, Weight::Semi, Align::Left);
            // The day as a band of sky, with now on it.
            sky_band(ui, Rect::new(row.x, c.b() - 10.0, row.w, 4.0), day_fraction(n));
        }
    }
}

fn draw_day(ui: &mut Ui, c: Rect, size: Size, n: &Now) {
    let f = day_fraction(n);
    let pct = (f * 100.0).floor() as u32;
    let h = n.hour as f32 + n.min as f32 / 60.0;
    let tint = sky(h).lerp(C_DAY, 0.35);
    let m = ((1.0 - f) * 1440.0) as u32;
    let left = format!("{} h {} min left", m / 60, m % 60);
    match size {
        Size::S => {
            ring_w(ui, c.cx(), c.cy(), 12.5, f, tint, 3.0);
            ui.text(&pct.to_string(), c, 10.0, col::TEXT, Weight::Bold, Align::Center);
        }
        Size::M => {
            let mut row = c.inset_xy(7.0, 0.0);
            let ic = row.cut_left(24.0, 7.0);
            ring_w(ui, ic.cx(), ic.cy(), 10.5, f, tint, 2.6);
            ui.icon(if (6.0..18.0).contains(&h) { "\u{E706}" } else { "\u{E708}" }, ic, 10.0, tint);
            two_lines(ui, row, &format!("{pct}%"), 15.0, "of today", col::TEXT);
        }
        Size::L => {
            let r = c.inset_xy(9.0, 0.0);
            let head = format!("{pct}%");
            let hw = ui.measure(&head, 14.0, Weight::Bold);
            ui.text(&head, Rect::new(r.x, c.y + 3.0, hw + 2.0, 17.0), 14.0, col::TEXT, Weight::Bold, Align::Left);
            ui.text(&left, Rect::new(r.x + hw + 6.0, c.y + 5.5, r.w - hw - 6.0, 13.0), 9.5, col::DIM, Weight::Regular, Align::Left);
            sky_band(ui, Rect::new(r.x, c.b() - 11.0, r.w, 5.0), f);
        }
        Size::W => {
            let r = c.inset_xy(9.0, 0.0);
            let head = format!("{pct}% of today");
            ui.text(&head, Rect::new(r.x, c.y + 2.0, r.w, 16.0), 12.5, col::TEXT, Weight::Bold, Align::Left);
            let lw = ui.measure(&left, 9.5, Weight::Regular);
            ui.text(&left, Rect::new(r.r() - lw - 1.0, c.y + 4.0, lw + 2.0, 13.0), 9.5, col::DIM, Weight::Regular, Align::Left);
            // 24 hours, each in the sky's colour at that hour.
            segments(ui, Rect::new(r.x, c.b() - 13.0, r.w, 8.0), 24, f, 1.5, |i| sky(i as f32 + 0.5));
        }
    }
}

fn draw_year(ui: &mut Ui, c: Rect, size: Size, n: &Now) {
    let f = year_fraction(n);
    let pct = (f * 100.0).floor() as u32;
    let d = year_days(n.year) - day_of_year(n);
    let left = if d == 1 { "1 day left".to_string() } else { format!("{d} days left") };
    let month_color = |i: usize| hue(0.33 - i as f32 / 12.0 * 0.9);
    match size {
        Size::S => {
            ring_w(ui, c.cx(), c.cy(), 12.5, f, C_YEAR, 3.0);
            ui.text(&pct.to_string(), c, 10.0, col::TEXT, Weight::Bold, Align::Center);
        }
        Size::M => {
            let mut row = c.inset_xy(7.0, 0.0);
            let ic = row.cut_left(24.0, 7.0);
            ring_w(ui, ic.cx(), ic.cy(), 10.5, f, C_YEAR, 2.6);
            two_lines(ui, row, &format!("{pct}%"), 15.0, &n.year.to_string(), col::TEXT);
        }
        Size::L => {
            let r = c.inset_xy(9.0, 0.0);
            let head = format!("{pct}% of {}", n.year);
            let hw = ui.measure(&head, 12.5, Weight::Bold);
            ui.text(&head, Rect::new(r.x, c.y + 3.0, hw + 2.0, 16.0), 12.5, col::TEXT, Weight::Bold, Align::Left);
            ui.text(&format!("{d}d"), Rect::new(r.x + hw + 6.0, c.y + 5.0, 40.0, 13.0), 9.5, col::DIM, Weight::Regular, Align::Left);
            let mf = (n.month as f32 - 1.0 + n.day as f32 / crate::time::days_in_month(n.year as i32, n.month as u32) as f32) / 12.0;
            segments(ui, Rect::new(r.x, c.b() - 12.0, r.w, 6.0), 12, mf, 2.0, month_color);
        }
        Size::W => {
            let r = c.inset_xy(9.0, 0.0);
            let head = format!("{pct}% of {}", n.year);
            ui.text(&head, Rect::new(r.x, c.y + 1.5, r.w, 15.0), 12.0, col::TEXT, Weight::Bold, Align::Left);
            let lw = ui.measure(&left, 9.5, Weight::Regular);
            ui.text(&left, Rect::new(r.r() - lw - 1.0, c.y + 3.0, lw + 2.0, 13.0), 9.5, col::DIM, Weight::Regular, Align::Left);
            let mf = (n.month as f32 - 1.0 + n.day as f32 / crate::time::days_in_month(n.year as i32, n.month as u32) as f32) / 12.0;
            let seg = Rect::new(r.x, c.b() - 16.0, r.w, 5.0);
            segments(ui, seg, 12, mf, 2.0, month_color);
            let w = r.w / 12.0;
            for (i, l) in ["J", "F", "M", "A", "M", "J", "J", "A", "S", "O", "N", "D"].iter().enumerate() {
                let now = i + 1 == n.month as usize;
                ui.text(l, Rect::new(r.x + i as f32 * w, c.b() - 11.0, w, 10.0), 7.5, if now { col::TEXT } else { col::FAINT }, if now { Weight::Bold } else { Weight::Regular }, Align::Center);
            }
        }
    }
}

// ------------------------------------------------------------ new widgets

/// The day or night where a city is, as a small dot in the sky's colour.
fn sky_dot(ui: &mut Ui, cx: f32, cy: f32, hour: f32, r: f32) {
    ui.circle(cx, cy, r + 1.5, sky(hour).with_a(0.25));
    ui.circle(cx, cy, r, sky(hour));
}

fn draw_world(ui: &mut Ui, c: Rect, size: Size, info: &Info) {
    let cities: Vec<(&str, (u16, u16, i32, i32))> = info.cities.iter().filter_map(|c| crate::zones::time_in(c).map(|t| (*c, t))).collect();
    if cities.is_empty() {
        two_lines(ui, c.inset_xy(10.0, 0.0), "World clock", 13.0, "Pick cities in its bubble", col::TEXT);
        return;
    }
    let time = |t: &(u16, u16, i32, i32)| crate::time::clock_full(t.0 as u32, t.1 as u32);
    let day = |d: i32| match d {
        1 => " +1",
        -1 => " \u{2212}1",
        _ => "",
    };
    match size {
        Size::M | Size::S => {
            let (name, t) = cities[0];
            let mut row = c.inset_xy(8.0, 0.0);
            let dot = row.cut_left(10.0, 6.0);
            sky_dot(ui, dot.cx(), dot.cy(), t.0 as f32 + t.1 as f32 / 60.0, 4.0);
            ui.text(name, Rect::new(row.x, c.cy() - 15.0, row.w, 13.0), 9.5, C_WORLD, Weight::Bold, Align::Left);
            ui.text(&format!("{}{}", time(&t), day(t.2)), Rect::new(row.x, c.cy() - 2.0, row.w, 17.0), 13.0, col::TEXT, Weight::Bold, Align::Left);
        }
        Size::L => {
            let rows = c.inset_xy(9.0, 3.0).rows(2, 0.0);
            for (k, (name, t)) in cities.iter().take(2).enumerate() {
                let mut r = rows[k];
                let dot = r.cut_left(8.0, 6.0);
                sky_dot(ui, dot.cx(), r.cy(), t.0 as f32 + t.1 as f32 / 60.0, 3.2);
                let tm = format!("{}{}", time(t), day(t.2));
                let tw = ui.measure(&tm, 11.5, Weight::Bold);
                ui.text(&tm, Rect::new(r.r() - tw - 1.0, r.y, tw + 2.0, r.h), 11.5, col::TEXT, Weight::Bold, Align::Left);
                ui.text(name, Rect::new(r.x, r.y, r.w - tw - 4.0, r.h), 10.5, col::DIM, Weight::Semi, Align::Left);
            }
        }
        Size::W => {
            let n = cities.len().min(3);
            for (k, cell) in c.inset_xy(6.0, 0.0).columns(n, 4.0).into_iter().enumerate() {
                let (name, t) = cities[k];
                if k > 0 {
                    ui.fill_rr(Rect::new(cell.x - 2.5, c.y + 8.0, 1.0, c.h - 16.0), 0.5, col::TEXT.with_a(0.12));
                }
                sky_dot(ui, cell.x + 6.0, c.y + 10.0, t.0 as f32 + t.1 as f32 / 60.0, 3.0);
                ui.text(name, Rect::new(cell.x + 12.0, c.y + 3.0, cell.w - 12.0, 13.0), 9.0, C_WORLD, Weight::Bold, Align::Left);
                let tm = time(&t);
                let z = fit_size(ui, &tm, 13.0, cell.w - 4.0);
                ui.text(&format!("{tm}{}", day(t.2)), Rect::new(cell.x + 2.0, c.y + 16.0, cell.w, 17.0), z, col::TEXT, Weight::Bold, Align::Left);
            }
        }
    }
}

/// Sunrise and sunset where you are today (minutes after midnight).
pub fn sun_today(info: &Info, n: &Now) -> Option<(f64, f64)> {
    let (lat, lon) = info.place?;
    crate::astro::sun_times(lat, lon, day_of_year(n), tz_minutes())
}

fn draw_sun(ui: &mut Ui, c: Rect, size: Size, info: &Info, n: &Now) {
    let now = n.hour as f64 * 60.0 + n.min as f64;
    let Some((rise, set)) = sun_today(info, n) else {
        two_lines(ui, c.inset_xy(10.0, 0.0), "Sun", 13.0, "Set your place in Settings", col::TEXT);
        return;
    };
    let (next_up, next_t) = if now < rise { (true, rise) } else if now < set { (false, set) } else { (true, rise + 1440.0) };
    let arc = |ui: &mut Ui, r: Rect| {
        // The day's arc from sunrise to sunset, the sun on it.
        let rad = (r.w / 2.0 - 2.0).min(r.h - 6.0);
        let (cx, base) = (r.cx(), r.cy() + rad / 2.0);
        ui.line(r.x, base, r.r(), base, col::TEXT.with_a(0.3), 1.0);
        ui.arc(cx, base, rad, -FRAC_PI_2, PI, C_SUN.with_a(0.3), 1.4);
        let f = ((now - rise) / (set - rise)) as f32;
        if (0.0..=1.0).contains(&f) {
            ui.arc(cx, base, rad, -FRAC_PI_2, f * PI, C_SUN, 1.8);
            let a = -FRAC_PI_2 + f * PI;
            let (sx, sy) = (cx + rad * a.sin(), base - rad * a.cos());
            ui.circle(sx, sy, 5.0, C_SUN.with_a(0.3));
            ui.circle(sx, sy, 3.2, hex(0xFFE08A));
        } else {
            ui.circle(cx, base + 1.0, 3.0, C_SUN.with_a(0.5));
        }
    };
    let left = (set - now).max(0.0);
    let left_t = if now < rise || now > set { "Night".to_string() } else { format!("{} h {} min of light left", left as u32 / 60, left as u32 % 60) };
    let arrow = if next_up { "\u{2191}" } else { "\u{2193}" };
    match size {
        Size::S => {
            arc(ui, Rect::new(c.x + 6.0, c.y + 2.0, c.w - 12.0, 18.0));
            let t = hm(next_t);
            let t = t.trim_end_matches(" AM").trim_end_matches(" PM");
            ui.text(&format!("{arrow}{t}"), Rect::new(c.x, c.y + 20.0, c.w, 13.0), 9.5, col::TEXT, Weight::Bold, Align::Center);
        }
        Size::M => {
            let mut row = c.inset_xy(7.0, 0.0);
            arc(ui, row.cut_left(30.0, 7.0).centered(30.0, 22.0));
            ui.text(if next_up { "Sunrise" } else { "Sunset" }, Rect::new(row.x, c.cy() - 15.0, row.w, 13.0), 9.5, C_SUN, Weight::Bold, Align::Left);
            let t = hm(next_t);
            let z = fit_size(ui, &t, 13.0, row.w);
            ui.text(&t, Rect::new(row.x, c.cy() - 2.0, row.w, 17.0), z, col::TEXT, Weight::Bold, Align::Left);
        }
        Size::L => {
            let mut row = c.inset_xy(8.0, 0.0);
            arc(ui, row.cut_left(34.0, 8.0).centered(34.0, 24.0));
            two_lines(ui, row, &format!("{} {}", if next_up { "Sunrise" } else { "Sunset" }, hm(next_t)), 12.5, &left_t, col::TEXT);
        }
        Size::W => {
            let mut row = c.inset_xy(8.0, 0.0);
            arc(ui, row.cut_left(40.0, 10.0).centered(40.0, 26.0));
            let cols = row.columns(2, 6.0);
            for (k, (label, t)) in [("\u{2191} Sunrise", rise), ("\u{2193} Sunset", set)].into_iter().enumerate() {
                ui.text(label, Rect::new(cols[k].x, c.cy() - 15.0, cols[k].w, 13.0), 9.5, C_SUN, Weight::Bold, Align::Left);
                ui.text(&hm(t), Rect::new(cols[k].x, c.cy() - 2.0, cols[k].w, 17.0), 13.0, col::TEXT, Weight::Bold, Align::Left);
            }
        }
    }
}

/// The moon as it looks tonight: its dark face, the lit part on it.
pub fn moon_disc(ui: &mut Ui, cx: f32, cy: f32, r: f32, age: f64) {
    ui.circle(cx, cy, r + 2.0, C_MOON.with_a(0.12));
    ui.circle(cx, cy, r, hex(0x3A3550));
    let pts = crate::astro::moon_lit_shape(cx, cy, r, age);
    ui.polygon(&pts, hex(0xF2EEFF));
    // A few soft craters on the lit side.
    for (dx, dy, cr) in [(-0.3, -0.25, 0.18), (0.25, 0.2, 0.14), (0.05, -0.45, 0.1)] {
        let (x, y) = (cx + dx * r, cy + dy * r);
        ui.circle(x, y, cr * r, hex(0xB9B2D6).with_a(0.35));
    }
}

fn draw_moon_face(ui: &mut Ui, c: Rect, size: Size) {
    let age = crate::astro::moon_age(crate::time::unix_now());
    let lit = (crate::astro::moon_lit(age) * 100.0).round() as u32;
    let (full, _) = crate::astro::moon_next(age);
    match size {
        Size::S => moon_disc(ui, c.cx(), c.cy(), 11.0, age),
        Size::M => {
            let mut row = c.inset_xy(8.0, 0.0);
            let m = row.cut_left(24.0, 8.0);
            moon_disc(ui, m.cx(), m.cy(), 11.0, age);
            two_lines(ui, row, &format!("{lit}%"), 14.0, "lit tonight", col::TEXT);
        }
        _ => {
            let mut row = c.inset_xy(8.0, 0.0);
            let m = row.cut_left(28.0, 9.0);
            moon_disc(ui, m.cx(), m.cy(), 13.0, age);
            let when = if full < 1.0 { "Full moon tonight".to_string() } else { format!("Full moon in {} days", full.round() as u32) };
            two_lines(ui, row, crate::astro::moon_name(age), 12.5, &when, col::TEXT);
        }
    }
}

fn draw_quote(ui: &mut Ui, c: Rect, size: Size, info: &Info) {
    let (q, who) = QUOTES[info.quote % QUOTES.len()];
    let mut row = c.inset_xy(9.0, 0.0);
    ui.text("\u{201C}", row.cut_left(16.0, 4.0).inset_xy(0.0, -4.0), 26.0, C_QUOTE, Weight::Bold, Align::Left);
    match size {
        Size::M => two_lines(ui, row, "Today's quote", 11.5, who, col::TEXT),
        _ => {
            let z = if size == Size::W { 11.5 } else { 11.0 };
            ui.text(q, Rect::new(row.x, c.cy() - 15.5, row.w, 17.0), z, col::TEXT, Weight::Semi, Align::Left);
            ui.text(&format!("\u{2014} {who}"), Rect::new(row.x, c.cy() + 2.0, row.w, 13.0), 9.5, C_QUOTE, Weight::Semi, Align::Left);
        }
    }
}

/// A pair's rate: (base, quote, value, change since the day before %).
pub fn fx_pair(info: &Info) -> Option<(&'static str, &'static str, f64, Option<f64>)> {
    let (_, r, prev) = crate::modules::tools::rates_for_widget()?;
    let cur = crate::modules::tools::CURRENCIES;
    let (b, q) = info.fx;
    let v = r[q] / r[b];
    let ch = prev.map(|p| (v / (p[q] / p[b]) - 1.0) * 100.0);
    Some((cur[b], cur[q], v, ch))
}

/// "302.45", "0.92", "1,234".
pub fn money(v: f64) -> String {
    if v >= 1000.0 {
        let s = format!("{:.0}", v);
        let mut out = String::new();
        for (i, ch) in s.chars().enumerate() {
            if i > 0 && (s.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(ch);
        }
        out
    } else if v >= 10.0 {
        format!("{v:.2}")
    } else {
        format!("{v:.4}")
    }
}

fn draw_currency(ui: &mut Ui, c: Rect, size: Size, info: &Info) {
    let Some((b, q, v, ch)) = fx_pair(info) else {
        two_lines(ui, c.inset_xy(10.0, 0.0), "Currency", 13.0, "Getting today's rates…", col::TEXT);
        return;
    };
    let change = |ui: &mut Ui, r: Rect| {
        if let Some(ch) = ch {
            let (t, col_) = if ch.abs() < 0.005 { ("\u{2022} 0.00%".to_string(), col::DIM) } else if ch > 0.0 { (format!("\u{25B2} {ch:.2}%"), col::GREEN) } else { (format!("\u{25BC} {:.2}%", ch.abs()), col::RED) };
            ui.text(&t, r, 9.5, col_, Weight::Bold, Align::Left);
        }
    };
    match size {
        Size::S => {
            ui.text(q, Rect::new(c.x, c.y + 3.0, c.w, 13.0), 9.0, C_MONEY, Weight::Bold, Align::Center);
            let t = if v >= 100.0 { format!("{v:.0}") } else { format!("{v:.2}") };
            let z = fit_size(ui, &t, 11.0, c.w - 4.0);
            ui.text(&t, Rect::new(c.x, c.y + 16.0, c.w, 16.0), z, col::TEXT, Weight::Bold, Align::Center);
        }
        Size::M => {
            let row = c.inset_xy(9.0, 0.0);
            ui.text(&format!("{b} \u{2192} {q}"), Rect::new(row.x, c.cy() - 15.0, row.w, 13.0), 9.5, C_MONEY, Weight::Bold, Align::Left);
            let t = money(v);
            let z = fit_size(ui, &t, 14.0, row.w);
            ui.text(&t, Rect::new(row.x, c.cy() - 2.0, row.w, 17.0), z, col::TEXT, Weight::Bold, Align::Left);
        }
        _ => {
            let mut row = c.inset_xy(8.0, 0.0);
            let badge = row.cut_left(34.0, 8.0).centered(34.0, 24.0);
            ui.fill_rr_grad(badge, 7.0, C_MONEY, hex(0x0A84FF), false);
            ui.text(q, badge, 10.0, hex(0xFFFFFF), Weight::Bold, Align::Center);
            let t = money(v);
            ui.text(&t, Rect::new(row.x, c.cy() - 16.0, row.w, 18.0), 14.5, col::TEXT, Weight::Bold, Align::Left);
            let tw = ui.measure(&t, 14.5, Weight::Bold);
            let mut under = Rect::new(row.x, c.cy() + 2.0, row.w, 13.0);
            ui.text(&format!("per 1 {b}"), under.cut_left(54.0, 4.0), 9.5, col::DIM, Weight::Regular, Align::Left);
            change(ui, under);
            if size == Size::W {
                // Two more of today's rates against the same base.
                let cur = crate::modules::tools::CURRENCIES;
                if let Some((_, r, _)) = crate::modules::tools::rates_for_widget() {
                    let others: Vec<usize> = (0..cur.len()).filter(|&k| k != info.fx.0 && k != info.fx.1).take(2).collect();
                    let x0 = row.x + tw.max(70.0) + 12.0;
                    for (j, k) in others.into_iter().enumerate() {
                        let y = c.y + 4.0 + j as f32 * 15.0;
                        ui.text(&format!("{} {}", cur[k], money(r[k] / r[info.fx.0])), Rect::new(x0, y, row.r() - x0, 13.0), 9.5, col::DIM, Weight::Semi, Align::Right);
                    }
                }
            }
        }
    }
}

// ------------------------------------------------------------ value widgets

/// Value widgets with a design of their own (a picture of what they are,
/// not just a number). False: draw the general card.
fn value_face(ui: &mut Ui, c: Rect, w: &Widget, size: Size, tint: Color) -> bool {
    let num = |s: &str| s.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse::<u32>().unwrap_or(0);
    match w.id {
        "tasks" => {
            // A checklist: the count big, the boxes ticked off.
            let open = num(&w.value);
            let done = w.detail.split(" · ").nth(1).map(|s| num(s)).unwrap_or(0);
            let list = |ui: &mut Ui, r: Rect| {
                for k in 0..3 {
                    let y = r.y + k as f32 * (r.h / 3.0);
                    let b = Rect::new(r.x, y + 1.5, 7.0, 7.0);
                    let ticked = (k as u32) < done.min(3);
                    ui.stroke_rr(b.inset(0.5), 2.0, tint.with_a(0.8), 1.0);
                    if ticked {
                        ui.fill_rr(b, 2.0, tint);
                    }
                    ui.fill_rr(Rect::new(r.x + 10.0, y + 4.0, r.w - 10.0 - k as f32 * 4.0, 2.0), 1.0, col::TEXT.with_a(if ticked { 0.18 } else { 0.4 }));
                }
            };
            match size {
                Size::S => {
                    list(ui, Rect::new(c.x + 8.0, c.y + 4.0, c.w - 16.0, 14.0));
                    ui.text(&open.to_string(), Rect::new(c.x, c.y + 19.0, c.w, 13.0), 10.0, col::TEXT, Weight::Bold, Align::Center);
                }
                _ => {
                    let mut row = c.inset_xy(9.0, 0.0);
                    list(ui, row.cut_left(if size == Size::M { 22.0 } else { 30.0 }, 8.0).centered(if size == Size::M { 22.0 } else { 30.0 }, 22.0));
                    let big = if open == 0 { "All done".to_string() } else if open == 1 { "1 to do".to_string() } else { format!("{open} to do") };
                    let small = if size == Size::M { String::new() } else { format!("{done} done today") };
                    let z = fit_size(ui, &big, 14.0, row.w);
                    two_lines(ui, row, &big, z, &small, col::TEXT);
                    if size == Size::W && open + done > 0 {
                        let m = Rect::new(row.r() - 70.0, c.cy() - 2.5, 70.0, 5.0);
                        grad_bar(ui, m, done as f32 / (open + done) as f32, tint.with_a(0.7), tint);
                    }
                }
            }
            true
        }
        "water" => {
            // A glass that fills.
            let f = w.ring.unwrap_or(0.0).clamp(0.0, 1.0);
            let glass = |ui: &mut Ui, r: Rect| {
                ui.stroke_rr(r.inset(0.5), 3.5, tint.with_a(0.8), 1.2);
                let inner = r.inset(2.0);
                let h = inner.h * f;
                if h > 0.5 {
                    ui.fill_rr_grad(Rect::new(inner.x, inner.b() - h, inner.w, h), 2.0, hex(0x64D2FF), hex(0x0A84FF), true);
                }
            };
            match size {
                Size::S => {
                    glass(ui, Rect::new(c.cx() - 6.0, c.y + 3.0, 12.0, 16.0));
                    ui.text(&w.value, Rect::new(c.x, c.y + 20.0, c.w, 13.0), 9.5, col::TEXT, Weight::Bold, Align::Center);
                }
                _ => {
                    let mut row = c.inset_xy(9.0, 0.0);
                    glass(ui, row.cut_left(16.0, 9.0).centered(16.0, 24.0));
                    let small = if size == Size::M { String::new() } else { w.detail.clone() };
                    two_lines(ui, row, &w.value, 14.0, &small, col::TEXT);
                    if size == Size::W {
                        segments(ui, Rect::new(row.r() - 72.0, c.cy() - 4.0, 72.0, 8.0), 8, f, 2.0, |_| hex(0x64D2FF));
                    }
                }
            }
            true
        }
        "next_prayer" | "countdown" | "focus" | "screentime" | "uptime" | "disk" | "bluetooth" | "mic" => {
            let f = w.ring;
            let big = match size {
                Size::S => w.short.clone(),
                _ => w.value.clone(),
            };
            let small = match w.id {
                "next_prayer" => w.label.clone(),
                "countdown" => w.label.clone(),
                "focus" => if num(&w.value) == 1 { "session today".into() } else { "sessions today".into() },
                "screentime" => "at the PC today".into(),
                "uptime" => "since Windows started".into(),
                "disk" => format!("free on {}", w.label),
                "bluetooth" => w.label.clone(),
                _ => w.detail.clone(),
            };
            match size {
                Size::S => {
                    match f {
                        Some(f) => ring_w(ui, c.cx(), c.y + 11.0, 8.0, f, tint, 2.2),
                        None => {}
                    }
                    ui.icon(&w.icon, Rect::new(c.cx() - 8.0, c.y + 3.0, 16.0, 16.0), 10.0, tint);
                    let z = fit_size(ui, &big, 10.0, c.w - 4.0);
                    ui.text(&big, Rect::new(c.x, c.y + 20.0, c.w, 13.0), z, col::TEXT, Weight::Bold, Align::Center);
                }
                _ => {
                    let mut row = c.inset_xy(8.0, 0.0);
                    // A tile in the widget's colour with its symbol.
                    let tile = row.cut_left(26.0, 8.0).centered(26.0, 26.0);
                    ui.fill_rr_grad(tile, 8.0, tint, tint.lerp(hex(0x000000), 0.35), true);
                    ui.icon(&w.icon, tile, 13.0, hex(0xFFFFFF));
                    if let Some(f) = f {
                        ring_w(ui, tile.cx(), tile.cy(), 15.5, f, tint, 1.6);
                    }
                    let small = if size == Size::M { String::new() } else { small };
                    let z = fit_size(ui, &big, 14.0, row.w);
                    two_lines(ui, row, &big, z, &small, col::TEXT);
                    if size == Size::W {
                        if let Some(f) = f {
                            grad_bar(ui, Rect::new(row.r() - 64.0, c.cy() - 2.5, 64.0, 5.0), f, tint.with_a(0.6), tint);
                        }
                    }
                }
            }
            true
        }
        _ => false,
    }
}

// ------------------------------------------------------------ the right end

/// The keyboard language, like Windows: "ENG" over "US".
fn sys_lang(ui: &mut Ui, r: Rect, info: &Info) {
    let (code, region, _) = &info.lang;
    let code = if code.len() == 2 { match code.as_str() { "EN" => "ENG".to_string(), c => c.to_string() } } else { code.clone() };
    ui.text(&code, Rect::new(r.x, r.cy() - 13.0, r.w, 14.0), 10.5, col::TEXT, Weight::Semi, Align::Center);
    ui.text(region, Rect::new(r.x, r.cy() + 0.5, r.w, 13.0), 9.5, col::DIM, Weight::Regular, Align::Center);
}

/// Wi-Fi, sound and battery side by side, like Windows' quick settings
/// button.
fn sys_quick(ui: &mut Ui, r: Rect, info: &Info) {
    let list = &info.quick;
    if list.is_empty() {
        ui.icon(glyph::WIFI, r, 14.0, col::TEXT);
        return;
    }
    let w = 21.0;
    let total = w * list.len() as f32;
    let mut x = r.cx() - total / 2.0;
    for item in list {
        let cell = Rect::new(x, r.y, w, r.h);
        if item.id == "battery" {
            let f = item.ring.unwrap_or(0.0);
            let tint = if item.color == col::DIM { col::TEXT } else { item.color };
            battery_shape(ui, Rect::new(cell.cx() - 9.0, cell.cy() - 4.5, 18.0, 9.0), f, tint, item.label == "Charging");
        } else {
            ui.icon(&item.icon, cell, 14.0, if item.quiet { col::DIM } else { col::TEXT });
        }
        x += w;
    }
}

/// The time over the date, like Windows.
fn sys_clock(ui: &mut Ui, r: Rect, n: &Now) {
    let time = crate::time::clock_full(n.hour as u32, n.min as u32);
    ui.text(&time, Rect::new(r.x, r.cy() - 14.0, r.w, 15.0), 11.5, col::TEXT, Weight::Semi, Align::Center);
    let date = format!("{}/{}/{}", n.day, n.month, n.year);
    ui.text(&date, Rect::new(r.x, r.cy() + 1.0, r.w, 13.0), 10.0, col::TEXT.with_a(0.82), Weight::Regular, Align::Center);
}

pub fn size_text(b: u64) -> String {
    let mb = b as f64 / 1048576.0;
    if mb >= 1024.0 { format!("{:.1} GB", mb / 1024.0) } else if mb >= 10.0 { format!("{mb:.0} MB") } else { format!("{mb:.1} MB") }
}

// ------------------------------------------------------------ readings

/// The keyboard language of the window you type in: ("EN", "US", "English").
fn keyboard_language() -> (String, String, String) {
    use windows::Win32::Globalization::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayout;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    unsafe {
        let tid = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let lang = (GetKeyboardLayout(tid).0 as usize & 0xFFFF) as u32;
        let mut name = [0u16; 85];
        let n = LCIDToLocaleName(lang, Some(&mut name), 0);
        if n <= 1 {
            return ("??".into(), String::new(), "Keyboard".into());
        }
        let locale = windows::core::PCWSTR(name.as_ptr());
        let info = |t: u32| -> String {
            let mut b = [0u16; 128];
            let k = GetLocaleInfoEx(locale, t, Some(&mut b));
            String::from_utf16_lossy(&b[..(k.max(1) - 1) as usize])
        };
        let code = info(LOCALE_SISO639LANGNAME).to_uppercase();
        let region = info(LOCALE_SISO3166CTRYNAME);
        let full = info(LOCALE_SENGLISHLANGUAGENAME);
        (code, region, full)
    }
}

/// Windows' apps are in dark mode.
fn dark_mode() -> bool {
    use windows::Win32::System::Registry::*;
    let mut v: u32 = 1;
    let mut n = 4u32;
    let r = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, windows::core::w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"), windows::core::w!("AppsUseLightTheme"), RRF_RT_REG_DWORD, None, Some(&mut v as *mut u32 as _), Some(&mut n))
    };
    r.is_ok() && v == 0
}

/// Items in the Recycle Bin (all drives) and their size.
fn recycle_bin() -> (u64, u64) {
    use windows::Win32::UI::Shell::{SHQUERYRBINFO, SHQueryRecycleBinW};
    let mut i = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, ..Default::default() };
    if unsafe { SHQueryRecycleBinW(windows::core::PCWSTR::null(), &mut i) }.is_ok() { (i.i64NumItems.max(0) as u64, i.i64Size.max(0) as u64) } else { (0, 0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(year: u16, month: u16, day: u16, dow: u16) -> Now {
        Now { year, month, day, dow, hour: 12, min: 0, sec: 0, unix: 0.0 }
    }

    #[test]
    fn iso_weeks() {
        assert_eq!(week(&at(2026, 11, 18, 3)), 47);
        assert_eq!(week(&at(2026, 1, 1, 4)), 1);
        assert_eq!(week(&at(2027, 1, 1, 5)), 53); // Fri 1 Jan 2027 is in 2026's week 53
        assert_eq!(week(&at(2024, 12, 30, 1)), 1);
    }

    #[test]
    fn progress() {
        let n = at(2026, 1, 1, 4);
        assert!((day_fraction(&n) - 0.5).abs() < 1e-6);
        assert!(year_fraction(&n) < 0.01);
        assert_eq!(day_of_year(&at(2026, 12, 31, 4)), 365);
    }
}
