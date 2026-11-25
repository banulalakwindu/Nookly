//! Health: screen time today (counted only while you use the PC, from the
//! app in front; nothing leaves your PC), eye breaks (20-20-20), stand-up
//! reminders and battery care. Reminders wait while you're in a game, a
//! full-screen video or a call, then catch up.

use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

use crate::gfx::*;
use crate::modules::{Ctx, Module, Rel, Toast, Widget};
use crate::store;
use crate::ui::Ui;

const A_EYE: u32 = 1;
const A_STAND: u32 = 2;
const A_BATTERY: u32 = 3;
const A_STAND_MIN: u32 = 10; // + index into STAND_MINS
const A_DONE: u32 = 20;
const A_SNOOZE: u32 = 21;
const STAND_MINS: [u32; 4] = [30, 45, 60, 90];
/// Away this long: the clock for breaks starts again.
const AWAY: u32 = 5 * 60;
const C_HEALTH: Color = Color { r: 0.40, g: 0.85, b: 0.60, a: 1.0 };

pub struct Health {
    day: String,
    total: u32,
    apps: Vec<(String, u32)>,
    saved_at: f64,
    counted_at: f64,
    /// Seconds used since the last eye break / stand-up.
    eye_run: u32,
    stand_run: u32,
    /// A reminder waiting for a quiet moment: (key, title, sub).
    waiting: Vec<(&'static str, String, String)>,
    battery_told: bool,
    pid: u32,
    pid_name: String,
    eye: bool,
    stand: bool,
    stand_min: u32,
    battery: bool,
}

fn idle_secs() -> u32 {
    let mut li = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    if unsafe { GetLastInputInfo(&mut li) }.as_bool() {
        unsafe { GetTickCount() }.wrapping_sub(li.dwTime) / 1000
    } else {
        0
    }
}

/// A friendlier name for an app's exe.
pub fn friendly(exe: &str) -> String {
    let l = exe.to_lowercase();
    let known = [
        ("msedge", "Edge"),
        ("chrome", "Chrome"),
        ("firefox", "Firefox"),
        ("code", "VS Code"),
        ("explorer", "File Explorer"),
        ("windowsterminal", "Terminal"),
        ("winword", "Word"),
        ("excel", "Excel"),
        ("powerpnt", "PowerPoint"),
        ("outlook", "Outlook"),
        ("olk", "Outlook"),
        ("ms-teams", "Teams"),
        ("teams", "Teams"),
        ("spotify", "Spotify"),
        ("discord", "Discord"),
        ("slack", "Slack"),
        ("whatsapp", "WhatsApp"),
        ("devenv", "Visual Studio"),
        ("nookly", "Nookly"),
    ];
    if let Some((_, n)) = known.iter().find(|(k, _)| *k == l) {
        return n.to_string();
    }
    let mut c = exe.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// The exe name (without .exe) of the window in front.
pub fn front_app(cache: &mut (u32, String)) -> Option<String> {
    unsafe {
        let h: HWND = GetForegroundWindow();
        if h.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        if pid == cache.0 {
            return Some(cache.1.clone());
        }
        let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 520];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
        let _ = CloseHandle(p);
        if !ok {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..n as usize]);
        let stem = std::path::Path::new(&path).file_stem()?.to_string_lossy().into_owned();
        *cache = (pid, stem.clone());
        Some(stem)
    }
}

pub fn fmt_hm(secs: u32) -> String {
    let m = secs / 60;
    if m < 60 {
        format!("{m} min")
    } else {
        format!("{} h {} m", m / 60, m % 60)
    }
}

