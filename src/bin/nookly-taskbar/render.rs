//! `nookly-taskbar --render-docs <dir>`: the website's pictures of the
//! taskbar, drawn by the taskbar itself with made-up apps, windows, progress
//! and tray icons (none of yours: your apps, files and windows are never
//! looked at), and the widget faces the notch's own docs render drew
//! (`bar_faces_<theme>.bin`). Nothing is shown on the screen, nothing is
//! registered with Windows, and no other program is told anything.
//!
//! Each scene is written as `tb_<name>.raw` (u32 width, u32 height, then
//! premultiplied BGRA): the bottom of a 1280 x 720 screen at 2x, the bar
//! and its pop-ups on a transparent ground. docs/tools/make_taskbar.py puts
//! them on a desktop.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use nookly_core::gfx::*;
use nookly_core::surfaces::Face;
use nookly_core::ui::Ui;
use windows::Win32::Foundation::{HWND, RECT};

use crate::apps::{App, Group};
use crate::{BAR_H, Bar, Pop, items};

/// The screen (DIPs) and the scale it's drawn at.
const W: f32 = 1280.0;
const SH: f32 = 720.0;
const K: f32 = 2.0;
/// How much of the screen's bottom a picture holds (DIPs).
const SHOT_H: f32 = 520.0;

/// Made-up windows: their titles, and which are minimized.
static WINDOWS: Mutex<Option<HashMap<isize, (String, bool, u8)>>> = Mutex::new(None);

/// A made-up window's title (None: not one of them).
pub fn title(h: HWND) -> Option<String> {
    WINDOWS.lock().ok()?.as_ref()?.get(&(h.0 as isize)).map(|w| w.0.clone())
}

/// Is this made-up window minimized?
pub fn minimized(h: HWND) -> bool {
    WINDOWS.lock().ok().and_then(|g| g.as_ref().and_then(|m| m.get(&(h.0 as isize)).map(|w| w.1))).unwrap_or(false)
}

/// A made-up window's picture, where its live preview would be: a page, a
/// mailbox or a document, in the app's colours.
pub fn preview(ui: &mut Ui, r: Rect, h: HWND) {
    let kind = WINDOWS.lock().ok().and_then(|g| g.as_ref().and_then(|m| m.get(&(h.0 as isize)).map(|w| w.2))).unwrap_or(0);
    let accent = [hex(0x3B82F6), hex(0x0F6CBD), hex(0xF59E0B), hex(0x7C3AED), hex(0x10B981)][kind as usize % 5];
    ui.g.fill_rr_raw(r, 8.0, hex(0xF6F7FB));
    let mut c = r.inset(7.0);
    let bar = c.cut_top(12.0, 6.0);
    ui.g.fill_rr_raw(bar, 4.0, hex(0xE3E7EF));
    ui.g.fill_rr_raw(Rect::new(bar.x + 4.0, bar.y + 3.0, 34.0, 6.0), 3.0, accent.with_a(0.8));
    match kind % 3 {
        // A page: a picture and lines of text.
        0 => {
            let pic = c.cut_left(c.w * 0.42, 8.0);
            ui.g.fill_rr_raw(pic, 6.0, accent.with_a(0.85));
            ui.g.ellipse_raw(pic.x + pic.w * 0.7, pic.y + pic.h * 0.3, pic.w * 0.12, pic.w * 0.12, hex(0xFFFFFF).with_a(0.8));
            for k in 0..6 {
                let y = c.y + 2.0 + k as f32 * 11.0;
                if y + 5.0 > c.b() {
                    break;
                }
                ui.g.fill_rr_raw(Rect::new(c.x, y, c.w * [0.9, 0.7, 0.85, 0.6, 0.8, 0.5][k], 5.0), 2.5, hex(0xC9CFDA));
            }
        }
        // A mailbox: a list down the side, a message on the right.
        1 => {
            let list = c.cut_left(c.w * 0.38, 6.0);
            for k in 0..6 {
                let row = Rect::new(list.x, list.y + k as f32 * 14.0, list.w, 11.0);
                if row.b() > list.b() {
                    break;
                }
                ui.g.fill_rr_raw(row, 3.0, if k == 1 { accent.with_a(0.25) } else { hex(0xE6E9F0) });
            }
            ui.g.fill_rr_raw(Rect::new(c.x, c.y, c.w * 0.6, 8.0), 3.0, hex(0x9AA3B2));
            for k in 0..5 {
                ui.g.fill_rr_raw(Rect::new(c.x, c.y + 16.0 + k as f32 * 10.0, c.w * [0.95, 0.8, 0.9, 0.6, 0.4][k], 4.0), 2.0, hex(0xCDD3DD));
            }
        }
        // A document: a heading and paragraphs.
        _ => {
            ui.g.fill_rr_raw(Rect::new(c.x + c.w * 0.1, c.y, c.w * 0.5, 9.0), 3.0, hex(0x30343C));
            for k in 0..7 {
                let y = c.y + 18.0 + k as f32 * 9.0;
                if y + 4.0 > c.b() {
                    break;
                }
                ui.g.fill_rr_raw(Rect::new(c.x + c.w * 0.1, y, c.w * [0.8, 0.75, 0.8, 0.4, 0.8, 0.7, 0.55][k], 4.0), 2.0, hex(0xCDD3DD));
            }
        }
    }
}

