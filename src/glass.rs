//! The Liquid Glass theme's backdrop: what is behind the notch, captured
//! small and blurred. The notch itself is left out of captures while that
//! theme is on (`SetWindowDisplayAffinity`), so it can see through itself;
//! the same flag keeps it out of screenshots and screen sharing then.
//!
//! Cheap by design: one GDI capture, scaled down 4x by the GPU-less
//! HALFTONE stretch, then a three-pass box blur on the small image; at most
//! ten times a second while the notch animates, about once a second at rest.

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE};

/// Captured pixels: (width, height, premultiplied BGRA, average luminance).
pub type Backdrop = (u32, u32, Vec<u8>, f32);

/// Shrink factor of the capture (and so of the blur's cost).
const DOWN: i32 = 4;

/// Keep the notch out of screen captures (Liquid Glass on) or not.
/// Grab what's behind the notch with the notch itself left out, only for
/// that moment: screen recorders refuse to start while any window stays
/// excluded from capture, so it never does for longer than one grab.
pub fn capture_behind(hwnd: HWND, r: RECT) -> Option<Backdrop> {
    exclude_from_capture(hwnd, true);
    // Let the desktop compose once without the notch.
    unsafe {
        let _ = windows::Win32::Graphics::Dwm::DwmFlush();
    }
    let b = capture(r);
    exclude_from_capture(hwnd, false);
    b
}

pub fn exclude_from_capture(hwnd: HWND, on: bool) {
    unsafe {
        let _ = SetWindowDisplayAffinity(hwnd, if on { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE });
    }
}

/// A fingerprint of a few small screen rectangles (pixels), coarse enough
/// that noise in the low bits doesn't count. Cheap: a few hundred pixels.
pub fn probe(rects: &[RECT]) -> Option<u64> {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return None;
        }
        let mem = CreateCompatibleDC(Some(screen));
        for r in rects {
            let (w, h) = (r.right - r.left, r.bottom - r.top);
            if w < 1 || h < 1 {
                continue;
            }
            let (sw, sh) = ((w / 2).max(1), (h / 2).max(1));
            let bi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: sw,
                    biHeight: -sh,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let Ok(dib) = CreateDIBSection(Some(mem), &bi, DIB_RGB_COLORS, &mut bits, None, 0) else { continue };
            let old = SelectObject(mem, dib.into());
            SetStretchBltMode(mem, HALFTONE);
            if StretchBlt(mem, 0, 0, sw, sh, Some(screen), r.left, r.top, w, h, SRCCOPY).as_bool() && !bits.is_null() {
                let px = std::slice::from_raw_parts(bits as *const u8, (sw * sh * 4) as usize);
                for p in px.chunks_exact(4) {
                    for c in &p[..3] {
                        hash = (hash ^ (*c >> 4) as u64).wrapping_mul(0x100_0000_01b3);
                    }
                }
            }
            SelectObject(mem, old);
            let _ = DeleteObject(dib.into());
        }
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
    }
    Some(hash)
}

/// Capture the screen rectangle `r` (pixels), scaled down and blurred.
pub fn capture(r: RECT) -> Option<Backdrop> {
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    if w < 8 || h < 8 {
        return None;
    }
    let (sw, sh) = ((w / DOWN).max(2), (h / DOWN).max(2));
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return None;
        }
        let mem = CreateCompatibleDC(Some(screen));
        let bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: sw,
                biHeight: -sh,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let Ok(dib) = CreateDIBSection(Some(mem), &bi, DIB_RGB_COLORS, &mut bits, None, 0) else {
            let _ = DeleteDC(mem);
            ReleaseDC(None, screen);
            return None;
        };
        let old = SelectObject(mem, dib.into());
        SetStretchBltMode(mem, HALFTONE);
        let ok = StretchBlt(mem, 0, 0, sw, sh, Some(screen), r.left, r.top, w, h, SRCCOPY | CAPTUREBLT).as_bool();
        let mut px = vec![0u8; (sw * sh * 4) as usize];
        if ok && !bits.is_null() {
            std::ptr::copy_nonoverlapping(bits as *const u8, px.as_mut_ptr(), px.len());
        }
        SelectObject(mem, old);
        let _ = DeleteObject(dib.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        if !ok {
            return None;
        }
        Some(finish(sw as u32, sh as u32, px))
    }
}

