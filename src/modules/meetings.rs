//! Next meeting: reads your calendar's private iCal link (Google Calendar,
//! Outlook...) about every 15 minutes while the feature is on, shows
//! "Standup in 8 min" on the closed notch and a Join button for Teams,
//! Zoom and Meet links. Events also show in the Calendar tab.

use std::sync::Mutex;

use crate::gfx::*;
use crate::modules::{live_simple, live_simple_side, Ctx, Module, Rel, Toast};
use crate::time::unix_now;
use crate::ui::Ui;

const A_JOIN: u32 = 1;
const C_MEET: Color = Color { r: 0.55, g: 0.55, b: 1.0, a: 1.0 };
/// How long before a meeting it shows on the notch.
const SOON: f64 = 15.0 * 60.0;
const REFRESH: f64 = 15.0 * 60.0;
pub const URL_KEY: &str = "ics_url";

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub start: f64,
    pub end: f64,
    pub title: String,
    pub link: Option<String>,
    pub all_day: bool,
}

static EVENTS: Mutex<Vec<Event>> = Mutex::new(Vec::new());
/// (fetching, last result: Ok(event count) or Err(why)).
static STATUS: Mutex<(bool, Option<Result<usize, String>>)> = Mutex::new((false, None));

pub fn status() -> (bool, Option<Result<usize, String>>) {
    STATUS.lock().unwrap().clone()
}

/// Events starting on a local day, earliest first.
pub fn events_on(y: i32, m: u32, d: u32) -> Vec<Event> {
    let start = local_to_unix(y, m, d, 0, 0, 0);
    let end = start + 86400.0;
    let mut v: Vec<Event> = EVENTS.lock().unwrap().iter().filter(|e| e.start < end && e.end.max(e.start + 1.0) > start).cloned().collect();
    v.sort_by(|a, b| a.start.total_cmp(&b.start));
    v
}

/// The next timed event that hasn't ended, if it starts within `within` s.
pub fn next_event(now: f64, within: f64) -> Option<Event> {
    EVENTS.lock().unwrap().iter().filter(|e| !e.all_day && e.end > now && e.start - now <= within).min_by(|a, b| a.start.total_cmp(&b.start)).cloned()
}

/// Local wall time to unix seconds (Windows' own time-zone rules).
pub fn local_to_unix(y: i32, m: u32, d: u32, h: u32, mi: u32, s: u32) -> f64 {
    use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows::Win32::System::Time::{SystemTimeToFileTime, TzSpecificLocalTimeToSystemTime};
    let local = SYSTEMTIME { wYear: y as u16, wMonth: m as u16, wDay: d as u16, wHour: h as u16, wMinute: mi as u16, wSecond: s as u16, ..Default::default() };
    let mut utc = SYSTEMTIME::default();
    let mut ft = FILETIME::default();
    unsafe {
        if TzSpecificLocalTimeToSystemTime(None, &local, &mut utc).is_err() || SystemTimeToFileTime(&utc, &mut ft).is_err() {
            return utc_to_unix(y, m, d, h, mi, s);
        }
    }
    crate::time::filetime_to_unix(((ft.dwHighDateTime as i64) << 32) | ft.dwLowDateTime as i64)
}

fn utc_to_unix(y: i32, m: u32, d: u32, h: u32, mi: u32, s: u32) -> f64 {
    crate::modules::countdown::day_number(y, m, d) as f64 * 86400.0 + (h * 3600 + mi * 60 + s) as f64
}

/// DTSTART value: (unix, all day). "20261112T090000Z" is UTC, without the
/// Z it's local wall time (a TZID other than yours is read as yours),
/// "20261112" is a whole day.
fn parse_time(v: &str) -> Option<(f64, bool)> {
    let v = v.trim();
    let n = |a: usize, b: usize| v.get(a..b)?.parse::<u32>().ok();
    let (y, m, d) = (n(0, 4)? as i32, n(4, 6)?, n(6, 8)?);
    if v.len() == 8 {
        return Some((local_to_unix(y, m, d, 0, 0, 0), true));
    }
    let (h, mi, s) = (n(9, 11)?, n(11, 13)?, n(13, 15).unwrap_or(0));
    Some((if v.ends_with('Z') { utc_to_unix(y, m, d, h, mi, s) } else { local_to_unix(y, m, d, h, mi, s) }, false))
}

