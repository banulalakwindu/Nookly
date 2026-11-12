//! Mirror: a quick camera preview before a call. The camera runs only
//! while this panel is on screen (Windows shows its camera-in-use light and
//! Nookly's green dot meanwhile); frames stay in memory and are never saved.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

use crate::gfx::*;
use crate::modules::{Ctx, Module, Rel};
use crate::ui::Ui;

const A_FLIP: u32 = 1;
const C_MIRROR: Color = Color { r: 0.95, g: 0.55, b: 0.85, a: 1.0 };

/// The latest frame: (width, height, BGRA, frame number).
type Frame = Arc<Mutex<Option<(u32, u32, Vec<u8>, u64)>>>;

pub struct Mirror {
    frame: Frame,
    running: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
    seen_at: f64,
    shown: u64,
    bmp: Option<ID2D1Bitmap>,
    flip: bool,
}

impl Mirror {
    pub fn new() -> Mirror {
        Mirror { frame: Arc::new(Mutex::new(None)), running: Arc::new(AtomicBool::new(false)), error: Arc::new(Mutex::new(None)), seen_at: 0.0, shown: 0, bmp: None, flip: true }
    }
    fn start(&self) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let (frame, running, error) = (self.frame.clone(), self.running.clone(), self.error.clone());
        std::thread::spawn(move || {
            let res = unsafe { capture(&frame, &running) };
            *error.lock().unwrap() = res.err();
            running.store(false, Ordering::SeqCst);
            *frame.lock().unwrap() = None;
            crate::app::wake();
        });
    }
}

/// Read frames from the first camera until `running` goes false.
unsafe fn capture(frame: &Frame, running: &AtomicBool) -> std::result::Result<(), String> {
    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    MFStartup(MF_VERSION, MFSTARTUP_LITE).map_err(|_| "Media Foundation isn't available".to_string())?;
    let result = (|| {
        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attrs, 1).map_err(|e| e.to_string())?;
        let attrs = attrs.ok_or("no attributes")?;
        attrs.SetGUID(&MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID).map_err(|e| e.to_string())?;
        let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
        let mut count = 0u32;
        MFEnumDeviceSources(&attrs, &mut list, &mut count).map_err(|e| e.to_string())?;
        if count == 0 || list.is_null() {
            return Err("No camera found".to_string());
        }
        let devices = std::slice::from_raw_parts_mut(list, count as usize);
        let act = devices[0].take().ok_or("No camera found")?;
        for d in devices.iter_mut() {
            d.take();
        }
        windows::Win32::System::Com::CoTaskMemFree(Some(list as *const _));
        let source: IMFMediaSource = act.ActivateObject().map_err(|_| "The camera is busy or blocked (Settings > Privacy > Camera)".to_string())?;
        let mut rattrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut rattrs, 2).map_err(|e| e.to_string())?;
        let rattrs = rattrs.ok_or("no attributes")?;
        let _ = rattrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1);
        let reader = MFCreateSourceReaderFromMediaSource(&source, &rattrs).map_err(|e| e.to_string())?;
        let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        let ty = MFCreateMediaType().map_err(|e| e.to_string())?;
        ty.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(|e| e.to_string())?;
        ty.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).map_err(|e| e.to_string())?;
        reader.SetCurrentMediaType(stream, None, &ty).map_err(|_| "The camera can't give pictures Nookly can show".to_string())?;
        let cur = reader.GetCurrentMediaType(stream).map_err(|e| e.to_string())?;
        let size = cur.GetUINT64(&MF_MT_FRAME_SIZE).map_err(|e| e.to_string())?;
        let (w, h) = ((size >> 32) as u32, (size & 0xffff_ffff) as u32);
        let mut n = 0u64;
        while running.load(Ordering::SeqCst) {
            let mut flags = 0u32;
            let mut sample: Option<IMFSample> = None;
            if reader.ReadSample(stream, 0, None, Some(&mut flags), None, Some(&mut sample)).is_err() {
                break;
            }
            let Some(sample) = sample else { continue };
            let Ok(buf) = sample.ConvertToContiguousBuffer() else { continue };
            let mut p: *mut u8 = std::ptr::null_mut();
            let mut len = 0u32;
            if buf.Lock(&mut p, None, Some(&mut len)).is_ok() {
                let need = (w * h * 4) as usize;
                if (len as usize) >= need && !p.is_null() {
                    // RGB32 rows can come bottom-up; the stride sign says so.
                    let stride = cur.GetUINT32(&MF_MT_DEFAULT_STRIDE).map(|s| s as i32).unwrap_or((w * 4) as i32);
                    let src = std::slice::from_raw_parts(p, need);
                    let mut out = vec![0u8; need];
                    let row = (w * 4) as usize;
                    for y in 0..h as usize {
                        let sy = if stride < 0 { h as usize - 1 - y } else { y };
                        out[y * row..(y + 1) * row].copy_from_slice(&src[sy * row..(sy + 1) * row]);
                    }
                    for px in out.chunks_exact_mut(4) {
                        px[3] = 255;
                    }
                    n += 1;
                    *frame.lock().unwrap() = Some((w, h, out, n));
                    crate::app::wake();
                }
                let _ = buf.Unlock();
            }
        }
        let _ = source.Shutdown();
        Ok(())
    })();
    let _ = MFShutdown();
    result
}

