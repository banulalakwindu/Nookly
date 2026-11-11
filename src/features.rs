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

pub const GROUPS: [&str; 4] = ["Everyday", "Focus & health", "Work & tools", "System"];

const fn f(key: &'static str, group: u8, what: &'static str) -> Feature {
    Feature { key, group, what }
}

pub const FEATURES: [Feature; 15] = [
    f("Now Playing", 0, "Song, controls and lyrics; the album art shows on the notch while music plays."),
    f("Timer", 0, "Timer and stopwatch, with the countdown on the closed notch."),
    f("TodoList", 0, "Tasks with reminders and due times."),
    f("Calendar", 0, "The month at a glance, with the days your tasks are due."),
    f("Shelf", 0, "Drop files on the notch to keep them close; screenshots land here too."),
    f("Clipboard", 0, "History of what you copy and snippets; click one to paste it."),
    f("Pomodoro", 1, "Focus and break sessions, with the time left on the notch."),
    f("Water Reminder", 1, "Reminds you to drink and counts the glasses."),
    f("Prayer Times", 1, "Prayer times for your city, with reminders and a countdown."),
    f("AI Agents", 2, "Claude Code, Codex and Hermes: working, needs you, done."),
    f("Tools", 2, "Color picker, calculator, QR code, converter and time zones."),
    f("notifs", 2, "Other apps' notifications in the notch instead of the corner of the screen."),
    f("Widgets", 3, "Pick what the closed notch shows: battery, weather, Wi-Fi and more."),
    f("System Monitor", 3, "CPU, memory and network with one-minute graphs."),
    f("dots", 3, "A dot beside the notch while the microphone or camera is in use."),
];

/// Name and icon of the background features (panels use their module's).
pub fn background(key: &str) -> Option<(&'static str, &'static str)> {
    match key {
        "notifs" => Some(("Notifications", crate::gfx::glyph::BELL)),
        "dots" => Some(("Mic & camera", "\u{E720}")),
        _ => None,
    }
}

/// Starting points: (name, what's in it, the features that are on).
/// Windows notifications stay as they are: turning them on asks Windows.
pub const PRESETS: [(&str, &str, &[&str]); 4] = [
    ("Minimal", "Music, timer, to-do and calendar", &["Now Playing", "Timer", "TodoList", "Calendar", "Widgets", "dots"]),
    (
        "Student",
        "Minimal, plus Pomodoro, water, shelf, clipboard and tools",
        &["Now Playing", "Timer", "TodoList", "Calendar", "Widgets", "dots", "Pomodoro", "Water Reminder", "Shelf", "Clipboard", "Tools"],
    ),
    (
        "Work",
        "Minimal, plus Pomodoro, shelf, clipboard, tools, AI agents and monitor",
        &["Now Playing", "Timer", "TodoList", "Calendar", "Widgets", "dots", "Pomodoro", "Shelf", "Clipboard", "Tools", "AI Agents", "System Monitor"],
    ),
    (
        "Everything",
        "Every panel",
        &["Now Playing", "Timer", "TodoList", "Calendar", "Shelf", "Clipboard", "Pomodoro", "Water Reminder", "Prayer Times", "AI Agents", "Tools", "Widgets", "System Monitor", "dots"],
    ),
];

pub fn is_on(st: &Settings, key: &str) -> bool {
    match key {
        "notifs" => st.notif_on,
        "dots" => st.privacy_dots,
        k => !st.panel_hidden(k),
    }
}

pub fn set_on(st: &mut Settings, key: &str, on: bool) {
    match key {
        "notifs" => st.notif_on = on,
        "dots" => st.privacy_dots = on,
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
pub const HOTKEYS: [(&str, &str, &str); 3] = [
    ("hotkey_open", "Open notch", "9:78"), // Win+Alt+N
    ("hotkey_clip", "Clipboard", ""),
    ("hotkey_shot", "Screenshot", ""),
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
pub fn register(hwnd: HWND, st: &Settings) -> [bool; 3] {
    let mut fail = [false; 3];
    for k in 0..HOTKEYS.len() {
        unsafe {
            let _ = UnregisterHotKey(hwnd, 1 + k as i32);
        }
        if let Some((m, v)) = hotkey(st, k) {
            let ok = unsafe { RegisterHotKey(hwnd, 1 + k as i32, HOT_KEY_MODIFIERS(m) | MOD_NOREPEAT, v) }.is_ok();
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
        d.Show(owner).ok()?;
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
        d.Show(owner).ok()?;
        dialog_path(d.GetResult())
    }
}

fn known_documents() -> Option<IShellItem> {
    let home = std::env::var("USERPROFILE").ok()?;
    let p = HSTRING::from(format!("{home}\\Documents"));
    unsafe { SHCreateItemFromParsingName(PCWSTR(p.as_ptr()), None).ok() }
}
