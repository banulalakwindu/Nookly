//! Downloads finished: when a file lands in Downloads, the notch shows it
//! for a little while. Click to open it. Only the folder's own timestamp
//! is read every 2 s; the folder is listed only when that changes.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::gfx::*;
use crate::modules::{live_simple, live_simple_side, Ctx, Module, Rel, Toast};
use crate::ui::Ui;

const A_OPEN: u32 = 1;
const A_FOLDER: u32 = 2;
const SHOW_SECS: f64 = 25.0;
const C_DL: Color = Color { r: 0.35, g: 0.80, b: 1.0, a: 1.0 };

/// Files still being written by a browser or downloader.
fn partial(name: &str) -> bool {
    let l = name.to_lowercase();
    [".crdownload", ".part", ".partial", ".tmp", ".download", ".opdownload"].iter().any(|e| l.ends_with(e)) || l.starts_with('.') || l == "desktop.ini"
}

pub fn downloads_dir() -> Option<PathBuf> {
    use windows::Win32::UI::Shell::{FOLDERID_Downloads, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    unsafe {
        let p = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const _));
        s.map(PathBuf::from)
    }
}

/// The newest finished file in `dir` changed after `since`: (path, size).
pub fn newest_after(dir: &Path, since: SystemTime) -> Option<(PathBuf, u64)> {
    let mut best: Option<(SystemTime, PathBuf, u64)> = None;
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if partial(&name) {
            continue;
        }
        let Ok(md) = e.metadata() else { continue };
        if !md.is_file() {
            continue;
        }
        let t = md.modified().ok().max(md.created().ok())?;
        if t > since && best.as_ref().map_or(true, |b| t > b.0) {
            best = Some((t, e.path(), md.len()));
        }
    }
    best.map(|(_, p, n)| (p, n))
}

pub struct Downloads {
    on: bool,
    dir: Option<PathBuf>,
    dir_time: Option<SystemTime>,
    since: SystemTime,
    checked: f64,
    /// Seen once; shown when its size holds still (still being written).
    pending: Option<(PathBuf, u64)>,
    shown: Option<(PathBuf, f64)>,
    /// Watching a test folder (self-test).
    test: bool,
}

impl Downloads {
    pub fn new() -> Downloads {
        Downloads { on: false, dir: None, dir_time: None, since: SystemTime::now(), checked: 0.0, pending: None, shown: None, test: false }
    }
    fn name(&self) -> String {
        self.shown.as_ref().and_then(|(p, _)| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }
    /// For the self-test: look in this folder from now on.
    pub fn watch(&mut self, dir: PathBuf, since: SystemTime) {
        self.on = true;
        self.test = true;
        self.dir = Some(dir);
        self.dir_time = None;
        self.since = since;
        self.checked = 0.0;
    }
}

fn size_text(n: u64) -> String {
    if n >= 1 << 30 {
        format!("{:.1} GB", n as f64 / (1u64 << 30) as f64)
    } else if n >= 1 << 20 {
        format!("{:.1} MB", n as f64 / (1u64 << 20) as f64)
    } else {
        format!("{} KB", (n / 1024).max(1))
    }
}

impl Module for Downloads {
    fn title(&self) -> &'static str {
        "Downloads"
    }
    fn has_tab(&self) -> bool {
        false
    }
    fn icon(&self) -> &'static str {
        "\u{E896}"
    }
    fn accent(&self) -> Color {
        C_DL
    }
    fn tick(&mut self, ctx: &mut Ctx) {
        if !crate::features::is_on(ctx.settings, "downloads") && !self.test {
            self.on = false;
            self.shown = None;
            return;
        }
        if !self.on {
            self.on = true;
            self.since = SystemTime::now();
            self.dir_time = None;
        }
        let now = crate::time::unix_now();
        if let Some((_, at)) = &self.shown {
            if now - at > SHOW_SECS {
                self.shown = None;
            }
        }
        if now - self.checked < 2.0 {
            return;
        }
        self.checked = now;
        if self.dir.is_none() {
            self.dir = downloads_dir();
        }
        let Some(dir) = self.dir.clone() else { return };
        // Pending file: show it once its size held still for a check.
        if let Some((p, n)) = self.pending.take() {
            match std::fs::metadata(&p) {
                Ok(md) if md.len() == n && n > 0 => {
                    self.shown = Some((p.clone(), now));
                    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    ctx.toast(
                        Toast::new("download", "\u{E896}", C_DL, "Downloaded", name)
                            .right(size_text(n))
                            .action("Open", A_OPEN)
                            .action("Show in folder", A_FOLDER)
                            .click(A_OPEN)
                            .secs(6.0),
                    );
                }
                Ok(md) => self.pending = Some((p, md.len())),
                Err(_) => {}
            }
        }
        let t = std::fs::metadata(&dir).and_then(|m| m.modified()).ok();
        if t != self.dir_time || self.pending.is_none() && self.dir_time.is_none() {
            self.dir_time = t;
            if let Some((p, n)) = newest_after(&dir, self.since) {
                if self.shown.as_ref().map_or(true, |(s, _)| *s != p) {
                    self.since = std::fs::metadata(&p).and_then(|m| m.modified()).unwrap_or_else(|_| SystemTime::now());
                    self.pending = Some((p, n));
                }
            }
        }
    }
    fn action(&mut self, id: u32, _rel: Rel, _ctx: &mut Ctx) {
        let Some((p, _)) = self.shown.clone() else { return };
        match id {
            A_OPEN => crate::app::open_later(p.display().to_string()),
            A_FOLDER => {
                let _ = std::process::Command::new("explorer.exe").arg(format!("/select,{}", p.display())).spawn();
            }
            _ => {}
        }
    }
    fn live(&self) -> Option<u8> {
        self.shown.as_ref().map(|_| 30)
    }
    fn live_id(&self) -> &'static str {
        "downloads"
    }
    fn live_color(&self) -> Color {
        C_DL
    }
    fn draw_live(&mut self, ui: &mut Ui, left: Rect, right: Rect) {
        live_simple(ui, left, right, "\u{E896}", C_DL, &self.name(), "Done");
    }
    fn draw_live_side(&mut self, ui: &mut Ui, r: Rect, right: bool) {
        live_simple_side(ui, r, right, "\u{E896}", C_DL, "Done");
    }
    fn debug_state(&self) -> String {
        format!("downloads on={} shown={}", self.on, self.name())
    }
}
