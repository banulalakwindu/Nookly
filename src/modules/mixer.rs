//! Volume mixer: the master volume and each app that plays sound, with a
//! slider and mute, like Windows' own mixer but one hover away. Read only
//! while the panel is on screen.

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::core::Interface;

use crate::gfx::*;
use crate::modules::{Ctx, Module, Rel};
use crate::time::unix_now;
use crate::ui::Ui;

const A_SLIDE: u32 = 100; // + row (0 = master)
const A_MUTE: u32 = 200; // + row
const C_MIX: Color = Color { r: 0.55, g: 0.85, b: 0.75, a: 1.0 };
const ROW_H: f32 = 30.0;

struct Row {
    name: String,
    level: f32,
    muted: bool,
    /// None for the master volume.
    vol: Option<ISimpleAudioVolume>,
}

pub struct Mixer {
    rows: Vec<Row>,
    master: Option<IAudioEndpointVolume>,
    read_at: f64,
    seen_at: f64,
    /// Slider rects from the last draw (for dragging).
    sliders: Vec<Rect>,
    drag: Option<usize>,
    scroll: f32,
}

fn exe_name(pid: u32) -> Option<String> {
    unsafe {
        let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 520];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
        let _ = CloseHandle(p);
        ok.then(|| String::from_utf16_lossy(&buf[..n as usize])).and_then(|s| std::path::Path::new(&s).file_stem().map(|x| x.to_string_lossy().into_owned()))
    }
}

impl Mixer {
    pub fn new() -> Mixer {
        let mut m = Mixer { rows: Vec::new(), master: None, read_at: 0.0, seen_at: 0.0, sliders: Vec::new(), drag: None, scroll: 0.0 };
        if crate::time::demo() {
            let demo = [("Master", 0.62, false), ("Spotify", 0.8, false), ("Chrome", 0.45, false), ("Teams", 1.0, false), ("System sounds", 0.3, true)];
            m.rows = demo.iter().map(|&(n, l, mu)| Row { name: n.into(), level: l, muted: mu, vol: None }).collect();
        } else {
            // Read once now, so the panel opens at the right height.
            m.read();
        }
        m
    }

    /// Read the master volume and every app's session.
    fn read(&mut self) {
        let mut rows = Vec::new();
        unsafe {
            let Ok(en) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else { return };
            let Ok(dev) = en.GetDefaultAudioEndpoint(eRender, eConsole) else { return };
            let master: Option<IAudioEndpointVolume> = dev.Activate(CLSCTX_ALL, None).ok();
            if let Some(m) = &master {
                rows.push(Row { name: "Master".into(), level: m.GetMasterVolumeLevelScalar().unwrap_or(0.0), muted: m.GetMute().map_or(false, |b| b.as_bool()), vol: None });
            }
            self.master = master;
            let mgr: Option<IAudioSessionManager2> = dev.Activate(CLSCTX_ALL, None).ok();
            if let Some(list) = mgr.and_then(|m| m.GetSessionEnumerator().ok()) {
                let n = list.GetCount().unwrap_or(0);
                let mut seen: Vec<String> = Vec::new();
                for i in 0..n {
                    let Ok(ctl) = list.GetSession(i) else { continue };
                    let Ok(ctl2) = ctl.cast::<IAudioSessionControl2>() else { continue };
                    // Expired sessions are apps that closed.
                    if ctl.GetState().map_or(true, |s| s == AudioSessionStateExpired) {
                        continue;
                    }
                    let name = if ctl2.IsSystemSoundsSession().0 == 0 {
                        "System sounds".to_string()
                    } else {
                        let pid = ctl2.GetProcessId().unwrap_or(0);
                        match exe_name(pid) {
                            Some(n) => crate::modules::health::friendly(&n),
                            None => continue,
                        }
                    };
                    if seen.contains(&name) {
                        continue;
                    }
                    let Ok(v) = ctl.cast::<ISimpleAudioVolume>() else { continue };
                    seen.push(name.clone());
                    rows.push(Row { name, level: v.GetMasterVolume().unwrap_or(0.0), muted: v.GetMute().map_or(false, |b| b.as_bool()), vol: Some(v) });
                }
            }
        }
        self.rows = rows;
    }

    fn set(&mut self, k: usize, level: f32) {
        let level = level.clamp(0.0, 1.0);
        let Some(row) = self.rows.get_mut(k) else { return };
        row.level = level;
        unsafe {
            match (&row.vol, &self.master) {
                (Some(v), _) => {
                    let _ = v.SetMasterVolume(level, std::ptr::null());
                }
                (None, Some(m)) => {
                    let _ = m.SetMasterVolumeLevelScalar(level, std::ptr::null());
                }
                _ => {}
            }
        }
    }

    fn toggle_mute(&mut self, k: usize) {
        let Some(row) = self.rows.get_mut(k) else { return };
        row.muted = !row.muted;
        unsafe {
            match (&row.vol, &self.master) {
                (Some(v), _) => {
                    let _ = v.SetMute(row.muted, std::ptr::null());
                }
                (None, Some(m)) => {
                    let _ = m.SetMute(row.muted, std::ptr::null());
                }
                _ => {}
            }
        }
    }
}

