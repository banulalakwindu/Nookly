//! The taskbar add-on's settings that both exes need: what the bar can hold
//! and in what order. All of it lives in settings.txt (`bar_*` keys),
//! written by the notch's Settings window and read by the taskbar.

use crate::store::Settings;

/// What the bar can hold besides widgets: (key, name in Settings, icon).
/// Like Windows, the right end has the hidden icons, the keyboard
/// language, Wi-Fi / sound / battery and the date and time; each can be
/// switched off, moved, and pinned or left to scroll like a widget.
pub const ITEMS: [(&str, &str, &str); 8] = [
    ("start", "Start", "\u{E782}"),
    ("search", "Search", "\u{E721}"),
    ("apps", "Apps", "\u{ECAA}"),
    ("tray", "Hidden icons", "\u{E70E}"),
    ("lang", "Keyboard language", "\u{E765}"),
    ("quick", "Wi-Fi, sound and battery", "\u{E701}"),
    ("clock", "Date and time", "\u{E823}"),
    ("notifs", "Notifications", "\u{EA8F}"),
];

/// The right end's items that show a picture the notch draws (like a
/// widget): (item, face id, width in DIPs).
pub const SYS: [(&str, &str, f32); 3] = [("lang", "sys_lang", 34.0), ("quick", "sys_quick", 68.0), ("clock", "sys_clock", 66.0)];

/// An item's width on the bar (DIPs; the apps' depends on how many).
pub fn item_width(key: &str) -> f32 {
    match key {
        "start" | "search" | "notifs" => 38.0,
        "tray" => 28.0,
        "apps" => 0.0,
        k => SYS.iter().find(|s| s.0 == k).map_or(0.0, |s| s.2),
    }
}

/// Where something sits on the bar: pinned at the left end, in the middle
/// (which scrolls when it doesn't all fit), or pinned at the right end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Left,
    Mid,
    Right,
}

impl Zone {
    pub const ALL: [Zone; 3] = [Zone::Left, Zone::Mid, Zone::Right];
    fn key(self) -> &'static str {
        match self {
            Zone::Left => "l",
            Zone::Mid => "m",
            Zone::Right => "r",
        }
    }
    fn from_key(k: &str) -> Option<Zone> {
        Zone::ALL.into_iter().find(|z| z.key() == k)
    }
    pub fn name(self) -> &'static str {
        match self {
            Zone::Left => "Pinned left",
            Zone::Mid => "In the middle (scrolls)",
            Zone::Right => "Pinned right",
        }
    }
}

