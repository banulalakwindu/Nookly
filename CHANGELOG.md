# Changelog

Versions 0.1.x below were released before the Nookly name.

## 0.4.0: Settings in its own window, and Nookly's own installer

- **Settings has its own window**: a small rounded window in the notch's style and theme, instead of a big panel inside the notch. Drag it by the header, close it with the x or Esc. The gear in the tab bar, a right-click on the notch or the taskbar mascot, and the tray icon all open it.
- **Two tabs with master switches**: **Notch**, whose switch turns the whole notch on or off (off: hidden, nothing runs, the tray icon stays), and **Taskbar**, a custom Nookly taskbar that is coming soon (its switch stays off for now).
- **Every setting in one place, grouped**: Appearance, Theme, Mascot, Features, Widgets, Alerts, Privacy, Shortcuts and General, with a line at the bottom that explains whatever the mouse is on.
- **Widgets moved into Settings**: the closed notch's Show, Order, Alerts and Place pages live in Settings > Widgets now; the notch's tab bar holds only your panels and the gear.
- The welcome tour mentions the new window and shows once more after updating.
- **Nookly's own installer**: `NooklySetup-<version>.exe` is Nookly itself under its setup name, so the download is no bigger than the app and no Inno Setup is involved. One themed window: Start with Windows, desktop shortcut and automatic update checks, then Install (or Update over an installed Nookly). Installs from the old installer upgrade in place, and in-app updates install silently and start the new version as before.
- **Themed uninstaller, About and tray menu**: Settings > Apps > Uninstall and the tray's Uninstall open Nookly's own uninstall window (one question, with a switch to delete your data too); About and the tray icon's right-click menu are in Nookly's style and theme too. Dialogs and Settings have a minimize button next to close.
- **Side edges**: with music, a timer or widgets on the closed notch, the clock (hours over minutes) now stays in the middle, and it follows the 12-hour setting.
- Light by design: the window exists only while it's open and draws only when something changes.
- Updating from 0.3.x moves you off the old installer: its Apps entry and uninstaller are replaced by Nookly's own, so Windows' Uninstall always opens Nookly's window.
- Fresh screenshots everywhere, with new made-up demo content.

## 0.3.1: Weather, Volume and a real Notes editor

- **Weather** panel: now (temperature, feels like, humidity, wind), the hours around now (earlier ones greyed, rain chance ahead) and yesterday to six days ahead, with drawn icons for sun, moon, clouds, rain, snow, storms and fog. From Open-Meteo, only while the panel is on (on opening, then at most every 30 minutes).
- **Volume** panel: the master volume and each app's own volume with mute, like Windows' mixer. Read only while the panel is open.
- **Notes** is a real editor now: click or drag to place the caret and select, arrows (Shift selects, Ctrl jumps words), Home / End, Ctrl+A / C / X / V, Tab, the mouse wheel scrolls (with a scroll bar), and the text cursor shows over it and every text box.
- **Commands**: click a schedule to type any number of minutes (up to a week), besides the arrows.
- **Mic dot fixed on newer Windows**: Windows stopped writing the records Nookly read, so the mic dot never came on there. It now comes straight from Windows' audio sessions, so it works on every version. The camera dot still uses Windows' records (newer test builds of Windows lock those away); Nookly's own Mirror always shows it.
- **Widgets > Show**: the cards no longer scroll over the switches above them.
- The welcome tour shows once more after an update when it has something new.

## 0.3.0: a lot more in the notch

- **A new welcome tour**: hello, the look and where it sits, a colour, what you want in it, and four things to know. 30 seconds, all changeable later.
- **Fresh look everywhere**: new screenshots, README and website.

- **New panels** (each off until you switch it on in Settings > Features, or by a preset):
  - **Notes**: a scratchpad that saves as you type.
  - **Countdowns**: a name and a date picked with arrows (or the mouse wheel), "Every year" for birthdays and holidays, and a "12 days" widget.
  - **Launcher**: pin apps, files, folders and sites: **Add apps...** (opens in the Start menu folder, several at once), **Folder...**, drop them on the notch while the tab is open, or paste a link.
  - **Commands**: up to 5 of your own commands, each on its own schedule (1 minute to daily), with the first line each prints here and as widgets on the closed notch.
  - **Toggles**: Wi-Fi, Bluetooth, dark mode and microphone mute, plus shortcuts to Night light, hotspot, Focus and display.
  - **Health**: screen time today (top apps), eye breaks (20-20-20), stand-up reminders every 30 to 90 minutes and battery care (unplug at 80%).
  - **Mirror**: a camera preview before a call; the camera runs only while the panel is on screen.
  - **Notifications**: today's notifications in their own tab when "Show them in the notch" is on.
