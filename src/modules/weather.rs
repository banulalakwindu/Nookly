//! Weather: now, the hours around it (earlier today greyed) and the days
//! from yesterday to six ahead, for your place (Widgets > Place). From
//! Open-Meteo, only while this panel is on: when you open it, then at most
//! every 30 minutes. Icons are drawn, one per kind of weather.

use std::sync::{Arc, Mutex};

use crate::gfx::*;
use crate::modules::{Ctx, Module, Rel};
use crate::time::unix_now;
use crate::ui::Ui;

const A_REFRESH: u32 = 1;
const C_SKY: Color = Color { r: 0.45, g: 0.75, b: 1.0, a: 1.0 };
const SUN: Color = Color { r: 1.0, g: 0.78, b: 0.25, a: 1.0 };
const CLOUD: Color = Color { r: 0.82, g: 0.85, b: 0.92, a: 1.0 };
const DARK_CLOUD: Color = Color { r: 0.58, g: 0.62, b: 0.70, a: 1.0 };
const RAIN: Color = Color { r: 0.40, g: 0.70, b: 1.0, a: 1.0 };
const BOLT: Color = Color { r: 1.0, g: 0.85, b: 0.20, a: 1.0 };

#[derive(Clone, Debug, Default)]
pub struct Forecast {
    pub place: String,
    pub temp: f32,
    pub feels: f32,
    pub humidity: f32,
    pub wind: f32,
    pub code: u32,
    pub day: bool,
    /// (unix time, temperature, code, rain chance %).
    pub hours: Vec<(f64, f32, u32, f32)>,
    /// (unix time of the day, max, min, code, rain chance %).
    pub days: Vec<(f64, f32, f32, u32, f32)>,
    pub sunrise: String,
    pub sunset: String,
    pub fahrenheit: bool,
    pub at: f64,
}

/// What a WMO weather code means.
pub fn describe(code: u32) -> &'static str {
    match code {
        0 => "Clear",
        1 => "Mostly clear",
        2 => "Partly cloudy",
        3 => "Cloudy",
        45 | 48 => "Fog",
        51..=57 => "Drizzle",
        61 | 63 | 80 | 81 => "Rain",
        65 | 82 => "Heavy rain",
        66 | 67 => "Freezing rain",
        71..=77 | 85 | 86 => "Snow",
        95..=99 => "Thunderstorm",
        _ => "Weather",
    }
}

/// Draw the icon for a weather code into `r` (sun or moon, clouds, rain,
/// snow, a bolt, fog).
pub fn draw_icon(ui: &mut Ui, r: Rect, code: u32, day: bool) {
    let s = r.w.min(r.h);
    let (cx, cy) = (r.cx(), r.cy());
    let sky = |ui: &mut Ui, x: f32, y: f32, k: f32| {
        if day {
            ui.circle(x, y, s * 0.2 * k, SUN);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                let (r0, r1) = (s * 0.28 * k, s * 0.37 * k);
                let p0 = (x + r0 * a.cos(), y + r0 * a.sin());
                let p1 = (x + r1 * a.cos(), y + r1 * a.sin());
                ui.line(p0.0, p0.1, p1.0, p1.1, SUN, (s * 0.05).max(1.0));
            }
        } else {
            ui.circle(x, y, s * 0.22 * k, hex(0xE8E6FF));
            ui.circle(x + s * 0.1 * k, y - s * 0.07 * k, s * 0.19 * k, col::NOTCH);
        }
    };
    let cloud = |ui: &mut Ui, x: f32, y: f32, k: f32, c: Color| {
        ui.circle(x - s * 0.14 * k, y + s * 0.03 * k, s * 0.14 * k, c);
        ui.circle(x + s * 0.02 * k, y - s * 0.06 * k, s * 0.19 * k, c);
        ui.circle(x + s * 0.18 * k, y + s * 0.05 * k, s * 0.12 * k, c);
        ui.fill_rr(Rect::new(x - s * 0.28 * k, y + s * 0.02 * k, s * 0.58 * k, s * 0.15 * k), s * 0.07 * k, c);
    };
    let wet = matches!(code, 51..=67 | 80..=82 | 95..=99);
    let snowy = matches!(code, 71..=77 | 85 | 86);
    match code {
        0 => sky(ui, cx, cy, 1.15),
        1 | 2 => {
            sky(ui, cx - s * 0.12, cy - s * 0.1, 0.85);
            cloud(ui, cx + s * 0.06, cy + s * 0.1, 0.85, CLOUD);
        }
        45 | 48 => {
            cloud(ui, cx, cy - s * 0.1, 0.9, DARK_CLOUD);
            for i in 0..3 {
                let y = cy + s * (0.12 + i as f32 * 0.1);
                ui.fill_rr(Rect::new(cx - s * 0.3 + i as f32 * s * 0.05, y, s * 0.55, s * 0.05), s * 0.025, CLOUD.with_a(0.8));
            }
        }
        _ => {
            let c = if wet || snowy || code == 3 { if matches!(code, 95..=99 | 65 | 82) { DARK_CLOUD } else { CLOUD } } else { CLOUD };
            cloud(ui, cx, cy - s * 0.08, 1.0, c);
            if matches!(code, 95..=99) {
                let b = [(0.04, 0.08), (-0.06, 0.24), (0.02, 0.24), (-0.08, 0.42)];
                for w in b.windows(2) {
                    ui.line(cx + s * w[0].0, cy + s * w[0].1, cx + s * w[1].0, cy + s * w[1].1, BOLT, (s * 0.06).max(1.2));
                }
            } else if wet {
                for i in 0..3 {
                    let x = cx - s * 0.16 + i as f32 * s * 0.16;
                    ui.line(x, cy + s * 0.18, x - s * 0.05, cy + s * 0.34, RAIN, (s * 0.05).max(1.0));
                }
            } else if snowy {
                for i in 0..3 {
                    ui.circle(cx - s * 0.16 + i as f32 * s * 0.16, cy + s * 0.27 + (i % 2) as f32 * s * 0.06, s * 0.04, hex(0xFFFFFF));
                }
            }
        }
    }
}