/// One thing on the bar: an item (Start, Apps, the clock...) or a widget.
#[derive(Clone, Debug, PartialEq)]
pub enum Thing {
    Item(&'static str),
    Widget(String, Size),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub thing: Thing,
    pub zone: Zone,
    /// Items can be switched off (widgets are taken off instead).
    pub on: bool,
}

impl Entry {
    /// Its width on the bar (DIPs).
    pub fn width(&self) -> f32 {
        match &self.thing {
            Thing::Item(k) => item_width(k),
            Thing::Widget(_, size) => size.width(),
        }
    }
    fn save(&self) -> String {
        match &self.thing {
            Thing::Item(k) => format!("{k}:{}{}", self.zone.key(), if self.on { "" } else { ":off" }),
            Thing::Widget(id, size) => format!("@{id}:{}:{}", size.key(), self.zone.key()),
        }
    }
}

/// Where an item goes when it's new: Windows' places.
fn home(key: &str) -> Zone {
    match key {
        "start" | "search" => Zone::Left,
        "apps" => Zone::Mid,
        _ => Zone::Right,
    }
}

/// The widgets a new taskbar starts with: offline ones (the weather goes
/// online, so it waits until you add it).
const DEFAULT_WIDGETS: &str = "system:m";

/// Everything on the bar, in order (`bar_layout`): the left end, the
/// middle, the right end. Settings from before (`bar_order`, `bar_widgets`
/// with its pins) are read and moved over.
pub fn layout(st: &Settings) -> Vec<Entry> {
    let mut v: Vec<Entry> = match st.extra.get("bar_layout") {
        Some(raw) => raw
            .split(',')
            .filter_map(|e| {
                if let Some(w) = e.strip_prefix('@') {
                    let mut p = w.split(':');
                    let id = p.next()?;
                    let d = def(id)?;
                    let size = p.next().and_then(Size::from_key).filter(|s| d.sizes.contains(s)).unwrap_or(d.default);
                    let zone = p.next().and_then(Zone::from_key).unwrap_or(Zone::Mid);
                    Some(Entry { thing: Thing::Widget(id.to_string(), size), zone, on: true })
                } else {
                    let mut p = e.split(':');
                    let k = p.next()?;
                    let key = ITEMS.iter().find(|i| i.0 == k)?.0;
                    let zone = p.next().and_then(Zone::from_key).unwrap_or(home(key));
                    let on = p.next() != Some("off");
                    Some(Entry { thing: Thing::Item(key), zone, on })
                }
            })
            .collect(),
        None => moved_over(st),
    };
    // Each thing once; Apps always in the middle; items new since it was
    // saved go to their home (the right end's before Notifications).
    let mut seen: Vec<String> = Vec::new();
    v.retain(|e| {
        let k = match &e.thing {
            Thing::Item(k) => k.to_string(),
            Thing::Widget(id, _) => format!("@{id}"),
        };
        let new = !seen.contains(&k);
        seen.push(k);
        new
    });
    for e in v.iter_mut() {
        if e.thing == Thing::Item("apps") {
            e.zone = Zone::Mid;
            e.on = true;
        }
    }
    for (k, _, _) in ITEMS {
        if !v.iter().any(|e| e.thing == Thing::Item(k)) {
            let at = v.iter().position(|e| e.thing == Thing::Item("notifs")).filter(|_| home(k) == Zone::Right).unwrap_or(v.len());
            v.insert(at, Entry { thing: Thing::Item(k), zone: home(k), on: true });
        }
    }
    // Kept in zone order (left, middle, right).
    v.sort_by_key(|e| Zone::ALL.iter().position(|z| *z == e.zone));
    v
}

/// The layout from the settings before it (the items' order with the
/// widgets in their slot; pinned widgets at the ends).
fn moved_over(st: &Settings) -> Vec<Entry> {
    let list = |k: &str| -> Vec<String> { st.extra.get(k).map(|s| s.split(',').filter(|x| !x.is_empty()).map(String::from).collect()).unwrap_or_default() };
    let (mut saved, off) = (list("bar_order"), list("bar_off"));
    if !saved.iter().any(|k| k == "lang" || k == "quick") {
        saved.retain(|k| k != "clock");
    }
    if saved.is_empty() {
        saved = ITEMS.iter().map(|i| i.0.to_string()).collect();
    }
    if !saved.iter().any(|k| k == "widgets") {
        // The widgets' slot: after the apps.
        let at = saved.iter().position(|k| k == "apps").map_or(saved.len(), |i| i + 1);
        saved.insert(at, "widgets".into());
    }
    let raw = st.extra.get("bar_widgets").cloned().unwrap_or_else(|| DEFAULT_WIDGETS.to_string());
    let widgets: Vec<(String, Size, Zone)> = raw
        .split(',')
        .filter_map(|e| {
            let mut p = e.split(':');
            let id = p.next()?;
            let d = def(id)?;
            let size = p.next().and_then(Size::from_key).filter(|s| d.sizes.contains(s)).unwrap_or(d.default);
            let zone = match p.next() {
                Some("l") => Zone::Left,
                Some("r") => Zone::Right,
                _ => Zone::Mid,
            };
            Some((id.to_string(), size, zone))
        })
        .collect();
    let mut v = Vec::new();
    for k in &saved {
        if k == "widgets" {
            for (id, size, zone) in &widgets {
                v.push(Entry { thing: Thing::Widget(id.clone(), *size), zone: *zone, on: true });
            }
        } else if let Some(i) = ITEMS.iter().find(|i| i.0 == k) {
            v.push(Entry { thing: Thing::Item(i.0), zone: home(i.0), on: !off.contains(k) });
        }
    }
    v
}

fn save_layout(st: &mut Settings, v: &[Entry]) {
    let s: Vec<String> = v.iter().map(Entry::save).collect();
    st.extra.insert("bar_layout".into(), s.join(","));
}

/// The items, in order, and whether each is on.
pub fn order(st: &Settings) -> Vec<(&'static str, bool)> {
    layout(st)
        .into_iter()
        .filter_map(|e| match e.thing {
            Thing::Item(k) => Some((k, e.on)),
            _ => None,
        })
        .collect()
}

/// Switch an item on or off.
pub fn set_shown(st: &mut Settings, key: &str, shown: bool) {
    let mut v = layout(st);
    if let Some(e) = v.iter_mut().find(|e| matches!(e.thing, Thing::Item(k) if k == key)) {
        e.on = shown || key == "apps";
        save_layout(st, &v);
    }
}

/// Move the thing at place `from` (in `layout`) to place `to`, into the
/// zone of what's there (`zone` for an empty zone's place).
pub fn move_entry(st: &mut Settings, from: usize, to: usize, zone: Zone) {
    let mut v = layout(st);
    if from >= v.len() {
        return;
    }
    let mut e = v.remove(from);
    if e.thing != Thing::Item("apps") {
        e.zone = zone;
    }
    let to = to.min(v.len());
    v.insert(to, e);
    save_layout(st, &v);
}

/// Put the thing at place `k` in another zone (at that zone's inner end:
/// next to the middle).
pub fn set_zone(st: &mut Settings, k: usize, zone: Zone) {
    let mut v = layout(st);
    if k >= v.len() || v[k].thing == Thing::Item("apps") || v[k].zone == zone {
        return;
    }
    let mut e = v.remove(k);
    e.zone = zone;
    let at = match zone {
        Zone::Left => v.iter().rposition(|x| x.zone == Zone::Left).map_or(0, |i| i + 1),
        Zone::Mid => v.iter().rposition(|x| x.zone != Zone::Right).map_or(0, |i| i + 1),
        Zone::Right => v.iter().position(|x| x.zone == Zone::Right).unwrap_or(v.len()),
    };
    v.insert(at, e);
    save_layout(st, &v);
}

/// How wide the pinned ends would be (DIPs) with the thing at place `k`
/// in `zone`.
pub fn pinned_width(st: &Settings, k: usize, zone: Zone) -> f32 {
    layout(st)
        .iter()
        .enumerate()
        .filter(|(i, e)| e.on && if *i == k { zone != Zone::Mid } else { e.zone != Zone::Mid })
        // (The same gap the taskbar leaves between pinned things.)
        .map(|(_, e)| e.width() + 8.0)
        .sum()
}

/// The pinned ends may take this much of a bar `bar_w` DIPs wide, so the
/// middle keeps room for a few apps.
pub fn pin_room(bar_w: f32) -> f32 {
    (bar_w - 4.0 * 42.0 - 80.0).max(0.0)
}

pub fn flag(st: &Settings, key: &str, default: bool) -> bool {
    st.extra.get(key).map_or(default, |v| v == "1")
}

/// The taskbar's theme key: its own, or the notch's ("follow", the default).
pub fn theme_key(st: &Settings) -> String {
    match st.extra.get("bar_theme").map(String::as_str) {
        Some(k) if !k.is_empty() && k != "follow" => k.to_string(),
        _ => st.theme.clone(),
    }
}

// ------------------------------------------------------------ widgets
//
// Four kinds of things, never mixed up:
//   panels          the notch's tabs (Music, Todo...)
//   live activities short-lived, on the notch's sides (a song, a timer)
//   status          tiny always-on values on the notch's sides (battery...)
//   widgets         cards on the taskbar, each about ONE thing, in sizes
//
// A widget shows one subject (Clock is the time, Date the date, Date & Time
// both); a bigger size shows more of it. The notch draws them all (see
// surfaces.rs), so a widget looks the same wherever it's drawn.

/// A widget's size on the taskbar: the bar is one height, so sizes are
/// widths (Small is about one app tile).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Size {
    S,
    M,
    L,
    W,
}

