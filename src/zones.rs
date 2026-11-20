//! Other cities' time, for the World clock widget: Windows' own time zone
//! data (with daylight saving), by its zone names.

use std::collections::HashMap;
use std::sync::Mutex;

use windows::Win32::Foundation::SYSTEMTIME;
use windows::Win32::System::SystemInformation::GetSystemTime;
use windows::Win32::System::Time::{DYNAMIC_TIME_ZONE_INFORMATION, EnumDynamicTimeZoneInformation, SystemTimeToTzSpecificLocalTimeEx};

/// The cities you can pick: (name, Windows zone).
pub const CITIES: [(&str, &str); 16] = [
    ("London", "GMT Standard Time"),
    ("New York", "Eastern Standard Time"),
    ("Los Angeles", "Pacific Standard Time"),
    ("Paris", "Romance Standard Time"),
    ("Berlin", "W. Europe Standard Time"),
    ("Dubai", "Arabian Standard Time"),
    ("Riyadh", "Arab Standard Time"),
    ("Delhi", "India Standard Time"),
    ("Colombo", "Sri Lanka Standard Time"),
    ("Dhaka", "Bangladesh Standard Time"),
    ("Singapore", "Singapore Standard Time"),
    ("Hong Kong", "China Standard Time"),
    ("Tokyo", "Tokyo Standard Time"),
    ("Sydney", "AUS Eastern Standard Time"),
    ("Toronto", "Eastern Standard Time"),
    ("São Paulo", "E. South America Standard Time"),
];

const DEFAULT: &str = "London,New York,Tokyo,Dubai";

/// The cities picked (`world_cities`), up to four.
pub fn picked(st: &crate::store::Settings) -> Vec<&'static str> {
    let raw = st.extra.get("world_cities").map(String::as_str).unwrap_or(DEFAULT);
    raw.split(',').filter_map(|c| CITIES.iter().find(|x| x.0 == c).map(|x| x.0)).take(4).collect()
}

/// Add a city, or take it off (four at most).
pub fn toggle(st: &mut crate::store::Settings, city: &str) {
    let mut v: Vec<&str> = picked(st);
    if let Some(i) = v.iter().position(|c| *c == city) {
        v.remove(i);
    } else if v.len() < 4 {
        v.push(CITIES.iter().find(|x| x.0 == city).map_or(city, |x| x.0));
    }
    st.extra.insert("world_cities".into(), if v.is_empty() { ",".into() } else { v.join(",") });
}

static ZONES: Mutex<Option<HashMap<String, DYNAMIC_TIME_ZONE_INFORMATION>>> = Mutex::new(None);

fn zone(key: &str) -> Option<DYNAMIC_TIME_ZONE_INFORMATION> {
    let mut g = ZONES.lock().ok()?;
    let map = g.get_or_insert_with(|| {
        let mut m = HashMap::new();
        for i in 0..1000u32 {
            let mut d = DYNAMIC_TIME_ZONE_INFORMATION::default();
            if unsafe { EnumDynamicTimeZoneInformation(i, &mut d) } != 0 {
                break;
            }
            let name = String::from_utf16_lossy(&d.TimeZoneKeyName[..d.TimeZoneKeyName.iter().position(|&c| c == 0).unwrap_or(0)]);
            m.insert(name, d);
        }
        m
    });
    map.get(key).copied()
}

/// A city's time now: (hour, minute, days from your date: -1, 0 or 1,
/// minutes ahead of you).
pub fn time_in(city: &str) -> Option<(u16, u16, i32, i32)> {
    let key = CITIES.iter().find(|c| c.0 == city)?.1;
    let z = zone(key)?;
    unsafe {
        let utc: SYSTEMTIME = GetSystemTime();
        let mut there = SYSTEMTIME::default();
        SystemTimeToTzSpecificLocalTimeEx(Some(&z), &utc, &mut there).ok()?;
        let here = crate::time::Now::get();
        let day = |y: u16, m: u16, d: u16| y as i32 * 400 + m as i32 * 32 + d as i32;
        let dd = (day(there.wYear, there.wMonth, there.wDay) - day(here.year, here.month, here.day)).signum();
        let ahead = dd * 1440 + (there.wHour as i32 * 60 + there.wMinute as i32) - (here.hour as i32 * 60 + here.min as i32);
        Some((there.wHour, there.wMinute, dd, ahead))
    }
}

/// "+5:30 h", "−3 h", "same time".
pub fn ahead_text(min: i32) -> String {
    if min == 0 {
        return "Same time".into();
    }
    let s = if min > 0 { '+' } else { '\u{2212}' };
    let (h, m) = (min.abs() / 60, min.abs() % 60);
    if m == 0 { format!("{s}{h} h") } else { format!("{s}{h}:{m:02} h") }
}
