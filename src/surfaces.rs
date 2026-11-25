//! Widget surfaces: the notch draws every taskbar widget (it owns the
//! features: music, weather, the clock...) and the taskbar only places and
//! shows the pictures. Both draw with the same code, so a widget can never
//! look different in the two places, and the taskbar stays small.
//!
//! A block of named shared memory holds the latest picture of each face:
//!
//! ```text
//! [u32 seq][u32 count][u32 reserved][u32 reserved]
//! count x [id: 32 bytes][u32 w px][u32 h px][f32 w DIPs][u32 offset][u32 len][u32 version]
//! pixels (premultiplied BGRA, top-down), from PIXELS on
//! ```
//!
//! The sequence is odd while the notch writes; a reader that sees it odd,
//! or changed by the end, tries again. The notch rewrites the block only
//! when a face changed and then posts `CHANGED_MSG` to the taskbar. The
//! taskbar posts `WANT_MSG` when what it needs changed (its scale, or Liquid
//! Glass turning light or dark behind it).
//!
//! Each face has a version: a face that moves (music bars) is rewritten
//! eight times a second, and only its pixels are copied, by either side.

use std::sync::atomic::{AtomicU32, Ordering};

use windows::Win32::Foundation::*;
use windows::Win32::System::Memory::*;
use windows::core::*;

const NAME: PCWSTR = w!("Local\\Nookly.Surfaces");
/// Room for about 30 faces at 200% scale (pages are only used once touched).
const SIZE: usize = 8 * 1024 * 1024;
const HEAD: usize = 16;
const ID_LEN: usize = 32;
const ENTRY: usize = ID_LEN + 24;
const MAX_FACES: usize = 64;
const PIXELS: usize = 8192;

/// Notch -> taskbar: the faces changed.
pub const CHANGED_MSG: PCWSTR = w!("Nookly.SurfacesChanged");
/// Taskbar -> notch: draw for me again. wParam: 1 when Liquid Glass is
/// light behind the bar; lParam: the bar's scale x 1000.
pub const WANT_MSG: PCWSTR = w!("Nookly.SurfaceWant");

/// Taskbar -> notch: a widget's bubble opened (wParam: its place in
/// barcfg::CATALOG + 1) or closed (0). While open the notch draws the
/// widget's full view as the face "full@<id>".
pub const OPEN_MSG: PCWSTR = w!("Nookly.SurfaceOpen");
/// Taskbar -> notch: the mouse inside an open full view. wParam: what
/// (INPUT_*), and for the wheel its delta in the high 16 bits; lParam: x
/// and y in DIPs x 4 (i16 each, low and high).
pub const INPUT_MSG: PCWSTR = w!("Nookly.SurfaceInput");
pub const INPUT_MOVE: usize = 0;
pub const INPUT_DOWN: usize = 1;
pub const INPUT_UP: usize = 2;
pub const INPUT_WHEEL: usize = 3;
pub const INPUT_LEAVE: usize = 4;

/// Taskbar -> notch: the bar is out (wParam 1) or slid away (0), and
/// which widgets are in sight (lParam: a bit per place in barcfg::placed).
/// Faces that move only move while they can be seen.
pub const SHOWN_MSG: PCWSTR = w!("Nookly.SurfaceShown");

/// Taskbar -> notch: do something with the settings (they're the notch's).
/// wParam: COMMAND_*.
pub const COMMAND_MSG: PCWSTR = w!("Nookly.BarCommand");
/// Open Settings on its Taskbar tab.
pub const COMMAND_SETTINGS: usize = 1;
/// Switch auto-hide.
pub const COMMAND_AUTOHIDE: usize = 2;

/// Notch -> taskbar: use it from the keyboard (its shortcut was pressed).
pub const KEYS_MSG: PCWSTR = w!("Nookly.BarKeys");

/// The face name of a widget's full view.
pub fn full_face(id: &str) -> String {
    format!("full@{id}")
}