impl Size {
    pub const ALL: [Size; 4] = [Size::S, Size::M, Size::L, Size::W];
    pub fn key(self) -> &'static str {
        match self {
            Size::S => "s",
            Size::M => "m",
            Size::L => "l",
            Size::W => "w",
        }
    }
    pub fn from_key(k: &str) -> Option<Size> {
        Size::ALL.into_iter().find(|s| s.key() == k)
    }
    /// Width in DIPs (the height is surfaces::FACE_H).
    pub fn width(self) -> f32 {
        match self {
            Size::S => 44.0,
            Size::M => 100.0,
            Size::L => 156.0,
            Size::W => 236.0,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Size::S => "Small",
            Size::M => "Medium",
            Size::L => "Large",
            Size::W => "Wide",
        }
    }
    /// The letter on a size chip.
    pub fn letter(self) -> &'static str {
        match self {
            Size::S => "S",
            Size::M => "M",
            Size::L => "L",
            Size::W => "W",
        }
    }
}

/// A widget anyone can put on the taskbar.
pub struct Def {
    pub id: &'static str,
    pub name: &'static str,
    pub icon: &'static str,
    /// Where it's listed in Settings > Taskbar > Widgets.
    pub group: &'static str,
    /// What it shows, in a few words.
    pub about: &'static str,
    pub sizes: &'static [Size],
    pub default: Size,
    /// The notch feature it comes from ("" when always there), so Settings
    /// can say what to switch on.
    pub feature: &'static str,
}

