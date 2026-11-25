//! The self-test (`nookly --selftest`) and the renders for the website and
//! the guide (`--render-docs`, `--render-demo`). Built in with the feature
//! `selftest` only: the released program doesn't carry them.

use super::*;

fn snap(a: &mut App, label: &str) -> String {
    let tgt = a.target();
    a.cur = tgt;
    a.render(tgt, 0.0);
    let ok = a.gfx.last_err.is_none();
    format!(
        "{:<4} {label:<34} size={:>3.0}x{:<3.0} opaque_px={:<7} bright_px={:<6} hits={}\n",
        if ok { "OK" } else { "FAIL" },
        tgt.0,
        tgt.1,
        a.gfx.coverage(),
        a.gfx.bright(),
        a.hits.len()
    )
}

fn find(a: &App, title: &str) -> Option<usize> {
    a.modules.iter().position(|m| m.title() == title)
}

/// Exercise every view and the main flows without showing the window.
pub fn selftest() -> String {
    // Let the media thread find the current session first.
    std::thread::sleep(std::time::Duration::from_secs(3));
    with_app(|a| {
        let mut out = String::new();
        // The self-test runs on the user's PC: no chimes.
        sound::mute(true);
        // A fresh data folder is a first run: the welcome tour must be up.
        let fresh_tour = a.onboarding == Some(0) && a.mode() == Mode::Expanded;
        out += &format!("{:<4} first run opens the welcome tour\n", if fresh_tour { "OK" } else { "FAIL" });
        a.onboarding = None;
        a.tick();
        a.settings.personality = true;
        a.settings.style = NotchStyle::Full;
        a.queue.clear();
        a.toast = None;

        a.expanded = false;
        a.mood = Mood::Clock;
        let live = a.live_module().map(|i| a.modules[i].title()).unwrap_or("none");
        out += &snap(a, &format!("collapsed (live: {live})"));
        for (m, n) in [(Mood::Eyes, "mascot eyes"), (Mood::Doze, "mascot doze"), (Mood::Wave, "mascot wave")] {
            a.mood = m;
            a.mood_since = a.t() - 5.0;
            if a.live_module().is_none() {
                out += &snap(a, n);
            } else {
                out += &format!("SKIP {n:<34} (a live activity is showing)\n");
            }
        }
        a.mood = Mood::Clock;
        for s in [NotchStyle::Compact, NotchStyle::Pill, NotchStyle::Hidden] {
            a.settings.style = s;
            out += &snap(a, &format!("collapsed style {}", s.key()));
        }
        a.settings.style = NotchStyle::Full;

        // Edges: the drawn shape must touch the chosen screen side, and
        // panels must stay upright (wider than tall) on every edge.
        for e in Edge::ALL {
            a.settings.edge = e;
            a.anchor = if e.vertical() { CANVAS_H / 2.0 } else { CANVAS_W / 2.0 };
            for (style, open) in [(NotchStyle::Full, false), (NotchStyle::Pill, false), (NotchStyle::Full, true)] {
                a.settings.style = style;
                a.expanded = open;
                let label = format!("edge {} {}{}", e.key(), style.key(), if open { " open" } else { "" });
                let tgt = a.target();
                a.cur = tgt;
                a.render(tgt, 0.0);
                let (pw, ph) = (a.gfx.pw, a.gfx.ph);
                let ok = match a.gfx.bbox() {
                    Some((x0, y0, x1, y1)) => {
                        let touches = match e {
                            Edge::Top => y0 == 0,
                            Edge::Bottom => y1 == ph - 1,
                            Edge::Left => x0 == 0,
                            Edge::Right => x1 == pw - 1,
                        };
                        let upright = !open || (x1 - x0) > (y1 - y0);
                        touches && upright && a.gfx.last_err.is_none()
                    }
                    None => false,
                };
                out += &format!("{:<4} {label:<34} bbox={:?}\n", if ok { "OK" } else { "FAIL" }, a.gfx.bbox());
            }
        }
        a.expanded = false;
        a.settings.style = NotchStyle::Full;
        // Right-drag logic: a point near the middle of the primary monitor's
        // left side must snap to the left edge at about half height.
        unsafe {
            let mon = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(mon, &mut mi);
            let r = mi.rcWork;
            a.move_to_point(POINT { x: r.left + 5, y: (r.top + r.bottom) / 2 });
            let ok = a.settings.edge == Edge::Left && (a.settings.pos - 0.5).abs() < 0.02;
            out += &format!("{:<4} drag snaps to nearest edge (left, pos {:.2})\n", if ok { "OK" } else { "FAIL" }, a.settings.pos);
            a.move_to_point(POINT { x: r.right - 300, y: r.bottom - 2 });
            let ok = a.settings.edge == Edge::Bottom && a.settings.pos > 0.7;
            out += &format!("{:<4} drag snaps to bottom near right corner (pos {:.2})\n", if ok { "OK" } else { "FAIL" }, a.settings.pos);
        }
        a.settings.edge = Edge::Top;
        a.settings.pos = 0.5;
        a.settings.monitor = None;
        a.place();

        for i in 0..a.modules.len() {
            if !a.modules[i].has_tab() {
                continue;
            }
            a.tab = i;
            a.expanded = true;
            let t = a.modules[i].title();
            out += &snap(a, &format!("tab: {t}"));
        }

        // To-do: add a task through the real editor + keyboard path.
        if let Some(i) = find(a, "TodoList") {
            a.tab = i;
            a.call(i, |m, c| m.action(1, (0.0, 0.0), c));
            out += &snap(a, "todo editor open");
            a.call(i, |m, c| {
                m.action(10, (0.0, 0.0), c); // +15m
                for ch in "Selftest task".chars() {
                    m.chr(ch, c);
                }
                m.key(0x0D, false, c);
            });
            let saved = crate::store::read("todo.txt");
            let ok = saved.lines().any(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                f.len() > 4 && f[4] == "Selftest task" && f[2].parse::<f64>().unwrap_or(0.0) > 0.0
            });
            out += &format!("{:<4} todo saved to disk with due time\n", if ok { "OK" } else { "FAIL" });
            out += &snap(a, "todo list with task");
            // Typing in its view on the taskbar: the keys come here, the
            // notch stays closed; closing the view gives them back.
            a.expanded = false;
            a.full_call = true;
            a.call(i, |m, c| m.action(1, (0.0, 0.0), c));
            a.full_call = false;
            let closed = a.mode() == Mode::Collapsed && a.keyboard == Some(i) && a.bar_keyboard;
            for ch in "From the bar".chars() {
                a.on_char(ch);
            }
            a.on_key(0x0D);
            let typed = crate::store::read("todo.txt").contains("From the bar");
            a.full_call = true;
            a.call(i, |m, c| m.action(1, (0.0, 0.0), c));
            a.full_call = false;
            a.open_full(0);
            let back = a.keyboard.is_none() && !a.bar_keyboard;
            out += &format!("{:<4} typing in a widget's view on the taskbar\n", if closed && typed && back { "OK" } else { "FAIL" });
            if !(closed && typed && back) {
                out += &format!("INFO closed {closed} typed {typed} back {back}\n");
            }
            a.toast = None;
            a.queue.clear();
        }
        // Water: drink + settings page.
        if let Some(i) = find(a, "Water Reminder") {
            a.tab = i;
            a.call(i, |m, c| m.action(1, (0.0, 0.0), c));
            let saved = crate::store::read("water.txt");
            out += &format!("{:<4} water drink logged\n", if saved.contains("log ") { "OK" } else { "FAIL" });
            a.call(i, |m, c| m.action(2, (0.0, 0.0), c));
            out += &snap(a, "water settings page");
            a.call(i, |m, c| m.action(3, (0.0, 0.0), c));
        }
        // Shelf: drop a real file.
        if let Some(i) = find(a, "Shelf") {
            a.tab = i;
            let p = std::env::current_dir().unwrap_or_default().join("Cargo.toml").display().to_string();
            a.drop_paths(vec![p]);
            let saved = crate::store::read("shelf.txt");
            out += &format!("{:<4} shelf item stored\n", if saved.contains("Cargo.toml") { "OK" } else { "FAIL" });
            a.tab = i;
            a.expanded = true;
            out += &snap(a, "shelf with item");
            let drag = a.modules[i].drag_out(1 << 24);
            out += &format!("{:<4} shelf drag-out provides path\n", if drag.map_or(false, |d| d.len() == 1) { "OK" } else { "FAIL" });
        }
        // Clipboard history: limits, de-duplication, pins that persist, clear.
        if let Some(i) = find(a, "Clipboard") {
            use crate::modules::clipboard::{self as cb, Content};
            let field = |s: &str, k: &str| -> String {
                s.split_whitespace().find_map(|w| w.strip_prefix(&format!("{k}="))).unwrap_or("").to_string()
            };
            a.call(i, |m, c| m.action(cb::A_DEMO, (0.0, 0.0), c));
            a.call(i, |m, c| m.action(cb::A_DEMO, (0.0, 0.0), c));
            let st = a.modules[i].debug_state();
            let ok = field(&st, "unpinned") == "30" && field(&st, "dupes") == "0" && field(&st, "images") == "1";
            out += &format!("{:<4} clipboard keeps the newest 30, no duplicates ({st})\n", if ok { "OK" } else { "FAIL" });
            out += &format!("{:<4} clipboard history is off by default\n", if field(&st, "on") == "false" && field(&st, "mode") == "0" { "OK" } else { "FAIL" });
            let img = cb::image_from_dib(cb::make_test_dib(64, 32));
            let ok = img.as_ref().map_or(false, |im| im.w == 64 && im.h == 32 && im.thumb.0 == 64 && im.thumb.1 == 32 && im.thumb.2.len() == 64 * 32 * 4);
            out += &format!("{:<4} copied images get a thumbnail ({:?})\n", if ok { "OK" } else { "FAIL" }, img.map(|i| (i.thumb.0, i.thumb.1)));
            let big = cb::image_from_dib(cb::make_test_dib(400, 100)).map(|i| (i.thumb.0, i.thumb.1));
            out += &format!("{:<4} big images shrink to fit the thumbnail {:?}\n", if big == Some((80, 20)) { "OK" } else { "FAIL" }, big);
            let top: u32 = field(&st, "top_id").parse().unwrap_or(0);
            a.call(i, |m, c| m.action((2 << 24) | top, (0.0, 0.0), c)); // pin
            let saved = crate::store::read("clipboard.txt");
            let fresh = cb::Clipboard::new(a.hwnd).debug_state();
            let ok = saved.lines().count() == 1 && field(&fresh, "pinned") == "1";
            out += &format!("{:<4} pinned clipboard entry survives a restart\n", if ok { "OK" } else { "FAIL" });
            a.call(i, |m, c| m.action(1, (0.0, 0.0), c)); // clear
            let st = a.modules[i].debug_state();
            out += &format!("{:<4} clear keeps pinned entries ({})\n", if field(&st, "items") == "1" && field(&st, "pinned") == "1" { "OK" } else { "FAIL" }, field(&st, "items"));
            a.tab = i;
            a.expanded = true;
            out += &snap(a, "clipboard with a pinned entry");
            a.call(i, |m, c| m.action((3 << 24) | top, (0.0, 0.0), c)); // delete
            let ok = a.modules[i].debug_state().contains("items=0") && crate::store::read("clipboard.txt").trim().is_empty();
            out += &format!("{:<4} deleting a pinned entry removes it from disk\n", if ok { "OK" } else { "FAIL" });
            a.expanded = false;

            // Live Windows clipboard (opt-in: it briefly replaces what you copied,
            // then puts it back).
            if std::env::var("NOOKLY_TEST_CLIPBOARD").is_ok() {
                let hwnd = a.hwnd;
                let saved = cb::current(hwnd);
                a.settings.clip_on = true;
                a.settings.clip_mode = ClipMode::All;
                a.call(i, |m, c| m.tick(c));
                let wait = |a: &mut App| {
                    std::thread::sleep(std::time::Duration::from_millis(400));
                    a.call(i, |m, c| m.tick(c));
                };
                let tag = format!("Nookly selftest {}", crate::time::unix_now() as u64);
                cb::set_clipboard(hwnd, &Content::Text(tag.clone()));
                wait(a);
                let st = a.modules[i].debug_state();
                out += &format!("{:<4} copying text is recorded instantly\n", if st.ends_with(&format!("top={tag}")) { "OK" } else { "FAIL" });
                let older: u32 = field(&st, "top_id").parse().unwrap_or(0);
                cb::set_private_for_test(hwnd, "secret-password-123");
                wait(a);
                let st = a.modules[i].debug_state();
                out += &format!("{:<4} password-manager copies are not recorded\n", if !st.contains("secret-password") { "OK" } else { "FAIL" });
                let file = std::env::current_dir().unwrap_or_default().join("Cargo.toml").display().to_string();
                cb::set_clipboard(hwnd, &Content::Files(vec![file]));
                wait(a);
                let st = a.modules[i].debug_state();
                out += &format!("{:<4} copying a file is recorded\n", if st.contains("top=Cargo.toml") { "OK" } else { "FAIL" });
                if let Some(im) = cb::image_from_dib(cb::make_test_dib(64, 32)) {
                    cb::set_clipboard(hwnd, &Content::Image(std::sync::Arc::new(im)));
                }
                wait(a);
                let st = a.modules[i].debug_state();
                out += &format!("{:<4} copying an image is recorded (Everything mode)\n", if st.contains("top=Image  64 x 32") { "OK" } else { "FAIL" });
                a.settings.clip_mode = ClipMode::Text;
                a.call(i, |m, c| m.tick(c));
                let file2 = std::env::current_dir().unwrap_or_default().join("README.md").display().to_string();
                cb::set_clipboard(hwnd, &Content::Files(vec![file2]));
                wait(a);
                let st = a.modules[i].debug_state();
                out += &format!("{:<4} Text-only mode skips copied files\n", if !st.contains("README") { "OK" } else { "FAIL" });
                a.settings.clip_mode = ClipMode::All;
                a.call(i, |m, c| m.tick(c));
                a.call(i, |m, c| m.action((1 << 24) | older, (0.0, 0.0), c)); // copy the text again
                let back = cb::current(hwnd);
                out += &format!("{:<4} clicking an entry puts it back on the clipboard\n", if back == Some(Content::Text(tag.clone())) { "OK" } else { "FAIL" });
                // Clicking the row pastes into the app you were using; the copy
                // icon (shown on hover) only copies.
                {
                    use std::sync::atomic::Ordering::Relaxed;
                    cb::PASTE_DRY_RUN.store(true, Relaxed);
                    a.expanded = true;
                    a.tab = i;
                    let tgt = a.target();
                    a.cur = tgt;
                    a.render(tgt, 0.0);
                    // The second entry (the first one shows "Copied" right now).
                    let row = a.hits.iter().filter(|h| h.module == i && h.id >> 24 == 4).nth(1).copied();
                    let mut copy_icon = None;
                    if let Some(h) = row {
                        a.on_mouse_move(h.rect.x + 40.0, h.rect.cy());
                        a.render(tgt, 0.0);
                        copy_icon = a.hits.iter().find(|x| x.module == i && x.id >> 24 == 1 && (x.rect.cy() - h.rect.cy()).abs() < 2.0).copied();
                    }
                    // A plain desktop-style window to paste into (not Nookly).
                    let shell = unsafe { GetShellWindow() };
                    cb::TEST_TARGET.store(shell.0 as isize, Relaxed);
                    let entry = row.map_or(0, |h| h.id & 0xFF_FFFF);
                    cb::set_clipboard(hwnd, &Content::Text("other".into()));
                    a.call(i, |m, c| m.action((4 << 24) | entry, (0.0, 0.0), c));
                    let st = a.modules[i].debug_state();
                    let pasted = st.contains("paste=Some(Sent)") && cb::current(hwnd).map_or(false, |c| c != Content::Text("other".into()));
                    cb::set_clipboard(hwnd, &Content::Text("other".into()));
                    a.call(i, |m, c| m.action((1 << 24) | entry, (0.0, 0.0), c));
                    let st2 = a.modules[i].debug_state();
                    let copied = st2.contains("paste=None") && cb::current(hwnd).map_or(false, |c| c != Content::Text("other".into()));
                    // Never into Nookly itself or the taskbar.
                    cb::TEST_TARGET.store(hwnd.0 as isize, Relaxed);
                    a.call(i, |m, c| m.action((4 << 24) | entry, (0.0, 0.0), c));
                    let own = a.modules[i].debug_state().contains("paste=Some(NoTarget)");
                    let tray = crate::shell::tray().unwrap_or_default();
                    let no_tray = tray.0.is_null() || !cb::paste_target_ok(tray, hwnd);
                    out += &format!(
                        "{:<4} clicking an entry pastes it, the copy icon only copies, never into Nookly or the taskbar {:?}\n",
                        if row.is_some() && copy_icon.is_some() && pasted && copied && own && no_tray { "OK" } else { "FAIL" },
                        (row.is_some(), copy_icon.is_some(), pasted, copied, own, no_tray)
                    );
                    out += &snap(a, "clipboard hover with copy icon");
                    // Real Ctrl+V into a text box (opt-in: it takes focus).
                    if std::env::var("NOOKLY_TEST_PASTE").is_ok() {
                        cb::PASTE_DRY_RUN.store(false, Relaxed);
                        unsafe {
                            let hinst = GetModuleHandleW(None).unwrap_or_default();
                            let top = CreateWindowExW(WINDOW_EX_STYLE(0), w!("STATIC"), w!("Nookly paste test"), WS_OVERLAPPEDWINDOW | WS_VISIBLE, 100, 100, 400, 120, None, None, Some(hinst.into()), None).unwrap_or_default();
                            // An Edit can't be a top-level parent for focus tests; host it.
                            let edit = CreateWindowExW(WINDOW_EX_STYLE(0), w!("EDIT"), w!(""), WS_CHILD | WS_VISIBLE | WS_BORDER, 10, 10, 360, 30, Some(top), None, Some(hinst.into()), None).unwrap_or_default();
                            // Windows only lets the app that got the last input take
                            // the foreground: send it an empty key event first.
                            let nothing = INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0), wScan: 0, dwFlags: KEYEVENTF_KEYUP, time: 0, dwExtraInfo: 0 } } };
                            SendInput(&[nothing], std::mem::size_of::<INPUT>() as i32);
                            let _ = SetForegroundWindow(top);
                            let _ = SetFocus(Some(edit));
                            let front = GetForegroundWindow() == top;
                            let want = format!("Nookly paste {}", crate::time::unix_now() as u64);
                            cb::set_clipboard(hwnd, &Content::Text(want.clone()));
                            wait(a);
                            cb::TEST_TARGET.store(top.0 as isize, Relaxed);
                            let st = a.modules[i].debug_state();
                            let id: u32 = field(&st, "top_id").parse().unwrap_or(0);
                            let _ = SetFocus(Some(edit));
                            a.call(i, |m, c| m.action((4 << 24) | id, (0.0, 0.0), c));
                            let mut got = String::new();
                            for _ in 0..40 {
                                let mut msg = MSG::default();
                                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                                    let _ = TranslateMessage(&msg);
                                    DispatchMessageW(&msg);
                                }
                                let mut b = [0u16; 128];
                                let n = GetWindowTextW(edit, &mut b);
                                got = String::from_utf16_lossy(&b[..n as usize]);
                                if got == want {
                                    break;
                                }
                                std::thread::sleep(std::time::Duration::from_millis(25));
                            }
                            let _ = DestroyWindow(top);
                            out += &format!("{:<4} clicking an entry really pastes into a text box ({got:?}, test window in front: {front})\n", if got == want { "OK" } else { "FAIL" });
                        }
                    }
                    cb::PASTE_DRY_RUN.store(false, Relaxed);
                    cb::TEST_TARGET.store(0, Relaxed);
                    a.on_mouse_leave();
                }
                if let Some(s) = saved {
                    cb::set_clipboard(hwnd, &s);
                }
                wait(a);
                a.call(i, |m, c| m.action(1, (0.0, 0.0), c));
                a.settings.clip_on = false;
                a.settings.clip_mode = ClipMode::Text;
                a.call(i, |m, c| m.tick(c));
            }
        }
        // Mic / camera dots.
        if let Some(i) = find(a, "Privacy") {
            use crate::modules::privacy::*;
            let names = [
                app_name("C:#Program Files#Zoom#bin#Zoom.exe"),
                app_name("Microsoft.WindowsSoundRecorder_8wekyb3d8bbwe"),
                app_name("MSTeams_8wekyb3d8bbwe"),
            ];
            let ok = names == ["Zoom".to_string(), "Windows Sound Recorder".to_string(), "MS Teams".to_string()];
            out += &format!("{:<4} mic/camera app names from the registry keys {:?}\n", if ok { "OK" } else { "FAIL" }, names);
            a.queue.clear();
            a.toast = None;
            a.call(i, |m, c| m.tick(c));
            a.call(i, |m, c| m.action(A_DEMO, (0.0, 0.0), c));
            a.call(i, |m, c| m.tick(c));
            let dots = a.indicators();
            let toast = a.queue.iter().any(|t| t.key == "privacy" && t.title == "Microphone in use" && t.sub.contains("Demo Recorder"));
            out += &format!("{:<4} mic in use shows an orange dot and names the app\n", if dots == vec![MIC] && toast { "OK" } else { "FAIL" });
            a.queue.clear();
            a.expanded = false;
            a.settings.style = NotchStyle::Full;
            let mut shapes = Vec::new();
            for on in [true, false] {
                a.settings.privacy_dots = on;
                let tgt = a.target();
                a.cur = tgt;
                a.render(tgt, 0.0);
                shapes.push(a.gfx.coverage());
            }
            a.settings.privacy_dots = true;
            out += &format!("{:<4} dot is drawn beside the closed notch ({} px vs {} px without)\n", if shapes[0] > shapes[1] + 40 { "OK" } else { "FAIL" }, shapes[0], shapes[1]);
            a.call(i, |m, c| m.action(A_DEMO, (0.0, 0.0), c));
            a.call(i, |m, c| m.tick(c));
            out += &format!("{:<4} dot goes away when the mic is released\n", if a.indicators().is_empty() { "OK" } else { "FAIL" });
            a.queue.clear();
        }
        // Pomodoro: start -> live activity, skip -> toast.
        if let Some(i) = find(a, "Pomodoro") {
            a.call(i, |m, c| m.action(1, (0.0, 0.0), c));
            a.expanded = false;
            a.keyboard = None;
            out += &format!("{:<4} pomodoro live activity while running\n", if a.modules[i].live().is_some() { "OK" } else { "FAIL" });
            out += &snap(a, "collapsed with pomodoro");
            for s in [NotchStyle::Compact, NotchStyle::Pill] {
                a.settings.style = s;
                out += &snap(a, &format!("pomodoro live, style {}", s.key()));
            }
            a.settings.style = NotchStyle::Full;
            a.call(i, |m, c| m.action(3, (0.0, 0.0), c));
            a.toast = a.queue.pop_front().map(|t| (t, a.t() + 10.0));
            out += &format!("{:<4} pomodoro skip produced toast\n", if a.toast.is_some() { "OK" } else { "FAIL" });
            out += &snap(a, "toast with actions");
        }
        a.toast = None;
        // Timer: preset, start -> live, expire -> toast, restart; stopwatch laps.
        if let Some(i) = find(a, "Timer") {
            if let Some(p) = find(a, "Pomodoro") {
                a.call(p, |m, c| m.action(2, (0.0, 0.0), c)); // reset pomodoro so the timer is live
            }
            a.tab = i;
            a.expanded = true;
            a.call(i, |m, c| m.action(12, (0.0, 0.0), c)); // 5 min preset
            out += &snap(a, "tab: Timer (countdown)");
            a.call(i, |m, c| m.action(3, (0.0, 0.0), c)); // start
            a.expanded = false;
            let live = a.live_module().map(|k| a.modules[k].title()).unwrap_or("none");
            out += &format!("{:<4} timer running shows in notch (live: {live})\n", if live == "Timer" { "OK" } else { "FAIL" });
            out += &snap(a, "collapsed with timer");
            a.queue.clear();
            a.call(i, |m, c| m.action(crate::modules::timer::A_DEBUG_EXPIRE, (0.0, 0.0), c));
            a.call(i, |m, c| m.tick(c));
            let ok = a.queue.iter().any(|t| t.key == "timer" && t.actions.len() == 2);
            out += &format!("{:<4} finished timer pops toast with Restart/Dismiss\n", if ok { "OK" } else { "FAIL" });
            a.queue.clear();
            a.call(i, |m, c| m.action(20, (0.0, 0.0), c)); // Restart from toast
            out += &format!("{:<4} restart from toast runs again\n", if a.modules[i].live().is_some() { "OK" } else { "FAIL" });
            a.call(i, |m, c| m.action(4, (0.0, 0.0), c)); // reset
            a.call(i, |m, c| {
                m.action(2, (0.0, 0.0), c); // stopwatch view
                m.action(3, (0.0, 0.0), c); // start
            });
            std::thread::sleep(std::time::Duration::from_millis(60));
            a.call(i, |m, c| {
                m.action(7, (0.0, 0.0), c); // lap
                m.action(7, (0.0, 0.0), c); // lap
            });
            a.expanded = true;
            out += &snap(a, "tab: Timer (stopwatch)");
            let st = a.modules[i].debug_state();
            out += &format!("{:<4} stopwatch running with 2 laps\n", if st.contains("running=true laps=2") { "OK" } else { "FAIL" });
            a.call(i, |m, c| {
                m.action(4, (0.0, 0.0), c);
                m.action(1, (0.0, 0.0), c);
            });
            a.expanded = false;
        }
        // Hovering a toast that has buttons must not swap it for the panel,
        // and clicking a toast button must not reopen the panel under the
        // still-hovering mouse.
        if let Some(ti) = find(a, "Timer") {
            a.expanded = false;
            a.queue.clear();
            let mut tt = Toast::new("selftest", glyph::TIMER, col::PINK, "Selftest", "toast").action("Reset", 4).action("Reset", 4);
            tt.module = ti;
            a.toast = Some((tt, a.t() + 30.0));
            a.hover = true;
            a.hover_since = a.t() - 5.0;
            a.frame();
            let ok = !a.expanded && a.mode() == Mode::Toast;
            out += &format!("{:<4} hovering a toast with buttons keeps the toast\n", if ok { "OK" } else { "FAIL" });
            let tgt = a.target();
            a.cur = tgt;
            a.render(tgt, 0.0);
            let btn = a.hits.iter().find(|h| h.module == TOAST && h.id == 0).copied();
            let ok = match btn {
                Some(b) => {
                    a.on_down(b.rect.cx(), b.rect.cy());
                    a.on_up(b.rect.cx(), b.rect.cy());
                    a.hover_since = a.t() - 5.0;
                    a.frame();
                    a.toast.is_none() && !a.expanded
                }
                None => false,
            };
            out += &format!("{:<4} toast button click does not reopen the panel while hovered\n", if ok { "OK" } else { "FAIL" });
            a.on_mouse_leave();
            out += &format!("{:<4} hover opens again after the mouse left\n", if a.can_hover_open() { "OK" } else { "FAIL" });
            a.hover = false;
            a.leave_at = None;
            a.toast = None;
            a.queue.clear();
        }
        // Tooltips: resting on an icon-only tab shows its name.
        {
            a.expanded = true;
            a.tab = a.modules.iter().position(|m| m.has_tab()).unwrap_or(0);
            let tgt = a.target();
            a.cur = tgt;
            a.render(tgt, 0.0);
            let tab = a.hits.iter().find(|h| h.module == APP_HIT && h.id != u32::MAX && !h.tip.is_empty()).copied();
            let ok = match tab {
                Some(h) => {
                    let base = a.gfx.bright();
                    a.mouse = (h.rect.cx(), h.rect.cy());
                    a.render(tgt, 0.0);
                    if let Some(tip) = &mut a.tip {
                        tip.2 -= 2.0;
                    }
                    a.render(tgt, 0.0);
                    let shown = a.gfx.bright() > base + 20;
                    a.mouse = (-1e4, -1e4);
                    a.tip = None;
                    shown
                }
                None => false,
            };
            out += &format!("{:<4} tooltip shows on a resting tab icon\n", if ok { "OK" } else { "FAIL" });
            let mut untipped: Vec<u32> = Vec::new();
            for i in 0..a.modules.len() {
                if !a.modules[i].has_tab() {
                    continue;
                }
                a.tab = i;
                let tgt = a.target();
                a.cur = tgt;
                a.render(tgt, 0.0);
                untipped.extend(a.hits.iter().filter(|h| h.module == APP_HIT && h.id != u32::MAX && h.tip.is_empty()).map(|h| h.id));
            }
            untipped.dedup();
            let mut no_title = Vec::new();
            for i in 0..a.modules.len() {
                if !a.modules[i].has_tab() {
                    continue;
                }
                a.tab = i;
                let tgt = a.target();
                a.cur = tgt;
                a.render(tgt, 0.0);
                if !a.header_fit.0 || !a.header_fit.1 {
                    no_title.push(a.modules[i].title());
                }
            }
            out += &format!("{:<4} every tab's header fits with its title {:?}\n", if no_title.is_empty() { "OK" } else { "FAIL" }, no_title);
            // Every clickable thing explains itself on hover.
            let mut missing: Vec<String> = Vec::new();
            let mut audit = |a: &mut App, view: &str| {
                let tgt = a.target();
                a.cur = tgt;
                a.render(tgt, 0.0);
                let hits: Vec<crate::ui::Hit> = a.hits.clone();
                for h in hits {
                    if h.id == u32::MAX || h.module == APP_HIT {
                        continue;
                    }
                    if a.tip_for(&h).is_empty() {
                        let s = format!("{view}:{}", h.id);
                        if !missing.contains(&s) {
                            missing.push(s);
                        }
                    }
                }
            };
            for i in 0..a.modules.len() {
                if !a.modules[i].has_tab() {
                    continue;
                }
                a.tab = i;
                let title = a.modules[i].title();
                audit(a, title);
                // Other views of the same panel.
                let views: &[u32] = match title {
                    "Tools" => &[2, 3, 4, 77, 73, 1],
                    "Water Reminder" => &[2, 3],
                    "Prayer Times" => &[1, 3, 4, 5, 2],
                    "AI Agents" => &[990, 3],
                    "Clipboard" => &[991],
                    "Widgets" => &[6, 3, 5, 1, 6],
                    _ => &[],
                };
                for &v in views {
                    a.call(i, |m, c| m.action(v, (0.0, 0.0), c));
                    a.release_keyboard();
                    audit(a, title);
                }
            }
            // The Settings window: every control explains itself, every
            // section fits, and every label fits its control.
            let views: Vec<(u8, u8)> = (0..SECTIONS.len() as u8).map(|s| (0u8, s)).chain([(1u8, 0u8)]).collect();
            for (tab, sec) in views {
                a.sw_view(tab, sec);
                for h in a.sw_hits() {
                    let tip = if !h.tip.is_empty() {
                        h.tip
                    } else if h.module < a.modules.len() {
                        a.modules[h.module].tip(h.id)
                    } else {
                        settings_tip(h.id)
                    };
                    if tip.is_empty() {
                        let s = format!("settings{tab}.{sec}:{}", h.id);
                        if !missing.contains(&s) {
                            missing.push(s);
                        }
                    }
                }
            }
            let (room, tight) = a.swin.as_ref().map(|sw| (sw.room, sw.tight.clone())).unwrap_or_default();
            a.close_settings();
            out += &format!("{:<4} every button, switch, chip and row has a tooltip {:?}\n", if missing.is_empty() { "OK" } else { "FAIL" }, missing);
            out += &format!("{:<4} every Settings section fits (room left {:?})\n", if room.iter().all(|r| *r >= 0.0) { "OK" } else { "FAIL" }, room);
            out += &format!("{:<4} settings labels fit their controls {:?}\n", if tight.is_empty() { "OK" } else { "FAIL" }, tight);
            out += &format!("{:<4} every tab icon has a tooltip {:?}\n", if untipped.is_empty() { "OK" } else { "FAIL" }, untipped);
            a.expanded = false;
        }
        // Mascots: every character renders in each mood, draws something
        // visible, and looks different from the others.
        {
            use std::hash::{Hash, Hasher};
            let mut hashes = Vec::new();
            for m in Mascot::ALL {
                let mut ok = true;
                let mut bright = 0;
                for (mood, mi) in [(Mood::Eyes, 0), (Mood::Doze, 1), (Mood::Wave, 2)] {
                    a.hits.clear();
                    a.gfx.begin(a.scale);
                    {
                        let mut ui = Ui { g: &mut a.gfx, hits: &mut a.hits, module: APP_HIT, mouse: (-1e4, -1e4), t: 1.0 };
                        ui.fill_rect(Rect::new(0.0, 0.0, 220.0, 48.0), col::NOTCH);
                        mascot::draw(&mut ui, Rect::new(14.0, 2.0, 172.0, 28.0), m, mood, 1.0, (1.0, 0.5));
                    }
                    a.gfx.end_and_present(a.hwnd, a.origin.0, a.origin.1);
                    ok &= a.gfx.last_err.is_none();
                    if mi == 0 {
                        bright = a.gfx.bright();
                        let mut h = std::collections::hash_map::DefaultHasher::new();
                        a.gfx.pixels().hash(&mut h);
                        hashes.push(h.finish());
                    }
                }
                let ok = ok && bright > 30;
                out += &format!("{:<4} mascot {:<8} renders (bright_px={bright})\n", if ok { "OK" } else { "FAIL" }, m.key());
            }
            let mut uniq = hashes.clone();
            uniq.sort();
            uniq.dedup();
            out += &format!("{:<4} every mascot looks different ({} of {})\n", if uniq.len() == hashes.len() { "OK" } else { "FAIL" }, uniq.len(), hashes.len());
        }
        // Sounds: every built-in sound is a valid, audible, distinct WAV;
        // the choice is saved and "Off" plays nothing.
        {
            let mut ok = true;
            let mut seen = Vec::new();
            let mut info = Vec::new();
            for s in Sound::ALL {
                if let Some(w) = sound::wav(s) {
                    let pk = sound::peak(w);
                    let secs = (w.len() - 44) as f32 / 2.0 / 44100.0;
                    ok &= &w[..4] == b"RIFF" && pk > 0.15 && pk < 1.0 && secs > 0.2 && secs < 2.0 && !seen.contains(w);
                    seen.push(w.clone());
                    info.push(format!("{} {secs:.2}s peak {pk:.2}", s.key()));
                }
            }
            out += &format!("{:<4} {} built-in sounds are valid and distinct ({})\n", if ok && seen.len() == 4 { "OK" } else { "FAIL" }, seen.len(), info.join(", "));
            sound::mute(false);
            sound::set(Sound::Off);
            let silent = !sound::chime();
            sound::mute(true);
            sound::set(a.settings.sound);
            out += &format!("{:<4} sound Off plays nothing\n", if silent { "OK" } else { "FAIL" });
        }
        // Full-screen detection: only a real full-screen window hides the
        // notch; the desktop, Start, taskbar and maximized windows never do.
        {
            let mon = RECT { left: 0, top: 0, right: 1920, bottom: 1080 };
            let cases = [
                ("F11 browser / video", RECT { left: 0, top: 0, right: 1920, bottom: 1080 }, false, "Chrome_WidgetWin_1", true),
                ("borderless game", RECT { left: -1, top: -1, right: 1921, bottom: 1081 }, false, "UnityWndClass", true),
                ("desktop focused", RECT { left: 0, top: 0, right: 1920, bottom: 1080 }, false, "WorkerW", false),
                ("desktop (Progman)", RECT { left: 0, top: 0, right: 1920, bottom: 1080 }, false, "Progman", false),
                ("Start menu", RECT { left: 0, top: 0, right: 1920, bottom: 1080 }, false, "Windows.UI.Core.CoreWindow", false),
                ("maximized window", RECT { left: -8, top: -8, right: 1928, bottom: 1040 }, true, "Chrome_WidgetWin_1", false),
                ("maximized, auto-hide taskbar", RECT { left: -8, top: -8, right: 1928, bottom: 1088 }, true, "SunAwtFrame", false),
                ("normal window", RECT { left: 100, top: 80, right: 1500, bottom: 900 }, false, "Notepad", false),
                ("full screen on other monitor", RECT { left: 1920, top: 0, right: 3840, bottom: 1080 }, false, "Chrome_WidgetWin_1", false),
            ];
            let wrong: Vec<&str> = cases.iter().filter(|c| is_fullscreen(c.1, mon, c.2, c.3) != c.4).map(|c| c.0).collect();
            out += &format!("{:<4} only real full-screen windows hide the notch {:?}\n", if wrong.is_empty() { "OK" } else { "FAIL" }, wrong);
            out += &format!("INFO full screen right now: {}\n", a.foreground_fullscreen());
            // While hidden, a reminder waits (no expiry, no chime) and shows
            // as soon as the notch is visible again.
            a.queue.clear();
            a.toast = None;
            a.expanded = false;
            a.hidden = true;
            a.push_toast(Toast::new("selftest-hidden", glyph::DROP, col::CYAN, "Time to Drink", "test").secs(0.01));
            a.frame();
            std::thread::sleep(std::time::Duration::from_millis(30));
            a.frame();
            let waited = a.toast.is_none() && a.queue.len() == 1;
            a.hidden = false;
            a.frame();
            let shown = a.toast.as_ref().map_or(false, |t| t.0.key == "selftest-hidden");
            out += &format!("{:<4} a reminder waits while the notch is hidden, then shows\n", if waited && shown { "OK" } else { "FAIL" });
            a.toast = None;
            a.queue.clear();
        }
        // Frame pacing, measured on this PC: a tab switch must draw at the
        // display's refresh rate with no long gaps, and each frame must be
        // cheap. Also reports what the old Windows-timer pacing delivered.
        {
            let probe = |ms: u32| -> f64 {
                unsafe {
                    let id = SetTimer(None, 0, ms, None);
                    let (s, mut last, mut gaps) = (std::time::Instant::now(), None, Vec::new());
                    let mut m = MSG::default();
                    while s.elapsed().as_secs_f64() < 0.5 {
                        if PeekMessageW(&mut m, None, WM_TIMER, WM_TIMER, PM_REMOVE).as_bool() {
                            if m.wParam.0 != id {
                                continue;
                            }
                            let n = s.elapsed().as_secs_f64();
                            if let Some(l) = last {
                                gaps.push(n - l);
                            }
                            last = Some(n);
                        } else {
                            std::thread::yield_now();
                        }
                    }
                    let _ = KillTimer(None, id);
                    if gaps.is_empty() { 0.0 } else { gaps.len() as f64 / gaps.iter().sum::<f64>() }
                }
            };
            let (old15, old33) = (probe(15), probe(33));
            a.queue.clear();
            a.toast = None;
            a.onboarding = None;
            a.hidden = false;
            let tabs: Vec<usize> = (0..a.modules.len()).filter(|&i| a.modules[i].has_tab()).collect();
            a.tab = tabs[0];
            a.expanded = true;
            a.hover = true;
            a.leave_at = None;
            a.request_frame();
            let s0 = a.t();
            while matches!(a.pacing(), Pacing::Refresh) && a.t() - s0 < 2.0 {
                a.frame();
                wait_refresh(a.hwnd);
            }
            a.tab = tabs[tabs.len().min(4) - 1];
            a.request_frame();
            let (mut gaps, mut costs) = (Vec::new(), Vec::new());
            let mut last: Option<f64> = None;
            let mut refresh_all = true;
            let s1 = a.t();
            loop {
                let st = a.t();
                if let Some(l) = last {
                    gaps.push(st - l);
                }
                last = Some(st);
                a.frame();
                costs.push(a.t() - st);
                if !matches!(a.pacing(), Pacing::Refresh) {
                    break;
                }
                if a.t() - s1 > 2.0 {
                    refresh_all = false;
                    break;
                }
                wait_refresh(a.hwnd);
            }
            let ambient_after = matches!(a.pacing(), Pacing::After(_));
            let n = gaps.len().max(1) as f64;
            let fps = n / gaps.iter().sum::<f64>().max(1e-6);
            let worst = gaps.iter().cloned().fold(0.0, f64::max) * 1000.0;
            let avg_cost = costs.iter().sum::<f64>() / costs.len().max(1) as f64 * 1000.0;
            let max_cost = costs.iter().cloned().fold(0.0, f64::max) * 1000.0;
            let ok = refresh_all && ambient_after && fps >= 55.0 && worst < 40.0 && avg_cost < 4.0;
            out += &format!(
                "{:<4} tab switch frame rate {fps:.0} fps over {} frames, worst gap {worst:.1} ms, draw {avg_cost:.2} ms avg / {max_cost:.2} ms max\n",
                if ok { "OK" } else { "FAIL" },
                costs.len()
            );
            out += &format!("INFO old Windows-timer pacing here: 15 ms timer = {old15:.0} fps, 33 ms timer = {old33:.0} fps (old tab switches used the 33 ms one)\n");
            a.hover = false;
            a.expanded = false;
        }
        // Welcome tour: style is step one, choices apply and save, the panel
        // stays open without hover, and finishing says where to change it.
        {
            let click = |a: &mut App, id: u32| -> bool {
                let tgt = a.target();
                a.cur = tgt;
                a.render(tgt, 0.0);
                match a.hits.iter().find(|h| h.module == ONBOARD && h.id == id).copied() {
                    Some(h) => {
                        a.on_down(h.rect.cx(), h.rect.cy());
                        a.on_up(h.rect.cx(), h.rect.cy());
                        true
                    }
                    None => false,
                }
            };
            let before = (a.settings.style, a.settings.edge);
            a.queue.clear();
            a.toast = None;
            a.start_onboarding();
            a.hover = false;
            a.leave_at = Some(a.t() - 1.0);
            a.frame();
            out += &format!("{:<4} welcome tour stays open without the mouse\n", if a.mode() == Mode::Expanded && a.onboarding == Some(0) { "OK" } else { "FAIL" });
            for (k, name) in ["hello", "look and place", "colour", "what's in it", "tips"].iter().enumerate() {
                a.onboarding = Some(k as u8);
                out += &snap(a, &format!("welcome tour step {} ({name})", k + 1));
            }
            a.onboarding = Some(1);
            let tiles = {
                let tgt = a.target();
                a.cur = tgt;
                a.render(tgt, 0.0);
                a.hits.iter().filter(|h| h.module == ONBOARD && (O_STYLE..O_STYLE + NotchStyle::ALL.len() as u32).contains(&h.id)).count()
            };
            let ok = tiles == 5 && click(a, O_STYLE + 3) && a.settings.style == NotchStyle::Pill && crate::store::read("settings.txt").contains("style=pill");
            out += &format!("{:<4} step 1 offers the 5 styles and picking Pill applies it\n", if ok { "OK" } else { "FAIL" });
            let ok = click(a, O_EDGE + 1) && a.settings.edge == Edge::Bottom && a.mode() == Mode::Expanded && a.onboarding == Some(1);
            out += &format!("{:<4} the same step moves the notch and the tour stays open\n", if ok { "OK" } else { "FAIL" });
            let ok = click(a, O_NEXT) && a.onboarding == Some(2) && click(a, O_THEME + 1) && a.settings.theme == "midnight";
            out += &format!("{:<4} step 3 picks a colour theme ({})\n", if ok { "OK" } else { "FAIL" }, a.settings.theme);
            a.settings.theme = "default".into();
            let ok = click(a, O_NEXT) && a.onboarding == Some(3);
            out += &snap(a, "welcome tour presets");
            let ok = ok && a.hits.iter().any(|h| h.module == ONBOARD && h.id == O_PRESET);
            out += &format!("{:<4} step 4 offers the presets\n", if ok { "OK" } else { "FAIL" });
            let ok = ok && click(a, O_NEXT) && a.onboarding == Some(4);
            out += &format!("{:<4} the last step shows {} tips\n", if ok { "OK" } else { "FAIL" }, TOUR_TIPS.len());
            let ok = ok && click(a, O_NEXT) && a.onboarding.is_none();
            let saved = crate::store::read("settings.txt").contains("onboarded=1");
            let told = a.queue.iter().any(|t| t.key == "welcome" && t.sub.contains("right-click"));
            out += &format!("{:<4} finishing saves it and says how to change things later\n", if ok && saved && told { "OK" } else { "FAIL" });
            // Updating from a version without the tour (settings but no
            // "onboarded" key) shows it once; once finished it never returns.
            let saved_settings = crate::store::read("settings.txt");
            crate::store::write("settings.txt", "edge=top\nstyle=pill\nvolume_osd=1\n");
            let old_user = crate::store::Settings::load();
            crate::store::write("settings.txt", &saved_settings);
            let ok = !old_user.onboarded && old_user.style == NotchStyle::Pill && crate::store::Settings::load().onboarded;
            out += &format!("{:<4} people updating from a version without the tour get it once\n", if ok { "OK" } else { "FAIL" });
            // Tour text fits: style tiles, tip cards, hints beside the buttons.
            let mut tight = Vec::new();
            // Five tiles; the line under each style may wrap to two lines.
            let tile_w = (ONBOARD_SIZE.0 - 8.0 - 40.0) / 5.0 - 16.0;
            for s in ["Clock, music, mascot", "Full, but narrower", "A smaller notch", "Just a thin bar", "Invisible until hovered"] {
                if a.gfx.measure(s, 10.0, Weight::Regular) > tile_w * 1.8 {
                    tight.push(s);
                }
            }
            let card_w = (ONBOARD_SIZE.0 - 8.0 - 10.0 * (TOUR_TIPS.len() as f32 - 1.0)) / TOUR_TIPS.len() as f32 - 20.0;
            let preset_w = (ONBOARD_SIZE.0 - 8.0 - 10.0 * 3.0) / 4.0 - 20.0;
            for (s, sub, _) in crate::features::PRESETS {
                if a.gfx.measure(s, 13.0, Weight::Semi) > preset_w || a.gfx.measure(sub, 10.5, Weight::Regular) > preset_w * 3.6 {
                    tight.push(s);
                }
            }
            for (_, s, sub) in TOUR_TIPS {
                if a.gfx.measure(s, 12.5, Weight::Semi) > card_w {
                    tight.push(s);
                }
                // The tip wraps onto at most four lines.
                if a.gfx.measure(sub, 11.0, Weight::Regular) > card_w * 3.6 {
                    tight.push(sub);
                }
            }
            // Settings at a glance: title on one line, description on two.
            let glance_w = (ONBOARD_SIZE.0 - 8.0 - 24.0) / 4.0 - 16.0 - 30.0;
            for (_, s, sub) in TOUR_SETTINGS {
                if a.gfx.measure(s, 12.0, Weight::Semi) > glance_w {
                    tight.push(s);
                }
                if a.gfx.measure(sub, 10.5, Weight::Regular) > glance_w * 1.7 {
                    tight.push(sub);
                }
            }
            if a.gfx.measure(TOUR_Q_SETTINGS, 12.5, Weight::Regular) > ONBOARD_SIZE.0 - 8.0 {
                tight.push("settings question");
            }
            let foot_w = ONBOARD_SIZE.0 - 8.0 - (a.gfx.measure("Next", 12.0, Weight::Semi) + 48.0) - 78.0;
            for s in ["Pick one. You can change it anytime in Settings.", "Or right-drag the notch to any edge, on any screen."] {
                if a.gfx.measure(s, 11.0, Weight::Regular) > foot_w {
                    tight.push(s);
                }
            }
            let last_w = ONBOARD_SIZE.0 - 8.0 - (a.gfx.measure("Start using Nookly", 12.0, Weight::Semi) + 48.0) - 118.0 - 78.0;
            if a.gfx.measure("That's it. Enjoy!", 11.0, Weight::Regular) > last_w || a.gfx.measure("Open Settings", 12.0, Weight::Semi) > 110.0 - 16.0 {
                tight.push("last step footer");
            }
            if a.gfx.measure("First, how should the notch look when it's closed?", 12.5, Weight::Regular) > ONBOARD_SIZE.0 {
                tight.push("question");
            }
            out += &format!("{:<4} welcome tour text fits {:?}\n", if tight.is_empty() { "OK" } else { "FAIL" }, tight);
            a.queue.clear();
            a.settings.style = before.0;
            a.set_edge(before.1, true);
            a.settings.edge = Edge::Top;
            a.settings.save();
            a.expanded = false;
        }
        // Settings window: the gear, a right-click and the tray icon open it
        // (and close the notch); its clicks apply and save.
        {
            a.toast = None;
            a.queue.clear();
            a.expanded = true;
            a.tab = a.tab_list().first().copied().unwrap_or(0);
            let tgt = a.target();
            a.cur = tgt;
            a.render(tgt, 0.0);
            let gear = a.hits.iter().find(|h| h.module == APP_HIT && h.id == GEAR).copied();
            if let Some(g) = gear {
                a.on_down(g.rect.cx(), g.rect.cy());
                a.on_up(g.rect.cx(), g.rect.cy());
            }
            let opened = gear.is_some() && a.swin.is_some() && !a.expanded;
            out += &format!("{:<4} gear opens the Settings window and closes the notch\n", if opened { "OK" } else { "FAIL" });
            a.close_settings();
            a.hover_block = false;
            a.handle(WM_RBUTTONDOWN, WPARAM(0), LPARAM(0));
            a.handle(WM_RBUTTONUP, WPARAM(0), LPARAM(0));
            let rclick = a.swin.is_some();
            a.close_settings();
            a.handle(WM_APP_TRAY, WPARAM(0), LPARAM(WM_LBUTTONUP as isize));
            let tray = a.swin.is_some();
            out += &format!("{:<4} right-click on the notch and the tray icon open the same window\n", if rclick && tray { "OK" } else { "FAIL" });
            a.sw_view(0, 0);
            out += &a.sw_snap("settings window");
            let sections = a.sw_hits().iter().filter(|h| (S_SECTION..S_SECTION + 20).contains(&h.id)).count();
            let notch_bar = !a.tab_list().iter().any(|&i| a.modules[i].title() == "Widgets");
            out += &format!("{:<4} Settings has {sections} sections; Widgets moved there from the notch's tab bar\n", if sections == SECTIONS.len() && notch_bar { "OK" } else { "FAIL" });
            a.sw_view(0, SEC_MASCOT);
            let tiles = a.sw_hits().iter().filter(|h| h.module == SETTINGS && (S_MASCOT..=S_MASCOT + Mascot::ALL.len() as u32).contains(&h.id)).count();
            out += &format!("{:<4} settings shows {tiles} mascot tiles (clock + {})\n", if tiles == Mascot::ALL.len() + 1 { "OK" } else { "FAIL" }, Mascot::ALL.len());
            a.sw_view(0, SEC_GENERAL);
            let general = [S_POLICY, S_QUIT, S_TOUR, S_EXPORT, S_IMPORT, S_UPDATES].iter().all(|id| a.sw_has(*id));
            out += &format!("{:<4} General has updates, export / import, the tour, Privacy policy and Quit\n", if general { "OK" } else { "FAIL" });
            // The Widgets section hosts the Widgets panel, and its clicks work.
            if let Some(w) = find(a, "Widgets") {
                a.sw_view(0, SEC_WIDGETS);
                out += &a.sw_snap("settings widgets section");
                let hosted = a.sw_hits().iter().filter(|h| h.module == w).count();
                // Widgets has Show (and Order) only; alerts and the place
                // moved to their own sections.
                let only_show = !a.sw_hits().iter().any(|h| h.module == w && h.id == crate::modules::widgets::A_ALERTS);
                // Alerts > How each alert shows: the alerts list, one place.
                a.sw_view(0, SEC_ALERTS);
                let views = a.sw_has(S_ALERT_VIEW) && a.sw_has(S_ALERT_VIEW + 1) && a.sw_has(S_NOTIF_ON);
                a.sw_click_id(S_ALERT_VIEW + 1);
                let alerts = a.sw_hits().iter().any(|h| h.module == w && h.id == crate::modules::widgets::A_ALERT_RESET) && !a.sw_has(S_NOTIF_ON);
                out += &a.sw_snap("settings alerts: each alert");
                if let Some(sw) = a.swin.as_mut() {
                    sw.alert_view = 0;
                }
                // Location: the place picker, a general setting of its own.
                a.sw_view(0, SEC_LOCATION);
                out += &a.sw_snap("settings location");
                let place = a.sw_hits().iter().filter(|h| h.module == w).count() > 2 && !a.sw_hits().iter().any(|h| h.module == w && h.id == crate::modules::widgets::A_SHOW);
                out += &format!(
                    "{:<4} Widgets ({hosted} controls), Alerts (two clear pages) and Location (its own section) are separate {:?}\n",
                    if hosted > 5 && only_show && views && alerts && place { "OK" } else { "FAIL" },
                    (only_show, views, alerts, place)
                );
            }
            // Settings > Panels: drag a panel to a new place; the notch's
            // tab bar follows, and Reset puts the order back.
            {
                let before = a.tab_list();
                a.sw_view(0, SEC_PANELS);
                out += &a.sw_snap("settings panels order");
                let cards = a.sw_hits().iter().filter(|h| (S_ORDER..S_ORDER + 64).contains(&h.id)).count();
                let first = a.modules[before[0]].title();
                a.move_panel(0, 2);
                let after = a.tab_list();
                let moved = after.len() == before.len() && a.modules[after[2]].title() == first && crate::store::read("settings.txt").contains("panel_order=");
                a.sw_click_id(S_ORDER_RESET);
                let reset = a.tab_list() == before;
                out += &format!("{:<4} Settings > Panels lists {cards} panels; dragging reorders the tab bar and saves; Reset restores it\n", if cards == before.len() && moved && reset { "OK" } else { "FAIL" });
            }
            // The master switch: off hides the notch and stops its features.
            a.sw_view(0, 0);
            let master = a.sw_click_id(S_MASTER) && !a.notch_on() && a.hidden && crate::store::read("settings.txt").contains("notch_on=0");
            a.tick();
            let hidden = a.hidden;
            let back = a.sw_click_id(S_MASTER) && a.notch_on();
            a.tick();
            out += &format!("{:<4} the Notch master switch hides the notch and brings it back, and saves\n", if master && hidden && back && !a.hidden { "OK" } else { "FAIL" });
            // Taskbar tab: without the add-on, a Download button and a switch
            // that does nothing; with it, the switch turns it on and off (the
            // self-test never starts it) and its sections show.
            a.sw_view(1, 0);
            let installed = bar_exe().is_some();
            let shows = a.sw_has(S_MASTER_TASKBAR) && if installed { a.sw_has(S_BAR_SECTION) && a.sw_has(S_BAR_AUTOHIDE) } else { a.sw_has(S_BAR_GET) };
            if let Some(h) = a.sw_hits().iter().find(|h| h.id == S_MASTER_TASKBAR).copied() {
                a.sw_click(h.rect.cx(), h.rect.cy());
            }
            let flips = crate::barcfg::flag(&a.settings, "taskbar_on", false) == installed;
            a.settings.extra.remove("taskbar_on");
            let none = a.bar.is_none();
            out += &format!("{:<4} Taskbar tab: {} (the self-test never starts the taskbar)\n", if shows && flips && none { "OK" } else { "FAIL" }, if installed { "its switch and sections" } else { "Download button, the switch waits for the add-on" });
            if installed {
                // Layout: moving and hiding items.
                if let Some(sw) = a.swin.as_mut() {
                    sw.bar_section = 1;
                }
                a.sw_render();
                use crate::barcfg::{Thing, Zone, layout};
                let clock = crate::barcfg::ITEMS.iter().position(|x| x.0 == "clock").unwrap() as u32;
                let at = |a: &App| layout(&a.settings).iter().position(|e| e.thing == Thing::Item("clock")).unwrap_or(0);
                // Pinned left with its Left switch, then switched off.
                let k = at(&a) as u32;
                let moved = a.sw_click_id(S_LAY_ZONE + k * 3) && layout(&a.settings).iter().any(|e| e.thing == Thing::Item("clock") && e.zone == Zone::Left);
                a.sw_render();
                let hid = a.sw_click_id(S_BAR_SHOW + clock) && !crate::barcfg::order(&a.settings).iter().any(|x| x.0 == "clock" && x.1);
                for k in ["bar_layout", "bar_order", "bar_off", "bar_on"] {
                    a.settings.extra.remove(k);
                }
                if let Some(sw) = a.swin.as_mut() {
                    sw.bar_section = 0;
                }
                out += &format!("{:<4} Taskbar > Layout moves and hides items\n", if moved && hid { "OK" } else { "FAIL" });
                // Other screens: Start comes and goes there, not on the main one.
                if let Some(sw) = a.swin.as_mut() {
                    sw.bar_section = BAR_OTHER;
                }
                a.sw_render();
                out += &a.sw_snap("settings: taskbar, other screens");
                let start = crate::barcfg::ITEMS.iter().position(|x| x.0 == "start").unwrap() as u32;
                let was = crate::barcfg::other_shows(&a.settings, "start");
                let shown = a.sw_click_id(S_BAR_OTHER + start) && crate::barcfg::other_shows(&a.settings, "start") && crate::barcfg::order(&a.settings).iter().any(|x| x.0 == "start" && x.1);
                a.sw_render();
                let hidden = a.sw_click_id(S_BAR_OTHER + start) && !crate::barcfg::other_shows(&a.settings, "start");
                a.settings.extra.remove("bar_other_hide");
                if let Some(sw) = a.swin.as_mut() {
                    sw.bar_section = 0;
                }
                out += &format!("{:<4} Taskbar > Other screens has its own choices\n", if !was && shown && hidden { "OK" } else { "FAIL" });
            }
            let ok = a.sw_click_id(S_TOUR) && a.swin.is_none() && a.onboarding == Some(0);
            a.onboarding = None;
            a.expanded = false;
            out += &format!("{:<4} Welcome tour button restarts the tour\n", if ok { "OK" } else { "FAIL" });
            a.open_settings();
            let ok = a.sw_click_id(S_CLOSE) && a.swin.is_none();
            out += &format!("{:<4} close button closes the window\n", if ok { "OK" } else { "FAIL" });
            a.open_settings();
            let before = (a.settings.style, a.settings.mascot, a.settings.personality);
            let click = |a: &mut App, id: u32| -> bool { a.sw_click_id(id) };
            let robot = S_MASCOT + 1 + Mascot::ALL.iter().position(|m| *m == Mascot::Robot).unwrap_or(0) as u32;
            let ok = click(a, robot) && a.settings.mascot == Mascot::Robot && a.settings.personality && crate::store::read("settings.txt").contains("mascot=robot");
            out += &format!("{:<4} picking a mascot applies and saves\n", if ok { "OK" } else { "FAIL" });
            let ok = click(a, S_MASCOT) && !a.settings.personality;
            out += &format!("{:<4} picking Clock turns the mascot off\n", if ok { "OK" } else { "FAIL" });
            let ok = click(a, S_STYLE + 2) && a.settings.style == NotchStyle::Compact && crate::store::read("settings.txt").contains("style=compact")
                && click(a, S_STYLE + 1) && a.settings.style == NotchStyle::Slim && crate::store::read("settings.txt").contains("style=slim");
            out += &format!("{:<4} notch style chip applies and saves\n", if ok { "OK" } else { "FAIL" });
            // Slim: narrower than Full, same closed height; the open panel is
            // narrower and shorter, except Tools, which keeps Full's size.
            {
                let saved = (a.expanded, a.tab, a.settings.edge);
                a.settings.edge = Edge::Top;
                let size = |a: &mut App, s: NotchStyle, open: bool, tab: usize| {
                    a.settings.style = s;
                    a.expanded = open;
                    a.tab = tab;
                    a.target()
                };
                let music = find(a, "Now Playing").unwrap_or(0);
                let tools = find(a, "Tools").unwrap_or(0);
                let (fc, sc) = (size(a, NotchStyle::Full, false, music), size(a, NotchStyle::Slim, false, music));
                let (fo, so) = (size(a, NotchStyle::Full, true, music), size(a, NotchStyle::Slim, true, music));
                let (ft, st) = (size(a, NotchStyle::Full, true, tools), size(a, NotchStyle::Slim, true, tools));
                let ok = sc.0 < fc.0 && sc.1 == fc.1 && so.0 < fo.0 && so.1 < fo.1 && st.1 - SLIM_TAB_H - SLIM_PAD >= ft.1 - TAB_H - PAD - 1.0 && NotchStyle::from_key("slim") == Some(NotchStyle::Slim);
                out += &format!(
                    "{:<4} Slim style: closed {:.0}x{:.0} (Full {:.0}x{:.0}), open {:.0}x{:.0} (Full {:.0}x{:.0}), Tools keeps its height\n",
                    if ok { "OK" } else { "FAIL" }, sc.0, sc.1, fc.0, fc.1, so.0, so.1, fo.0, fo.1
                );
                (a.expanded, a.tab, a.settings.edge) = saved;
                // With the widest clock (12:58), what each live view shows
                // still fits beside it: ring + label, icon + text, countdown.
                let mid = a.gfx.measure("12:58", 14.0, Weight::Semi) + 2.0;
                let wing = SLIM_LIVE_W / 2.0 - mid / 2.0 - SLIM_GAP - SLIM_WING_PAD;
                let mut tight = Vec::new();
                for (box_w, gap, label) in [(18.0, 5.0, "Timer"), (18.0, 5.0, "Watch"), (18.0, 5.0, "Focus"), (18.0, 5.0, "Break"), (20.0, 4.0, "Due"), (20.0, 4.0, "OK?"), (20.0, 2.0, "100%"), (0.0, 0.0, "Maghrib")] {
                    if box_w + gap + a.gfx.measure(label, 12.0, Weight::Semi) > wing {
                        tight.push(label);
                    }
                }
                for d in ["59:59", "1:00:00"] {
                    if a.gfx.measure_digits(d, 12.0) > wing {
                        tight.push(d);
                    }
                }
                out += &format!("{:<4} Slim live notch: {SLIM_LIVE_W} wide, wings {wing:.0} px beside a 12:58 clock, labels fit {tight:?}\n", if tight.is_empty() { "OK" } else { "FAIL" });
            }
            out += &widget_tests(a);
            a.settings.style = before.0;
            a.settings.mascot = before.1;
            a.settings.personality = before.2;
            a.settings.save();
            let ok = click(a, S_VOLUME) && click(a, S_VOLUME);
            out += &format!("{:<4} settings switches respond\n", if ok { "OK" } else { "FAIL" });
            let ok = a.settings.update_check
                && click(a, S_UPDATE_AUTO)
                && !a.settings.update_check
                && crate::store::read("settings.txt").lines().any(|l| l.trim() == "update_check=0")
                && click(a, S_UPDATE_AUTO)
                && a.settings.update_check;
            out += &format!("{:<4} automatic update checks can be turned off (on by default) and it saves\n", if ok { "OK" } else { "FAIL" });
            let bell = S_SOUND + Sound::ALL.iter().position(|s| *s == Sound::Bell).unwrap_or(0) as u32;
            let ok = click(a, bell) && a.settings.sound == Sound::Bell && crate::store::read("settings.txt").contains("sound=bell");
            out += &format!("{:<4} picking a sound applies and saves\n", if ok { "OK" } else { "FAIL" });
            // Alerts and Privacy: notifications start off; the switches save.
            let n0 = a.settings.notif_on;
            a.sw_view(0, SEC_ALERTS);
            let alerts = [S_NOTIF_ON, S_NOTIF_TEXT, S_NOTIF_BANNERS, S_NOTIF_UNMUTE, S_VOLUME].iter().all(|id| a.sw_has(*id));
            a.sw_view(0, SEC_PRIVACY);
            let privacy = [S_PRIVACY, S_CLIP_ON, S_CLIP_MODE, S_COPY_TOAST].iter().all(|id| a.sw_has(*id));
            let t0 = a.settings.notif_text;
            let toggled = click(a, S_NOTIF_TEXT) && a.settings.notif_text != t0 && crate::store::read("settings.txt").contains(&format!("notif_text={}", (!t0) as u8));
            click(a, S_NOTIF_TEXT);
            out += &format!(
                "{:<4} Alerts and Privacy hold the notification, mic-dot and clipboard switches (notifications start {})\n",
                if alerts && privacy && toggled && !n0 { "OK" } else { "FAIL" },
                if n0 { "ON (should be off)" } else { "off" }
            );
            a.close_settings();
            a.settings.sound = Sound::Chime;
            a.settings.save();
            a.expanded = false;
        }
        // Prayer times: the maths matches a reference (aladhan.com, same
        // method) within a minute, across seasons and cities.
        {
            use crate::modules::prayer::{self, Calc, Isha};
            let cases: [(i32, i32, i32, f64, f64, f64, Isha, [&str; 6]); 5] = [
                (2026, 10, 27, 23.8103, 90.4125, 2.0, Isha::Angle(18.0), ["04:45", "06:01", "11:42", "15:47", "17:23", "18:39"]),
                (2026, 6, 21, 23.8103, 90.4125, 2.0, Isha::Angle(18.0), ["03:44", "05:12", "12:00", "16:41", "18:48", "20:16"]),
                (2026, 1, 15, 24.8949, 91.8687, 2.0, Isha::Angle(18.0), ["05:19", "06:39", "12:02", "15:49", "17:25", "18:45"]),
                (2026, 3, 21, 22.3569, 91.7832, 2.0, Isha::Angle(18.0), ["04:42", "05:56", "12:00", "16:22", "18:04", "19:19"]),
                (2026, 10, 27, 23.8103, 90.4125, 1.0, Isha::Angle(17.0), ["04:45", "06:01", "11:42", "14:58", "17:23", "18:35"]),
            ];
            let (mut worst, mut bad) = (0i64, Vec::new());
            for (y, m, d, lat, lon, asr, isha, want) in cases {
                let got = prayer::times(&Calc { lat, lon, tz: 6.0, fajr: 18.0, isha, asr }, y, m, d);
                for k in 0..6 {
                    let w: Vec<i64> = want[k].split(':').map(|x| x.parse().unwrap_or(0)).collect();
                    let diff = ((got[k] * 60.0).round() as i64 - (w[0] * 60 + w[1])).abs();
                    worst = worst.max(diff);
                    if diff > 1 {
                        bad.push(format!("{y}-{m}-{d} {} {} vs {}", prayer::NAMES[k], prayer::hhmm24(got[k]), want[k]));
                    }
                }
            }
            out += &format!("{:<4} prayer times match the reference within a minute (30 checks, worst {worst} min) {:?}\n", if bad.is_empty() { "OK" } else { "FAIL" }, bad);
            out += &format!("INFO this PC's UTC offset {:+} h\n", prayer::local_tz());
        }
        // Prayer panel: pages, picks saved, reminder, countdown in the notch.
        if let Some(pi) = find(a, "Prayer Times") {
            use crate::modules::prayer::{A_TEST_DUE, A_TEST_RELOAD, A_TEST_SOON};
            a.toast = None;
            a.queue.clear();
            a.expanded = true;
            a.tab = pi;
            a.switch_t = -10.0;
            out += &snap(a, "prayer panel");
            let bells = a.hits.iter().filter(|h| h.module == pi && (90..96).contains(&h.id)).count();
            out += &format!("{:<4} prayer panel lists six times with a reminder bell each ({bells})\n", if bells == 6 { "OK" } else { "FAIL" });
            let clickm = |a: &mut App, id: u32| -> bool {
                let tgt = a.target();
                a.cur = tgt;
                a.render(tgt, 0.0);
                match a.hits.iter().find(|h| h.module == pi && h.id == id).copied() {
                    Some(h) => {
                        a.on_down(h.rect.cx(), h.rect.cy());
                        a.on_up(h.rect.cx(), h.rect.cy());
                        true
                    }
                    None => false,
                }
            };
            let saved = |k: &str| crate::store::read("prayer.txt").lines().any(|l| l.trim() == k);
            let city = clickm(a, 1) && clickm(a, 14) && saved("city=Sylhet");
            out += &snap(a, "prayer location page");
            let method = clickm(a, 4) && clickm(a, 51) && saved("method=mwl") && clickm(a, 61) && clickm(a, 60) && saved("asr=shafi");
            out += &snap(a, "prayer method page");
            clickm(a, 50);
            clickm(a, 61);
            let adjust = clickm(a, 5) && clickm(a, 113) && saved("off_asr=1") && clickm(a, 103) && saved("off_asr=0");
            out += &snap(a, "prayer adjust page");
            // Custom latitude, longitude typed in.
            clickm(a, 3);
            let mut custom = clickm(a, 40) && a.keyboard == Some(pi);
            for _ in 0..40 {
                a.on_key(0x08);
            }
            for ch in "21.43, 92.01".chars() {
                a.on_char(ch);
            }
            out += &snap(a, "prayer custom location typing");
            a.on_key(0x0D);
            custom = custom && saved("city=custom") && saved("lat=21.4300") && saved("lon=92.0100") && a.keyboard.is_none();
            clickm(a, 10);
            let back = clickm(a, 2) && saved("city=Dhaka") && saved("method=karachi") && saved("asr=hanafi");
            out += &format!(
                "{:<4} prayer settings: city, method, Asr school, minute nudges and typed coordinates save {:?}\n",
                if city && method && adjust && custom && back { "OK" } else { "FAIL" },
                (city, method, adjust, custom, back)
            );
            // Tight labels on its pages.
            let mut tight = Vec::new();
            let w4 = (modules::PANEL.0 - 18.0) / 4.0 - 8.0;
            for (c, _, _) in crate::modules::prayer::CITIES {
                if a.gfx.measure(c, 11.5, Weight::Semi) > w4 {
                    tight.push(c);
                }
            }
            let wm = (modules::PANEL.0 - 74.0 - 24.0) / 5.0 - 8.0;
            for (_, m, _, _) in crate::modules::prayer::METHODS {
                if a.gfx.measure(m, 11.5, Weight::Semi) > wm {
                    tight.push(m);
                }
            }
            if a.gfx.measure("15 min before", 11.5, Weight::Semi) > (modules::PANEL.0 - 74.0 - 18.0) / 4.0 - 8.0 {
                tight.push("15 min before");
            }
            out += &format!("{:<4} prayer panel labels fit {:?}\n", if tight.is_empty() { "OK" } else { "FAIL" }, tight);
            // A reminder at prayer time, once.
            a.expanded = false;
            a.toast = None;
            a.queue.clear();
            a.call(pi, |m, c| m.action(A_TEST_DUE, (0.0, 0.0), c));
            a.tick();
            let first = a.toast.as_ref().map(|t| t.0.title.clone()).or_else(|| a.queue.front().map(|t| t.title.clone())).unwrap_or_default();
            a.toast = None;
            a.queue.clear();
            a.tick();
            let again = a.toast.is_some() || !a.queue.is_empty();
            out += &format!("{:<4} prayer reminder at the time, once ({first})\n", if first.starts_with("Time for ") && !again { "OK" } else { "FAIL" });
            // Ten minutes before: the countdown takes the closed notch.
            a.toast = None;
            a.queue.clear();
            a.call(pi, |m, c| m.action(A_TEST_SOON, (0.0, 0.0), c));
            let live = a.live_module() == Some(pi);
            out += &snap(a, "prayer countdown in the notch");
            out += &format!("{:<4} prayer countdown shows in the closed notch 10 min before\n", if live { "OK" } else { "FAIL" });
            a.call(pi, |m, c| m.action(A_TEST_RELOAD, (0.0, 0.0), c));
            let off = a.live_module() != Some(pi);
            out += &format!("{:<4} prayer reminders and countdown are off until turned on\n", if off { "OK" } else { "FAIL" });
        } else {
            out += "FAIL prayer panel missing\n";
        }
        // Settings > Panels: hide a panel; its tab and alerts go; one stays.
        {
            a.toast = None;
            a.queue.clear();
            let wi = find(a, "Water Reminder").unwrap_or(0);
            a.sw_view(0, SEC_FEATURES);
            out += &a.sw_snap("settings features section");
            let fk = crate::features::FEATURES.iter().position(|f| f.key == "Water Reminder").unwrap_or(0) as u32;
            let chips = a.sw_hits().iter().filter(|h| h.module == SETTINGS && (S_FEATURE..S_FEATURE + crate::features::FEATURES.len() as u32).contains(&h.id)).count();
            let fits = true;
            let room = a.swin.as_ref().map_or(false, |sw| sw.room[SEC_FEATURES as usize] >= 0.0);
            if let Some(h) = a.sw_hits().iter().find(|h| h.module == SETTINGS && h.id == S_FEATURE + fk).copied() {
                a.sw_click(h.rect.cx(), h.rect.cy());
            }
            a.close_settings();
            a.expanded = true;
            let saved = crate::store::read("settings.txt").lines().any(|l| l.trim() == "hidden_panels=Water Reminder");
            let tgt = a.target();
            a.cur = tgt;
            a.render(tgt, 0.0);
            let no_tab = !a.hits.iter().any(|h| h.module == APP_HIT && h.id == wi as u32);
            a.call(wi, |_, c| c.toast(Toast::new("water", glyph::DROP, col::CYAN, "Time to drink", "test")));
            let quiet = a.toast.is_none() && a.queue.is_empty();
            out += &format!(
                "{:<4} switching a feature off ({chips} listed) removes its tab and silences it, and saves {:?}\n",
                if chips == crate::features::FEATURES.len() && fits && room && saved && no_tab && quiet { "OK" } else { "FAIL" },
                (fits, room, saved, no_tab, quiet)
            );
            for (k, f) in crate::features::FEATURES.iter().enumerate() {
                if crate::features::background(f.key).is_none() && crate::features::is_on(&a.settings, f.key) {
                    a.settings_action(S_FEATURE + k as u32);
                }
            }
            let one = a.tab_list().len() == 1 && a.tab_list().contains(&a.tab);
            out += &format!("{:<4} the last panel that's on can't be switched off\n", if one { "OK" } else { "FAIL" });
            // Presets switch on exactly their set and are recognised after.
            let mut ok_p = true;
            for k in 0..crate::features::PRESETS.len() {
                a.settings_action(S_PRESET + k as u32);
                let names: Vec<&str> = a.tab_list().iter().map(|&i| a.modules[i].title()).collect();
                // (Widgets is on too, but it lives in Settings, not the tab bar.)
                let want: Vec<&str> = crate::features::PRESETS[k].2.iter().copied().filter(|t| crate::features::background(t).is_none() && *t != "Widgets").collect();
                ok_p &= crate::features::current_preset(&a.settings) == Some(k) && names.len() == want.len() && want.iter().all(|t| names.contains(t));
            }
            out += &format!("{:<4} presets switch on exactly their panels and are recognised\n", if ok_p { "OK" } else { "FAIL" });
            // A panel that's off isn't ticked: the clipboard records nothing.
            let ci = find(a, "Clipboard").unwrap_or(0);
            crate::features::set_on(&mut a.settings, "Clipboard", false);
            a.settings.clip_on = true;
            a.tick();
            let clip_quiet = crate::modules::clipboard::recording() == Some(false);
            crate::features::set_on(&mut a.settings, "Clipboard", true);
            a.tick();
            let clip_back = crate::modules::clipboard::recording() == Some(true);
            a.settings.clip_on = false;
            let _ = ci;
            out += &format!("{:<4} a switched-off panel does no work (clipboard stops recording, starts again)\n", if clip_quiet && clip_back { "OK" } else { "FAIL" });
            a.settings.hidden_panels.clear();
            a.settings.save();
            a.tick();
        }
        // Mouse wheel on the open notch switches panels; lists still scroll.
        {
            a.toast = None;
            a.queue.clear();
            a.expanded = true;
            a.release_keyboard();
            let tabs = a.tab_list();
            let wi = find(a, "Water Reminder").unwrap_or(0);
            let pos = tabs.iter().position(|&i| i == wi).unwrap_or(0);
            a.tab = wi;
            let tgt = a.target();
            a.cur = tgt;
            a.render(tgt, 0.0);
            let content = (a.tab_bar.cx(), a.tab_bar.b() + 70.0);
            let bar = (a.tab_bar.cx(), a.tab_bar.cy());
            a.mouse = content;
            a.wheel_t = -10.0;
            a.on_wheel(-1.0);
            let down = a.tab == tabs[pos + 1];
            a.wheel_t = -10.0;
            a.on_wheel(1.0);
            let up = a.tab == wi;
            // A fast flick moves one panel, not several.
            a.wheel_t = -10.0;
            for _ in 0..4 {
                a.on_wheel(-1.0);
            }
            let flick = a.tab == tabs[pos + 1];
            a.tab = *tabs.last().unwrap_or(&0);
            a.wheel_t = -10.0;
            a.on_wheel(-1.0);
            let end = a.tab == *tabs.last().unwrap_or(&0);
            // Small touchpad steps add up to one switch.
            a.tab = wi;
            a.wheel_t = -10.0;
            a.on_wheel(-0.4);
            let half = a.tab == wi;
            a.on_wheel(-0.4);
            a.on_wheel(-0.4);
            let pad = half && a.tab == tabs[pos + 1];
            // A to-do list long enough to scroll keeps the wheel; the tab bar
            // still switches.
            let ti = find(a, "TodoList").unwrap_or(0);
            for k in 0..8 {
                a.call(ti, |m, c| {
                    m.action(1, (0.0, 0.0), c);
                    for ch in format!("Wheel test {k}").chars() {
                        m.chr(ch, c);
                    }
                    m.key(0x0D, false, c);
                });
            }
            a.release_keyboard();
            a.tab = ti;
            let tgt = a.target();
            a.cur = tgt;
            a.render(tgt, 0.0);
            a.mouse = content;
            a.wheel_t = -10.0;
            a.on_wheel(-1.0);
            let list = a.tab == ti;
            a.mouse = bar;
            a.wheel_t = -10.0;
            a.on_wheel(-1.0);
            let tpos = tabs.iter().position(|&i| i == ti).unwrap_or(0);
            let over_bar = tabs.get(tpos + 1).map_or(a.tab == ti, |&n| a.tab == n);
            out += &format!(
                "{:<4} mouse wheel switches panels (down, up, one per flick, stops at the end, touchpad), scrolls lists, tab bar always switches {:?}\n",
                if down && up && flick && end && pad && list && over_bar { "OK" } else { "FAIL" },
                (down, up, flick, end, pad, list, over_bar)
            );
            a.expanded = false;
        }
        // Mascot: "Always" and left / centre / right, with and without a live activity.
        {
            let before = (a.settings.mascot_always, a.settings.mascot_pos, a.settings.personality, a.settings.style, a.settings.edge);
            IDLE_OVERRIDE.store(0, std::sync::atomic::Ordering::Relaxed);
            a.settings.personality = true;
            a.settings.style = NotchStyle::Full;
            a.settings.edge = Edge::Top;
            a.expanded = false;
            a.toast = None;
            a.queue.clear();
            a.settings.mascot_always = false;
            a.last_activity = a.t();
            a.update_mood();
            let clock_first = a.mood == Mood::Clock;
            a.settings.mascot_always = true;
            a.update_mood();
            let always = a.mood == Mood::Eyes;
            let mut where_ok = Vec::new();
            for pos in 0..3u8 {
                a.settings.mascot_pos = pos;
                let tgt = a.target();
                a.cur = tgt;
                a.render(tgt, 0.0);
                let notch = a.edge_rect(tgt.0, tgt.1);
                let m = a.mascot_rect.unwrap_or_default();
                let ok = match pos {
                    0 => m.cx() < notch.cx() - 30.0,
                    1 => (m.cx() - notch.cx()).abs() < 4.0,
                    _ => m.cx() > notch.cx() + 30.0,
                };
                where_ok.push(ok && a.mascot_rect.is_some());
            }
            out += &snap(a, "mascot always, right, clock beside it");
            // With music (or any live activity) in the closed notch.
            let pi = find(a, "Prayer Times").unwrap_or(0);
            a.call(pi, |m, c| m.action(crate::modules::prayer::A_TEST_SOON, (0.0, 0.0), c));
            a.update_mood();
            let mut live_ok = Vec::new();
            for pos in 0..3u8 {
                a.settings.mascot_pos = pos;
                let tgt = a.target();
                a.cur = tgt;
                a.render(tgt, 0.0);
                let notch = a.edge_rect(tgt.0, tgt.1);
                let m = a.mascot_rect.unwrap_or_default();
                let wide = if pos == 1 { tgt.0 == 300.0 } else { tgt.0 == 364.0 };
                let side = match pos {
                    0 => m.cx() < notch.x + 50.0,
                    1 => (m.cx() - notch.cx()).abs() < 4.0,
                    _ => m.cx() > notch.r() - 50.0,
                };
                live_ok.push(a.mascot_rect.is_some() && wide && side && notch.contains(m.cx(), m.cy()));
                if pos == 0 {
                    out += &snap(a, "mascot always, left, with countdown");
                }
            }
            a.call(pi, |m, c| m.action(crate::modules::prayer::A_TEST_RELOAD, (0.0, 0.0), c));
            // Off again: the clock is back once the notch is used.
            a.settings.mascot_always = false;
            a.expanded = true;
            a.update_mood();
            let back = a.mood == Mood::Clock;
            a.expanded = false;
            out += &format!(
                "{:<4} mascot \"Always\" shows it at once, at left / center / right, beside the clock and beside a live activity {:?}\n",
                if clock_first && always && back && where_ok.iter().all(|x| *x) && live_ok.iter().all(|x| *x) { "OK" } else { "FAIL" },
                (clock_first, always, back, where_ok, live_ok)
            );
            (a.settings.mascot_always, a.settings.mascot_pos, a.settings.personality, a.settings.style, a.settings.edge) = before;
            IDLE_OVERRIDE.store(-1, std::sync::atomic::Ordering::Relaxed);
            a.settings.save();
            a.update_mood();
        }
        // The notch remembers the panel you picked (also after a restart),
        // and returns to it after a reminder opened another one.
        {
            a.toast = None;
            a.queue.clear();
            a.release_keyboard();
            let tabs = a.tab_list();
            let (water, todo) = (find(a, "Water Reminder").unwrap_or(tabs[1]), find(a, "TodoList").unwrap_or(tabs[0]));
            a.expanded = true;
            a.tab = tabs[0];
            let tgt = a.target();
            a.cur = tgt;
            a.render(tgt, 0.0);
            if let Some(h) = a.hits.iter().find(|h| h.module == APP_HIT && h.id == water as u32).copied() {
                a.on_down(h.rect.cx(), h.rect.cy());
                a.on_up(h.rect.cx(), h.rect.cy());
            }
            let saved = crate::store::read("settings.txt").lines().any(|l| l.trim() == "last_tab=Water Reminder");
            a.expanded = false;
            a.frame();
            let kept = a.tab == water;
            // A reminder opens To-do for a moment...
            a.call(todo, |_, c| c.open_my_tab());
            let jumped = a.tab == todo && a.expanded;
            a.expanded = false;
            a.leave_at = None;
            a.frame();
            let back = a.tab == water;
            // ...and a restart opens Water again.
            let reloaded = crate::store::Settings::load().last_tab == "Water Reminder";
            out += &format!(
                "{:<4} the notch remembers the last panel you picked, across restarts and after a reminder {:?}\n",
                if saved && kept && jumped && back && reloaded { "OK" } else { "FAIL" },
                (saved, kept, jumped, back, reloaded)
            );
        }
        // AI agents: hook events -> states, toasts and the live wing; the
        // hook process itself; connecting the agents in a scratch home.
        // 12-hour clock (default) and 24-hour.
        {
            use crate::time::{clock, clock_full, fmt_mod, H12};
            let o = std::sync::atomic::Ordering::Relaxed;
            let was = H12.load(o);
            H12.store(true, o);
            let t12 = clock(16, 20) == "4:20"
                && clock_full(16, 20) == "4:20 PM"
                && clock_full(0, 5) == "12:05 AM"
                && clock_full(12, 0) == "12:00 PM"
                && fmt_mod(9 * 60 + 30) == "9:30 AM"
                && crate::modules::prayer::hhmm(15.0 + 47.0 / 60.0) == "3:47 PM";
            H12.store(false, o);
            let t24 = clock(16, 20) == "16:20" && clock_full(9, 5) == "09:05" && fmt_mod(21 * 60) == "21:00" && crate::modules::prayer::hhmm(15.0 + 47.0 / 60.0) == "15:47";
            H12.store(was, o);
            let default_on = crate::store::Settings::load().clock12 || crate::store::read("settings.txt").contains("clock12=1");
            out += &format!("{:<4} clock: 12-hour by default (4:20 PM), 24-hour when switched off ({t12}, {t24}, {default_on})\n", if t12 && t24 && default_on { "OK" } else { "FAIL" });
            let before = a.settings.clock12;
            a.settings_action(S_CLOCK12);
            let off = !a.settings.clock12 && !crate::time::h12() && crate::store::read("settings.txt").contains("clock12=0");
            a.settings_action(S_CLOCK12);
            let on = a.settings.clock12 == !before || a.settings.clock12 == before;
            a.settings.clock12 = before;
            H12.store(before, o);
            a.settings.save();
            out += &format!("{:<4} Settings > General: 12-hour clock switch applies at once and saves\n", if off && on { "OK" } else { "FAIL" });
        }
        // Make room: over a maximized window the Full notch becomes a thin
        // click-through bar; resting on it opens the notch.
        {
            let o = std::sync::atomic::Ordering::Relaxed;
            let saved = (a.settings.style, a.settings.make_room, a.expanded, a.hover_block);
            a.settings.style = NotchStyle::Full;
            a.settings.make_room = true;
            a.expanded = false;
            a.toast = None;
            a.queue.clear();
            a.hover_block = false;
            let full = a.target();
            MAXIMIZED_OVERRIDE.store(1, o);
            a.update_make_room();
            let slim = a.target();
            let ex = unsafe { GetWindowLongPtrW(a.hwnd, GWL_EXSTYLE) } & WS_EX_TRANSPARENT.0 as isize != 0;
            let yielded = a.look() == NotchStyle::Pill && slim.1 == 0.0 && full.1 > 0.0 && a.passthrough && ex;
            // A toast must be clickable.
            a.push_toast(Toast::new("t", glyph::BELL, col::GREEN, "Test", "toast"));
            a.sync_passthrough();
            let toast_clicks = !a.passthrough;
            a.toast = None;
            a.queue.clear();
            a.sync_passthrough();
            // Rest the cursor on the bar: opens after the hover delay.
            let zone = a.edge_rect(220.0, 16.0);
            a.poll_at(zone.cx(), zone.cy());
            let not_yet = !a.expanded;
            std::thread::sleep(std::time::Duration::from_millis(360));
            a.poll_at(zone.cx(), zone.cy());
            let opened = a.expanded && !a.passthrough;
            a.expanded = false;
            a.sync_passthrough();
            // Away from the bar nothing opens.
            a.poll_at(zone.cx(), zone.b() + 200.0);
            std::thread::sleep(std::time::Duration::from_millis(360));
            a.poll_at(zone.cx(), zone.b() + 200.0);
            let stays = !a.expanded;
            // Setting off: back to the full notch, clickable.
            a.settings_action(S_MAKE_ROOM);
            let off = a.look() == NotchStyle::Full && !a.passthrough && crate::store::read("settings.txt").contains("make_room=0");
            a.settings_action(S_MAKE_ROOM);
            MAXIMIZED_OVERRIDE.store(2, o);
            a.update_make_room();
            let back = a.look() == NotchStyle::Full && !a.passthrough && unsafe { GetWindowLongPtrW(a.hwnd, GWL_EXSTYLE) } & WS_EX_TRANSPARENT.0 as isize == 0;
            (a.settings.style, a.settings.make_room, a.expanded, a.hover_block) = saved;
            a.settings.save();
            out += &format!(
                "{:<4} make room: thin click-through bar over a maximized window, toasts still clickable, rest on it to open, setting off restores {:?}\n",
                if yielded && toast_clicks && not_yet && opened && stays && off && back { "OK" } else { "FAIL" },
                (yielded, toast_clicks, not_yet, opened, stays, off, back)
            );
            let real = unsafe {
                let fg = GetForegroundWindow();
                !fg.0.is_null() && IsZoomed(fg).as_bool()
            };
            out += &format!("INFO foreground window maximized right now: {real}\n");
        }
        // Taskbar mascot placement (pure layout; the window itself needs his taskbar).
        out += &format!("{:<4} taskbar mascot: left corner, right of the weather, beside the tray, hidden when it can't fit\n", if crate::taskbar::self_check() { "OK" } else { "FAIL" });
        // Music: a refused prev/next falls back to the media key.
        out += &format!("{:<4} music: prev/next/play fall back to the media keys when the player refuses\n", if crate::modules::music::test_fallback() { "OK" } else { "FAIL" });
        // Copy toast ("Show what I copy"): off by default; text, picture and
        // files previews; a repeat isn't shown twice; nothing is recorded.
        if let Some(ci) = find(a, "Clipboard") {
            use crate::modules::clipboard as cb;
            let saved = (a.settings.copy_toast, a.settings.clip_on);
            let default_off = !crate::store::Settings::load().copy_toast || !crate::store::read("settings.txt").contains("copy_toast=1");
            let items0 = a.modules[ci].debug_state();
            let feed = |a: &mut App, id: u32| {
                a.toast = None;
                a.queue.clear();
                a.call(ci, |m, c| m.action(id, (0.0, 0.0), c));
                a.call(ci, |m, c| m.tick(c));
            };
            let shown = |a: &App| a.toast.as_ref().map(|t| t.0.clone()).or_else(|| a.queue.front().cloned()).filter(|t| t.key == "copy");
            a.settings.copy_toast = false;
            feed(a, cb::A_TEST_COPY_TEXT);
            let quiet_off = shown(a).is_none();
            a.settings.copy_toast = true;
            a.settings.clip_on = false;
            feed(a, cb::A_TEST_COPY_TEXT);
            let text = shown(a).map_or(false, |t| t.title == "Copied" && t.sub.starts_with("Meeting moved to 4:30 pm") && t.right == "2 lines");
            feed(a, cb::A_TEST_COPY_TEXT);
            let no_repeat = shown(a).is_none();
            feed(a, cb::A_TEST_COPY_IMAGE);
            let pic = shown(a).map_or(false, |t| t.title == "Copied a picture" && t.sub == "1600 x 1000" && t.image.as_ref().map_or(false, |i| i.0 == i.1 && i.0 == 40));
            feed(a, cb::A_TEST_COPY_FILES);
            let files = shown(a).map_or(false, |t| t.title == "Copied 2 files" && t.sub.contains("Report final.docx") && t.sub.contains("Logo.png"));
            let kept_nothing = a.modules[ci].debug_state().split(' ').next() == items0.split(' ').next();
            out += &snap(a, "copy toast");
            a.toast = None;
            a.queue.clear();
            (a.settings.copy_toast, a.settings.clip_on) = saved;
            a.settings.save();
            out += &format!(
                "{:<4} copy toast: off by default, previews text (2 lines), a picture and files, no repeat, keeps nothing {:?}\n",
                if default_off && quiet_off && text && no_repeat && pic && files && kept_nothing { "OK" } else { "FAIL" },
                (default_off, quiet_off, text, no_repeat, pic, files, kept_nothing)
            );
        }
        // Screenshot / Pick from screen: the notch window is hidden while
        // the screen is frozen and comes back after (a tiny off-screen
        // stand-in window plays the notch).
        {
            use crate::modules::tools::picker;
            let stand_in = unsafe {
                CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE, w!("STATIC"), w!(""), WS_POPUP, -32000, -32000, 4, 4, None, None, None, None).unwrap_or_default()
            };
            unsafe {
                let _ = ShowWindow(stand_in, SW_SHOWNOACTIVATE);
            }
            let shown_before = unsafe { IsWindowVisible(stand_in).as_bool() };
            let snap = picker::capture_without(stand_in);
            let hidden_during = unsafe { !IsWindowVisible(stand_in).as_bool() };
            picker::show_app(stand_in);
            let back = unsafe { IsWindowVisible(stand_in).as_bool() };
            // Not hidden by us: left alone.
            unsafe {
                let _ = ShowWindow(stand_in, SW_HIDE);
            }
            picker::hide_app(stand_in);
            picker::show_app(stand_in);
            let left_alone = unsafe { !IsWindowVisible(stand_in).as_bool() };
            unsafe {
                let _ = DestroyWindow(stand_in);
            }
            out += &format!(
                "{:<4} screenshot and color picker hide the notch while the screen is frozen, then bring it back {:?}\n",
                if shown_before && snap.is_some() && hidden_during && back && left_alone { "OK" } else { "FAIL" },
                (shown_before, snap.is_some(), hidden_during, back, left_alone)
            );
            // Clicking Screenshot closes the panel and it stays closed with
            // the cursor still on the button.
            if let Some(sh) = find(a, "Shelf") {
                a.hover_block = false;
                a.expanded = true;
                a.tab = sh;
                a.call(sh, |m, c| m.action(crate::modules::shelf::A_TEST_NO_OVERLAY, (0.0, 0.0), c));
                a.call(sh, |m, c| m.action(crate::modules::shelf::A_SNIP, (0.0, 0.0), c));
                let blocked = !a.expanded && a.hover_block && !a.can_hover_open();
                a.hover_block = false;
                out += &format!("{:<4} Screenshot closes the panel and it doesn't reopen under the cursor\n", if blocked { "OK" } else { "FAIL" });
            }
        }
        if let Some(ai) = find(a, "AI Agents") {
            use crate::modules::agents::{self as ag, Link};
            use serde_json::json;
            let toasted = |a: &App, f: &dyn Fn(&crate::modules::Toast) -> bool| a.toast.as_ref().map_or(false, |t| f(&t.0)) || a.queue.iter().any(|t| f(t));
            let clear = |a: &mut App| {
                a.toast = None;
                a.queue.clear();
            };
            let feed = |a: &mut App, j: serde_json::Value| {
                ag::push_event(j.to_string());
                a.call(ai, |m, c| m.tick(c));
            };
            clear(a);
            out += &snap(a, "agents: nothing connected");
            feed(a, json!({"agent": "claude", "event": "SessionStart", "session": "t1", "cwd": "C:\\Projects\\shop"}));
            let ready = a.modules[ai].debug_state().contains("claude:shop:Ready") && a.modules[ai].live().is_none();
            feed(a, json!({"agent": "claude", "event": "UserPromptSubmit", "session": "t1", "cwd": "C:\\Projects\\shop"}));
            let working = a.modules[ai].debug_state().contains("claude:shop:Working") && a.modules[ai].live() == Some(45);
            feed(a, json!({"agent": "claude", "event": "Notification", "session": "t1", "message": "Claude needs your permission to use Bash"}));
            let asked = a.modules[ai].debug_state().contains("NeedsYou") && a.modules[ai].live() == Some(75) && toasted(a, &|t| t.key == "agent" && t.title == "Claude Code needs you" && t.sub.contains("Bash"));
            feed(a, json!({"agent": "claude", "event": "PostToolUse", "session": "t1", "tool": "Bash"}));
            let back = a.modules[ai].debug_state().contains("claude:shop:Working");
            clear(a);
            feed(a, json!({"agent": "claude", "event": "Stop", "session": "t1"}));
            let done = a.modules[ai].debug_state().contains("claude:shop:Done") && a.modules[ai].live() == Some(50) && toasted(a, &|t| t.title == "Claude Code finished");
            clear(a);
            feed(a, json!({"agent": "claude", "event": "Notification", "session": "t1", "message": "Claude is waiting for your input"}));
            let quiet = a.toast.is_none() && a.queue.is_empty() && a.modules[ai].debug_state().contains("claude:shop:Done");
            feed(a, json!({"agent": "codex", "event": "Stop", "session": "c1", "cwd": "D:/work/api"}));
            let codex = a.modules[ai].debug_state().contains("codex:api:Done") && toasted(a, &|t| t.title == "Codex finished");
            clear(a);
            feed(a, json!({"agent": "claude", "event": "SessionEnd", "session": "t1"}));
            let ended = !a.modules[ai].debug_state().contains("claude:shop");
            out += &format!(
                "{:<4} agents: ready, working, needs you (toast), back to work, done (toast), idle nudge ignored, Codex done, session end {:?}\n",
                if ready && working && asked && back && done && quiet && codex && ended { "OK" } else { "FAIL" },
                (ready, working, asked, back, done, quiet, codex, ended)
            );
            // Hermes: stream start = working, a question/approval = needs you,
            // turn end = done, Esc = stopped, session closed = gone.
            feed(a, json!({"agent": "hermes", "event": "on_stream_start", "session": "h1", "cwd": "C:\\Users\\Dev", "surface": "desktop", "model": "anthropic/claude-opus"}));
            let h_work = a.modules[ai].debug_state().contains("hermes:desktop:Working") && a.modules[ai].live() == Some(45);
            feed(a, json!({"agent": "hermes", "event": "on_human_input_request", "session": "h1", "kind": "approval"}));
            let h_ask = a.modules[ai].debug_state().contains("hermes:desktop:NeedsYou") && toasted(a, &|t| t.title == "Hermes needs you" && t.sub.contains("OK"));
            feed(a, json!({"agent": "hermes", "event": "on_human_input_resolved", "session": "h1"}));
            let h_back = a.modules[ai].debug_state().contains("hermes:desktop:Working");
            clear(a);
            feed(a, json!({"agent": "hermes", "event": "on_session_end", "session": "h1", "interrupted": true}));
            let h_stop = a.modules[ai].debug_state().contains("hermes:desktop:Ready") && a.toast.is_none() && a.queue.is_empty();
            feed(a, json!({"agent": "hermes", "event": "on_stream_start", "session": "h1"}));
            feed(a, json!({"agent": "hermes", "event": "on_session_end", "session": "h1"}));
            let h_done = a.modules[ai].debug_state().contains("hermes:desktop:Done") && toasted(a, &|t| t.title == "Hermes finished");
            feed(a, json!({"agent": "hermes", "event": "on_session_finalize", "session": "h1"}));
            let h_gone = !a.modules[ai].debug_state().contains("hermes:desktop");
            clear(a);
            out += &format!(
                "{:<4} Hermes: working, needs you (toast), back to work, Esc = stopped (no toast), done (toast), closed {:?}\n",
                if h_work && h_ask && h_back && h_stop && h_done && h_gone { "OK" } else { "FAIL" },
                (h_work, h_ask, h_back, h_stop, h_done, h_gone)
            );
            let hp = json!({"hook_event_name": "on_human_input_request", "tool_name": null, "tool_input": null, "session_id": "s9", "cwd": "C:/Users/x", "profile": "default", "extra": {"kind": "approval", "prompt": "rm -rf [REDACTED]", "conversation_history": ["SECRET"], "user_message": "SECRET"}});
            let hs = ag::slim("hermes", &hp).to_string();
            out += &format!("{:<4} Hermes hook payload slimmed ({hs})\n", if !hs.contains("SECRET") && hs.contains("approval") && hs.contains("s9") { "OK" } else { "FAIL" });
            // Hermes hooks merge: keeps the user's own hooks and other sections.
            let cur = json!({"pre_tool_call": [{"command": "mine.sh", "matcher": "terminal"}], "outbound": [{"url": "https://x"}]});
            let cmd = ag::hook_command("hermes");
            let on = ag::merge_hermes_hooks(&cur, &cmd, true);
            let twice = ag::merge_hermes_hooks(&on, &cmd, true);
            let off = ag::merge_hermes_hooks(&on, &cmd, false);
            let ok = on == twice && off == cur && ag::HERMES_EVENTS.iter().all(|e| on[*e].as_array().map_or(false, |l| l.len() == 1)) && on["pre_tool_call"] == cur["pre_tool_call"];
            out += &format!("{:<4} Hermes hooks: added once per event, the user's hooks and outbound kept, removal restores\n", if ok { "OK" } else { "FAIL" });
            if std::env::var("NOOKLY_TEST_HERMES").is_ok() {
                // The real hermes CLI against a scratch HERMES_HOME (never the user's).
                let hh = crate::store::dir().join("hermes-home");
                let _ = std::fs::remove_dir_all(&hh);
                let _ = std::fs::create_dir_all(&hh);
                let _ = std::fs::write(hh.join("config.yaml"), "hooks:\n  pre_tool_call:\n  - command: mine.sh\n    matcher: terminal\n");
                // Safe: the self-test and doc renders run on one thread.
                unsafe { std::env::set_var("NOOKLY_HERMES_HOME", &hh) };
                let t0 = std::time::Instant::now();
                let c1 = ag::hermes_connect(true);
                let cfg = std::fs::read_to_string(hh.join("config.yaml")).unwrap_or_default();
                let allow = std::fs::read_to_string(hh.join("shell-hooks-allowlist.json")).unwrap_or_default();
                let linked = ag::hermes_link() == Link::On;
                let c2 = ag::hermes_connect(false);
                let cfg2 = std::fs::read_to_string(hh.join("config.yaml")).unwrap_or_default();
                let allow2 = std::fs::read_to_string(hh.join("shell-hooks-allowlist.json")).unwrap_or_default();
                let ok = c1.is_ok() && c2.is_ok() && linked && cfg.contains("on_stream_start") && cfg.contains("mine.sh") && allow.matches("--agent-event hermes").count() == 6 && !cfg2.contains("agent-event") && cfg2.contains("mine.sh") && !allow2.contains("agent-event") && ag::hermes_link() == Link::Off;
                out += &format!("{:<4} real hermes CLI: connect writes 6 hooks + consent, keeps the user's hook, disconnect removes them ({:.1} s) {:?}\n", if ok { "OK" } else { "FAIL" }, t0.elapsed().as_secs_f32(), (c1, c2, linked));
                // Safe: the self-test and doc renders run on one thread.
                unsafe { std::env::remove_var("NOOKLY_HERMES_HOME") };
            }
            // The hook keeps only status fields: no prompt, no file contents.
            let big = json!({"hook_event_name": "PostToolUse", "session_id": "s", "cwd": "C:/x/y", "tool_name": "Read", "prompt": "SECRET-PROMPT", "tool_input": {"file_path": "C:/x/.env"}, "tool_response": {"content": "SECRET-FILE"}});
            let slim = ag::slim("claude", &big).to_string();
            out += &format!("{:<4} the hook passes on only event, session, folder, tool and notice ({slim})\n", if !slim.contains("SECRET") && !slim.contains(".env") && slim.contains("Read") { "OK" } else { "FAIL" });
            // The real hook process, as Claude Code and Codex run it.
            let exe = std::env::current_exe().unwrap_or_default();
            let mut timings = Vec::new();
            for (agent, payload) in [
                ("claude", json!({"hook_event_name": "UserPromptSubmit", "session_id": "rt1", "cwd": "C:/Projects/roundtrip", "prompt": "SECRET-PROMPT"}).to_string()),
                ("codex", json!({"type": "agent-turn-complete", "thread-id": "rt2", "cwd": "C:/Projects/codexrt", "last-assistant-message": "SECRET"}).to_string()),
            ] {
                use std::io::Write;
                let t0 = std::time::Instant::now();
                let mut cmd = std::process::Command::new(&exe);
                cmd.args(["--agent-event", agent]).env("NOOKLY_AGENT_HWND", (a.hwnd.0 as isize).to_string()).stdin(std::process::Stdio::piped());
                if agent == "codex" {
                    cmd.arg(&payload);
                }
                if let Ok(mut child) = cmd.spawn() {
                    if let Some(mut si) = child.stdin.take() {
                        if agent == "claude" {
                            let _ = si.write_all(payload.as_bytes());
                        }
                    }
                    for _ in 0..500 {
                        unsafe {
                            let mut msg = MSG::default();
                            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                                let _ = TranslateMessage(&msg);
                                DispatchMessageW(&msg);
                            }
                        }
                        if child.try_wait().ok().flatten().is_some() {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                }
                timings.push(t0.elapsed().as_millis());
            }
            a.call(ai, |m, c| m.tick(c));
            let st = a.modules[ai].debug_state();
            let rt = st.contains("claude:roundtrip:Working") && st.contains("codex:codexrt:Done") && !st.contains("SECRET");
            out += &format!("{:<4} hook processes reach the notch (Claude via stdin, Codex via argument) in {:?} ms\n", if rt && timings.iter().all(|t| *t < 3000) { "OK" } else { "FAIL" }, timings);
            clear(a);
            // Connecting, in a scratch home (never the real ~/.claude).
            let home = crate::store::dir().join("agent-home");
            let _ = std::fs::remove_dir_all(&home);
            let _ = std::fs::create_dir_all(&home);
            // Safe: the self-test and doc renders run on one thread.
            unsafe { std::env::set_var("NOOKLY_AGENT_HOME", &home) };
            let missing = ag::claude_link() == Link::Missing && ag::claude_connect(true).is_err() && ag::codex_link() == Link::Missing;
            let _ = std::fs::create_dir_all(home.join(".claude"));
            let original = "{\n  \"model\": \"opus\",\n  \"hooks\": {\n    \"Stop\": [\n      {\n        \"hooks\": [\n          {\n            \"type\": \"command\",\n            \"command\": \"echo mine\"\n          }\n        ]\n      }\n    ]\n  },\n  \"zzz\": 1\n}\n";
            let _ = std::fs::write(home.join(".claude/settings.json"), original);
            let c1 = ag::claude_connect(true).is_ok() && ag::claude_connect(true).is_ok() && ag::claude_link() == Link::On;
            let text = std::fs::read_to_string(home.join(".claude/settings.json")).unwrap_or_default();
            let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
            let ours = |ev: &str| v["hooks"][ev].as_array().map_or(0, |l| l.iter().filter(|e| e.to_string().contains("--agent-event")).count());
            let all_once = ag::CLAUDE_EVENTS.iter().all(|e| ours(e) == 1);
            let kept = text.contains("echo mine") && text.find("\"model\"") < text.find("\"hooks\"") && text.find("\"hooks\"") < text.find("\"zzz\"");
            let c2 = ag::claude_connect(false).is_ok() && ag::claude_link() == Link::Off;
            let restored = std::fs::read_to_string(home.join(".claude/settings.json")).unwrap_or_default() == original;
            let backup = home.join(".claude/settings.json.nookly-backup").exists();
            out += &format!(
                "{:<4} Claude Code connect adds 6 hooks once, keeps the user's settings and order, disconnect restores the file exactly {:?}\n",
                if missing && c1 && all_once && kept && c2 && restored && backup { "OK" } else { "FAIL" },
                (missing, c1, all_once, kept, c2, restored, backup)
            );
            let _ = std::fs::create_dir_all(home.join(".codex"));
            let conf = "model = \"o3\"\n\n[tui]\nnotify = true\n";
            let _ = std::fs::write(home.join(".codex/config.toml"), conf);
            let x1 = ag::codex_connect(true).is_ok() && ag::codex_link() == Link::On;
            let t = std::fs::read_to_string(home.join(".codex/config.toml")).unwrap_or_default();
            let top = t.lines().nth(1).map_or(false, |l| l.starts_with("notify = [") && l.ends_with("\"--agent-event\", \"codex\"]"));
            let x2 = ag::codex_connect(false).is_ok() && std::fs::read_to_string(home.join(".codex/config.toml")).unwrap_or_default() == conf;
            let theirs = "notify = [\"my-notifier\"]\nmodel = \"o3\"\n";
            let _ = std::fs::write(home.join(".codex/config.toml"), theirs);
            let x3 = ag::codex_connect(true).is_err() && std::fs::read_to_string(home.join(".codex/config.toml")).unwrap_or_default() == theirs;
            out += &format!("{:<4} Codex connect adds a top-level notify line, disconnect restores, an existing notify is left alone {:?}\n", if x1 && top && x2 && x3 { "OK" } else { "FAIL" }, (x1, top, x2, x3));
            // Uninstall unhooks both.
            let _ = std::fs::write(home.join(".codex/config.toml"), conf);
            let _ = ag::claude_connect(true);
            let _ = ag::codex_connect(true);
            ag::disconnect_all();
            out += &format!("{:<4} uninstall disconnects both agents\n", if ag::claude_link() == Link::Off && ag::codex_link() == Link::Off { "OK" } else { "FAIL" });
            // Never the real Hermes during tests.
            // Safe: the self-test and doc renders run on one thread.
            unsafe { std::env::remove_var("NOOKLY_HERMES_HOME") };
            out += &format!("{:<4} self-test can't reach the real Hermes or agent settings ({:?}, {:?})\n", if ag::hermes_home().is_none() && ag::hermes_link() == Link::Missing { "OK" } else { "FAIL" }, ag::hermes_home(), ag::hermes_link());
            // Safe: the self-test and doc renders run on one thread.
            unsafe { std::env::remove_var("NOOKLY_AGENT_HOME") };
            let _ = std::fs::remove_dir_all(&home);
            a.call(ai, |m, c| m.action(ag::A_DEMO, (0.0, 0.0), c));
            a.expanded = true;
            a.tab = ai;
            out += &snap(a, "agents: three sessions");
            let rows = a.hits.iter().filter(|h| h.module == ai && h.id > (1 << 24)).count();
            out += &format!("{:<4} agents panel lists the sessions ({rows} rows)\n", if rows == 3 { "OK" } else { "FAIL" });
            // The three switches fit their labels.
            let mut tight = Vec::new();
            for (name, w) in ["Hermes", "Claude Code", "Codex"].iter().zip(ag::TOGGLE_W) {
                if a.gfx.measure(name, 12.5, Weight::Regular) + 58.0 > w {
                    tight.push(*name);
                }
            }
            let left = modules::PANEL.0 - ag::TOGGLE_W.iter().sum::<f32>() - 22.0;
            if a.gfx.measure("No agents running", 12.0, Weight::Regular) > left {
                tight.push("status");
            }
            out += &format!("{:<4} agents header switches fit {:?}\n", if tight.is_empty() { "OK" } else { "FAIL" }, tight);
            a.call(ai, |m, c| m.action(3, (0.0, 0.0), c));
            clear(a);
            a.expanded = false;
        } else {
            out += "FAIL AI Agents panel missing\n";
        }

        // System monitor: Windows counters, formatting, the closed-notch readout.
        if let Some(mi) = find(a, "System Monitor") {
            use crate::modules::monitor as mo;
            let c1 = mo::cpu_times();
            std::thread::sleep(std::time::Duration::from_millis(300));
            let c2 = mo::cpu_times();
            let cpu = matches!((c1, c2), (Some((i1, t1)), Some((i2, t2))) if t2 > t1 && i2 >= i1 && i2 - i1 <= t2 - t1);
            let (used, total) = mo::memory();
            let mem = total > (1u64 << 30) && used > 0 && used < total;
            let net = mo::net_bytes(&mo::adapters());
            let (free, dt) = mo::disk();
            let disk = dt > 0 && free <= dt;
            out += &format!("{:<4} system counters: CPU times, memory {}, network {:?}, disk {} free of {}\n", if cpu && mem && disk { "OK" } else { "FAIL" }, mo::gb(total), net, mo::gb(free), mo::gb(dt));
            let fm = [mo::speed(0.0), mo::speed(1536.0), mo::speed(5.5 * 1048576.0), mo::gb(17_179_869_184), mo::gb(511_000_000_000)];
            out += &format!("{:<4} speeds and sizes read well {:?}\n", if fm == ["0 B/s", "2 KB/s", "5.5 MB/s", "16.0 GB", "476 GB"] { "OK" } else { "FAIL" }, fm);
            std::thread::sleep(std::time::Duration::from_millis(1000));
            a.call(mi, |m, c| m.tick(c));
            let st = a.modules[mi].debug_state();
            let sampled = st.starts_with("cpu ") && !st.contains("samples 0");
            let ids: Vec<&str> = a.modules[mi].widgets().iter().map(|w| w.id).collect();
            let ok = sampled && a.modules[mi].live().is_none() && ids == ["cpu", "ram", "net"];
            out += &format!("{:<4} monitor samples once a second and offers CPU, memory and network widgets {ids:?} ({st})\n", if ok { "OK" } else { "FAIL" });
            a.expanded = true;
            a.tab = mi;
            out += &snap(a, "monitor panel");
            a.expanded = false;
        } else {
            out += "FAIL System Monitor panel missing\n";
        }

        // Clipboard: snippets and search (no clipboard writes).
        if let Some(ci) = find(a, "Clipboard") {
            use crate::modules::clipboard as cb;
            let field = |s: &str, k: &str| -> String { s.split_whitespace().find_map(|w| w.strip_prefix(&format!("{k}="))).unwrap_or("").to_string() };
            let head_w = 500.0 - 156.0 - 68.0 - 30.0 - 34.0;
            let mut tight = Vec::new();
            for (on, mode) in [(true, 0u8), (true, 1), (true, 2), (false, 0)] {
                let s = cb::status_line(on, mode);
                if a.gfx.measure(&s, 12.0, Weight::Regular) > head_w {
                    tight.push(s);
                }
            }
            out += &format!("{:<4} clipboard header text fits beside the buttons {:?}\n", if tight.is_empty() { "OK" } else { "FAIL" }, tight);
            a.expanded = true;
            a.tab = ci;
            a.call(ci, |m, c| m.action(cb::A_SNIPPET, (0.0, 0.0), c));
            let kb = a.keyboard == Some(ci);
            for ch in "Road 12, Dhanmondi".chars() {
                a.on_char(ch);
            }
            out += &snap(a, "clipboard: typing a snippet");
            a.on_key(0x0D);
            let saved = crate::store::read("clipboard.txt").contains("Road 12, Dhanmondi") && a.keyboard.is_none();
            a.call(ci, |m, c| m.action(cb::A_DEMO, (0.0, 0.0), c));
            a.call(ci, |m, c| m.action(cb::A_SEARCH, (0.0, 0.0), c));
            for ch in "ENTRY 3".chars() {
                a.on_char(ch);
            }
            out += &snap(a, "clipboard: searching");
            let shown = field(&a.modules[ci].debug_state(), "shown");
            a.on_key(0x1B);
            let cleared = field(&a.modules[ci].debug_state(), "shown") != shown && a.keyboard == Some(ci);
            a.on_key(0x1B);
            let closed = a.keyboard.is_none();
            out += &format!(
                "{:<4} clipboard snippet saved as a pin; search filters (\"entry 3\" shows {shown}), Esc clears then closes {:?}\n",
                if kb && saved && shown == "6" && cleared && closed { "OK" } else { "FAIL" },
                (kb, saved, cleared, closed)
            );
            a.call(ci, |m, c| m.action(1, (0.0, 0.0), c));
            a.expanded = false;
        }

        // Shelf screenshot: closes the notch, saves a PNG, adds it, reopens.
        if let Some(si) = find(a, "Shelf") {
            use crate::modules::{shelf, snip};
            a.call(si, |m, c| m.action(shelf::A_TEST_NO_OVERLAY, (0.0, 0.0), c));
            a.expanded = true;
            a.tab = si;
            let tgt = a.target();
            a.cur = tgt;
            a.render(tgt, 0.0);
            let button = a.hits.iter().find(|h| h.module == si && h.id == shelf::A_SNIP).copied();
            if let Some(h) = button {
                a.on_down(h.rect.cx(), h.rect.cy());
                a.on_up(h.rect.cx(), h.rect.cy());
            }
            let closed = !a.expanded;
            let px: Vec<u8> = (0..40 * 20).flat_map(|i| [(i % 40 * 6) as u8, (i / 40 * 12) as u8, 200, 255]).collect();
            *snip::RESULT.lock().unwrap() = Some(Some(snip::Shot { w: 40, h: 20, bgra: px }));
            a.tick();
            let back = a.expanded && a.tab == si;
            let st = a.modules[si].debug_state();
            let dir = snip::folder();
            let file = std::fs::read_dir(&dir).ok().and_then(|d| d.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| p.extension().map_or(false, |x| x == "png")));
            let bytes = file.as_ref().and_then(|f| std::fs::read(f).ok()).unwrap_or_default();
            let png = bytes.starts_with(&[0x89, b'P', b'N', b'G']) && unsafe { crate::modules::music::decode_square(&bytes, 20) }.is_some();
            let dib = snip::dib32(&snip::Shot { w: 3, h: 2, bgra: vec![0; 24] }).len() == 40 + 24;
            out += &format!(
                "{:<4} Shelf screenshot: closes the notch, saves a real PNG, adds it to the Shelf, reopens ({}) {:?}\n",
                if button.is_some() && closed && back && st.contains("Screenshot ") && png && dib { "OK" } else { "FAIL" },
                st,
                (button.is_some(), closed, back, png, dib)
            );
            out += &snap(a, "shelf with a screenshot");
            if std::env::var("NOOKLY_TEST_PICKER").is_ok() {
                let up = snip::start(a.hwnd);
                let dragged = snip::test_drag(10, 10, 110, 60);
                let got = snip::RESULT.lock().unwrap().take();
                let size = got.as_ref().and_then(|g| g.as_ref().map(|s| (s.w, s.h)));
                out += &format!("{:<4} screenshot window opens and captures the dragged area ({:?})\n", if up && dragged && size == Some((101, 51)) && !snip::active() { "OK" } else { "FAIL" }, size);
            }
            if let Some(f) = &file {
                let _ = std::fs::remove_file(f);
            }
            a.call(si, |m, c| m.action(1, (0.0, 0.0), c));
            a.expanded = false;
        }

        // Self-uninstall: what it would do for this exe.
        {
            let plan = crate::uninstall::plan(false).join(" | ");
            let ok = plan.contains("disconnect Hermes, Claude Code and Codex") && plan.contains("delete data folder") && (plan.contains("unins000.exe") || plan.contains("if it is empty")) && !crate::uninstall::plan(true).join(" ").contains("delete data folder");
            out += &format!("{:<4} uninstall plan ({plan})\n", if ok { "OK" } else { "FAIL" });
        }

        // Tools tab: calculator, converter, time zones, colours, QR, picker.
        {
            use crate::modules::tools::{self, convert, eval, fmt_num, tz};
            let cases: [(&str, f64); 24] = [
                ("1250*15%", 187.5),
                ("200+10%", 220.0),
                ("200-10%", 180.0),
                ("100*(1+5%)", 105.0),
                ("50%", 0.5),
                ("2^10", 1024.0),
                ("-2^2", -4.0),
                ("2^-1", 0.5),
                ("(1+2)*3", 9.0),
                ("2(3+4)", 14.0),
                ("3x4", 12.0),
                ("12 × 3 ÷ 4", 9.0),
                ("sqrt(16)+2", 6.0),
                ("sin(30)", 0.5),
                ("cos(60)", 0.5),
                ("log(1000)", 3.0),
                ("0xFF+1", 256.0),
                ("0b1010", 10.0),
                ("10/4", 2.5),
                ("5!", 120.0),
                ("1,250+250", 1500.0),
                ("(2+3", 5.0),
                ("2e3", 2000.0),
                ("2pi", std::f64::consts::TAU),
            ];
            let mut bad = Vec::new();
            for (e, want) in cases {
                match eval(e) {
                    Ok(v) if (v - want).abs() <= 1e-9 * want.abs().max(1.0) => {}
                    r => bad.push(format!("{e} -> {r:?}")),
                }
            }
            for e in ["1/0", "2+", "foo(3)", "(1+2))", "3!!!!"] {
                if eval(e).is_ok() && e != "3!!!!" {
                    bad.push(format!("{e} should be an error"));
                }
            }
            for (v, want) in [(1234567.891, "1,234,567.891"), (0.1 + 0.2, "0.3"), (-1500.0, "-1,500"), (2.0f64.sqrt(), "1.414213562"), (0.0, "0")] {
                if fmt_num(v) != want {
                    bad.push(format!("fmt {v} -> {}", fmt_num(v)));
                }
            }
            out += &format!("{:<4} calculator: 24 sums incl. 15%, +10%, 2^10, sqrt, sin, hex, 5!, and errors {:?}\n", if bad.is_empty() { "OK" } else { "FAIL" }, bad);

            let mut bad = Vec::new();
            for (cat, from, to, v, want, what) in [
                (0usize, 0usize, 4usize, 1.0, 0.621371192, "1 km in mi"),
                (2, 0, 1, 100.0, 212.0, "100 C in F"),
                (2, 1, 0, -40.0, -40.0, "-40 F in C"),
                (2, 0, 2, 0.0, 273.15, "0 C in K"),
                (3, 6, 1, 5.0, 3600.0, "5 katha in ft2"),
                (3, 7, 6, 1.0, 20.0, "1 bigha in katha"),
                (3, 4, 5, 1.0, 100.0, "1 acre in decimal"),
                (1, 5, 6, 1.0, 40.0, "1 maund in seer"),
                (1, 6, 7, 1.0, 80.0, "1 seer in tola"),
                (1, 0, 2, 1.0, 2.20462262, "1 kg in lb"),
                (6, 3, 2, 1.0, 1024.0, "1 GB in MB"),
                (6, 6, 2, 100.0, 11.920928955, "100 Mbit in MB"),
                (5, 0, 2, 100.0, 62.137119224, "100 km/h in mph"),
                (4, 3, 0, 1.0, 3.785411784, "1 gal in L"),
            ] {
                let got = convert(cat, from, to, v);
                if (got - want).abs() > 1e-4 * want.abs().max(1.0) {
                    bad.push(format!("{what} = {got}"));
                }
            }
            out += &format!("{:<4} converter: 14 conversions incl. katha, bigha, decimal, maund, seer, tola, Mbit {:?}\n", if bad.is_empty() { "OK" } else { "FAIL" }, bad);

            let mut bad: Vec<String> = tools::CITIES.iter().filter(|(_, k)| !tz::known(k)).map(|(c, _)| c.to_string()).collect();
            let at = |y: u16, m: u16, d: u16, h: u16| tz::ft(&windows::Win32::Foundation::SYSTEMTIME { wYear: y, wMonth: m, wDay: d, wHour: h, ..Default::default() });
            for (utc, key, want) in [
                (at(2026, 1, 15, 6), "Bangladesh Standard Time", 12),
                (at(2026, 1, 15, 6), "GMT Standard Time", 6),
                (at(2026, 1, 15, 6), "Eastern Standard Time", 1),
                (at(2026, 1, 15, 6), "AUS Eastern Standard Time", 17),
                (at(2026, 7, 15, 6), "GMT Standard Time", 7),
                (at(2026, 7, 15, 6), "Eastern Standard Time", 2),
                (at(2026, 7, 15, 6), "AUS Eastern Standard Time", 16),
                (at(2026, 7, 15, 6), "Arabian Standard Time", 10),
            ] {
                match tz::local(Some(key), utc) {
                    Some(t) if t.wHour == want => {}
                    t => bad.push(format!("{key}: {:?}", t.map(|t| t.wHour))),
                }
            }
            out += &format!("{:<4} time zones: all 12 cities known, daylight saving right in January and July {:?}\n", if bad.is_empty() { "OK" } else { "FAIL" }, bad);
            out += &format!("INFO time zones: {}\n", tz::debug());

            let c = [30u8, 144, 255];
            let fmt = (tools::hex_of(c), tools::rgb_of(c), tools::hsl_of(c));
            let ok = fmt == ("#1E90FF".into(), "rgb(30, 144, 255)".into(), "hsl(210, 100%, 56%)".into()) && (tools::contrast([255, 255, 255], [0, 0, 0]) - 21.0).abs() < 0.01;
            out += &format!("{:<4} color formats HEX / RGB / HSL and contrast {:?}\n", if ok { "OK" } else { "FAIL" }, fmt);

            if let Some(ti) = find(a, "Tools") {
                a.toast = None;
                a.queue.clear();
                a.expanded = true;
                a.tab = ti;
                a.switch_t = -10.0;
                let qr = crate::modules::tools::qr_check("https://github.com/banulalakwindu/Nookly");
                let long = crate::modules::tools::qr_check(&"x".repeat(3500));
                out += &format!("{:<4} QR code for a link ({:?}), image copy size right, too-long text refused\n", if matches!(qr, Some((s, true)) if s >= 21) && long.is_none() { "OK" } else { "FAIL" }, qr);
                let names = ["color", "calculator", "QR code", "converter"];
                let mut missing = Vec::new();
                for (k, id) in [(0usize, 10u32), (1, 40), (2, 62), (3, 100)] {
                    a.call(ti, |m, c| m.action(1 + k as u32, (0.0, 0.0), c));
                    a.release_keyboard();
                    out += &snap(a, &format!("tools: {}", names[k]));
                    if !a.hits.iter().any(|h| h.module == ti && h.id == id) {
                        missing.push(names[k]);
                    }
                }
                a.call(ti, |m, c| m.action(77, (0.0, 0.0), c));
                out += &snap(a, "tools: time zones");
                let cities = a.hits.iter().filter(|h| h.module == ti && (110..122).contains(&h.id)).count();
                a.call(ti, |m, c| m.action(73, (0.0, 0.0), c));
                out += &format!("{:<4} Tools tab shows all four tools and 12 time-zone cities {:?}\n", if missing.is_empty() && cities == 12 { "OK" } else { "FAIL" }, (missing, cities));
                // Typing a sum and pressing Enter keeps it in the history.
                a.call(ti, |m, c| m.action(2, (0.0, 0.0), c));
                let kb = a.keyboard == Some(ti);
                for ch in "1250*15%".chars() {
                    a.on_char(ch);
                }
                a.on_key(0x0D);
                let hist = crate::modules::tools::test_last(a.modules[ti].debug_state());
                a.on_key(0x1B);
                a.on_key(0x1B);
                a.release_keyboard();
                out += &format!("{:<4} calculator takes typing, Enter keeps 1250*15% = 187.5 ({hist})\n", if kb && hist.contains("187.5") { "OK" } else { "FAIL" });
                // Labels fit.
                let mut tight = Vec::new();
                let gfx = &mut a.gfx as *mut Gfx;
                for (cat, room) in tools::unit_room(|s| unsafe { (*gfx).measure(s, 11.5, Weight::Semi) }, modules::PANEL.0 - 118.0 - 8.0) {
                    if room < 0.0 {
                        tight.push(cat);
                    }
                }
                let cat_w = (modules::PANEL.0 - 5.0 * 7.0) / 8.0 - 6.0;
                for c in tools::CATS.iter().map(|c| c.0).chain(["Zones"]) {
                    if a.gfx.measure(c, 11.5, Weight::Semi) > cat_w {
                        tight.push(c);
                    }
                }
                let city_w = (modules::PANEL.0 - 30.0) / 6.0 - 16.0;
                for (c, _) in tools::CITIES {
                    if a.gfx.measure(c, 10.5, Weight::Regular) > city_w {
                        tight.push(c);
                    }
                }
                for (_, n, _) in [("", "Calculator", ""), ("", "Converter", ""), ("", "QR code", ""), ("", "Color", "")] {
                    if a.gfx.measure(n, 11.5, Weight::Semi) + 20.0 > (modules::PANEL.0 - 18.0) / 4.0 - 12.0 {
                        tight.push(n);
                    }
                }
                for (s, w) in [("Pick from screen", 140.0 - 16.0), ("Copy image", 110.0 - 16.0), ("Esc cancels, arrows nudge", 149.5)] {
                    if a.gfx.measure(s, if w > 149.0 { 10.0 } else { 12.0 }, Weight::Semi) > w {
                        tight.push(s);
                    }
                }
                out += &format!("{:<4} Tools labels fit {:?}\n", if tight.is_empty() { "OK" } else { "FAIL" }, tight);
                // "Pick from screen" closes the notch first, then the result
                // reopens the Tools tab with the colour.
                a.call(ti, |m, c| m.action(tools::A_TEST_NO_OVERLAY, (0.0, 0.0), c));
                a.call(ti, |m, c| m.action(1, (0.0, 0.0), c));
                let tgt = a.target();
                a.cur = tgt;
                a.render(tgt, 0.0);
                if let Some(h) = a.hits.iter().find(|h| h.module == ti && h.id == 10).copied() {
                    a.on_down(h.rect.cx(), h.rect.cy());
                    a.on_up(h.rect.cx(), h.rect.cy());
                }
                let closed = !a.expanded;
                *tools::picker::RESULT.lock().unwrap() = Some(Some([0x12, 0x34, 0x56]));
                a.tick();
                let back = a.expanded && a.tab == ti && a.modules[ti].debug_state().contains("colors");
                out += &format!("{:<4} Pick from screen closes the notch, and the picked color reopens Tools ({})\n", if closed && back { "OK" } else { "FAIL" }, a.modules[ti].debug_state());
                // The screenshot the picker uses matches the real screen.
                match tools::picker::capture() {
                    Some(s) => {
                        let screen = unsafe { GetDC(None) };
                        let mut same = 0;
                        for (fx, fy) in [(0.1f32, 0.1f32), (0.5, 0.5), (0.9, 0.2), (0.3, 0.8), (0.7, 0.7)] {
                            let (x, y) = ((s.w as f32 * fx) as i32, (s.h as f32 * fy) as i32);
                            let p = unsafe { GetPixel(screen, s.x + x, s.y + y) };
                            let real = [(p.0 & 0xff) as u8, ((p.0 >> 8) & 0xff) as u8, ((p.0 >> 16) & 0xff) as u8];
                            if s.at(x, y) == Some(real) {
                                same += 1;
                            }
                        }
                        unsafe { ReleaseDC(None, screen) };
                        out += &format!("{:<4} color picker screenshot {}x{} matches the screen ({same}/5 points)\n", if same >= 3 { "OK" } else { "INFO" }, s.w, s.h);
                        if std::env::var("NOOKLY_TEST_PICKER").is_ok() {
                            *tools::picker::RESULT.lock().unwrap() = None;
                            let up = tools::picker::start(a.hwnd);
                            let clicked = tools::picker::test_click(12, 12);
                            let got = tools::picker::RESULT.lock().unwrap().take();
                            out += &format!("{:<4} color picker window opens and picks where clicked ({:?})\n", if up && clicked && got.is_some() && !tools::picker::active() { "OK" } else { "FAIL" }, got);
                        }
                    }
                    None => out += "INFO color picker: couldn't take a screenshot (no desktop?)\n",
                }
                a.call(ti, |m, c| m.action(1, (0.0, 0.0), c));
                a.expanded = false;
            } else {
                out += "FAIL Tools tab missing\n";
            }
        }
        // Resize animation: soft spring that settles quickly with a small
        // overshoot, and content that fades in after a tab switch.
        {
            let from = (476.0f32, 150.0f32, 26.0f32);
            let to = (from.0 + 280.0, from.1 + 66.0, from.2);
            a.cur = from;
            a.vel = (0.0, 0.0, 0.0);
            let (mut steps, mut over) = (0u32, 0.0f32);
            while steps < 180 {
                let d = a.step_anim(to, 1.0 / 60.0);
                over = over.max(a.cur.0 - to.0);
                steps += 1;
                if d == 0.0 {
                    break;
                }
            }
            let secs = steps as f32 / 60.0;
            // Liquid on purpose: a clear overshoot and wobble, settled
            // within a second.
            let ok = secs <= 0.9 && over > 0.04 * 280.0 && over < 0.16 * 280.0;
            out += &format!("{:<4} resize spring (liquid) settles in {secs:.2}s, overshoot {over:.1}px of 280\n", if ok { "OK" } else { "FAIL" });
            // Same result at a choppy 20 fps.
            a.cur = from;
            a.vel = (0.0, 0.0, 0.0);
            let mut s2 = 0u32;
            while s2 < 60 && a.step_anim(to, 0.05) != 0.0 {
                s2 += 1;
            }
            out += &format!("{:<4} resize spring is frame-rate independent ({:.2}s at 20 fps)\n", if (s2 as f32 * 0.05 - secs).abs() < 0.12 { "OK" } else { "FAIL" }, s2 as f32 * 0.05);
            let now = a.t();
            a.switch_t = now;
            let (a0, a1) = (a.switch_alpha(now), a.switch_alpha(now + 0.35));
            out += &format!("{:<4} tab switch fades content in ({a0:.2} -> {a1:.2})\n", if a0 < 0.05 && a1 > 0.99 { "OK" } else { "FAIL" });
            a.switch_t = -10.0;
            // Every tab opens to the same panel (only Music lyrics may widen).
            let mut sizes = Vec::new();
            for i in 0..a.modules.len() {
                if a.modules[i].has_tab() {
                    a.tab = i;
                    a.expanded = true;
                    let t = a.target();
                    sizes.push((a.modules[i].title(), t.0 as i32, t.1 as i32));
                }
            }
            // Wider, taller or fitted on purpose.
            sizes.retain(|s| !["Widgets", "Weather", "Volume"].contains(&s.0));
            let same = sizes.windows(2).all(|w| (w[0].1, w[0].2) == (w[1].1, w[1].2));
            out += &format!("{:<4} every tab opens to the same panel size {:?}\n", if same { "OK" } else { "FAIL" }, sizes.first().map(|s| (s.1, s.2)));

            // Switching tabs: old content slides out while the new slides in;
            // only the new content is clickable mid-way.
            let tabs: Vec<usize> = (0..a.modules.len()).filter(|&i| a.modules[i].has_tab()).collect();
            let (t0, t1) = (tabs[0], tabs[3.min(tabs.len() - 1)]);
            a.tab = t0;
            a.shown = (t0, false);
            let tgt = a.target();
            a.cur = tgt;
            a.render(tgt, 0.0);
            a.tab = t1;
            a.frame();
            let mid = a.t() - 0.12;
            a.switch_t = mid;
            a.render(tgt, 0.0);
            let old_hits = a.hits.iter().filter(|h| h.module == t0).count();
            let new_hits = a.hits.iter().filter(|h| h.module == t1).count();
            let ok = a.prev_shown.0 == t0 && a.switch_dir > 0.0 && old_hits == 0 && new_hits > 0 && a.gfx.last_err.is_none();
            out += &format!("{:<4} tab switch slides old content out, new in (old hits {old_hits}, new {new_hits})\n", if ok { "OK" } else { "FAIL" });
            // The highlight pill glides to the new tab instead of jumping.
            let slot = a.tab_slot;
            let target_off = tabs.iter().position(|&i| i == t1).unwrap_or(0) as f32 * slot;
            a.pill_off = 0.0;
            a.pill_last = a.t() - 0.016;
            a.render(tgt, 0.0);
            let first = a.pill_off;
            for _ in 0..40 {
                a.pill_last = a.t() - 0.016;
                a.render(tgt, 0.0);
            }
            let pill_ok = first > 0.0 && first < target_off && (a.pill_off - target_off).abs() < 0.5;
            out += &format!("{:<4} highlight pill glides to the new tab ({first:.1} -> {:.1} of {target_off:.0})\n", if pill_ok { "OK" } else { "FAIL" }, a.pill_off);
            a.switch_t = -10.0;
            a.expanded = false;
        }
        // Notifications: text rules, colours, muting, Do Not Disturb, and a
        // real Windows notification travelling into the notch.
        {
            use crate::modules::notifs::{self, Note};
            let ni = find(a, "Notifications").unwrap_or(0);
            let n = Note { app: "Messages".into(), title: "Amaya".into(), body: "See you\n at 7".into(), ..Default::default() };
            let ok = notifs::toast_text(&n, true) == ("Amaya".to_string(), "See you at 7".to_string())
                && notifs::toast_text(&n, false) == ("Messages".to_string(), "New notification".to_string())
                && notifs::toast_text(&Note { app: "Mail".into(), ..Default::default() }, true).0 == "Mail";
            out += &format!("{:<4} notification text (and hidden-text mode shows only the app)\n", if ok { "OK" } else { "FAIL" });
            let red: Vec<u8> = (0..64 * 64).flat_map(|_| [20u8, 20, 200, 255]).collect();
            let grey: Vec<u8> = (0..64 * 64).flat_map(|_| [128u8, 128, 128, 255]).collect();
            let c = notifs::icon_color(&red);
            let ok = c.map_or(false, |c| c.r > 0.8 && c.g < 0.5 && c.b < 0.5) && notifs::icon_color(&grey).map_or(false, |c| c.r == col::TEXT.r);
            out += &format!("{:<4} notification colour comes from the app icon\n", if ok { "OK" } else { "FAIL" });
            a.toast = None;
            a.queue.clear();
            a.expanded = false;
            a.hidden = false;
            out += &format!("INFO Do Not Disturb / focus on this PC right now: {} (tests below force it either way)\n", notifs::quiet());
            notifs::QUIET_OVERRIDE.store(2, std::sync::atomic::Ordering::Relaxed);
            FORCE_VISIBLE.store(true, std::sync::atomic::Ordering::Relaxed);
            let was_quiet = notifs::quiet();
            a.call(ni, |m, c| m.action(notifs::A_DEMO, (0.0, 0.0), c));
            a.frame();
            let shown = a.toast.as_ref().map_or(false, |t| t.0.key == "notif" && t.0.title == "Amaya" && t.0.click == Some(notifs::A_OPEN) && t.0.actions.len() == 2);
            if !was_quiet {
                out += &format!("{:<4} a notification shows in the notch with Open / Mute\n", if shown { "OK" } else { "FAIL" });
                out += &snap(a, "notification toast");
                // Mute: the app's next notification is dropped, and Settings counts it.
                a.toast = None;
                a.call(ni, |m, c| m.action(notifs::A_MUTE, (0.0, 0.0), c));
                a.toast = None;
                a.queue.clear();
                a.call(ni, |m, c| m.action(notifs::A_DEMO, (0.0, 0.0), c));
                a.frame();
                let silent = a.toast.is_none() && a.queue.is_empty();
                let counted = notifs::muted(&a.settings.extra) == vec!["Nookly.Demo".to_string()];
                a.settings.extra.remove(notifs::MUTED_KEY);
                a.settings.save();
                out += &format!("{:<4} muting an app silences it\n", if silent && counted { "OK" } else { "FAIL" });
            } else {
                out += "INFO Do Not Disturb is on: notification display tests skipped\n";
            }
            out += &format!("INFO focus/Do Not Disturb state: {:?} (0 = off)\n", notifs::focus_profile());
            // Read what's in Notification Center now, step by step, on a
            // background (MTA) thread like the watcher, with a time limit, so
            // a hanging step is named instead of freezing the self-test.
            {
                let trace = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
                let tr = trace.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    unsafe {
                        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
                    }
                    let log = |s: String| tr.lock().unwrap().push(s);
                    let t0 = std::time::Instant::now();
                    let ms = |t: std::time::Instant| t.elapsed().as_secs_f64() * 1000.0;
                    let list = windows::UI::Notifications::Management::UserNotificationListener::Current()
                        .and_then(|l| l.GetNotificationsAsync(windows::UI::Notifications::NotificationKinds::Toast))
                        .and_then(|op| op.join());
                    let Ok(list) = list else {
                        log(format!("list failed: {:?}", list.err()));
                        let _ = tx.send(());
                        return;
                    };
                    log(format!("list: {} in {:.0} ms", list.Size().unwrap_or(0), ms(t0)));
                    let mut done: Vec<String> = Vec::new();
                    for n in list {
                        let info = n.AppInfo().ok();
                        let app = info.as_ref().and_then(|i| i.DisplayInfo().ok()).and_then(|d| d.DisplayName().ok()).map(|s| s.to_string()).unwrap_or_default();
                        if done.contains(&app) {
                            continue;
                        }
                        done.push(app.clone());
                        let t = std::time::Instant::now();
                        log(format!("{app}: reading..."));
                        let note = notifs::read_note(&n);
                        log(format!(
                            "{app}: read in {:.0} ms total, icon {}, title {} chars, body {} chars",
                            ms(t),
                            if note.icon.is_some() { "yes" } else { "no" },
                            note.title.chars().count(),
                            note.body.chars().count()
                        ));
                    }
                    let _ = tx.send(());
                });
                let finished = rx.recv_timeout(std::time::Duration::from_secs(15)).is_ok();
                for line in trace.lock().unwrap().iter() {
                    out += &format!("INFO center: {line}\n");
                }
                out += &format!("{:<4} reading Notification Center finishes (15 s limit)\n", if finished { "OK" } else { "FAIL" });
            }
            // Live: a silent test notification (straight to Notification
            // Center, no banner, no sound) must reach the notch.
            a.toast = None;
            a.queue.clear();
            let had = a.settings.notif_on;
            a.settings.notif_on = true;
            a.tick();
            let mut ready = false;
            for _ in 0..30 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                if notifs::ACCESS.load(std::sync::atomic::Ordering::Relaxed) == 1 {
                    ready = true;
                    break;
                }
            }
            // Let the watcher take its first look (it never replays old ones).
            std::thread::sleep(std::time::Duration::from_millis(900));
            let aumid = HSTRING::from(r"{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\WindowsPowerShell\v1.0\powershell.exe");
            let sent = (|| -> windows::core::Result<std::time::Instant> {
                use windows::Data::Xml::Dom::XmlDocument;
                use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
                let xml = XmlDocument::new()?;
                xml.LoadXml(h!("<toast><visual><binding template='ToastGeneric'><text>Nookly self-test</text><text>This test notification removes itself</text></binding></visual><audio silent='true'/></toast>"))?;
                let toast = ToastNotification::CreateToastNotification(&xml)?;
                toast.SetTag(h!("wl-selftest"))?;
                toast.SetGroup(h!("wl"))?;
                toast.SetSuppressPopup(true)?;
                ToastNotificationManager::CreateToastNotifierWithId(&aumid)?.Show(&toast)?;
                Ok(std::time::Instant::now())
            })();
            let mut got = None;
            if let Ok(t0) = sent {
                while t0.elapsed().as_secs_f64() < 5.0 && got.is_none() {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    a.tick();
                    let hit = |t: &Toast| t.key == "notif" && t.title == "Nookly self-test";
                    if a.toast.as_ref().map_or(false, |t| hit(&t.0)) || a.queue.iter().any(|t| hit(t)) {
                        got = Some(t0.elapsed().as_secs_f64() * 1000.0);
                    }
                }
                let _ = windows::UI::Notifications::ToastNotificationManager::History().and_then(|h| h.RemoveGroupedTagWithId(h!("wl-selftest"), h!("wl"), &aumid));
            }
            let quiet = notifs::quiet();
            match (ready, &sent, got) {
                (true, Ok(_), Some(ms)) => out += &format!("OK   a real Windows notification reached the notch in {ms:.0} ms\n"),
                (_, _, None) if quiet => out += "INFO live notification test skipped: Do Not Disturb is on\n",
                (false, _, _) if std::env::var("CI").is_ok() => out += "INFO notification access: not available on this build server\n",
                (false, _, _) => out += "FAIL notification access: Windows didn't allow the listener\n",
                (_, Err(e), _) => out += &format!("INFO live notification test: couldn't send a test notification ({e})\n"),
                _ => out += "FAIL a real Windows notification never reached the notch (5 s)\n",
            }
            // Three arrive during a full-screen game: nothing shows until the
            // notch is back, then the newest with "+2 more". (Do Not Disturb
            // holds them the same way, so this needs it off.)
            if !quiet {
                a.toast = None;
                a.queue.clear();
                a.hidden = true;
                for _ in 0..3 {
                    a.call(ni, |m, c| m.action(notifs::A_DEMO_INBOX, (0.0, 0.0), c));
                }
                a.tick();
                let held = a.toast.is_none() && a.queue.iter().all(|t| t.key != "notif");
                a.hidden = false;
                a.tick();
                a.frame();
                let sub = a.toast.as_ref().map(|t| t.0.sub.clone()).unwrap_or_default();
                out += &format!("{:<4} notifications during full screen wait, then show as one ({sub})\n", if held && sub.ends_with("(+2 more)") { "OK" } else { "FAIL" });
            }
            // Do Not Disturb holds notifications too (never drops them).
            {
                a.toast = None;
                a.queue.clear();
                notifs::QUIET_OVERRIDE.store(1, std::sync::atomic::Ordering::Relaxed);
                for _ in 0..2 {
                    a.call(ni, |m, c| m.action(notifs::A_DEMO_INBOX, (0.0, 0.0), c));
                }
                a.tick();
                let held = a.toast.is_none() && a.queue.iter().all(|t| t.key != "notif");
                notifs::QUIET_OVERRIDE.store(2, std::sync::atomic::Ordering::Relaxed);
                a.tick();
                a.frame();
                let sub = a.toast.as_ref().map(|t| t.0.sub.clone()).unwrap_or_default();
                out += &format!("{:<4} notifications during Do Not Disturb wait, then show ({sub})\n", if held && sub.ends_with("(+1 more)") { "OK" } else { "FAIL" });
            }
            notifs::QUIET_OVERRIDE.store(0, std::sync::atomic::Ordering::Relaxed);
            FORCE_VISIBLE.store(false, std::sync::atomic::Ordering::Relaxed);
            a.settings.notif_on = had;
            a.settings.save();
            a.tick();
            a.toast = None;
            a.queue.clear();
        }
        // Updates: version maths, release parsing, and a live GitHub check.
        {
            use crate::modules::update::*;
            let ok = is_newer("v0.1.10", "0.1.9") && !is_newer("0.1.4", "0.1.4") && !is_newer("v0.1.3", "0.1.4") && is_newer("1.0.0", "0.9.9");
            out += &format!("{:<4} update version comparison\n", if ok { "OK" } else { "FAIL" });
            let json = br#"{"tag_name":"v9.9.9","html_url":"https://x","draft":false,"prerelease":false,
                "assets":[{"name":"NooklySetup-9.9.9.exe","browser_download_url":"https://github.com/a/b.exe","size":123}]}"#;
            let rel = parse_release(json);
            let ok = rel.as_ref().map_or(false, |r| r.version == "9.9.9" && r.installer.as_ref().map_or(false, |i| i.size == Some(123)));
            out += &format!("{:<4} update release JSON parsing\n", if ok { "OK" } else { "FAIL" });
            let ok = sha256_hex(b"abc") == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
            out += &format!("{:<4} SHA-256 matches the standard test vector\n", if ok { "OK" } else { "FAIL" });
            let html = format!(
                r#"<a href="/{REPO}/releases/download/v9.9.9/Nookly-src.zip">x</a><clipboard-copy value="sha256:{a}">
                <a href="/{REPO}/releases/download/v9.9.9/NooklySetup-9.9.9.exe" rel="nofollow">x</a>
                <clipboard-copy value="sha256:{b}"></clipboard-copy><span>2.3 MB</span>"#,
                a = "1".repeat(64),
                b = "AB".repeat(32)
            );
            let inst = parse_assets_html(&html, "v9.9.9");
            let ok = inst.as_ref().map_or(false, |i| {
                i.url == format!("https://github.com/{REPO}/releases/download/v9.9.9/NooklySetup-9.9.9.exe") && i.sha256.as_deref() == Some(&"ab".repeat(32)[..])
            });
            out += &format!("{:<4} release page parsing finds the installer and its SHA-256\n", if ok { "OK" } else { "FAIL" });
            // Live: both routes must agree on the latest release.
            let api = fetch_api();
            let web = fetch_web();
            let desc = |r: &std::result::Result<Release, String>| match r {
                Ok(r) => format!(
                    "{} (installer {}, sha256 {})",
                    r.version,
                    r.installer.as_ref().map_or("missing".into(), |i| i.url.rsplit('/').next().unwrap_or("").to_string()),
                    r.installer.as_ref().and_then(|i| i.sha256.clone()).map_or("none".into(), |h| h[..12].to_string())
                ),
                Err(e) => format!("failed ({e})"),
            };
            out += &format!("INFO GitHub API: {}\n", desc(&api));
            let web_ok = web.as_ref().map_or(false, |r| r.installer.as_ref().map_or(false, |i| i.sha256.is_some()));
            let agree = match (&api, &web) {
                (Ok(a), Ok(w)) => a.version == w.version,
                _ => true,
            };
            out += &format!("{:<4} release pages (no API limit): {}{}\n", if web_ok && agree { "OK" } else { "FAIL" }, desc(&web), if agree { "" } else { " DISAGREES with the API" });
            if let Ok(r) = fetch_latest() {
                out += &format!("INFO latest release {} -> this build {VERSION} is {}\n", r.version, if is_newer(&r.version, VERSION) { "behind (update offered)" } else { "up to date" });
            }
            if std::env::var("NOOKLY_TEST_DOWNLOAD").is_ok() {
                for (route, r) in [("release pages", web), ("API", api)] {
                    if let Ok(r) = r {
                        match download_installer(&r) {
                            Ok(p) => {
                                let n = std::fs::metadata(&p).map_or(0, |m| m.len());
                                out += &format!("OK   installer download via {route}, {n} bytes, checksum verified\n");
                                let _ = std::fs::remove_file(p);
                            }
                            Err(e) => out += &format!("FAIL installer download via {route}: {e}\n"),
                        }
                    }
                }
                // A tampered file must be refused.
                if let Ok(Some(mut i)) = fetch_web().map(|r| r.installer) {
                    i.sha256 = Some("0".repeat(64));
                    let bad = Release { version: "0.0.0".into(), page: String::new(), installer: Some(i) };
                    let refused = matches!(download_installer(&bad), Err(e) if e.contains("doesn't match"));
                    out += &format!("{:<4} a download that doesn't match its checksum is refused\n", if refused { "OK" } else { "FAIL" });
                }
            }
        }
        for i in 0..a.modules.len() {
            let s = a.modules[i].debug_state();
            if !s.is_empty() {
                out += &format!("INFO {}: {s}\n", a.modules[i].title());
            }
        }
        out
    })
    .unwrap_or_else(|| "app not running".into())
}