fn hw(n: isize) -> HWND {
    HWND((0x7A00 + n) as *mut _)
}

/// An app of the demo: Windows' own apps (their icons come from this PC).
fn app(id: &str, name: &str, pinned: bool, windows: &[(isize, &str, bool, u8)]) -> App {
    let launch = if id.contains('\\') { id.to_string() } else { format!("shell:AppsFolder\\{id}") };
    if let Ok(mut g) = WINDOWS.lock() {
        let m = g.get_or_insert_with(HashMap::new);
        for &(n, t, min, kind) in windows {
            m.insert(hw(n).0 as isize, (t.to_string(), min, kind));
        }
    }
    App { key: id.to_lowercase(), name: name.into(), launch, pinned, windows: windows.iter().map(|w| hw(w.0)).collect(), front: windows.first().map(|w| hw(w.0)), target: None }
}

fn demo_apps(b: &mut Bar) {
    let list = vec![
        app("MSEdge", "Microsoft Edge", true, &[(1, "Q3 report - Dashboard - Microsoft Edge", false, 0), (2, "Design review notes - Microsoft Edge", false, 2), (3, "Flights to Colombo - Microsoft Edge", true, 0)]),
        app("C:\\Windows\\explorer.exe", "File Explorer", true, &[(4, "Downloads", false, 2), (5, "Projects", false, 2)]),
        app("Microsoft.OutlookForWindows_8wekyb3d8bbwe!Microsoft.OutlookforWindows", "Outlook", true, &[(6, "Inbox - Outlook", false, 1)]),
        app("MSTeams_8wekyb3d8bbwe!MSTeams", "Microsoft Teams", true, &[(7, "Chat | Design team | Microsoft Teams", false, 1)]),
        app("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App", "Terminal", true, &[(8, "PowerShell", false, 2)]),
        app("Microsoft.Windows.Photos_8wekyb3d8bbwe!App", "Photos", true, &[]),
        app("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic", "Media Player", false, &[(9, "Media Player", false, 0)]),
        app("Microsoft.WindowsNotepad_8wekyb3d8bbwe!App", "Notepad", false, &[(10, "Ideas.txt - Notepad", false, 2)]),
        app("Microsoft.Paint_8wekyb3d8bbwe!App", "Paint", false, &[(11, "Logo sketch - Paint", false, 0)]),
        app("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App", "Calculator", false, &[(12, "Calculator", false, 2)]),
        app("Microsoft.Todos_8wekyb3d8bbwe!App", "Microsoft To Do", false, &[(13, "My Day - To Do", false, 1)]),
        app("windows.immersivecontrolpanel_cw5n1h2txyewy!microsoft.windows.immersivecontrolpanel", "Settings", false, &[(14, "Settings", false, 2)]),
        app("Microsoft.ScreenSketch_8wekyb3d8bbwe!App", "Snipping Tool", false, &[(15, "Snipping Tool", false, 0)]),
    ];
    let groups = vec![Group {
        name: "Make".into(),
        members: vec![
            app("Microsoft.WindowsAlarms_8wekyb3d8bbwe!App", "Clock", false, &[]),
            app("Microsoft.MicrosoftStickyNotes_8wekyb3d8bbwe!App", "Sticky Notes", false, &[]),
            app("Microsoft.BingWeather_8wekyb3d8bbwe!App", "Weather", false, &[]),
            app("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App", "Calculator", false, &[]),
        ],
    }];
    b.apps.list = list;
    b.apps.groups = groups;
    b.apps.active = hw(1);
    // Progress and badges, as apps report them: a copy, a download, mail.
    crate::band::demo(hw(4), crate::band::State::Normal, 0.62, None);
    crate::band::demo(hw(2), crate::band::State::Paused, 0.35, None);
    let n = b.apps.icon(&b.apps.list[3].clone(), 32).cloned();
    crate::band::demo(hw(6), crate::band::State::None, 0.0, badge(3).or(n));
    crate::unread::demo(4);
    // Hidden icons: a few apps' own (their app icons stand in).
    let mut tray = Vec::new();
    for (k, (i, tip)) in [(1, "OneDrive - Up to date"), (3, "Microsoft Teams"), (11, "Settings"), (6, "Media Player"), (7, "Notepad"), (0, "Microsoft Edge")].iter().enumerate() {
        let a = b.apps.list[*i].clone();
        let icon = b.apps.icon(&a, 32).cloned();
        tray.push(crate::tray::TrayIcon { owner: 1, id: k as u32, guid: None, callback: 0, version: 4, tip: tip.to_string(), hidden: false, icon, icon_ver: 1, pid: 0 });
    }
    crate::tray::demo(tray);
}