const SMLW: &[Size] = &[Size::S, Size::M, Size::L, Size::W];
const SML: &[Size] = &[Size::S, Size::M, Size::L];
const SM: &[Size] = &[Size::S, Size::M];
const MLW: &[Size] = &[Size::M, Size::L, Size::W];

pub const GROUPS: [&str; 5] = ["Time & date", "System", "Weather & sky", "Media & money", "Your day"];

const fn d(id: &'static str, name: &'static str, icon: &'static str, group: &'static str, about: &'static str, sizes: &'static [Size], default: Size, feature: &'static str) -> Def {
    Def { id, name, icon, group, about, sizes, default, feature }
}

/// Every taskbar widget, in the order Settings lists them.
pub const CATALOG: &[Def] = &[
    d("clock", "Clock", "\u{E917}", "Time & date", "The time", SMLW, Size::M, ""),
    d("date", "Date", "\u{E787}", "Time & date", "Today's date", SMLW, Size::M, ""),
    d("datetime", "Date & Time", "\u{E823}", "Time & date", "The time and today's date together", MLW, Size::L, ""),
    d("worldclock", "World clock", "\u{E909}", "Time & date", "The time in other cities", MLW, Size::L, ""),
    d("dayprog", "Day progress", "\u{E916}", "Time & date", "How much of today has gone", SMLW, Size::S, ""),
    d("yearprog", "Year progress", "\u{E81C}", "Time & date", "How much of the year has gone", SMLW, Size::M, ""),
    d("uptime", "Uptime", "\u{E7E8}", "Time & date", "How long since Windows started", SML, Size::M, ""),
    d("system", "CPU & memory", "\u{E9D9}", "System", "Processor and memory, two live graphs", MLW, Size::L, "System Monitor"),
    d("cpu", "CPU", "\u{E950}", "System", "Processor use, with a graph", SMLW, Size::M, "System Monitor"),
    d("ram", "Memory", "\u{E964}", "System", "Memory in use, with a graph", SMLW, Size::M, "System Monitor"),
    d("net", "Network speed", "\u{E896}", "System", "Download and upload speed", SMLW, Size::M, "System Monitor"),
    d("disk", "Free space", "\u{EDA2}", "System", "Space left on your drives", SMLW, Size::M, ""),
    d("battery", "Battery", "\u{E83F}", "System", "Charge, charging and time left", SMLW, Size::M, ""),
    d("wifi", "Wi-Fi", "\u{E701}", "System", "Signal and network name", SMLW, Size::S, ""),
    d("volume", "Volume", "\u{E767}", "System", "Sound level", SMLW, Size::S, ""),
    d("mic", "Microphone", "\u{E720}", "System", "Muted or on", SM, Size::S, ""),
    d("bluetooth", "Bluetooth", "\u{E702}", "System", "On, off and connected devices", SMLW, Size::S, ""),
    d("lang", "Keyboard language", "\u{E765}", "System", "The keyboard language you type in", SM, Size::S, ""),
    d("recycle", "Recycle Bin", "\u{E74D}", "System", "What's in the Recycle Bin", SML, Size::S, ""),
    d("mode", "Light / dark", "\u{E793}", "System", "Windows' light or dark mode", SM, Size::S, ""),
    d("weather", "Weather", "\u{E706}", "Weather & sky", "Temperature, sky and the day's high and low", SMLW, Size::M, ""),
    d("sun", "Sunrise & sunset", "\u{E706}", "Weather & sky", "When the sun rises and sets, and daylight left", SMLW, Size::M, ""),
    d("moon", "Moon", "\u{E708}", "Weather & sky", "Tonight's moon and the next full moon", SML, Size::S, ""),
    d("media", "Now playing", "\u{E8D6}", "Media & money", "The song or video playing, with its cover", SMLW, Size::L, "Now Playing"),
    d("currency", "Currency", "\u{E8C7}", "Media & money", "Exchange rate of a pair you pick, today", SMLW, Size::M, ""),
    d("tasks", "Tasks", "\u{E73A}", "Your day", "Your next task and how many are left", SMLW, Size::M, "TodoList"),
    d("focus", "Focus", "\u{E916}", "Your day", "Focus time today", SMLW, Size::M, "Pomodoro"),
    d("water", "Water", "\u{E7BE}", "Your day", "Glasses of water today", SMLW, Size::M, "Water Reminder"),
    d("screentime", "Screen time", "\u{E7F4}", "Your day", "Time at the screen today", SMLW, Size::M, "Health"),
    d("countdown", "Countdown", "\u{E787}", "Your day", "Days until your next date", SMLW, Size::M, "Countdowns"),
    d("next_prayer", "Next prayer", "\u{E708}", "Your day", "The next prayer and when", SMLW, Size::M, "Prayer Times"),
    d("quote", "Quote of the day", "\u{E8F2}", "Your day", "A new thought each day", MLW, Size::W, ""),
];