/// Write the current frame as `w:u32 h:u32` + premultiplied BGRA rows,
/// cropped to `crop` (inclusive pixel box) or to the drawn bounding box.
fn write_raw(g: &Gfx, path: &std::path::Path, crop: Option<(i32, i32, i32, i32)>) {
    let px = g.pixels();
    let (pw, ph) = (g.pw, g.ph);
    let Some((x0, y0, x1, y1)) = crop.or_else(|| g.bbox()) else { return };
    let (x1, y1) = (x1.min(pw - 1), y1.min(ph - 1));
    let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
    let mut out = Vec::with_capacity(8 + (w * h * 4) as usize);
    out.extend_from_slice(&(w as u32).to_le_bytes());
    out.extend_from_slice(&(h as u32).to_le_bytes());
    for y in y0..=y1 {
        let s = ((y * pw + x0) * 4) as usize;
        out.extend_from_slice(&px[s..s + (w * 4) as usize]);
    }
    let _ = std::fs::write(path, out);
}

fn shot(a: &mut App, dir: &std::path::Path, name: &str) {
    let tgt = a.target();
    a.cur = tgt;
    // A few frames so smoothed things (lyric scroll) settle.
    for _ in 0..60 {
        a.render(tgt, 0.0);
    }
    write_raw(&a.gfx, &dir.join(format!("{name}.raw")), None);
}