/// Opaque, blurred and a little more colourful (glass makes what's
/// behind it look vivid), with its average luminance.
pub fn finish(w: u32, h: u32, mut px: Vec<u8>) -> Backdrop {
    for p in px.chunks_exact_mut(4) {
        p[3] = 255;
    }
    for _ in 0..3 {
        box_blur(&mut px, w as usize, h as usize, 2);
    }
    for p in px.chunks_exact_mut(4) {
        let l = 0.0722 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.2126 * p[2] as f32;
        for c in &mut p[..3] {
            *c = (l + (*c as f32 - l) * 1.35).clamp(0.0, 255.0) as u8;
        }
    }
    let n = (w * h) as f32;
    let lum = px.chunks_exact(4).map(|p| 0.0722 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.2126 * p[2] as f32).sum::<f32>() / n / 255.0;
    (w, h, px, lum)
}

/// Keep what's written on the glass readable: behind dark glass (white
/// text) the brightest patches are toned down, behind light glass the
/// darkest are lifted. The rest, most of the picture, is as it was.
pub fn tame(px: &mut [u8], light: bool) {
    // Past the knee it rises less and less, with no corner in the curve (one
    // shows as a contour in a soft picture).
    const KNEE: f32 = 0.42 * 255.0;
    const SOFT: f32 = 0.18 * 255.0;
    let over = |x: f32| SOFT * (1.0 - (-x / SOFT).exp());
    for p in px.chunks_exact_mut(4) {
        let l = 0.0722 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.2126 * p[2] as f32;
        if !light && l > KNEE {
            let k = (KNEE + over(l - KNEE)) / l;
            // Toward a cool smoke: white made grey looks dull, even olive.
            let w = 1.0 - k;
            for (c, tint) in p.iter_mut().zip([0.30, -0.10, -0.22]) {
                *c = (*c as f32 * k * (1.0 + tint * w)).min(255.0) as u8;
            }
        } else if light && l < 255.0 - KNEE {
            // Toward white, as far as it takes.
            let want = 255.0 - KNEE - over(255.0 - KNEE - l);
            let k = (want - l) / (255.0 - l);
            for c in &mut p[..3] {
                *c = (*c as f32 + (255.0 - *c as f32) * k) as u8;
            }
        }
    }
}