/// A red count badge, as Outlook draws its own (premultiplied BGRA).
fn badge(n: u32) -> Option<crate::apps::Icon> {
    let mut g = Gfx::small(32, 32).ok()?;
    g.begin(1.0);
    g.circle(16.0, 16.0, 15.0, hex(0xE5484D));
    g.text(&n.to_string(), Rect::new(0.0, 0.0, 32.0, 32.0), 19.0, hex(0xFFFFFF), Weight::Bold, Align::Center);
    g.end().then(|| (32, g.pixels()))
}

/// The faces the notch's docs render drew for this theme.
fn faces(dir: &Path, theme: &str) -> Vec<Face> {
    let Ok(b) = std::fs::read(dir.join(format!("bar_faces_{theme}.bin"))) else { return Vec::new() };
    let rd = |o: usize| b.get(o..o + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]));
    let mut out = Vec::new();
    let mut at = 4;
    for _ in 0..rd(0).unwrap_or(0) {
        let Some(n) = rd(at) else { break };
        let id = String::from_utf8_lossy(b.get(at + 4..at + 4 + n as usize).unwrap_or(&[])).to_string();
        at += 4 + n as usize;
        let (Some(w), Some(h), Some(dw), Some(len)) = (rd(at), rd(at + 4), rd(at + 8), rd(at + 12)) else { break };
        at += 16;
        let Some(px) = b.get(at..at + len as usize) else { break };
        at += len as usize;
        out.push(Face { id, w, h, dip_w: f32::from_bits(dw), px: px.to_vec() });
    }
    out
}

/// Liquid Glass: what's behind, from the docs' desktop picture
/// (NOOKLY_GLASS_SCENE: u32 w, u32 h, BGRA; the screen at 2x), as the
/// capture of `r` (pixels) would be.
pub fn scene(r: RECT) -> Option<nookly_core::glass::Backdrop> {
    let b = std::fs::read(std::env::var("NOOKLY_GLASS_SCENE").ok()?).ok()?;
    let w = u32::from_le_bytes(b.get(0..4)?.try_into().ok()?) as i32;
    let h = u32::from_le_bytes(b.get(4..8)?.try_into().ok()?) as i32;
    let px = b.get(8..)?;
    let (x0, y0, x1, y1) = (r.left.max(0), r.top.max(0), r.right.min(w), r.bottom.min(h));
    let (sw, sh) = (((x1 - x0) / 4).max(2), ((y1 - y0) / 4).max(2));
    let mut out = vec![0u8; (sw * sh * 4) as usize];
    for y in 0..sh {
        for x in 0..sw {
            let mut acc = [0u32; 4];
            for dy in 0..4 {
                for dx in 0..4 {
                    let (sx, sy) = ((x0 + x * 4 + dx).min(w - 1), (y0 + y * 4 + dy).min(h - 1));
                    let i = ((sy * w + sx) * 4) as usize;
                    for c in 0..4 {
                        acc[c] += px.get(i + c).copied().unwrap_or(0) as u32;
                    }
                }
            }
            let o = ((y * sw + x) * 4) as usize;
            for c in 0..4 {
                out[o + c] = (acc[c] / 16) as u8;
            }
        }
    }
    Some(nookly_core::glass::finish(sw as u32, sh as u32, out))
}

