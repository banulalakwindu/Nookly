//! System monitor: CPU, memory and network speed, with a minute of history,
//! plus free space on the Windows drive and uptime. CPU, memory and
//! download speed are also widgets for the resting notch. Reads Windows' own counters once a
//! second (the notch already ticks once a second), so it costs nothing
//! extra while idle.

use windows::core::*;
use windows::Win32::NetworkManagement::IpHelper::*;
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::Win32::System::SystemInformation::*;
use windows::Win32::System::Threading::GetSystemTimes;

use crate::gfx::*;
use crate::modules::{Ctx, Module, Rel, Widget};
use crate::time::unix_now;
use crate::ui::Ui;

pub const ACCENT: Color = hex(0x5AC8FA);
pub const ICON: &str = "\u{E9D9}";
const C_CPU: Color = hex(0x5AC8FA);
const C_MEM: Color = hex(0xBF5AF2);
const C_DOWN: Color = hex(0x30D158);
const C_UP: Color = hex(0xFF9F0A);
const HISTORY: usize = 60;

/// Demo video / docs renders: steady made-up numbers.
pub const A_DEMO: u32 = 990;

#[derive(Clone, Copy, Default, Debug)]
pub struct Sample {
    pub cpu: f32,
    pub mem_used: u64,
    pub mem_total: u64,
    pub down: f64,
    pub up: f64,
}

fn ft(f: &windows::Win32::Foundation::FILETIME) -> u64 {
    ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
}

/// (idle, total) CPU time so far, in 100 ns units, over all cores.
pub fn cpu_times() -> Option<(u64, u64)> {
    let (mut i, mut k, mut u) = Default::default();
    unsafe { GetSystemTimes(Some(&mut i), Some(&mut k), Some(&mut u)) }.ok()?;
    // Kernel time includes idle time.
    Some((ft(&i), ft(&k) + ft(&u)))
}

/// Bytes received and sent so far on real network adapters (Wi-Fi,
/// Ethernet, phone tethering). Loopback and the filter layers Windows
/// stacks on each adapter are skipped so nothing is counted twice.
pub fn net_bytes() -> Option<(u64, u64)> {
    unsafe {
        let mut t: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        if GetIfTable2(&mut t).is_err() || t.is_null() {
            return None;
        }
        let n = (*t).NumEntries as usize;
        let rows = std::slice::from_raw_parts((*t).Table.as_ptr(), n);
        let (mut rx, mut tx) = (0u64, 0u64);
        for r in rows {
            let flags = r.InterfaceAndOperStatusFlags._bitfield;
            let hardware = flags & 1 != 0;
            let filter = flags & 2 != 0;
            if hardware && !filter && r.Type != 24 {
                rx += r.InOctets;
                tx += r.OutOctets;
            }
        }
        FreeMibTable(t as *const _);
        Some((rx, tx))
    }
}

pub fn memory() -> (u64, u64) {
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    if unsafe { GlobalMemoryStatusEx(&mut m) }.is_ok() {
        (m.ullTotalPhys - m.ullAvailPhys, m.ullTotalPhys)
    } else {
        (0, 0)
    }
}

/// Free and total bytes on the Windows drive.
pub fn disk() -> (u64, u64) {
    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()) + "\\";
    let (mut free, mut total) = (0u64, 0u64);
    let h = HSTRING::from(drive.as_str());
    let _ = unsafe { GetDiskFreeSpaceExW(&h, None, Some(&mut total), Some(&mut free)) };
    (free, total)
}

/// 1.2 MB/s, 340 KB/s, 0 B/s (1024-based, like Task Manager's file sizes).
pub fn speed(bps: f64) -> String {
    if bps >= 1024.0 * 1024.0 {
        format!("{:.1} MB/s", bps / 1048576.0)
    } else if bps >= 1024.0 {
        format!("{:.0} KB/s", bps / 1024.0)
    } else {
        format!("{:.0} B/s", bps.max(0.0))
    }
}

/// Short form for the closed notch: 1.2M, 340K, 0K.
pub fn speed_short(bps: f64) -> String {
    if bps >= 1024.0 * 1024.0 {
        format!("{:.1}M", bps / 1048576.0)
    } else {
        format!("{:.0}K", bps / 1024.0)
    }
}

pub fn gb(b: u64) -> String {
    let g = b as f64 / 1073741824.0;
    if g >= 100.0 {
        format!("{g:.0} GB")
    } else {
        format!("{g:.1} GB")
    }
}