/// One horizontal and one vertical box blur pass of radius `r` (the alpha
/// is left as it is; past the edges, the edge pixels go on).
fn box_blur(px: &mut [u8], w: usize, h: usize, r: usize) {
    if w == 0 || h == 0 {
        return;
    }
    let k = (2 * r + 1) as u32;
    // The sums divided by k, without dividing (exact for sums this small).
    let m = u32::MAX as u64 / k as u64 + 1;
    let div = |s: u32| ((s as u64 * m) >> 32) as u8;
    let stride = w * 4;
    let mut tmp = px.to_vec();
    for y in 0..h {
        let row = &px[y * stride..(y + 1) * stride];
        let out = &mut tmp[y * stride..(y + 1) * stride];
        let mut sum = [0u32; 3];
        for d in 0..=2 * r {
            let x = d.saturating_sub(r).min(w - 1) * 4;
            for c in 0..3 {
                sum[c] += row[x + c] as u32;
            }
        }
        for x in 0..w {
            let (add, sub) = ((x + r + 1).min(w - 1) * 4, x.saturating_sub(r) * 4);
            for c in 0..3 {
                out[x * 4 + c] = div(sum[c]);
                sum[c] = sum[c] + row[add + c] as u32 - row[sub + c] as u32;
            }
        }
    }
    // Down the columns, a whole row of sums at a time.
    let line = |y: usize| &tmp[y * stride..(y + 1) * stride];
    let mut sums = vec![0u32; stride];
    for d in 0..=2 * r {
        for (s, v) in sums.iter_mut().zip(line(d.saturating_sub(r).min(h - 1))) {
            *s += *v as u32;
        }
    }
    for y in 0..h {
        for (o, s) in px[y * stride..(y + 1) * stride].chunks_exact_mut(4).zip(sums.chunks_exact(4)) {
            for c in 0..3 {
                o[c] = div(s[c]);
            }
        }
        let (add, sub) = (line((y + r + 1).min(h - 1)), line(y.saturating_sub(r)));
        for ((s, a), b) in sums.iter_mut().zip(add).zip(sub) {
            *s = *s + *a as u32 - *b as u32;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn tames() {
        // Mid-tones stay; white behind dark glass and black behind light don't.
        let mut px = vec![80, 80, 80, 255, 255, 255, 255, 255, 0, 0, 0, 255];
        super::tame(&mut px, false);
        assert_eq!(px[0], 80);
        assert!(px[5] < 165 && px[5] > 125, "{}", px[5]);
        assert_eq!(px[8], 0);
        let mut px = vec![200, 200, 200, 255, 255, 255, 255, 255, 0, 0, 0, 255];
        super::tame(&mut px, true);
        assert_eq!((px[0], px[4]), (200, 255));
        assert!(px[8] > 85 && px[8] < 125, "{}", px[8]);
        assert_eq!(px[11], 255);
    }

    /// The blur against a plain one, pixel by pixel.
    #[test]
    fn blurs() {
        for (w, h, r) in [(7usize, 5usize, 2usize), (3, 2, 6), (1, 1, 2), (40, 23, 3)] {
            let src: Vec<u8> = (0..w * h * 4).map(|i| (i * 97 % 251) as u8).collect();
            let k = (2 * r + 1) as u32;
            let at = |p: &[u8], x: isize, y: isize, c: usize| p[(y.clamp(0, h as isize - 1) as usize * w + x.clamp(0, w as isize - 1) as usize) * 4 + c] as u32;
            let (mut a, mut b) = (src.clone(), src.clone());
            let ri = r as isize;
            for y in 0..h as isize {
                for x in 0..w as isize {
                    for c in 0..3 {
                        a[(y as usize * w + x as usize) * 4 + c] = ((-ri..=ri).map(|d| at(&src, x + d, y, c)).sum::<u32>() / k) as u8;
                    }
                }
            }
            for y in 0..h as isize {
                for x in 0..w as isize {
                    for c in 0..3 {
                        b[(y as usize * w + x as usize) * 4 + c] = ((-ri..=ri).map(|d| at(&a, x, y + d, c)).sum::<u32>() / k) as u8;
                    }
                }
            }
            let mut px = src.clone();
            super::box_blur(&mut px, w, h, r);
            assert_eq!(px, b, "{w}x{h} r{r}");
        }
    }
}

/// Docs renders: a scene image as the backdrop (raw file: width and height
/// as little-endian u32, then BGRA pixels, the size of the whole canvas),
/// shrunk to `w` x `h` and blurred like a screen grab.
pub fn from_raw_file(path: &str, w: u32, h: u32) -> Option<Backdrop> {
    let b = std::fs::read(path).ok()?;
    let sw = u32::from_le_bytes(b.get(0..4)?.try_into().ok()?);
    let sh = u32::from_le_bytes(b.get(4..8)?.try_into().ok()?);
    let src = b.get(8..8 + (sw * sh * 4) as usize)?;
    let mut px = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            // The average of the source pixels this one covers.
            let (x0, x1) = (x * sw / w, ((x + 1) * sw / w).max(x * sw / w + 1));
            let (y0, y1) = (y * sh / h, ((y + 1) * sh / h).max(y * sh / h + 1));
            let mut acc = [0u32; 3];
            let mut n = 0;
            for sy in y0..y1.min(sh) {
                for sx in x0..x1.min(sw) {
                    let i = ((sy * sw + sx) * 4) as usize;
                    for c in 0..3 {
                        acc[c] += src[i + c] as u32;
                    }
                    n += 1;
                }
            }
            let o = ((y * w + x) * 4) as usize;
            for c in 0..3 {
                px[o + c] = (acc[c] / n.max(1)) as u8;
            }
        }
    }
    Some(finish(w, h, px))
}

/// A made-up backdrop for renders and tests: "light", "dark" or a colourful
/// one (anything else), like wallpapers and windows behind the notch.
pub fn sample(kind: &str, w: u32, h: u32) -> Backdrop {
    let mut px = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
            // Soft blobs, like a photo or a busy page behind the notch.
            let blob = |cx: f32, cy: f32, s: f32| (-((fx - cx).powi(2) + (fy - cy).powi(2)) / s).exp();
            let (r, g, b) = match kind {
                "light" => {
                    let v = 0.93 - 0.08 * blob(0.3, 0.4, 0.03);
                    (v + 0.05 * blob(0.7, 0.6, 0.02), v, v - 0.06 * blob(0.7, 0.6, 0.02))
                }
                "dark" => {
                    let v = 0.10 + 0.08 * blob(0.6, 0.3, 0.05);
                    (v, v + 0.02, v + 0.05)
                }
                _ => (
                    0.35 + 0.6 * blob(0.15, 0.3, 0.06) + 0.3 * blob(0.85, 0.7, 0.05),
                    0.25 + 0.5 * blob(0.55, 0.8, 0.05),
                    0.55 + 0.4 * blob(0.4, 0.2, 0.04) - 0.3 * blob(0.85, 0.7, 0.05),
                ),
            };
            let i = ((y * w + x) * 4) as usize;
            px[i] = (b.clamp(0.0, 1.0) * 255.0) as u8;
            px[i + 1] = (g.clamp(0.0, 1.0) * 255.0) as u8;
            px[i + 2] = (r.clamp(0.0, 1.0) * 255.0) as u8;
        }
    }
    finish(w, h, px)
}