- **Calendar files**: besides a link, choose an exported `.ics` file (Calendar > link > Choose file...).
- **Liquid Glass no longer blocks screen recorders**: the notch used to stay hidden from capture while Liquid Glass was on, which some recorders refuse ("Nookly is preventing desktop capture"). Now it hides only for the instant it looks behind itself, so it also shows in screenshots and recordings.
- **Next meeting**: link your calendar (its private iCal link, or an exported `.ics` file) in the Calendar tab. Events show in the month and the day panel, and the closed notch shows "Standup in 8 min" with a Join button for Teams, Zoom and Meet. Repeating events, exceptions and cancelled ones are handled.
- **Downloads finished**: a live activity and a toast when a file lands in Downloads; click to open it.
- **Ramadan mode** in Prayer: Sehri and Iftar countdowns in the panel and on the closed notch (auto by the Hijri calendar, or on / off).
- **Money** in the Converter: USD, EUR, GBP, LKR, BDT, INR, SAR and AED at today's rates (fetched once a day, only after you ask).
- **Share** zone: drag files onto the notch and drop them on Share to open Windows' Share window (Nearby Share, Phone Link, mail).
- **Music**: drag the seek bar; "Song colour around the notch" puts a soft glow in the album art's colour around the closed notch that breathes with the music (read from Windows' audio meter, only while it's shown).
- **Mouse on the closed notch** (Settings > Features > Keyboard & mouse shortcuts): scroll for volume or brightness (Shift + scroll is always brightness, with an overlay), middle-click play/pause, double-click for a screenshot, clipboard or a note. A fourth keyboard shortcut opens a quick note.
- **Settings > Look** across the whole panel: **your own theme** (background and text colour, with a contrast check and "Fix it"), **size** (smaller, normal, larger), **reduce motion**, and **hide the notch for chosen apps**.
- **Profiles**: Home, Work and Focus each keep their own features, widgets and theme; **Auto** switches to Work on weekdays 9 to 5.
- Chiming reminders wait while you're on a call (the microphone is in use), as they already did during full-screen games and videos.
- The Features page has 24 switches in four groups; what each does is in its tooltip. Switched-on ones are tinted with a coloured icon, switched-off ones plain with an outlined switch.
- **More panels** (⋯) shows compact tiles that always fit the panel.

## 0.2.0: first Nookly release

- **New name, Nookly**, with its own logo, installer, data folder (`%APPDATA%\Nookly`) and update channel. On first start, settings and data from the earlier version's folder are copied over, so nothing is lost.

- **Settings > Features** (was Panels): every feature on one page, in four groups (Everyday, Focus & health, Work & tools, System), each with a switch; resting the mouse on one says what it does. A feature that's off has no tab, no reminders and does no work in the background (its panel isn't ticked; clipboard history stops listening).
  - **Presets**: Minimal (music, timer, to-do, calendar, widgets), Student, Work and Everything switch on exactly that set; change any of it after. A new install starts on Minimal, and the welcome tour has a step to pick one.
  - **Keyboard shortcuts**: **Win+Alt+N** opens and closes the notch from anywhere; optional shortcuts for clipboard history (opens it with search) and a screenshot. Click a shortcut and press the keys to change it; Backspace removes it. A shortcut another app already has shows "In use".
