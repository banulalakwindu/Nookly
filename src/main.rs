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
        // `--probe-privacy <file> [camera]`: write what Windows reports as
        // using the microphone and camera, every second for 8 seconds (to
        // check the mic and camera dots); with `camera`, open the camera
        // meanwhile.
        if let Some(i) = args.iter().position(|a| a == "--probe-privacy") {
            let out = args.get(i + 1).cloned().unwrap_or_else(|| "probe.txt".into());
            let cam = args.iter().any(|a| a == "camera").then(|| modules::mirror::probe_camera(6.0));
            let mut log = String::new();
            for k in 0..8 {
                let c = cam.as_ref().map(|(f, e)| (f.lock().unwrap().as_ref().map(|x| x.3), e.lock().unwrap().clone()));
                log += &format!("{k}s mic={:?} camera={:?} frames/err={:?}
", modules::privacy::in_use("microphone"), modules::privacy::in_use("webcam"), c);
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            let _ = std::fs::write(out, log);
            std::process::exit(0);
        }
        // Docs renders: demo data, fixed clock, no window shown.
        if args.iter().any(|a| a == "--selftest" || a == "--render-demo" || a == "--render-docs") {
            modules::agents::ISOLATED.store(true, std::sync::atomic::Ordering::Relaxed);
            // His screen may have a maximized window open: renders and tests
            // assume none unless a test says otherwise.
            app::MAXIMIZED_OVERRIDE.store(2, std::sync::atomic::Ordering::Relaxed);
            // Tests and renders change data (tasks, settings...): never
            // your own unless asked with --real-data.
            let set = std::env::var("NOOKLY_DATA_DIR").map_or(false, |d| !d.is_empty());
            if !set && !args.iter().any(|a| a == "--real-data") {
                let d = std::env::temp_dir().join(format!("nookly-test-{}", std::process::id()));
                let _ = std::fs::create_dir_all(&d);
                // Renders start past the welcome tour (the self-test checks
                // the tour itself, from a fresh folder).
                if !args.iter().any(|a| a == "--selftest") {
                    let _ = std::fs::write(d.join("settings.txt"), "onboarded=1\n");
                }
                std::env::set_var("NOOKLY_DATA_DIR", &d);
            }
        }
        if let Some(i) = args.iter().position(|a| a == "--render-demo") {
            time::DEMO.store(true, std::sync::atomic::Ordering::Relaxed);
            let dir = std::path::PathBuf::from(args.get(i + 1).cloned().unwrap_or_else(|| "demo-render".into()));
            let _ = app::App::create(false);
            let log = app::render_demo(&dir);
            let _ = std::fs::write(dir.join("render.log"), log);
            app::shutdown();
            std::process::exit(0);
        }
        if let Some(i) = args.iter().position(|a| a == "--render-docs") {
            time::DEMO.store(true, std::sync::atomic::Ordering::Relaxed);
            let dir = std::path::PathBuf::from(args.get(i + 1).cloned().unwrap_or_else(|| "docs-render".into()));
            let _ = app::App::create(false);
            let log = app::render_docs(&dir);
            let _ = std::fs::write(dir.join("render.log"), log);
            app::shutdown();
            std::process::exit(0);
        }

        // The self-test never shows a window, so it may run alongside the app.
        if std::env::args().any(|a| a == "--selftest") {
            let _ = app::App::create(false);
            let mut report = app::selftest();
            report += &format!("INFO lyrics (LRCLIB): {}\n", modules::music::selftest_lyrics());
            report += &format!("INFO data dir: {}\n", store::dir().display());
            let out = std::env::var("NOOKLY_SELFTEST_OUT").unwrap_or_else(|_| "selftest.txt".into());
            let _ = std::fs::write(out, report);
            app::shutdown();
            std::process::exit(0);
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