/// Let the springs come to rest, drawing as it goes.
fn settle(b: &mut Bar) {
    let t0 = Instant::now();
    loop {
        let moving = b.step();
        b.tick_glass();
        b.render();
        if (!moving && t0.elapsed() > Duration::from_millis(250)) || t0.elapsed() > Duration::from_secs(5) {
            break;
        }
        std::thread::sleep(Duration::from_millis(8));
    }
    b.render();
}

/// The bar and its pop-ups as they'd be on the screen: the bottom
/// `SHOT_H` of it.
fn write(b: &mut Bar, dir: &Path, name: &str) {
    let (cw, ch) = ((W * K) as usize, (SHOT_H * K) as usize);
    let mut canvas = vec![0u8; cw * ch * 4];
    let over = |canvas: &mut Vec<u8>, px: &[u8], pw: usize, ph: usize, ox: i32, oy: i32| {
        for y in 0..ph {
            let cy = oy + y as i32;
            if cy < 0 || cy >= ch as i32 {
                continue;
            }
            for x in 0..pw {
                let cx = ox + x as i32;
                if cx < 0 || cx >= cw as i32 {
                    continue;
                }
                let s = (y * pw + x) * 4;
                let a = px[s + 3] as u32;
                if a == 0 {
                    continue;
                }
                let d = (cy as usize * cw + cx as usize) * 4;
                for c in 0..4 {
                    canvas[d + c] = (px[s + c] as u32 + canvas[d + c] as u32 * (255 - a) / 255).min(255) as u8;
                }
            }
        }
    };
    let Some(g) = b.gfx.as_ref() else { return };
    let (pw, ph) = (g.pw as usize, g.ph as usize);
    let top = ch as i32 - ph as i32;
    let bar_px = g.pixels();
    for (sub, bb) in [(false, b.pop_b.as_ref()), (true, b.sub_b.as_ref())] {
        let Some(bb) = bb.filter(|x| x.visible && x.active()) else { continue };
        let _ = sub;
        let px = bb.gfx.pixels();
        over(&mut canvas, &px, bb.gfx.pw as usize, bb.gfx.ph as usize, (bb.origin.0 * K).round() as i32, top + (bb.origin.1 * K).round() as i32);
    }
    over(&mut canvas, &bar_px, pw, ph, 0, top);
    let mut out = Vec::with_capacity(8 + canvas.len());
    out.extend_from_slice(&(cw as u32).to_le_bytes());
    out.extend_from_slice(&(ch as u32).to_le_bytes());
    out.extend_from_slice(&canvas);
    let _ = std::fs::write(dir.join(format!("tb_{name}.raw")), out);
    println!("  tb_{name}");
}

/// A bar of the demo in `theme` (on the main screen, or another one).
fn bar(dir: &Path, theme: &str, light: bool, other: bool) -> Option<Bar> {
    let mut b = Bar::new(if other { "DEMO2" } else { "" }).ok()?;
    b.settings.extra.insert("bar_theme".into(), theme.into());
    b.load_settings();
    b.settings.extra.insert("bar_theme".into(), theme.into());
    demo_apps(&mut b);
    b.faces.list = faces(dir, &format!("{theme}{}", if light { "_light" } else { "" }));
    b.feed = std::fs::read_to_string(dir.join("bar_feed.txt")).map(|s| nookly_core::feed::decode(&s)).unwrap_or_default();
    b.glass_light = light;
    b.mon = RECT { left: 0, top: 0, right: (W * K) as i32, bottom: (SH * K) as i32 };
    b.scale = K;
    b.size_window();
    settle(&mut b);
    Some(b)
}

fn tile_x(b: &Bar, id: u32) -> f32 {
    b.hits.iter().find(|h| h.id == id).map_or(W / 2.0, |h| h.rect.cx())
}

/// Open `p` and let it settle.
fn open(b: &mut Bar, p: Pop) {
    b.set_pop(Some(p));
    settle(b);
}