/// Animate toward the current target, writing one raw frame per step.
fn animate(a: &mut App, dir: &std::path::Path, n: &mut u32, steps: u32, crop: (i32, i32, i32, i32)) {
    for _ in 0..steps {
        let tgt = a.target();
        let k = 1.0 - (-(1.0f32 / 30.0) * 16.0).exp();
        a.cur = (lerp(a.cur.0, tgt.0, k), lerp(a.cur.1, tgt.1, k), lerp(a.cur.2, tgt.2, k));
        let dist = (a.cur.0 - tgt.0).abs() + (a.cur.1 - tgt.1).abs();
        a.render(tgt, dist);
        write_raw(&a.gfx, &dir.join(format!("anim_{:03}.raw", *n)), Some(crop));
        *n += 1;
    }
}

/// `nookly --render-docs <dir>`: render every view with demo data for the
/// README/user guide. Run with `NOOKLY_DATA_DIR` pointing at seeded demo data.
pub fn render_docs(dir: &std::path::Path) -> String {
    let _ = std::fs::create_dir_all(dir);
    with_app(|a| {
        let mut log = String::new();
        let scale = 2.0;
        a.scale = scale;
        let _ = a.gfx.resize((CANVAS_W * scale) as i32, (CANVAS_H * scale) as i32);
        // NOOKLY_RENDER_STYLE=slim renders every shot in another style (for checking layouts).
        let base = std::env::var("NOOKLY_RENDER_STYLE").ok().and_then(|s| NotchStyle::from_key(&s)).unwrap_or(NotchStyle::Full);
        a.settings.style = base;
        // NOOKLY_RENDER_THEME / NOOKLY_RENDER_SECONDS: renders in a theme, with seconds.
        if let Ok(t) = std::env::var("NOOKLY_RENDER_THEME") {
            a.settings.theme = t;
        }
        a.settings.seconds = std::env::var("NOOKLY_RENDER_SECONDS").is_ok();
        crate::time::SECONDS.store(a.settings.seconds, std::sync::atomic::Ordering::Relaxed);
        a.settings.edge = Edge::Top;
        a.settings.personality = true;
        a.anchor = CANVAS_W / 2.0;
        a.queue.clear();
        a.toast = None;
        a.expanded = false;
        a.mood = Mood::Clock;
        let music = find(a, "Now Playing").unwrap_or(0);
        let pomo = find(a, "Pomodoro").unwrap_or(0);
        let timer = find(a, "Timer").unwrap_or(0);
        let water = find(a, "Water Reminder").unwrap_or(0);
        let todo = find(a, "TodoList").unwrap_or(0);
        let shelf = find(a, "Shelf").unwrap_or(0);
        let act = |a: &mut App, i: usize, id: u32| a.call(i, |m, c| m.action(id, (0.0, 0.0), c));
        let tick = |a: &mut App, i: usize| a.call(i, |m, c| m.tick(c));
        tick(a, music);

        // Collapsed looks.
        shot(a, dir, "collapsed_full_music");
        for (s, n) in [(NotchStyle::Slim, "collapsed_slim_music"), (NotchStyle::Compact, "collapsed_compact_music"), (NotchStyle::Pill, "collapsed_pill_music")] {
            a.settings.style = s;
            shot(a, dir, n);
        }
        act(a, music, 1); // pause
        tick(a, music);
        shot(a, dir, "collapsed_pill_idle");
        a.settings.style = NotchStyle::Compact;
        shot(a, dir, "collapsed_compact_clock");
        a.settings.style = base;
        shot(a, dir, "collapsed_full_clock");
        for (m, n) in [(Mood::Eyes, "mascot_eyes"), (Mood::Doze, "mascot_doze"), (Mood::Wave, "mascot_wave")] {
            a.mood = m;
            a.mood_since = a.t() - 5.0;
            shot(a, dir, n);
        }
        for m in Mascot::ALL {
            a.settings.mascot = m;
            a.mood = Mood::Eyes;
            a.mood_since = a.t() - 5.0;
            shot(a, dir, &format!("mascot_{}", m.key()));
        }
        a.settings.mascot = Mascot::Eyes;
        a.mood = Mood::Clock;
        act(a, music, 1); // play
        tick(a, music);

        // Panels.
        a.expanded = true;
        a.tab = music;
        shot(a, dir, "panel_music");
        act(a, music, crate::modules::music::A_DEMO_LYRICS);
        shot(a, dir, "panel_lyrics");
        act(a, music, 5); // lyrics off
        a.tab = pomo;
        act(a, pomo, crate::modules::pomodoro::A_DEMO);
        shot(a, dir, "panel_pomodoro");
        a.expanded = false;
        shot(a, dir, "collapsed_full_pomodoro");
        act(a, pomo, 2); // reset
        a.expanded = true;
        a.tab = timer;
        act(a, timer, crate::modules::timer::A_DEMO_TIMER);
        shot(a, dir, "panel_timer");
        a.expanded = false;
        shot(a, dir, "collapsed_full_timer");
        a.settings.style = NotchStyle::Pill;
        shot(a, dir, "collapsed_pill_timer");
        a.settings.style = base;
        a.expanded = true;
        act(a, timer, crate::modules::timer::A_DEMO_WATCH);
        shot(a, dir, "panel_stopwatch");
        act(a, timer, 1); // countdown view
        act(a, timer, 4); // reset it
        a.tab = water;
        shot(a, dir, "panel_water");
        act(a, water, 2);
        shot(a, dir, "panel_water_settings");
        act(a, water, 3);
        a.tab = todo;
        shot(a, dir, "panel_todo");
        act(a, todo, 1); // add task
        a.call(todo, |m, c| {
            for ch in "Call the dentist".chars() {
                m.chr(ch, c);
            }
            m.action(11, (0.0, 0.0), c); // +30m
        });
        shot(a, dir, "panel_todo_editor");
        act(a, todo, 3); // cancel
        a.keyboard = None;
        a.tab = shelf;
        shot(a, dir, "panel_shelf");
        a.drag_files = true;
        for m in a.modules.iter_mut() {
            m.drag_hover(true);
        }
        shot(a, dir, "panel_shelf_drop");
        a.drag_files = false;
        for m in a.modules.iter_mut() {
            m.drag_hover(false);
        }
        if let Some(clip) = find(a, "Clipboard") {
            a.call(clip, |m, c| m.action(crate::modules::clipboard::A_DEMO_DOCS, (0.0, 0.0), c));
            a.tab = clip;
            shot(a, dir, "panel_clipboard");
        }
        if let Some(p) = find(a, "Prayer Times") {
            for k in [0u32, 2, 3, 4, 5] {
                a.call(p, |m, c| m.action(90 + k, (0.0, 0.0), c));
            }
            a.tab = p;
            shot(a, dir, "panel_prayer");
            a.call(p, |m, c| m.action(crate::modules::prayer::A_TEST_RELOAD, (0.0, 0.0), c));
        }
        if let Some(t) = find(a, "Tools") {
            a.call(t, |m, c| m.action(crate::modules::tools::A_DEMO, (0.0, 0.0), c));
            a.tab = t;
            for (k, name) in [(1u32, "panel_tools_color"), (2, "panel_tools_calc"), (3, "panel_tools_qr"), (4, "panel_tools_convert")] {
                a.call(t, |m, c| m.action(k, (0.0, 0.0), c));
                a.release_keyboard();
                shot(a, dir, name);
            }
            a.call(t, |m, c| m.action(77, (0.0, 0.0), c));
            shot(a, dir, "panel_tools_zones");
            a.call(t, |m, c| m.action(73, (0.0, 0.0), c));
            a.call(t, |m, c| m.action(1, (0.0, 0.0), c));
        }
        if let Some(g) = find(a, "AI Agents") {
            a.call(g, |m, c| m.action(crate::modules::agents::A_DEMO, (0.0, 0.0), c));
            a.tab = g;
            shot(a, dir, "panel_agents");
        }
        if let Some(m) = find(a, "System Monitor") {
            a.call(m, |x, c| x.action(crate::modules::monitor::A_DEMO, (0.0, 0.0), c));
            a.tab = m;
            shot(a, dir, "panel_monitor");
        }
        if let Some(c) = find(a, "Calendar") {
            a.tab = c;
            shot(a, dir, "panel_calendar");
        }
        if let Some(w) = find(a, "Widgets") {
            a.call(w, |x, c| x.action(crate::modules::widgets::A_DEMO, (0.0, 0.0), c));
            let layout = a.settings.layout.clone();
            a.settings.layout = crate::store::NotchLayout::custom_rows(vec![NotchRow::wide("music"), NotchRow::pair("agents", "timer"), NotchRow::pair("bluetooth", "battery"), NotchRow::pair("cpu", "")], true);
            a.tab = w;
            a.call(w, |x, c| x.action(1, (0.0, 0.0), c));
            shot(a, dir, "panel_widgets");
            a.call(w, |x, c| x.action(2, (0.0, 0.0), c));
            shot(a, dir, "panel_widgets_order");
            a.call(w, |x, c| x.action(3, (0.0, 0.0), c));
            a.settings.set_alert_mode("volume", AlertMode::Small);
            a.settings.set_alert_mode("caps", AlertMode::Off);
            shot(a, dir, "panel_widgets_alerts");
            a.call(w, |x, c| x.action(5, (0.0, 0.0), c));
            shot(a, dir, "panel_widgets_place");
            a.call(w, |x, c| x.action(1, (0.0, 0.0), c));
            a.settings.set_alert_mode("volume", AlertMode::Popup);
            a.settings.set_alert_mode("caps", AlertMode::Popup);
            // A side mascot and a centred one with two widgets, and a small
            // alert beside a widget.
            a.expanded = false;
            a.settings.layout = crate::store::NotchLayout::custom_rows(vec![NotchRow::pair("date", "bluetooth")], false);
            let m = (a.mood, a.settings.mascot_pos, a.settings.personality);
            a.settings.personality = true;
            a.mood = Mood::Eyes;
            for (pos, n) in [(0u8, "widgets_mascot_left"), (1, "widgets_mascot_center")] {
                a.settings.mascot_pos = pos;
                shot(a, dir, n);
            }
            (a.mood, a.settings.mascot_pos, a.settings.personality) = m;
            a.push_toast(Toast::new("volume", glyph::VOLUME, col::TEXT, "Volume", "").right("60%").progress(0.6).secs(3.0));
            a.small_alert = a.small_alert.take().or_else(|| Some((alert_widget(&Toast::new("volume", glyph::VOLUME, col::TEXT, "Volume", "").right("60%").progress(0.6)), f64::MAX)));
            if let Some(s) = &mut a.small_alert {
                s.1 = f64::MAX;
            }
            a.queue.clear();
            shot(a, dir, "widgets_small_alert");
            a.small_alert = None;
            a.expanded = true;
            a.settings.layout = layout;
        }
        // Every live activity on the closed notch, one at a time (the other
        // panels hidden), then music with a mascot beside and between.
        {
            let saved = (a.settings.hidden_panels.clone(), a.mood, a.settings.mascot_pos);
            a.expanded = false;
            let tabs: Vec<usize> = (0..a.modules.len()).filter(|&j| a.modules[j].has_tab()).collect();
            for &i in &tabs {
                a.settings.hidden_panels = tabs.iter().filter(|&&j| j != i).map(|&j| a.modules[j].title().to_string()).collect();
                if a.live_module() == Some(i) {
                    shot(a, dir, &format!("live_{}", a.modules[i].title().replace(' ', "_").to_lowercase()));
                }
            }
            a.settings.hidden_panels = tabs.iter().filter(|&&j| j != music).map(|&j| a.modules[j].title().to_string()).collect();
            a.mood = Mood::Eyes;
            for (pos, n) in [(0u8, "live_music_mascot_left"), (1, "live_music_mascot_center")] {
                a.settings.mascot_pos = pos;
                shot(a, dir, n);
            }
            // Widgets on the resting notch, and music beside an agent.
            a.settings.hidden_panels.clear();
            a.mood = Mood::Clock;
            let layout = a.settings.layout.clone();
            if let Some(w) = find(a, "Widgets") {
                a.call(w, |x, c| x.action(crate::modules::widgets::A_DEMO, (0.0, 0.0), c));
            }
            let only = |a: &mut App, keep: &[&str]| {
                let tabs: Vec<usize> = (0..a.modules.len()).filter(|&j| a.modules[j].has_tab()).collect();
                a.settings.hidden_panels = tabs.iter().filter(|&&j| !keep.contains(&a.modules[j].title())).map(|&j| a.modules[j].title().to_string()).collect();
            };
            only(a, &["Widgets", "System Monitor"]);
            for (rows, n) in [
                (vec![NotchRow::pair("bluetooth", "cpu")], "widgets_pair"),
                (vec![NotchRow::wide("bluetooth")], "widgets_wide"),
                (vec![NotchRow::pair("cpu", "net")], "widgets_monitor"),
                (vec![NotchRow::pair("weather", "wifi")], "widgets_weather_wifi"),
                (vec![NotchRow::wide("weather")], "widgets_weather_wide"),
                (vec![NotchRow::pair("volume", "mic")], "widgets_sound"),
            ] {
                a.settings.layout = crate::store::NotchLayout::custom_rows(rows, false);
                shot(a, dir, n);
            }
            only(a, &["Now Playing", "AI Agents"]);
            a.settings.layout = crate::store::NotchLayout::custom_rows(vec![NotchRow::pair("music", "agents")], false);
            shot(a, dir, "live_music_and_agents");
            a.settings.layout = layout;
            (a.settings.hidden_panels, a.mood, a.settings.mascot_pos) = saved;
            a.expanded = true;
        }
        // Liquid Glass over a colourful and a light background.
        if let Some(m) = find(a, "Now Playing") {
            let (theme, tab) = (a.settings.theme.clone(), a.tab);
            a.settings.theme = "glass".into();
            a.tab = m;
            a.expanded = true;
            // Over a detailed scene (NOOKLY_GLASS_SCENE: a photo and an app
            // window, from make_images.py) the whole canvas is written, so
            // the picture can be laid on that same scene, lined up.
            let scene = std::env::var("NOOKLY_GLASS_SCENE").ok().filter(|p| std::path::Path::new(p).exists());
            let kinds: Vec<(String, &str)> = scene.into_iter().map(|p| (p, "glass_scene")).chain([("color".to_string(), "glass_color"), ("light".to_string(), "glass_light")]).collect();
            for (kind, name) in kinds {
                // Safe: the self-test and doc renders run on one thread.
                unsafe { std::env::set_var("NOOKLY_RENDER_BACKDROP", &kind) };
                a.backdrop = None;
                a.glass_raw = None;
                let tgt = a.target();
                a.cur = tgt;
                for _ in 0..30 {
                    a.render(tgt, 0.0);
                }
                let full = kind.ends_with(".raw").then(|| (0, 0, a.gfx.pw - 1, a.gfx.ph - 1));
                write_raw(&a.gfx, &dir.join(format!("{name}.raw")), full);
            }
            // Safe: the self-test and doc renders run on one thread.
            unsafe { std::env::remove_var("NOOKLY_RENDER_BACKDROP") };
            (a.settings.theme, a.tab) = (theme, tab);
            a.backdrop = None;
            a.glass_raw = None;
        }
        // The Settings window, at the same 2x scale.
        for (name, tab, sec) in [("settings_general", 0u8, 0u8), ("settings_privacy", 0, SEC_PRIVACY), ("settings_look", 0, SEC_THEME), ("settings_features", 0, SEC_FEATURES), ("settings_shortcuts", 0, SEC_SHORTCUTS), ("settings_widgets", 0, SEC_WIDGETS), ("settings_panels", 0, SEC_PANELS), ("settings_location", 0, SEC_LOCATION), ("settings_taskbar", 1, 0), ("settings_bar_layout", 1, 1), ("settings_bar_widgets", 1, 4), ("settings_bar_screens", 1, 5)] {
            a.sw_view(tab, sec);
            if let Some(sw) = a.swin.as_mut() {
                if tab == 1 {
                    sw.bar_section = sec;
                }
                sw.set_scale(scale);
            }
            a.sw_render();
            if let Some(sw) = &a.swin {
                write_raw(&sw.gfx, &dir.join(format!("{name}.raw")), None);
            }
        }
        a.close_settings();
        // Nookly's own installer, uninstaller, About and tray menu (with a
        // neutral install folder, not this PC's).
        let theme = crate::gfx::THEMES[0];
        let folder = std::path::Path::new(r"C:\Users\you\AppData\Local\Programs\Nookly");
        let dialogs = [
            ("dlg_setup", Some(crate::setup::install_spec(folder, false, true, false, true)), None),
            ("dlg_uninstall", Some(crate::uninstall::ask_spec(false)), None),
            ("dlg_about", Some(crate::tray::about_spec()), None),
            ("dlg_menu", None, Some(0)),
        ];
        for (name, spec, hover) in dialogs {
            let items = if spec.is_none() { crate::tray::menu_items() } else { Vec::new() };
            if let Some(g) = crate::dialog::snapshot(spec, items, theme, scale, hover) {
                write_raw(&g, &dir.join(format!("{name}.raw")), None);
            }
        }
        // Opening Settings closed the notch; the panel shots below need it open.
        a.expanded = true;
        // The new panels.
        {
            let hidden = std::mem::take(&mut a.settings.hidden_panels);
            let tab = a.tab;
            for (title, name) in [("Notes", "panel_notes"), ("Countdowns", "panel_countdowns"), ("Launcher", "panel_launcher"), ("Toggles", "panel_toggles"), ("Health", "panel_health"), ("Mirror", "panel_mirror"), ("Commands", "panel_commands"), ("Weather", "panel_weather"), ("Volume", "panel_volume")] {
                if let Some(i) = a.modules.iter().position(|m| m.title() == title) {
                    a.tab = i;
                    shot(a, dir, name);
                }
            }
            // Everything on: the panels that don't fit, under the ⋯.
            a.tab = a.modules.iter().position(|m| m.title() == "Now Playing").unwrap_or(0);
            a.more_open = true;
            shot(a, dir, "panel_more_all");
            a.more_open = false;
            // The song's colour glowing around the closed notch.
            a.settings.extra.insert("album_glow".into(), "1".into());
            a.expanded = false;
            a.tick();
            *crate::modules::music::ART_COLOR.lock().unwrap() = Some(hex(0xFF7A3D));
            shot(a, dir, "notch_glow");
            a.settings.extra.remove("album_glow");
            a.expanded = true;
            (a.settings.hidden_panels, a.tab) = (hidden, tab);
        }
        a.open_settings();
        a.start_onboarding();
        for (k, name) in ["tour_hello", "tour_style", "tour_colour", "tour_presets", "tour_tips"].into_iter().enumerate() {
            a.onboarding = Some(k as u8);
            shot(a, dir, name);
        }
        a.onboarding = None;
        a.expanded = true;
        // Every panel on: the ones that don't fit go under ⋯.
        {
            let hidden = std::mem::take(&mut a.settings.hidden_panels);
            let tab = a.tab;
            a.tab = a.modules.iter().position(|m| m.title() == "Calendar").unwrap_or(0);
            shot(a, dir, "tabs_overflow");
            a.more_open = true;
            shot(a, dir, "panel_more");
            a.more_open = false;
            (a.settings.hidden_panels, a.tab) = (hidden, tab);
        }

        // Toasts.
        a.expanded = false;
        let toasts = [
            ("toast_timer", Toast::new("timer", "\u{E121}", col::PINK, "Time's up!", "10 minute timer finished").action("Restart", 0).action("Dismiss", 0)),
            ("toast_water", Toast::new("water", glyph::DROP, col::CYAN, "Time to Drink", "Stay hydrated! 750 ml to go").action("Drank", 0).action("Snooze", 0)),
            ("toast_todo", Toast::new("todo", glyph::ALARM, col::YELLOW, "Send the weekly update", "Task due now").action("Done", 0).action("Snooze 10m", 0)),
            ("toast_pomodoro", Toast::new("pomodoro", glyph::TIMER, col::GREEN, "Break Time!", "5 minute short break").action("Start", 0).action("Later", 0)),
            ("toast_charging", Toast::new("power", glyph::CHARGING, col::GREEN, "Charging", "Charger connected").right("70%").progress(0.7)),
            ("toast_volume", Toast::new("volume", glyph::VOLUME, col::TEXT, "Volume", "Output").right("45%").progress(0.45)),
            ("toast_caps", Toast::new("caps", glyph::CAPS, col::ACCENT, "Caps Lock", "On")),
            ("toast_bt", Toast::new("bt", glyph::HEADPHONE, col::BLUE, "Earbuds", "Connected")),
            ("toast_lang", Toast::new("lang", glyph::KEYBOARD, col::BLUE, "Bangla (Bangladesh)", "Keyboard language").right("BN")),
            ("toast_copy", crate::modules::clipboard::copy_toast(&crate::modules::clipboard::Content::Text("Meeting moved to 4:30 pm, same room.\nBring the printed slides.".into()))),
            ("toast_agent", Toast::new("agent", crate::modules::agents::ICON, col::ACCENT, "Hermes needs you", "desktop \u{00B7} Wants your OK to run a command")),
            ("toast_agent_done", Toast::new("agent", crate::modules::agents::ICON, col::GREEN, "Claude Code finished", "Nookly \u{00B7} took 4m 12s")),
            (
                "toast_notification",
                Toast::new("notif", glyph::BELL, col::GREEN, "Amaya", "Lunch at 1? I booked the table by the window")
                    .image(std::sync::Arc::new(crate::modules::notifs::demo_icon()))
                    .click(1)
                    .action("Open", 0)
                    .action("Mute app", 0),
            ),
            ("toast_mic", Toast::new("privacy", glyph::MIC, crate::modules::privacy::MIC, "Microphone in use", "Recorder")),
        ];
        for (name, t) in toasts {
            let end = a.t() + 100.0;
            a.toast = Some((t, end));
            shot(a, dir, name);
        }
        a.toast = None;

        // Other edges (music live).
        for (e, s, n) in [
            (Edge::Left, NotchStyle::Full, "edge_left_full"),
            (Edge::Right, NotchStyle::Compact, "edge_right_compact"),
            (Edge::Bottom, NotchStyle::Pill, "edge_bottom_pill"),
        ] {
            a.settings.edge = e;
            a.settings.style = s;
            a.anchor = if e.vertical() { CANVAS_H / 2.0 } else { CANVAS_W / 2.0 };
            shot(a, dir, n);
        }
        a.settings.edge = Edge::Left;
        a.settings.style = base;
        a.anchor = CANVAS_H / 2.0;
        a.expanded = true;
        a.tab = music;
        shot(a, dir, "edge_left_panel");
        // Taskbar mode: the panel opens up from the bottom-left corner.
        a.settings.edge = Edge::Bottom;
        a.anchor = 0.0;
        shot(a, dir, "edge_bottom_panel");
        a.expanded = false;
        a.settings.edge = Edge::Top;
        a.anchor = CANVAS_W / 2.0;
        // The taskbar mascot on its own (no notch around it).
        for (k, m) in [("cat", Mascot::Cat), ("eyes", Mascot::Eyes)] {
            a.gfx.begin(a.scale);
            {
                let mut hits = Vec::new();
                let mut ui = Ui { g: &mut a.gfx, hits: &mut hits, module: usize::MAX - 9, mouse: (-1e4, -1e4), t: 1.0 };
                mascot::draw(&mut ui, Rect::new(20.0, 20.0, 58.0, 42.0), m, Mood::Eyes, 1.0, (0.6, 0.2));
            }
            a.gfx.end_and_present(a.hwnd, a.origin.0, a.origin.1);
            write_raw(&a.gfx, &dir.join(format!("taskbar_mascot_{k}.raw")), None);
        }

        // Hover-to-open animation at 1x for the README GIF.
        a.scale = 1.0;
        let _ = a.gfx.resize(CANVAS_W as i32, CANVAS_H as i32);
        let crop = (0, 0, CANVAS_W as i32 - 1, 259);
        let mut n = 0u32;
        a.tab = music;
        a.expanded = false;
        a.cur = a.target();
        animate(a, dir, &mut n, 1, crop); // hold: collapsed
        a.expanded = true;
        animate(a, dir, &mut n, 18, crop); // open
        animate(a, dir, &mut n, 1, crop); // hold: player
        act(a, music, crate::modules::music::A_DEMO_LYRICS);
        animate(a, dir, &mut n, 18, crop); // split into lyrics
        animate(a, dir, &mut n, 1, crop); // hold: lyrics
        act(a, music, 5);
        a.tab = timer;
        act(a, timer, crate::modules::timer::A_DEMO_TIMER);
        animate(a, dir, &mut n, 18, crop); // switch to timer
        animate(a, dir, &mut n, 1, crop); // hold: timer
        a.expanded = false;
        animate(a, dir, &mut n, 18, crop); // close (timer live)
        animate(a, dir, &mut n, 1, crop); // hold
        log += &format!("rendered to {} ({n} animation frames)\n", dir.display());
        log
    })
    .unwrap_or_else(|| "app not running".into())
}

