//! Immediate-mode UI helpers on top of `Gfx`.
//!
//! Modules draw every frame and register clickable areas with `ui.hit(rect, id)`
//! (buttons do this for you). When the user clicks, the app looks up the hit
//! under the cursor and calls `Module::action(id, ...)` on the owning module.

use std::ops::{Deref, DerefMut};

use crate::gfx::*;

#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub rect: Rect,
    pub module: usize,
    pub id: u32,
    /// Tooltip shown when the mouse rests on the hit ("" = none).
    pub tip: &'static str,
}

pub struct Ui<'a> {
    pub g: &'a mut Gfx,
    pub hits: &'a mut Vec<Hit>,
    /// Index of the module currently drawing (stamped on registered hits).
    pub module: usize,
    /// Mouse position in DIPs (or far away when outside the window).
    pub mouse: (f32, f32),
    /// Seconds since app start, for animations.
    pub t: f64,
}

impl<'a> Deref for Ui<'a> {
    type Target = Gfx;
    fn deref(&self) -> &Gfx {
        self.g
    }
}
impl<'a> DerefMut for Ui<'a> {
    fn deref_mut(&mut self) -> &mut Gfx {
        self.g
    }
}

impl<'a> Ui<'a> {
    pub fn hit(&mut self, r: Rect, id: u32) {
        self.hit_tip(r, id, "");
    }

    /// Clickable area with a tooltip that appears when the mouse rests on it.
    pub fn hit_tip(&mut self, r: Rect, id: u32, tip: &'static str) {
        self.hits.push(Hit { rect: r, module: self.module, id, tip });
    }

    pub fn hovered(&self, r: Rect) -> bool {
        r.contains(self.mouse.0, self.mouse.1)
    }

    /// Pill button with a text label.
    pub fn button(&mut self, r: Rect, label: &str, id: u32) {
        self.button_c(r, label, id, col::BTN, col::TEXT);
    }

    /// Accent (filled) button.
    pub fn button_accent(&mut self, r: Rect, label: &str, id: u32, accent: Color) {
        let bg = if self.hovered(r) { accent.with_a(0.85) } else { accent };
        self.fill_rr(r, r.h / 2.0, bg);
        self.text(label, r, 12.0, hex(0x010101), Weight::Semi, Align::Center);
        self.hit(r, id);
    }

    pub fn button_c(&mut self, r: Rect, label: &str, id: u32, bg: Color, fg: Color) {
        let bg = if self.hovered(r) { col::BTN_HOVER } else { bg };
        self.fill_rr(r, r.h / 2.0, bg);
        self.text(label, r, 12.0, fg, Weight::Semi, Align::Center);
        self.hit(r, id);
    }

    /// Selectable chip (segmented control item).
    pub fn chip(&mut self, r: Rect, label: &str, id: u32, selected: bool, accent: Color) {
        let bg = if selected {
            accent.with_a(0.25)
        } else if self.hovered(r) {
            col::BTN_HOVER
        } else {
            col::BTN
        };
        self.fill_rr(r, r.h / 2.0, bg);
        let fg = if selected { accent } else { col::DIM };
        self.text(label, r, 11.5, fg, Weight::Semi, Align::Center);
        self.hit(r, id);
    }

    /// Round icon button. `bg = None` draws only a hover circle. `tip` is the
    /// tooltip shown on hover; icon-only buttons should always have one.
    pub fn icon_btn(&mut self, r: Rect, g: &str, id: u32, size: f32, fg: Color, bg: Option<Color>, tip: &'static str) {
        let hov = self.hovered(r);
        let rad = r.w.min(r.h) / 2.0;
        match bg {
            Some(b) => self.fill_rr(r, rad, if hov { b.lerp(col::TEXT, 0.15) } else { b }),
            None if hov => self.fill_rr(r, rad, col::BTN_HOVER),
            None => {}
        }
        self.icon(g, r, size, fg);
        self.hit_tip(r, id, tip);
    }

    /// Thin progress bar. Registers a hit if `id` is given (for seek bars:
    /// `Module::action` receives the click's relative x in `rel.0`).
    pub fn progress(&mut self, r: Rect, frac: f32, fg: Color, id: Option<u32>) {
        let hov = id.is_some() && self.hovered(r.inset_xy(0.0, -6.0));
        let h = if hov { r.h + 2.0 } else { r.h };
        let rr = Rect::new(r.x, r.cy() - h / 2.0, r.w, h);
        self.fill_rr(rr, h / 2.0, col::TEXT.with_a(0.16));
        let f = frac.clamp(0.0, 1.0);
        if f > 0.0 {
            self.fill_rr(Rect::new(rr.x, rr.y, (rr.w * f).max(h), rr.h), h / 2.0, fg);
        }
        if let Some(id) = id {
            self.hit(r.inset_xy(0.0, -6.0), id);
        }
    }

    /// Label on the left, on/off switch on the right; the whole row is clickable.
    pub fn toggle(&mut self, r: Rect, label: &str, on: bool, id: u32, accent: Color) {
        if self.hovered(r) {
            self.fill_rr(r, 8.0, col::TEXT.with_a(0.05));
        }
        let mut row = r.inset_xy(8.0, 0.0);
        let sw = row.cut_right(34.0, 8.0).centered(34.0, 18.0);
        self.text(label, row, 12.5, col::TEXT, Weight::Regular, Align::Left);
        self.fill_rr(sw, 9.0, if on { accent } else { col::TEXT.with_a(0.18) });
        let kx = if on { sw.r() - 9.0 } else { sw.x + 9.0 };
        self.circle(kx, sw.cy(), 7.0, if on { hex(0x010101) } else { col::TEXT });
        self.hit(r, id);
    }