- **⋯ in the tab bar**: when more panels are on than fit, the rest sit under ⋯ as big tiles. The panel you're on always stays in the bar, and the mouse wheel still goes through all of them.
- **Export / Import settings** (Settings > General): saves your settings and data (to-do, water, prayer, tools, shelf, pinned clipboard) in one `.nookly` file for another PC. Import shows what's in the file and asks before replacing anything, then Nookly starts again on it.
- **Calendar** tab: the month with today highlighted, days with to-do tasks due marked, previous / next month (or the mouse wheel), Today, and the chosen day's tasks beside the grid. The week starts on Windows' first day of the week.
- **Themes** (Settings > Look): Black (default), Graphite, Midnight, Rose, Forest, Ocean, Light, Blossom, Paper and **Liquid Glass**, which blurs what's behind the notch under a clear tint with a bright rim and switches between light and dark glass with the background. While Liquid Glass is on the notch is left out of screenshots and screen sharing. Every theme meets WCAG AA text contrast, and coloured labels on the light themes reach 3:1 (checked by the self-test, Liquid Glass over light, dark and colourful backgrounds).
- **Show seconds** (Settings > Look): seconds on the clock; Slim and Compact grow 24 px so nothing gets squeezed.
- **Smart hide** (Settings > General, "Step aside for windows under it", on by default; replaces "Slim down over maximized apps"): whenever a window's top (title bar, tabs, address bar) is under the notch, not only a maximized one, the notch steps aside to a thin click-through line, comes back after about 2.5 s (still click-through), steps aside again while the mouse is near it, and opens when you rest the mouse on it. Switching to such a window or dragging one across the top makes it step aside too; windows on another screen (or lower down) don't. Windows are checked only on window events (switch, drag start and stop), 10 times a second during a drag, and once a second otherwise.
- New **Widgets** tab, for what the closed notch shows and in which order.
  - **Pick** switches items on: live activities (music, timer, Pomodoro, prayer countdown, due tasks, AI agents) and widgets: PC battery, Bluetooth device battery (AirPods too, from their battery broadcast), CPU, memory, network, free space, uptime, date, water, focus sessions today, open tasks and next prayer. Out of the box only the live activities are on (music, timer, Pomodoro, prayer countdown, AI agents, due tasks), each across both sides of the clock; the widgets (battery, Bluetooth...) are yours to switch on. A live activity you switch on gets both sides too.
  - **Order** arranges them in rows: one item across both sides of the clock, or two one-side items in a row. Drag items between sides and rows, drag rows (or ▲ ▼) to reorder. The first row with something to show wins, so you choose, for example, whether a song or a working agent shows when both are on.
  - **When it matters** skips widgets with nothing to say (battery on the charger, no Bluetooth device, good Wi-Fi, sound not muted).
  - More widgets: **Weather** (from Open-Meteo, about every 30 min, only while it's on; the privacy policy lists it), **Wi-Fi** (signal and network name), **Volume** and **Microphone** mute.
  - Simple by default: the **Show** page switches things on and Nookly orders them (most urgent first). **Custom order** reveals the Order page; **Reset to default** (asks first). Each widget can show **Always**, when it **Matters**, or for a few seconds when it **Changes**.
  - **Alerts** page: Volume, Copied, Battery, Network, Caps Lock, Language, Bluetooth and Mic & camera can each be **Off**, **Small** (a few seconds inside the closed notch, beside what it shows) or a **Pop-up**.
  - **Place** page for the weather (and prayer times if you like): the city of Windows' time zone, Windows location, or a city you search for.
  - Slim with the mascot: in the middle it takes the clock's place, at a side it takes that side, so the notch stays 200 wide and nothing gets cut short. Tight values switch to a shorter form ("11" for "11 Nov") instead of "...".
  - Clicking the closed notch opens the panel of what it shows. The welcome tour mentions widgets.
  - `--selftest` and `--render-docs` use a throwaway data folder unless given `--real-data`.
  - Weather in °C by default, °F on the Place page. The Show and Order pages open taller.
- Fixed: after **Open data folder** (or GitHub page, Privacy policy, opening a Shelf file or a notification's app, the update page) the notch could stay open and stop reacting to hover until Nookly restarted. Links now open after the click is handled, and a once-a-second check catches a lost "mouse left" message.
- Fixed: a drag in the Widgets order list that lost the mouse (Alt+Tab) kept the panel open.
  - The default order is the same as before. The System monitor's "Show on closed notch" switch is gone: CPU and network are widgets now (switched on for you if you used it).
- The clock now stays in the middle of the closed notch while something is
  live: music (between the album art and the equaliser), the timer and
  stopwatch, Pomodoro, prayer countdown, due tasks, AI agents and the
  system monitor. It is always centred on the notch. Top and bottom
  edges; a centred mascot keeps its spot.
- New notch style, **Slim**, between Full and Compact (Settings > Notch
  style, and in the welcome tour): Full's look and height, but narrower.
  The resting clock is 120 wide instead of 200, and while something is
  live (music, timer, stopwatch, Pomodoro, prayer, to-do, AI agents,
  monitor) it is always 200 instead of 300, with the clock in the exact
  middle. Tight labels shorten instead of moving the clock: Stopwatch
  shows as "Watch", an agent's "Needs you" as "OK?". The open panel is narrower and a little shorter too
  (about 464 x 198 instead of 552 x 230), with a slimmer tab bar. Tools
  and the Water settings page keep Full's panel size so nothing gets cut
  off; Settings and the welcome tour are unchanged.

## 0.1.21

- Taskbar mascot: Settings > Mascot > **Taskbar** moves the mascot into the
  empty corner of the Windows taskbar, where the Widgets weather goes (just
  right of the weather when that is on; beside the clock when the taskbar
  icons are on the left). The notch at the screen edge hides; hover the
  mascot and the panel opens right above it, click it to open, right-click
  for Settings. Alerts pop up there too. Picking a style or position, or
  dragging the notch, brings the notch back.
- README and website: one picture with every panel, and a What's new box on
  the website. The demo video is no longer shown.

## 0.1.20

- Makes room for maximized windows (Settings > General, on by default):
  with a maximized window under it, the notch slims to a thin bar that
  clicks pass through, so browser tabs under it stay reachable. Rest the
  mouse on the bar to open it; alerts still show. Fixes #2.
- 12-hour clock by default (4:20 PM), with a switch for 24-hour in
  Settings > General. Applies to the notch, reminders, prayer times, time
  zones and the timer.
- Show what I copy (Settings > Privacy, off by default): a small toast
  previews each copy, Android style: the text, the file names, or a
  thumbnail of the picture. Works with clipboard history off; nothing is
  kept, and password-manager copies stay private.
- Screenshot and Pick from screen: the notch now hides completely while the
  screen is frozen, and the panel no longer reopens under the cursor, so
  nothing covers the screen you're capturing.
- Music: when a player turns down previous/next (or play/pause), Nookly
  presses the matching media key instead, which most players and browsers
  still act on. The buttons dim when the player says it can't skip.

## 0.1.19

- AI agents tab: connect Hermes, Claude Code and Codex CLI with one switch, and the
  notch shows when they're working (with a timer), need your OK (pops up,
  orange) or finish. Only the status passes through; nothing leaves the PC.
- System monitor tab: CPU, memory and network with a one-minute graph, free
  disk space and uptime; optional CPU and download speed on the closed notch.
- Clipboard: search the history (Enter pastes the first match) and save
  snippets you paste often.
- Shelf: Screenshot button. Drag over part of the screen (or click for the
  whole screen); the PNG goes to Pictures\Screenshots, onto the Shelf and
  onto the clipboard.
- Uninstall Nookly... in the tray menu, and `nookly.exe --uninstall`:
  works even where the installer's own uninstaller is blocked, and
  disconnects the agents. The installer's uninstaller disconnects them too.
- The welcome tour has a new step for these panels.
- New Tools tab, all offline:
  - Color picker: pick any pixel on any screen with a magnifier, copy it as
    HEX, RGB or HSL, see its contrast on white and black, last 8 colors kept.
  - Calculator: answers as you type, with percentages (`200+10%`), powers,
    roots, trigonometry in degrees, factorials, hex and binary, and a history.
  - QR code: what you copied (or typed) as a QR code; copy it as an image.
  - Converter: length, weight, temperature, area, volume, speed and data,
    including katha, bigha, decimal, maund, seer and tola; and the time in 12
    cities with daylight saving handled by Windows.
- Clipboard: clicking an entry pastes it into the app you were using; the
  new copy icon (on hover) only copies. Apps running as administrator get
  "Copied, press Ctrl+V" instead.
- The notch opens on the panel you last picked, also after a restart. A
  reminder or a file drag that opens another panel doesn't change it.
- The welcome tour mentions the Tools tab; the demo and screenshots show it.
- Self-test fixes for GitHub's build servers (no notification listener there,
  and no mouse movement, which made the mascot doze).