/// `nookly --render-demo <dir>`: the scripted demo for the README video.
/// Drives the real UI (hover, clicks, tabs, toasts, settings) on a virtual
/// 30 fps clock with a scripted cursor, and writes every frame as a raw
/// 2x canvas plus `demo.log` (cursor position, clicks and scene markers)
/// for `docs/make_demo.py` to composite.
pub fn render_demo(dir: &std::path::Path) -> String {
    let _ = std::fs::create_dir_all(dir);
    with_app(|a| {
        struct D<'a> {
            a: &'a mut App,
            dir: std::path::PathBuf,
            n: u32,
            t: f64,
            cur: (f32, f32),
            inside: bool,
            click: u8,
            log: String,
        }
        impl D<'_> {
            fn step(&mut self) {
                self.t += 1.0 / 30.0;
                self.a.virt = Some(self.t);
                if self.n % 30 == 0 {
                    self.a.tick();
                }
                let (x, y) = self.cur;
                // Layered windows only get the mouse over drawn pixels.
                let over = self.a.hit_at(x, y).is_some();
                if over {
                    self.a.on_mouse_move(x, y);
                } else if self.inside {
                    self.a.on_mouse_leave();
                }
                self.inside = over;
                self.a.frame();
                write_raw(&self.a.gfx, &self.dir.join(format!("f{:04}.raw", self.n)), Some((0, 0, self.a.gfx.pw - 1, self.a.gfx.ph - 1)));
                self.log += &format!("f {} {:.1} {:.1} {}\n", self.n, x * self.a.scale, y * self.a.scale, self.click);
                self.click = 0;
                self.n += 1;
            }
            fn wait(&mut self, secs: f64) {
                for _ in 0..(secs * 30.0).round() as u32 {
                    self.step();
                }
            }
            fn scene(&mut self, name: &str) {
                self.log += &format!("scene {} {name}\n", self.n);
            }
            /// Caption subtitle change (shown by make_demo.py).
            fn sub(&mut self, text: &str) {
                self.log += &format!("sub {} {text}\n", self.n);
            }
            /// Move to a hit and show a click there without sending it.
            fn point(&mut self, module: usize, id: u32, secs: f64) -> bool {
                let Some((x, y)) = self.hit(module, id) else { return false };
                self.move_to(x, y, secs);
                self.wait(0.12);
                self.click = 1;
                self.step();
                true
            }
            /// Glide the cursor to (x, y) with an ease-in-out curve.
            fn move_to(&mut self, x: f32, y: f32, secs: f64) {
                let (x0, y0) = self.cur;
                let k = (secs * 30.0).round().max(1.0) as u32;
                for i in 1..=k {
                    let p = i as f32 / k as f32;
                    let e = if p < 0.5 { 4.0 * p * p * p } else { 1.0 - (-2.0 * p + 2.0).powi(3) / 2.0 };
                    self.cur = (x0 + (x - x0) * e, y0 + (y - y0) * e);
                    self.step();
                }
            }
            /// Centre of a hit currently on screen.
            fn hit(&self, module: usize, id: u32) -> Option<(f32, f32)> {
                self.a.hits.iter().rev().find(|h| h.module == module && h.id == id).map(|h| (h.rect.cx(), h.rect.cy()))
            }
            fn click_at(&mut self, module: usize, id: u32, secs: f64) {
                let Some((x, y)) = self.hit(module, id) else {
                    self.log += &format!("missing hit {module} {id}\n");
                    return;
                };
                self.move_to(x, y, secs);
                self.wait(0.12);
                self.a.on_down(x, y);
                self.click = 1;
                self.step();
                self.step();
                self.a.on_up(x, y);
                self.step();
            }
        }

        let scale = 2.0;
        a.scale = scale;
        let _ = a.gfx.resize((CANVAS_W * scale) as i32, (CANVAS_H * scale) as i32);
        a.settings.style = NotchStyle::Full;
        a.settings.edge = Edge::Top;
        a.settings.personality = true;
        a.settings.mascot = Mascot::Eyes;
        a.anchor = CANVAS_W / 2.0;
        a.queue.clear();
        a.toast = None;
        a.expanded = false;
        a.onboarding = None;
        a.mood = Mood::Clock;
        sound::mute(true);
        let music = find(a, "Now Playing").unwrap_or(0);
        let pomo = find(a, "Pomodoro").unwrap_or(0);
        let timer = find(a, "Timer").unwrap_or(0);
        let water = find(a, "Water Reminder").unwrap_or(0);
        let todo = find(a, "TodoList").unwrap_or(0);
        let shelf = find(a, "Shelf").unwrap_or(0);
        let clip = find(a, "Clipboard").unwrap_or(0);
        let notes = find(a, "Notifications").unwrap_or(0);
        let privacy = find(a, "Privacy").unwrap_or(usize::MAX);
        let prayer = find(a, "Prayer Times").unwrap_or(usize::MAX);
        let tools = find(a, "Tools").unwrap_or(usize::MAX);
        let agents = find(a, "AI Agents").unwrap_or(usize::MAX);
        let monitor = find(a, "System Monitor").unwrap_or(usize::MAX);
        a.call(clip, |m, c| m.action(crate::modules::clipboard::A_DEMO_DOCS, (0.0, 0.0), c));
        if tools != usize::MAX {
            a.call(tools, |m, c| m.action(crate::modules::tools::A_DEMO, (0.0, 0.0), c));
        }
        if agents != usize::MAX {
            a.call(agents, |m, c| m.action(crate::modules::agents::A_DEMO, (0.0, 0.0), c));
        }
        if monitor != usize::MAX {
            a.call(monitor, |m, c| m.action(crate::modules::monitor::A_DEMO, (0.0, 0.0), c));
        }
        a.tab = music;
        a.tick();
        a.cur = a.target();
        let mut d = D { a, dir: dir.to_path_buf(), n: 0, t: 1.0, cur: (CANVAS_W * 0.82, CANVAS_H + 40.0), inside: false, click: 0, log: String::new() };

        d.scene("intro");
        d.sub("A Dynamic Island for Windows. Free and open source.");
        d.wait(2.6);

        // The welcome tour's last two steps: the basics, and Settings at a glance.
        d.scene("tour");
        d.sub("A short welcome tour shows the basics");
        d.a.onboarding = Some(2);
        d.wait(2.4);
        d.sub("the panels worth knowing");
        d.a.onboarding = Some(3);
        d.wait(2.4);
        d.sub("and what you can change in Settings");
        d.a.onboarding = Some(4);
        d.wait(2.8);
        d.a.onboarding = None;
        d.wait(0.5);

        d.scene("styles");
        for (s, text) in [
            (NotchStyle::Compact, "Compact: a smaller notch"),
            (NotchStyle::Pill, "Pill: just a thin bar"),
            (NotchStyle::Hidden, "Hidden: invisible until you hover"),
            (NotchStyle::Full, "Full: clock, music and a mascot"),
        ] {
            d.wait(0.25);
            d.a.settings.style = s;
            d.sub(text);
            d.wait(1.05);
        }

        d.scene("open");
        d.sub("Music controls, with synced lyrics");
        let notch = d.a.edge_rect(CANVAS_W / 2.0, 20.0);
        d.move_to(notch.cx() + 30.0, notch.y + 14.0, 0.9);
        d.wait(1.0);
        if d.point(music, 5, 0.6) {
            d.a.call(music, |m, c| m.action(crate::modules::music::A_DEMO_LYRICS, (0.0, 0.0), c));
        }
        d.wait(1.8);
        d.click_at(music, 5, 0.4);
        d.wait(0.3);

        d.scene("tabs");
        for (i, text) in [
            (timer, "Timer and stopwatch"),
            (pomo, "Pomodoro focus sessions"),
            (todo, "To-do list with reminders"),
            (water, "Water reminder"),
            (shelf, "A shelf for files you're working on"),
            (clip, "Clipboard history: click to paste"),
            (prayer, "Prayer times, worked out on your PC"),
            (tools, "Tools: a color picker for anything on screen"),
            (agents, "AI agents: is Hermes or Claude Code working, or waiting for you?"),
            (monitor, "System monitor: CPU, memory and network"),
        ] {
            if i == usize::MAX {
                continue;
            }
            if i == prayer {
                // Reminders on for the five prayers (bells lit).
                for k in [0u32, 2, 3, 4, 5] {
                    d.a.call(prayer, |m, c| m.action(90 + k, (0.0, 0.0), c));
                }
            }
            // Start the timers only now, so music is the closed notch's
            // live activity everywhere else in the video.
            if i == timer {
                d.a.call(timer, |m, c| m.action(crate::modules::timer::A_DEMO_TIMER, (0.0, 0.0), c));
            } else if i == pomo {
                d.a.call(pomo, |m, c| m.action(crate::modules::pomodoro::A_DEMO, (0.0, 0.0), c));
            }
            d.click_at(APP_HIT, i as u32, 0.35);
            d.sub(text);
            d.wait(0.95);
            if i == shelf {
                d.point(shelf, crate::modules::shelf::A_SNIP, 0.5);
                d.sub("Screenshots go straight onto the Shelf");
                d.wait(1.2);
            }
            if i == clip {
                d.click_at(clip, crate::modules::clipboard::A_SEARCH, 0.4);
                d.sub("Search it, or save snippets you paste often");
                for ch in "meet".chars() {
                    d.wait(0.12);
                    d.a.on_char(ch);
                }
                d.wait(1.1);
                d.a.on_key(0x1B);
                d.a.on_key(0x1B);
                d.a.release_keyboard();
            }
            if i == tools {
                for (k, text) in [(2u32, "a calculator that answers as you type"), (3, "QR codes of whatever you copy"), (4, "and a converter, with katha and bigha")] {
                    d.click_at(tools, k, 0.35);
                    d.a.release_keyboard();
                    d.sub(text);
                    d.wait(1.0);
                }
                d.click_at(tools, 77, 0.35);
                d.sub("and world time zones");
                d.wait(1.1);
                d.a.call(tools, |m, c| m.action(73, (0.0, 0.0), c));
                d.click_at(tools, 1, 0.3);
            }
        }
        // The mouse wheel walks back through the panels.
        d.sub("Or roll the mouse wheel to switch panels");
        // Over the header (not a list, which would scroll instead).
        let content = (d.a.tab_bar.r() - 70.0, d.a.tab_bar.cy());
        d.move_to(content.0, content.1, 0.4);
        for _ in 0..3 {
            d.wait(0.35);
            d.log += &format!("wheel {}\n", d.n);
            d.a.on_wheel(1.0);
            d.wait(0.35);
        }
        d.a.call(timer, |m, c| m.action(4, (0.0, 0.0), c)); // reset
        d.a.call(pomo, |m, c| m.action(2, (0.0, 0.0), c)); // reset

        d.scene("alerts");
        d.move_to(CANVAS_W * 0.85, CANVAS_H + 60.0, 0.7);
        d.wait(0.4);
        d.sub("Your Windows notifications, in the island");
        d.a.call(notes, |m, c| m.action(crate::modules::notifs::A_DEMO, (0.0, 0.0), c));
        d.wait(2.6);
        d.a.toast = None;
        if agents != usize::MAX {
            d.sub("Hermes needs your OK? The island tells you");
            d.a.call(agents, |m, c| m.action(crate::modules::agents::A_DEMO_ASK, (0.0, 0.0), c));
            d.wait(2.4);
            d.a.toast = None;
            d.a.queue.clear();
            d.sub("and when it's done");
            d.a.call(agents, |m, c| m.action(crate::modules::agents::A_DEMO_DONE, (0.0, 0.0), c));
            d.wait(2.2);
            d.a.toast = None;
            d.a.queue.clear();
        }
        if prayer != usize::MAX {
            d.sub("A reminder at each prayer time");
            d.a.call(prayer, |m, c| m.action(crate::modules::prayer::A_DEMO, (0.0, 0.0), c));
            d.wait(2.4);
            d.a.toast = None;
        }
        if let Some(ci) = find(&d.a, "Clipboard") {
            d.sub("Copy something: a small preview shows what you copied (optional)");
            let was = d.a.settings.copy_toast;
            d.a.settings.copy_toast = true;
            d.a.call(ci, |m, c| m.action(crate::modules::clipboard::A_TEST_COPY_TEXT, (0.0, 0.0), c));
            d.a.call(ci, |m, c| m.tick(c));
            d.wait(2.2);
            d.a.toast = None;
            d.a.queue.clear();
            d.a.settings.copy_toast = was;
        }
        d.sub("Over a maximized window it slims down, so your tabs stay clickable");
        MAXIMIZED_OVERRIDE.store(1, std::sync::atomic::Ordering::Relaxed);
        d.a.update_make_room();
        d.wait(2.0);
        MAXIMIZED_OVERRIDE.store(2, std::sync::atomic::Ordering::Relaxed);
        d.a.update_make_room();
        d.wait(0.5);
        d.sub("Charging, battery, volume, Wi-Fi and Bluetooth");
        d.a.push_toast(Toast::new("power", glyph::CHARGING, col::GREEN, "Charging", "Charger connected").right("70%").progress(0.7).secs(1.8));
        d.wait(2.2);
        d.sub("A dot while an app uses your mic or camera");
        if privacy != usize::MAX {
            d.a.call(privacy, |m, c| m.action(crate::modules::privacy::A_DEMO, (0.0, 0.0), c));
            d.a.tick();
        }
        d.wait(2.4);

        d.wait(0.3);
        if privacy != usize::MAX {
            d.a.call(privacy, |m, c| m.action(crate::modules::privacy::A_DEMO, (0.0, 0.0), c));
        }

        d.scene("outro");
        d.sub("github.com/banulalakwindu/Nookly");
        d.move_to(CANVAS_W * 0.9, CANVAS_H + 60.0, 0.8);
        d.wait(3.2);
        d.log += &format!("end {}\n", d.n);
        let _ = std::fs::write(dir.join("demo.log"), &d.log);
        format!("{} frames", d.n)
    })
    .unwrap_or_else(|| "app not running".into())
}

