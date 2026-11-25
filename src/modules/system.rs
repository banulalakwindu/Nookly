//! Background watcher (no tab): pops toasts for battery/charger, network,
//! Caps Lock, keyboard-language switches, volume and Bluetooth devices.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use windows::core::*;
use windows::Devices::Bluetooth::{BluetoothConnectionStatus, BluetoothDevice};
use windows::Devices::Enumeration::{DeviceInformation, DeviceInformationUpdate, DeviceWatcher};
use windows::Foundation::TypedEventHandler;
use windows::Win32::Foundation::*;
use windows::Win32::Globalization::*;
use windows::Win32::Media::Audio::Endpoints::*;
use windows::Win32::Media::Audio::*;
use windows::Win32::Networking::NetworkListManager::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Power::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Input::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::app::WM_APP_WAKE;
use crate::gfx::*;
use crate::modules::{Ctx, Module, Toast};
use crate::time::unix_now;


fn wake(h: isize) {
    unsafe {
        let _ = PostMessageW(Some(HWND(h as *mut _)), WM_APP_WAKE, WPARAM(0), LPARAM(0));
    }
}

/// Hear when Caps Lock or a language-switch chord is let go, so the toast
/// appears at once instead of on the next 1 s tick. Windows only tells
/// (afterwards): nothing typed ever waits for Nookly, as it would with a
/// keyboard hook.
fn hear_keys(hwnd: HWND) {
    let dev = RAWINPUTDEVICE { usUsagePage: 1, usUsage: 6, dwFlags: RIDEV_INPUTSINK, hwndTarget: hwnd };
    unsafe {
        let _ = RegisterRawInputDevices(&[dev], std::mem::size_of::<RAWINPUTDEVICE>() as u32);
    }
}

/// WM_INPUT: was it one of those keys? (Which other keys are pressed isn't
/// looked at or kept.)
pub fn is_switch_key(lp: LPARAM) -> bool {
    let mut raw = RAWINPUT::default();
    let mut n = std::mem::size_of::<RAWINPUT>() as u32;
    let k = unsafe {
        let got = GetRawInputData(HRAWINPUT(lp.0 as *mut _), RID_INPUT, Some(&mut raw as *mut _ as *mut _), &mut n, std::mem::size_of::<RAWINPUTHEADER>() as u32);
        if got == u32::MAX || got == 0 || raw.header.dwType != RIM_TYPEKEYBOARD.0 {
            return false;
        }
        raw.data.keyboard
    };
    if k.Flags as u32 & RI_KEY_BREAK == 0 {
        return false;
    }
    let down = |vk: VIRTUAL_KEY| unsafe { GetAsyncKeyState(vk.0 as i32) } < 0;
    match VIRTUAL_KEY(k.VKey) {
        VK_CAPITAL => true,
        // Win+Space; Alt+Shift, Ctrl+Shift.
        VK_SPACE => down(VK_LWIN) || down(VK_RWIN),
        VK_SHIFT | VK_LSHIFT | VK_RSHIFT => down(VK_MENU) || down(VK_CONTROL),
        VK_MENU | VK_LMENU | VK_RMENU | VK_CONTROL | VK_LCONTROL | VK_RCONTROL => down(VK_SHIFT),
        _ => false,
    }
}

#[implement(IAudioEndpointVolumeCallback)]
struct VolCb {
    shared: Arc<Mutex<Option<(f32, bool)>>>,
    hwnd: isize,
}

