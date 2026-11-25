//! Drawing layer: Direct2D into a 32-bit DIB that is pushed to a layered
//! window with per-pixel alpha (fully transparent pixels are click-through).
//!
//! All coordinates are in DIPs; `scale` maps them to physical pixels.

use std::collections::HashMap;
use std::mem::ManuallyDrop;

use windows::core::*;
use windows_numerics::{Matrix3x2, Vector2};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub use crate::store::Edge;

// ---------------------------------------------------------------- geometry

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Rect { x, y, w, h }
    }
    pub fn r(&self) -> f32 {
        self.x + self.w
    }
    pub fn b(&self) -> f32 {
        self.y + self.h
    }
    pub fn cx(&self) -> f32 {
        self.x + self.w / 2.0
    }
    pub fn cy(&self) -> f32 {
        self.y + self.h / 2.0
    }
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.r() && py >= self.y && py < self.b()
    }
    pub fn inset(&self, d: f32) -> Rect {
        self.inset_xy(d, d)
    }
    pub fn inset_xy(&self, dx: f32, dy: f32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, (self.w - 2.0 * dx).max(0.0), (self.h - 2.0 * dy).max(0.0))
    }
    /// Slice `w` off the left; `self` keeps the rest minus `gap`.
    pub fn cut_left(&mut self, w: f32, gap: f32) -> Rect {
        let out = Rect::new(self.x, self.y, w.min(self.w), self.h);
        self.x += w + gap;
        self.w = (self.w - w - gap).max(0.0);
        out
    }
    pub fn cut_right(&mut self, w: f32, gap: f32) -> Rect {
        let out = Rect::new(self.r() - w.min(self.w), self.y, w.min(self.w), self.h);
        self.w = (self.w - w - gap).max(0.0);
        out
    }
    pub fn cut_top(&mut self, h: f32, gap: f32) -> Rect {
        let out = Rect::new(self.x, self.y, self.w, h.min(self.h));
        self.y += h + gap;
        self.h = (self.h - h - gap).max(0.0);
        out
    }
    pub fn cut_bottom(&mut self, h: f32, gap: f32) -> Rect {
        let out = Rect::new(self.x, self.b() - h.min(self.h), self.w, h.min(self.h));
        self.h = (self.h - h - gap).max(0.0);
        out
    }
    /// A `w`×`h` rect centred inside this one.
    pub fn centered(&self, w: f32, h: f32) -> Rect {
        Rect::new(self.cx() - w / 2.0, self.cy() - h / 2.0, w, h)
    }
    /// Split horizontally into `n` equal columns with `gap` between.
    /// Split into `n` equal rows separated by `gap`.
    pub fn rows(&self, n: usize, gap: f32) -> Vec<Rect> {
        let n = n.max(1);
        let h = (self.h - gap * (n - 1) as f32) / n as f32;
        (0..n).map(|i| Rect::new(self.x, self.y + i as f32 * (h + gap), self.w, h)).collect()
    }

    pub fn columns(&self, n: usize, gap: f32) -> Vec<Rect> {
        let n = n.max(1);
        let cw = (self.w - gap * (n as f32 - 1.0)) / n as f32;
        (0..n).map(|i| Rect::new(self.x + i as f32 * (cw + gap), self.y, cw, self.h)).collect()
    }
    fn d2d(&self) -> D2D_RECT_F {
        D2D_RECT_F { left: self.x, top: self.y, right: self.r(), bottom: self.b() }
    }
}

// ------------------------------------------------------------------ colour

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

