//! A bubble: a pop-up that grows out of an edge (the bar's top, or another
//! bubble's top) like the notch grows out of the screen edge. The same
//! silhouette (rounded corners on top, flares curving out into the edge it
//! grows from), the same springs (it overshoots and wobbles a little), the
//! same squash and stretch. It lives in a window of its own so the bar's
//! window, which holds the screen strip, never changes size.
//!
//! Coordinates are the bar's DIPs (above the bar is negative y).

use nookly_core::gfx::*;
use nookly_core::store::Edge;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use crate::Spring;

/// The bubbles' window class (the bar finds the bar window by its own).
pub const CLASS: PCWSTR = w!("NooklyTaskbarPop");
/// Corner radius and flare size (like the notch's).
const RAD: f32 = 16.0;
pub const EAR: f32 = 12.0;
/// The base runs this far into the edge it grows from. None: the flares
/// must land exactly on the edge (sunk, they'd cross it at an angle and the
/// join would look cut); the base is on a whole pixel row, so it's crisp.
const SINK: f32 = 0.0;
/// Room around the shape inside the window (for the overshoot).
const ROOM: f32 = 14.0;
/// Springs (stiffness, damping ratio): width, height, centre.
const SPRING_W: (f32, f32) = (19.0, 0.55);
const SPRING_H: (f32, f32) = (16.0, 0.62);
const SPRING_X: (f32, f32) = (22.0, 0.8);

/// Where a bubble is going: its centre x and size, the edge it grows from
/// (its base y), and the width it starts from (the tile it opens over).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Goal {
    pub cx: f32,
    pub w: f32,
    pub h: f32,
    pub base: f32,
    pub from_w: f32,
}

pub struct Bubble {
    pub hwnd: HWND,
    pub gfx: Gfx,
    /// The window's top-left in bar coordinates.
    pub origin: (f32, f32),
    pub visible: bool,
    /// The mouse is over this bubble's window.
    pub inside: bool,
    /// Live previews in it: (source window, DWM thumbnail).
    pub thumbs: Vec<(HWND, isize)>,
    w: Spring,
    h: Spring,
    cx: Spring,
    /// Open toward `goal`; None while it closes back into its edge.
    pub goal: Option<Goal>,
    /// The last goal (the content keeps its layout while closing).
    pub last: Goal,
    /// Liquid Glass: what's behind the bubble, and where (bar coordinates).
    pub backdrop: Option<(ID2D1Bitmap, Rect, u32)>,
}

