<p align="center"><img src="docs/images/logo.png" alt="Nookly" width="120"></p>

<h1 align="center">Nookly</h1>

<p align="center"><b>A little notch that does a lot. For Windows 10 and 11.</b><br>
Music, calendar, notes, timers, widgets, health and quick tools, one hover away at the top of your screen.<br>
Free, open source, private, and light enough that you'll forget it's running.</p>

<p align="center">
  <a href="https://github.com/banulalakwindu/Nookly/releases/latest"><b>⬇ Download for Windows</b></a> ·
  <a href="https://banulalakwindu.github.io/Nookly/">Website</a> ·
  <a href="docs/USER_GUIDE.md">User guide</a> ·
  <a href="CHANGELOG.md">What's new</a>
</p>

<p align="center"><img src="docs/images/hero.png" alt="Nookly: every panel at a glance" width="100%"></p>

## Why Nookly

- **Everything one hover away.** Rest the mouse on the notch and it opens; move away and it tucks itself back. Or press **Win+Alt+N** from anywhere.
- **Only what you want.** Start from a preset (Minimal, Student, Work, Everything) and switch every feature on or off. What's off doesn't run at all.
- **Light and smooth.** Written in Rust, drawn on the GPU, and it reacts to events instead of polling. It idles near zero CPU.
- **Yours alone.** No account, no ads, no tracking. Your data is plain text on your PC.
- **Made to look good.** Five notch styles, ten themes plus Liquid Glass and your own, on any edge of any screen.

## What's inside

| | |
|---|---|
| <img src="docs/images/lyrics.png" alt="Music with synced lyrics"> | <img src="docs/images/calendar.png" alt="Calendar"> |
| **Music** from any app, with synced lyrics, a draggable seek bar and an optional glow in the album's colour that moves with the beat | **Calendar** with your tasks, plus your Google or Outlook calendar (iCal link or `.ics` file): "Standup in 8 min" on the notch and a **Join** button |
| <img src="docs/images/widgets.png" alt="Widgets"> | <img src="docs/images/health.png" alt="Health"> |
| **Widgets** on the closed notch: battery, AirPods, weather, Wi-Fi, CPU, countdowns, your commands... in the order you like | **Health**: screen time today, eye breaks (20-20-20), stand-up reminders and battery care. Reminders wait during games and calls |
| <img src="docs/images/countdowns.png" alt="Countdowns"> | <img src="docs/images/notes.png" alt="Notes"> |
| **Countdowns** to exams, trips and birthdays, "12 days" on the notch | **Notes**: a scratchpad that saves as you type |
| <img src="docs/images/launcher.png" alt="Launcher"> | <img src="docs/images/toggles.png" alt="Quick toggles"> |
| **Launcher**: your apps, folders and sites, one click away | **Quick toggles**: Wi-Fi, Bluetooth, dark mode and mic mute |
| <img src="docs/images/commands.png" alt="Commands"> | <img src="docs/images/agents.png" alt="AI agents"> |
| **Commands** on a schedule (1 minute to daily) with their output on the notch | **AI agents**: Claude Code, Codex and Hermes working, needing you, done |
| <img src="docs/images/timer.png" alt="Timer"> | <img src="docs/images/pomodoro.png" alt="Pomodoro"> |
| **Timer and stopwatch** | **Pomodoro** focus sessions |
| <img src="docs/images/clipboard.png" alt="Clipboard"> | <img src="docs/images/shelf.png" alt="Shelf"> |
| **Clipboard history** with search and snippets; click to paste | **Shelf and Share**: drop files to keep them close, or to share them |
| <img src="docs/images/tools.png" alt="Tools"> | <img src="docs/images/prayer.png" alt="Prayer times"> |
| **Tools**: colour picker, calculator, QR codes, converter with today's currency rates | **Prayer times** worked out on your PC, with reminders and Ramadan mode |
| <img src="docs/images/notifications.png" alt="Notifications"> | <img src="docs/images/todo.png" alt="To-do"> |
| **Your notifications** in the notch, with today's list | **To-do** with reminders |

And more: **Downloads finished** (click to open), **Mirror** (a camera check before calls), **System monitor**, **Water reminder**, **mic and camera dots**, **smart hide** that steps aside for windows under it, **profiles** (Home, Work, Focus), mouse shortcuts on the closed notch (scroll for volume or brightness, middle-click to pause), and **export / import** of your settings.

<img src="docs/images/settings-features.png" alt="Settings: Features">

<img src="docs/images/themes.png" alt="Themes">

| | |
|---|---|
| <img src="docs/images/styles.png" alt="Notch styles"> | <img src="docs/images/positions.png" alt="Any edge"> |
| **Five styles**: Full, Slim, Compact, Pill, Hidden | **Any edge** of any screen |

## Install

1. Download `NooklySetup-<version>.exe` from [Releases](https://github.com/banulalakwindu/Nookly/releases/latest) or the [website](https://banulalakwindu.github.io/Nookly/).
2. Run it. It installs just for you, no admin rights needed.
3. A short welcome tour helps you pick the look, the colour and what you want in it.

Updates arrive in the notch, with one click to install.

**"Windows protected your PC"?** Nookly isn't code-signed yet: click **More info** > **Run anyway**. Windows 11's *Smart App Control* blocks every unsigned app; the only way past it is to turn it off in Windows Security > App & browser control.

## Private by design

Everything Nookly keeps is plain text in `%APPDATA%\Nookly`. It goes online only for lyrics (LRCLIB), update checks (GitHub), and, only if you turn them on, the weather (Open-Meteo), your calendar link and currency rates. Clipboard history and notifications are off until you switch them on. Read the [privacy policy](PRIVACY.md).

## Build it yourself

Needs Rust (stable).

```powershell
cargo build --release                       # target\release\nookly.exe
.\target\release\nookly.exe --selftest     # headless checks
.\build.ps1                                 # build + icon + self-test + installer (Inno Setup 6)
python docs\tools\make_images.py shots      # the screenshots in this README
python docs\tools\make_logo.py              # the logo and icons
```

Each feature is one file in `src/modules/` implementing the `Module` trait: copy `src/modules/example.rs` and register it in `modules::all()`.

**Releases and the website** are automatic. The site in `docs/` goes live after every push to `main`. To release, raise `version` in `Cargo.toml`, add a `## <version>` section to `CHANGELOG.md` and push: GitHub Actions builds the installer from the source, tags it and publishes the release.

## License

[MIT](LICENSE) · Made by [Banula Lakwindu](https://github.com/banulalakwindu)