/// A Teams, Zoom, Meet or Webex link in the text, if any.
pub fn find_link(text: &str) -> Option<String> {
    let hosts = ["teams.microsoft.com", "teams.live.com", "zoom.us", "meet.google.com", "webex.com", "whereby.com", "meet.jit.si"];
    for word in text.split(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '(' | ')' | ',')) {
        let w = word.trim_end_matches(|c: char| matches!(c, '.' | ';' | ']'));
        if w.starts_with("https://") && hosts.iter().any(|h| w.contains(h)) {
            return Some(w.to_string());
        }
    }
    None
}

fn unescape(s: &str) -> String {
    s.replace("\\n", "\n").replace("\\N", "\n").replace("\\,", ",").replace("\\;", ";").replace("\\\\", "\\")
}

/// Every event (repeats expanded) from `from` to `to`.
pub fn parse_ics(text: &str, from: f64, to: f64) -> Vec<Event> {
    // Unfold: a line starting with a space continues the one before.
    let mut lines: Vec<String> = Vec::new();
    for l in text.lines() {
        if (l.starts_with(' ') || l.starts_with('\t')) && !lines.is_empty() {
            lines.last_mut().unwrap().push_str(&l[1..]);
        } else {
            lines.push(l.to_string());
        }
    }
    let mut out = Vec::new();
    let mut ev: Option<Vec<(String, String, String)>> = None;
    for l in lines {
        if l == "BEGIN:VEVENT" {
            ev = Some(Vec::new());
            continue;
        }
        if l == "END:VEVENT" {
            if let Some(props) = ev.take() {
                expand(&props, from, to, &mut out);
            }
            continue;
        }
        if let Some(props) = ev.as_mut() {
            if let Some((head, value)) = l.split_once(':') {
                let (name, params) = head.split_once(';').unwrap_or((head, ""));
                props.push((name.to_uppercase(), params.to_string(), value.to_string()));
            }
        }
    }
    out.sort_by(|a, b| a.start.total_cmp(&b.start));
    out
}