/// Height of every face (DIPs): the bar's buttons are as tall.
pub const FACE_H: f32 = 40.0;

/// One widget's picture.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Face {
    pub id: String,
    /// Pixel size.
    pub w: u32,
    pub h: u32,
    /// Width in DIPs (what the bar lays out).
    pub dip_w: f32,
    pub px: Vec<u8>,
}

/// Pack faces into a block (the bytes after the sequence number), each
/// with its version. `same`: where the layout is as before, faces whose
/// version didn't change keep their pixels (not copied again).
fn pack(faces: &[Face], vers: &[u32], same: Option<&[u32]>, out: &mut [u8]) -> bool {
    let n = faces.len().min(MAX_FACES);
    let mut at = PIXELS;
    put(out, 4, n as u32);
    for (i, f) in faces.iter().take(n).enumerate() {
        let len = (f.w * f.h * 4) as usize;
        if f.px.len() < len || at + len > out.len() {
            return false;
        }
        let e = HEAD + i * ENTRY;
        let v = vers.get(i).copied().unwrap_or(0);
        let id = f.id.as_bytes();
        out[e..e + ID_LEN].fill(0);
        out[e..e + id.len().min(ID_LEN)].copy_from_slice(&id[..id.len().min(ID_LEN)]);
        put(out, e + ID_LEN, f.w);
        put(out, e + ID_LEN + 4, f.h);
        put(out, e + ID_LEN + 8, f.dip_w.to_bits());
        put(out, e + ID_LEN + 12, at as u32);
        put(out, e + ID_LEN + 16, len as u32);
        put(out, e + ID_LEN + 20, v);
        if same.is_none_or(|s| s.get(i) != Some(&v)) {
            out[at..at + len].copy_from_slice(&f.px[..len]);
        }
        at += len;
    }
    true
}

/// The faces in a block, with their versions; a face whose version is in
/// `known` comes without its pixels (the reader has them).
fn unpack(block: &[u8], known: &dyn Fn(&str, u32) -> bool) -> Vec<(Face, u32, bool)> {
    let n = (get(block, 4) as usize).min(MAX_FACES);
    let mut faces = Vec::with_capacity(n);
    for i in 0..n {
        let e = HEAD + i * ENTRY;
        let raw = &block[e..e + ID_LEN];
        let id = String::from_utf8_lossy(&raw[..raw.iter().position(|&b| b == 0).unwrap_or(ID_LEN)]).to_string();
        let (w, h) = (get(block, e + ID_LEN), get(block, e + ID_LEN + 4));
        let dip_w = f32::from_bits(get(block, e + ID_LEN + 8));
        let (at, len) = (get(block, e + ID_LEN + 12) as usize, get(block, e + ID_LEN + 16) as usize);
        let v = get(block, e + ID_LEN + 20);
        if len as u64 != w as u64 * h as u64 * 4 || at + len > block.len() {
            continue;
        }
        let have = known(&id, v);
        let px = if have { Vec::new() } else { block[at..at + len].to_vec() };
        faces.push((Face { id, w, h, dip_w, px }, v, have));
    }
    faces
}

