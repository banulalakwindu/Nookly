//! Settings > Features: what Nookly has switched on, the presets for
//! it, keyboard shortcuts, and moving settings to another PC.
//!
//! A feature is a panel (its key is the module title) or a background
//! feature without a tab ("notifs", "dots"). A panel that's off has no tab,
//! no reminders and is not ticked, so it costs nothing.

use crate::store::Settings;
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_NOREPEAT};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{FileOpenDialog, FileSaveDialog, IFileOpenDialog, IFileSaveDialog, SHCreateItemFromParsingName, IShellItem, SIGDN_FILESYSPATH, FOS_FORCEFILESYSTEM, FOS_OVERWRITEPROMPT};

/// One switch on the Features page.
pub struct Feature {
    pub key: &'static str,
    /// 0 Everyday, 1 Focus & health, 2 Work & tools, 3 System.
    pub group: u8,
    /// Shown under the grid while the mouse is over it.
    pub what: &'static str,
}

pub const GROUPS: [&str; 4] = ["Everyday", "Files & tools", "Focus & health", "Work & system"];

const fn f(key: &'static str, group: u8, what: &'static str) -> Feature {
    Feature { key, group, what }
}

pub const FEATURES: [Feature; 26] = [
    f("Now Playing", 0, "Song, controls and lyrics; the album art shows on the notch while music plays."),
    f("Timer", 0, "Timer and stopwatch, with the countdown on the closed notch."),
    f("TodoList", 0, "Tasks with reminders and due times."),
    f("Calendar", 0, "The month at a glance, your tasks' due dates and, if you link it, your calendar's events."),
    f("Notes", 0, "A scratchpad that saves as you type."),
    f("Countdowns", 0, "Exams, birthdays, Eid, salary day: \"12 days\" on the notch."),
    f("downloads", 0, "When a download finishes, the notch shows it; click to open it."),
    f("Weather", 0, "Now, the hours around it and the week ahead for your place (goes online only while it's on)."),
    f("Shelf", 1, "Drop files on the notch to keep them close; screenshots land here too."),
    f("Clipboard", 1, "History of what you copy and snippets; click one to paste it."),
    f("Launcher", 1, "Pinned apps, folders and sites, one click away."),
    f("Tools", 1, "Color picker, calculator, QR code, converter and time zones."),
    f("Mirror", 1, "A quick camera preview before a call. The camera is on only while it's open."),
    f("Toggles", 1, "Wi-Fi, Bluetooth, dark mode and microphone switches, plus Night light and hotspot."),
    f("Commands", 1, "Your own commands on a schedule, with what they print on the notch."),
    f("Volume", 1, "The master volume and each app's own, with mute."),
    f("Pomodoro", 2, "Focus and break sessions, with the time left on the notch."),
    f("Water Reminder", 2, "Reminds you to drink and counts the glasses."),
    f("Prayer Times", 2, "Prayer times for your city, reminders, a countdown and Ramadan mode."),
    f("Health", 2, "Screen time today, eye breaks, stand-up reminders and battery care."),
    f("AI Agents", 3, "Claude Code, Codex and Hermes: working, needs you, done."),
    f("meetings", 3, "Your next meeting on the notch with a Join button (link a calendar in the Calendar tab)."),
    f("notifs", 3, "Other apps' notifications in the notch, and today's list in their own tab."),
    f("Widgets", 3, "Pick what the closed notch shows: battery, weather, Wi-Fi and more."),
    f("System Monitor", 3, "CPU, memory and network with one-minute graphs."),
    f("dots", 3, "A dot beside the notch while the microphone or camera is in use."),
];

/// Name and icon of the background features (panels use their module's).
pub fn background(key: &str) -> Option<(&'static str, &'static str)> {
    match key {
        "notifs" => Some(("Notifications", crate::gfx::glyph::BELL)),
        "dots" => Some(("Mic & camera", "\u{E720}")),
        "downloads" => Some(("Downloads", "\u{E896}")),
        "meetings" => Some(("Next meeting", "\u{E8F2}")),
        _ => None,
    }
}