- Releases are built by GitHub Actions from the version tag
  (`.github/workflows/release.yml`), as a draft release to check and publish.
- `nookly.exe` and the installer carry version info (product name and
  version).
- The installer shows a short privacy summary and has a "Check for updates
  automatically" option; the same switch is in Settings > General.
- README: what to do when Smart App Control blocks the installer.

## 0.1.18

- Windows notifications in the notch (opt-in: Settings > Privacy).
  Other apps' notifications drop out of the notch with the app's icon and
  colour; click to open the app, or mute an app. Message text can be hidden
  (app name only). Notifications that arrive during Do Not Disturb or focus
  (which Windows 11 turns on by itself for games and full-screen apps), or
  during a full-screen game or video, wait and then show as one ("+2 more").
  Windows' own banner keeps showing unless you turn banners off, and
  Settings links straight there. Uses Windows' notification listener (no
  signing or packaging needed) and only looks when Windows' notification
  database changes, so it costs nothing while nothing arrives. App icons
  come from the shell (the app's Start icon), which works for Store apps like
  WhatsApp and desktop apps alike, and a watchdog restarts the watcher if a
  call into Windows ever stops responding.
- Prayer times panel. Times are calculated on your PC from the sun's
  position (nothing is fetched): pick one of 16 Bangladesh cities or type a
  latitude and longitude, choose the method (Karachi, MWL, ISNA, Egypt,
  Makkah) and Shafi or Hanafi Asr, and nudge any prayer by a few minutes to
  match your mosque. Shows the next prayer with a countdown; optional
  reminders per prayer (on time or 5 to 15 minutes before) and a countdown in
  the closed notch for the last 15 minutes. Reminders are off until you turn
  them on.
- Settings > Panels: show or hide any panel. A hidden panel is fully off (no
  tab, no reminders, and clipboard history stops recording). One panel always
  stays.
- The mouse wheel on the open notch switches panels (down for the next).
  Lists that can scroll (to-do, clipboard, lyrics) still scroll, and the
  wheel over the tab bar always switches. The timer's hidden "wheel changes
  the duration" is gone (it has buttons).
- Mascot: **Always** shows it all the time, not only when idle, and
  **Left / Center / Right** places it in the notch, with the clock or the
  live activity (music, timer, prayer countdown) beside it.
- The welcome tour has a new last step, Settings at a glance (style, mascot,
  sounds, panels, notifications, clipboard, prayer times, updates), with an
  **Open Settings** button, and its tips now include the mouse wheel.
- The README demo starts with the welcome tour and shows the mouse wheel,
  the prayer panel and reminder, mascot placement and the Panels page.
- Settings has three pages on the right: General, Privacy (notifications,
  mic and camera dots, clipboard history) and Panels.
- Privacy policy (PRIVACY.md), linked from Settings > General and the
  README.
- Fixed animations stalling to a few frames a second on an otherwise idle
  screen: frames were paced with DwmFlush, which waits for the next desktop
  composition (about 250 ms when nothing else changes). Frames now sleep
  until the display's next refresh, from the compositor's timing.
- New README demo video and screenshots, rebuilt from the real app by
  `docs/tools/make_images.py` (`nookly --render-demo` plays a scripted
  session on a virtual clock).

## 0.1.17

- Update checks keep working when GitHub's API refuses: it allows only 60
  requests an hour per internet address, and many ISPs share one address
  between lots of customers, so checks could silently fail for days. Nookly
  now falls back to the normal release pages, and retries a failed check
  within the hour instead of 12 hours later.
- The downloaded installer is checked against the SHA-256 that GitHub
  publishes for it before it runs.
- Installers larger than 4 MB no longer fail to download.

## 0.1.16

- People updating from a version without the welcome tour now see it once
  too (it was only shown on brand-new installs). Their current style and
  position are pre-selected, so nothing changes unless they pick.

## 0.1.15

- Much smoother animation. Frames were timed with a Windows timer, which
  ticks in ~15.6 ms steps: tab switches ran at about 21 fps and opening the
  notch at about 63 fps. Transitions (tab switch, highlight glide, opening,
  closing, notices) now draw once per display refresh, so 144 fps on a 144 Hz
  screen, and live content (lyrics, progress, stopwatch) uses a precise
  60 fps timer. Nothing changes when idle: no frames are drawn.
- With NOOKLY_DEBUG=1, each animation's frame rate and worst gap is
  written to debug.log.

## 0.1.14

- The selected tab icon no longer moves at all: it stays still and only fades
  from its tab colour to white, while the highlight glides to it.

## 0.1.13

- The newly selected tab icon now makes one small, smooth hop instead of
  growing and shrinking (which looked shaky).

## 0.1.12

- Choose the reminder sound in Settings: Chime (as before), Bell, Pop,
  Marimba, your Windows notification sound, or Off. Picking one plays it.
- The self-test no longer plays sounds while building.

## 0.1.11

- Fixed the notch (especially the Pill) sometimes vanishing until the Start
  menu was opened. Windows' "busy" signal also fires while the desktop has
  focus and for some normal windows; Nookly now hides only when a window
  really fills the screen (games, full-screen video, presentations), never
  for the desktop, Start, the taskbar or a maximized window, and re-checks the
  moment another window comes to the front.
- Fixed reminders chiming while their notice was hidden. A notice that
  arrives while the notch is hidden now waits, and chimes when it actually
  appears. The notch also re-raises itself above other always-on-top windows
  when a notice shows or it opens.

## 0.1.10

- Welcome tour on first run, inside the notch: 1. pick how the closed notch
  looks (Full, Compact, Pill or Hidden, each with a little preview),
  2. pick the edge, 3. three quick tips. Choices apply straight away, and the
  last notice says where to change them later. Skip any time; replay it from
  the tray menu (Welcome tour).

## 0.1.9

- Every tab now opens to the same panel size, so the tab bar and buttons
  stay put while switching (only Music's lyrics view is wider).
- Switching tabs slides the old content out and the new content in from the
  side you moved toward. The highlight glides to the new tab, which glows in
  its own colour and pops briefly.
- Clipboard history is now off until you turn it on (in the tab or in
  Settings), and records text only by default. Settings offers Text,
  Text + files, or Everything, which adds images with thumbnails (images
  stay in memory only and can't be pinned).
- Mic and camera dots: an orange (microphone) or green (camera) dot appears
  beside the notch while any app is using them, with a short notice naming
  the app. Turn it off in Settings.

## 0.1.8

- Clipboard history tab: text and files you copy are listed (newest first,
  up to 30). Click one to copy it again, pin the ones you want to keep across
  restarts, remove or clear the rest, or pause recording. Unpinned entries
  are kept in memory only. Copies that password managers mark as private are
  never recorded.
- The tab bar header drops the date, then the title, when a narrow panel has
  no room, instead of overlapping.

## 0.1.7

- Settings panel inside the notch: click the gear at the end of the tab bar,
  right-click the notch, or left-click the tray icon. It has notch style,
  position, mascot, Start with Windows, volume overlay, updates, data folder,
  GitHub page and Quit. The tray menu is now just Settings, Check for
  updates, About and Exit.
- Mascots: pick Eyes, Cat, Ghost, Robot, Sparky, Sprout, Owl or Panda (or
  Clock for none). They all follow the mouse, blink, doze and wave. Add your
  own in `src/mascot.rs`.
- Smoother resizing when switching tabs: the notch now moves on a soft
  spring, the tab bar glides with it, and the new tab's content fades in
  instead of popping.

## 0.1.6

- Fixed toast buttons (timer Restart/Dismiss, reminders, Pomodoro) being hard
  to click: moving the mouse onto a toast used to open the full panel on top
  of it. A toast with buttons now stays while hovered, and after clicking one
  of its buttons the panel only opens again once the mouse has left.
- Tooltips: resting the mouse on an icon-only button (tabs, music controls,
  undo, settings, delete, +/-) shows what it does.
- `Ui::icon_btn` takes a tooltip, and `Ui::hit_tip` adds one to any hit.

## 0.1.5

- Update check: shortly after start and twice a day Nookly looks for a
  newer GitHub release and offers Update / Later / Skip. Update downloads
  the installer, upgrades in place and restarts. Also in the tray menu as
  Check for updates. About now shows the version.
- Bug report and feature request forms on GitHub.
- Fixed album art sometimes staying on the previous song: players publish
  the new title before the new cover, so the cover is now re-checked for a
  while after each track change.

## 0.1.4

- Fixed the Timer's -1 min button being cut off and the Water settings
  buttons being squashed.
- README screenshots, demo GIF and user guide (`docs/`), rendered from the
  real app with `--render-docs` and `docs/tools/make_images.py`.

## 0.1.3

- Put the notch on any screen edge (top, bottom, left, right) and anywhere
  along it, on any monitor. Right-click and drag the notch to move it, or use
  Position in the tray menu. Side edges get a vertical tab; panels stay upright.

## 0.1.2

- Timer tab: countdown with presets (1 min to 1 h), +/-1 min and scroll to
  adjust, chime and Restart/Dismiss toast when done. Stopwatch with laps.
  Both show in the collapsed notch while running.
- The self-test can now run while the app is open.

## 0.1.1

- Notch style setting in the tray menu: Full, Compact, Pill (thin bar, tinted
  while music or a timer is live) and Hidden (hover the top edge).
- Smaller styles open only after the mouse rests briefly, so reaching for
  browser tabs doesn't open the notch.

## 0.1.0

- First release: notch window, now playing, synced lyrics, Pomodoro, water
  reminder, to-do, file shelf, system overlays, mascot, tray menu, installer.
