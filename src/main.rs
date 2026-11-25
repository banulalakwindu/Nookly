#![windows_subsystem = "windows"]
// Many helpers (gfx shapes, palette, Ctx methods) exist for future modules.
#![allow(dead_code)]
//! Nookly: a living notch for Windows.
//!
//! Layout:
//!   app.rs      window, animation, input routing, toasts, mascot
//!   gfx.rs      Direct2D drawing into a per-pixel-alpha layered window
//!   ui.rs       immediate-mode widgets (buttons, chips, progress, hits)
//!   modules/    one file per feature, add your own here (see modules/mod.rs)
//!   tray.rs     tray icon + menu
//!   dragdrop.rs OLE drag in/out
//!   store.rs    %APPDATA%\Nookly\*.txt persistence
//!   http.rs     WinHTTP GET (used for LRCLIB lyrics)

mod app;
mod astro;
mod faces;
mod fullviews;
mod zones;
// Shared with the taskbar add-on (src/lib.rs).
use nookly_core::{barcfg, feed, gfx, glass, shell, store, surfaces, time, ui};
mod brightness;
mod dialog;
mod dragdrop;
mod features;
mod http;
mod mascot;
mod modules;
mod sound;
mod setup;
mod share;
mod taskbar;
mod tray;
mod uninstall;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Ole::OleInitialize;
use windows::Win32::System::Threading::{CreateMutexW, CreateWaitableTimerExW, SetWaitableTimer, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, INFINITE};
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// The message loop, which also paces animation frames: transitions draw
/// once per display refresh (`app::wait_refresh`), slow ambient animations
/// wake on a precise high-resolution timer, and when nothing moves the
/// thread sleeps until a message arrives. (A plain Windows timer ticks in
/// ~15.6 ms steps and is delayed freely, which made animations choppy.)
unsafe fn run_loop() { unsafe {
    const TIMER_ALL_ACCESS: u32 = 0x1F0003;
    let timer = CreateWaitableTimerExW(None, PCWSTR::null(), CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, TIMER_ALL_ACCESS)
        .or_else(|_| CreateWaitableTimerExW(None, PCWSTR::null(), 0, TIMER_ALL_ACCESS))
        .ok();
    let mut msg = MSG::default();
    loop {
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            if msg.message == WM_QUIT {
                return;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let wake = |handles: &[HANDLE]| {
            MsgWaitForMultipleObjectsEx(Some(handles), INFINITE, QS_ALLINPUT, MWMO_INPUTAVAILABLE) == WAIT_OBJECT_0 && !handles.is_empty()
        };
        match app::with_app(|a| a.pacing()) {
            Some(app::Pacing::Refresh) => {
                if let Some(h) = app::with_app(|a| {
                    a.frame();
                    a.hwnd
                }) {
                    app::wait_refresh(h);
                }
            }
            Some(app::Pacing::After(secs)) if secs < 0.001 => {
                app::with_app(|a| a.frame());
            }
            Some(app::Pacing::After(secs)) => match timer {
                Some(t) => {
                    let due = -((secs * 1e7) as i64).max(1);
                    let _ = SetWaitableTimer(t, &due, 0, None, None, false);
                    if wake(&[t]) {
                        app::with_app(|a| a.frame());
                    }
                }
                None => {
                    std::thread::sleep(std::time::Duration::from_secs_f64(secs.min(0.05)));
                }
            },
            _ => {
                wake(&[]);
            }
        }
    }
}}

fn main() {
    // Run by Claude Code / Codex hooks: hand the event to the notch and quit
    // at once (no window, no COM).
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--agent-event") {
        std::process::exit(modules::agents::hook_main(&args));
    }
    // The installer's uninstaller calls this so no hook is left behind.
    if args.iter().any(|a| a == "--disconnect-agents") {
        modules::agents::disconnect_all();
        std::process::exit(0);
    }
    // Started as unins000.exe (where the old Inno installer's uninstaller
    // was, which Windows may still remember): open Nookly's own uninstaller.
    if setup::is_inno_stub() {
        let nookly = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("nookly.exe")));
        if let Some(n) = nookly.filter(|n| n.exists()) {
            let _ = std::process::Command::new(n).arg("--uninstall").spawn();
        }
        std::process::exit(0);
    }
    // NooklySetup-<version>.exe is this exe: install it (setup.rs).
    if setup::wanted(&args) {
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        std::process::exit(setup::run(&args));
    }
    if args.iter().any(|a| a == "--uninstall") {
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        std::process::exit(uninstall::run(&args));
    }
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = OleInitialize(None);

        let args: Vec<String> = std::env::args().collect();
        // `--selftest` and the docs renders (a build with the feature
        // `selftest`; the released program has none of it).
        #[cfg(feature = "selftest")]
        app::run_tests(&args);
        #[cfg(not(feature = "selftest"))]
        if args.iter().any(|a| matches!(a.as_str(), "--selftest" | "--render-demo" | "--render-docs" | "--probe-privacy")) {
            std::process::exit(2);
        }

        let mutex = CreateMutexW(None, true, w!("Local\\Nookly.SingleInstance"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }
        if let Ok(h) = &mutex {
            app::SINGLE_INSTANCE.store(h.0 as isize, std::sync::atomic::Ordering::SeqCst);
        }

        if let Err(e) = app::App::create(true) {
            let msg = HSTRING::from(format!("Nookly failed to start:\n{e}"));
            MessageBoxW(None, &msg, w!("Nookly"), MB_ICONERROR);
            return;
        }

        run_loop();
        app::shutdown();
        drop(mutex);
        // Exit directly. OleUninitialize() while the app's COM objects (D2D,
        // audio callback, network list) are still alive never returns, and
        // worker threads may be mid-WinRT call; the OS cleans all of it up.
        std::process::exit(0);
    }
}