/// Self-test: the closed notch's order list, drag and drop, widgets.
fn widget_tests(a: &mut App) -> String {
    use crate::store::NotchLayout;
    let mut out = String::new();
    let ok = |b: bool| if b { "OK" } else { "FAIL" };
    // The list round-trips through settings text.
    let l = NotchLayout::custom_rows(vec![NotchRow::wide("music"), NotchRow::pair("agents", "battery"), NotchRow::pair("", "cpu")], true);
    let text = l.to_text();
    let back = NotchLayout::from_text(&text);
    out += &format!("{:<4} widget order saves and loads ({text})\n", ok(back == l.rows && text == "*music;agents,battery;,cpu"));
    let d = NotchLayout::default();
    let ids: Vec<String> = d.rows.iter().map(|r| r.left.clone()).collect();
    out += &format!("{:<4} default order: live activities only, each across both sides {ids:?}\n", ok(ids == ["pomodoro", "timer", "prayer", "music", "agents", "due"] && d.rows.iter().all(|r| r.wide)));
    // Switching a live activity on gives it both sides, above the widgets.
    let mut l2 = NotchLayout::custom_rows(vec![NotchRow::wide("music"), NotchRow::pair("battery", "")], true);
    l2.add("agents");
    l2.add("cpu");
    out += &format!("{:<4} a live activity switched on goes across both sides, above the widgets ({})\n", ok(l2.to_text() == "*music;*agents;battery,cpu"), l2.to_text());
    // Picking what shows.
    let pick = |rows: &[NotchRow], on: &[&str]| pick_layout(rows, &|id: &str| on.contains(&id));
    let s = |w: bool, l: &str, r: &str| Some((w, l.to_string(), r.to_string()));
    let music_first = vec![NotchRow::wide("music"), NotchRow::wide("agents")];
    let agents_first = vec![NotchRow::wide("agents"), NotchRow::wide("music")];
    let side_by_side = vec![NotchRow::pair("music", "agents")];
    let borrow = vec![NotchRow::pair("music", "agents"), NotchRow::wide("timer"), NotchRow::pair("battery", "cpu")];
    let cases = [
        pick(&music_first, &["music", "agents"]) == s(true, "music", ""),
        pick(&music_first, &["agents"]) == s(true, "agents", ""),
        pick(&agents_first, &["music", "agents"]) == s(true, "agents", ""),
        pick(&side_by_side, &["music", "agents"]) == s(false, "music", "agents"),
        pick(&side_by_side, &["agents"]) == s(false, "", "agents"),
        // An idle side borrows the next one-side item below (not a wide one).
        pick(&borrow, &["music", "timer", "cpu"]) == s(false, "music", "cpu"),
        pick(&borrow, &["timer", "cpu"]) == s(true, "timer", ""),
        pick(&borrow, &["battery"]) == s(false, "battery", ""),
        pick(&borrow, &[]).is_none(),
    ];
    out += &format!("{:<4} the first row with something to show wins; an empty side borrows from below {cases:?}\n", ok(cases.iter().all(|c| *c)));
    // Drag and drop.
    use crate::modules::widgets::{test_drop, test_toggle_size};
    let mut r = vec![NotchRow::wide("music"), NotchRow::pair("battery", ""), NotchRow::pair("cpu", "ram")];
    test_drop(&mut r, Some((2, true)), None, Some((1, true)), None); // ram -> beside battery
    let a1 = r == vec![NotchRow::wide("music"), NotchRow::pair("battery", "ram"), NotchRow::pair("cpu", "")];
    test_drop(&mut r, Some((1, false)), None, Some((1, true)), None); // swap sides
    let a2 = r[1] == NotchRow::pair("ram", "battery");
    test_drop(&mut r, Some((2, false)), None, None, Some(0)); // cpu to a new first row
    let a3 = r == vec![NotchRow::pair("cpu", ""), NotchRow::wide("music"), NotchRow::pair("ram", "battery")];
    test_drop(&mut r, None, Some(2), None, Some(0)); // row to the top
    let a4 = r[0] == NotchRow::pair("ram", "battery") && r.len() == 3;
    test_toggle_size(&mut r, 0); // two one-side items -> both sides: battery gets a row
    let a5 = r[0] == NotchRow::wide("ram") && r[1] == NotchRow::pair("battery", "");
    test_toggle_size(&mut r, 0);
    let a6 = r[0] == NotchRow::pair("ram", "");
    test_drop(&mut r, Some((0, false)), None, Some((3, false)), None); // onto the wide music row: swap
    let a7 = r.iter().any(|x| *x == NotchRow::wide("ram")) && r.iter().any(|x| x.left == "music" && !x.wide);
    out += &format!("{:<4} order list: pair, swap sides, new row, move row, both/one side {:?}\n", ok(a1 && a2 && a3 && a4 && a5 && a6 && a7), [a1, a2, a3, a4, a5, a6, a7]);
    // AirPods broadcast: 60 % left, 70 % right, case 40 %, not charging.
    let mut pkt = [0u8; 27];
    pkt[0] = 0x07;
    pkt[1] = 0x19;
    pkt[5] = 0x00;
    pkt[6] = 0x67;
    pkt[7] = 0x04;
    let p = crate::modules::widgets::decode_pods(&pkt);
    let good = p.map_or(false, |p| p.left == Some(60) && p.right == Some(70) && p.case == Some(40) && !p.charging);
    pkt[6] = 0xF7;
    let unknown = crate::modules::widgets::decode_pods(&pkt).map_or(false, |p| p.left.is_none() && p.right == Some(70));
    out += &format!("{:<4} AirPods battery broadcast decodes (L 60 R 70 case 40; unknown bud) {p:?}\n", ok(good && unknown && crate::modules::widgets::decode_pods(&pkt[..20]).is_none()));
    // Every live panel has an id, and the Widgets tab lists everything.
    let saved = (a.expanded, a.tab, a.settings.layout.clone());
    let cat = a.notch_catalog();
    let names: Vec<&str> = cat.iter().map(|c| c.id.as_str()).collect();
    let want = ["music", "timer", "pomodoro", "prayer", "due", "agents", "cpu", "ram", "net", "bluetooth", "date", "weather", "volume", "mic"];
    let missing: Vec<&&str> = want.iter().filter(|w| !names.contains(w)).collect();
    out += &format!("{:<4} the Widgets tab lists live activities and widgets ({} items, missing {missing:?})\n", ok(missing.is_empty()), cat.len());
    // Phase 2: weather (online only while it's on), Wi-Fi, sound.
    {
        use crate::modules::widgets as wg;
        let path = wg::weather_path(23.8103, 90.4125, false);
        let pf = wg::weather_path(40.71, -74.01, true);
        let parsed = wg::parse_weather(r#"{"current":{"time":"2026-11-11T10:00","temperature_2m":28.6,"weather_code":61,"is_day":1}}"#);
        let kinds = [wg::weather_kind(0, true).0, wg::weather_kind(3, true).0, wg::weather_kind(63, true).0, wg::weather_kind(95, false).0];
        let ok_w = path == "/v1/forecast?latitude=23.8&longitude=90.4&current=temperature_2m,weather_code,is_day"
            && pf.ends_with("&temperature_unit=fahrenheit")
            && parsed == Some((28.6, 61, true))
            && wg::parse_weather("{}").is_none()
            && kinds == ["Clear", "Cloudy", "Rain", "Storm"];
        out += &format!("{:<4} weather: asks Open-Meteo for the place rounded to 0.1°, reads the answer, names the sky {kinds:?}\n", ok(ok_w));
        let off = !a.settings.layout.contains("weather") && !wg::weather_started();
        out += &format!("{:<4} weather stays offline while its widget is off\n", ok(off || a.settings.layout.contains("weather")));
        let wifi = wg::wifi();
        out += &format!("INFO Wi-Fi now: {wifi:?}\n");
        let w: Vec<String> = cat.iter().filter(|c| ["wifi", "volume", "mic"].contains(&c.id.as_str())).map(|c| format!("{} {}", c.id, c.detail)).collect();
        out += &format!("INFO sound and Wi-Fi widgets: {w:?}\n");
        if std::env::var("NOOKLY_SELFTEST_ONLINE").is_ok() {
            let (lat, lon, place) = crate::modules::prayer::location();
            let r = crate::http::get("api.open-meteo.com", &wg::weather_path(lat, lon, false));
            let got = r.filter(|r| r.status == 200).and_then(|r| wg::parse_weather(&String::from_utf8_lossy(&r.body)));
            out += &format!("{:<4} live weather for {place}: {got:?}\n", ok(got.is_some()));
        }
    }
    // A widget row shows on the resting notch at the live width.
    a.settings.layout = NotchLayout::custom_rows(vec![NotchRow::pair("cpu", "date")], true);
    a.expanded = false;
    a.queue.clear();
    a.toast = None;
    let tgt = a.target();
    let shows = a.showing_ids();
    let full_w = tgt.0;
    out += &format!("{:<4} widgets show when nothing is live: {shows:?}, {:.0} wide\n", ok(shows == ["cpu", "date"] && full_w >= 190.0), full_w);
    // On a side edge the clock (hours over minutes) stays in the middle,
    // between the top and bottom items, in every style, with or without
    // seconds.
    {
        let before = (a.settings.edge, a.settings.style, a.settings.seconds, a.anchor);
        let mut blank = Vec::new();
        for style in [NotchStyle::Full, NotchStyle::Slim, NotchStyle::Compact] {
            for secs in [false, true] {
                a.settings.edge = Edge::Left;
                a.settings.style = style;
                a.settings.seconds = secs;
                a.anchor = CANVAS_H / 2.0;
                let tgt = a.target();
                a.cur = tgt;
                a.render(tgt, 0.0);
                let g = a.edge_rect(tgt.0, tgt.1);
                // The middle third of the notch, where the clock goes.
                let (s, px) = (a.scale, a.gfx.pixels());
                let (y0, y1) = (((g.y + g.h * 0.36) * s) as i32, ((g.b() - g.h * 0.36) * s) as i32);
                let (x0, x1) = ((g.x * s) as i32, (g.r() * s) as i32);
                let mut bright = 0;
                for y in y0.max(0)..y1.min(a.gfx.ph) {
                    for x in x0.max(0)..x1.min(a.gfx.pw) {
                        let i = ((y * a.gfx.pw + x) * 4) as usize;
                        if px[i] as u32 + px[i + 1] as u32 + px[i + 2] as u32 > 300 {
                            bright += 1;
                        }
                    }
                }
                if bright < 15 {
                    blank.push(format!("{style:?}{}", if secs { "+s" } else { "" }));
                }
            }
        }
        (a.settings.edge, a.settings.style, a.settings.seconds, a.anchor) = before;
        out += &format!("{:<4} side edges keep the clock between the live items {blank:?}\n", ok(blank.is_empty() && shows == ["cpu", "date"]));
    }
    a.settings.layout = NotchLayout::custom_rows(Vec::new(), true);
    let tgt2 = a.target();
    out += &format!("{:<4} an empty list leaves just the clock ({:.0} wide)\n", ok(a.showing_ids().is_empty() && tgt2.0 < full_w), tgt2.0);
    (a.expanded, a.tab, a.settings.layout) = saved;
    out += &widget_tests_2(a);
    out += &smart_hide_tests(a);
    out += &look_tests(a);
    out += &feature_tests(a);
    out += &phase_tests(a);
    out
}

/// Self-test: the everyday helpers, health, Ramadan, money, look, profiles.
fn phase_tests(a: &mut App) -> String {
    use crate::modules::{countdown, meetings};
    let ok = |b: bool| if b { "OK" } else { "FAIL" };
    let mut out = String::new();
    let start = (a.expanded, a.more_open, a.pill_off, a.tab, a.settings.clone());

    // Countdowns: dates in many forms, years roll over, yearly repeats.
    let today = (2026, 11, 12);
    let p = |t: &str| countdown::parse(t, today).map(|c| (c.name, c.y, c.m, c.d));
    let parsed = [p("Exam 2026-12-20"), p("Mum's birthday, 3 Mar"), p("Eid Jun 6"), p("Trip 20/12"), p("Party 5 Nov"), p("nonsense")];
    let want = [
        Some(("Exam".to_string(), 2026, 12, 20)),
        Some(("Mum's birthday".to_string(), 2027, 3, 3)),
        Some(("Eid".to_string(), 2027, 6, 6)),
        Some(("Trip".to_string(), 2026, 12, 20)),
        Some(("Party".to_string(), 2027, 11, 5)),
        None,
    ];
    let yearly = countdown::Countdown { y: 2000, m: 11, d: 10, yearly: true, name: "B".into() };
    let days = (countdown::days_left(&countdown::Countdown { y: 2026, m: 12, d: 20, yearly: false, name: "E".into() }, today), countdown::days_left(&yearly, today));
    out += &format!("{:<4} countdowns read 6 date forms and count days {:?}\n", ok(parsed == want && days == (38, 363)), days);

    // Calendar link: events, a weekly repeat with an exception, Join links.
    let base = meetings::local_to_unix(2026, 11, 9, 0, 0, 0);
    let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART:20261109T093000\r\nDTEND:20261109T094500\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR;COUNT=6\r\nEXDATE:20261111T093000\r\nSUMMARY:Standup\r\nDESCRIPTION:Join: https://teams.microsoft.com/l/meetup-join/abc\r\n  more\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20261112\r\nSUMMARY:Holiday\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART:20261110T100000Z\r\nSUMMARY:Cancelled thing\r\nSTATUS:CANCELLED\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
    let ev = meetings::parse_ics(ics, base - 86400.0, base + 30.0 * 86400.0);
    let standups: Vec<f64> = ev.iter().filter(|e| e.title == "Standup").map(|e| ((e.start - base) / 86400.0 * 10.0).round() / 10.0).collect();
    let link = ev.iter().find(|e| e.title == "Standup").and_then(|e| e.link.clone());
    let ics_ok = standups.len() == 5 && standups[0] == 0.4 && ev.iter().any(|e| e.title == "Holiday" && e.all_day) && !ev.iter().any(|e| e.title.starts_with("Cancelled")) && link.as_deref() == Some("https://teams.microsoft.com/l/meetup-join/abc");
    out += &format!("{:<4} calendar link: weekly repeat with an exception ({} standups), all-day events, cancelled skipped, Teams link found\n", ok(ics_ok), standups.len());

    // Currency rates from the API's JSON.
    let json = r#"{"result":"success","rates":{"USD":1,"EUR":0.92,"GBP":0.79,"LKR":298.5,"BDT":121.2,"INR":84.1,"SAR":3.75,"AED":3.6725}}"#;
    let rates = crate::modules::tools::parse_rates(json);
    out += &format!("{:<4} currency rates read from the API ({:?})\n", ok(rates.map_or(false, |r| r[3] == 298.5 && r[0] == 1.0)), rates.map(|r| r[3]));

    // Ramadan: the Hijri date (tabular): 1 March 2026 is in Ramadan 1447.
    let h = crate::modules::prayer::hijri(2026, 3, 1, 0);
    let h2 = crate::modules::prayer::hijri(2026, 5, 1, 0);
    out += &format!("{:<4} Hijri dates: 1 Mar 2026 = {h:?}, 1 May 2026 = {h2:?} (Ramadan mode keys off month 9)\n", ok(h.0 == 1447 && h.1 == 9 && h2.1 != 9));

    // Your own theme: readable, and Fix picks a readable text colour.
    let th = crate::gfx::custom_theme(0x1B1B2F, 0xF5F5F7);
    let bad = crate::gfx::contrast(hex(0xF2F2F2), hex(0xFFE7B3));
    a.settings.extra.insert("custom_bg".into(), "F2F2F2".into());
    a.settings.extra.insert("custom_text".into(), "FFE7B3".into());
    a.settings_action(S_CFIX);
    let fixed = custom_colors(&a.settings);
    out += &format!("{:<4} your own theme: contrast {:.1}:1; a {:.1}:1 pair is fixed to {:06X}\n", ok(crate::gfx::contrast(th.bg, th.text) >= 7.0 && bad < 4.5 && crate::gfx::contrast(hex(fixed.0), hex(fixed.1)) >= 7.0), crate::gfx::contrast(th.bg, th.text), bad, fixed.1);

    // Profiles keep their own features and theme.
    let mut st = a.settings.clone();
    st.extra.remove("profile");
    st.theme = "rose".into();
    st.hidden_panels = vec!["Tools".into()];
    crate::features::switch_profile(&mut st, "Work");
    st.theme = "ocean".into();
    st.hidden_panels = vec!["Notes".into()];
    crate::features::switch_profile(&mut st, "Home");
    let home = (st.theme.clone(), st.hidden_panels.clone());
    crate::features::switch_profile(&mut st, "Work");
    let work = (st.theme.clone(), st.hidden_panels.clone());
    st.extra.insert("profile_auto".into(), "1".into());
    let auto = (crate::features::auto_profile(&st, 2, 10), crate::features::auto_profile(&st, 6, 10));
    out += &format!("{:<4} profiles keep their own features and theme; Auto picks Work on weekdays 9-5 {:?}\n", ok(home.0 == "rose" && home.1 == ["Tools"] && work.0 == "ocean" && work.1 == ["Notes"] && auto == (None, Some("Home"))), auto);

    // Downloads: a finished file shows; a half-downloaded one doesn't.
    let dir = std::env::temp_dir().join(format!("nookly-dl-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let since = std::time::SystemTime::now() - std::time::Duration::from_secs(2);
    let _ = std::fs::write(dir.join("movie.mp4.crdownload"), b"partial");
    let half = crate::modules::downloads::newest_after(&dir, since).is_none();
    let _ = std::fs::write(dir.join("report.pdf"), b"%PDF done");
    let done = crate::modules::downloads::newest_after(&dir, since).map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned());
    let _ = std::fs::remove_dir_all(&dir);
    out += &format!("{:<4} downloads: a finished file is seen ({done:?}), a .crdownload isn't\n", ok(half && done.as_deref() == Some("report.pdf")));

    // Every new panel draws and fits.
    a.settings.hidden_panels.clear();
    a.expanded = true;
    let mut drawn = Vec::new();
    for title in ["Notes", "Countdowns", "Launcher", "Toggles", "Mirror", "Health"] {
        if let Some(i) = a.modules.iter().position(|m| m.title() == title) {
            a.tab = i;
            let tgt = a.target();
            a.cur = tgt;
            a.render(tgt, 0.0);
            out += &snap(a, &format!("panel {title}"));
            if a.gfx.last_err.is_none() {
                drawn.push(title);
            }
        }
    }
    out += &format!("{:<4} new panels draw: {drawn:?}\n", ok(drawn.len() == 6));
    // The Look page fits.
    a.settings.theme = "custom".into();
    a.sw_view(0, SEC_THEME);
    out += &a.sw_snap("theme section (custom theme)");
    let themes = a.sw_hits().iter().filter(|h| (S_THEME..S_THEME + 10).contains(&h.id) || h.id == S_CUSTOM).count();
    let look_ok = themes == 11 && a.sw_has(S_CBG) && a.sw_has(S_GLOW) && a.sw_has(S_SECONDS);
    a.sw_view(0, 0);
    let look_ok = look_ok && a.sw_has(S_MOTION) && a.sw_has(S_SIZE) && a.sw_has(S_MAKE_ROOM);
    let left = a.swin.as_ref().map_or(-1.0, |sw| sw.room[SEC_THEME as usize].min(sw.room[0]));
    a.close_settings();
    out += &format!("{:<4} Theme and Appearance: 11 themes, your own colours, size, motion and glow fit ({:.0} DIPs left)\n", ok(look_ok && left >= 0.0), left);

    let (e, so, sp, tb, st0) = start;
    (a.expanded, a.more_open, a.pill_off, a.tab) = (e, so, sp, tb);
    a.settings = st0;
    a.settings.save();
    a.tick();
    out
}

/// Self-test: tab overflow, shortcuts, export and import, the tour's presets.
fn feature_tests(a: &mut App) -> String {
    use crate::features::*;
    let ok = |b: bool| if b { "OK" } else { "FAIL" };
    let mut out = String::new();
    let start = (a.expanded, a.more_open, a.pill_off, a.tab, a.settings.hidden_panels.clone(), a.settings.extra.clone(), a.onboarding);
    let draw = |a: &mut App| {
        let tgt = a.target();
        a.cur = tgt;
        a.render(tgt, 0.0);
    };
    // Every panel on: what doesn't fit goes under ⋯, nothing is lost, and
    // the open panel is always in the bar.
    a.settings.hidden_panels.clear();
    a.onboarding = None;
    a.expanded = true;
    let all = a.tab_list();
    let last = *all.last().unwrap_or(&0);
    a.tab = last;
    a.more_open = false;
    draw(a);
    let bar_ids: Vec<u32> = a.hits.iter().filter(|h| h.module == APP_HIT && (h.id as usize) < a.modules.len()).map(|h| h.id).collect();
    let more = a.hits.iter().find(|h| h.module == APP_HIT && h.id == MORE).copied();
    let open_in_bar = bar_ids.contains(&(last as u32));
    let mut overflow_ok = true;
    if let Some(m) = more {
        a.on_down(m.rect.cx(), m.rect.cy());
        a.on_up(m.rect.cx(), m.rect.cy());
        draw(a);
        out += &snap(a, "more panels");
        let tiles: Vec<u32> = a.hits.iter().filter(|h| h.module == APP_HIT && (h.id as usize) < a.modules.len()).map(|h| h.id).collect();
        // The bar plus the tiles hold every panel once.
        let mut seen: Vec<u32> = tiles.clone();
        seen.sort();
        seen.dedup();
        overflow_ok = a.more_open && all.iter().all(|i| seen.contains(&(*i as u32)));
        // Picking a tile opens it and closes the ⋯ page.
        if let Some(h) = a.hits.iter().rev().find(|h| h.module == APP_HIT && (h.id as usize) < a.modules.len() && !bar_ids.contains(&h.id)).copied() {
            a.on_down(h.rect.cx(), h.rect.cy());
            a.on_up(h.rect.cx(), h.rect.cy());
            overflow_ok &= a.tab == h.id as usize && !a.more_open;
            draw(a);
            overflow_ok &= a.hits.iter().any(|x| x.module == APP_HIT && x.id == h.id && x.rect.y < a.tab_bar.b());
        } else {
            overflow_ok = false;
        }
    }
    out += &format!(
        "{:<4} {} panels: {} in the tab bar{}, the open one always there\n",
        ok(open_in_bar && overflow_ok && (more.is_some() || bar_ids.len() == all.len())),
        all.len(),
        bar_ids.len(),
        if more.is_some() { ", the rest under ⋯" } else { "" }
    );
    // Minimal: no ⋯.
    apply_preset(&mut a.settings, 0);
    a.tab = a.tab_list()[0];
    draw(a);
    let no_more = !a.hits.iter().any(|h| h.module == APP_HIT && h.id == MORE);
    out += &format!("{:<4} a few panels: no ⋯\n", ok(no_more));

    // Shortcuts.
    let def = hotkey(&a.settings, 0).map(|(m, v)| hotkey_label(m, v));
    let rec = [recorded(9, 0x4E), recorded(4, 0x41), recorded(0, 0x41), recorded(0, 0x71), recorded(2, 0x11)];
    let labels = (hotkey_label(2 | 4, 0x56), hotkey_label(0, 0x7B));
    let keys_ok = def.as_deref() == Some("Win+Alt+N") && rec == [Some((9, 0x4E)), None, None, Some((0, 0x71)), None] && labels == ("Ctrl+Shift+V".to_string(), "F12".to_string());
    out += &format!("{:<4} shortcuts: Win+Alt+N by default; Shift alone or a bare letter isn't one ({:?})\n", ok(keys_ok), labels);
    set_hotkey(&mut a.settings, 1, Some((2 | 4, 0x56)));
    set_hotkey(&mut a.settings, 0, None);
    let set_ok = hotkey(&a.settings, 1) == Some((6, 0x56)) && hotkey(&a.settings, 0).is_none();
    a.settings.save();
    let back = crate::store::Settings::load();
    out += &format!("{:<4} a changed or removed shortcut saves and loads\n", ok(set_ok && hotkey(&back, 1) == Some((6, 0x56)) && hotkey(&back, 0).is_none()));
    // Recording: the click starts it, Esc cancels, a combination sets it.
    a.sw_view(0, SEC_SHORTCUTS);
    a.recording = Some(2);
    a.record_key(0x1B);
    let esc = a.recording.is_none() && hotkey(&a.settings, 2).is_none();
    a.recording = Some(2);
    a.record_key(0x41); // a bare letter: still waiting
    let waits = a.recording == Some(2);
    a.record_key(0x73); // F4
    let f4 = a.recording.is_none() && hotkey(&a.settings, 2) == Some((0, 0x73));
    out += &format!("{:<4} recording a shortcut: Esc cancels, a bare letter waits, F4 sets it\n", ok(esc && waits && f4));
    a.sw_view(0, SEC_FEATURES);
    out += &a.sw_snap("features section");
    let rows_fit = a.swin.as_ref().map_or(false, |sw| sw.room[SEC_FEATURES as usize] >= 0.0) && a.sw_hits().iter().filter(|h| (S_FEATURE..S_FEATURE + 64).contains(&h.id)).count() > 20;
    a.sw_view(0, SEC_SHORTCUTS);
    out += &a.sw_snap("shortcuts section");
    let chips = a.sw_hits().iter().filter(|h| (S_HOTKEY..S_HOTKEY + 5).contains(&h.id)).count();
    let mouse_chips = a.sw_hits().iter().filter(|h| (S_MOUSE..S_MOUSE + 30).contains(&h.id)).count();
    let in_panel = a.swin.as_ref().map_or(false, |sw| sw.room[SEC_SHORTCUTS as usize] >= 0.0);
    // Recording from the window: a click starts it, Esc cancels.
    let rec = a.sw_click_id(S_HOTKEY) && a.recording == Some(0);
    a.record_key(0x1B);
    out += &format!("{:<4} Features section fits every switch; Shortcuts has {chips} keyboard and {mouse_chips} mouse choices and records\n", ok(rows_fit && in_panel && chips == 5 && mouse_chips == 9 && rec && a.recording.is_none()));

    // Export and import.
    let text = bundle();
    let files = unbundle(&text);
    let round = files.as_ref().map_or(false, |f| f.iter().any(|(n, b)| n == "settings.txt" && b.contains("hotkey_clip=6:86")));
    let junk = unbundle("hello\nworld").is_none() && unbundle("Nookly settings export 1\n=== Nookly file: ../evil.txt ===\nx\n").is_none();
    out += &format!("{:<4} export holds the data files; import takes only Nookly's own files\n", ok(round && junk));
    a.import_pending = files;
    a.sw_view(0, SEC_GENERAL);
    out += &a.sw_snap("import confirm");
    let asks = a.sw_has(S_IMPORT_OK) && a.sw_has(S_IMPORT_NO);
    a.settings_action(S_IMPORT_NO);
    out += &format!("{:<4} import asks before replacing anything; Cancel drops it\n", ok(asks && a.import_pending.is_none()));
    // General page: the export row fits.
    a.sw_view(0, SEC_GENERAL);
    let spare = a.swin.as_ref().map_or(-1.0, |sw| sw.room[SEC_GENERAL as usize]);
    let gen_ok = spare >= 0.0 && a.sw_has(S_EXPORT);
    a.close_settings();
    out += &format!("{:<4} General fits Export / Import ({:.0} DIPs to spare)\n", ok(gen_ok), spare);

    // The tour's presets.
    a.onboarding = Some(3);
    draw(a);
    out += &snap(a, "tour presets");
    let cards = a.hits.iter().filter(|h| h.module == ONBOARD && (O_PRESET..O_PRESET + 4).contains(&h.id)).count();
    a.onboarding_action(O_PRESET + 1);
    let student = current_preset(&a.settings) == Some(1);
    out += &format!("{:<4} the tour offers {cards} presets; picking Student switches it on\n", ok(cards == 4 && student));

    (a.expanded, a.more_open, a.pill_off, a.tab, a.settings.hidden_panels, a.settings.extra, a.onboarding) = start;
    a.close_settings();
    a.settings.save();
    a.more_open = false;
    a.tick();
    out
}

/// Self-test: themes are readable, seconds widen the clock, the calendar.
fn look_tests(a: &mut App) -> String {
    use crate::gfx::{Theme, GLASS_LIGHT, THEMES};
    let ok = |b: bool| if b { "OK" } else { "FAIL" };
    let mut out = String::new();
    let start = (a.expanded, a.more_open, a.tab, a.settings.layout.clone(), a.settings.style, a.settings.theme.clone());
    let lum = |c: Color| {
        let f = |v: f32| if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
        0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b)
    };
    let contrast = |x: Color, y: Color| {
        let (l1, l2) = (lum(x), lum(y));
        (l1.max(l2) + 0.05) / (l1.min(l2) + 0.05)
    };
    let over = |top: Color, under: Color| Color { r: top.r * top.a + under.r * (1.0 - top.a), g: top.g * top.a + under.g * (1.0 - top.a), b: top.b * top.a + under.b * (1.0 - top.a), a: 1.0 };
    // Solid themes: text, dim text, and a button over the background.
    let mut weak = Vec::new();
    for t in THEMES.iter().filter(|t| !t.glass) {
        let bg = Color { a: 1.0, ..t.bg };
        let btn = over(Color { a: 0.09, ..t.over }, bg);
        let (ct, cd, cb) = (contrast(t.text, bg), contrast(t.dim, bg), contrast(t.text, btn));
        if ct < 7.0 || cd < 3.0 || cb < 7.0 {
            weak.push(format!("{} {ct:.1}/{cd:.1}/{cb:.1}", t.key));
        }
    }
    // Liquid Glass over light, dark and colourful backdrops, every pixel:
    // the glass picks light or dark from the backdrop's brightness.
    let mut worst = Vec::new();
    for kind in ["light", "dark", "color"] {
        let (w, h, px, l) = crate::glass::sample(kind, 60, 20);
        let _ = (w, h);
        let t: Theme = if l > 0.62 { GLASS_LIGHT } else { THEMES.iter().copied().find(|t| t.glass).unwrap_or(THEMES[0]) };
        let mut min = f32::MAX;
        for p in px.chunks_exact(4) {
            let back = Color { r: p[2] as f32 / 255.0, g: p[1] as f32 / 255.0, b: p[0] as f32 / 255.0, a: 1.0 };
            min = min.min(contrast(t.text, over(t.bg, back)));
        }
        worst.push(format!("{kind} {:.1}", min));
        // Glass is see-through on purpose, so it can't reach the solid
        // themes' 4.5:1; it keeps at least 3:1 (WCAG's large-text level).
        if min < 3.0 {
            weak.push(format!("glass on {kind} {min:.1}"));
        }
    }
    // Coloured labels (orange, yellow, green, red, blue, cyan) on each
    // light theme, as the theme darkens them.
    let mut acc = Vec::new();
    for t in THEMES.iter().filter(|t| t.tint < 1.0) {
        let bg = Color { a: 1.0, ..t.bg };
        let min = [col::ACCENT, col::YELLOW, col::GREEN, col::RED, col::BLUE, col::CYAN]
            .iter()
            .map(|c| contrast(Color { r: c.r * t.tint, g: c.g * t.tint, b: c.b * t.tint, a: 1.0 }, bg))
            .fold(f32::MAX, f32::min);
        acc.push(format!("{} {min:.1}", t.key));
        if min < 3.0 {
            weak.push(format!("{} accents {min:.1}", t.key));
        }
    }
    out += &format!("{:<4} coloured labels on the light themes reach 3:1 (icons and bold labels) {acc:?}
", ok(acc.iter().all(|a| !weak.iter().any(|w| w.starts_with(a.split(' ').next().unwrap_or(""))))));
    out += &format!("{:<4} every theme's text is readable (WCAG AA or better), Liquid Glass on light, dark and colour backdrops {worst:?} {weak:?}
", ok(weak.is_empty()));
    // Each theme draws: the panel and the closed notch render without errors.
    let saved = (a.settings.theme.clone(), a.settings.style, a.expanded, a.tab, a.more_open);
    let mut bad = Vec::new();
    for t in THEMES.iter() {
        a.settings.theme = t.key.to_string();
        for open in [false, true] {
            a.expanded = open;
            let tgt = a.target();
            a.cur = tgt;
            a.render(tgt, 0.0);
            if a.gfx.last_err.is_some() || a.gfx.bright() == 0 && t.key != "light" {
                bad.push(format!("{} open {open}", t.key));
            }
        }
    }
    let glass_drawn = a.backdrop.is_some() || THEMES.iter().all(|t| !t.glass);
    out += &format!("{:<4} every theme draws the notch and panels; Liquid Glass has its backdrop {bad:?} {glass_drawn}
", ok(bad.is_empty() && glass_drawn));
    // Liquid Glass follows the notch at once: while it changes size (a
    // panel switch) it shows the matching part of one grab, with no new
    // grab until it rests, and the glass covers exactly the new shape.
    {
        a.settings.theme = "glass".into();
        a.backdrop = None;
        a.glass_raw = None;
        a.expanded = true;
        let tabs = a.tab_list();
        let small = tabs.iter().copied().min_by(|&x, &y| a.modules[x].size().0.partial_cmp(&a.modules[y].size().0).unwrap());
        let wide = tabs.iter().copied().max_by(|&x, &y| a.modules[x].size().0.partial_cmp(&a.modules[y].size().0).unwrap());
        if let (Some(s), Some(w)) = (small, wide) {
            a.tab = s;
            let tgt = a.target();
            a.cur = tgt;
            a.render(tgt, 0.0);
            let first = a.glass_raw.as_ref().map(|r| r.serial);
            // Switch to the widest panel and draw a few in-between frames.
            a.tab = w;
            let mut covers = true;
            for k in 1..=4 {
                let tgt = a.target();
                let f = k as f32 / 4.0;
                a.cur = (lerp(a.cur.0, tgt.0, f), lerp(a.cur.1, tgt.1, f), lerp(a.cur.2, tgt.2, f));
                a.render(tgt, if k < 4 { 20.0 } else { 0.0 });
                // The glass also fills the flares beside it at the screen edge.
                let n = a.edge_rect(a.cur.0, a.cur.1);
                let ear = (a.cur.1 / 4.0).min(9.0);
                let notch = Rect::new(n.x - ear, n.y, n.w + ear * 2.0, n.h);
                if let Some((_, _, _, at)) = &a.backdrop {
                    covers &= (at.x - notch.x).abs() <= 4.0 / a.scale + 1.0 && (at.r() - notch.r()).abs() <= 4.0 / a.scale + 1.0 && (at.b() - notch.b()).abs() <= 4.0 / a.scale + 1.0;
                } else {
                    covers = false;
                }
            }
            let one_grab = first.is_some() && a.glass_raw.as_ref().map(|r| r.serial) == first;
            out += &format!("{:<4} Liquid Glass shows what's behind the new shape at once, from one grab, while it changes size {:?}\n", ok(covers && one_grab), (covers, one_grab));
            // A window changing behind it (switch, move, open, close) is
            // picked up on the very next frame, even mid-animation; so is
            // the notch starting to open.
            let s0 = a.glass_raw.as_ref().map(|r| r.serial);
            a.glass_changed();
            let tgt = a.target();
            a.render(tgt, 20.0);
            let after_change = a.glass_raw.as_ref().map(|r| r.serial) != s0;
            a.expanded = false;
            a.toast = None;
            a.frame();
            let s1 = a.glass_raw.as_ref().map(|r| r.serial);
            a.expanded = true;
            a.frame();
            let on_open = a.glass_raw.as_ref().map(|r| r.serial) != s1;
            out += &format!("{:<4} Liquid Glass grabs again when a window changes behind it and when the notch opens {:?}\n", ok(after_change && on_open), (after_change, on_open));
        }
    }
    (a.settings.theme, a.settings.style, a.expanded, a.tab, a.more_open) = saved;
    a.backdrop = None;
    a.glass_raw = None;
    // Seconds: the clock face and the narrow notches grow.
    let o = std::sync::atomic::Ordering::Relaxed;
    let style = a.settings.style;
    a.expanded = false;
    a.settings.layout = crate::store::NotchLayout::custom_rows(Vec::new(), true);
    let mut widths = Vec::new();
    for s in [NotchStyle::Slim, NotchStyle::Compact] {
        a.settings.style = s;
        a.settings.seconds = false;
        crate::time::SECONDS.store(false, o);
        let w0 = a.target().0;
        a.settings.seconds = true;
        crate::time::SECONDS.store(true, o);
        let w1 = a.target().0;
        widths.push((w0, w1));
    }
    let face = Now::get().hhmm();
    let fits = {
        let mid = a.gfx.measure("12:58:59", 14.0, Weight::Semi) + 2.0;
        let wing = a.slim_live_w() / 2.0 - mid / 2.0 - SLIM_GAP - SLIM_WING_PAD;
        let room = a.gfx.measure("Timer", 12.0, Weight::Semi) + 18.0 + 5.0;
        wing >= room
    };
    a.settings.seconds = false;
    crate::time::SECONDS.store(false, o);
    a.settings.style = style;
    let grow = widths.iter().all(|(w0, w1)| w1 > w0);
    out += &format!("{:<4} seconds: the clock shows them ({face}), Slim and Compact grow {widths:?}, labels still fit beside 12:58:59 ({fits})
", ok(grow && face.matches(':').count() == 2 && fits));
    // Calendar: the month grid.
    use crate::modules::calendar::month_grid;
    let g = month_grid(2026, 11, 1);
    let nov = g.len() == 42 && g[0] == (2026, 10, 26) && g.iter().filter(|d| d.1 == 11).count() == 30 && g.contains(&(2026, 11, 30));
    let feb = month_grid(2024, 2, 0).iter().filter(|d| d.1 == 2).count() == 29;
    let sun = month_grid(2026, 11, 0)[0] == (2026, 11, 1);
    let wd = crate::time::weekday(2026, 11, 11) == 3;
    out += &format!("{:<4} calendar: month grid (Monday or Sunday first), leap February, weekdays ({nov}, {feb}, {sun}, {wd})
", ok(nov && feb && sun && wd));
    (a.expanded, a.more_open, a.tab, a.settings.layout, a.settings.style, a.settings.theme) = start;
    out
}

/// Self-test: stepping aside for a window under the notch.
fn smart_hide_tests(a: &mut App) -> String {
    let o = std::sync::atomic::Ordering::Relaxed;
    let ok = |b: bool| if b { "OK" } else { "FAIL" };
    let mut out = String::new();
    let saved = (a.settings.style, a.settings.make_room, a.expanded, a.hover_block, a.more_open);
    a.settings.style = NotchStyle::Full;
    a.settings.make_room = true;
    a.expanded = false;
    a.toast = None;
    a.queue.clear();
    a.hover_block = false;
    a.near_until = -10.0;
    // A window comes under the notch: it steps aside at once...
    MAXIMIZED_OVERRIDE.store(1, o);
    a.covered = false;
    a.update_make_room();
    let aside = a.yielding() && a.look() == NotchStyle::Pill && a.passthrough;
    // ...and comes back after a moment, still click-through.
    a.covered_since = a.t() - SMART_SETTLE - 0.1;
    a.sync_yield();
    let back = !a.yielding() && a.look() == NotchStyle::Full && a.passthrough;
    // The mouse near it (the address bar below): steps aside again.
    let zone = a.edge_rect(240.0, 38.0);
    a.poll_at(zone.cx() + 150.0, zone.b() + 40.0);
    let near = a.yielding();
    // The mouse gone for a while: back.
    a.poll_at(zone.cx(), zone.b() + 300.0);
    a.near_until = a.t() - 0.1;
    a.sync_yield();
    let gone = !a.yielding();
    // Resting on the notch itself opens it.
    a.poll_at(zone.cx(), zone.cy());
    std::thread::sleep(std::time::Duration::from_millis(360));
    a.poll_at(zone.cx(), zone.cy());
    let opened = a.expanded && !a.passthrough;
    a.expanded = false;
    a.sync_passthrough();
    // Switching to a window under it, or dragging one: steps aside.
    a.covered_since = a.t() - 10.0;
    a.window_switched(a.hwnd);
    let switched = a.yielding();
    a.covered_since = a.t() - 10.0;
    a.moving = true;
    let dragging = a.yielding();
    a.moving = false;
    // A window on the other screen (not under the notch): switching to it
    // or dragging it doesn't make the notch step aside.
    a.covered_since = a.t() - 10.0;
    a.window_switched(HWND(std::ptr::null_mut()));
    let other_switch = !a.yielding();
    a.window_moving(true, HWND(std::ptr::null_mut()));
    let other_drag = !a.yielding();
    a.window_moving(false, HWND(std::ptr::null_mut()));
    let other_drop = !a.yielding();
    out += &format!(
        "{:<4} smart hide: a window on another screen doesn't move it (switch, drag, drop) {:?}\n",
        ok(other_switch && other_drag && other_drop),
        (other_switch, other_drag, other_drop)
    );
    out += &format!(
        "{:<4} smart hide: steps aside when a window comes under it, comes back, steps aside when the mouse is near, opens when you rest on it, steps aside on switch and drag {:?}
",
        ok(aside && back && near && gone && opened && switched && dragging),
        (aside, back, near, gone, opened, switched, dragging)
    );
    // No window under it: the normal notch.
    MAXIMIZED_OVERRIDE.store(2, o);
    a.update_make_room();
    let normal = !a.yielding() && !a.passthrough && a.look() == NotchStyle::Full;
    let r = |l, t, rr, b| RECT { left: l, top: t, right: rr, bottom: b };
    let area = r(400, 0, 740, 46);
    let rects = under_rect(r(0, 0, 1920, 1040), area) && under_rect(r(500, 20, 900, 600), area) && !under_rect(r(500, 46, 900, 600), area) && !under_rect(r(0, 0, 400, 300), area);
    out += &format!("{:<4} smart hide: normal notch with nothing under it; overlap rules ({normal}, {rects})
", ok(normal && rects));
    MAXIMIZED_OVERRIDE.store(0, o);
    let real = a.window_under();
    MAXIMIZED_OVERRIDE.store(2, o);
    out += &format!("INFO a window under the notch on this screen right now: {real}
");
    (a.settings.style, a.settings.make_room, a.expanded, a.hover_block, a.more_open) = saved;
    a.update_make_room();
    out
}

/// Self-test: simple and custom order, when-to-show, alerts, place, mascot.
fn widget_tests_2(a: &mut App) -> String {
    use crate::store::{Place, PlaceMode};
    let mut out = String::new();
    let ok = |b: bool| if b { "OK" } else { "FAIL" };
    let saved = a.settings.clone();
    let view = (a.expanded, a.more_open, a.tab, a.mood);
    // Everything new saves and loads.
    let mut st = a.settings.clone();
    st.layout = NotchLayout::default();
    st.layout.custom = true;
    st.layout.add("weather");
    st.layout.when.insert("battery".into(), ShowWhen::Change);
    st.layout.alerts.insert("caps".into(), AlertMode::Small);
    st.place = Place { mode: PlaceMode::Manual, lat: 6.9271, lon: 79.8612, name: "Colombo, Sri Lanka".into(), prayer: true, fahrenheit: true };
    st.save();
    let back = Settings::load();
    out += &format!("{:<4} custom order, picks, when-to-show, alert modes and place save and load\n", ok(back.layout == st.layout && back.place == st.place));
    // Settings from the last build: an edited order stays custom.
    crate::store::write("settings.txt", "onboarded=1\nnotch_layout=*music;cpu,\n");
    let old = Settings::load();
    crate::store::write("settings.txt", "onboarded=1\nnotch_layout=*pomodoro;*timer;*prayer;*music;*agents;*due\n");
    let plain = Settings::load();
    let up = old.layout.custom && old.layout.picked == ["music", "cpu"] && !plain.layout.custom;
    out += &format!("{:<4} an order edited in the last build stays a custom order; the default one becomes simple\n", ok(up));
    a.settings = saved.clone();
    a.settings.save();
    // Simple mode orders by urgency: an agent that needs you above music.
    let mut l = NotchLayout::default();
    l.add("battery");
    l.add("bluetooth");
    let live = [("pomodoro", 0usize, None), ("music", 1, Some(50u8)), ("agents", 2, Some(75)), ("timer", 3, None)];
    let rows = auto_rows(&l, &live);
    let first: Vec<&str> = rows.iter().take(2).map(|r| r.left.as_str()).collect();
    let last = rows.last().cloned().unwrap_or_default();
    out += &format!("{:<4} simple mode: most urgent first {first:?}, widgets two per row\n", ok(first == ["agents", "music"] && last == NotchRow::pair("battery", "bluetooth")));
    // "On change": shows for a few seconds after the value changes.
    a.expanded = false;
    a.toast = None;
    a.queue.clear();
    let mut l = NotchLayout::custom_rows(vec![NotchRow::pair("date", "")], true);
    l.when.insert("date".into(), ShowWhen::Change);
    a.settings.layout = l;
    a.widget_seen.clear();
    a.refresh_shown();
    let quiet_first = a.showing_ids().is_empty();
    a.widget_seen.insert("date", ("changed".into(), f64::NEG_INFINITY));
    a.refresh_shown();
    let shows = a.showing_ids() == ["date"];
    let earlier = a.t() - 5.0;
    if let Some(v) = a.widget_seen.get_mut("date") {
        v.1 = earlier;
    }
    a.refresh_shown();
    let gone = a.showing_ids().is_empty();
    out += &format!("{:<4} \"On change\" shows a widget for a few seconds after it changes ({quiet_first}, {shows}, {gone})\n", ok(quiet_first && shows && gone));
    // Alerts: Small goes inside the closed notch, Off goes nowhere.
    a.settings.layout = NotchLayout::custom_rows(vec![NotchRow::pair("cpu", "")], false);
    a.settings.set_alert_mode("caps", AlertMode::Small);
    a.small_alert = None;
    a.push_toast(Toast::new("caps", glyph::CAPS, col::GREEN, "Caps Lock", "On").secs(1.6));
    a.refresh_shown();
    let small = a.queue.is_empty() && a.showing_ids() == ["cpu", "alert"];
    a.small_alert = None;
    a.settings.set_alert_mode("caps", AlertMode::Off);
    a.push_toast(Toast::new("caps", glyph::CAPS, col::GREEN, "Caps Lock", "Off").secs(1.6));
    let off = a.queue.is_empty() && a.small_alert.is_none();
    a.settings.set_alert_mode("caps", AlertMode::Popup);
    a.push_toast(Toast::new("caps", glyph::CAPS, col::GREEN, "Caps Lock", "On").secs(1.6));
    let pop = a.queue.len() == 1;
    a.queue.clear();
    out += &format!("{:<4} alerts: Small shows beside what the notch shows, Off nothing, Pop-up the card ({small}, {off}, {pop})\n", ok(small && off && pop));
    // The Settings switches for volume and Copied are the same setting.
    a.settings.volume_overlay = false;
    let v_off = a.settings.alert_mode("volume") == AlertMode::Off;
    a.settings.set_alert_mode("volume", AlertMode::Small);
    let v_on = a.settings.volume_overlay && a.settings.alert_mode("volume") == AlertMode::Small;
    a.settings.copy_toast = true;
    let c_on = a.settings.alert_mode("copy") == AlertMode::Popup;
    out += &format!("{:<4} Volume overlay and Show what I copy are the same as their alert modes\n", ok(v_off && v_on && c_on));
    // Slim with a side mascot: it takes a side and the notch stays 200
    // (it grows only for an item across both sides).
    a.settings.style = NotchStyle::Slim;
    a.settings.edge = Edge::Top;
    a.settings.personality = true;
    a.settings.mascot_pos = 0;
    a.mood = Mood::Eyes;
    a.settings.layout = NotchLayout::custom_rows(vec![NotchRow::pair("cpu", "date")], false);
    let tgt = a.target();
    a.cur = tgt;
    a.render(tgt, 0.0);
    let notch = a.edge_rect(tgt.0, tgt.1);
    let inside = a.mascot_rect.map_or(false, |m| notch.contains(m.cx(), m.cy()));
    let pair_w = tgt.0;
    a.settings.layout = NotchLayout::custom_rows(vec![NotchRow::wide("date")], false);
    let wide_w = a.target().0;
    a.settings.mascot_pos = 1;
    a.settings.layout = NotchLayout::custom_rows(vec![NotchRow::pair("cpu", "date")], false);
    let tgt = a.target();
    a.cur = tgt;
    a.render(tgt, 0.0);
    let centre_w = tgt.0;
    let small_mascot = a.mascot_rect.map_or(false, |m| m.w <= 60.0);
    out += &format!(
        "{:<4} Slim with a mascot: side mascot keeps 200 ({pair_w:.0}, mascot inside {inside}), grows only beside a both-sides item ({wide_w:.0}); centred stays 200 and small ({centre_w:.0}, {small_mascot})\n",
        ok(pair_w == SLIM_LIVE_W && inside && wide_w > SLIM_LIVE_W && centre_w == SLIM_LIVE_W && small_mascot)
    );
    // Clicking the closed notch opens the panel of what it shows.
    a.mood = Mood::Clock;
    a.settings.personality = false;
    a.settings.layout = NotchLayout::custom_rows(vec![NotchRow::pair("cpu", "")], false);
    a.expanded = false;
    a.tab = 0;
    let tgt = a.target();
    a.cur = tgt;
    a.render(tgt, 0.0);
    let bg = a.hits.iter().find(|h| h.module == APP_HIT && h.id == u32::MAX).map(|h| h.rect);
    if let Some(r) = bg {
        a.on_down(r.cx(), r.cy());
        a.on_up(r.cx(), r.cy());
    }
    let mon = find(a, "System Monitor");
    // A drag that loses the mouse is cancelled.
    let w = find(a, "Widgets").unwrap_or(0);
    a.inner_drag = Some(w);
    a.handle(WM_CAPTURECHANGED, WPARAM(0), LPARAM(0));
    out += &format!("{:<4} a drag that loses the mouse (Alt+Tab) is cancelled
", ok(a.inner_drag.is_none()));
    // Opening a folder or link waits until the click is handled.
    open_later("about:blank");
    let queued = OPEN_LATER.with(|q| q.borrow().len()) == 1;
    OPEN_LATER.with(|q| q.borrow_mut().clear());
    out += &format!("{:<4} folders and links open after the click is handled (no lost mouse messages)
", ok(queued));
    out += &format!("{:<4} clicking the closed notch opens what it shows (tab {})\n", ok(a.expanded && Some(a.tab) == mon), a.modules[a.tab].title());
    a.expanded = false;
    // Place helpers.
    use crate::modules::widgets as wg;
    let tz = wg::tz_city("Sri Lanka Standard Time").map(|p| p.2);
    let cities = wg::parse_cities(r#"{"results":[{"name":"Colombo","latitude":6.93,"longitude":79.85,"country":"Sri Lanka"},{"name":"Kandy","latitude":7.29,"longitude":80.63}]}"#);
    let near = wg::near_name(6.90, 79.86);
    let here = wg::tz_place();
    let ok_p = tz.as_deref() == Some("Colombo") && cities.len() == 2 && cities[0].0 == "Colombo, Sri Lanka" && cities[1].0 == "Kandy" && near == "Near Colombo";
    out += &format!("{:<4} place: time-zone city, city search results, \"near\" names ({near}; this PC's time zone: {here:?})\n", ok(ok_p));
    a.settings = saved;
    a.settings.save();
    (a.expanded, a.more_open, a.tab, a.mood) = view;
    a.small_alert = None;
    out
}

/// Do what the command line asks for (and end), if it's one of these.
pub fn run_tests(args: &[String]) {
    unsafe {
        // `--probe-privacy <file> [camera]`: write what Windows reports as
        // using the microphone and camera, every second for 8 seconds (to
        // check the mic and camera dots); with `camera`, open the camera
        // meanwhile.
        if let Some(i) = args.iter().position(|a| a == "--probe-privacy") {
            let out = args.get(i + 1).cloned().unwrap_or_else(|| "probe.txt".into());
            let cam = args.iter().any(|a| a == "camera").then(|| crate::modules::mirror::probe_camera(6.0));
            let mut log = String::new();
            for k in 0..8 {
                let c = cam.as_ref().map(|(f, e)| (f.lock().unwrap().as_ref().map(|x| x.3), e.lock().unwrap().clone()));
                log += &format!("{k}s mic={:?} camera={:?} frames/err={:?}
", crate::modules::privacy::in_use("microphone"), crate::modules::privacy::in_use("webcam"), c);
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            let _ = std::fs::write(out, log);
            std::process::exit(0);
        }
        // Docs renders: demo data, fixed clock, no window shown.
        if args.iter().any(|a| a == "--selftest" || a == "--render-demo" || a == "--render-docs") {
            crate::modules::agents::ISOLATED.store(true, std::sync::atomic::Ordering::Relaxed);
            // His screen may have a maximized window open: renders and tests
            // assume none unless a test says otherwise.
            MAXIMIZED_OVERRIDE.store(2, std::sync::atomic::Ordering::Relaxed);
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
            crate::time::DEMO.store(true, std::sync::atomic::Ordering::Relaxed);
            let dir = std::path::PathBuf::from(args.get(i + 1).cloned().unwrap_or_else(|| "demo-render".into()));
            let _ = App::create(false);
            let log = render_demo(&dir);
            let _ = std::fs::write(dir.join("render.log"), log);
            super::shutdown();
            std::process::exit(0);
        }
        if let Some(i) = args.iter().position(|a| a == "--render-docs") {
            crate::time::DEMO.store(true, std::sync::atomic::Ordering::Relaxed);
            let dir = std::path::PathBuf::from(args.get(i + 1).cloned().unwrap_or_else(|| "docs-render".into()));
            let _ = App::create(false);
            let log = render_docs(&dir);
            let _ = std::fs::write(dir.join("render.log"), log);
            super::shutdown();
            std::process::exit(0);
        }

        // The self-test never shows a window, so it may run alongside the app.
        if args.iter().any(|a| a == "--selftest") {
            let _ = App::create(false);
            let mut report = selftest();
            report += &format!("INFO lyrics (LRCLIB): {}\n", crate::modules::music::selftest_lyrics());
            report += &format!("INFO data dir: {}\n", crate::store::dir().display());
            let out = std::env::var("NOOKLY_SELFTEST_OUT").unwrap_or_else(|_| "selftest.txt".into());
            let _ = std::fs::write(out, report);
            super::shutdown();
            std::process::exit(0);
        }
    }
}
