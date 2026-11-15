//! Commands: run your own commands on a schedule (cmd.exe, no window) and
//! see the first line each one prints, here and as widgets on the closed
//! notch. Nothing runs while the panel is switched off. Saved in
//! commands.txt as `minutes|command` lines.

use std::sync::{Arc, Mutex};

use crate::gfx::*;
use crate::modules::{Ctx, Module, Rel, Widget};
use crate::store;
use crate::time::unix_now;
use crate::ui::{FieldKey, TextField, Ui};

const A_FIELD: u32 = 1;
const A_ADD: u32 = 2;
const A_NEW_LESS: u32 = 3;
const A_NEW_MORE: u32 = 4;
const A_RUN: u32 = 100; // + index
const A_REMOVE: u32 = 120; // + index
const A_LESS: u32 = 140; // + index
const A_MORE: u32 = 160; // + index
/// Click the time to type any number of minutes: + index (MAX: the new one).
const A_EDIT: u32 = 180;
const LONGEST: u32 = 7 * 1440;
pub const MAX: usize = 5;
/// The schedules to choose from, in minutes.
const EVERY: [u32; 10] = [1, 2, 5, 10, 15, 30, 60, 120, 240, 1440];
const IDS: [&str; MAX] = ["cmd1", "cmd2", "cmd3", "cmd4", "cmd5"];
const C_CMD: Color = Color { r: 0.65, g: 0.80, b: 0.95, a: 1.0 };

/// Last result: (first line, when, running).
type Out = Arc<Mutex<(String, f64, bool)>>;

struct Cmd {
    text: String,
    every: u32,
    out: Out,
    last: f64,
}

fn every_text(m: u32) -> String {
    match m {
        m if m < 60 => format!("{m} min"),
        1440 => "Daily".into(),
        m if m % 1440 == 0 => format!("{} days", m / 1440),
        m if m % 60 == 0 => format!("{} h", m / 60),
        m => format!("{} h {} m", m / 60, m % 60),
    }
}

fn step(m: u32, by: i32) -> u32 {
    let k = EVERY.iter().position(|&e| e >= m).unwrap_or(EVERY.len() - 1) as i32;
    EVERY[(k + by).clamp(0, EVERY.len() as i32 - 1) as usize]
}

/// The command's short name: its first word, without a path or .exe.
fn label(cmd: &str) -> String {
    let w = cmd.split_whitespace().next().unwrap_or("Command");
    let w = w.rsplit(['\\', '/']).next().unwrap_or(w);
    w.trim_end_matches(".exe").trim_end_matches(".bat").trim_end_matches(".cmd").to_string()
}

/// Run a command (cmd.exe, no window) and keep its first line of output.
fn run(cmd: String, out: Out) {
    {
        let mut o = out.lock().unwrap();
        if o.2 {
            return;
        }
        o.2 = true;
    }
    std::thread::spawn(move || {
        use std::os::windows::process::CommandExt;
        let res = std::process::Command::new("cmd").args(["/D", "/C", &cmd]).creation_flags(0x0800_0000).output();
        let line: String = match res {
            Ok(o) => {
                let text = if o.stdout.iter().any(|b| !b.is_ascii_whitespace()) { o.stdout } else { o.stderr };
                String::from_utf8_lossy(&text).lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("(no output)").chars().take(80).collect()
            }
            Err(e) => format!("Couldn't run it: {e}"),
        };
        *out.lock().unwrap() = (line, unix_now(), false);
        crate::app::wake();
    });
}

pub struct Commands {
    list: Vec<Cmd>,
    field: TextField,
    new_every: u32,
    /// Typing minutes for a command's schedule: which one (MAX: the new one).
    editing: Option<usize>,
    minutes: TextField,
}