fn expand(props: &[(String, String, String)], from: f64, to: f64, out: &mut Vec<Event>) {
    let get = |n: &str| props.iter().find(|p| p.0 == n).map(|p| p.2.clone());
    if get("STATUS").map_or(false, |s| s.eq_ignore_ascii_case("CANCELLED")) {
        return;
    }
    // Overridden occurrences come as their own events with RECURRENCE-ID.
    let Some((start, all_day)) = get("DTSTART").and_then(|v| parse_time(&v)) else { return };
    let end = get("DTEND").and_then(|v| parse_time(&v)).map(|x| x.0).unwrap_or(start + if all_day { 86400.0 } else { 1800.0 });
    let dur = (end - start).max(0.0);
    let title = get("SUMMARY").map(|s| unescape(&s)).unwrap_or_else(|| "Event".into());
    let text = format!("{} {} {}", get("LOCATION").unwrap_or_default(), get("URL").unwrap_or_default(), get("DESCRIPTION").map(|s| unescape(&s)).unwrap_or_default());
    let link = find_link(&text);
    let exdates: Vec<f64> = props.iter().filter(|p| p.0 == "EXDATE").flat_map(|p| p.2.split(',').filter_map(|v| parse_time(v).map(|x| x.0)).collect::<Vec<_>>()).collect();
    let mut push = |s: f64| {
        if s < to && s + dur > from && !exdates.iter().any(|x| (x - s).abs() < 60.0) {
            out.push(Event { start: s, end: s + dur, title: title.clone(), link: link.clone(), all_day });
        }
    };
    let Some(rule) = get("RRULE") else {
        push(start);
        return;
    };
    let r: std::collections::HashMap<String, String> = rule.split(';').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.to_uppercase(), v.to_uppercase())).collect();
    let interval = r.get("INTERVAL").and_then(|v| v.parse::<i64>().ok()).unwrap_or(1).max(1);
    let count = r.get("COUNT").and_then(|v| v.parse::<usize>().ok());
    let until = r.get("UNTIL").and_then(|v| parse_time(v)).map(|x| x.0 + if x.1 { 86399.0 } else { 0.0 });
    let freq = r.get("FREQ").cloned().unwrap_or_default();
    let byday: Vec<u32> = r
        .get("BYDAY")
        .map(|v| v.split(',').filter_map(|d| ["MO", "TU", "WE", "TH", "FR", "SA", "SU"].iter().position(|x| d.ends_with(x)).map(|k| k as u32)).collect())
        .unwrap_or_default();
    // Work on local days so the wall time stays put across DST.
    let (y0, m0, d0, h0, mi0) = {
        let (y, m, d, mins) = crate::time::local_time(start);
        (y, m, d, mins / 60, mins % 60)
    };
    let day0 = crate::modules::countdown::day_number(y0, m0, d0);
    let at = |day: i64| {
        let (y, m, d) = civil(day);
        local_to_unix(y, m, d, h0, mi0, 0)
    };
    let mut n = 0usize;
    let limit = |s: f64, n: usize| until.map_or(false, |u| s > u) || count.map_or(false, |c| n >= c) || s > to;
    match freq.as_str() {
        "DAILY" => {
            let mut day = day0;
            loop {
                let s = at(day);
                if limit(s, n) || n > 3000 {
                    break;
                }
                push(s);
                n += 1;
                day += interval;
            }
        }
        "WEEKLY" => {
            // Monday-based weekday of the first day: 0 = Monday.
            let wd0 = ((day0 + 3).rem_euclid(7)) as u32;
            let days = if byday.is_empty() { vec![wd0] } else { byday.clone() };
            let week0 = day0 - wd0 as i64;
            let mut week = week0;
            'w: loop {
                for &wd in &days {
                    let day = week + wd as i64;
                    if day < day0 {
                        continue;
                    }
                    let s = at(day);
                    if limit(s, n) || n > 3000 {
                        break 'w;
                    }
                    push(s);
                    n += 1;
                }
                week += 7 * interval;
            }
        }
        "MONTHLY" | "YEARLY" => {
            let mut k = 0i64;
            loop {
                let months = if freq == "MONTHLY" { k * interval } else { k * interval * 12 };
                let total = (y0 as i64) * 12 + (m0 as i64 - 1) + months;
                let (y, m) = ((total / 12) as i32, (total % 12) as u32 + 1);
                k += 1;
                if d0 > crate::time::days_in_month(y, m) {
                    continue;
                }
                let s = local_to_unix(y, m, d0, h0, mi0, 0);
                if limit(s, n) || n > 600 {
                    break;
                }
                push(s);
                n += 1;
            }
        }
        _ => push(start),
    }
}

/// Civil date of a day number (for other modules).
pub fn civil_pub(z: i64) -> (i32, u32, u32) {
    civil(z)
}

/// Civil date of a day number (inverse of countdown::day_number).
fn civil(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((if m <= 2 { y + 1 } else { y }) as i32, m, d)
}

/// Fetch the calendar on a worker thread.
fn fetch(url: String) {
    {
        let mut st = STATUS.lock().unwrap();
        if st.0 {
            return;
        }
        st.0 = true;
    }
    std::thread::spawn(move || {
        let u = url.trim().replacen("webcal://", "https://", 1);
        let res = (|| {
            // A calendar file on this PC (exported from Outlook, Google...).
            if !u.starts_with("https://") && !u.starts_with("http://") {
                let text = std::fs::read(&u).map_err(|_| "Couldn't open that calendar file".to_string())?;
                let text = String::from_utf8_lossy(&text).into_owned();
                if !text.contains("BEGIN:VCALENDAR") {
                    return Err("That file isn't an iCal (.ics) calendar".to_string());
                }
                let now = unix_now();
                let ev = parse_ics(&text, now - 45.0 * 86400.0, now + 365.0 * 86400.0);
                let n = ev.len();
                *EVENTS.lock().unwrap() = ev;
                return Ok::<usize, String>(n);
            }
            let rest = u.strip_prefix("https://").ok_or_else(|| "The link must start with https:// or webcal://".to_string())?;
            let (host, path) = rest.split_once('/').map(|(h, p)| (h.to_string(), format!("/{p}"))).unwrap_or((rest.to_string(), "/".into()));
            let r = crate::http::get(&host, &path).ok_or_else(|| "Couldn't reach the calendar".to_string())?;
            if r.status != 200 {
                return Err(format!("The calendar answered {}", r.status));
            }
            let text = String::from_utf8_lossy(&r.body).into_owned();
            if !text.contains("BEGIN:VCALENDAR") {
                return Err("That link isn't an iCal (.ics) calendar".to_string());
            }
            let now = unix_now();
            let ev = parse_ics(&text, now - 45.0 * 86400.0, now + 90.0 * 86400.0);
            let n = ev.len();
            *EVENTS.lock().unwrap() = ev;
            Ok::<usize, String>(n)
        })();
        *STATUS.lock().unwrap() = (false, Some(res));
        crate::app::wake();
    });
}