/// The forecast request: now, hourly from yesterday, and 7 days.
pub fn path(lat: f64, lon: f64, f: bool) -> String {
    format!(
        "/v1/forecast?latitude={lat:.2}&longitude={lon:.2}&current=temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,wind_speed_10m,is_day&hourly=temperature_2m,weather_code,precipitation_probability&daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,sunrise,sunset&timezone=auto&timeformat=unixtime&past_days=1&forecast_days=7{}",
        if f { "&temperature_unit=fahrenheit&wind_speed_unit=mph" } else { "" }
    )
}

pub fn parse(body: &str, place: &str, f: bool) -> Option<Forecast> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let c = v.get("current")?;
    let num = |x: &serde_json::Value, k: &str| x.get(k).and_then(|n| n.as_f64()).unwrap_or(0.0) as f32;
    let arr = |x: &serde_json::Value, k: &str| -> Vec<f64> { x.get(k).and_then(|a| a.as_array()).map(|a| a.iter().map(|n| n.as_f64().unwrap_or(0.0)).collect()).unwrap_or_default() };
    let h = v.get("hourly")?;
    let (ht, htemp, hcode, hrain) = (arr(h, "time"), arr(h, "temperature_2m"), arr(h, "weather_code"), arr(h, "precipitation_probability"));
    let hours = ht.iter().enumerate().map(|(i, t)| (*t, *htemp.get(i).unwrap_or(&0.0) as f32, *hcode.get(i).unwrap_or(&0.0) as u32, *hrain.get(i).unwrap_or(&0.0) as f32)).collect();
    let d = v.get("daily")?;
    let (dt, dmax, dmin, dcode, drain) = (arr(d, "time"), arr(d, "temperature_2m_max"), arr(d, "temperature_2m_min"), arr(d, "weather_code"), arr(d, "precipitation_probability_max"));
    let days = dt.iter().enumerate().map(|(i, t)| (*t, *dmax.get(i).unwrap_or(&0.0) as f32, *dmin.get(i).unwrap_or(&0.0) as f32, *dcode.get(i).unwrap_or(&0.0) as u32, *drain.get(i).unwrap_or(&0.0) as f32)).collect();
    let clock = |k: &str| {
        arr(d, k).get(1).map_or(String::new(), |&t| {
            let (_, _, _, m) = crate::time::local_time(t);
            crate::time::fmt_mod(m)
        })
    };
    Some(Forecast {
        place: place.to_string(),
        temp: num(c, "temperature_2m"),
        feels: num(c, "apparent_temperature"),
        humidity: num(c, "relative_humidity_2m"),
        wind: num(c, "wind_speed_10m"),
        code: c.get("weather_code").and_then(|n| n.as_u64()).unwrap_or(0) as u32,
        day: c.get("is_day").and_then(|n| n.as_u64()).unwrap_or(1) == 1,
        hours,
        days,
        sunrise: clock("sunrise"),
        sunset: clock("sunset"),
        fahrenheit: f,
        at: unix_now(),
    })
}

pub struct Weather {
    data: Arc<Mutex<(Option<Forecast>, bool, Option<String>)>>,
    seen_at: f64,
    asked: f64,
}

