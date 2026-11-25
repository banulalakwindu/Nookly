//! Microphone / camera in-use indicator. Windows records every app's use of
//! the mic and camera under `CapabilityAccessManager\ConsentStore` in the
//! registry: while an app is using one, its `LastUsedTimeStop` is 0. This
//! module reads that when Windows says it changed (no permissions needed,
//! nothing leaves the PC), shows an orange (mic) or green (camera) dot on the notch while
//! anything is listening or watching, and drops a short notice naming the
//! app when it starts.

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Registry::*;

use crate::gfx::*;
use crate::modules::{Ctx, Module, Rel, Toast};

const ROOT: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore";
pub const MIC: Color = col::ACCENT; // orange, like a phone's mic dot
pub const CAM: Color = col::GREEN;
/// Self-test: pretend a demo app is using the mic (toggle).
pub const A_DEMO: u32 = 990;

/// The microphone is in use (a call): chiming reminders wait for it.
pub static IN_CALL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(Default)]
pub struct Privacy {
    mic: Vec<String>,
    cam: Vec<String>,
    demo: bool,
    started: bool,
    /// Ticks also run on every background wake-up; scan at most once a second.
    last_scan: f64,
    watch: Watch,
    /// From the records, when they last changed: the microphone's and the
    /// camera's apps.
    recorded: (Vec<String>, Vec<String>),
    mics: Vec<windows::Win32::Media::Audio::IAudioSessionManager2>,
    mics_at: f64,
    mic_at: f64,
    mic_apps: Vec<String>,
}

/// Windows' own note that something under the records changed.
#[derive(Default)]
struct Watch {
    key: HKEY,
    event: HANDLE,
    /// When the records were last read.
    read_at: f64,
}

impl Watch {
    /// The records may have changed since the last time it was asked.
    fn changed(&mut self, now: f64) -> bool {
        use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
        let first = self.event.is_invalid();
        unsafe {
            if first {
                self.event = CreateEventW(None, false, false, None).unwrap_or_default();
                let _ = RegOpenKeyExW(HKEY_CURRENT_USER, &HSTRING::from(ROOT), Some(0), KEY_NOTIFY, &mut self.key);
            }
            let armed = !self.event.is_invalid() && !self.key.is_invalid();
            // (Now and then in any case, should a note be missed.)
            let fired = first || !armed || now - self.read_at > 30.0 || WaitForSingleObject(self.event, 0) == WAIT_OBJECT_0;
            if fired && armed {
                let _ = RegNotifyChangeKeyValue(self.key, true, REG_NOTIFY_CHANGE_NAME | REG_NOTIFY_CHANGE_LAST_SET, Some(self.event), true);
            }
            if fired {
                self.read_at = now;
            }
            fired
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        unsafe {
            if !self.key.is_invalid() {
                let _ = RegCloseKey(self.key);
            }
            if !self.event.is_invalid() {
                let _ = CloseHandle(self.event);
            }
        }
    }
}

impl Privacy {
    pub fn new() -> Privacy {
        Privacy::default()
    }
}

/// Readable app name from a ConsentStore key: packaged apps are
/// `Publisher.App_hash`, desktop apps are the exe path with `#` for `\`.
pub fn app_name(key: &str) -> String {
    if key.contains('#') {
        let file = key.rsplit('#').next().unwrap_or(key);
        let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
        return stem.to_string();
    }
    let base = key.split('_').next().unwrap_or(key);
    let last = base.rsplit('.').next().unwrap_or(base);
    // "WindowsSoundRecorder" -> "Windows Sound Recorder", "MSTeams" -> "MS Teams"
    let cs: Vec<char> = last.chars().collect();
    let mut out = String::new();
    for (i, &c) in cs.iter().enumerate() {
        if i > 0 && c.is_uppercase() {
            let prev_lower = cs[i - 1].is_lowercase();
            let next_lower = cs.get(i + 1).map_or(false, |n| n.is_lowercase());
            if prev_lower || (cs[i - 1].is_uppercase() && next_lower) {
                out.push(' ');
            }
        }
        out.push(c);
    }
    out
}

fn subkeys(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, &HSTRING::from(path), Some(0), KEY_READ, &mut key) != ERROR_SUCCESS {
            return out;
        }
        let mut buf = [0u16; 512];
        for i in 0..1024 {
            let mut len = buf.len() as u32;
            if RegEnumKeyExW(key, i, Some(PWSTR(buf.as_mut_ptr())), &mut len, None, Some(PWSTR::null()), None, None) != ERROR_SUCCESS {
                break;
            }
            out.push(String::from_utf16_lossy(&buf[..len as usize]));
        }
        let _ = RegCloseKey(key);
    }
    out
}

fn qword(path: &str, name: &str) -> Option<u64> {
    let mut v = 0u64;
    let mut size = 8u32;
    let r = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, &HSTRING::from(path), &HSTRING::from(name), RRF_RT_REG_QWORD, None, Some(&mut v as *mut u64 as *mut _), Some(&mut size))
    };
    (r == ERROR_SUCCESS).then_some(v)
}

/// Each microphone's list of the apps that use it.
fn mics() -> Vec<windows::Win32::Media::Audio::IAudioSessionManager2> {
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
    unsafe {
        let Ok(en) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else { return Vec::new() };
        let Ok(devs) = en.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE) else { return Vec::new() };
        (0..devs.GetCount().unwrap_or(0)).filter_map(|d| devs.Item(d).ok()?.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None).ok()).collect()
    }
}

