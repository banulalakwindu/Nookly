//! Countdowns: exams, birthdays, Eid, salary day. "12 days" on the notch.
//! Saved in countdowns.txt as `yyyy-mm-dd|yearly(0/1)|name` lines.

use crate::gfx::*;
use crate::modules::{Ctx, Module, Rel, Toast, Widget};
use crate::store;
use crate::time::Now;
use crate::ui::{FieldKey, TextField, Ui};

const A_FIELD: u32 = 1;
const A_ADD: u32 = 2;
const A_REPEAT: u32 = 100; // + index
/// Date picker: + 0 day, 1 month, 2 year; DOWN goes back, UP forward.
const A_DOWN: u32 = 10;
const A_UP: u32 = 20;
const A_YEARLY: u32 = 30;
const A_REMOVE: u32 = 200; // + index

const C_COUNT: Color = Color { r: 0.55, g: 0.75, b: 1.0, a: 1.0 };
const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];

#[derive(Clone, Debug, PartialEq)]
pub struct Countdown {
    pub y: i32,
    pub m: u32,
    pub d: u32,
    pub yearly: bool,
    pub name: String,
}

/// Days since 1970-01-01 of a civil date.
pub fn day_number(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Days from `today` to the next time it comes (yearly ones roll over).
pub fn days_left(c: &Countdown, today: (i32, u32, u32)) -> i64 {
    let t = day_number(today.0, today.1, today.2);
    if !c.yearly {
        return day_number(c.y, c.m, c.d) - t;
    }
    let d = c.d.min(crate::time::days_in_month(today.0, c.m));
    let this = day_number(today.0, c.m, d);
    if this >= t {
        this - t
    } else {
        day_number(today.0 + 1, c.m, c.d.min(crate::time::days_in_month(today.0 + 1, c.m))) - t
    }
}

/// "Exam 2026-12-20", "Mum's birthday, 3 Mar", "Eid Jun 6", "Trip 20/12":
/// the name and the date (a date without a year is the next one to come).
pub fn parse(s: &str, today: (i32, u32, u32)) -> Option<Countdown> {
    let s = s.trim();
    let words: Vec<&str> = s.split(|c: char| c.is_whitespace() || c == ',').filter(|w| !w.is_empty()).collect();
    // Try the date in the last 1-3 words.
    for take in [1usize, 2, 3] {
        if words.len() < take + 1 {
            continue;
        }
        let date = words[words.len() - take..].join(" ");
        if let Some((y, m, d, has_year)) = parse_date(&date) {
            let name = words[..words.len() - take].join(" ");
            let mut y = if has_year { y } else { today.0 };
            if !has_year && day_number(y, m, d) < day_number(today.0, today.1, today.2) {
                y += 1;
            }
            if d == 0 || d > crate::time::days_in_month(y, m) {
                return None;
            }
            return Some(Countdown { y, m, d, yearly: false, name });
        }
    }
    None
}

fn month_of(w: &str) -> Option<u32> {
    let w = w.to_lowercase();
    MONTHS.iter().position(|m| w.starts_with(m)).map(|k| k as u32 + 1)
}

fn parse_date(s: &str) -> Option<(i32, u32, u32, bool)> {
    let num = |x: &str| x.trim().parse::<u32>().ok();
    // 2026-12-20
    let p: Vec<&str> = s.split('-').collect();
    if p.len() == 3 && p[0].len() == 4 {
        return Some((num(p[0])? as i32, num(p[1]).filter(|m| (1..=12).contains(m))?, num(p[2])?, true));
    }
    // 20/12, 20/12/2026, 20.12.2026
    for sep in ['/', '.'] {
        let p: Vec<&str> = s.split(sep).collect();
        if p.len() >= 2 && p.len() <= 3 && p.iter().all(|x| x.chars().all(|c| c.is_ascii_digit()) && !x.is_empty()) {
            let (d, m) = (num(p[0])?, num(p[1]).filter(|m| (1..=12).contains(m))?);
            let y = p.get(2).and_then(|y| num(y)).map(|y| if y < 100 { 2000 + y as i32 } else { y as i32 });
            return Some((y.unwrap_or(0), m, d, y.is_some()));
        }
    }
    // 20 Dec [2026], Dec 20 [2026]
    let w: Vec<&str> = s.split_whitespace().collect();
    let (d, m, rest) = match w.as_slice() {
        [a, b, rest @ ..] if num(a).is_some() && month_of(b).is_some() => (num(a)?, month_of(b)?, rest),
        [a, b, rest @ ..] if month_of(a).is_some() && num(b).is_some() => (num(b)?, month_of(a)?, rest),
        _ => return None,
    };
    match rest {
        [] => Some((0, m, d, false)),
        [y] if y.len() == 4 => Some((num(y)? as i32, m, d, true)),
        _ => None,
    }
}

pub struct Countdowns {
    items: Vec<Countdown>,
    field: TextField,
    error: bool,
    last_day: String,
    /// The date being picked for a new countdown, and whether it repeats.
    pick: (i32, u32, u32),
    yearly: bool,
    /// Picker part under the mouse (0 day, 1 month, 2 year), for the wheel.
    hover_part: Option<u32>,
}

impl Countdowns {
    pub fn load() -> Countdowns {
        if crate::time::demo() {
            let t = Self::today();
            let at = |days: i64| crate::modules::meetings::civil_pub(day_number(t.0, t.1, t.2) + days);
            let items = [(9, "Final exam", false), (38, "Trip to Kandy", false), (121, "Mum's birthday", true)]
                .iter()
                .map(|&(d, n, yearly)| {
                    let (y, m, dd) = at(d);
                    Countdown { y, m, d: dd, yearly, name: n.into() }
                })
                .collect();
            let mut c = Countdowns { items, field: TextField::new(60), error: false, last_day: String::new(), pick: t, yearly: false, hover_part: None };
            c.step(0, 7);
            return c;
        }
        let items = store::read("countdowns.txt")
            .lines()
            .filter_map(|l| {
                let mut p = l.splitn(3, '|');
                let date: Vec<u32> = p.next()?.split('-').filter_map(|x| x.parse().ok()).collect();
                let yearly = p.next()? == "1";
                let name = store::unesc(p.next()?);
                (date.len() == 3).then(|| Countdown { y: date[0] as i32, m: date[1], d: date[2], yearly, name })
            })
            .collect();
        let t = Self::today();
        let mut c = Countdowns { items, field: TextField::new(60), error: false, last_day: String::new(), pick: t, yearly: false, hover_part: None };
        c.step(0, 7);
        c
    }
    fn save(&self) {
        let s: String = self.items.iter().map(|c| format!("{:04}-{:02}-{:02}|{}|{}\n", c.y, c.m, c.d, c.yearly as u8, store::esc(&c.name))).collect();
        store::write("countdowns.txt", &s);
    }
    /// Move the picked date: part 0 day, 1 month, 2 year.
    fn step(&mut self, part: u32, by: i32) {
        let (mut y, mut m, mut d) = self.pick;
        match part {
            0 => {
                let n = day_number(y, m, d) + by as i64;
                (y, m, d) = crate::modules::meetings::civil_pub(n);
            }
            1 => {
                let k = y * 12 + m as i32 - 1 + by;
                y = k.div_euclid(12);
                m = (k.rem_euclid(12) + 1) as u32;
            }
            _ => y += by,
        }
        let t = Self::today();
        y = y.clamp(t.0 - 1, t.0 + 20);
        d = d.min(crate::time::days_in_month(y, m));
        self.pick = (y, m, d);
    }
    fn today() -> (i32, u32, u32) {
        let n = Now::get();
        (n.year as i32, n.month as u32, n.day as u32)
    }
    /// Upcoming ones, soonest first: (index, days left).
    fn upcoming(&self) -> Vec<(usize, i64)> {
        let t = Self::today();
        let mut v: Vec<(usize, i64)> = self.items.iter().enumerate().map(|(i, c)| (i, days_left(c, t))).collect();
        v.sort_by_key(|&(i, d)| (d < 0, d.abs(), i));
        v
    }
}

fn left_text(d: i64) -> String {
    match d {
        0 => "Today".into(),
        1 => "Tomorrow".into(),
        d if d < 0 => format!("{} d ago", -d),
        d => format!("{d} days"),
    }
}

impl Module for Countdowns {
    fn title(&self) -> &'static str {
        "Countdowns"
    }
    fn icon(&self) -> &'static str {
        "\u{E7C1}"
    }
    fn accent(&self) -> Color {
        C_COUNT
    }
    fn size(&self) -> (f32, f32) {
        (500.0, 170.0)
    }
    fn tick(&mut self, ctx: &mut Ctx) {
        // On the day: a note once.
        let day = ctx.now.day_key();
        if day != self.last_day {
            let first = self.last_day.is_empty();
            self.last_day = day;
            if !first {
                for (i, d) in self.upcoming() {
                    if d == 0 {
                        ctx.toast(Toast::new("countdown", "\u{E7C1}", C_COUNT, self.items[i].name.clone(), "It's today!").secs(8.0).chime());
                    }
                }
            }
        }
    }
    fn draw(&mut self, ui: &mut Ui, mut r: Rect) {
        // A name, and the date picked with arrows (or the mouse wheel).
        let mut row = r.cut_top(28.0, 6.0);
        ui.button_accent(row.cut_right(70.0, 6.0), "Add", A_ADD, C_COUNT);
        ui.field(row, &self.field, if self.error { "Give it a name first" } else { "Name: exam, birthday, Eid..." }, A_FIELD, C_COUNT);
        let mut row = r.cut_top(26.0, 8.0);
        let (y, m, d) = self.pick;
        let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        self.hover_part = None;
        for (part, (text, w)) in [(d.to_string(), 34.0), (months[(m - 1) as usize].to_string(), 40.0), (y.to_string(), 46.0)].into_iter().enumerate() {
            let cell = row.cut_left(w + 48.0, 6.0);
            if ui.hovered(cell) {
                self.hover_part = Some(part as u32);
            }
            ui.fill_rr(cell, 13.0, col::BTN);
            let mut c = cell;
            ui.icon_btn(c.cut_left(24.0, 0.0), "\u{E76B}", A_DOWN + part as u32, 9.0, col::DIM, None, "Earlier");
            ui.icon_btn(c.cut_right(24.0, 0.0), "\u{E76C}", A_UP + part as u32, 9.0, col::DIM, None, "Later");
            ui.text(&text, c, 12.5, col::TEXT, Weight::Semi, Align::Center);
        }
        ui.chip(row.cut_left(92.0, 8.0), "Every year", A_YEARLY, self.yearly, C_COUNT);
        let left = days_left(&Countdown { y, m, d, yearly: self.yearly, name: String::new() }, Self::today());
        let when = match left {
            0 => "today".to_string(),
            1 => "tomorrow".to_string(),
            l if l > 1 => format!("in {l} days"),
            _ => "in the past".to_string(),
        };
        ui.text(&when, row, 11.0, col::DIM, Weight::Regular, Align::Left);
        if self.items.is_empty() {
            ui.text_wrap("Add a date to count down to. Turn on the Countdown widget (Settings > Widgets) to see \"12 days\" on the closed notch.", r.inset_xy(4.0, 6.0), 12.0, col::DIM, Weight::Regular, Align::Left);
            return;
        }
        let list = self.upcoming();
        let t = Self::today();
        for (k, cell) in r.rows(3, 4.0).into_iter().enumerate() {
            let Some(&(i, d)) = list.get(k) else { break };
            let c = &self.items[i];
            let mut row = cell;
            ui.fill_rr(row, 9.0, col::BTN);
            row = row.inset_xy(10.0, 0.0);
            ui.icon_btn(row.cut_right(22.0, 2.0).centered(22.0, 22.0), glyph::CLOSE, A_REMOVE + i as u32, 9.0, col::DIM, None, "Remove");
            ui.icon_btn(
                row.cut_right(22.0, 8.0).centered(22.0, 22.0),
                "\u{E8EE}",
                A_REPEAT + i as u32,
                10.0,
                if c.yearly { C_COUNT } else { col::FAINT },
                None,
                "Repeat every year",
            );
            let lt = left_text(d);
            ui.text(&lt, row.cut_right(80.0, 6.0), 12.5, if d <= 7 && d >= 0 { C_COUNT } else { col::TEXT }, Weight::Semi, Align::Right);
            let date = format!("{} {}", c.d, ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][(c.m - 1) as usize]);
            let date = if c.yearly || c.y == t.0 { date } else { format!("{date} {}", c.y) };
            ui.text(&date, row.cut_right(80.0, 6.0), 11.0, col::DIM, Weight::Regular, Align::Right);
            ui.text(&c.name, row, 12.5, col::TEXT, Weight::Regular, Align::Left);
        }
    }
    fn action(&mut self, id: u32, _rel: Rel, ctx: &mut Ctx) {
        match id {
            A_FIELD => {
                self.field.focused = true;
                ctx.want_keyboard(true);
            }
            A_ADD => self.submit(ctx),
            i if (A_DOWN..A_DOWN + 3).contains(&i) => self.step(i - A_DOWN, -1),
            i if (A_UP..A_UP + 3).contains(&i) => self.step(i - A_UP, 1),
            A_YEARLY => self.yearly = !self.yearly,
            i if (A_REPEAT..A_REPEAT + 100).contains(&i) => {
                if let Some(c) = self.items.get_mut((i - A_REPEAT) as usize) {
                    c.yearly = !c.yearly;
                    self.save();
                }
            }
            i if (A_REMOVE..A_REMOVE + 100).contains(&i) => {
                let k = (i - A_REMOVE) as usize;
                if k < self.items.len() {
                    self.items.remove(k);
                    self.save();
                }
            }
            _ => {}
        }
    }
    fn key(&mut self, vk: u16, ctrl: bool, ctx: &mut Ctx) -> bool {
        match self.field.key(vk, ctrl) {
            FieldKey::Submit => {
                self.submit(ctx);
                true
            }
            FieldKey::Cancel => {
                self.field.focused = false;
                ctx.want_keyboard(false);
                true
            }
            FieldKey::Edited => {
                self.error = false;
                true
            }
            FieldKey::Ignored => false,
        }
    }
    fn chr(&mut self, c: char, _ctx: &mut Ctx) -> bool {
        self.error = false;
        self.field.chr(c)
    }
    fn blur(&mut self) {
        self.field.focused = false;
    }
    fn scroll(&mut self, delta: f32) -> bool {
        match self.hover_part {
            Some(p) => {
                self.step(p, if delta > 0.0 { 1 } else { -1 });
                true
            }
            None => false,
        }
    }
    fn widgets(&mut self) -> Vec<Widget> {
        let Some(&(i, d)) = self.upcoming().first() else { return Vec::new() };
        if d < 0 {
            return Vec::new();
        }
        let c = &self.items[i];
        let value = if d == 0 { "Today".to_string() } else { format!("{d} d") };
        vec![Widget::new("countdown", "Countdown", "\u{E7C1}", C_COUNT, value)
            .label(c.name.clone())
            .short(format!("{d}"))
            .detail(format!("{}: {}", c.name, left_text(d)))
            .quiet(d > 30)]
    }
    fn tip(&self, id: u32) -> &'static str {
        match id {
            A_FIELD => "What you're counting down to",
            i if (A_DOWN..A_DOWN + 3).contains(&i) => "Earlier (or scroll the wheel here)",
            i if (A_UP..A_UP + 3).contains(&i) => "Later (or scroll the wheel here)",
            A_YEARLY => "Repeat every year (birthdays, holidays)",
            A_ADD => "Add this countdown",
            _ => "",
        }
    }
    fn debug_state(&self) -> String {
        format!("countdowns n={}", self.items.len())
    }
}

impl Countdowns {
    fn submit(&mut self, ctx: &mut Ctx) {
        let name = self.field.text.trim().to_string();
        if name.is_empty() {
            self.error = true;
            return;
        }
        let (y, m, d) = self.pick;
        self.items.push(Countdown { y, m, d, yearly: self.yearly, name });
        self.save();
        self.field.text.clear();
        self.field.focused = false;
        self.yearly = false;
        ctx.want_keyboard(false);
    }
}
