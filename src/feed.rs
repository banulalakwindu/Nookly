//! The widget feed: the notch owns the features (battery, Wi-Fi, focus,
//! water...) and shares their current values with the taskbar add-on, so
//! the taskbar runs none of them itself. A small block of named shared
//! memory holds the latest snapshot; the notch rewrites it only when
//! something changed and posts `FEED_MSG` to the taskbar, which reads it.
//!
//! Layout: [u32 sequence][u32 length][text]. The sequence is odd while the
//! notch writes; a reader that sees it odd, or changed by the end, tries
//! again. The text is one widget per line, fields separated by tabs.

use std::sync::atomic::{AtomicU32, Ordering};

use windows::Win32::Foundation::*;
use windows::Win32::System::Memory::*;
use windows::core::*;

use crate::gfx::Color;

const NAME: PCWSTR = w!("Local\\Nookly.BarFeed");
const SIZE: usize = 128 * 1024;
const HEAD: usize = 8;
/// Notch -> taskbar: the feed changed.
pub const FEED_MSG: PCWSTR = w!("Nookly.FeedChanged");
/// Taskbar -> notch: a widget was clicked (wParam = its index in the feed).
pub const OPEN_MSG: PCWSTR = w!("Nookly.OpenWidget");

/// One widget's value, as the taskbar shows it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub color: Color,
    pub value: String,
    pub short: String,
    pub ring: Option<f32>,
    pub quiet: bool,
}

fn clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

fn pack(c: Color) -> u32 {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    (b(c.r) << 24) | (b(c.g) << 16) | (b(c.b) << 8) | b(c.a)
}

fn unpack(v: u32) -> Color {
    let f = |s: u32| ((v >> s) & 0xFF) as f32 / 255.0;
    Color { r: f(24), g: f(16), b: f(8), a: f(0) }
}

pub fn encode(items: &[Item]) -> String {
    items
        .iter()
        .map(|i| {
            format!(
                "{}\t{}\t{}\t{:08x}\t{}\t{}\t{}\t{}\n",
                clean(&i.id),
                clean(&i.name),
                clean(&i.icon),
                pack(i.color),
                clean(&i.value),
                clean(&i.short),
                i.ring.map_or(String::from("-"), |r| format!("{r:.3}")),
                if i.quiet { 1 } else { 0 }
            )
        })
        .collect()
}

pub fn decode(s: &str) -> Vec<Item> {
    s.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            if f.len() < 8 {
                return None;
            }
            Some(Item {
                id: f[0].to_string(),
                name: f[1].to_string(),
                icon: f[2].to_string(),
                color: unpack(u32::from_str_radix(f[3], 16).unwrap_or(0xFFFFFFFF)),
                value: f[4].to_string(),
                short: f[5].to_string(),
                ring: f[6].parse().ok(),
                quiet: f[7] == "1",
            })
        })
        .collect()
}

/// The notch's side: makes the block and writes snapshots into it.
pub struct Writer {
    handle: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
    last: String,
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
            Some(Writer { handle, view, last: String::new() })
        }
    }

    /// Write the snapshot if it changed. True when it did.
    pub fn write(&mut self, text: &str) -> bool {
        if text == self.last {
            return false;
        }
        let bytes = text.as_bytes();
        let n = bytes.len().min(SIZE - HEAD);
        unsafe {
            let base = self.view.Value as *mut u8;
            let seq = &*(base as *const AtomicU32);
            let s = seq.load(Ordering::Relaxed);
            seq.store(s | 1, Ordering::SeqCst);
            std::ptr::write_volatile(base.add(4) as *mut u32, n as u32);
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), base.add(HEAD), n);
            seq.store((s | 1) + 1, Ordering::SeqCst);
        }
        self.last = text.to_string();
        true
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

/// The taskbar's side: the latest snapshot (None until the notch made one).
pub fn read() -> Option<Vec<Item>> {
    unsafe {
        let handle = OpenFileMappingW(FILE_MAP_READ.0, false, NAME).ok()?;
        let view = MapViewOfFile(handle, FILE_MAP_READ, 0, 0, SIZE);
        let out = if view.Value.is_null() {
            None
        } else {
            let base = view.Value as *const u8;
            let seq = &*(base as *const AtomicU32);
            let mut got = None;
            for _ in 0..50 {
                let a = seq.load(Ordering::SeqCst);
                if a & 1 == 1 {
                    std::thread::yield_now();
                    continue;
                }
                let n = (std::ptr::read_volatile(base.add(4) as *const u32) as usize).min(SIZE - HEAD);
                let bytes = std::slice::from_raw_parts(base.add(HEAD), n).to_vec();
                if seq.load(Ordering::SeqCst) == a {
                    got = Some(String::from_utf8_lossy(&bytes).to_string());
                    break;
                }
            }
            let _ = UnmapViewOfFile(view);
            got
        };
        let _ = CloseHandle(handle);
        out.map(|t| decode(&t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let items = vec![
            Item { id: "battery".into(), name: "Battery".into(), icon: "\u{E83F}".into(), color: Color { r: 1.0, g: 0.5, b: 0.0, a: 1.0 }, value: "82%".into(), short: "82".into(), ring: Some(0.82), quiet: false },
            Item { id: "x".into(), name: "Tab\there".into(), icon: "i".into(), color: Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }, value: "a\nb".into(), short: "".into(), ring: None, quiet: true },
        ];
        let back = decode(&encode(&items));
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].value, "82%");
        assert_eq!(back[0].ring, Some(0.82));
        assert!((back[0].color.g - 0.5).abs() < 0.01);
        assert_eq!(back[1].name, "Tab here");
        assert_eq!(back[1].value, "a b");
        assert!(back[1].quiet && back[1].ring.is_none());
    }
}