pub fn run(dir: &Path) -> i32 {
    crate::DEMO.store(true, Ordering::Relaxed);
    nookly_core::time::DEMO.store(true, Ordering::Relaxed);
    let _ = std::fs::create_dir_all(dir);
    // (No console: a failure is written down.)
    let log = dir.join("render_error.txt");
    let _ = std::fs::remove_file(&log);
    std::panic::set_hook(Box::new(move |info| {
        let _ = std::fs::write(&log, info.to_string());
    }));
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
    }
    if crate::register_classes().is_err() {
        return 1;
    }
    let Some(mut b) = bar(dir, "default", false, false) else { return 1 };
    write(&mut b, dir, "bar");

    // An app's windows, with their previews (one minimized).
    let x = tile_x(&b, items::APP);
    b.mouse = (x, b.h - BAR_H / 2.0);
    open(&mut b, Pop::App(0, x));
    write(&mut b, dir, "windows");

    // The right-click menu, with the app's recent pages and tasks.
    b.mouse = (-1e4, -1e4);
    b.set_pop(None);
    settle(&mut b);
    let edge = b.apps.list[0].clone();
    let mut m = crate::menu::Menu::for_app(edge, None);
    let jump = |l: &str, recent: bool| crate::jump::Entry { label: l.into(), path: String::new(), args: String::new(), recent, folder: false };
    m.add_jump(vec![jump("Q3 report - Dashboard", true), jump("Flights to Colombo", true), jump("Design review notes", true), jump("New InPrivate window", false), jump("History", false)]);
    b.menu = Some(m);
    let x = tile_x(&b, items::APP);
    open(&mut b, Pop::Menu(x));
    write(&mut b, dir, "menu");

    // The apps that don't fit, and the windows of one grown out of them.
    b.set_pop(None);
    b.settings.extra.insert("bar_max_apps".into(), "8".into());
    settle(&mut b);
    let x = tile_x(&b, items::OVERFLOW);
    open(&mut b, Pop::Overflow(x));
    let more = items::more_apps(&b);
    if let Some((k, a)) = more.iter().find(|(_, a)| !a.windows.is_empty()).cloned() {
        let cx = b.hits.iter().find(|h| h.id == items::POP_APP + k as u32).map_or(x, |h| h.rect.cx());
        b.open_sub(a, cx);
        settle(&mut b);
    }
    write(&mut b, dir, "more");

    // A collection.
    b.set_pop(None);
    settle(&mut b);
    let x = tile_x(&b, items::GROUP);
    open(&mut b, Pop::Group(0, x));
    write(&mut b, dir, "group");

    // The hidden icons.
    b.set_pop(None);
    settle(&mut b);
    let x = tile_x(&b, items::TRAY);
    open(&mut b, Pop::Tray(x));
    write(&mut b, dir, "tray");

    // A widget's own view (the weather's).
    b.set_pop(None);
    settle(&mut b);
    if let Some(k) = b.placed.iter().position(|p| p.id == "weather") {
        let x = tile_x(&b, items::WIDGET + k as u32);
    open(&mut b, Pop::Widget(k, x));
        write(&mut b, dir, "widget");
    }

    // From the keyboard: the ring on an app, and progress, a badge, and an
    // app asking to be looked at.
    b.set_pop(None);
    b.settings.extra.remove("bar_max_apps");
    b.apps.attention = vec![hw(7)];
    b.attention_at = Some(Instant::now() - Duration::from_secs(10));
    b.keys = Some((Some(items::APP + 1), HWND::default()));
    settle(&mut b);
    write(&mut b, dir, "marks");
    drop(b);

    // Themes, and Liquid Glass over the desktop (dark and light).
    for (theme, light, name) in [("paper", false, "paper"), ("ocean", false, "ocean"), ("glass", false, "glass"), ("glass", true, "glass_light")] {
        // Light glass over a light desktop (its own picture).
        if let Ok(p) = std::env::var(if light { "NOOKLY_GLASS_SCENE_LIGHT" } else { "NOOKLY_GLASS_SCENE_DARK" }) {
            unsafe { std::env::set_var("NOOKLY_GLASS_SCENE", p) };
        }
        if let Some(mut b) = bar(dir, theme, light, false) {
            write(&mut b, dir, &format!("theme_{name}"));
        }
    }
    // A bar on another screen: the apps and widgets there.
    if let Some(mut b) = bar(dir, "default", false, true) {
        write(&mut b, dir, "other");
    }
    0
}