/// What a widget's bubble shows: a notch panel that is about the same
/// thing (by its title), or a view of the widget's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Full {
    Panel(&'static str),
    Own,
}

/// Each widget's bubble (every widget has one, and each is its own).
pub fn full_for(id: &str) -> Option<Full> {
    Some(match id {
        "date" => Full::Panel("Calendar"),
        "volume" => Full::Panel("Volume"),
        "media" => Full::Panel("Now Playing"),
        "weather" => Full::Panel("Weather"),
        "tasks" => Full::Panel("TodoList"),
        "focus" => Full::Panel("Pomodoro"),
        "water" => Full::Panel("Water Reminder"),
        "screentime" => Full::Panel("Health"),
        "countdown" => Full::Panel("Countdowns"),
        "next_prayer" => Full::Panel("Prayer Times"),
        _ if def(id).is_some() => Full::Own,
        _ => return None,
    })
}

/// The notch panel a widget's bubble shows, if it shows one.
pub fn panel_for(id: &str) -> Option<&'static str> {
    match full_for(id) {
        Some(Full::Panel(t)) => Some(t),
        _ => None,
    }
}

pub fn def(id: &str) -> Option<&'static Def> {
    CATALOG.iter().find(|d| d.id == id)
}

/// Where a widget sits (from its zone): in the middle with the apps (it
/// scrolls with them when they don't fit), or pinned at an end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pin {
    No,
    Left,
    Right,
}