/// Starting points: (name, what's in it, the features that are on).
/// Windows notifications stay as they are: turning them on asks Windows.
pub const PRESETS: [(&str, &str, &[&str]); 4] = [
    ("Minimal", "Music, timer, to-do, calendar and widgets", &["Now Playing", "Timer", "TodoList", "Calendar", "Widgets", "dots"]),
    (
        "Student",
        "Minimal, plus notes, Pomodoro, water, shelf and tools",
        &["Now Playing", "Timer", "TodoList", "Calendar", "Widgets", "dots", "Notes", "Countdowns", "Pomodoro", "Water Reminder", "Shelf", "Clipboard", "Tools", "Health"],
    ),
    (
        "Work",
        "Minimal, plus meetings, notes, launcher, tools and agents",
        &["Now Playing", "Timer", "TodoList", "Calendar", "Widgets", "dots", "Notes", "meetings", "downloads", "Pomodoro", "Shelf", "Clipboard", "Launcher", "Tools", "Toggles", "AI Agents", "System Monitor", "Health"],
    ),
    (
        "Everything",
        "Every feature",
        &[
            "Now Playing", "Timer", "TodoList", "Calendar", "Notes", "Countdowns", "downloads", "Shelf", "Clipboard", "Launcher", "Tools", "Mirror", "Toggles", "Pomodoro", "Water Reminder",
            "Prayer Times", "Health", "AI Agents", "meetings", "Widgets", "System Monitor", "dots", "Commands", "Weather", "Volume",
        ],
    ),
];

/// Background features without a tab, switched in `extra` (feat_<key>).
const BACKGROUND: [&str; 2] = ["downloads", "meetings"];

/// The live activity a feature brings to the closed notch.
fn live_of(key: &str) -> Option<&'static str> {
    match key {
        "downloads" => Some("downloads"),
        "meetings" => Some("meeting"),
        _ => None,
    }
}

pub fn is_on(st: &Settings, key: &str) -> bool {
    match key {
        "notifs" => st.notif_on,
        "dots" => st.privacy_dots,
        k if BACKGROUND.contains(&k) => st.extra.get(&format!("feat_{k}")).map_or(false, |v| v == "1"),
        k => !st.panel_hidden(k),
    }
}

pub fn set_on(st: &mut Settings, key: &str, on: bool) {
    // Switching on something with a live activity puts it on the notch.
    if let Some(id) = live_of(key) {
        if on && !is_on(st, key) {
            st.layout.add(id);
        }
    }
    match key {
        "notifs" => st.notif_on = on,
        "dots" => st.privacy_dots = on,
        k if BACKGROUND.contains(&k) => {
            st.extra.insert(format!("feat_{k}"), if on { "1" } else { "0" }.into());
        }
        k => {
            st.hidden_panels.retain(|t| t != k);
            if !on {
                st.hidden_panels.push(k.to_string());
            }
        }
    }
}

/// Switch on exactly the preset's features (notifications stay as they are).
pub fn apply_preset(st: &mut Settings, k: usize) {
    let list = PRESETS[k].2;
    for f in FEATURES.iter().filter(|f| f.key != "notifs") {
        set_on(st, f.key, list.contains(&f.key));
    }
}

/// The preset that matches what's on now, if any.
pub fn current_preset(st: &Settings) -> Option<usize> {
    (0..PRESETS.len()).find(|&k| FEATURES.iter().filter(|f| f.key != "notifs").all(|f| is_on(st, f.key) == PRESETS[k].2.contains(&f.key)))
}

// ------------------------------------------------------------- shortcuts

/// The keyboard shortcuts: (settings key, name, default).
/// A shortcut is "mods:vk" (MOD_ALT 1, MOD_CONTROL 2, MOD_SHIFT 4,
/// MOD_WIN 8); empty is none.
pub const HOTKEYS: [(&str, &str, &str); 4] = [
    ("hotkey_open", "Open or close the notch", "9:78"), // Win+Alt+N
    ("hotkey_clip", "Clipboard history", ""),
    ("hotkey_shot", "Screenshot", ""),
    ("hotkey_note", "Quick note", ""),
];

/// Hotkey ids passed to RegisterHotKey are 1 + index.
pub fn hotkey(st: &Settings, k: usize) -> Option<(u32, u32)> {
    let (key, _, def) = HOTKEYS[k];
    let s = st.extra.get(key).map_or(def, |v| v.as_str());
    let (m, v) = s.split_once(':')?;
    Some((m.parse().ok()?, v.parse().ok()?)).and_then(|(m, v)| recorded(m, v))
}