fn uptime() -> String {
    let s = unsafe { GetTickCount64() } / 1000;
    let (d, h, m) = (s / 86400, (s % 86400) / 3600, (s % 3600) / 60);
    if d > 0 {
        format!("{d} d {h} h")
    } else if h > 0 {
        format!("{h} h {m} min")
    } else {
        format!("{m} min")
    }
}

pub struct Monitor {
    hist: Vec<Sample>,
    last_cpu: Option<(u64, u64)>,
    last_net: Option<(u64, u64, f64)>,
    disk: (u64, u64),
    disk_at: f64,
    demo: bool,
}

impl Monitor {
    pub fn new() -> Monitor {
        let mut m = Monitor { hist: Vec::new(), last_cpu: None, last_net: None, disk: (0, 0), disk_at: 0.0, demo: false };
        m.sample();
        m
    }

    /// Read the counters; speeds are averaged since the previous read.
    pub fn sample(&mut self) {
        if self.demo {
            return;
        }
        let now = unix_now();
        let cpu = match (cpu_times(), self.last_cpu) {
            (Some((i, t)), Some((pi, pt))) if t > pt => 100.0 * (1.0 - (i.saturating_sub(pi)) as f64 / (t - pt) as f64),
            _ => -1.0,
        };
        self.last_cpu = cpu_times();
        let (mut down, mut up) = (-1.0, -1.0);
        if let Some((rx, tx)) = net_bytes() {
            if let Some((prx, ptx, pt)) = self.last_net {
                let dt = now - pt;
                if dt > 0.2 {
                    down = rx.saturating_sub(prx) as f64 / dt;
                    up = tx.saturating_sub(ptx) as f64 / dt;
                }
            }
            self.last_net = Some((rx, tx, now));
        }
        if now - self.disk_at > 30.0 {
            self.disk = disk();
            self.disk_at = now;
        }
        if cpu < 0.0 {
            return;
        }
        let (mem_used, mem_total) = memory();
        self.hist.push(Sample { cpu: cpu.clamp(0.0, 100.0) as f32, mem_used, mem_total, down: down.max(0.0), up: up.max(0.0) });
        if self.hist.len() > HISTORY {
            self.hist.remove(0);
        }
    }

    pub fn latest(&self) -> Option<Sample> {
        self.hist.last().copied()
    }

    fn spark(ui: &mut Ui, r: Rect, vals: &[f32], max: f32, c: Color) {
        if vals.len() < 2 || max <= 0.0 {
            return;
        }
        let step = r.w / (HISTORY - 1) as f32;
        let x0 = r.r() - step * (vals.len() - 1) as f32;
        let y = |v: f32| r.b() - (v / max).clamp(0.0, 1.0) * r.h;
        let mut pts: Vec<(f32, f32)> = vals.iter().enumerate().map(|(i, v)| (x0 + i as f32 * step, y(*v))).collect();
        for w in pts.windows(2) {
            ui.line(w[0].0, w[0].1, w[1].0, w[1].1, c, 1.6);
        }
        pts.push((r.r(), r.b()));
        pts.push((x0, r.b()));
        ui.polygon(&pts, c.with_a(0.14));
    }

    /// A big graph: faint lines at the quarters, the line with a soft fill.
    fn graph(ui: &mut Ui, r: Rect, vals: &[f32], max: f32, c: Color) {
        ui.fill_rr(r, 10.0, col::TEXT.with_a(0.04));
        for k in 1..4 {
            let y = r.y + r.h * k as f32 / 4.0;
            ui.fill_rect(Rect::new(r.x + 6.0, y, r.w - 12.0, 1.0), col::TEXT.with_a(0.06));
        }
        let inner = r.inset_xy(4.0, 6.0);
        crate::faces::spark(ui, inner, &vals.iter().map(|v| v / max).collect::<Vec<f32>>(), c);
    }

    fn card(ui: &mut Ui, r: Rect, title: &str, big: &str, sub: &str, c: Color) -> Rect {
        ui.fill_rr(r, 12.0, col::BTN);
        let mut inner = r.inset_xy(12.0, 9.0);
        ui.text(title, inner.cut_top(14.0, 2.0), 10.5, col::DIM, Weight::Semi, Align::Left);
        ui.text(big, inner.cut_top(26.0, 0.0), 21.0, c, Weight::Semi, Align::Left);
        ui.text(sub, inner.cut_top(15.0, 4.0), 10.5, col::FAINT, Weight::Regular, Align::Left);
        inner
    }
}