pub const fn hex(v: u32) -> Color {
    Color {
        r: ((v >> 16) & 0xff) as f32 / 255.0,
        g: ((v >> 8) & 0xff) as f32 / 255.0,
        b: (v & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

impl Color {
    pub const fn with_a(self, a: f32) -> Color {
        Color { r: self.r, g: self.g, b: self.b, a }
    }
    pub fn mul_a(self, a: f32) -> Color {
        Color { a: self.a * a, ..self }
    }
    pub fn lerp(self, o: Color, t: f32) -> Color {
        Color {
            r: self.r + (o.r - self.r) * t,
            g: self.g + (o.g - self.g) * t,
            b: self.b + (o.b - self.b) * t,
            a: self.a + (o.a - self.a) * t,
        }
    }
}

/// Palette. Change these to re-theme the whole notch.
pub mod col {
    use super::{hex, Color};
    pub const NOTCH: Color = hex(0x000000);
    pub const TEXT: Color = hex(0xF5F5F7);
    pub const DIM: Color = hex(0x9A9AA0);
    pub const FAINT: Color = hex(0x5A5A60);
    pub const BTN: Color = hex(0xFFFFFF).with_a(0.09);
    pub const BTN_HOVER: Color = hex(0xFFFFFF).with_a(0.17);
    pub const ACCENT: Color = hex(0xFF9F0A);
    pub const GREEN: Color = hex(0x30D158);
    pub const BLUE: Color = hex(0x0A84FF);
    pub const CYAN: Color = hex(0x64D2FF);
    pub const RED: Color = hex(0xFF453A);
    pub const YELLOW: Color = hex(0xFFD60A);
    pub const PURPLE: Color = hex(0xBF5AF2);
    pub const PINK: Color = hex(0xFF375F);
}

/// A colour theme (Settings > Look). Drawing code keeps using `col::`;
/// the brush swaps the neutral colours (background, text, overlays) for the
/// theme's, so every panel follows without changes of its own.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Theme {
    pub key: &'static str,
    pub name: &'static str,
    pub bg: Color,
    pub text: Color,
    pub dim: Color,
    pub faint: Color,
    /// What white overlays (buttons, hover) become.
    pub over: Color,
    /// Tooltip background.
    pub tip: Color,
    /// Other colours are multiplied by this (darker accents on light).
    pub tint: f32,
    /// Liquid Glass: a blurred view of what is behind the notch.
    pub glass: bool,
}

/// Relative luminance (WCAG) of a colour.
pub fn luminance(c: Color) -> f32 {
    let f = |v: f32| if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
    0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b)
}

/// WCAG contrast ratio of two colours (1 to 21).
pub fn contrast(a: Color, b: Color) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// Your own theme from a background and a text colour; the dim and faint
/// texts sit between them, and coloured labels darken on a light one.
pub fn custom_theme(bg: u32, text: u32) -> Theme {
    let (b, t) = (hex(bg), hex(text));
    let light = luminance(b) > 0.45;
    Theme {
        key: "custom",
        name: "Custom",
        bg: b,
        text: t,
        dim: t.lerp(b, 0.36),
        faint: t.lerp(b, 0.58),
        over: t,
        tip: b.lerp(t, 0.14),
        tint: if light { 0.6 } else { 1.0 },
        glass: false,
    }
}

const fn theme(key: &'static str, name: &'static str, bg: Color, text: u32, dim: u32, faint: u32, over: u32, tip: Color, tint: f32, glass: bool) -> Theme {
    Theme { key, name, bg, text: hex(text), dim: hex(dim), faint: hex(faint), over: hex(over), tip, tint, glass }
}

pub const THEMES: [Theme; 10] = [
    theme("default", "Black", hex(0x000000), 0xF5F5F7, 0x9A9AA0, 0x5A5A60, 0xFFFFFF, hex(0x2A2A2E), 1.0, false),
    theme("graphite", "Graphite", hex(0x1C1C1E), 0xF5F5F7, 0xA1A1A6, 0x6E6E73, 0xFFFFFF, hex(0x3A3A3C), 1.0, false),
    theme("midnight", "Midnight", hex(0x0A1430), 0xEAF0FF, 0x9AAACB, 0x5B6A8A, 0xC8D6FF, hex(0x1A2648), 1.0, false),
    theme("rose", "Rose", hex(0x3A0F26), 0xFFF0F6, 0xF0A8C6, 0xA4708A, 0xFFD6E8, hex(0x541A3A), 1.0, false),
    theme("forest", "Forest", hex(0x0F2A1D), 0xEAF7EE, 0x9FCDB0, 0x5F8C73, 0xCDEFD9, hex(0x1B3F2C), 1.0, false),
    theme("ocean", "Ocean", hex(0x0B2A3A), 0xE8F6FB, 0x95C5D8, 0x5A8A9D, 0xC9EAF6, hex(0x15394D), 1.0, false),
    theme("light", "Light", hex(0xF5F5F7), 0x1C1C1E, 0x6E6E73, 0x9C9CA1, 0x000000, hex(0xFFFFFF), 0.6, false),
    theme("blossom", "Blossom", hex(0xFBE6EE), 0x3A1424, 0x87556A, 0xB4899B, 0x3A1424, hex(0xFFF5F9), 0.6, false),
    theme("paper", "Paper", hex(0xF0E6CF), 0x3A2E1C, 0x75664E, 0xA39276, 0x4A3A22, hex(0xFBF5E6), 0.6, false),
    theme("glass", "Liquid Glass", hex(0x000000).with_a(0.26), 0xFFFFFF, 0xDCDCE2, 0xA8A8B0, 0xFFFFFF, hex(0x2A2A2E).with_a(0.92), 1.0, true),
];

/// Liquid Glass over a bright backdrop: light glass with dark text.
pub const GLASS_LIGHT: Theme = theme("glass", "Liquid Glass", hex(0xFFFFFF).with_a(0.38), 0x111114, 0x3A3A40, 0x6E6E73, 0x000000, hex(0xF2F2F7).with_a(0.95), 0.8, true);

/// The theme `key` stands for (a key from `THEMES`, or "custom" for your
/// own colours from Settings). Liquid Glass turns light over a bright
/// backdrop (`glass_light`).
pub fn pick_theme(st: &crate::store::Settings, key: &str, glass_light: bool) -> Theme {
    match key {
        "glass" if glass_light => GLASS_LIGHT,
        "custom" => {
            let get = |k: &str, d: u32| st.extra.get(k).and_then(|v| u32::from_str_radix(v, 16).ok()).unwrap_or(d);
            custom_theme(get("custom_bg", 0x1B1B2F), get("custom_text", 0xF5F5F7))
        }
        // The taskbar's own colours (Settings > Taskbar > Theme > Yours).
        "custom_bar" => {
            let get = |k: &str, d: u32| st.extra.get(k).and_then(|v| u32::from_str_radix(v, 16).ok()).unwrap_or(d);
            let mut t = custom_theme(get("bar_custom_bg", 0x1B1B2F), get("bar_custom_text", 0xF5F5F7));
            t.key = "custom_bar";
            t
        }
        k => THEMES.iter().find(|t| t.key == k).copied().unwrap_or(THEMES[0]),
    }
}

/// Segoe MDL2 / Fluent icon glyphs used across the app.
pub mod glyph {
    pub const PLAY: &str = "\u{E768}";
    pub const PAUSE: &str = "\u{E769}";
    pub const PREV: &str = "\u{E892}";
    pub const NEXT: &str = "\u{E893}";
    pub const MUSIC: &str = "\u{E8D6}";
    pub const LYRICS: &str = "\u{E8E4}";
    pub const TIMER: &str = "\u{E916}";
    pub const TODO: &str = "\u{E9D5}";
    pub const SHELF: &str = "\u{E7B8}";
    pub const FOLDER: &str = "\u{E8B7}";
    pub const FILE: &str = "\u{E7C3}";
    pub const LINK: &str = "\u{E71B}";
    pub const ADD: &str = "\u{E710}";
    pub const DELETE: &str = "\u{E74D}";
    pub const CLOSE: &str = "\u{E711}";
    pub const CHECK: &str = "\u{E73E}";
    pub const SETTINGS: &str = "\u{E713}";
    pub const BACK: &str = "\u{E72B}";
    pub const RESET: &str = "\u{E72C}";
    pub const SKIP: &str = "\u{E893}";
    pub const ALARM: &str = "\u{E823}";
    pub const BATTERY: &str = "\u{E83F}";
    pub const CHARGING: &str = "\u{E83E}";
    pub const WIFI: &str = "\u{E701}";
    pub const WIFI_OFF: &str = "\u{EB5E}";
    pub const NO_NET: &str = "\u{E871}";
    pub const BLUETOOTH: &str = "\u{E702}";
    pub const HEADPHONE: &str = "\u{E7F6}";
    pub const VOLUME: &str = "\u{E767}";
    pub const MUTE: &str = "\u{E74F}";
    pub const KEYBOARD: &str = "\u{E765}";
    pub const MORE: &str = "\u{E712}";
    pub const CAPS: &str = "\u{E752}";
    pub const DROP: &str = "\u{EB42}";
    pub const COPY: &str = "\u{E8C8}";
    pub const PASTE: &str = "\u{E77F}";
    pub const PIN: &str = "\u{E718}";
    pub const MIC: &str = "\u{E720}";
    pub const CAMERA: &str = "\u{E722}";
    pub const IMAGE: &str = "\u{EB9F}";
    pub const BELL: &str = "\u{EA8F}";
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Weight {
    Regular,
    Semi,
    Bold,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Family {
    Text,
    Icon,
    Mono,
}

// -------------------------------------------------------------------- Gfx

pub struct Gfx {
    factory: ID2D1Factory,
    dw: IDWriteFactory,
    rt: ID2D1DCRenderTarget,
    brush: ID2D1SolidColorBrush,
    formats: HashMap<(u32, Weight, Align, Family, bool), IDWriteTextFormat>,
    icon_family: HSTRING,
    mem_dc: HDC,
    dib: HBITMAP,
    old_bmp: HGDIOBJ,
    bits: *mut u8,
    /// Last EndDraw failure (None = last frame OK).
    pub last_err: Option<HRESULT>,
    pub pw: i32,
    pub ph: i32,
    pub scale: f32,
    /// Global opacity multiplier applied to every draw call.
    pub alpha: f32,
    /// Bumped whenever the render target is recreated; bitmaps made with an
    /// older generation must be rebuilt (see `music.rs`).
    pub generation: u32,
    /// Unique per drawing surface: bitmaps belong to the surface that made
    /// them (another window's can't draw them).
    pub id: u32,
    layers: Vec<ID2D1Layer>,
    /// The colour theme every draw call goes through.
    pub theme: Theme,
    /// The part of the surface drawn into (pixels: x, y, w, h). See `set_view`.
    view: (i32, i32, i32, i32),
    /// Drawn without the GPU.
    soft: bool,
    /// What's on the screen (the pixels last presented), where, and when.
    shown: Vec<u8>,
    shown_at: Option<(isize, i32, i32, std::time::Instant)>,
}

/// The rectangle that differs between two pictures `pw` pixels wide (its x
/// in steps of 16 pixels), None when they're the same.
fn changed(a: &[u8], b: &[u8], pw: usize) -> Option<RECT> {
    let stride = pw * 4;
    let rows = a.len() / stride.max(1);
    let same = |y: usize| a[y * stride..(y + 1) * stride] == b[y * stride..(y + 1) * stride];
    let top = (0..rows).find(|&y| !same(y))?;
    let bottom = (top..rows).rev().find(|&y| !same(y)).unwrap_or(top) + 1;
    const STEP: usize = 64;
    let cols = stride.div_ceil(STEP);
    let (mut l, mut r) = (cols, 0);
    for y in top..bottom {
        let (ra, rb) = (&a[y * stride..(y + 1) * stride], &b[y * stride..(y + 1) * stride]);
        let part = |k: usize| k * STEP..((k + 1) * STEP).min(stride);
        if let Some(k) = (0..l).find(|&k| ra[part(k)] != rb[part(k)]) {
            l = k;
        }
        if let Some(k) = (r..cols).rev().find(|&k| ra[part(k)] != rb[part(k)]) {
            r = k + 1;
        }
    }
    Some(RECT { left: (l * STEP / 4) as i32, top: top as i32, right: (r * STEP / 4).min(pw) as i32, bottom: bottom as i32 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changed_part() {
        let (w, h) = (100usize, 20usize);
        let a = vec![0u8; w * h * 4];
        assert!(changed(&a, &a, w).is_none());
        let mut b = a.clone();
        b[(5 * w + 40) * 4] = 1;
        b[(9 * w + 70) * 4 + 3] = 1;
        let r = changed(&b, &a, w).unwrap();
        assert_eq!((r.left, r.top, r.right, r.bottom), (32, 5, 80, 10));
        // The last, narrower step.
        let mut c = a.clone();
        c[(19 * w + 99) * 4] = 1;
        let r = changed(&c, &a, w).unwrap();
        assert_eq!((r.left, r.top, r.right, r.bottom), (96, 19, 100, 20));
    }
}

fn d2c(c: Color, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: c.r, g: c.g, b: c.b, a: c.a * a }
}

fn pt(x: f32, y: f32) -> Vector2 {
    Vector2 { X: x, Y: y }
}

fn ident() -> Matrix3x2 {
    Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: 0.0, M32: 0.0 }
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

impl Gfx {
    pub fn new(pw: i32, ph: i32) -> Result<Gfx> {
        Self::make(pw, ph, false)
    }

    /// One that draws without the GPU: for small pictures that's cheaper
    /// than sending them there and back.
    pub fn small(pw: i32, ph: i32) -> Result<Gfx> {
        Self::make(pw, ph, true)
    }

    fn make(pw: i32, ph: i32, soft: bool) -> Result<Gfx> {
        unsafe {
            let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dw: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let rt = Self::make_rt(&factory, soft)?;
            let brush = rt.CreateSolidColorBrush(&d2c(col::TEXT, 1.0), None)?;

            // Win11 ships "Segoe Fluent Icons"; Win10 has "Segoe MDL2 Assets".
            let mut icon_family = HSTRING::from("Segoe MDL2 Assets");
            let mut coll: Option<IDWriteFontCollection> = None;
            if dw.GetSystemFontCollection(&mut coll, false).is_ok() {
                if let Some(coll) = coll {
                    let mut idx = 0u32;
                    let mut exists = BOOL(0);
                    let name = HSTRING::from("Segoe Fluent Icons");
                    if coll.FindFamilyName(&name, &mut idx, &mut exists).is_ok() && exists.as_bool() {
                        icon_family = name;
                    }
                }
            }

            let mem_dc = CreateCompatibleDC(None);
            let mut g = Gfx {
                factory,
                dw,
                rt,
                brush,
                formats: HashMap::new(),
                icon_family,
                mem_dc,
                dib: HBITMAP::default(),
                old_bmp: HGDIOBJ::default(),
                bits: std::ptr::null_mut(),
                last_err: None,
                pw: 0,
                ph: 0,
                scale: 1.0,
                alpha: 1.0,
                generation: 0,
                id: {
                    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
                    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                },
                layers: Vec::new(),
                theme: THEMES[0],
                view: (0, 0, 0, 0),
                soft,
                shown: Vec::new(),
                shown_at: None,
            };
            g.resize(pw, ph)?;
            Ok(g)
        }
    }

    unsafe fn make_rt(factory: &ID2D1Factory, soft: bool) -> Result<ID2D1DCRenderTarget> { unsafe {
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: if soft { D2D1_RENDER_TARGET_TYPE_SOFTWARE } else { D2D1_RENDER_TARGET_TYPE_DEFAULT },
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let rt = factory.CreateDCRenderTarget(&props)?;
        rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
        Ok(rt)
    }}

    pub fn resize(&mut self, pw: i32, ph: i32) -> Result<()> {
        if pw == self.pw && ph == self.ph {
            return Ok(());
        }
        unsafe {
            if !self.dib.is_invalid() {
                SelectObject(self.mem_dc, self.old_bmp);
                let _ = DeleteObject(self.dib.into());
            }
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: pw,
                    biHeight: -ph,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            self.dib = CreateDIBSection(Some(self.mem_dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)?;
            self.old_bmp = SelectObject(self.mem_dc, self.dib.into());
            self.bits = bits as *mut u8;
            self.pw = pw;
            self.ph = ph;
            let rc = RECT { left: 0, top: 0, right: pw, bottom: ph };
            self.rt.BindDC(self.mem_dc, &rc)?;
            self.view = (0, 0, pw, ph);
            self.shown_at = None;
        }
        Ok(())
    }

    /// Draw only into this part of the surface from now on (DIPs; None: all
    /// of it). The rest is empty. What's drawn costs the GPU by its size:
    /// the closed notch is a small part of its window.
    pub fn set_view(&mut self, r: Option<Rect>, scale: f32) {
        let v = match r {
            Some(r) => {
                // In steps of 16 pixels, so it isn't rebound for every pixel.
                let x0 = ((r.x * scale).floor() as i32).clamp(0, self.pw) / 16 * 16;
                let y0 = ((r.y * scale).floor() as i32).clamp(0, self.ph) / 16 * 16;
                let x1 = (((r.r() * scale).ceil() as i32 + 15) / 16 * 16).clamp(x0 + 1, self.pw.max(x0 + 1));
                let y1 = (((r.b() * scale).ceil() as i32 + 15) / 16 * 16).clamp(y0 + 1, self.ph.max(y0 + 1));
                (x0, y0, x1 - x0, y1 - y0)
            }
            None => (0, 0, self.pw, self.ph),
        };
        if v == self.view || self.bits.is_null() {
            return;
        }
        unsafe {
            let rc = RECT { left: v.0, top: v.1, right: v.0 + v.2, bottom: v.1 + v.3 };
            if self.rt.BindDC(self.mem_dc, &rc).is_err() {
                return;
            }
            // Nothing stays from what was drawn outside it.
            let _ = GdiFlush();
            std::ptr::write_bytes(self.bits, 0, (self.pw * self.ph * 4) as usize);
        }
        self.view = v;
    }

    pub fn begin(&mut self, scale: f32) {
        self.scale = scale;
        self.alpha = 1.0;
        unsafe {
            self.rt.BeginDraw();
            self.rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            let m = Matrix3x2 { M11: scale, M12: 0.0, M21: 0.0, M22: scale, M31: -self.view.0 as f32, M32: -self.view.1 as f32 };
            self.rt.SetTransform(&m);
        }
    }

    /// Finish the frame without showing it anywhere (its pixels stay in
    /// `pixels()`): false if it failed.
    pub fn end(&mut self) -> bool {
        let res = unsafe { self.rt.EndDraw(None, None) };
        self.after_end(res)
    }

    fn after_end(&mut self, res: Result<()>) -> bool {
        self.last_err = res.as_ref().err().map(|e| e.code());
        if let Err(e) = res {
            if e.code() == D2DERR_RECREATE_TARGET {
                if let Ok(rt) = unsafe { Self::make_rt(&self.factory, self.soft) } {
                    self.rt = rt;
                    if let Ok(b) = unsafe { self.rt.CreateSolidColorBrush(&d2c(col::TEXT, 1.0), None) } {
                        self.brush = b;
                    }
                    self.layers.clear();
                    self.generation += 1;
                    let (pw, ph) = (self.pw, self.ph);
                    self.pw = 0;
                    let _ = self.resize(pw, ph);
                }
            }
            return false;
        }
        true
    }

    /// Finish the frame and push it to the layered window at screen (x, y).
    pub fn end_and_present(&mut self, hwnd: HWND, x: i32, y: i32) {
        unsafe {
            let res = self.rt.EndDraw(None, None);
            self.last_err = res.as_ref().err().map(|e| e.code());
            if let Err(e) = res {
                if e.code() == D2DERR_RECREATE_TARGET {
                    if let Ok(rt) = Self::make_rt(&self.factory, self.soft) {
                        self.rt = rt;
                        if let Ok(b) = self.rt.CreateSolidColorBrush(&d2c(col::TEXT, 1.0), None) {
                            self.brush = b;
                        }
                        self.layers.clear();
                        self.generation += 1;
                        let (pw, ph) = (self.pw, self.ph);
                        self.pw = 0;
                        let _ = self.resize(pw, ph);
                    }
                }
                return;
            }
            // Only what changed is sent to the screen: Windows puts that part
            // together again and nothing else. The same picture: nothing is
            // sent at all. (All of it when the window is elsewhere than it
            // was left, and now and then anyway.)
            let _ = GdiFlush();
            let mut at = RECT::default();
            let in_place = self
                .shown_at
                .is_some_and(|(h, sx, sy, since)| h == hwnd.0 as isize && (sx, sy) == (x, y) && since.elapsed().as_secs() < 30)
                && GetWindowRect(hwnd, &mut at).is_ok()
                && (at.left, at.top, at.right - at.left, at.bottom - at.top) == (x, y, self.pw, self.ph);
            let len = (self.pw * self.ph * 4) as usize;
            let mut dirty = RECT { left: 0, top: 0, right: self.pw, bottom: self.ph };
            if !self.bits.is_null() {
                let now = std::slice::from_raw_parts(self.bits, len);
                if in_place && self.shown.len() == len {
                    let Some(d) = changed(now, &self.shown, self.pw as usize) else { return };
                    dirty = d;
                    let stride = self.pw as usize * 4;
                    let (a, b) = (dirty.top as usize * stride, dirty.bottom as usize * stride);
                    self.shown[a..b].copy_from_slice(&now[a..b]);
                } else {
                    self.shown.clear();
                    self.shown.extend_from_slice(now);
                }
            }
            let screen = GetDC(None);
            let pos = POINT { x, y };
            let size = SIZE { cx: self.pw, cy: self.ph };
            let src = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let info = UPDATELAYEREDWINDOWINFO {
                cbSize: std::mem::size_of::<UPDATELAYEREDWINDOWINFO>() as u32,
                hdcDst: screen,
                pptDst: &pos,
                psize: &size,
                hdcSrc: self.mem_dc,
                pptSrc: &src,
                crKey: COLORREF(0),
                pblend: &blend,
                dwFlags: ULW_ALPHA,
                prcDirty: &dirty,
            };
            let ok = UpdateLayeredWindowIndirect(hwnd, &info).as_bool();
            let since = match self.shown_at {
                Some((h, sx, sy, s)) if in_place && h == hwnd.0 as isize && (sx, sy) == (x, y) => s,
                _ => std::time::Instant::now(),
            };
            self.shown_at = ok.then_some((hwnd.0 as isize, x, y, since));
            ReleaseDC(None, screen);
        }
    }

    /// Number of non-transparent pixels in the last frame (self-test aid).
    pub fn coverage(&self) -> usize {
        if self.bits.is_null() {
            return 0;
        }
        let n = (self.pw * self.ph) as usize;
        let px = unsafe { std::slice::from_raw_parts(self.bits, n * 4) };
        px.chunks_exact(4).filter(|p| p[3] > 0).count()
    }

    /// Bounding box (x0, y0, x1, y1) of non-transparent pixels (self-test aid).
    pub fn bbox(&self) -> Option<(i32, i32, i32, i32)> {
        if self.bits.is_null() {
            return None;
        }
        let px = unsafe { std::slice::from_raw_parts(self.bits, (self.pw * self.ph * 4) as usize) };
        let mut b: Option<(i32, i32, i32, i32)> = None;
        for y in 0..self.ph {
            for x in 0..self.pw {
                if px[((y * self.pw + x) * 4 + 3) as usize] > 0 {
                    b = Some(match b {
                        None => (x, y, x, y),
                        Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
                    });
                }
            }
        }
        b
    }

    /// Copy of the last frame as premultiplied BGRA (docs image export).
    pub fn pixels(&self) -> Vec<u8> {
        if self.bits.is_null() {
            return Vec::new();
        }
        unsafe { std::slice::from_raw_parts(self.bits, (self.pw * self.ph * 4) as usize).to_vec() }
    }

    /// Number of bright (text/icon-like) pixels in the last frame.
    pub fn bright(&self) -> usize {
        if self.bits.is_null() {
            return 0;
        }
        let n = (self.pw * self.ph) as usize;
        let px = unsafe { std::slice::from_raw_parts(self.bits, n * 4) };
        px.chunks_exact(4).filter(|p| p[0] as u32 + p[1] as u32 + p[2] as u32 > 300).count()
    }

    fn set(&self, c: Color) {
        unsafe { self.brush.SetColor(&d2c(self.themed(c), self.alpha)) }
    }

    /// A colour as the current theme draws it.
    pub fn themed(&self, c: Color) -> Color {
        let t = &self.theme;
        if t.key == "default" {
            return c;
        }
        let is = |k: Color| c.r == k.r && c.g == k.g && c.b == k.b;
        let with = |n: Color| Color { a: c.a * n.a, ..n };
        if is(col::NOTCH) {
            with(t.bg)
        } else if is(col::TEXT) {
            with(t.text)
        } else if is(col::DIM) {
            with(t.dim)
        } else if is(col::FAINT) {
            with(t.faint)
        } else if is(hex(0xFFFFFF)) {
            with(t.over)
        } else if is(hex(0x2A2A2E)) {
            with(t.tip)
        } else if t.tint < 1.0 {
            Color { r: c.r * t.tint, g: c.g * t.tint, b: c.b * t.tint, a: c.a }
        } else {
            c
        }
    }

    /// Colours exactly as given (theme previews, the glass rim).
    fn set_raw(&self, c: Color) {
        unsafe { self.brush.SetColor(&d2c(c, self.alpha)) }
    }

    pub fn fill_rr_raw(&mut self, r: Rect, rad: f32, c: Color) {
        self.set_raw(c);
        let rr = D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: rad, radiusY: rad };
        unsafe { self.rt.FillRoundedRectangle(&rr, &self.brush) }
    }

    pub fn stroke_geometry_raw(&mut self, geo: &ID2D1PathGeometry, c: Color, w: f32) {
        self.set_raw(c);
        unsafe { self.rt.DrawGeometry(geo, &self.brush, w, None) }
    }

    // ------------------------------------------------------------ shapes

    pub fn fill_rect(&mut self, r: Rect, c: Color) {
        self.set(c);
        unsafe { self.rt.FillRectangle(&r.d2d(), &self.brush) }
    }

    pub fn fill_rr(&mut self, r: Rect, rad: f32, c: Color) {
        self.set(c);
        let rr = D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: rad, radiusY: rad };
        unsafe { self.rt.FillRoundedRectangle(&rr, &self.brush) }
    }

    /// A rounded outline in exactly this colour (themes don't touch it).
    pub fn stroke_rr_raw(&mut self, r: Rect, rad: f32, c: Color, w: f32) {
        self.set_raw(c);
        let rr = D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: rad, radiusY: rad };
        unsafe { self.rt.DrawRoundedRectangle(&rr, &self.brush, w, None) }
    }

    /// A rounded rectangle filled with colours that blend from one to the
    /// next: `stops` are (position 0..1, colour), left to right (or top to
    /// bottom when `down`).
    pub fn fill_rr_stops(&mut self, r: Rect, rad: f32, stops: &[(f32, Color)], down: bool) {
        if stops.len() < 2 {
            if let Some(&(_, c)) = stops.first() {
                self.fill_rr(r, rad, c);
            }
            return;
        }
        let list: Vec<D2D1_GRADIENT_STOP> = stops.iter().map(|&(p, c)| D2D1_GRADIENT_STOP { position: p, color: d2c(self.themed(c), self.alpha) }).collect();
        unsafe {
            let Ok(coll) = self.rt.CreateGradientStopCollection(&list, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP) else { return };
            let end = if down { pt(r.x, r.b()) } else { pt(r.r(), r.y) };
            let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: pt(r.x, r.y), endPoint: end };
            let Ok(br) = self.rt.CreateLinearGradientBrush(&props, None, &coll) else { return };
            let rr = D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: rad, radiusY: rad };
            self.rt.FillRoundedRectangle(&rr, &br);
        }
    }

    /// A rounded rectangle blending from `a` to `b`.
    pub fn fill_rr_grad(&mut self, r: Rect, rad: f32, a: Color, b: Color, down: bool) {
        self.fill_rr_stops(r, rad, &[(0.0, a), (1.0, b)], down);
    }

    pub fn stroke_rr(&mut self, r: Rect, rad: f32, c: Color, w: f32) {
        self.set(c);
        let rr = D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: rad, radiusY: rad };
        unsafe { self.rt.DrawRoundedRectangle(&rr, &self.brush, w, None) }
    }

    /// An ellipse in exactly this colour (themes don't touch it).
    pub fn ellipse_raw(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, c: Color) {
        self.set_raw(c);
        let e = D2D1_ELLIPSE { point: pt(cx, cy), radiusX: rx, radiusY: ry };
        unsafe { self.rt.FillEllipse(&e, &self.brush) }
    }

    pub fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, c: Color) {
        self.set(c);
        let e = D2D1_ELLIPSE { point: pt(cx, cy), radiusX: rx, radiusY: ry };
        unsafe { self.rt.FillEllipse(&e, &self.brush) }
    }

    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, c: Color) {
        self.ellipse(cx, cy, r, r, c);
    }

    pub fn stroke_circle(&mut self, cx: f32, cy: f32, r: f32, c: Color, w: f32) {
        self.set(c);
        let e = D2D1_ELLIPSE { point: pt(cx, cy), radiusX: r, radiusY: r };
        unsafe { self.rt.DrawEllipse(&e, &self.brush, w, None) }
    }

    pub fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, c: Color, w: f32) {
        self.set(c);
        unsafe { self.rt.DrawLine(pt(x1, y1), pt(x2, y2), &self.brush, w, None) }
    }

    fn path(&self, build: impl FnOnce(&ID2D1GeometrySink)) -> Option<ID2D1PathGeometry> {
        unsafe {
            let geo = self.factory.CreatePathGeometry().ok()?;
            let sink = geo.Open().ok()?;
            build(&sink);
            sink.Close().ok()?;
            Some(geo)
        }
    }

    /// Stroke a circular arc. Angles in radians, 0 = 12 o'clock, clockwise.
    pub fn arc(&mut self, cx: f32, cy: f32, r: f32, a0: f32, sweep: f32, c: Color, w: f32) {
        if sweep.abs() < 0.001 {
            return;
        }
        let sweep = sweep.clamp(-6.2831, 6.2831);
        let p = |a: f32| pt(cx + r * a.sin(), cy - r * a.cos());
        let start = p(a0);
        let end = p(a0 + sweep);
        let geo = self.path(|s| unsafe {
            s.BeginFigure(start, D2D1_FIGURE_BEGIN_HOLLOW);
            s.AddArc(&D2D1_ARC_SEGMENT {
                point: end,
                size: D2D_SIZE_F { width: r, height: r },
                rotationAngle: 0.0,
                sweepDirection: if sweep > 0.0 { D2D1_SWEEP_DIRECTION_CLOCKWISE } else { D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE },
                arcSize: if sweep.abs() > std::f32::consts::PI { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
            });
            s.EndFigure(D2D1_FIGURE_END_OPEN);
        });
        if let Some(geo) = geo {
            self.set(c);
            unsafe {
                let style = self
                    .factory
                    .CreateStrokeStyle(
                        &D2D1_STROKE_STYLE_PROPERTIES {
                            startCap: D2D1_CAP_STYLE_ROUND,
                            endCap: D2D1_CAP_STYLE_ROUND,
                            ..Default::default()
                        },
                        None,
                    )
                    .ok();
                self.rt.DrawGeometry(&geo, &self.brush, w, style.as_ref());
            }
        }
    }

    pub fn polygon(&mut self, pts: &[(f32, f32)], c: Color) {
        if pts.len() < 3 {
            return;
        }
        let geo = self.path(|s| unsafe {
            s.BeginFigure(pt(pts[0].0, pts[0].1), D2D1_FIGURE_BEGIN_FILLED);
            for p in &pts[1..] {
                s.AddLine(pt(p.0, p.1));
            }
            s.EndFigure(D2D1_FIGURE_END_CLOSED);
        });
        if let Some(geo) = geo {
            self.set(c);
            unsafe { self.rt.FillGeometry(&geo, &self.brush, None) }
        }
    }

    /// The notch silhouette: flat side flush with the screen `edge`, rounded
    /// corners on the inner side, and small concave "ears" flaring into the edge.
    /// Built in edge-local coordinates (u along the edge, v away from it).
    pub fn notch_geometry(&self, r: Rect, rad: f32, ear: f32, edge: Edge) -> Option<ID2D1PathGeometry> {
        self.notch_path(r, rad, ear, edge, true)
    }

    /// The same silhouette's outline without the side along the screen
    /// edge (from one ear's tip round to the other's), for strokes.
    pub fn notch_outline(&self, r: Rect, rad: f32, ear: f32, edge: Edge) -> Option<ID2D1PathGeometry> {
        self.notch_path(r, rad, ear, edge, false)
    }

    fn notch_path(&self, r: Rect, rad: f32, ear: f32, edge: Edge, closed: bool) -> Option<ID2D1PathGeometry> {
        let (len, dep) = if edge.vertical() { (r.h, r.w) } else { (r.w, r.h) };
        let rad = rad.min(dep / 2.0).min(len / 2.0).max(0.0);
        let map = |u: f32, v: f32| match edge {
            Edge::Top => pt(r.x + u, r.y + v),
            Edge::Bottom => pt(r.x + u, r.b() - v),
            Edge::Left => pt(r.x + v, r.y + u),
            Edge::Right => pt(r.r() - v, r.y + u),
        };
        // Mirrored mappings reverse the arc direction.
        let sweep = if matches!(edge, Edge::Bottom | Edge::Left) {
            D2D1_SWEEP_DIRECTION_CLOCKWISE
        } else {
            D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE
        };
        let arc = |p: Vector2| D2D1_ARC_SEGMENT {
            point: p,
            size: D2D_SIZE_F { width: rad, height: rad },
            rotationAngle: 0.0,
            sweepDirection: sweep,
            arcSize: D2D1_ARC_SIZE_SMALL,
        };
        self.path(|s| unsafe {
            s.BeginFigure(map(-ear, 0.0), if closed { D2D1_FIGURE_BEGIN_FILLED } else { D2D1_FIGURE_BEGIN_HOLLOW });
            s.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT { point1: map(0.0, 0.0), point2: map(0.0, ear) });
            s.AddLine(map(0.0, dep - rad));
            s.AddArc(&arc(map(rad, dep)));
            s.AddLine(map(len - rad, dep));
            s.AddArc(&arc(map(len, dep - rad)));
            s.AddLine(map(len, ear));
            s.AddQuadraticBezier(&D2D1_QUADRATIC_BEZIER_SEGMENT { point1: map(len, 0.0), point2: map(len + ear, 0.0) });
            s.EndFigure(if closed { D2D1_FIGURE_END_CLOSED } else { D2D1_FIGURE_END_OPEN });
        })
    }

    /// A bottom-edge silhouette (flares on the edge, rounded top corners)
    /// with another one standing on its top edge (`bump`: its rect, corner
    /// radius and flares; its base on `r`'s top). One outline for both, so a
    /// rim drawn along it runs from one up into the other without a break.
    /// `closed`: a fillable shape; else the outline without the edge side.
    pub fn bottom_outline(&self, r: Rect, rad: f32, ear: f32, bump: Option<(Rect, f32, f32)>, closed: bool) -> Option<ID2D1PathGeometry> {
        let rad = rad.min(r.h / 2.0).min(r.w / 2.0).max(0.0);
        let arc = |p: Vector2, rr: f32| D2D1_ARC_SEGMENT {
            point: p,
            size: D2D_SIZE_F { width: rr, height: rr },
            rotationAngle: 0.0,
            sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
            arcSize: D2D1_ARC_SIZE_SMALL,
        };
        let quad = |c: Vector2, p: Vector2| D2D1_QUADRATIC_BEZIER_SEGMENT { point1: c, point2: p };
        self.path(|s| unsafe {
            s.BeginFigure(pt(r.x - ear, r.b()), if closed { D2D1_FIGURE_BEGIN_FILLED } else { D2D1_FIGURE_BEGIN_HOLLOW });
            s.AddQuadraticBezier(&quad(pt(r.x, r.b()), pt(r.x, r.b() - ear)));
            s.AddLine(pt(r.x, r.y + rad));
            s.AddArc(&arc(pt(r.x + rad, r.y), rad));
            if let Some((b, brad, bear)) = bump {
                let brad = brad.min(b.h / 2.0).min(b.w / 2.0).max(0.0);
                let base = r.y;
                s.AddLine(pt(b.x - bear, base));
                s.AddQuadraticBezier(&quad(pt(b.x, base), pt(b.x, base - bear)));
                s.AddLine(pt(b.x, b.y + brad));
                s.AddArc(&arc(pt(b.x + brad, b.y), brad));
                s.AddLine(pt(b.r() - brad, b.y));
                s.AddArc(&arc(pt(b.r(), b.y + brad), brad));
                s.AddLine(pt(b.r(), base - bear));
                s.AddQuadraticBezier(&quad(pt(b.r(), base), pt(b.r() + bear, base)));
            }
            s.AddLine(pt(r.r() - rad, r.y));
            s.AddArc(&arc(pt(r.r(), r.y + rad), rad));
            s.AddLine(pt(r.r(), r.b() - ear));
            s.AddQuadraticBezier(&quad(pt(r.r(), r.b()), pt(r.r() + ear, r.b())));
            s.EndFigure(if closed { D2D1_FIGURE_END_CLOSED } else { D2D1_FIGURE_END_OPEN });
        })
    }

    pub fn fill_geometry(&mut self, geo: &ID2D1PathGeometry, c: Color) {
        self.set(c);
        unsafe { self.rt.FillGeometry(geo, &self.brush, None) }
    }

    /// Clip subsequent drawing to `geo` and fade it by `opacity`.
    pub fn push_layer(&mut self, geo: Option<&ID2D1PathGeometry>, opacity: f32) {
        unsafe {
            let layer = match self.rt.CreateLayer(None) {
                Ok(l) => l,
                Err(_) => return,
            };
            let params = D2D1_LAYER_PARAMETERS {
                contentBounds: D2D_RECT_F { left: -1e6, top: -1e6, right: 1e6, bottom: 1e6 },
                geometricMask: ManuallyDrop::new(geo.map(|g| g.cast::<ID2D1Geometry>().unwrap())),
                maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                maskTransform: ident(),
                opacity: opacity.clamp(0.0, 1.0),
                opacityBrush: ManuallyDrop::new(None),
                layerOptions: D2D1_LAYER_OPTIONS_NONE,
            };
            self.rt.PushLayer(&params, &layer);
            let mut p = params;
            ManuallyDrop::drop(&mut p.geometricMask);
            self.layers.push(layer);
        }
    }

    /// Draw what follows inside `r` only, fading out over `left` DIPs at its
    /// left edge and `right` at its right (0 = a straight cut). Ends with
    /// `pop_layer`.
    pub fn push_fade(&mut self, r: Rect, left: f32, right: f32) {
        unsafe {
            let Ok(layer) = self.rt.CreateLayer(None) else { return };
            let w = r.w.max(1.0);
            let (a, b) = ((left / w).clamp(0.0, 0.5), (1.0 - right / w).clamp(0.5, 1.0));
            let c = |o: f32| D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: o };
            let stops = [
                D2D1_GRADIENT_STOP { position: 0.0, color: c(if left > 0.0 { 0.0 } else { 1.0 }) },
                D2D1_GRADIENT_STOP { position: a, color: c(1.0) },
                D2D1_GRADIENT_STOP { position: b, color: c(1.0) },
                D2D1_GRADIENT_STOP { position: 1.0, color: c(if right > 0.0 { 0.0 } else { 1.0 }) },
            ];
            let brush = self.rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP).ok().and_then(|coll| {
                let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: pt(r.x, r.y), endPoint: pt(r.r(), r.y) };
                self.rt.CreateLinearGradientBrush(&props, None, &coll).ok()
            });
            let params = D2D1_LAYER_PARAMETERS {
                contentBounds: r.d2d(),
                geometricMask: ManuallyDrop::new(None),
                maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                maskTransform: ident(),
                opacity: 1.0,
                opacityBrush: ManuallyDrop::new(brush.map(|b| b.cast::<ID2D1Brush>().unwrap())),
                layerOptions: D2D1_LAYER_OPTIONS_NONE,
            };
            self.rt.PushLayer(&params, &layer);
            let mut p = params;
            ManuallyDrop::drop(&mut p.opacityBrush);
            self.layers.push(layer);
        }
    }

    pub fn pop_layer(&mut self) {
        if self.layers.pop().is_some() {
            unsafe { self.rt.PopLayer() }
        }
    }

    pub fn push_clip(&mut self, r: Rect) {
        unsafe { self.rt.PushAxisAlignedClip(&r.d2d(), D2D1_ANTIALIAS_MODE_ALIASED) }
    }

    pub fn pop_clip(&mut self) {
        unsafe { self.rt.PopAxisAlignedClip() }
    }

    // -------------------------------------------------------------- text

    fn format(&mut self, size: f32, weight: Weight, align: Align, fam: Family, wrap: bool) -> Option<IDWriteTextFormat> {
        let key = ((size * 4.0) as u32, weight, align, fam, wrap);
        if let Some(f) = self.formats.get(&key) {
            return Some(f.clone());
        }
        unsafe {
            let family = match fam {
                Family::Text => HSTRING::from("Segoe UI"),
                Family::Icon => self.icon_family.clone(),
                Family::Mono => HSTRING::from("Segoe UI"),
            };
            let w = match weight {
                Weight::Regular => DWRITE_FONT_WEIGHT_NORMAL,
                Weight::Semi => DWRITE_FONT_WEIGHT_SEMI_BOLD,
                Weight::Bold => DWRITE_FONT_WEIGHT_BOLD,
            };
            let f = self
                .dw
                .CreateTextFormat(&family, None, w, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, w!("en-us"))
                .ok()?;
            let _ = f.SetTextAlignment(match align {
                Align::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
                Align::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
                Align::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
            });
            if wrap {
                let _ = f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR);
                let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP);
            } else {
                let _ = f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
                let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
                if let Ok(sign) = self.dw.CreateEllipsisTrimmingSign(&f) {
                    let trim = DWRITE_TRIMMING {
                        granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                        delimiter: 0,
                        delimiterCount: 0,
                    };
                    let _ = f.SetTrimming(&trim, &sign);
                }
            }
            self.formats.insert(key, f.clone());
            Some(f)
        }
    }

    fn draw_str(&mut self, s: &str, r: Rect, f: &IDWriteTextFormat, c: Color) {
        self.set(c);
        let w = wide(s);
        unsafe {
            self.rt.DrawText(
                &w,
                f,
                &r.d2d(),
                &self.brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            )
        }
    }

    /// Single line, vertically centred in `r`, trimmed with an ellipsis.
    pub fn text(&mut self, s: &str, r: Rect, size: f32, c: Color, weight: Weight, align: Align) {
        if let Some(f) = self.format(size, weight, align, Family::Text, false) {
            self.draw_str(s, r, &f, c);
        }
    }

    /// Wrapped paragraph starting at the top of `r`.
    pub fn text_wrap(&mut self, s: &str, r: Rect, size: f32, c: Color, weight: Weight, align: Align) {
        if let Some(f) = self.format(size, weight, align, Family::Text, true) {
            self.draw_str(s, r, &f, c);
        }
    }

    /// Tabular-ish digits for timers (Segoe UI digits are already equal-width).
    pub fn digits(&mut self, s: &str, r: Rect, size: f32, c: Color, align: Align) {
        if let Some(f) = self.format(size, Weight::Semi, align, Family::Mono, false) {
            self.draw_str(s, r, &f, c);
        }
    }

    pub fn icon(&mut self, g: &str, r: Rect, size: f32, c: Color) {
        if let Some(f) = self.format(size, Weight::Regular, Align::Center, Family::Icon, false) {
            self.draw_str(g, r, &f, c);
        }
    }

    pub fn measure(&mut self, s: &str, size: f32, weight: Weight) -> f32 {
        self.measure_in(s, size, weight, Family::Text)
    }

    /// Width of `s` as `digits` draws it.
    pub fn measure_digits(&mut self, s: &str, size: f32) -> f32 {
        self.measure_in(s, size, Weight::Semi, Family::Mono)
    }

    fn measure_in(&mut self, s: &str, size: f32, weight: Weight, family: Family) -> f32 {
        let Some(f) = self.format(size, weight, Align::Left, family, false) else {
            return 0.0;
        };
        unsafe {
            let w = wide(s);
            let Ok(layout) = self.dw.CreateTextLayout(&w, &f, 10000.0, 100.0) else {
                return 0.0;
            };
            let mut m = DWRITE_TEXT_METRICS::default();
            if layout.GetMetrics(&mut m).is_ok() {
                m.widthIncludingTrailingWhitespace
            } else {
                0.0
            }
        }
    }

    // ------------------------------------------------------------ bitmaps

    /// Create a bitmap from premultiplied BGRA pixels.
    pub fn make_bitmap(&mut self, w: u32, h: u32, bgra: &[u8]) -> Option<ID2D1Bitmap> {
        unsafe {
            let props = D2D1_BITMAP_PROPERTIES {
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                dpiX: 96.0,
                dpiY: 96.0,
            };
            self.rt
                .CreateBitmap(D2D_SIZE_U { width: w, height: h }, Some(bgra.as_ptr() as _), w * 4, &props)
                .ok()
        }
    }

    /// Draw a bitmap stretched into a rounded rectangle.
    pub fn image_rr(&mut self, bmp: &ID2D1Bitmap, r: Rect, rad: f32) {
        unsafe {
            let sz = bmp.GetSize();
            if sz.width <= 0.0 || sz.height <= 0.0 {
                return;
            }
            let Ok(brush) = self.rt.CreateBitmapBrush(bmp, None, None) else { return };
            let sx = r.w / sz.width;
            let sy = r.h / sz.height;
            let m = Matrix3x2 { M11: sx, M12: 0.0, M21: 0.0, M22: sy, M31: r.x, M32: r.y };
            brush.SetTransform(&m);
            brush.SetOpacity(self.alpha);
            let rr = D2D1_ROUNDED_RECT { rect: r.d2d(), radiusX: rad, radiusY: rad };
            self.rt.FillRoundedRectangle(&rr, &brush);
        }
    }
}