pub fn set_hotkey(st: &mut Settings, k: usize, key: Option<(u32, u32)>) {
    st.extra.insert(HOTKEYS[k].0.into(), key.map_or(String::new(), |(m, v)| format!("{m}:{v}")));
}

/// "Win+Alt+N".
pub fn hotkey_label(mods: u32, vk: u32) -> String {
    let mut s = String::new();
    for (bit, name) in [(8, "Win"), (2, "Ctrl"), (1, "Alt"), (4, "Shift")] {
        if mods & bit != 0 {
            s += name;
            s += "+";
        }
    }
    s + &key_name(vk)
}

fn key_name(vk: u32) -> String {
    match vk {
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(vk).map(String::from).unwrap_or_default(),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        0x20 => "Space".into(),
        0x0D => "Enter".into(),
        0x09 => "Tab".into(),
        0x2C => "PrtScn".into(),
        0x25 => "Left".into(),
        0x26 => "Up".into(),
        0x27 => "Right".into(),
        0x28 => "Down".into(),
        0x2D => "Insert".into(),
        0x24 => "Home".into(),
        0x23 => "End".into(),
        0x21 => "PgUp".into(),
        0x22 => "PgDn".into(),
        0xBA => ";".into(),
        0xBB => "=".into(),
        0xBC => ",".into(),
        0xBD => "-".into(),
        0xBE => ".".into(),
        0xBF => "/".into(),
        0xC0 => "`".into(),
        0xDB => "[".into(),
        0xDC => "\\".into(),
        0xDD => "]".into(),
        0xDE => "'".into(),
        v => format!("Key {v}"),
    }
}

/// A key pressed while recording a shortcut: the modifiers held and the
/// key. Modifier keys alone don't count; a shortcut needs Ctrl, Alt or Win
/// (Shift alone would steal typing), except F1-F24.
pub fn recorded(mods: u32, vk: u32) -> Option<(u32, u32)> {
    let modifier = matches!(vk, 0x10 | 0x11 | 0x12 | 0x5B | 0x5C | 0xA0..=0xA5);
    if modifier {
        return None;
    }
    let fkey = (0x70..=0x87).contains(&vk);
    (mods & 0xB != 0 || fkey).then_some((mods, vk))
}

/// Register every shortcut; returns which ones Windows refused (taken by
/// another app).
pub fn register(hwnd: HWND, st: &Settings) -> [bool; 4] {
    let mut fail = [false; 4];
    for k in 0..HOTKEYS.len() {
        unsafe {
            let _ = UnregisterHotKey(Some(hwnd), 1 + k as i32);
        }
        if let Some((m, v)) = hotkey(st, k) {
            let ok = unsafe { RegisterHotKey(Some(hwnd), 1 + k as i32, HOT_KEY_MODIFIERS(m) | MOD_NOREPEAT, v) }.is_ok();
            fail[k] = !ok;
        }
    }
    fail
}

// ------------------------------------------------------- export / import

/// The data files, in the order they're written.
pub const DATA_FILES: [&str; 7] = ["settings.txt", "todo.txt", "water.txt", "prayer.txt", "tools.txt", "shelf.txt", "clipboard.txt"];
const HEADER: &str = "Nookly settings export 1";
const MARK: &str = "=== Nookly file: ";

/// Every data file in one text file.
pub fn bundle() -> String {
    let mut out = format!("{HEADER}\n");
    for f in DATA_FILES {
        let body = crate::store::read(f);
        if body.is_empty() {
            continue;
        }
        out += &format!("{MARK}{f} ===\n");
        out += &body;
        if !body.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

/// The files in an export, or None if it isn't one. Only the known data
/// files are taken.
pub fn unbundle(text: &str) -> Option<Vec<(String, String)>> {
    let text = text.trim_start_matches('\u{FEFF}');
    let mut lines = text.lines();
    if lines.next()?.trim() != HEADER {
        return None;
    }
    let mut files: Vec<(String, String)> = Vec::new();
    for line in lines {
        if let Some(name) = line.strip_prefix(MARK).and_then(|r| r.strip_suffix(" ===")) {
            if DATA_FILES.contains(&name) && !files.iter().any(|(n, _)| n == name) {
                files.push((name.to_string(), String::new()));
            } else {
                // Unknown file: skip its lines.
                files.push((String::new(), String::new()));
            }
        } else if let Some((_, body)) = files.last_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }
    files.retain(|(n, _)| !n.is_empty());
    files.iter().any(|(n, _)| n == "settings.txt").then_some(files)
}

fn dialog_path(item: windows::core::Result<IShellItem>) -> Option<String> {
    let item = item.ok()?;
    let p = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }.ok()?;
    let s = unsafe { p.to_string() }.ok();
    unsafe { windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const _)) };
    s
}

