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

/// Whose clock a time is on: this PC's, UTC ("...Z"), or a time zone the
/// calendar names (TZID, by Windows' name for it).
#[derive(Clone, Debug, PartialEq)]
enum Clock {
    Local,
    Utc,
    Zone(String),
}

impl Clock {
    /// The clock of a value with these parameters. A zone that can't be
    /// told counts as this PC's.
    fn of(v: &str, params: &str) -> Clock {
        if v.trim().ends_with('Z') {
            return Clock::Utc;
        }
        params
            .split(';')
            .find_map(|p| p.split_once('=').filter(|(k, _)| k.eq_ignore_ascii_case("TZID")).map(|x| x.1))
            .and_then(crate::modules::tools::tz::windows_key)
            .map_or(Clock::Local, Clock::Zone)
    }

    /// A wall time on this clock, as unix seconds.
    fn unix(&self, y: i32, m: u32, d: u32, h: u32, mi: u32, s: u32) -> f64 {
        match self {
            Clock::Local => local_to_unix(y, m, d, h, mi, s),
            Clock::Utc => utc_to_unix(y, m, d, h, mi, s),
            Clock::Zone(k) => {
                let st = windows::Win32::Foundation::SYSTEMTIME { wYear: y as u16, wMonth: m as u16, wDay: d as u16, wHour: h as u16, wMinute: mi as u16, wSecond: s as u16, ..Default::default() };
                crate::modules::tools::tz::utc(k, &st).map_or_else(|| local_to_unix(y, m, d, h, mi, s), crate::time::filetime_to_unix)
            }
        }
    }
}

/// A date-time value's parts: (year, month, day, hour, minute, second, all
/// day). "20261112T090000" or "20261112" (a whole day).
fn wall(v: &str) -> Option<(i32, u32, u32, u32, u32, u32, bool)> {
    let v = v.trim();
    let n = |a: usize, b: usize| v.get(a..b)?.parse::<u32>().ok();
    let (y, m, d) = (n(0, 4)? as i32, n(4, 6)?, n(6, 8)?);
    if v.len() == 8 {
        return Some((y, m, d, 0, 0, 0, true));
    }
    Some((y, m, d, n(9, 11)?, n(11, 13)?, n(13, 15).unwrap_or(0), false))
}

/// A date-time value: (unix, all day). "20261112T090000Z" is UTC; with a
/// TZID it's that zone's wall time, without one this PC's; "20261112" is a
/// whole day (here).
fn parse_time_in(v: &str, params: &str) -> Option<(f64, bool)> {
    let (y, m, d, h, mi, s, all_day) = wall(v)?;
    let clock = if all_day { Clock::Local } else { Clock::of(v, params) };
    Some((clock.unix(y, m, d, h, mi, s), all_day))
}

fn parse_time(v: &str) -> Option<(f64, bool)> {
    parse_time_in(v, "")
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
    let mut all: Vec<Props> = Vec::new();
    let mut ev: Option<Props> = None;
    for l in lines {
        if l == "BEGIN:VEVENT" {
            ev = Some(Vec::new());
            continue;
        }
        if l == "END:VEVENT" {
            all.extend(ev.take());
            continue;
        }
        if let Some(props) = ev.as_mut() {
            if let Some((head, value)) = l.split_once(':') {
                let (name, params) = head.split_once(';').unwrap_or((head, ""));
                props.push((name.to_uppercase(), params.to_string(), value.to_string()));
            }
        }
    }
    // One occurrence of a repeating event that was moved or changed is an
    // event of its own, saying which one it stands in for.
    let moved: Vec<(String, f64)> = all.iter().filter_map(|p| Some((prop(p, "UID")?.to_string(), timed(p, "RECURRENCE-ID")?.0))).collect();
    let mut out = Vec::new();
    for p in &all {
        let uid = prop(p, "UID").unwrap_or("");
        let gone: Vec<f64> = moved.iter().filter(|m| m.0 == uid).map(|m| m.1).collect();
        expand(p, &gone, from, to, &mut out);
    }
    out.sort_by(|a, b| a.start.total_cmp(&b.start));
    out
}

/// An event's lines: (name, parameters, value).
type Props = Vec<(String, String, String)>;

/// A date-time property, in its own time zone.
fn timed(p: &Props, name: &str) -> Option<(f64, bool)> {
    p.iter().find(|x| x.0 == name).and_then(|x| parse_time_in(&x.2, &x.1))
}

fn prop<'a>(p: &'a Props, name: &str) -> Option<&'a str> {
    p.iter().find(|x| x.0 == name).map(|x| x.2.as_str())
}