/// Apps recording from any of these microphones right now, from Windows'
/// audio sessions (works on every Windows version; the registry records
/// below stopped being written on newer builds).
fn recording(mics: &[windows::Win32::Media::Audio::IAudioSessionManager2]) -> Vec<String> {
    use windows::core::Interface;
    use windows::Win32::Media::Audio::*;
    let me = std::process::id();
    let mut out = Vec::new();
    unsafe {
        for mgr in mics {
            let Ok(list) = mgr.GetSessionEnumerator() else { continue };
            for i in 0..list.GetCount().unwrap_or(0) {
                let Ok(ctl) = list.GetSession(i) else { continue };
                if ctl.GetState().map_or(true, |s| s != AudioSessionStateActive) {
                    continue;
                }
                let Ok(c2) = ctl.cast::<IAudioSessionControl2>() else { continue };
                let pid = c2.GetProcessId().unwrap_or(0);
                if pid == me || pid == 0 {
                    continue;
                }
                out.push(exe_of(pid).map_or_else(|| "An app".to_string(), |n| crate::modules::health::friendly(&n)));
            }
        }
    }
    out
}

fn exe_of(pid: u32) -> Option<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 520];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
        let _ = CloseHandle(p);
        ok.then(|| String::from_utf16_lossy(&buf[..n as usize])).and_then(|s| std::path::Path::new(&s).file_stem().map(|x| x.to_string_lossy().into_owned()))
    }
}

/// Apps currently using `cap` ("microphone" or "webcam").
pub fn in_use(cap: &str) -> Vec<String> {
    in_use_by(cap, &in_use_registry(cap), &if cap == "microphone" { recording(&mics()) } else { Vec::new() })
}

fn in_use_by(cap: &str, recorded: &[String], recording: &[String]) -> Vec<String> {
    let mut names = recorded.to_vec();
    names.extend_from_slice(recording);
    if cap == "webcam" && crate::modules::mirror::RUNNING.load(std::sync::atomic::Ordering::Relaxed) {
        names.push("Nookly Mirror".into());
    }
    names.sort();
    names.dedup();
    names
}

/// From Windows' privacy records in the registry.
fn in_use_registry(cap: &str) -> Vec<String> {
    let base = format!(r"{ROOT}\{cap}");
    let mut keys: Vec<String> = subkeys(&base).into_iter().filter(|k| k != "NonPackaged").map(|k| format!(r"{base}\{k}")).collect();
    let np = format!(r"{base}\NonPackaged");
    keys.extend(subkeys(&np).into_iter().map(|k| format!(r"{np}\{k}")));
    let mut names: Vec<String> = keys
        .iter()
        .filter(|k| qword(k, "LastUsedTimeStart").unwrap_or(0) > 0 && qword(k, "LastUsedTimeStop") == Some(0))
        .map(|k| app_name(k.rsplit('\\').next().unwrap_or(k)))
        .collect();
    names.sort();
    names.dedup();
    names
}

fn list(names: &[String]) -> String {
    match names.len() {
        0 => String::new(),
        1 => names[0].clone(),
        2 => format!("{} and {}", names[0], names[1]),
        n => format!("{}, {} and {} more", names[0], names[1], n - 2),
    }
}

impl Module for Privacy {
    fn title(&self) -> &'static str {
        "Privacy"
    }
    fn has_tab(&self) -> bool {
        false
    }

    fn tick(&mut self, ctx: &mut Ctx) {
        let now = crate::time::unix_now();
        if now - self.last_scan < 0.9 && self.started {
            return;
        }
        self.last_scan = now;
        if self.watch.changed(now) {
            self.recorded = (in_use_registry("microphone"), in_use_registry("webcam"));
        }
        // Listing the microphones costs far more than asking them.
        if now - self.mics_at > 10.0 || now < self.mics_at {
            self.mics_at = now;
            self.mics = mics();
        }
        if now - self.mic_at > 1.9 || now < self.mic_at {
            self.mic_at = now;
            self.mic_apps = recording(&self.mics);
        }
        let (mut mic, cam) = (in_use_by("microphone", &self.recorded.0, &self.mic_apps), in_use_by("webcam", &self.recorded.1, &[]));
        // A call (an app other than Nookly using the mic): reminders wait.
        IN_CALL.store(mic.iter().any(|a| !a.to_lowercase().contains("nookly")), std::sync::atomic::Ordering::Relaxed);
        if self.demo {
            mic.push("Demo Recorder".into());
        }
        if !ctx.settings.privacy_dots {
            self.mic.clear();
            self.cam.clear();
            return;
        }
        // Announce what started (not on Nookly's own start-up).
        if self.started {
            let new_mic: Vec<String> = mic.iter().filter(|a| !self.mic.contains(a)).cloned().collect();
            let new_cam: Vec<String> = cam.iter().filter(|a| !self.cam.contains(a)).cloned().collect();
            if !new_cam.is_empty() {
                ctx.toast(Toast::new("privacy", glyph::CAMERA, CAM, "Camera in use", list(&new_cam)).secs(3.0));
            } else if !new_mic.is_empty() {
                ctx.toast(Toast::new("privacy", glyph::MIC, MIC, "Microphone in use", list(&new_mic)).secs(3.0));
            }
        }
        self.started = true;
        self.mic = mic;
        self.cam = cam;
    }

    fn indicator(&self) -> Vec<Color> {
        let mut v = Vec::new();
        if !self.cam.is_empty() {
            v.push(CAM);
        }
        if !self.mic.is_empty() {
            v.push(MIC);
        }
        v
    }

    fn action(&mut self, id: u32, _rel: Rel, _ctx: &mut Ctx) {
        if id == A_DEMO {
            self.demo = !self.demo;
            self.last_scan = 0.0;
        }
    }

    fn debug_state(&self) -> String {
        format!("mic=[{}] cam=[{}]", self.mic.join(", "), self.cam.join(", "))
    }
}