impl Drop for Gfx {
    fn drop(&mut self) {
        unsafe {
            if !self.dib.is_invalid() {
                SelectObject(self.mem_dc, self.old_bmp);
                let _ = DeleteObject(self.dib.into());
            }
            let _ = DeleteDC(self.mem_dc);
        }
    }
}

/// Build a 32×32 tray icon procedurally: the Nookly mark.
/// Nookly's mark at `n` x `n` pixels, exactly as docs/tools/make_logo.py
/// draws it (same 32-unit grid): an amber "n" on a dark ink tile, a teal
/// light in its arch. Each pixel is sampled 4x4. BGRA, straight alpha, or
/// premultiplied for Direct2D.
pub fn logo_pixels(n: u32, premultiplied: bool) -> Vec<u8> {
    let mut px = vec![0u8; (n * n * 4) as usize];
    let k = 32.0 / n as f32;
    let tile = |x: f32, y: f32| {
        let (l, t, r, b, rad) = (1.0, 1.0, 31.0, 31.0, 8.0f32);
        let (cx, cy) = (x.clamp(l + rad, r - rad), y.clamp(t + rad, b - rad));
        x >= l && x <= r && y >= t && y <= b && (x - cx).hypot(y - cy) <= rad
    };
    let n_dist = |x: f32, y: f32| {
        let arc = if y <= 15.0 { ((x - 16.0).hypot(y - 15.0) - 6.0).abs() } else { f32::MAX };
        let leg = |lx: f32| (x - lx).hypot(y - y.clamp(15.0, 25.0));
        arc.min(leg(10.0)).min(leg(22.0))
    };
    let mix = |a: [f32; 3], b: [f32; 3], t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
    // (b, g, r)
    let (ink_top, ink_bottom) = ([62.0, 38.0, 42.0], [25.0, 16.0, 17.0]);
    let (amber_top, amber_bottom) = ([74.0, 196.0, 255.0], [56.0, 128.0, 255.0]);
    let teal = [210.0, 225.0, 92.0];
    for y in 0..n {
        for x in 0..n {
            let (mut acc, mut cov) = ([0.0f32; 3], 0.0f32);
            for sy in 0..4 {
                for sx in 0..4 {
                    let fx = (x as f32 + (sx as f32 + 0.5) / 4.0) * k;
                    let fy = (y as f32 + (sy as f32 + 0.5) / 4.0) * k;
                    if !tile(fx, fy) {
                        continue;
                    }
                    let c = if (fx - 16.0).hypot(fy - 22.4) <= 2.5 {
                        teal
                    } else if n_dist(fx, fy) <= 2.4 {
                        mix(amber_top, amber_bottom, ((fy - 9.0) / 16.0).clamp(0.0, 1.0))
                    } else {
                        let glow = (-((fx - 16.0).hypot(fy - 22.4) / 4.5).powi(2)).exp() * 0.35;
                        mix(mix(ink_top, ink_bottom, fy / 32.0), teal, glow)
                    };
                    for j in 0..3 {
                        acc[j] += c[j];
                    }
                    cov += 1.0;
                }
            }
            let i = ((y * n + x) * 4) as usize;
            let a = cov / 16.0;
            for j in 0..3 {
                let v = acc[j] / cov.max(1.0);
                px[i + j] = (if premultiplied { v * a } else { v }) as u8;
            }
            px[i + 3] = (a * 255.0) as u8;
        }
    }
    px
}

pub fn make_tray_icon() -> HICON {
    unsafe {
        let n = 32i32;
        let px = logo_pixels(32, false);
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: n,
                biHeight: -n,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let dc = GetDC(None);
        let mut bits = std::ptr::null_mut();
        let Ok(color) = CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) else {
            ReleaseDC(None, dc);
            return HICON::default();
        };
        std::ptr::copy_nonoverlapping(px.as_ptr(), bits as *mut u8, px.len());
        let mask = CreateBitmap(n, n, 1, 1, None);
        ReleaseDC(None, dc);
        let info = ICONINFO { fIcon: TRUE, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
        let icon = CreateIconIndirect(&info).unwrap_or_default();
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        icon
    }
}

/// Put new pixels into a bitmap of the same size (false if the size differs).
pub fn refill_bitmap(bmp: &ID2D1Bitmap, w: u32, h: u32, bgra: &[u8]) -> bool {
    unsafe {
        let sz = bmp.GetPixelSize();
        if (sz.width, sz.height) != (w, h) || bgra.len() < (w * h * 4) as usize {
            return false;
        }
        bmp.CopyFromMemory(None, bgra.as_ptr() as _, w * 4).is_ok()
    }
}