impl Bubble {
    pub fn new() -> Option<Bubble> {
        unsafe {
            let hinst = GetModuleHandleW(None).ok()?;
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                CLASS,
                w!("Nookly Taskbar pop-up"),
                WS_POPUP,
                0,
                0,
                10,
                10,
                None,
                None,
                Some(hinst.into()),
                None,
            )
            .ok()?;
            let gfx = Gfx::new(10, 10).ok()?;
            Some(Bubble {
                hwnd,
                gfx,
                origin: (0.0, 0.0),
                visible: false,
                inside: false,
                thumbs: Vec::new(),
                w: Spring::default(),
                h: Spring::default(),
                cx: Spring::default(),
                goal: None,
                last: Goal::default(),
                backdrop: None,
            })
        }
    }

    /// Open (or move to) `g`. From closed it grows out of its edge, from
    /// the width of the tile it opens over; already open, it reshapes.
    pub fn open(&mut self, g: Goal) {
        if self.h.pos < 1.0 || !self.visible {
            self.w = Spring { pos: g.from_w, vel: 0.0 };
            self.h = Spring { pos: 0.0, vel: 0.0 };
            self.cx = Spring { pos: g.cx, vel: 0.0 };
        }
        if self.goal.is_none_or(|o| o.w != g.w || o.h != g.h || (o.cx - g.cx).abs() > 0.5) {
            self.backdrop = None;
        }
        self.goal = Some(g);
        self.last = g;
    }

    pub fn close(&mut self) {
        self.goal = None;
    }

    /// Shown, or still closing.
    pub fn active(&self) -> bool {
        self.goal.is_some() || self.h.pos > 0.5
    }

    /// Advance the springs; true while it moves. Fully closed, it hides.
    pub fn step(&mut self, dt: f32, reduce: bool) -> bool {
        let (tw, th, tx) = match self.goal {
            Some(g) => (g.w, g.h, g.cx),
            None => (self.last.from_w, 0.0, self.cx.pos),
        };
        if reduce {
            self.w = Spring { pos: tw, vel: 0.0 };
            self.h = Spring { pos: th, vel: 0.0 };
            self.cx = Spring { pos: tx, vel: 0.0 };
        } else {
            let n = ((dt * 240.0).ceil() as usize).clamp(1, 32);
            let s = dt / n as f32;
            for _ in 0..n {
                self.w.step(tw, s, SPRING_W.0, SPRING_W.1);
                self.h.step(th, s, SPRING_H.0, SPRING_H.1);
                self.cx.step(tx, s, SPRING_X.0, SPRING_X.1);
            }
            self.h.pos = self.h.pos.max(0.0);
        }
        let dist = (self.w.pos - tw).abs() + (self.h.pos - th).abs() + (self.cx.pos - tx).abs();
        let speed = self.w.vel.abs() + self.h.vel.abs() + self.cx.vel.abs();
        let moving = !(dist < 0.3 && speed < 6.0);
        if !moving {
            self.w = Spring { pos: tw, vel: 0.0 };
            self.h = Spring { pos: th, vel: 0.0 };
            self.cx = Spring { pos: tx, vel: 0.0 };
        }
        if self.goal.is_none() && self.h.pos < 0.5 {
            self.hide();
        }
        moving
    }

    pub fn hide(&mut self) {
        self.clear_thumbs();
        self.inside = false;
        self.backdrop = None;
        if self.visible {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            self.visible = false;
        }
        self.h = Spring::default();
    }

    pub fn clear_thumbs(&mut self) {
        for (_, t) in self.thumbs.drain(..) {
            unsafe {
                let _ = windows::Win32::Graphics::Dwm::DwmUnregisterThumbnail(t);
            }
        }
    }

    /// The shape now (bar coordinates), squashed and stretched by its speed.
    pub fn shape(&self) -> Rect {
        let k = |v: f32| (v * 0.00004).clamp(-0.06, 0.06);
        let w = (self.w.pos * (1.0 - k(self.h.vel))).max(0.0);
        let h = (self.h.pos * (1.0 - k(self.w.vel))).max(0.0);
        Rect::new(self.cx.pos - w / 2.0, self.last.base - h, w, h)
    }

    /// Where the content goes (bar coordinates): its full size, standing on
    /// the edge, moving with the bubble.
    pub fn content(&self) -> Rect {
        let g = self.last;
        Rect::new(self.cx.pos - g.w / 2.0, g.base - g.h, g.w, g.h)
    }

    /// How visible the content is: it fades in as the bubble grows into
    /// its shape (and out as it closes).
    pub fn content_alpha(&self) -> f32 {
        if self.goal.is_none() {
            return 0.0;
        }
        let g = self.last;
        let grown = (self.h.pos / g.h.max(1.0)).clamp(0.0, 1.0);
        let wide = (1.0 - (self.w.pos - g.w).abs() / (g.w * 0.5).max(1.0)).clamp(0.0, 1.0);
        (grown * wide).powi(2)
    }

    /// Size the window around the shape (with room for the overshoot and
    /// the flares) and remember where it is. Returns its screen position.
    pub fn place(&mut self, scale: f32, bar_origin: (i32, i32)) -> (i32, i32) {
        let g = self.last;
        let maxw = g.w.max(self.w.pos) * 1.15 + 2.0 * (EAR + ROOM);
        let maxh = g.h * 1.15 + ROOM;
        // The base lands exactly on a pixel row (it meets the edge with a
        // crisp line: no soft gap, no overlap).
        let maxh = (maxh * scale).round() / scale;
        let origin = (self.cx.pos - maxw / 2.0, g.base - maxh);
        let pw = (maxw * scale).ceil() as i32;
        let ph = ((maxh + SINK + 1.0) * scale).ceil() as i32;
        // Only grow, or shrink by a lot: resizing every frame is wasted work.
        if pw > self.gfx.pw || ph > self.gfx.ph || pw < self.gfx.pw - 40 || ph < self.gfx.ph - 40 {
            let _ = self.gfx.resize(pw, ph);
        }
        self.origin = origin;
        (bar_origin.0 + (origin.0 * scale).round() as i32, bar_origin.1 + (origin.1 * scale).round() as i32)
    }

    /// Its silhouette for an outline drawn with the shape it stands on (bar
    /// coordinates): the rect, corner radius and flares, as drawn.
    pub fn silhouette(&self) -> (Rect, f32, f32) {
        let s = self.shape();
        let s = Rect::new(s.x, s.y, s.w, s.h + SINK);
        (s, RAD.min(s.h / 2.0).min(s.w / 2.0), EAR.min(s.h / 3.0))
    }

    /// The shape in this window's own coordinates.
    pub fn local(&self, r: Rect) -> Rect {
        Rect::new(r.x - self.origin.0, r.y - self.origin.1, r.w, r.h)
    }

    /// Fill the shape (and the glass, and its rim of light). Returns the
    /// shape's geometry, to clip the content to.
    /// `child`: a bubble standing on this one (its silhouette, bar
    /// coordinates): the rim follows the two together (this one draws its
    /// part, below its top), so the light runs up into it as one shape.
    pub fn draw_shape(&mut self, theme: &Theme, glass_light: bool, child: Option<(Rect, f32, f32)>) -> Option<windows::Win32::Graphics::Direct2D::ID2D1PathGeometry> {
        let s = self.local(self.shape());
        let s = Rect::new(s.x, s.y, s.w, s.h + SINK);
        let rad = RAD.min(s.h / 2.0).min(s.w / 2.0);
        let ear = EAR.min(s.h / 3.0);
        let geo = self.gfx.notch_geometry(s, rad, ear, Edge::Bottom)?;
        // Drawn down to its base and no further: the shape runs a little
        // past it (so its soft edge doesn't show) and is cut there, so it
        // meets the bar edge to edge (glass would show any overlap as a
        // darker line).
        let base = self.last.base - self.origin.1;
        self.gfx.push_clip(Rect::new(-1e4, -1e4, 2e4, base + 1e4));
        if theme.glass {
            if let Some((bmp, r, _)) = self.backdrop.clone() {
                let lr = self.local(r);
                self.gfx.push_layer(Some(&geo), 1.0);
                self.gfx.image_rr(&bmp, lr, 0.0);
                self.gfx.pop_layer();
            }
        }
        self.gfx.fill_geometry(&geo, col::NOTCH);
        if theme.glass {
            let a = if glass_light { 1.0 } else { 0.8 };
            let white = hex(0xFFFFFF);
            let child = child.map(|(r, cr, ce)| (self.local(r), cr, ce));
            let (both, outline) = match child {
                Some(_) => (self.gfx.bottom_outline(s, rad, ear, child, true), self.gfx.bottom_outline(s, rad, ear, child, false)),
                None => (None, self.gfx.notch_outline(s, rad, ear, Edge::Bottom)),
            };
            // (With a bubble on top, that one draws the rim above this one's top.)
            let top = if child.is_some() { s.y } else { -1e4 };
            self.gfx.push_clip(Rect::new(-1e4, top, 2e4, 2e4));
            self.gfx.push_layer(Some(both.as_ref().unwrap_or(&geo)), 1.0);
            if let Some(outline) = outline {
                for (w, k) in [(2.0, 0.32), (7.0, 0.10), (16.0, 0.05)] {
                    self.gfx.stroke_geometry_raw(&outline, white.with_a(k * a), w);
                }
            }
            self.gfx.pop_layer();
            self.gfx.pop_clip();
        }
        self.gfx.pop_clip();
        Some(geo)
    }
}
