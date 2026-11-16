//! Quick toggles: Wi-Fi, Bluetooth, dark mode and the microphone, plus
//! shortcuts to Night light, the mobile hotspot and Focus (which Windows
//! only lets you switch in its own Settings). States are read only while
//! the panel is open.

use std::sync::{Arc, Mutex};

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Registry::*;
use windows::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE};

use crate::gfx::*;
use crate::modules::{Ctx, Module, Rel};
use crate::ui::Ui;

const A_WIFI: u32 = 1;
const A_BT: u32 = 2;
const A_DARK: u32 = 3;
const A_MIC: u32 = 4;
const A_NIGHT: u32 = 5;
const A_HOTSPOT: u32 = 6;
const A_FOCUS: u32 = 7;
const A_DISPLAY: u32 = 8;

const C_TOG: Color = Color { r: 0.40, g: 0.70, b: 1.0, a: 1.0 };

#[derive(Clone, Copy, Default, Debug)]
struct State {
    wifi: Option<bool>,
    bt: Option<bool>,
}

pub struct Toggles {
    radios: Arc<Mutex<State>>,
    busy: Arc<Mutex<bool>>,
    read_at: f64,
    seen_at: f64,
    dark: bool,
    mic_muted: Option<bool>,
}

const PERSONALIZE: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");

fn reg_dword(key: PCWSTR, name: PCWSTR) -> Option<u32> {
    let mut v = 0u32;
    let mut n = 4u32;
    unsafe { RegGetValueW(HKEY_CURRENT_USER, key, name, RRF_RT_REG_DWORD, None, Some(&mut v as *mut u32 as *mut _), Some(&mut n)).is_ok().then_some(v) }
}

fn set_reg_dword(key: PCWSTR, name: PCWSTR, v: u32) {
    unsafe {
        let _ = RegSetKeyValueW(HKEY_CURRENT_USER, key, name, REG_DWORD.0, Some(&v as *const u32 as *const _), 4);
    }
}

pub fn dark_mode() -> bool {
    reg_dword(PERSONALIZE, w!("AppsUseLightTheme")).map_or(false, |v| v == 0)
}

/// Apps and Windows itself (taskbar, Start) light or dark.
pub fn set_dark_mode(dark: bool) {
    let v = if dark { 0 } else { 1 };
    set_reg_dword(PERSONALIZE, w!("AppsUseLightTheme"), v);
    set_reg_dword(PERSONALIZE, w!("SystemUsesLightTheme"), v);
    let s = HSTRING::from("ImmersiveColorSet");
    unsafe {
        let _ = SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, WPARAM(0), LPARAM(s.as_ptr() as isize), SMTO_ABORTIFHUNG, 200, None);
    }
}

fn mic() -> Option<windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume> {
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
    unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        en.GetDefaultAudioEndpoint(eCapture, eConsole).ok()?.Activate(CLSCTX_ALL, None).ok()
    }
}

pub fn mic_muted() -> Option<bool> {
    unsafe { mic()?.GetMute().ok().map(|b| b.as_bool()) }
}

/// Mute or unmute the default microphone (every app hears the change).
pub fn set_mic_muted(m: bool) -> bool {
    unsafe { mic().map_or(false, |e| e.SetMute(m, std::ptr::null()).is_ok()) }
}

/// Read (or set, with `want`) the Wi-Fi and Bluetooth radios, off the UI
/// thread: the WinRT calls can take a moment.
fn radios(state: Arc<Mutex<State>>, busy: Arc<Mutex<bool>>, want: Option<(windows::Devices::Radios::RadioKind, bool)>) {
    {
        let mut b = busy.lock().unwrap();
        if *b {
            return;
        }
        *b = true;
    }
    std::thread::spawn(move || {
        use windows::Devices::Radios::*;
        let _ = unsafe { windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED) };
        let mut st = State::default();
        if let Ok(list) = Radio::GetRadiosAsync().and_then(|op| op.join()) {
            for r in list {
                let Ok(kind) = r.Kind() else { continue };
                if let Some((k, on)) = want {
                    if kind == k {
                        let _ = r.SetStateAsync(if on { RadioState::On } else { RadioState::Off }).and_then(|op| op.join());
                    }
                }
                let on = r.State().map_or(false, |s| s == RadioState::On);
                if kind == RadioKind::WiFi {
                    st.wifi = Some(on);
                } else if kind == RadioKind::Bluetooth {
                    st.bt = Some(on);
                }
            }
        }
        *state.lock().unwrap() = st;
        *busy.lock().unwrap() = false;
        crate::app::wake();
    });
}

impl Toggles {
    pub fn new() -> Toggles {
        Toggles { radios: Arc::new(Mutex::new(State::default())), busy: Arc::new(Mutex::new(false)), read_at: 0.0, seen_at: 0.0, dark: dark_mode(), mic_muted: mic_muted() }
    }
}