impl IAudioEndpointVolumeCallback_Impl for VolCb_Impl {
    fn OnNotify(&self, p: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> Result<()> {
        if !p.is_null() {
            let d = unsafe { &*p };
            *self.shared.lock().unwrap() = Some((d.fMasterVolume, d.bMuted.as_bool()));
            wake(self.hwnd);
        }
        Ok(())
    }
}

struct Volume {
    _ep: IAudioEndpointVolume,
    _cb: IAudioEndpointVolumeCallback,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Net {
    Internet,
    Local,
    Offline,
}

#[derive(Default)]
struct BtShared {
    events: Vec<(String, bool)>,
}

pub struct System {
    started: f64,
    hwnd: isize,
    // battery
    ac: Option<bool>,
    low_warned: bool,
    crit_warned: bool,
    full_warned: bool,
    // network
    nlm: Option<INetworkListManager>,
    net: Option<Net>,
    net_pending: Option<(Net, f64)>,
    // keys
    caps: Option<bool>,
    hkl: Option<isize>,
    // volume
    _volume: Option<Volume>,
    vol_shared: Arc<Mutex<Option<(f32, bool)>>>,
    // bluetooth
    bt: Arc<Mutex<BtShared>>,
}

impl System {
    pub fn new(hwnd: HWND) -> System {
        let h = hwnd.0 as isize;
        if !crate::modules::agents::ISOLATED.load(Ordering::Relaxed) {
            hear_keys(hwnd);
        }
        let nlm: Option<INetworkListManager> = unsafe { CoCreateInstance(&NetworkListManager, None, CLSCTX_ALL).ok() };
        let vol_shared = Arc::new(Mutex::new(None));
        let volume = Self::watch_volume(vol_shared.clone(), h);
        let bt = Arc::new(Mutex::new(BtShared::default()));
        spawn_bt_watcher(bt.clone(), h);
        System {
            started: unix_now(),
            hwnd: h,
            ac: None,
            low_warned: false,
            crit_warned: false,
            full_warned: false,
            nlm,
            net: None,
            net_pending: None,
            caps: None,
            hkl: None,
            _volume: volume,
            vol_shared,
            bt,
        }
    }

    fn watch_volume(shared: Arc<Mutex<Option<(f32, bool)>>>, hwnd: isize) -> Option<Volume> {
        unsafe {
            let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
            let dev = en.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
            let ep: IAudioEndpointVolume = dev.Activate(CLSCTX_ALL, None).ok()?;
            let cb: IAudioEndpointVolumeCallback = VolCb { shared, hwnd }.into();
            ep.RegisterControlChangeNotify(&cb).ok()?;
            Some(Volume { _ep: ep, _cb: cb })
        }
    }

    fn warm(&self) -> bool {
        unix_now() - self.started > 3.0
    }

    fn battery(&mut self, ctx: &mut Ctx) {
        let mut s = SYSTEM_POWER_STATUS::default();
        if unsafe { GetSystemPowerStatus(&mut s) }.is_err() || s.BatteryFlag & 128 != 0 || s.BatteryLifePercent > 100 {
            return; // no battery (desktop) or unknown
        }
        let pct = s.BatteryLifePercent as u32;
        let ac = s.ACLineStatus == 1;
        let right = format!("{pct}%");
        if let Some(prev) = self.ac {
            if prev != ac {
                if ac {
                    let t = if pct >= 100 { "Fully charged" } else { "Charging" };
                    ctx.toast(Toast::new("power", glyph::CHARGING, col::GREEN, t, "Charger connected").right(right.clone()).progress(pct as f32 / 100.0));
                } else {
                    ctx.toast(
                        Toast::new("power", glyph::BATTERY, col::YELLOW, "Charger unplugged", format!("{pct}% remaining"))
                            .right(right.clone())
                            .progress(pct as f32 / 100.0),
                    );
                }
            }
        }
        self.ac = Some(ac);
        if ac {
            self.low_warned = false;
            self.crit_warned = false;
            if pct >= 100 && !self.full_warned && self.warm() {
                self.full_warned = true;
                ctx.toast(Toast::new("power", glyph::CHARGING, col::GREEN, "Fully charged", "You can unplug").right(right));
            }
        } else {
            self.full_warned = false;
            if pct <= 10 && !self.crit_warned {
                self.crit_warned = true;
                self.low_warned = true;
                ctx.toast(Toast::new("power", glyph::BATTERY, col::RED, "Battery critical", "Plug in now").right(right).secs(8.0).chime());
            } else if pct <= 20 && !self.low_warned {
                self.low_warned = true;
                ctx.toast(Toast::new("power", glyph::BATTERY, col::ACCENT, "Low battery", "Plug in soon").right(right).secs(6.0).chime());
            }
        }
    }

