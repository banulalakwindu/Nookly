# Security policy

## Supported versions

Only the [latest release](https://github.com/banulalakwindu/Nookly/releases/latest)
gets fixes. Nookly updates itself (Settings > General > Check for updates).

## Reporting a vulnerability

Please report security problems privately, not in a public issue: use
GitHub's [private vulnerability reporting](https://github.com/banulalakwindu/Nookly/security/advisories/new)
for this repository.

Include what you found, how to reproduce it, and which version you used.
You'll get a reply within a week, and credit in the release notes if you'd
like it.

## Good to know

- Nookly runs as your user, with no admin rights, and installs only to
  `%LOCALAPPDATA%\Programs\Nookly`.
- The in-app updater checks every download (the taskbar add-on too)
  against the SHA-256 published with the GitHub release, and refuses what
  it can't check.
- Everything Nookly stores is plain text in `%APPDATA%\Nookly`; see
  [PRIVACY.md](PRIVACY.md) for what it reads and when it goes online.
