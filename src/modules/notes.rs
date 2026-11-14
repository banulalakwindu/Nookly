//! Quick notes: one scratchpad that saves as you type (notes.txt). A real
//! little editor: click or drag to place the caret and select, arrows (with
//! Shift to select, Ctrl to jump words), Home / End, Ctrl+A / C / X / V,
//! the mouse wheel scrolls.

use crate::gfx::*;
use crate::modules::clipboard::{set_clipboard, Content};
use crate::modules::{Ctx, Module, Rel, Toast};
use crate::store;
use crate::ui::Ui;

const A_TEXT: u32 = 1;
const A_COPY: u32 = 2;
const A_CLEAR: u32 = 3;
pub const A_FOCUS: u32 = 4;

const C_NOTES: Color = Color { r: 1.0, g: 0.80, b: 0.30, a: 1.0 };
const SIZE: f32 = 12.5;
const LINE_H: f32 = 18.0;
const MAX: usize = 20_000;

/// One line on screen: byte range in the text and its x offsets.
#[derive(Clone, Debug)]
struct Line {
    start: usize,
    end: usize,
}

pub struct Notes {
    text: String,
    caret: usize,
    /// The other end of the selection, if any.
    anchor: Option<usize>,
    focused: bool,
    scroll: f32,
    /// From the last draw: the text box, its lines, and each line's
    /// character x positions (for clicks and the caret).
    area: Rect,
    lines: Vec<Line>,
    xs: Vec<Vec<f32>>,
    /// Up / Down keep the column they started from.
    want_x: Option<f32>,
    dragging: bool,
}

fn prev_char(s: &str, i: usize) -> usize {
    s[..i].char_indices().next_back().map_or(0, |(k, _)| k)
}

fn next_char(s: &str, i: usize) -> usize {
    s[i..].chars().next().map_or(i, |c| i + c.len_utf8())
}

fn prev_word(s: &str, mut i: usize) -> usize {
    while i > 0 && s[..i].chars().next_back().map_or(false, char::is_whitespace) {
        i = prev_char(s, i);
    }
    while i > 0 && s[..i].chars().next_back().map_or(false, |c| !c.is_whitespace()) {
        i = prev_char(s, i);
    }
    i
}

fn next_word(s: &str, mut i: usize) -> usize {
    while i < s.len() && s[i..].chars().next().map_or(false, |c| !c.is_whitespace()) {
        i = next_char(s, i);
    }
    while i < s.len() && s[i..].chars().next().map_or(false, char::is_whitespace) {
        i = next_char(s, i);
    }
    i
}