    /// Circular checkbox.
    pub fn check(&mut self, cx: f32, cy: f32, on: bool, accent: Color) {
        if on {
            self.circle(cx, cy, 8.0, accent);
            self.icon(crate::gfx::glyph::CHECK, Rect::new(cx - 8.0, cy - 8.0, 16.0, 16.0), 9.0, hex(0x010101));
        } else {
            self.stroke_circle(cx, cy, 7.5, col::DIM, 1.4);
        }
    }
}

/// A one-line (or, with `multi`, multi-line) text box owned by a module.
/// The module asks for the keyboard (`ctx.want_keyboard(true)`) when it is
/// clicked, then passes keys and characters here.
#[derive(Clone, Debug, Default)]
pub struct TextField {
    pub text: String,
    pub focused: bool,
    pub multi: bool,
    pub max: usize,
}

/// What a key did to a text field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FieldKey {
    /// Nothing for the field.
    Ignored,
    Edited,
    /// Enter (one-line fields).
    Submit,
    /// Esc.
    Cancel,
}

impl TextField {
    pub fn new(max: usize) -> TextField {
        TextField { max, ..Default::default() }
    }
    pub fn multi(max: usize) -> TextField {
        TextField { max, multi: true, ..Default::default() }
    }
    pub fn key(&mut self, vk: u16, ctrl: bool) -> FieldKey {
        if !self.focused {
            return FieldKey::Ignored;
        }
        match vk {
            0x08 => {
                if ctrl {
                    // Ctrl+Backspace: the last word.
                    let t = self.text.trim_end().len();
                    let cut = self.text[..t].rfind(|c: char| c.is_whitespace()).map_or(0, |i| i + 1);
                    self.text.truncate(cut);
                } else {
                    self.text.pop();
                }
                FieldKey::Edited
            }
            0x0D if self.multi && !ctrl => {
                if self.text.len() < self.max {
                    self.text.push('\n');
                }
                FieldKey::Edited
            }
            0x0D => FieldKey::Submit,
            0x1B => FieldKey::Cancel,
            0x56 if ctrl => {
                if let Some(s) = crate::modules::todo::clipboard_text() {
                    for c in s.chars() {
                        if self.text.len() >= self.max {
                            break;
                        }
                        if c == '\n' && self.multi || !c.is_control() {
                            self.text.push(c);
                        }
                    }
                }
                FieldKey::Edited
            }
            _ => FieldKey::Ignored,
        }
    }
    pub fn chr(&mut self, c: char) -> bool {
        if !self.focused || c.is_control() || self.text.len() >= self.max {
            return false;
        }
        self.text.push(c);
        true
    }
}

impl<'a> Ui<'a> {
    /// Draw a text box; clicking it sends `id`. A blinking caret shows while
    /// it has focus, the placeholder while it's empty.
    pub fn field(&mut self, r: Rect, f: &TextField, placeholder: &str, id: u32, accent: Color) {
        text_area(r);
        let hov = self.hovered(r);
        self.fill_rr(r, 9.0, if f.focused { col::BTN_HOVER } else if hov { col::BTN_HOVER } else { col::BTN });
        if f.focused {
            self.stroke_rr(r, 9.0, accent.with_a(0.8), 1.2);
        }
        let inner = r.inset_xy(10.0, if f.multi { 7.0 } else { 0.0 });
        let caret = f.focused && (self.t * 2.0) as i64 % 2 == 0;
        if f.text.is_empty() {
            self.text(placeholder, if f.multi { Rect::new(inner.x, inner.y, inner.w, 16.0) } else { inner }, 12.0, col::FAINT, Weight::Regular, Align::Left);
            if caret {
                self.fill_rect(Rect::new(inner.x, if f.multi { inner.y + 1.0 } else { inner.cy() - 7.0 }, 1.4, 14.0), accent);
            }
        } else if f.multi {
            let shown = format!("{}{}", f.text, if caret { "|" } else { "" });
            self.push_clip(inner);
            self.text_wrap(&shown, inner, 12.0, col::TEXT, Weight::Regular, Align::Left);
            self.pop_clip();
        } else {
            // Long text: show its end.
            let mut s = f.text.clone();
            while self.measure(&s, 12.0, Weight::Regular) > inner.w - 6.0 && !s.is_empty() {
                s.remove(0);
            }
            let w = self.measure(&s, 12.0, Weight::Regular);
            self.text(&s, inner, 12.0, col::TEXT, Weight::Regular, Align::Left);
            if caret {
                self.fill_rect(Rect::new(inner.x + w + 1.0, inner.cy() - 7.0, 1.4, 14.0), accent);
            }
        }
        self.hit(r, id);
    }
}

thread_local! {
    /// Text boxes drawn this frame: the mouse shows the I-beam over them.
    static TEXT_AREAS: std::cell::RefCell<Vec<Rect>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// This rect takes typing: show the text cursor over it.
pub fn text_area(r: Rect) {
    TEXT_AREAS.with(|t| t.borrow_mut().push(r));
}

/// A new frame starts: forget last frame's text boxes.
pub fn clear_text_areas() {
    TEXT_AREAS.with(|t| t.borrow_mut().clear());
}

pub fn over_text(x: f32, y: f32) -> bool {
    TEXT_AREAS.with(|t| t.borrow().iter().any(|r| r.contains(x, y)))
}