impl Module for Monitor {
    fn title(&self) -> &'static str {
        "System Monitor"
    }
    fn icon(&self) -> &'static str {
        ICON
    }
    fn accent(&self) -> Color {
        ACCENT
    }
    fn size(&self) -> (f32, f32) {
        crate::modules::PANEL
    }

    fn tick(&mut self, _ctx: &mut Ctx) {
        // tick also runs right after background wake-ups: one read per second.
        let due = self.last_cpu.is_none() || self.last_net.map_or(true, |(_, _, t)| unix_now() - t >= 0.9);
        if due {
            self.sample();
        }
    }

    fn draw(&mut self, ui: &mut Ui, mut r: Rect) {
        let foot = r.cut_bottom(24.0, 8.0);
        let s = self.latest().unwrap_or_default();
        let cols = r.columns(3, 10.0);
        let cpu: Vec<f32> = self.hist.iter().map(|h| h.cpu).collect();
        let a = Self::card(ui, cols[0], "CPU", &format!("{:.0}%", s.cpu), &format!("{} cores", std::thread::available_parallelism().map_or(0, |n| n.get())), C_CPU);
        Self::spark(ui, a, &cpu, 100.0, C_CPU);

        let mem_pct = if s.mem_total > 0 { s.mem_used as f32 / s.mem_total as f32 } else { 0.0 };
        let mut b = Self::card(ui, cols[1], "Memory", &format!("{:.0}%", mem_pct * 100.0), &format!("{} of {} used", gb(s.mem_used), gb(s.mem_total)), C_MEM);
        let mem: Vec<f32> = self.hist.iter().map(|h| if h.mem_total > 0 { h.mem_used as f32 / h.mem_total as f32 * 100.0 } else { 0.0 }).collect();
        let bar = b.cut_bottom(6.0, 0.0);
        Self::spark(ui, Rect::new(b.x, b.y, b.w, b.h - 6.0), &mem, 100.0, C_MEM);
        ui.fill_rr(bar, 3.0, col::TEXT.with_a(0.1));
        ui.fill_rr(Rect::new(bar.x, bar.y, bar.w * mem_pct.clamp(0.0, 1.0), bar.h), 3.0, C_MEM);

        let net = cols[2];
        ui.fill_rr(net, 12.0, col::BTN);
        let mut inner = net.inset_xy(12.0, 9.0);
        ui.text("Network", inner.cut_top(14.0, 2.0), 10.5, col::DIM, Weight::Semi, Align::Left);
        for (arrow, v, c) in [("\u{2193}", s.down, C_DOWN), ("\u{2191}", s.up, C_UP)] {
            let mut row = inner.cut_top(20.0, 0.0);
            ui.text(arrow, row.cut_left(14.0, 2.0), 14.0, c, Weight::Semi, Align::Left);
            ui.text(&speed(v), row, 14.5, c, Weight::Semi, Align::Left);
        }
        let down: Vec<f32> = self.hist.iter().map(|h| h.down as f32).collect();
        let up: Vec<f32> = self.hist.iter().map(|h| h.up as f32).collect();
        let max = down.iter().chain(up.iter()).cloned().fold(64.0 * 1024.0, f32::max) * 1.15;
        let sp = inner.inset_xy(0.0, 2.0);
        Self::spark(ui, sp, &up, max, C_UP);
        Self::spark(ui, sp, &down, max, C_DOWN);

        let (free, total) = self.disk;
        let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        let mut info = format!("Up {}", if crate::time::demo() { "2 d 5 h".to_string() } else { uptime() });
        if total > 0 {
            info = format!("{drive} {} free of {} · {info}", gb(free), gb(total));
        }
        ui.text(&info, foot, 11.0, col::DIM, Weight::Regular, Align::Left);
    }

    fn action(&mut self, id: u32, _rel: Rel, ctx: &mut Ctx) {
        let _ = ctx;
        match id {
            A_DEMO => {
                self.demo = true;
                self.hist = (0..HISTORY)
                    .map(|i| {
                        let t = i as f32;
                        let cpu = 18.0 + 9.0 * (t * 0.35).sin() + 6.0 * (t * 0.9).cos() + if (38..44).contains(&i) { 30.0 } else { 0.0 };
                        let down = 1.6e6 + 1.2e6 * (t * 0.21).sin().abs() as f64 * 1.0 + if (20..30).contains(&i) { 3.5e6 } else { 0.0 };
                        Sample { cpu, mem_used: 10_200_000_000 + (i as u64) * 4_000_000, mem_total: 17_179_869_184, down, up: 180_000.0 + 90_000.0 * (t * 0.5).cos().abs() as f64 }
                    })
                    .collect();
                self.disk = (213_000_000_000, 511_000_000_000);
                self.disk_at = f64::MAX;
            }
            _ => {}
        }
    }

    fn faces(&self) -> Vec<&'static str> {
        vec!["cpu", "ram", "net", "system"]
    }

    fn full_size(&self, id: &str) -> Option<(f32, f32)> {
        match id {
            "cpu" | "ram" | "net" => Some((440.0, 200.0)),
            "system" => Some((460.0, 230.0)),
            _ => None,
        }
    }

    fn draw_full(&mut self, id: &str, ui: &mut Ui, r: Rect) {
        let r = r.inset(14.0);
        let last: Vec<Sample> = self.hist.iter().copied().collect();
        let s = self.latest().unwrap_or_default();
        let mem_f = |x: &Sample| if x.mem_total > 0 { x.mem_used as f32 / x.mem_total as f32 } else { 0.0 };
        let cpu: Vec<f32> = last.iter().map(|x| x.cpu).collect();
        let mem: Vec<f32> = last.iter().map(|x| mem_f(x) * 100.0).collect();
        let avg = |v: &[f32]| if v.is_empty() { 0.0 } else { v.iter().sum::<f32>() / v.len() as f32 };
        let peak = |v: &[f32]| v.iter().fold(0.0f32, |a, b| a.max(*b));
        let mut r = r;
        let mut foot = r.cut_bottom(30.0, 10.0);
        let w = ui.measure("Task Manager", 12.0, Weight::Semi) + 28.0;
        ui.button(foot.cut_right(w, 0.0), "Task Manager", 1);
        match id {
            "cpu" | "ram" => {
                let (name, c, v, now, sub) = if id == "cpu" {
                    ("Processor", C_CPU, &cpu, format!("{:.0}%", s.cpu), format!("Average {:.0}%  \u{00B7}  peak {:.0}% in the last minute", avg(&cpu), peak(&cpu)))
                } else {
                    ("Memory", C_MEM, &mem, format!("{:.0}%", mem_f(&s) * 100.0), format!("{} of {} in use  \u{00B7}  {} free", gb(s.mem_used), gb(s.mem_total), gb(s.mem_total.saturating_sub(s.mem_used))))
                };
                let mut top = r.cut_top(30.0, 4.0);
                let nw = ui.measure(&now, 24.0, Weight::Bold) + 6.0;
                ui.text(&now, top.cut_left(nw, 8.0), 24.0, c, Weight::Bold, Align::Left);
                ui.text(name, top, 14.0, col::TEXT, Weight::Semi, Align::Left);
                ui.text(&sub, foot, 11.0, col::DIM, Weight::Regular, Align::Left);
                Self::graph(ui, r, v, 100.0, c);
            }
            "net" => {
                let down: Vec<f32> = last.iter().map(|x| x.down as f32).collect();
                let up: Vec<f32> = last.iter().map(|x| x.up as f32).collect();
                let top = peak(&down).max(peak(&up)).max(1024.0);
                let row = r.cut_top(30.0, 4.0);
                let cols = row.columns(2, 10.0);
                ui.text(&format!("\u{2193} {}", speed(s.down)), cols[0], 18.0, C_DOWN, Weight::Bold, Align::Left);
                ui.text(&format!("\u{2191} {}", speed(s.up)), cols[1], 18.0, C_UP, Weight::Bold, Align::Left);
                ui.text(&format!("Peak \u{2193} {}  \u{2191} {} in the last minute", speed(peak(&down) as f64), speed(peak(&up) as f64)), foot, 11.0, col::DIM, Weight::Regular, Align::Left);
                Self::graph(ui, r, &down, top, C_DOWN);
                Self::spark(ui, r, &up, top, C_UP);
            }
            _ => {
                // CPU and memory, a row each.
                ui.text(&format!("Average CPU {:.0}%  \u{00B7}  memory {} of {}", avg(&cpu), gb(s.mem_used), gb(s.mem_total)), foot, 11.0, col::DIM, Weight::Regular, Align::Left);
                let rows = r.rows(2, 10.0);
                for (k, (name, c, v, now)) in [("CPU", C_CPU, &cpu, format!("{:.0}%", s.cpu)), ("Memory", C_MEM, &mem, format!("{:.0}%", mem_f(&s) * 100.0))].into_iter().enumerate() {
                    let mut row = rows[k];
                    let label = row.cut_left(86.0, 10.0);
                    ui.text(name, Rect::new(label.x, label.y + 4.0, label.w, 16.0), 12.0, col::DIM, Weight::Semi, Align::Left);
                    ui.text(&now, Rect::new(label.x, label.y + 20.0, label.w, 28.0), 24.0, c, Weight::Bold, Align::Left);
                    Self::graph(ui, row, v, 100.0, c);
                }
            }
        }
    }

    fn full_action(&mut self, _id: &str, hit: u32, _ctx: &mut Ctx) {
        use windows::Win32::UI::Input::KeyboardAndMouse::*;
        if hit == 1 {
            crate::shell::press(&[VK_CONTROL, VK_SHIFT, VK_ESCAPE]);
        }
    }

    fn face_key(&self, id: &str, size: crate::barcfg::Size) -> String {
        // The graph moves with every sample, Small shows only the number.
        let s = self.latest().unwrap_or_default();
        match size {
            crate::barcfg::Size::S => format!("{id}{:.0}{:.0}{:.0}", s.cpu, s.mem_used as f64 / 1e7, s.down / 1e4),
            _ => format!("{id}{}{:.1}{}{}", self.hist.len(), s.cpu, s.mem_used, s.down),
        }
    }

    fn draw_face(&mut self, id: &str, size: crate::barcfg::Size, ui: &mut Ui, r: Rect) {
        use crate::barcfg::Size;
        use crate::faces::{card, card_rect, disc, ring_w, spark};
        let c = card_rect(r);
        let s = self.latest().unwrap_or_default();
        let last: Vec<Sample> = self.hist.iter().rev().take(40).rev().copied().collect();
        let (name, color, frac, value, detail, vals): (&str, Color, f32, String, String, Vec<f32>) = match id {
            "cpu" => ("CPU", C_CPU, s.cpu / 100.0, format!("{:.0}%", s.cpu), "Processor".to_string(), last.iter().map(|x| x.cpu / 100.0).collect()),
            "ram" => {
                let f = if s.mem_total > 0 { s.mem_used as f32 / s.mem_total as f32 } else { 0.0 };
                ("Memory", C_MEM, f, format!("{:.0}%", f * 100.0), format!("{} of {}", gb(s.mem_used), gb(s.mem_total)), last.iter().map(|x| if x.mem_total > 0 { x.mem_used as f32 / x.mem_total as f32 } else { 0.0 }).collect())
            }
            _ => {
                let top = last.iter().map(|x| x.down.max(x.up)).fold(1.0f64, f64::max);
                ("Network", C_DOWN, 0.0, format!("\u{2193} {}", speed_short(s.down)), format!("\u{2191} {}", speed(s.up)), last.iter().map(|x| (x.down / top) as f32).collect())
            }
        };
        let icon = match id {
            "cpu" => ICON,
            "ram" => "\u{E964}",
            _ => "\u{E896}",
        };
        if id == "system" {
            let cpu_v: Vec<f32> = last.iter().map(|x| x.cpu / 100.0).collect();
            let mem_v: Vec<f32> = last.iter().map(|x| if x.mem_total > 0 { x.mem_used as f32 / x.mem_total as f32 } else { 0.0 }).collect();
            let mem_now = if s.mem_total > 0 { s.mem_used as f32 / s.mem_total as f32 } else { 0.0 };
            card(ui, r, C_CPU.lerp(C_MEM, 0.5));
            let rows = c.inset_xy(8.0, 4.0).rows(2, 2.0);
            for (k, (label, v, now, tint)) in [("CPU", &cpu_v, s.cpu / 100.0, C_CPU), ("MEM", &mem_v, mem_now, C_MEM)].into_iter().enumerate() {
                let mut row = rows[k];
                let lw = if size == Size::M { 22.0 } else { 26.0 };
                ui.text(label, row.cut_left(lw, 4.0), 8.5, tint, Weight::Bold, Align::Left);
                let pct = format!("{:.0}%", now * 100.0);
                ui.text(&pct, row.cut_right(if size == Size::M { 26.0 } else { 30.0 }, 4.0), 10.5, col::TEXT, Weight::Bold, Align::Right);
                if size == Size::W {
                    crate::faces::bar(ui, Rect::new(row.r() - 40.0, row.cy() - 1.5, 40.0, 3.0), now, tint);
                    row.cut_right(46.0, 0.0);
                }
                spark(ui, row.inset_xy(0.0, 1.0), &v.iter().rev().take(if size == Size::M { 24 } else { 40 }).rev().copied().collect::<Vec<f32>>(), tint);
            }
            return;
        }
        let net = id == "net";
        // Busy turns the card warmer.
        let tint = if !net && frac > 0.85 { col::RED } else if !net && frac > 0.65 { col::YELLOW.lerp(color, 0.4) } else { color };
        card(ui, r, tint);
        match size {
            Size::S => {
                if net {
                    ui.icon(icon, Rect::new(c.x, c.y + 2.0, c.w, 17.0), 12.0, tint);
                    ui.text(&speed_short(s.down), Rect::new(c.x, c.y + 18.0, c.w, 13.0), 9.5, col::TEXT, Weight::Bold, Align::Center);
                } else {
                    ring_w(ui, c.cx(), c.cy(), 12.5, frac, tint, 3.0);
                    ui.text(&format!("{:.0}", frac * 100.0), c, 10.0, col::TEXT, Weight::Bold, Align::Center);
                }
            }
            Size::M => {
                let mut row = c.inset_xy(8.0, 0.0);
                ui.text(name, Rect::new(row.x, c.cy() - 15.0, row.w, 13.0), 9.0, tint, Weight::Bold, Align::Left);
                let vw = 46.0;
                ui.text(&value, Rect::new(row.x, c.cy() - 2.5, vw + 10.0, 18.0), 14.0, col::TEXT, Weight::Bold, Align::Left);
                let g = row.cut_right(row.w - vw, 0.0);
                spark(ui, Rect::new(g.x, c.y + 7.0, g.w, c.h - 13.0), &vals, tint);
            }
            Size::L | Size::W => {
                let mut row = c.inset_xy(8.0, 0.0);
                let gw = if size == Size::W { 100.0 } else { 52.0 };
                let g = row.cut_right(gw, 8.0);
                spark(ui, Rect::new(g.x, c.y + 6.0, g.w, c.h - 11.0), &vals, tint);
                let ic = row.cut_left(24.0, 7.0);
                disc(ui, ic.cx(), ic.cy(), 11.0, tint, icon);
                let small = if size == Size::W { detail } else { name.to_string() };
                ui.text(&value, Rect::new(row.x, c.cy() - 16.0, row.w, 19.0), 15.0, col::TEXT, Weight::Bold, Align::Left);
                ui.text(&small, Rect::new(row.x, c.cy() + 2.0, row.w, 13.0), 10.0, col::DIM, Weight::Regular, Align::Left);
            }
        }
    }

    fn widgets(&mut self) -> Vec<Widget> {
        let Some(s) = self.latest() else { return Vec::new() };
        let mem = if s.mem_total > 0 { s.mem_used as f32 / s.mem_total as f32 } else { 0.0 };
        vec![
            Widget::new("cpu", "CPU", ICON, C_CPU, format!("{:.0}%", s.cpu)).ring(s.cpu / 100.0).detail(format!("{:.0}% in use", s.cpu)),
            Widget::new("ram", "Memory", "\u{E964}", C_MEM, format!("{:.0}%", mem * 100.0)).label("RAM").ring(mem).detail(format!("{} of {}", gb(s.mem_used), gb(s.mem_total))),
            Widget::new("net", "Network", "\u{E896}", C_DOWN, speed_short(s.down))
                .label("Down")
                .detail(format!("\u{2193} {}  \u{2191} {}", speed(s.down), speed(s.up))),
        ]
    }

    fn debug_state(&self) -> String {
        match self.latest() {
            Some(s) => format!("cpu {:.1}%, mem {} of {}, down {}, up {}, disk {} free, samples {}", s.cpu, gb(s.mem_used), gb(s.mem_total), speed(s.down), speed(s.up), gb(self.disk.0), self.hist.len()),
            None => "no samples yet".to_string(),
        }
    }
}
