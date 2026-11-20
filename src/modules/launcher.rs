//! Launcher: pinned apps, folders, files and links, one click away.
//! Drop something on the notch while this tab is open to pin it, or type
//! a link. Saved in launcher.txt (one path or URL per line).

use crate::gfx::*;
use crate::modules::{Ctx, Module, Rel};
use crate::store;
use crate::ui::{FieldKey, TextField, Ui};

const A_FIELD: u32 = 1;
const A_PIN: u32 = 2;
const A_OPEN: u32 = 100; // + index
const A_REMOVE: u32 = 200; // + index
const A_BROWSE: u32 = 3;
const A_FOLDER: u32 = 4;
const MAX: usize = 15;

/// Open the file picker (0 none, 1 apps and files, 2 folders); the app
/// shows it outside drawing, then pins what was picked.
pub static PICK: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

const C_LAUNCH: Color = Color { r: 0.45, g: 0.85, b: 0.55, a: 1.0 };

pub struct Launcher {
    items: Vec<String>,
    field: TextField,
    hover_drop: bool,
}

fn name_of(p: &str) -> String {
    if p.starts_with("http://") || p.starts_with("https://") {
        let host = p.split("://").nth(1).unwrap_or(p).split('/').next().unwrap_or(p);
        return host.trim_start_matches("www.").to_string();
    }
    let n = std::path::Path::new(p).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| p.to_string());
    if n.is_empty() {
        p.to_string()
    } else {
        n
    }
}

fn icon_of(p: &str) -> &'static str {
    let l = p.to_lowercase();
    if l.starts_with("http") {
        "\u{E774}"
    } else if std::path::Path::new(p).is_dir() {
        "\u{E8B7}"
    } else if l.ends_with(".exe") || l.ends_with(".lnk") || l.ends_with(".url") || l.ends_with(".appref-ms") {
        "\u{E7FC}"
    } else {
        "\u{E8A5}"
    }
}

impl Launcher {
    /// Pin what the file picker returned.
    pub fn pin_all(&mut self, paths: Vec<String>) {
        for p in paths {
            self.pin(p);
        }
    }
    pub fn load() -> Launcher {
        if crate::time::demo() {
            let items = ["C:\\Program Files\\Microsoft VS Code\\Code.exe", "C:\\Users\\Public\\Documents", "https://github.com", "C:\\Windows\\notepad.exe", "https://calendar.google.com", "C:\\Users\\Public\\Music", "https://web.whatsapp.com", "C:\\Program Files\\Spotify\\Spotify.exe"];
            return Launcher { items: items.iter().map(|s| s.to_string()).collect(), field: TextField::new(400), hover_drop: false };
        }
        let items = store::read("launcher.txt").lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
        Launcher { items, field: TextField::new(400), hover_drop: false }
    }
    fn save(&self) {
        store::write("launcher.txt", &self.items.join("\n"));
    }
    fn pin(&mut self, p: String) {
        if !p.is_empty() && !self.items.contains(&p) && self.items.len() < MAX {
            self.items.push(p);
            self.save();
        }
    }
}

impl Module for Launcher {
    fn title(&self) -> &'static str {
        "Launcher"
    }
    fn icon(&self) -> &'static str {
        "\u{E8FC}"
    }
    fn accent(&self) -> Color {
        C_LAUNCH
    }
    fn size(&self) -> (f32, f32) {
        (500.0, 170.0)
    }
    fn draw(&mut self, ui: &mut Ui, mut r: Rect) {
        let mut row = r.cut_top(28.0, 8.0);
        ui.button(row.cut_right(76.0, 6.0), "Folder...", A_FOLDER);
        ui.button_accent(row.cut_right(96.0, 6.0), "Add apps...", A_BROWSE, C_LAUNCH);
        if !self.field.text.is_empty() {
            ui.button(row.cut_right(52.0, 6.0), "Pin", A_PIN);
        }
        ui.field(row, &self.field, "Paste a link, or drop apps here", A_FIELD, C_LAUNCH);
        if self.hover_drop {
            ui.stroke_rr(r, 12.0, C_LAUNCH, 1.5);
            ui.text("Release to pin", r, 13.0, C_LAUNCH, Weight::Semi, Align::Center);
            return;
        }
        if self.items.is_empty() {
            ui.text_wrap("Pin the apps, folders and sites you open every day. Drag them onto the notch while this tab is open.", r.inset_xy(4.0, 6.0), 12.0, col::DIM, Weight::Regular, Align::Left);
            return;
        }
        let per = 5;
        let rows = r.rows(3, 6.0);
        for (i, p) in self.items.iter().enumerate().take(MAX) {
            let cell = rows[i / per].columns(per, 6.0)[i % per];
            let hov = ui.hovered(cell);
            ui.fill_rr(cell, 10.0, if hov { col::BTN_HOVER } else { col::BTN });
            let mut c = cell.inset_xy(8.0, 0.0);
            let ic = c.cut_left(16.0, 6.0);
            ui.icon(icon_of(p), ic, 12.0, C_LAUNCH);
            ui.text(&name_of(p), c, 11.5, col::TEXT, Weight::Regular, Align::Left);
            ui.hit(cell, A_OPEN + i as u32);
            if hov {
                let x = Rect::new(cell.r() - 18.0, cell.y + 2.0, 16.0, 16.0);
                ui.icon_btn(x, glyph::CLOSE, A_REMOVE + i as u32, 8.0, col::DIM, Some(col::BTN), "Unpin");
            }
        }
    }
    fn action(&mut self, id: u32, _rel: Rel, ctx: &mut Ctx) {
        match id {
            A_FIELD => {
                self.field.focused = true;
                ctx.want_keyboard(true);
            }
            A_BROWSE => PICK.store(1, std::sync::atomic::Ordering::Relaxed),
            A_FOLDER => PICK.store(2, std::sync::atomic::Ordering::Relaxed),
            A_PIN => {
                let p = self.field.text.trim().trim_matches('"').to_string();
                self.pin(p);
                self.field.text.clear();
            }
            i if (A_OPEN..A_OPEN + 100).contains(&i) => {
                if let Some(p) = self.items.get((i - A_OPEN) as usize) {
                    crate::app::open_later(p.clone());
                    ctx.close_panel();
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
                let p = self.field.text.trim().trim_matches('"').to_string();
                self.pin(p);
                self.field.text.clear();
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
        self.field.chr(c)
    }
    fn blur(&mut self) {
        self.field.focused = false;
    }
    fn picked(&mut self, paths: Vec<String>, _ctx: &mut Ctx) {
        self.pin_all(paths);
    }
    fn wants_drops(&self) -> bool {
        true
    }
    fn drag_hover(&mut self, on: bool) {
        self.hover_drop = on;
    }
    fn drop_files(&mut self, paths: Vec<String>, _ctx: &mut Ctx) -> bool {
        for p in paths {
            self.pin(p);
        }
        true
    }
    fn tip(&self, id: u32) -> &'static str {
        match id {
            A_FIELD => "A link (https://...) or a path to pin",
            A_PIN => "Pin it",
            A_BROWSE => "Pick apps or files to pin (several at once)",
            A_FOLDER => "Pick folders to pin",
            i if (A_OPEN..A_OPEN + 100).contains(&i) => "Open",
            _ => "",
        }
    }
    fn debug_state(&self) -> String {
        format!("launcher n={}", self.items.len())
    }
}