    fn network(&mut self, ctx: &mut Ctx) {
        let Some(nlm) = &self.nlm else { return };
        let Ok(c) = (unsafe { nlm.GetConnectivity() }) else { return };
        let now = if c.0 & (NLM_CONNECTIVITY_IPV4_INTERNET.0 | NLM_CONNECTIVITY_IPV6_INTERNET.0) != 0 {
            Net::Internet
        } else if c.0 & (NLM_CONNECTIVITY_IPV4_LOCALNETWORK.0 | NLM_CONNECTIVITY_IPV6_LOCALNETWORK.0 | NLM_CONNECTIVITY_IPV4_SUBNET.0 | NLM_CONNECTIVITY_IPV6_SUBNET.0) != 0 {
            Net::Local
        } else {
            Net::Offline
        };
        match self.net {
            None => self.net = Some(now),
            Some(prev) if prev != now => {
                // Debounce: the state must hold for 2 s (Wi-Fi roaming flaps).
                match self.net_pending {
                    Some((p, since)) if p == now && unix_now() - since >= 2.0 => {
                        self.net = Some(now);
                        self.net_pending = None;
                        let t = match now {
                            Net::Internet => Toast::new("net", glyph::WIFI, col::GREEN, "Connected", "You're back online"),
                            Net::Local => Toast::new("net", glyph::NO_NET, col::YELLOW, "No internet access", "Connected to a network without internet"),
                            Net::Offline => Toast::new("net", glyph::WIFI_OFF, col::RED, "Disconnected", "No network connection"),
                        };
                        ctx.toast(t);
                    }
                    Some((p, _)) if p == now => {}
                    _ => self.net_pending = Some((now, unix_now())),
                }
            }
            _ => self.net_pending = None,
        }
    }

    fn keys(&mut self, ctx: &mut Ctx) {
        let caps = unsafe { GetKeyState(VK_CAPITAL.0 as i32) } & 1 == 1;
        if let Some(prev) = self.caps {
            if prev != caps {
                let (sub, c) = if caps { ("On", col::ACCENT) } else { ("Off", col::DIM) };
                ctx.toast(Toast::new("caps", glyph::CAPS, c, "Caps Lock", sub).secs(1.6));
            }
        }
        self.caps = Some(caps);

        let hkl = unsafe {
            let fg = GetForegroundWindow();
            let tid = GetWindowThreadProcessId(fg, None);
            GetKeyboardLayout(tid).0 as isize
        };
        if hkl == 0 {
            return;
        }
        if let Some(prev) = self.hkl {
            if prev != hkl {
                let lang = (hkl & 0xffff) as u32;
                let (name, short) = lang_name(lang);
                ctx.toast(Toast::new("lang", glyph::KEYBOARD, col::BLUE, name, "Keyboard language").right(short).secs(1.8));
            }
        }
        self.hkl = Some(hkl);
    }

    fn volume(&mut self, ctx: &mut Ctx) {
        let v = self.vol_shared.lock().unwrap().take();
        if let Some((level, muted)) = v {
            if !ctx.settings.volume_overlay || !self.warm() {
                return;
            }
            let pct = (level * 100.0).round() as u32;
            let (g, sub) = if muted || pct == 0 { (glyph::MUTE, "Muted") } else { (glyph::VOLUME, "Output") };
            ctx.toast(
                Toast::new("volume", g, col::TEXT, "Volume", sub)
                    .right(format!("{pct}%"))
                    .progress(if muted { 0.0 } else { level })
                    .secs(1.4),
            );
        }
    }