impl Commands {
    /// The schedule in the middle of the arrows: click to type minutes.
    fn every_label(&self, ui: &mut Ui, r: Rect, which: usize, every: u32, size: f32) {
        if self.editing == Some(which) {
            let mut f = self.minutes.clone();
            f.focused = true;
            ui.field(r.inset_xy(0.0, 1.0), &f, "min", A_EDIT + which.min(MAX) as u32, C_CMD);
        } else {
            ui.text(&every_text(every), r, size, if which == MAX { col::TEXT } else { col::DIM }, Weight::Semi, Align::Center);
            ui.hit(r, A_EDIT + which.min(MAX) as u32);
        }
    }
    /// Apply the typed minutes.
    fn commit_minutes(&mut self) {
        if let (Some(w), Ok(m)) = (self.editing.take(), self.minutes.text.trim().parse::<u32>()) {
            let m = m.clamp(1, LONGEST);
            if w == MAX {
                self.new_every = m;
            } else if let Some(c) = self.list.get_mut(w) {
                c.every = m;
                self.save();
            }
        }
        self.minutes.text.clear();
        self.minutes.focused = false;
    }
    pub fn load() -> Commands {
        if crate::time::demo() {
            let now = unix_now();
            let demo = [("curl -s wttr.in/Colombo?format=3", 15, "Colombo: ⛅ +29°C"), ("git -C C:\\code\\app status -sb", 5, "## main...origin/main [ahead 2]"), ("ping -n 1 1.1.1.1 | find \"time=\"", 1, "Reply from 1.1.1.1: time=14ms")];
            let list = demo.iter().map(|&(c, e, o)| Cmd { text: c.into(), every: e, out: Arc::new(Mutex::new((o.into(), now - 120.0, false))), last: now }).collect();
            return Commands { list, field: TextField::new(400), new_every: 5, editing: None, minutes: TextField::new(5) };
        }
        let list = store::read("commands.txt")
            .lines()
            .filter_map(|l| {
                let (e, c) = l.split_once('|')?;
                Some(Cmd { text: store::unesc(c), every: e.parse().ok()?, out: Arc::default(), last: 0.0 })
            })
            .take(MAX)
            .collect();
        Commands { list, field: TextField::new(400), new_every: 5, editing: None, minutes: TextField::new(5) }
    }
    fn save(&self) {
        let s: String = self.list.iter().map(|c| format!("{}|{}\n", c.every, store::esc(&c.text))).collect();
        store::write("commands.txt", &s);
    }
    fn add(&mut self) {
        let t = self.field.text.trim().to_string();
        if t.is_empty() || self.list.len() >= MAX {
            return;
        }
        let out: Out = Arc::default();
        run(t.clone(), out.clone());
        self.list.push(Cmd { text: t, every: self.new_every, out, last: unix_now() });
        self.field.text.clear();
        self.save();
    }
}