fn put(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn get(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// The notch's side: makes the block and writes the faces into it.
pub struct Writer {
    handle: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
    /// What was written last: each face, its version; and the next version.
    last: Vec<Face>,
    vers: Vec<u32>,
    next: u32,
}

impl Writer {
    pub fn open() -> Option<Writer> {
        unsafe {
            let handle = CreateFileMappingW(INVALID_HANDLE_VALUE, None, PAGE_READWRITE, 0, SIZE as u32, NAME).ok()?;
            let view = MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, SIZE);
            if view.Value.is_null() {
                let _ = CloseHandle(handle);
                return None;
            }
            Some(Writer { handle, view, last: Vec::new(), vers: Vec::new(), next: 1 })
        }
    }

    /// Write all faces (false if they didn't fit).
    pub fn write(&mut self, faces: &[Face]) -> bool {
        unsafe {
            let base = self.view.Value as *mut u8;
            let seq = &*(base as *const AtomicU32);
            let s = seq.load(Ordering::Relaxed);
            seq.store(s | 1, Ordering::SeqCst);
            let block = std::slice::from_raw_parts_mut(base, SIZE);
            // A face as before keeps its version (and its pixels in place,
            // when nothing moved around it).
            let mut vers = Vec::with_capacity(faces.len());
            for f in faces {
                match self.last.iter().position(|x| x == f) {
                    Some(k) => vers.push(self.vers[k]),
                    None => {
                        vers.push(self.next);
                        self.next = self.next.wrapping_add(1).max(1);
                    }
                }
            }
            let laid_same = self.last.len() == faces.len() && self.last.iter().zip(faces).all(|(a, b)| a.id == b.id && a.w == b.w && a.h == b.h);
            let old = self.vers.clone();
            let ok = pack(faces, &vers, laid_same.then_some(&old[..]), block);
            if !ok {
                put(block, 4, 0);
                self.last.clear();
                self.vers.clear();
            } else {
                self.last = faces.to_vec();
                self.vers = vers;
            }
            seq.store((s | 1) + 1, Ordering::SeqCst);
            ok
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        unsafe {
            let _ = UnmapViewOfFile(self.view);
            let _ = CloseHandle(self.handle);
        }
    }
}

/// The taskbar's side: the latest faces (None until the notch drew some).
pub fn read() -> Option<Vec<Face>> {
    read_since(&|_, _| false).map(|v| v.into_iter().map(|x| x.0).collect())
}

/// The latest faces with their versions; those `known` (id, version) come
/// without pixels (true in the third place).
pub fn read_since(known: &dyn Fn(&str, u32) -> bool) -> Option<Vec<(Face, u32, bool)>> {
    unsafe {
        let handle = OpenFileMappingW(FILE_MAP_READ.0, false, NAME).ok()?;
        let view = MapViewOfFile(handle, FILE_MAP_READ, 0, 0, SIZE);
        let out = if view.Value.is_null() {
            None
        } else {
            let base = view.Value as *const u8;
            let seq = &*(base as *const AtomicU32);
            let block = std::slice::from_raw_parts(base, SIZE);
            let mut got = None;
            for _ in 0..50 {
                let a = seq.load(Ordering::SeqCst);
                if a & 1 == 1 {
                    std::thread::yield_now();
                    continue;
                }
                let faces = unpack(block, known);
                if seq.load(Ordering::SeqCst) == a {
                    got = Some(faces);
                    break;
                }
            }
            let _ = UnmapViewOfFile(view);
            got
        };
        let _ = CloseHandle(handle);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let faces = vec![
            Face { id: "clock".into(), w: 2, h: 1, dip_w: 1.5, px: vec![1, 2, 3, 4, 5, 6, 7, 8] },
            Face { id: "a-very-long-widget-id-that-goes-past-32-bytes".into(), w: 1, h: 1, dip_w: 1.0, px: vec![9, 9, 9, 9] },
        ];
        let mut block = vec![0u8; PIXELS + 64];
        assert!(pack(&faces, &[3, 4], None, &mut block));
        let back = unpack(&block, &|_, _| false);
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].0, faces[0]);
        assert_eq!(back[0].1, 3);
        assert_eq!(back[1].0.id.len(), ID_LEN);
        assert_eq!(back[1].0.px, vec![9, 9, 9, 9]);
        // Known versions come without pixels.
        let back = unpack(&block, &|id, v| id == "clock" && v == 3);
        assert!(back[0].2 && back[0].0.px.is_empty());
        assert!(!back[1].2);
    }

    #[test]
    fn too_big_is_refused() {
        let faces = vec![Face { id: "x".into(), w: 100, h: 100, dip_w: 100.0, px: vec![0; 40000] }];
        let mut block = vec![0u8; PIXELS + 100];
        assert!(!pack(&faces, &[1], None, &mut block));
    }
}