impl Weather {
    pub fn new() -> Weather {
        let w = Weather { data: Arc::new(Mutex::new((None, false, None))), seen_at: 0.0, asked: 0.0 };
        if crate::time::demo() {
            w.data.lock().unwrap().0 = Some(demo());
        }
        w
    }
    fn fetch(&mut self, f: bool) {
        let place = crate::modules::widgets::PLACE.lock().unwrap().clone().or_else(|| {
            let (lat, lon, name) = crate::modules::prayer::location();
            Some((lat, lon, name))
        });
        let Some((lat, lon, name)) = place else { return };
        {
            let mut d = self.data.lock().unwrap();
            if d.1 {
                return;
            }
            d.1 = true;
        }
        self.asked = unix_now();
        let data = self.data.clone();
        std::thread::spawn(move || {
            let got = crate::http::get("api.open-meteo.com", &path(lat, lon, f)).filter(|r| r.status == 200).and_then(|r| parse(&String::from_utf8_lossy(&r.body), &name, f));
            let mut d = data.lock().unwrap();
            d.1 = false;
            match got {
                Some(fc) => {
                    d.0 = Some(fc);
                    d.2 = None;
                }
                None => d.2 = Some("Couldn't get the weather; it tries again soon".into()),
            }
            drop(d);
            crate::app::wake();
        });
    }
}

fn demo() -> Forecast {
    let t = unix_now();
    let now = t - (crate::time::local_time(t).3 % 60) as f64 * 60.0;
    let codes = [0, 1, 2, 2, 3, 61, 80, 95, 3, 2, 1, 0, 0, 1, 2, 3, 61, 63, 2, 1, 0, 0, 1, 2];
    let hours = (0..48).map(|i| (now + (i as f64 - 24.0) * 3600.0, 26.0 + 4.0 * ((i as f32) * 0.26).sin(), codes[i % codes.len()], (i % 7 * 10) as f32)).collect();
    let day0 = now - 86400.0;
    let dc = [2, 1, 61, 95, 3, 0, 1, 2];
    let days = (0..8).map(|i| (day0 + i as f64 * 86400.0, 31.0 - (i % 3) as f32, 24.0 + (i % 2) as f32, dc[i], (i * 12 % 90) as f32)).collect();
    Forecast { place: "Colombo".into(), temp: 29.0, feels: 33.0, humidity: 74.0, wind: 14.0, code: 2, day: true, hours, days, sunrise: "6:02 AM".into(), sunset: "5:51 PM".into(), fahrenheit: false, at: unix_now() }
}