impl Module for Commands {
    fn title(&self) -> &'static str {
        "Commands"
    }
    fn icon(&self) -> &'static str {
        "\u{E756}"
    }
    fn accent(&self) -> Color {
        C_CMD
    }
    fn size(&self) -> (f32, f32) {
        (520.0, 176.0)
    }
    fn tick(&mut self, _ctx: &mut Ctx) {
        if crate::time::demo() {
            return;
        }
        let now = unix_now();
        for c in &mut self.list {
            if now - c.last >= c.every as f64 * 60.0 {
                c.last = now;
                run(c.text.clone(), c.out.clone());
            }
        }
    }
    fn draw(&mut self, ui: &mut Ui, mut r: Rect) {
        let mut row = r.cut_top(28.0, 8.0);
        let full = self.list.len() >= MAX;
        ui.button_accent(row.cut_right(60.0, 6.0), "Add", A_ADD, C_CMD);
        let mut ev = row.cut_right(104.0, 6.0);
        ui.fill_rr(ev, 14.0, col::BTN);
        ui.icon_btn(ev.cut_left(24.0, 0.0), "\u{E76B}", A_NEW_LESS, 9.0, col::DIM, None, "Less often");
        ui.icon_btn(ev.cut_right(24.0, 0.0), "\u{E76C}", A_NEW_MORE, 9.0, col::DIM, None, "More often");
        self.every_label(ui, ev, MAX, self.new_every, 11.5);
        ui.field(row, &self.field, if full { "That's 5: remove one to add another" } else { "A command, e.g. curl -s wttr.in/Colombo?format=3" }, A_FIELD, C_CMD);
        if self.list.is_empty() {
            ui.text_wrap(
                "Run your own commands on a schedule and see what they print, here and as widgets on the closed notch (switch them on in Settings > Widgets). They run on this PC with cmd.exe, only while this panel is on.",
                r.inset_xy(4.0, 4.0),
                11.5,
                col::DIM,
                Weight::Regular,
                Align::Left,
            );
            return;
        }
        for (k, cell) in r.rows(MAX.min(4), 4.0).into_iter().enumerate() {
            let Some(c) = self.list.get(k) else { break };
            let (out, at, running) = c.out.lock().unwrap().clone();
            ui.fill_rr(cell, 9.0, col::BTN);
            let mut row = cell.inset_xy(10.0, 0.0);
            ui.icon_btn(row.cut_right(22.0, 2.0).centered(22.0, 22.0), glyph::CLOSE, A_REMOVE + k as u32, 9.0, col::DIM, None, "Remove");
            ui.icon_btn(row.cut_right(22.0, 6.0).centered(22.0, 22.0), "\u{E768}", A_RUN + k as u32, 10.0, C_CMD, None, "Run now");
            let mut ev = row.cut_right(84.0, 8.0).centered(84.0, 22.0);
            ui.icon_btn(ev.cut_left(20.0, 0.0), "\u{E76B}", A_LESS + k as u32, 8.0, col::DIM, None, "Less often");
            ui.icon_btn(ev.cut_right(20.0, 0.0), "\u{E76C}", A_MORE + k as u32, 8.0, col::DIM, None, "More often");
            let every = c.every;
            self.every_label(ui, ev, k, every, 10.5);
            let name = row.cut_left(110.0, 8.0);
            ui.text(&c.text, name, 11.5, col::TEXT, Weight::Semi, Align::Left);
            let shown = if running { "Running...".to_string() } else if out.is_empty() { "-".into() } else { out };
            let when = if at > 0.0 { crate::time::fmt_mod(crate::time::local_time(at).3) } else { String::new() };
            ui.text(&when, row.cut_right(52.0, 4.0), 10.0, col::FAINT, Weight::Regular, Align::Right);
            ui.text(&shown, row, 11.0, col::DIM, Weight::Regular, Align::Left);
        }
    }
    fn action(&mut self, id: u32, _rel: Rel, ctx: &mut Ctx) {
        match id {
            A_FIELD => {
                self.commit_minutes();
                self.field.focused = true;
                ctx.want_keyboard(true);
            }
            i if (A_EDIT..=A_EDIT + MAX as u32).contains(&i) => {
                let w = (i - A_EDIT) as usize;
                if self.editing != Some(w) {
                    self.commit_minutes();
                    self.editing = Some(w);
                    self.minutes.text.clear();
                    self.minutes.focused = true;
                    self.field.focused = false;
                    ctx.want_keyboard(true);
                }
            }
            A_ADD => self.add(),
            A_NEW_LESS => self.new_every = step(self.new_every, 1),
            A_NEW_MORE => self.new_every = step(self.new_every, -1),
            i if (A_RUN..A_RUN + MAX as u32).contains(&i) => {
                if let Some(c) = self.list.get_mut((i - A_RUN) as usize) {
                    c.last = unix_now();
                    run(c.text.clone(), c.out.clone());
                }
            }
            i if (A_REMOVE..A_REMOVE + MAX as u32).contains(&i) => {
                let k = (i - A_REMOVE) as usize;
                if k < self.list.len() {
                    self.list.remove(k);
                    self.save();
                }
            }
            i if (A_LESS..A_LESS + MAX as u32).contains(&i) => {
                if let Some(c) = self.list.get_mut((i - A_LESS) as usize) {
                    c.every = step(c.every, 1);
                }
                self.save();
            }
            i if (A_MORE..A_MORE + MAX as u32).contains(&i) => {
                if let Some(c) = self.list.get_mut((i - A_MORE) as usize) {
                    c.every = step(c.every, -1);
                }
                self.save();
            }
            _ => {}
        }
    }
    fn key(&mut self, vk: u16, ctrl: bool, ctx: &mut Ctx) -> bool {
        if self.editing.is_some() {
            match vk {
                0x0D => self.commit_minutes(),
                0x1B => {
                    self.editing = None;
                    self.minutes.text.clear();
                }
                0x08 => {
                    self.minutes.text.pop();
                }
                _ => return false,
            }
            return true;
        }
        match self.field.key(vk, ctrl) {
            FieldKey::Submit => {
                self.add();
                true
            }
            FieldKey::Cancel => {
                self.field.focused = false;
                ctx.want_keyboard(false);
                true
            }
            FieldKey::Edited => true,
            FieldKey::Ignored => false,
        }
    }
    fn chr(&mut self, c: char, _ctx: &mut Ctx) -> bool {
        if self.editing.is_some() {
            if c.is_ascii_digit() && self.minutes.text.len() < 5 {
                self.minutes.text.push(c);
            }
            return true;
        }
        self.field.chr(c)
    }
    fn blur(&mut self) {
        self.commit_minutes();
        self.field.focused = false;
    }
    fn widgets(&mut self) -> Vec<Widget> {
        self.list
            .iter()
            .enumerate()
            .map(|(k, c)| {
                let o = c.out.lock().unwrap().0.clone();
                let v = if o.is_empty() { "...".to_string() } else { o.clone() };
                let short: String = v.chars().take(14).collect();
                Widget::new(IDS[k], "Command", "\u{E756}", C_CMD, short.clone()).short(short).label(label(&c.text)).detail(format!("{}: {}", label(&c.text), if o.is_empty() { "waiting for the first run" } else { &o }))
            })
            .collect()
    }
    fn tip(&self, id: u32) -> &'static str {
        match id {
            A_FIELD => "The command to run with cmd.exe",
            A_ADD => "Add it (it runs once now)",
            A_NEW_LESS | A_NEW_MORE => "How often it runs",
            i if (A_EDIT..=A_EDIT + MAX as u32).contains(&i) => "Click and type any number of minutes, then Enter",
            _ => "",
        }
    }
    fn debug_state(&self) -> String {
        format!("commands n={}", self.list.len())
    }
}