impl Notes {
    pub fn load() -> Notes {
        let text = if crate::time::demo() {
            "Call the bank about the card\nIdeas for the weekend: Ella hike, train ride\nWi-Fi at the office: blue-ocean-42".to_string()
        } else {
            store::read("notes.txt")
        };
        Notes { caret: text.len(), text, anchor: None, focused: false, scroll: 0.0, area: Rect::new(0.0, 0.0, 0.0, 0.0), lines: Vec::new(), xs: Vec::new(), want_x: None, dragging: false }
    }
    fn save(&self) {
        store::write("notes.txt", &self.text);
    }
    fn selection(&self) -> Option<(usize, usize)> {
        self.anchor.filter(|&a| a != self.caret).map(|a| (a.min(self.caret), a.max(self.caret)))
    }
    fn selected(&self) -> String {
        self.selection().map_or(String::new(), |(a, b)| self.text[a..b].to_string())
    }
    fn delete_selection(&mut self) -> bool {
        match self.selection() {
            Some((a, b)) => {
                self.text.replace_range(a..b, "");
                self.caret = a;
                self.anchor = None;
                true
            }
            None => false,
        }
    }
    fn insert(&mut self, s: &str) {
        self.delete_selection();
        let room = MAX.saturating_sub(self.text.len());
        let mut cut = s.len().min(room);
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        self.text.insert_str(self.caret, &s[..cut]);
        self.caret += cut;
        self.anchor = None;
    }
    /// Move the caret; with `extend` the selection grows from where it was.
    fn move_to(&mut self, i: usize, extend: bool) {
        if extend {
            if self.anchor.is_none() {
                self.anchor = Some(self.caret);
            }
        } else {
            self.anchor = None;
        }
        self.caret = i.min(self.text.len());
    }
    /// Which line the caret is on (from the last layout).
    fn caret_line(&self) -> usize {
        self.lines.iter().rposition(|l| l.start <= self.caret).unwrap_or(0)
    }
    fn x_of(&self, line: usize, i: usize) -> f32 {
        let (l, xs) = (&self.lines[line], &self.xs[line]);
        let k = self.text[l.start..i.min(l.end).max(l.start)].chars().count();
        xs.get(k).copied().unwrap_or(0.0)
    }
    fn index_at_x(&self, line: usize, x: f32) -> usize {
        let (l, xs) = (&self.lines[line], &self.xs[line]);
        let mut best = (f32::MAX, l.start);
        for (k, (i, _)) in self.text[l.start..l.end].char_indices().chain(std::iter::once((l.end - l.start, ' '))).enumerate() {
            let d = (xs.get(k).copied().unwrap_or(0.0) - x).abs();
            if d < best.0 {
                best = (d, l.start + i);
            }
        }
        best.1
    }
    /// Text index under a point on screen.
    fn index_at(&self, x: f32, y: f32) -> usize {
        if self.lines.is_empty() {
            return 0;
        }
        let line = (((y - self.area.y + self.scroll) / LINE_H).floor().max(0.0) as usize).min(self.lines.len() - 1);
        self.index_at_x(line, x - self.area.x)
    }
    /// Keep the caret's line in view.
    fn reveal(&mut self) {
        if self.lines.is_empty() {
            return;
        }
        let y = self.caret_line() as f32 * LINE_H;
        if y < self.scroll {
            self.scroll = y;
        } else if y + LINE_H > self.scroll + self.area.h {
            self.scroll = y + LINE_H - self.area.h;
        }
    }
    /// Word-wrap the text to `w` and remember where every character sits.
    fn layout(&mut self, ui: &mut Ui, w: f32) {
        self.lines.clear();
        self.xs.clear();
        let mut start = 0;
        for para in self.text.split('\n') {
            let base = start;
            let mut ls = 0usize;
            loop {
                // As many words as fit on this line.
                let rest = &para[ls..];
                let mut end = rest.len();
                if ui.measure(rest, SIZE, Weight::Regular) > w {
                    let mut last_space = None;
                    let mut fit = 0;
                    for (i, c) in rest.char_indices() {
                        let next = i + c.len_utf8();
                        if ui.measure(&rest[..next], SIZE, Weight::Regular) > w {
                            break;
                        }
                        fit = next;
                        if c == ' ' {
                            last_space = Some(next);
                        }
                    }
                    end = last_space.filter(|&s| s > 0).unwrap_or(fit.max(rest.chars().next().map_or(0, char::len_utf8)));
                }
                let (a, b) = (base + ls, base + ls + end);
                let mut xs = vec![0.0];
                let line = &self.text[a..b];
                for (i, c) in line.char_indices() {
                    xs.push(ui.measure(&line[..i + c.len_utf8()], SIZE, Weight::Regular));
                }
                self.lines.push(Line { start: a, end: b });
                self.xs.push(xs);
                ls += end;
                if ls >= para.len() {
                    break;
                }
            }
            start = base + para.len() + 1;
        }
    }
}