impl Module for Mixer {
    fn title(&self) -> &'static str {
        "Volume"
    }
    fn icon(&self) -> &'static str {
        "\u{E767}"
    }
    fn accent(&self) -> Color {
        C_MIX
    }
    fn size(&self) -> (f32, f32) {
        // As tall as the rows (up to 7, then it scrolls).
        (500.0, (self.rows.len().clamp(1, 7) as f32 * ROW_H - 4.0).max(ROW_H))
    }
    fn fit_height(&self) -> bool {
        true
    }
    fn tick(&mut self, _ctx: &mut Ctx) {
        let now = unix_now();
        // Only while you look at it (and not mid-drag).
        if now - self.seen_at < 2.0 && now - self.read_at > 1.0 && self.drag.is_none() && !crate::time::demo() {
            self.read_at = now;
            self.read();
        }
    }
    fn draw(&mut self, ui: &mut Ui, r: Rect) {
        let now = unix_now();
        if (now - self.seen_at > 2.0 || self.rows.is_empty()) && self.drag.is_none() {
            self.read_at = now;
            if crate::time::demo() {
                let demo = [("Master", 0.62, false), ("Spotify", 0.8, false), ("Chrome", 0.45, false), ("Teams", 1.0, false), ("System sounds", 0.3, true)];
                self.rows = demo.iter().map(|&(n, l, m)| Row { name: n.into(), level: l, muted: m, vol: None }).collect();
            } else {
                self.read();
            }
        }
        self.seen_at = now;
        if self.rows.is_empty() {
            ui.text("No sound device found", r, 12.0, col::DIM, Weight::Regular, Align::Center);
            return;
        }
        let total = self.rows.len() as f32 * ROW_H;
        self.scroll = self.scroll.clamp(0.0, (total - r.h).max(0.0));
        self.sliders.clear();
        ui.push_clip(r.inset(-2.0));
        for (k, row) in self.rows.iter().enumerate() {
            let rr = Rect::new(r.x, r.y + k as f32 * ROW_H - self.scroll, r.w, ROW_H - 4.0);
            self.sliders.push(Rect::new(0.0, 0.0, 0.0, 0.0));
            if rr.b() < r.y || rr.y > r.b() {
                continue;
            }
            if k == 0 {
                ui.fill_rr(rr, 9.0, C_MIX.with_a(0.12));
            }
            let mut c = rr.inset_xy(10.0, 0.0);
            let icon = if row.muted || row.level <= 0.001 { glyph::MUTE } else { glyph::VOLUME };
            ui.icon_btn(c.cut_left(24.0, 6.0).centered(24.0, 24.0), icon, A_MUTE + k as u32, 10.5, if row.muted { col::RED } else { col::DIM }, None, "Mute or unmute");
            ui.text(&row.name, c.cut_left(120.0, 8.0), 12.0, if k == 0 { col::TEXT } else { col::TEXT }, if k == 0 { Weight::Semi } else { Weight::Regular }, Align::Left);
            ui.text(&format!("{:.0}%", row.level * 100.0), c.cut_right(40.0, 6.0), 11.0, col::DIM, Weight::Regular, Align::Right);
            let bar = c.centered(c.w, 6.0);
            self.sliders[k] = bar;
            ui.progress(bar, row.level, if row.muted { col::FAINT } else { C_MIX }, Some(A_SLIDE + k as u32));
            ui.circle(bar.x + bar.w * row.level, bar.cy(), 6.0, if row.muted { col::DIM } else { col::TEXT });
        }
        ui.pop_clip();
    }
    fn action(&mut self, id: u32, rel: Rel, _ctx: &mut Ctx) {
        match id {
            i if (A_SLIDE..A_SLIDE + 64).contains(&i) => self.set((i - A_SLIDE) as usize, rel.0),
            i if (A_MUTE..A_MUTE + 64).contains(&i) => self.toggle_mute((i - A_MUTE) as usize),
            _ => {}
        }
    }
    fn drag_begin(&mut self, id: u32, x: f32, _y: f32) -> bool {
        if !(A_SLIDE..A_SLIDE + 64).contains(&id) {
            return false;
        }
        let k = (id - A_SLIDE) as usize;
        self.drag = Some(k);
        self.drag_to(x, 0.0);
        true
    }
    fn drag_to(&mut self, x: f32, _y: f32) {
        if let Some(k) = self.drag {
            if let Some(b) = self.sliders.get(k).copied().filter(|b| b.w > 0.0) {
                self.set(k, (x - b.x) / b.w);
            }
        }
    }
    fn drag_drop(&mut self, _x: f32, _y: f32, _ctx: &mut Ctx) {
        self.drag = None;
    }
    fn scroll(&mut self, delta: f32) -> bool {
        let total = self.rows.len() as f32 * ROW_H;
        if total <= 7.0 * ROW_H {
            return false;
        }
        self.scroll -= delta * ROW_H;
        true
    }
    fn tip(&self, id: u32) -> &'static str {
        match id {
            i if (A_SLIDE..A_SLIDE + 64).contains(&i) => "Click or drag to set the volume",
            i if (A_MUTE..A_MUTE + 64).contains(&i) => "Mute or unmute",
            _ => "",
        }
    }
    fn debug_state(&self) -> String {
        format!("mixer rows={}", self.rows.len())
    }
}
