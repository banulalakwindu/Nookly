//! Quick notes: one scratchpad that saves as you type (notes.txt).

use crate::gfx::*;
use crate::modules::clipboard::{set_clipboard, Content};
use crate::modules::{Ctx, Module, Rel, Toast};
use crate::store;
use crate::ui::{FieldKey, TextField, Ui};

const A_FIELD: u32 = 1;
const A_COPY: u32 = 2;
const A_CLEAR: u32 = 3;
pub const A_FOCUS: u32 = 4;

const C_NOTES: Color = Color { r: 1.0, g: 0.80, b: 0.30, a: 1.0 };

pub struct Notes {
    field: TextField,
}

impl Notes {
    pub fn load() -> Notes {
        let mut field = TextField::multi(20_000);
        field.text = if crate::time::demo() {
            "Call the bank about the card\nIdeas for the weekend: Ella hike, train ride\nWi-Fi at the office: blue-ocean-42".into()
        } else {
            store::read("notes.txt")
        };
        Notes { field }
    }
    fn save(&self) {
        store::write("notes.txt", &self.field.text);
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
        if !self.field.text.is_empty() {
            ui.button(head.cut_right(64.0, 6.0), "Clear", A_CLEAR);
            ui.button(head.cut_right(64.0, 6.0), "Copy", A_COPY);
        }
        let words = self.field.text.split_whitespace().count();
        let info = if self.field.text.is_empty() {
            "Saves as you type. Give it a shortcut in Settings > Features > Keyboard & mouse".to_string()
        } else {
            format!("{words} word{}", if words == 1 { "" } else { "s" })
        };
        ui.text(&info, head, 11.0, col::DIM, Weight::Regular, Align::Left);
        ui.field(r, &self.field, "Jot something down...", A_FIELD, C_NOTES);
    }
    fn action(&mut self, id: u32, _rel: Rel, ctx: &mut Ctx) {
        match id {
            A_FIELD | A_FOCUS => {
                self.field.focused = true;
                ctx.want_keyboard(true);
                if id == A_FOCUS {
                    ctx.open_my_tab();
                }
            }
            A_COPY => {
                if set_clipboard(ctx.hwnd, &Content::Text(self.field.text.clone())) {
                    ctx.toast(Toast::new("notes", glyph::CHECK, C_NOTES, "Note copied", "").secs(2.0));
                }
            }
            A_CLEAR => {
                self.field.text.clear();
                self.save();
            }
            _ => {}
        }
    }
    fn key(&mut self, vk: u16, ctrl: bool, ctx: &mut Ctx) -> bool {
        match self.field.key(vk, ctrl) {
            FieldKey::Edited => {
                self.save();
                true
            }
            FieldKey::Cancel => {
                self.field.focused = false;
                ctx.want_keyboard(false);
                true
            }
            FieldKey::Submit => true,
            FieldKey::Ignored => false,
        }
    }
    fn chr(&mut self, c: char, _ctx: &mut Ctx) -> bool {
        let ok = self.field.chr(c);
        if ok {
            self.save();
        }
        ok
    }
    fn blur(&mut self) {
        self.field.focused = false;
    }
    fn tip(&self, id: u32) -> &'static str {
        match id {
            A_FIELD => "Type a note; it's saved as you go",
            A_COPY => "Copy the whole note",
            A_CLEAR => "Clear the note",
            _ => "",
        }
    }
    fn debug_state(&self) -> String {
        format!("notes chars={}", self.field.text.len())
    }
}