    fn bluetooth(&mut self, ctx: &mut Ctx) {
        let events = std::mem::take(&mut self.bt.lock().unwrap().events);
        for (name, connected) in events {
            if connected {
                ctx.toast(Toast::new("bt", glyph::HEADPHONE, col::BLUE, name, "Connected").secs(3.0));
            } else {
                ctx.toast(Toast::new("bt", glyph::BLUETOOTH, col::DIM, name, "Disconnected").secs(2.5));
            }
        }
    }
}

fn lang_name(langid: u32) -> (String, String) {
    unsafe {
        let mut buf = [0u16; 85];
        let n = LCIDToLocaleName(langid, Some(&mut buf), 0);
        if n <= 1 {
            return (format!("Layout {langid:04x}"), String::new());
        }
        let loc = String::from_utf16_lossy(&buf[..(n - 1) as usize]);
        let locw = HSTRING::from(loc.as_str());
        let mut name = [0u16; 128];
        let m = GetLocaleInfoEx(&locw, LOCALE_SLOCALIZEDDISPLAYNAME, Some(&mut name));
        let display = if m > 1 { String::from_utf16_lossy(&name[..(m - 1) as usize]) } else { loc.clone() };
        let short = loc.split('-').next().unwrap_or("").to_uppercase();
        (display, short)
    }
}

/// Connected Bluetooth (classic) devices coming and going: Windows says so
/// (nothing is polled).
fn spawn_bt_watcher(shared: Arc<Mutex<BtShared>>, hwnd: isize) {
    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let Ok(_watcher) = bt_watcher(shared, hwnd) else { return };
        // (It works as long as it's held.)
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
    });
}

fn bt_watcher(shared: Arc<Mutex<BtShared>>, hwnd: isize) -> Result<DeviceWatcher> {
    let selector = BluetoothDevice::GetDeviceSelectorFromConnectionStatus(BluetoothConnectionStatus::Connected)?;
    let watcher = DeviceInformation::CreateWatcherAqsFilter(&selector)?;
    // The ones connected now (id, name); the ones there at the start are no news.
    let known: Arc<Mutex<Vec<(String, String)>>> = Arc::default();
    let started = Arc::new(AtomicBool::new(false));
    let tell = move |name: String, connected: bool| {
        shared.lock().unwrap().events.push((name, connected));
        wake(hwnd);
    };
    let tell = Arc::new(tell);
    let (k, st, t) = (known.clone(), started.clone(), tell.clone());
    watcher.Added(&TypedEventHandler::new(move |_w: Ref<DeviceWatcher>, d: Ref<DeviceInformation>| {
        if let Some(d) = d.as_ref() {
            let (id, name) = (d.Id()?.to_string(), d.Name()?.to_string());
            let mut k = k.lock().unwrap();
            // A headset can be there more than once, under one name.
            let new = !k.iter().any(|x| x.1 == name);
            k.push((id, name.clone()));
            drop(k);
            if new && st.load(Ordering::Relaxed) {
                t(name, true);
            }
        }
        Ok(())
    }))?;
    let (k, t) = (known.clone(), tell.clone());
    watcher.Removed(&TypedEventHandler::new(move |_w: Ref<DeviceWatcher>, d: Ref<DeviceInformationUpdate>| {
        if let Some(d) = d.as_ref() {
            let id = d.Id()?.to_string();
            let mut k = k.lock().unwrap();
            let name = k.iter().position(|x| x.0 == id).map(|i| k.remove(i).1);
            let name = name.filter(|n| !k.iter().any(|x| x.1 == *n));
            drop(k);
            if let Some(n) = name {
                t(n, false);
            }
        }
        Ok(())
    }))?;
    // (Windows tells about the others only when this one is listened for too.)
    watcher.Updated(&TypedEventHandler::new(|_w: Ref<DeviceWatcher>, _d: Ref<DeviceInformationUpdate>| Ok(())))?;
    let st = started;
    watcher.EnumerationCompleted(&TypedEventHandler::new(move |_w: Ref<DeviceWatcher>, _x: Ref<IInspectable>| {
        st.store(true, Ordering::Relaxed);
        Ok(())
    }))?;
    watcher.Start()?;
    Ok(watcher)
}

impl Module for System {
    fn has_tab(&self) -> bool {
        false
    }
    fn tick(&mut self, ctx: &mut Ctx) {
        let _ = self.hwnd;
        self.battery(ctx);
        self.network(ctx);
        self.keys(ctx);
        self.volume(ctx);
        self.bluetooth(ctx);
    }
}