/// The days of a month a rule picks: BYMONTHDAY (from the end when
/// negative), else BYDAY ("2TU" the second Tuesday, "-1FR" the last Friday,
/// "MO" every Monday; BYSETPOS picks among them), else the first day's own.
fn month_days(y: i32, m: u32, d0: u32, bymonthday: &[i32], byday: &[(i32, u32)], setpos: &[i32]) -> Vec<u32> {
    let len = crate::time::days_in_month(y, m) as i32;
    let mut v: Vec<i32> = if !bymonthday.is_empty() {
        bymonthday.iter().map(|&d| if d < 0 { len + 1 + d } else { d }).collect()
    } else if !byday.is_empty() {
        let first = (crate::modules::countdown::day_number(y, m, 1) + 3).rem_euclid(7) as i32;
        byday
            .iter()
            .flat_map(|&(nth, wd)| {
                let days: Vec<i32> = (1..=len).filter(|d| (first + d - 1).rem_euclid(7) == wd as i32).collect();
                match nth {
                    0 => days,
                    n if n > 0 => days.get(n as usize - 1).copied().into_iter().collect(),
                    n => days.len().checked_sub(n.unsigned_abs() as usize).and_then(|k| days.get(k)).copied().into_iter().collect(),
                }
            })
            .collect()
    } else {
        vec![d0 as i32]
    };
    v.retain(|d| (1..=len).contains(d));
    v.sort_unstable();
    v.dedup();
    if !setpos.is_empty() {
        v = setpos.iter().filter_map(|&p| if p > 0 { v.get(p as usize - 1) } else { v.len().checked_sub(p.unsigned_abs() as usize).and_then(|k| v.get(k)) }).copied().collect();
        v.sort_unstable();
    }
    v.into_iter().map(|d| d as u32).collect()
}