impl Module for Mirror {
    fn title(&self) -> &'static str {
        "Mirror"
    }
    fn icon(&self) -> &'static str {
        "\u{E722}"
    }
    fn accent(&self) -> Color {
        C_MIRROR
    }
    fn size(&self) -> (f32, f32) {
        (500.0, 176.0)
    }
    fn tick(&mut self, _ctx: &mut Ctx) {
        // Off as soon as the panel isn't on screen.
        if crate::time::unix_now() - self.seen_at > 1.0 && self.running.load(Ordering::SeqCst) {
            self.running.store(false, Ordering::SeqCst);
            self.bmp = None;
        }
    }
    fn tick_off(&mut self, ctx: &mut Ctx) {
        self.seen_at = 0.0;
        self.tick(ctx);
    }
    fn animating(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
    fn draw(&mut self, ui: &mut Ui, r: Rect) {
        self.seen_at = crate::time::unix_now();
        if !crate::modules::agents::ISOLATED.load(Ordering::Relaxed) {
            self.start();
        }
        let box_r = r.centered((r.h * 16.0 / 9.0).min(r.w), r.h);
        ui.fill_rr(box_r, 14.0, col::BTN);
        let f = self.frame.lock().unwrap().clone();
        match f {
            Some((w, h, px, n)) => {
                if n != self.shown || self.bmp.is_none() {
                    let px = if self.flip {
                        let row = (w * 4) as usize;
                        let mut m = px.clone();
                        for y in 0..h as usize {
                            for x in 0..w as usize {
                                let (a, b) = (y * row + x * 4, y * row + (w as usize - 1 - x) * 4);
                                m[a..a + 4].copy_from_slice(&px[b..b + 4]);
                            }
                        }
                        m
                    } else {
                        px
                    };
                    self.bmp = ui.make_bitmap(w, h, &px);
                    self.shown = n;
                }
                if let Some(b) = &self.bmp {
                    // Fill the box, cropping the long side.
                    let ar = w as f32 / h as f32;
                    let (dw, dh) = if box_r.w / box_r.h > ar { (box_r.w, box_r.w / ar) } else { (box_r.h * ar, box_r.h) };
                    ui.push_clip(box_r);
                    ui.image_rr(b, Rect::new(box_r.cx() - dw / 2.0, box_r.cy() - dh / 2.0, dw, dh), 14.0);
                    ui.pop_clip();
                }
                let fl = Rect::new(box_r.r() - 34.0, box_r.b() - 34.0, 26.0, 26.0);
                ui.icon_btn(fl, "\u{E8AB}", A_FLIP, 11.0, col::TEXT, Some(hex(0x000000).with_a(0.5)), "Flip left and right");
            }
            None => {
                let msg = self.error.lock().unwrap().clone().unwrap_or_else(|| "Starting the camera...".into());
                ui.icon("\u{E722}", box_r.centered(30.0, 30.0).inset_xy(0.0, -14.0), 18.0, C_MIRROR);
                ui.text(&msg, Rect::new(box_r.x, box_r.cy() + 10.0, box_r.w, 18.0), 12.0, col::DIM, Weight::Regular, Align::Center);
            }
        }
    }
    fn action(&mut self, id: u32, _rel: Rel, _ctx: &mut Ctx) {
        if id == A_FLIP {
            self.flip = !self.flip;
            self.shown = 0;
        }
    }
    fn tip(&self, id: u32) -> &'static str {
        if id == A_FLIP {
            "Flip left and right"
        } else {
            ""
        }
    }
    fn debug_state(&self) -> String {
        format!("mirror running={}", self.running.load(Ordering::SeqCst))
    }
}