fn filter() -> [COMDLG_FILTERSPEC; 2] {
    [
        COMDLG_FILTERSPEC { pszName: w!("Nookly settings"), pszSpec: w!("*.nookly") },
        COMDLG_FILTERSPEC { pszName: w!("All files"), pszSpec: w!("*.*") },
    ]
}

/// Ask where to save the export (Documents by default). Must run outside
/// the app's borrow: the dialog pumps messages.
pub fn save_dialog(owner: HWND) -> Option<String> {
    unsafe {
        let d: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let _ = d.SetFileTypes(&filter());
        let _ = d.SetDefaultExtension(w!("nookly"));
        let name = HSTRING::from(format!("Nookly settings {}.nookly", { let (y, m, d) = crate::time::local_date(crate::time::unix_now()); format!("{y}-{m:02}-{d:02}") }));
        let _ = d.SetFileName(PCWSTR(name.as_ptr()));
        let _ = d.SetTitle(w!("Export Nookly settings"));
        if let Ok(o) = d.GetOptions() {
            let _ = d.SetOptions(o | FOS_FORCEFILESYSTEM | FOS_OVERWRITEPROMPT);
        }
        if let Some(docs) = known_documents() {
            let _ = d.SetFolder(&docs);
        }
        d.Show(Some(owner)).ok()?;
        dialog_path(d.GetResult())
    }
}

pub fn open_dialog(owner: HWND) -> Option<String> {
    unsafe {
        let d: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let _ = d.SetFileTypes(&filter());
        let _ = d.SetTitle(w!("Import Nookly settings"));
        if let Ok(o) = d.GetOptions() {
            let _ = d.SetOptions(o | FOS_FORCEFILESYSTEM);
        }
        if let Some(docs) = known_documents() {
            let _ = d.SetFolder(&docs);
        }
        d.Show(Some(owner)).ok()?;
        dialog_path(d.GetResult())
    }
}

fn known_documents() -> Option<IShellItem> {
    let home = std::env::var("USERPROFILE").ok()?;
    let p = HSTRING::from(format!("{home}\\Documents"));
    unsafe { SHCreateItemFromParsingName(PCWSTR(p.as_ptr()), None).ok() }
}

// -------------------------------------------------------------- profiles

/// Profiles: each keeps its own features, closed-notch widgets and theme.
pub const PROFILES: [&str; 3] = ["Home", "Work", "Focus"];
const SEP: char = '\u{1F}';

/// What a profile keeps, as one line.
fn snapshot(st: &Settings) -> String {
    let feats: Vec<String> = BACKGROUND.iter().map(|k| format!("{k}={}", is_on(st, k) as u8)).collect();
    [
        st.hidden_panels.join("/"),
        st.theme.clone(),
        st.layout.to_text(),
        st.layout.picked.join(","),
        (st.layout.custom as u8).to_string(),
        feats.join(","),
        (st.seconds as u8).to_string(),
    ]
    .join(&SEP.to_string())
}

fn restore(st: &mut Settings, snap: &str) {
    let p: Vec<&str> = snap.split(SEP).collect();
    if p.len() < 7 {
        return;
    }
    st.hidden_panels = p[0].split('/').filter(|s| !s.is_empty()).map(String::from).collect();
    st.theme = p[1].to_string();
    st.layout.rows = crate::store::NotchLayout::from_text(p[2]);
    st.layout.picked = p[3].split(',').filter(|s| !s.is_empty()).map(String::from).collect();
    st.layout.custom = p[4] == "1";
    for kv in p[5].split(',') {
        if let Some((k, v)) = kv.split_once('=') {
            if BACKGROUND.contains(&k) {
                st.extra.insert(format!("feat_{k}"), v.to_string());
            }
        }
    }
    st.seconds = p[6] == "1";
}