impl Module for Notes {
    fn title(&self) -> &'static str {
        "Notes"
    }
    fn icon(&self) -> &'static str {
        "\u{E70B}"
    }
    fn accent(&self) -> Color {
        C_NOTES
    }
    fn size(&self) -> (f32, f32) {
        (500.0, 160.0)
    }
    fn draw(&mut self, ui: &mut Ui, mut r: Rect) {
        let mut head = r.cut_top(24.0, 8.0);
        if !self.text.is_empty() {
            ui.button(head.cut_right(64.0, 6.0), "Clear", A_CLEAR);
            ui.button(head.cut_right(64.0, 6.0), if self.selection().is_some() { "Copy sel." } else { "Copy all" }, A_COPY);
        }
        let words = self.text.split_whitespace().count();
        let info = if self.text.is_empty() {
            "Saves as you type. Give it a shortcut in Settings > Shortcuts".to_string()
        } else {
            format!("{words} word{}", if words == 1 { "" } else { "s" })
        };
        ui.text(&info, head, 11.0, col::DIM, Weight::Regular, Align::Left);

        let hov = ui.hovered(r);
        ui.fill_rr(r, 9.0, if self.focused || hov { col::BTN_HOVER } else { col::BTN });
        if self.focused {
            ui.stroke_rr(r, 9.0, C_NOTES.with_a(0.8), 1.2);
        }
        let inner = r.inset_xy(10.0, 7.0);
        self.area = inner;
        crate::ui::text_area(inner);
        self.layout(ui, inner.w - 4.0);
        let total = self.lines.len() as f32 * LINE_H;
        self.scroll = self.scroll.clamp(0.0, (total - inner.h).max(0.0));
        if self.text.is_empty() && !self.focused {
            ui.text("Jot something down...", Rect::new(inner.x, inner.y, inner.w, LINE_H), SIZE, col::FAINT, Weight::Regular, Align::Left);
        }
        ui.push_clip(inner);
        let sel = self.selection();
        let caret_line = self.caret_line();
        for (k, l) in self.lines.iter().enumerate() {
            let y = inner.y + k as f32 * LINE_H - self.scroll;
            if y + LINE_H < inner.y || y > inner.b() {
                continue;
            }
            if let Some((a, b)) = sel {
                if a < l.end.max(l.start + 1) && b > l.start {
                    let x0 = self.x_of(k, a.max(l.start));
                    let x1 = if b > l.end { self.xs[k].last().copied().unwrap_or(0.0) + 5.0 } else { self.x_of(k, b) };
                    ui.fill_rect(Rect::new(inner.x + x0, y + 1.0, (x1 - x0).max(2.0), LINE_H - 2.0), C_NOTES.with_a(0.3));
                }
            }
            ui.text(&self.text[l.start..l.end], Rect::new(inner.x, y, inner.w + 40.0, LINE_H), SIZE, col::TEXT, Weight::Regular, Align::Left);
            if self.focused && k == caret_line && (ui.t * 2.0) as i64 % 2 == 0 {
                let x = inner.x + self.x_of(k, self.caret);
                ui.fill_rect(Rect::new(x, y + 2.0, 1.4, LINE_H - 4.0), C_NOTES);
            }
        }
        ui.pop_clip();
        if total > inner.h {
            // A thin scroll bar.
            let h = (inner.h * inner.h / total).max(12.0);
            let y = inner.y + (inner.h - h) * self.scroll / (total - inner.h);
            ui.fill_rr(Rect::new(r.r() - 5.0, y, 3.0, h), 1.5, col::TEXT.with_a(0.3));
        }
        ui.hit(r, A_TEXT);
    }
    fn action(&mut self, id: u32, rel: Rel, ctx: &mut Ctx) {
        match id {
            A_TEXT | A_FOCUS => {
                self.focused = true;
                ctx.want_keyboard(true);
                if id == A_FOCUS {
                    ctx.open_my_tab();
                    self.caret = self.text.len();
                    self.anchor = None;
                } else if !self.dragging {
                    // A click places the caret (the box is padded by 10 x 7).
                    let outer = Rect::new(self.area.x - 10.0, self.area.y - 7.0, self.area.w + 20.0, self.area.h + 14.0);
                    let i = self.index_at(outer.x + rel.0 * outer.w, outer.y + rel.1 * outer.h);
                    self.move_to(i, false);
                }
                self.dragging = false;
                self.want_x = None;
            }
            A_COPY => {
                let s = self.selection().map_or(self.text.clone(), |_| self.selected());
                if set_clipboard(ctx.hwnd, &Content::Text(s)) {
                    ctx.toast(Toast::new("notes", glyph::CHECK, C_NOTES, "Copied", "").secs(1.5));
                }
            }
            A_CLEAR => {
                self.text.clear();
                self.caret = 0;
                self.anchor = None;
                self.save();
            }
            _ => {}
        }
    }
    fn drag_begin(&mut self, id: u32, x: f32, y: f32) -> bool {
        if id != A_TEXT {
            return false;
        }
        let i = self.index_at(x, y);
        self.caret = i;
        self.anchor = Some(i);
        self.dragging = true;
        true
    }
    fn drag_to(&mut self, x: f32, y: f32) {
        // Past the top or bottom: scroll along.
        if y < self.area.y {
            self.scroll = (self.scroll - 6.0).max(0.0);
        } else if y > self.area.b() {
            self.scroll += 6.0;
        }
        self.caret = self.index_at(x, y.clamp(self.area.y, self.area.b() - 1.0));
    }
    fn drag_drop(&mut self, _x: f32, _y: f32, ctx: &mut Ctx) {
        self.focused = true;
        ctx.want_keyboard(true);
    }
    fn scroll(&mut self, delta: f32) -> bool {
        let total = self.lines.len() as f32 * LINE_H;
        if total <= self.area.h {
            return false;
        }
        self.scroll = (self.scroll - delta * LINE_H * 3.0).clamp(0.0, total - self.area.h);
        true
    }
    fn key(&mut self, vk: u16, ctrl: bool, ctx: &mut Ctx) -> bool {
        if !self.focused {
            return false;
        }
        let shift = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x10) } < 0;
        let t = self.text.clone();
        let mut edited = false;
        let vertical = matches!(vk, 0x26 | 0x28);
        match vk {
            0x25 => self.move_to(if !shift && self.selection().is_some() { self.selection().unwrap().0 } else if ctrl { prev_word(&t, self.caret) } else { prev_char(&t, self.caret) }, shift),
            0x27 => self.move_to(if !shift && self.selection().is_some() { self.selection().unwrap().1 } else if ctrl { next_word(&t, self.caret) } else { next_char(&t, self.caret) }, shift),
            0x26 | 0x28 => {
                let line = self.caret_line();
                let target = if vk == 0x26 { line.checked_sub(1) } else { Some(line + 1).filter(|&l| l < self.lines.len()) };
                let x = *self.want_x.get_or_insert(self.x_of(line, self.caret));
                let i = match target {
                    Some(l) => self.index_at_x(l, x),
                    None if vk == 0x26 => 0,
                    None => t.len(),
                };
                self.move_to(i, shift);
            }
            0x24 => {
                let i = if ctrl { 0 } else { self.lines.get(self.caret_line()).map_or(0, |l| l.start) };
                self.move_to(i, shift);
            }
            0x23 => {
                let i = if ctrl { t.len() } else { self.lines.get(self.caret_line()).map_or(t.len(), |l| l.end) };
                self.move_to(i, shift);
            }
            0x41 if ctrl => {
                self.anchor = Some(0);
                self.caret = t.len();
            }
            0x43 | 0x58 if ctrl => {
                let s = self.selected();
                if !s.is_empty() {
                    set_clipboard(ctx.hwnd, &Content::Text(s));
                    if vk == 0x58 {
                        edited = self.delete_selection();
                    }
                }
            }
            0x56 if ctrl => {
                if let Some(s) = crate::modules::todo::clipboard_text() {
                    self.insert(&s.replace("\r\n", "\n"));
                    edited = true;
                }
            }
            0x08 => {
                if !self.delete_selection() && self.caret > 0 {
                    let a = if ctrl { prev_word(&t, self.caret) } else { prev_char(&t, self.caret) };
                    self.text.replace_range(a..self.caret, "");
                    self.caret = a;
                }
                edited = true;
            }
            0x2E => {
                if !self.delete_selection() && self.caret < t.len() {
                    let b = if ctrl { next_word(&t, self.caret) } else { next_char(&t, self.caret) };
                    self.text.replace_range(self.caret..b, "");
                }
                edited = true;
            }
            0x0D => {
                self.insert("\n");
                edited = true;
            }
            0x09 => {
                self.insert("    ");
                edited = true;
            }
            0x1B => {
                if self.anchor.take().is_none() {
                    self.focused = false;
                    ctx.want_keyboard(false);
                }
            }
            _ => return false,
        }
        if !vertical {
            self.want_x = None;
        }
        if edited {
            self.save();
        }
        self.reveal();
        true
    }
    fn chr(&mut self, c: char, _ctx: &mut Ctx) -> bool {
        if !self.focused || c.is_control() {
            return false;
        }
        let mut b = [0u8; 4];
        self.insert(c.encode_utf8(&mut b));
        self.want_x = None;
        self.save();
        self.reveal();
        true
    }
    fn blur(&mut self) {
        self.focused = false;
    }
    fn animating(&self) -> bool {
        self.focused
    }
    fn tip(&self, id: u32) -> &'static str {
        match id {
            A_TEXT => "Type here; drag to select",
            A_COPY => "Copy the selection, or the whole note",
            A_CLEAR => "Clear the note",
            _ => "",
        }
    }
    fn debug_state(&self) -> String {
        format!("notes chars={} caret={} sel={:?}", self.text.len(), self.caret, self.selection())
    }
}

impl Notes {
    /// For the self-test: type, move and select as if from the keyboard.
    pub fn test_text(&self) -> (&str, usize, Option<(usize, usize)>) {
        (&self.text, self.caret, self.selection())
    }
    pub fn test_set(&mut self, text: &str) {
        self.text = text.to_string();
        self.caret = text.len();
        self.anchor = None;
        self.focused = true;
    }
}