impl Health {
    pub fn load() -> Health {
        let mut h = Health {
            day: String::new(),
            total: 0,
            apps: Vec::new(),
            saved_at: 0.0,
            counted_at: 0.0,
            eye_run: 0,
            stand_run: 0,
            waiting: Vec::new(),
            battery_told: false,
            pid: 0,
            pid_name: String::new(),
            eye: true,
            stand: true,
            stand_min: 60,
            battery: true,
        };
        if crate::time::demo() {
            h.total = 4 * 3600 + 12 * 60;
            h.apps = vec![("VS Code".into(), 7300), ("Chrome".into(), 4200), ("Teams".into(), 2400), ("Spotify".into(), 1200)];
            return h;
        }
        // screentime.txt: day|total|app=secs,app=secs
        let s = store::read("screentime.txt");
        let mut p = s.trim().splitn(3, '|');
        if let (Some(day), Some(total), Some(apps)) = (p.next(), p.next(), p.next()) {
            h.day = day.to_string();
            h.total = total.parse().unwrap_or(0);
            h.apps = apps.split(',').filter_map(|kv| kv.rsplit_once('=')).map(|(k, v)| (k.to_string(), v.parse().unwrap_or(0))).collect();
        }
        h
    }
    fn save(&self) {
        let apps: Vec<String> = self.apps.iter().map(|(k, v)| format!("{k}={v}")).collect();
        store::write("screentime.txt", &format!("{}|{}|{}", self.day, self.total, apps.join(",")));
    }
    fn top(&self, n: usize) -> Vec<(String, u32)> {
        let mut v = self.apps.clone();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v.truncate(n);
        v
    }
    fn remind(&mut self, ctx: &mut Ctx, key: &'static str, title: &str, sub: &str) {
        // Quiet while busy: full screen (game, video) or a call.
        if ctx.hidden || crate::modules::privacy::IN_CALL.load(std::sync::atomic::Ordering::Relaxed) {
            if !self.waiting.iter().any(|w| w.0 == key) {
                self.waiting.push((key, title.into(), sub.into()));
            }
            return;
        }
        ctx.toast(Toast::new(key, "\u{E95E}", C_HEALTH, title, sub).action("Done", A_DONE).action("In 5 min", A_SNOOZE).secs(15.0).chime());
    }
    /// Count one second of use (for the self-test too).
    pub fn count(&mut self, app: &str, day: &str) {
        if self.day != day {
            self.day = day.to_string();
            self.total = 0;
            self.apps.clear();
        }
        self.total += 1;
        match self.apps.iter_mut().find(|(k, _)| k == app) {
            Some(e) => e.1 += 1,
            None => self.apps.push((app.to_string(), 1)),
        }
    }
    pub fn today_total(&self) -> u32 {
        self.total
    }
}