impl Module for Weather {
    fn title(&self) -> &'static str {
        "Weather"
    }
    fn icon(&self) -> &'static str {
        "\u{E753}"
    }
    fn accent(&self) -> Color {
        C_SKY
    }
    fn size(&self) -> (f32, f32) {
        (580.0, 186.0)
    }
    fn slim_min(&self) -> (f32, f32) {
        (580.0, 186.0)
    }
    fn tick(&mut self, ctx: &mut Ctx) {
        if crate::time::demo() {
            return;
        }
        // Only while you look at it: on opening, then every 30 minutes.
        let now = unix_now();
        let f = ctx.settings.place.fahrenheit;
        let (fresh, unit) = self.data.lock().unwrap().0.as_ref().map_or((0.0, f), |d| (d.at, d.fahrenheit));
        if now - self.seen_at < 3.0 && (now - fresh > 1800.0 || unit != f) && now - self.asked > 60.0 {
            self.fetch(f);
        }
    }
    fn draw(&mut self, ui: &mut Ui, r: Rect) {
        self.seen_at = unix_now();
        let (data, busy, err) = self.data.lock().unwrap().clone();
        let Some(w) = data else {
            let msg = err.unwrap_or_else(|| if busy { "Getting the weather...".into() } else { "Opening: getting the weather for your place (Widgets > Place)".into() });
            ui.text(&msg, r, 12.0, col::DIM, Weight::Regular, Align::Center);
            return;
        };
        let unit = if w.fahrenheit { "°F" } else { "°C" };
        let mut r = r;
        let mut top = r.cut_top(96.0, 12.0);
        // Now.
        let mut now_r = top.cut_left(196.0, 14.0);
        let ic = now_r.cut_left(64.0, 8.0);
        draw_icon(ui, ic.centered(64.0, 64.0), w.code, w.day);
        ui.text(&format!("{:.0}{unit}", w.temp), now_r.cut_top(34.0, 0.0), 28.0, col::TEXT, Weight::Semi, Align::Left);
        ui.text(describe(w.code), now_r.cut_top(16.0, 0.0), 12.0, col::TEXT, Weight::Semi, Align::Left);
        ui.text(&format!("Feels {:.0}° · {:.0}% · {:.0} {}", w.feels, w.humidity, w.wind, if w.fahrenheit { "mph" } else { "km/h" }), now_r.cut_top(15.0, 0.0), 10.5, col::DIM, Weight::Regular, Align::Left);
        // A place picked by coordinates has no name worth showing.
        let named = w.place.chars().any(|c| c.is_alphabetic());
        ui.text(if named { &w.place } else { "Your location" }, now_r, 10.0, col::FAINT, Weight::Regular, Align::Left);
        // The hours: 3 before now, 8 after.
        let now = unix_now();
        let cur = w.hours.iter().rposition(|h| h.0 <= now).unwrap_or(0);
        let from = cur.saturating_sub(3);
        let slots: Vec<_> = w.hours.iter().enumerate().skip(from).take(10).collect();
        for (cell, (i, h)) in top.columns(slots.len().max(1), 2.0).into_iter().zip(slots) {
            let past = i < cur;
            let is_now = i == cur;
            if is_now {
                ui.fill_rr(cell, 10.0, C_SKY.with_a(0.16));
            }
            let mut c = cell.inset_xy(0.0, 4.0);
            let (_, _, _, m) = crate::time::local_time(h.0);
            let label = if is_now {
                "Now".to_string()
            } else if crate::time::h12() {
                let h = m / 60;
                format!("{}{}", if h % 12 == 0 { 12 } else { h % 12 }, if h < 12 { "a" } else { "p" })
            } else {
                format!("{:02}", m / 60)
            };
            ui.text(&label, c.cut_top(14.0, 2.0), 9.5, if past { col::FAINT } else { col::DIM }, Weight::Semi, Align::Center);
            let icr = c.cut_top(28.0, 2.0);
            ui.g.alpha = if past { 0.45 } else { 1.0 };
            let hour = (m / 60) as i32;
            draw_icon(ui, icr.centered(26.0, 26.0), h.2, (6..18).contains(&hour));
            ui.g.alpha = 1.0;
            ui.text(&format!("{:.0}°", h.1), c.cut_top(15.0, 0.0), 11.5, if past { col::FAINT } else { col::TEXT }, Weight::Semi, Align::Center);
            if h.3 >= 30.0 && !past {
                ui.text(&format!("{:.0}%", h.3), c, 8.5, RAIN, Weight::Semi, Align::Center);
            }
        }
        // The days: yesterday, today and five more.
        let today = crate::time::local_date(now);
        for (cell, d) in r.columns(w.days.len().min(7).max(1), 6.0).into_iter().zip(w.days.iter()) {
            let date = crate::time::local_date(d.0 + 43200.0);
            let rel = crate::modules::countdown::day_number(date.0, date.1, date.2) - crate::modules::countdown::day_number(today.0, today.1, today.2);
            let past = rel < 0;
            let name = match rel {
                -1 => "Yesterday".to_string(),
                0 => "Today".to_string(),
                _ => crate::time::DAYS[crate::time::weekday(date.0, date.1, date.2) as usize][..3].to_string(),
            };
            ui.fill_rr(cell, 10.0, if rel == 0 { C_SKY.with_a(0.16) } else { col::BTN });
            // The day, its icon, then high / low, stacked so nothing is cut.
            let mut c = cell.inset_xy(4.0, 6.0);
            ui.text(&name, c.cut_top(14.0, 2.0), 10.0, if past { col::FAINT } else { col::DIM }, Weight::Semi, Align::Center);
            let temps = c.cut_bottom(16.0, 2.0);
            ui.g.alpha = if past { 0.5 } else { 1.0 };
            draw_icon(ui, c.centered(32.0, 32.0), d.3, true);
            ui.g.alpha = 1.0;
            let hi = format!("{:.0}°", d.1);
            let lo = format!(" {:.0}°", d.2);
            let (wh, wl) = (ui.measure(&hi, 11.5, Weight::Semi), ui.measure(&lo, 10.5, Weight::Regular));
            let x0 = temps.cx() - (wh + wl) / 2.0;
            ui.text(&hi, Rect::new(x0, temps.y, wh + 1.0, temps.h), 11.5, if past { col::FAINT } else { col::TEXT }, Weight::Semi, Align::Left);
            ui.text(&lo, Rect::new(x0 + wh, temps.y, wl + 2.0, temps.h), 10.5, col::FAINT, Weight::Regular, Align::Left);
        }
        let _ = busy;
    }
    fn action(&mut self, id: u32, _rel: Rel, ctx: &mut Ctx) {
        if id == A_REFRESH {
            self.fetch(ctx.settings.place.fahrenheit);
        }
    }
    fn tip(&self, id: u32) -> &'static str {
        if id == A_REFRESH {
            "Get the weather now"
        } else {
            ""
        }
    }
    fn debug_state(&self) -> String {
        format!("weather has={}", self.data.lock().unwrap().0.is_some())
    }
}