pub fn active_profile(st: &Settings) -> String {
    st.extra.get("profile").cloned().unwrap_or_else(|| PROFILES[0].to_string())
}

/// Keep what's set now in the current profile, then load `name` (a profile
/// never used starts as a copy of the current one).
pub fn switch_profile(st: &mut Settings, name: &str) {
    let cur = active_profile(st);
    if cur == name {
        return;
    }
    let snap = snapshot(st);
    st.extra.insert(format!("profile_{cur}"), snap.clone());
    let next = st.extra.get(&format!("profile_{name}")).cloned().unwrap_or(snap);
    restore(st, &next);
    st.extra.insert("profile".into(), name.to_string());
}

/// With Auto on: Work on weekdays from 9 to 5, Home otherwise (Focus is
/// only ever picked by hand, and Auto leaves it alone).
pub fn auto_profile(st: &Settings, weekday: u32, hour: u32) -> Option<&'static str> {
    if st.extra.get("profile_auto").map_or(true, |v| v != "1") || active_profile(st) == "Focus" {
        return None;
    }
    let work = (1..=5).contains(&weekday) && (9..17).contains(&hour);
    let want = if work { "Work" } else { "Home" };
    (active_profile(st) != want).then_some(want)
}

/// Pick apps and files (or, with `folder`, folders) to pin; several at once.
pub fn pick_paths(owner: HWND, folder: bool) -> Vec<String> {
    use windows::Win32::UI::Shell::{IShellItemArray, FOS_ALLOWMULTISELECT, FOS_PICKFOLDERS};
    unsafe {
        let Ok(d) = CoCreateInstance::<_, IFileOpenDialog>(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) else { return Vec::new() };
        let _ = d.SetTitle(if folder { w!("Pin folders") } else { w!("Pin apps and files") });
        if !folder {
            let f = [
                COMDLG_FILTERSPEC { pszName: w!("Apps and shortcuts"), pszSpec: w!("*.exe;*.lnk;*.url;*.appref-ms") },
                COMDLG_FILTERSPEC { pszName: w!("All files"), pszSpec: w!("*.*") },
            ];
            let _ = d.SetFileTypes(&f);
            // Start in the Start menu, where the app shortcuts are.
            if let Ok(pd) = std::env::var("ProgramData") {
                let p = HSTRING::from(format!("{pd}\\Microsoft\\Windows\\Start Menu\\Programs"));
                if let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(PCWSTR(p.as_ptr()), None) {
                    let _ = d.SetFolder(&item);
                }
            }
        }
        if let Ok(o) = d.GetOptions() {
            let _ = d.SetOptions(o | FOS_ALLOWMULTISELECT | if folder { FOS_PICKFOLDERS } else { FOS_FORCEFILESYSTEM });
        }
        if d.Show(Some(owner)).is_err() {
            return Vec::new();
        }
        let Ok(arr): windows::core::Result<IShellItemArray> = d.GetResults() else { return Vec::new() };
        let n = arr.GetCount().unwrap_or(0);
        (0..n).filter_map(|i| dialog_path(arr.GetItemAt(i))).collect()
    }
}

/// Pick an .ics calendar file.
pub fn pick_ics(owner: HWND) -> Option<String> {
    unsafe {
        let d: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let f = [
            COMDLG_FILTERSPEC { pszName: w!("Calendar files"), pszSpec: w!("*.ics") },
            COMDLG_FILTERSPEC { pszName: w!("All files"), pszSpec: w!("*.*") },
        ];
        let _ = d.SetFileTypes(&f);
        let _ = d.SetTitle(w!("Choose a calendar file"));
        if let Ok(o) = d.GetOptions() {
            let _ = d.SetOptions(o | FOS_FORCEFILESYSTEM);
        }
        if let Some(docs) = known_documents() {
            let _ = d.SetFolder(&docs);
        }
        d.Show(Some(owner)).ok()?;
        dialog_path(d.GetResult())
    }
}
