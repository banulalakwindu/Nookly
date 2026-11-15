# Contributing to Nookly

Thanks for helping make Nookly better. Bug reports, ideas, docs fixes and
code are all welcome.

## Before you start

- **Bugs and ideas:** open an [issue](https://github.com/banulalakwindu/Nookly/issues/new/choose)
  with the bug or feature form. Check the [user guide](docs/USER_GUIDE.md)
  and the latest release first.
- **Bigger changes** (a new panel, a new setting, anything that changes how
  Nookly looks or behaves): open an issue first so we can agree on the
  approach before you spend time on it.
- **Security problems:** please don't open a public issue; see
  [SECURITY.md](SECURITY.md).

## Build and test

Nookly is a single Rust program for Windows 10 and 11. You need Rust
(stable) and nothing else.

```powershell
cargo build --release                       # target\release\nookly.exe
.\target\release\nookly.exe --selftest     # headless checks (writes selftest.txt)
.\build.ps1                                 # build + icon + version info + self-test + installer
```

The self-test runs against a throwaway data folder when you set
`NOOKLY_DATA_DIR`, so it never touches your own settings:

```powershell
$env:NOOKLY_DATA_DIR = "$env:TEMP\nookly-test"; .\target\release\nookly.exe --selftest
```

Every line should start with `OK`. The tab-switch frame-rate check depends
on how busy your PC is; an occasional FAIL there is not a blocker.

## How the code is laid out

| Path | What's there |
|---|---|
| `src/app.rs` | The notch window: layout, animation, input, toasts, the mascot, self-tests |
| `src/app/settings.rs` | The Settings window (sections, master switches) |
| `src/dialog.rs` | Nookly's own small windows: About, uninstall, the tray menu |
| `src/setup.rs` | The installer (it's nookly.exe under its setup name) |
| `src/modules/` | One file per panel or feature, each implementing the `Module` trait |
| `src/gfx.rs`, `src/ui.rs` | Direct2D drawing, themes, and the small UI helpers |
| `src/glass.rs` | The Liquid Glass backdrop |
| `src/features.rs` | Feature switches, presets, shortcuts, export / import |
| `docs/` | The website (GitHub Pages), the user guide and screenshots |

**Adding a panel:** copy `src/modules/example.rs`, implement `Module`, and
add it to `modules::all()` and to `features::FEATURES`.

## Guidelines

- **Light first.** Nookly should idle near zero CPU. Prefer reacting to
  events over polling, do slow work on a background thread, and only draw
  when something changes. A feature that's switched off must do no work.
- **Private by default.** No telemetry. Anything that goes online is opt-in
  and listed in [PRIVACY.md](PRIVACY.md).
- **Plain words.** Labels and tooltips say what a control does, briefly.
  Every clickable thing has a tooltip (the self-test checks this).
- **Style.** `cargo fmt` (see `rustfmt.toml`), comments that explain why,
  and a self-test check for new behaviour where it can be tested headless.
- **Screenshots.** If you change how something looks, regenerate them:
  `python docs\tools\make_images.py shots` (needs Python with Pillow and numpy).

## Branches, commits and pull requests

- Branch from `main` and name it by what it does: `feature/panel-order`,
  `fix/weather-place`, `docs/contributing`.
- Keep commits small and focused, with a short summary line in the
  imperative ("Add drag to reorder panels") and a body when the why isn't
  obvious.
- Open a pull request against `main`, fill in the template, and make sure
  the build passes (GitHub Actions runs it on every push).
- Releases are made by the maintainer: raising `version` in `Cargo.toml`
  with a matching `## <version>` section in `CHANGELOG.md` publishes the
  installer and updates the website.

## License

By contributing, you agree that your contributions are released under the
[MIT License](LICENSE).