fn expand(props: &Props, moved: &[f64], from: f64, to: f64, out: &mut Vec<Event>) {
    let get = |n: &str| prop(props, n);
    if get("STATUS").map_or(false, |s| s.eq_ignore_ascii_case("CANCELLED")) {
        return;
    }
    let Some((start, all_day)) = timed(props, "DTSTART") else { return };
    let end = timed(props, "DTEND").map(|x| x.0).unwrap_or(start + if all_day { 86400.0 } else { 1800.0 });
    let dur = (end - start).max(0.0);
    let title = get("SUMMARY").map(unescape).unwrap_or_else(|| "Event".into());
    let text = format!("{} {} {}", get("LOCATION").unwrap_or_default(), get("URL").unwrap_or_default(), get("DESCRIPTION").map(unescape).unwrap_or_default());
    let link = find_link(&text);
    let exdates: Vec<f64> = props.iter().filter(|p| p.0 == "EXDATE").flat_map(|p| p.2.split(',').filter_map(|v| parse_time_in(v, &p.1).map(|x| x.0)).collect::<Vec<_>>()).collect();
    let mut push = |s: f64| {
        if s < to && s + dur > from && !exdates.iter().any(|x| (x - s).abs() < 60.0) {
            out.push(Event { start: s, end: s + dur, title: title.clone(), link: link.clone(), all_day });
        }
    };
    let Some(rule) = get("RRULE").filter(|_| get("RECURRENCE-ID").is_none()) else {
        push(start);
        return;
    };
    let r: std::collections::HashMap<String, String> = rule.split(';').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.to_uppercase(), v.to_uppercase())).collect();
    let interval = r.get("INTERVAL").and_then(|v| v.parse::<i64>().ok()).unwrap_or(1).clamp(1, 1000);
    let count = r.get("COUNT").and_then(|v| v.parse::<usize>().ok());
    let until = r.get("UNTIL").and_then(|v| parse_time(v)).map(|x| x.0 + if x.1 { 86399.0 } else { 0.0 });
    let nums = |k: &str| -> Vec<i32> { r.get(k).map(|v| v.split(',').filter_map(|x| x.parse().ok()).collect()).unwrap_or_default() };
    let (bymonthday, setpos, bymonth) = (nums("BYMONTHDAY"), nums("BYSETPOS"), nums("BYMONTH"));
    // (which one in the month, weekday with Monday first)
    let byday: Vec<(i32, u32)> = r
        .get("BYDAY")
        .map(|v| {
            v.split(',')
                .filter_map(|d| {
                    let wd = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"].iter().position(|x| d.ends_with(x))?;
                    Some((d[..d.len() - 2].parse().unwrap_or(0), wd as u32))
                })
                .collect()
        })
        .unwrap_or_default();
    // Work on local days so the wall time stays put across DST.
    // On the event's own clock (a zone's day and hour stay put across its
    // summer time, not this PC's).
    let dt = props.iter().find(|x| x.0 == "DTSTART");
    let clock = dt.filter(|_| !all_day).map_or(Clock::Local, |x| Clock::of(&x.2, &x.1));
    let Some((y0, m0, d0, h0, mi0, _, _)) = dt.and_then(|x| wall(&x.2)) else { return };
    let number = crate::modules::countdown::day_number;
    let day0 = number(y0, m0, d0);
    // Straight to the days that matter (a repeat that started years ago),
    // unless occurrences are counted from the first.
    let first = if count.is_some() { day0 } else { day0.max((from / 86400.0).floor() as i64 - 2 - (dur / 86400.0) as i64) };
    let mut n = 0usize;
    // The days of the k-th period (day, week, month or year) from the first.
    for k in 0..20_000i64 {
        let days: Vec<i64> = match r.get("FREQ").map_or("", String::as_str) {
            "DAILY" => {
                let day = day0 + ((first - day0) / interval + k) * interval;
                let (_, m, _) = civil(day);
                let wd = (day + 3).rem_euclid(7) as u32;
                if (byday.is_empty() || byday.iter().any(|b| b.1 == wd)) && (bymonth.is_empty() || bymonth.contains(&(m as i32))) { vec![day] } else { Vec::new() }
            }
            "WEEKLY" => {
                let wd0 = (day0 + 3).rem_euclid(7);
                let week = day0 - wd0 + ((first - day0) / 7 / interval + k) * 7 * interval;
                let mut d: Vec<i64> = if byday.is_empty() { vec![week + wd0] } else { byday.iter().map(|b| week + b.1 as i64).collect() };
                d.sort_unstable();
                d
            }
            f @ ("MONTHLY" | "YEARLY") => {
                let (fy, fm, _) = civil(first);
                let step = if f == "MONTHLY" { interval } else { interval * 12 };
                let since = (fy as i64 - y0 as i64) * 12 + fm as i64 - m0 as i64;
                let month = y0 as i64 * 12 + m0 as i64 - 1 + ((since.max(0) / step) + k) * step;
                let months: Vec<i64> = if f == "YEARLY" && !bymonth.is_empty() {
                    let mut v: Vec<i64> = bymonth.iter().filter(|m| (1..=12).contains(*m)).map(|m| month.div_euclid(12) * 12 + *m as i64 - 1).collect();
                    v.sort_unstable();
                    v
                } else {
                    vec![month]
                };
                months
                    .into_iter()
                    .flat_map(|t| {
                        let (y, m) = (t.div_euclid(12) as i32, t.rem_euclid(12) as u32 + 1);
                        let wanted = f == "YEARLY" || bymonth.is_empty() || bymonth.contains(&(m as i32));
                        let d = if wanted { month_days(y, m, d0, &bymonthday, &byday, &setpos) } else { Vec::new() };
                        d.into_iter().map(move |d| number(y, m, d))
                    })
                    .collect()
            }
            _ => {
                push(start);
                return;
            }
        };
        // A period with no day in it (a 31st) still has to end somewhere.
        if days.is_empty() && k > 400 && n == 0 {
            return;
        }
        for day in days {
            if day < day0 {
                continue;
            }
            let (y, m, d) = civil(day);
            let s = clock.unix(y, m, d, h0, mi0, 0);
            if until.is_some_and(|u| s > u) || count.is_some_and(|c| n >= c) || s > to {
                return;
            }
            n += 1;
            if !moved.iter().any(|x| (x - s).abs() < 60.0) {
                push(s);
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The days ("M-D") an event with this rule falls on, over 2026.
    fn days(dtstart: &str, rule: &str, extra: &str) -> Vec<String> {
        let ics = format!("BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:a\nDTSTART:{dtstart}\nSUMMARY:x\nRRULE:{rule}\nEND:VEVENT\n{extra}END:VCALENDAR");
        let (from, to) = (local_to_unix(2026, 1, 1, 0, 0, 0), local_to_unix(2026, 12, 31, 0, 0, 0));
        parse_ics(&ics, from, to)
            .iter()
            .map(|e| {
                let (_, m, d, _) = crate::time::local_time(e.start);
                format!("{m}-{d}")
            })
            .collect()
    }

    #[test]
    fn monthly_by_weekday() {
        // Second Tuesday; last Friday; the last working day.
        assert_eq!(days("20260113T090000", "FREQ=MONTHLY;BYDAY=2TU", "")[..3], ["1-13", "2-10", "3-10"]);
        assert_eq!(days("20260130T090000", "FREQ=MONTHLY;BYDAY=-1FR", "")[..3], ["1-30", "2-27", "3-27"]);
        assert_eq!(days("20260130T090000", "FREQ=MONTHLY;BYDAY=MO,TU,WE,TH,FR;BYSETPOS=-1", "")[..3], ["1-30", "2-27", "3-31"]);
        assert_eq!(days("20260131T090000", "FREQ=MONTHLY;BYMONTHDAY=-1", "")[..3], ["1-31", "2-28", "3-31"]);
        // The 31st: only the months that have one.
        assert_eq!(days("20260131T090000", "FREQ=MONTHLY", ""), ["1-31", "3-31", "5-31", "7-31", "8-31", "10-31"]);
    }

    #[test]
    fn started_long_ago() {
        assert_eq!(days("20100104T090000", "FREQ=DAILY", "").len(), 364);
        assert_eq!(days("20100104T090000", "FREQ=DAILY;BYDAY=MO,TU,WE,TH,FR", "")[..3], ["1-1", "1-2", "1-5"]);
        assert_eq!(days("20100104T090000", "FREQ=WEEKLY;BYDAY=FR,MO", "")[..3], ["1-2", "1-5", "1-9"]);
        assert_eq!(days("19900215T090000", "FREQ=YEARLY", ""), ["2-15"]);
        // Every other week keeps its own weeks.
        assert_eq!(days("20251229T090000", "FREQ=WEEKLY;INTERVAL=2", "")[..2], ["1-12", "1-26"]);
        // Counted ones end where they end.
        assert_eq!(days("20251231T090000", "FREQ=DAILY;COUNT=3", ""), ["1-1", "1-2"]);
    }

    /// UTC hours of an event, in this zone, over 2026.
    fn utc_hours(dtstart: &str, rule: &str) -> Vec<String> {
        let ics = format!("BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:a\nDTSTART;{dtstart}\nSUMMARY:x\n{rule}END:VEVENT\nEND:VCALENDAR");
        parse_ics(&ics, utc_to_unix(2026, 1, 1, 0, 0, 0), utc_to_unix(2026, 12, 31, 0, 0, 0))
            .iter()
            .map(|e| {
                let s = e.start as i64;
                let (_, m, d) = civil(s.div_euclid(86400));
                format!("{m}-{d} {}:{:02}", s.rem_euclid(86400) / 3600, s.rem_euclid(3600) / 60)
            })
            .take(3)
            .collect()
    }

    #[test]
    fn other_time_zones() {
        // Google writes IANA names, Outlook Windows' own: 9:00 in New York
        // is 14:00 UTC in winter and 13:00 in summer.
        assert_eq!(utc_hours("TZID=America/New_York:20260601T090000", ""), ["6-1 13:00"]);
        assert_eq!(utc_hours("TZID=Eastern Standard Time:20260115T090000", ""), ["1-15 14:00"]);
        assert_eq!(utc_hours("TZID=\"Europe/London\":20260115T090000", ""), ["1-15 9:00"]);
        // A weekly one keeps its 9:00 there across the change (8 March).
        assert_eq!(utc_hours("TZID=America/New_York:20260302T090000", "RRULE:FREQ=WEEKLY;COUNT=2\n"), ["3-2 14:00", "3-9 13:00"]);
        // Unknown zones count as this PC's (as before), never dropped.
        assert_eq!(utc_hours("TZID=Customized Time Zone:20260601T090000", "").len(), 1);
    }

    #[test]
    fn moved_once() {
        let extra = "BEGIN:VEVENT\nUID:a\nRECURRENCE-ID:20260112T090000\nDTSTART:20260113T150000\nSUMMARY:x\nEND:VEVENT\n";
        assert_eq!(days("20260105T090000", "FREQ=WEEKLY;COUNT=3", extra), ["1-5", "1-13", "1-19"]);
    }

    #[test]
    fn nonsense_is_survived() {
        for rule in ["FREQ=MONTHLY;BYMONTHDAY=40", "FREQ=YEARLY;BYMONTH=13", "FREQ=WEEKLY;BYDAY=ZZ,\u{e9}MO", "FREQ=DAILY;INTERVAL=99999999999", "FREQ=MONTHLY;BYDAY=-9MO", "FREQ=HOURLY", "X"] {
            days("20260105T090000", rule, "");
        }
        days("00000000T000000", "FREQ=DAILY", "");
        days("99991231T235959", "FREQ=YEARLY", "");
    }
}