impl Module for Toggles {
    fn title(&self) -> &'static str {
        "Toggles"
    }
    fn icon(&self) -> &'static str {
        "\u{E9E9}"
    }
    fn accent(&self) -> Color {
        C_TOG
    }
    fn size(&self) -> (f32, f32) {
        (500.0, 160.0)
    }
    fn tick(&mut self, _ctx: &mut Ctx) {
        // Only while the panel is being looked at.
        let now = crate::time::unix_now();
        if now - self.seen_at < 2.0 && now - self.read_at > 3.0 {
            self.read_at = now;
            self.dark = dark_mode();
            self.mic_muted = mic_muted();
            radios(self.radios.clone(), self.busy.clone(), None);
        }
    }
    fn draw(&mut self, ui: &mut Ui, r: Rect) {
        self.seen_at = crate::time::unix_now();
        let st = *self.radios.lock().unwrap();
        let busy = *self.busy.lock().unwrap();
        let rows = r.rows(2, 10.0);
        let big: [(u32, &str, &str, Option<bool>); 4] = [
            (A_WIFI, "\u{E701}", "Wi-Fi", st.wifi),
            (A_BT, "\u{E702}", "Bluetooth", st.bt),
            (A_DARK, "\u{E708}", "Dark mode", Some(self.dark)),
            (A_MIC, "\u{E720}", "Microphone", self.mic_muted.map(|m| !m)),
        ];
        for (cell, (id, icon, name, on)) in rows[0].columns(4, 8.0).into_iter().zip(big) {
            let active = on == Some(true);
            let hov = ui.hovered(cell);
            ui.fill_rr(cell, 14.0, if active { C_TOG.with_a(if hov { 0.36 } else { 0.28 }) } else if hov { col::BTN_HOVER } else { col::BTN });
            let mut c = cell.inset(10.0);
            let ic = c.cut_top(26.0, 4.0);
            ui.circle(ic.x + 13.0, ic.cy(), 13.0, if active { C_TOG } else { col::TEXT.with_a(0.12) });
            ui.icon(icon, Rect::new(ic.x, ic.y, 26.0, 26.0), 12.0, if active { hex(0x010101) } else { col::TEXT });
            ui.text(name, c.cut_top(16.0, 0.0), 12.0, col::TEXT, Weight::Semi, Align::Left);
            let state = match on {
                None if busy || id <= A_BT => "...",
                None => "Not found",
                Some(true) if id == A_MIC => "On",
                Some(false) if id == A_MIC => "Muted",
                Some(true) => "On",
                Some(false) => "Off",
            };
            ui.text(state, c, 11.0, col::DIM, Weight::Regular, Align::Left);
            ui.hit(cell, id);
        }
        let links: [(u32, &str, &str); 4] = [(A_NIGHT, "\u{E793}", "Night light"), (A_HOTSPOT, "\u{E704}", "Hotspot"), (A_FOCUS, "\u{E7ED}", "Focus"), (A_DISPLAY, "\u{E7F4}", "Display")];
        for (cell, (id, icon, name)) in rows[1].centered(rows[1].w, 30.0).columns(4, 8.0).into_iter().zip(links) {
            let hov = ui.hovered(cell);
            ui.fill_rr(cell, 15.0, if hov { col::BTN_HOVER } else { col::BTN });
            let mut c = cell.inset_xy(12.0, 0.0);
            ui.icon(icon, c.cut_left(14.0, 8.0), 11.0, col::DIM);
            ui.text(name, c, 11.5, col::TEXT, Weight::Regular, Align::Left);
            ui.hit(cell, id);
        }
    }
    fn action(&mut self, id: u32, _rel: Rel, _ctx: &mut Ctx) {
        use windows::Devices::Radios::RadioKind;
        let st = *self.radios.lock().unwrap();
        match id {
            A_WIFI => {
                if let Some(on) = st.wifi {
                    self.radios.lock().unwrap().wifi = Some(!on);
                    radios(self.radios.clone(), self.busy.clone(), Some((RadioKind::WiFi, !on)));
                } else {
                    crate::app::open_later("ms-settings:network-wifi");
                }
            }
            A_BT => {
                if let Some(on) = st.bt {
                    self.radios.lock().unwrap().bt = Some(!on);
                    radios(self.radios.clone(), self.busy.clone(), Some((RadioKind::Bluetooth, !on)));
                } else {
                    crate::app::open_later("ms-settings:bluetooth");
                }
            }
            A_DARK => {
                self.dark = !self.dark;
                set_dark_mode(self.dark);
            }
            A_MIC => {
                if let Some(m) = self.mic_muted {
                    if set_mic_muted(!m) {
                        self.mic_muted = Some(!m);
                    }
                }
            }
            A_NIGHT => crate::app::open_later("ms-settings:nightlight"),
            A_HOTSPOT => crate::app::open_later("ms-settings:network-mobilehotspot"),
            A_FOCUS => crate::app::open_later("ms-settings:quiethours"),
            A_DISPLAY => crate::app::open_later("ms-settings:display"),
            _ => {}
        }
    }
    fn tip(&self, id: u32) -> &'static str {
        match id {
            A_WIFI => "Turn Wi-Fi on or off",
            A_BT => "Turn Bluetooth on or off",
            A_DARK => "Switch Windows and apps between dark and light",
            A_MIC => "Mute or unmute your microphone for every app",
            A_NIGHT => "Night light settings (Windows only lets you switch it there)",
            A_HOTSPOT => "Mobile hotspot settings",
            A_FOCUS => "Focus and Do Not Disturb settings",
            A_DISPLAY => "Display settings (brightness, scale)",
            _ => "",
        }
    }
    fn debug_state(&self) -> String {
        format!("toggles dark={} mic_muted={:?}", self.dark, self.mic_muted)
    }
}