/// A widget placed on the taskbar, at a size.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub id: String,
    pub size: Size,
    pub pin: Pin,
}

impl Placed {
    /// Its face's name in the shared block: "clock@m".
    pub fn face(&self) -> String {
        format!("{}@{}", self.id, self.size.key())
    }
    /// Its width in DIPs.
    pub fn width(&self) -> f32 {
        SYS.iter().find(|s| s.1 == self.id).map_or(self.size.width(), |s| s.2)
    }
}

/// The widgets on the taskbar, in the layout's order (each once).
pub fn placed(st: &Settings) -> Vec<Placed> {
    layout(st)
        .into_iter()
        .filter_map(|e| match e.thing {
            Thing::Widget(id, size) => Some(Placed {
                id,
                size,
                pin: match e.zone {
                    Zone::Left => Pin::Left,
                    Zone::Mid => Pin::No,
                    Zone::Right => Pin::Right,
                },
            }),
            _ => None,
        })
        .collect()
}

/// The place in `layout` of the widget at place `pos` among the widgets.
pub fn widget_entry(st: &Settings, pos: usize) -> Option<usize> {
    layout(st).iter().enumerate().filter(|(_, e)| matches!(e.thing, Thing::Widget(..))).nth(pos).map(|(i, _)| i)
}

/// Put a widget on the taskbar (at the end of the middle, its usual size);
/// each widget goes on once (its size says how much it shows).
pub fn add_widget(st: &mut Settings, id: &str) {
    let Some(d) = def(id) else { return };
    let mut v = layout(st);
    if v.iter().any(|e| matches!(&e.thing, Thing::Widget(x, _) if x == id)) {
        return;
    }
    let at = v.iter().rposition(|x| x.zone != Zone::Right).map_or(0, |i| i + 1);
    v.insert(at, Entry { thing: Thing::Widget(id.to_string(), d.default), zone: Zone::Mid, on: true });
    save_layout(st, &v);
}

/// Take the widget at `pos` off the taskbar.
pub fn remove_widget(st: &mut Settings, pos: usize) {
    if let Some(k) = widget_entry(st, pos) {
        let mut v = layout(st);
        v.remove(k);
        save_layout(st, &v);
    }
}

/// Change the size of the widget at `pos`.
pub fn resize_widget(st: &mut Settings, pos: usize, size: Size) {
    let Some(k) = widget_entry(st, pos) else { return };
    let mut v = layout(st);
    if let Thing::Widget(id, s) = &mut v[k].thing {
        if def(id).is_some_and(|d| d.sizes.contains(&size)) {
            *s = size;
            save_layout(st, &v);
        }
    }
}

/// Pin the widget at `pos` left or right, or let it scroll.
pub fn pin_widget(st: &mut Settings, pos: usize, pin: Pin) {
    if let Some(k) = widget_entry(st, pos) {
        set_zone(st, k, match pin {
            Pin::Left => Zone::Left,
            Pin::No => Zone::Mid,
            Pin::Right => Zone::Right,
        });
    }
}

/// The faces the notch draws for the taskbar (each widget and size once),
/// while the taskbar is on.
pub fn faces(st: &Settings) -> Vec<Placed> {
    if !flag(st, "taskbar_on", false) {
        return Vec::new();
    }
    let items = order(st);
    let on = |key: &str| items.iter().any(|&(k, on)| k == key && on);
    let mut v = placed(st);
    // The right end's language, Wi-Fi / sound / battery and clock.
    for (item, id, _) in SYS {
        if on(item) {
            v.push(Placed { id: id.to_string(), size: Size::S, pin: Pin::No });
        }
    }
    let mut seen = Vec::new();
    v.retain(|p| {
        let f = p.face();
        let new = !seen.contains(&f);
        seen.push(f);
        new
    });
    v
}