impl Module for Health {
    fn title(&self) -> &'static str {
        "Health"
    }
    fn icon(&self) -> &'static str {
        "\u{E95E}"
    }
    fn accent(&self) -> Color {
        C_HEALTH
    }
    fn size(&self) -> (f32, f32) {
        (500.0, 170.0)
    }
    fn tick(&mut self, ctx: &mut Ctx) {
        let x = &ctx.settings.extra;
        let flag = |k: &str| x.get(k).map_or(true, |v| v == "1");
        self.eye = flag("health_eye");
        self.stand = flag("health_stand");
        self.battery = flag("health_battery");
        self.stand_min = x.get("health_stand_min").and_then(|v| v.parse().ok()).unwrap_or(60);
        if crate::time::demo() {
            return;
        }
        // Ticks also come in between (a wake-up): a second is counted once.
        let clock = crate::time::unix_now();
        if clock - self.counted_at < 0.8 && clock >= self.counted_at {
            return;
        }
        self.counted_at = clock;
        let idle = idle_secs();
        let day = ctx.now.day_key();
        if idle < 120 {
            let mut cache = (self.pid, std::mem::take(&mut self.pid_name));
            if let Some(app) = front_app(&mut cache) {
                self.count(&friendly(&app), &day);
            }
            (self.pid, self.pid_name) = cache;
            self.eye_run += 1;
            self.stand_run += 1;
        } else if idle >= AWAY {
            // Away from the PC is a break.
            self.eye_run = 0;
            self.stand_run = 0;
        }
        let now = crate::time::unix_now();
        if now - self.saved_at > 60.0 {
            self.saved_at = now;
            self.save();
        }
        // Waiting reminders, once it's quiet.
        if !self.waiting.is_empty() && !ctx.hidden && !crate::modules::privacy::IN_CALL.load(std::sync::atomic::Ordering::Relaxed) {
            for (key, title, sub) in std::mem::take(&mut self.waiting) {
                ctx.toast(Toast::new(key, "\u{E95E}", C_HEALTH, title, sub).action("Done", A_DONE).secs(15.0).chime());
            }
        }
        if self.eye && self.eye_run >= 20 * 60 {
            self.eye_run = 0;
            self.remind(ctx, "eyes", "Rest your eyes", "Look at something about 6 m away for 20 seconds");
        }
        if self.stand && self.stand_run >= self.stand_min * 60 {
            self.stand_run = 0;
            self.remind(ctx, "stand", "Time to stand up", &format!("You've been at it for {}. Stretch for a minute.", fmt_hm(self.stand_min * 60)));
        }
        if self.battery {
            if let Some(p) = crate::modules::widgets::power() {
                if p.on_ac && p.pct >= 80 && !self.battery_told {
                    self.battery_told = true;
                    ctx.toast(Toast::new("battery_care", glyph::CHARGING, C_HEALTH, "Unplug at 80%", "Your battery lasts longer when it isn't kept full").right(format!("{}%", p.pct)).secs(10.0));
                } else if !p.on_ac {
                    self.battery_told = false;
                }
            }
        }
    }
    fn draw(&mut self, ui: &mut Ui, r: Rect) {
        let mut r = r;
        let mut right = r.cut_right(230.0, 16.0);
        // Screen time.
        let mut l = r;
        ui.text("Screen time today", l.cut_top(16.0, 2.0), 11.5, col::DIM, Weight::Semi, Align::Left);
        ui.text(&fmt_hm(self.total), l.cut_top(34.0, 6.0), 26.0, col::TEXT, Weight::Bold, Align::Left);
        let top = self.top(4);
        let max = top.first().map_or(1, |t| t.1).max(1);
        for (name, secs) in top {
            let mut row = l.cut_top(18.0, 4.0);
            let t = fmt_hm(secs);
            ui.text(&t, row.cut_right(62.0, 4.0), 10.5, col::DIM, Weight::Regular, Align::Right);
            let mut nm = row.cut_left(90.0, 6.0);
            nm.w = nm.w.max(0.0);
            ui.text(&name, nm, 11.0, col::TEXT, Weight::Regular, Align::Left);
            let bar = row.centered(row.w, 6.0);
            ui.fill_rr(bar, 3.0, col::TEXT.with_a(0.1));
            ui.fill_rr(Rect::new(bar.x, bar.y, (bar.w * secs as f32 / max as f32).max(6.0), bar.h), 3.0, C_HEALTH);
        }
        // Reminders.
        ui.toggle(right.cut_top(26.0, 2.0), "Eye breaks (20-20-20)", self.eye, A_EYE, C_HEALTH);
        ui.toggle(right.cut_top(26.0, 4.0), "Stand-up reminders", self.stand, A_STAND, C_HEALTH);
        let row = right.cut_top(24.0, 8.0);
        for (k, (cell, m)) in row.columns(4, 4.0).into_iter().zip(STAND_MINS).enumerate() {
            ui.chip(cell, &format!("{m} m"), A_STAND_MIN + k as u32, self.stand_min == m && self.stand, C_HEALTH);
        }
        ui.toggle(right.cut_top(26.0, 4.0), "Battery care (unplug at 80%)", self.battery, A_BATTERY, C_HEALTH);
        ui.text_wrap("Reminders wait while you're in a game, a full-screen video or a call.", right, 10.0, col::FAINT, Weight::Regular, Align::Left);
    }
    fn action(&mut self, id: u32, _rel: Rel, ctx: &mut Ctx) {
        let x = &mut ctx.settings.extra;
        let mut flip = |k: &str, v: bool| {
            x.insert(k.to_string(), if v { "0" } else { "1" }.into());
        };
        match id {
            A_EYE => flip("health_eye", self.eye),
            A_STAND => flip("health_stand", self.stand),
            A_BATTERY => flip("health_battery", self.battery),
            i if (A_STAND_MIN..A_STAND_MIN + 4).contains(&i) => {
                x.insert("health_stand_min".into(), STAND_MINS[(i - A_STAND_MIN) as usize].to_string());
                x.insert("health_stand".into(), "1".into());
                self.stand_run = 0;
            }
            A_DONE => {}
            A_SNOOZE => {
                // Five minutes from now.
                self.eye_run = self.eye_run.min(15 * 60);
                self.stand_run = self.stand_run.min(self.stand_min.saturating_sub(5) * 60);
            }
            _ => {}
        }
        ctx.settings.save();
    }
    fn widgets(&mut self) -> Vec<Widget> {
        vec![Widget::new("screentime", "Screen time", "\u{E95E}", C_HEALTH, fmt_hm(self.total).replace(" min", "m").replace(" h ", "h ").replace(" m", "m")).detail(format!("{} at the PC today", fmt_hm(self.total)))]
    }
    fn tip(&self, id: u32) -> &'static str {
        match id {
            A_EYE => "Every 20 minutes at the PC: look 6 m away for 20 seconds",
            A_STAND => "A reminder to get up and stretch",
            i if (A_STAND_MIN..A_STAND_MIN + 4).contains(&i) => "How often",
            A_BATTERY => "On the charger at 80%: a reminder to unplug",
            _ => "",
        }
    }
    fn debug_state(&self) -> String {
        format!("health total={} apps={}", self.total, self.apps.len())
    }
}