pub struct Meetings {
    fetched: f64,
    url: String,
    alerted: f64,
    next: Option<Event>,
}

impl Meetings {
    pub fn new() -> Meetings {
        Meetings { fetched: 0.0, url: String::new(), alerted: 0.0, next: None }
    }
}

/// Refresh now (after the link changed).
pub fn refresh(url: &str) {
    if url.trim().is_empty() {
        EVENTS.lock().unwrap().clear();
        *STATUS.lock().unwrap() = (false, None);
    } else {
        fetch(url.to_string());
    }
}

fn mins_text(secs: f64) -> String {
    let m = (secs / 60.0).ceil() as i64;
    if secs <= 0.0 {
        "Now".into()
    } else if m < 60 {
        format!("{m} min")
    } else {
        format!("{} h {} m", m / 60, m % 60)
    }
}

impl Module for Meetings {
    fn title(&self) -> &'static str {
        "Meetings"
    }
    fn has_tab(&self) -> bool {
        false
    }
    fn icon(&self) -> &'static str {
        "\u{E8F2}"
    }
    fn accent(&self) -> Color {
        C_MEET
    }
    fn tick(&mut self, ctx: &mut Ctx) {
        let on = crate::features::is_on(ctx.settings, "meetings");
        let url = ctx.settings.extra.get(URL_KEY).cloned().unwrap_or_default();
        let now = unix_now();
        if !on || url.is_empty() {
            self.next = None;
            return;
        }
        if url != self.url || now - self.fetched > REFRESH {
            self.url = url.clone();
            self.fetched = now;
            fetch(url);
        }
        self.next = next_event(now, SOON);
        if let Some(e) = &self.next {
            // A minute before: drop down with Join.
            if e.start - now <= 60.0 && e.start - now > -120.0 && self.alerted != e.start {
                self.alerted = e.start;
                let mut t = Toast::new("meeting", "\u{E8F2}", C_MEET, e.title.clone(), if e.start > now { "Starts in a minute" } else { "Starting now" }).secs(12.0).chime();
                if e.link.is_some() {
                    t = t.action("Join", A_JOIN).click(A_JOIN);
                }
                ctx.toast(t);
            }
        }
    }
    fn action(&mut self, id: u32, _rel: Rel, _ctx: &mut Ctx) {
        if id == A_JOIN {
            if let Some(l) = self.next.as_ref().and_then(|e| e.link.clone()) {
                crate::app::open_later(l);
            }
        }
    }
    fn live(&self) -> Option<u8> {
        self.next.as_ref().map(|e| if e.start <= unix_now() { 60 } else { 52 })
    }
    fn live_id(&self) -> &'static str {
        "meeting"
    }
    fn live_color(&self) -> Color {
        C_MEET
    }
    fn draw_live(&mut self, ui: &mut Ui, left: Rect, right: Rect) {
        if let Some(e) = self.next.clone() {
            live_simple(ui, left, right, "\u{E8F2}", C_MEET, &e.title, &mins_text(e.start - unix_now()));
        }
    }
    fn draw_live_side(&mut self, ui: &mut Ui, r: Rect, right: bool) {
        if let Some(e) = self.next.clone() {
            live_simple_side(ui, r, right, "\u{E8F2}", C_MEET, &mins_text(e.start - unix_now()));
        }
    }
    fn debug_state(&self) -> String {
        format!("meetings next={:?}", self.next.as_ref().map(|e| e.title.clone()))
    }
}