/// The taskbar is on and shows this widget (so the notch keeps its value
/// up to date, as it does for its own status items).
pub fn bar_wants(st: &Settings, id: &str) -> bool {
    flag(st, "taskbar_on", false)
        && (placed(st).iter().any(|p| p.id == id) || matches!(id, "wifi" | "volume" | "battery") && order(st).iter().any(|&(k, on)| k == "quick" && on))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st() -> Settings {
        let mut s = Settings::load();
        s.extra.clear();
        s
    }

    fn ids(s: &Settings) -> Vec<String> {
        placed(s).into_iter().map(|p| format!("{}:{}", p.id, p.size.key())).collect()
    }

    fn keys(s: &Settings) -> Vec<String> {
        layout(s).iter().map(|e| e.save()).collect()
    }

    #[test]
    fn default_layout() {
        let s = st();
        assert_eq!(keys(&s), ["start:l", "search:l", "apps:m", "@system:m:m", "tray:r", "lang:r", "quick:r", "clock:r", "notifs:r"]);
        assert!(order(&s).iter().all(|x| x.1));
    }

    #[test]
    fn old_settings_move_over() {
        let mut s = st();
        s.extra.insert("bar_order".into(), "start,search,apps,widgets,tray,notifs".into());
        s.extra.insert("bar_off".into(), "search".into());
        s.extra.insert("bar_widgets".into(), "clock:s,date:m:r,cpu:m:l".into());
        assert_eq!(keys(&s), ["start:l", "search:l:off", "@cpu:m:l", "apps:m", "@clock:s:m", "@date:m:r", "tray:r", "lang:r", "quick:r", "clock:r", "notifs:r"]);
        assert_eq!(placed(&s)[2].pin, Pin::Right);
    }

    #[test]
    fn zones_and_moves() {
        let mut s = st();
        add_widget(&mut s, "clock");
        add_widget(&mut s, "clock");
        assert_eq!(ids(&s), ["system:m", "clock:m"]);
        // Pin the clock right: at the right end's inner side.
        pin_widget(&mut s, 1, Pin::Right);
        assert_eq!(keys(&s)[4], "@clock:m:r");
        // Hidden icons into the middle: it scrolls now.
        let k = layout(&s).iter().position(|e| e.thing == Thing::Item("tray")).unwrap();
        set_zone(&mut s, k, Zone::Mid);
        assert_eq!(layout(&s).iter().find(|e| e.thing == Thing::Item("tray")).unwrap().zone, Zone::Mid);
        // Apps never leave the middle.
        let a = layout(&s).iter().position(|e| e.thing == Thing::Item("apps")).unwrap();
        set_zone(&mut s, a, Zone::Left);
        assert_eq!(layout(&s)[a].zone, Zone::Mid);
        // A drag to the front: into the left end.
        let n = layout(&s).len();
        move_entry(&mut s, n - 1, 0, Zone::Left);
        assert_eq!(keys(&s)[0], "notifs:l");
        resize_widget(&mut s, 0, Size::S);
        remove_widget(&mut s, 1);
        assert_eq!(ids(&s), ["system:m"]);
        set_shown(&mut s, "search", false);
        assert!(!order(&s).iter().find(|x| x.0 == "search").unwrap().1);
    }

    #[test]
    fn catalog_is_sound() {
        for d in CATALOG {
            assert!(d.sizes.contains(&d.default), "{}", d.id);
            assert!(GROUPS.contains(&d.group), "{}", d.id);
            assert_eq!(CATALOG.iter().filter(|x| x.id == d.id).count(), 1, "{}", d.id);
        }
    }
}
