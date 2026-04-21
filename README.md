# Brave Offline Config Editor (boce)

A Rust tool that reads and edits Brave browser configuration files **while Brave
is closed**. Cross-platform (Windows / Linux / macOS), runs as a GUI or a CLI.

<img width="1786" height="1298" alt="boce" src="https://github.com/user-attachments/assets/8dedb74c-2a62-4305-8e72-c34c85aea5d6" />

Useful when:

- `brave://settings/…` is slow or unresponsive on a large profile.
- You want to script configuration changes across profiles or machines.
- You want transparent, diff-able, timestamped backups of every change.

## What it edits

Everything below is stored as JSON under Brave's user-data directory. The tool
only touches the files described here, never `Secure Preferences`
(Brave's HMAC-signed file) and never bookmarks / extensions / installed files.

| Tab | What it does |
| --- | --- |
| **Get started** | Profile display name, "On startup" mode (NTP / resume / URLs). Read-only channel info. |
| **Shields** | Per-profile Shields defaults: Trackers & Ads (Disabled / Standard / Aggressive), HTTPS upgrade, Block scripts, Block fingerprinting, Block cookies, Forget-me. |
| **Filters** | Brave adblock filter lists (regional + custom subscriptions + custom rules). Loads Brave's on-disk filter-list catalog to show real names. |
| **Privacy** | De-AMP, Debounce tracking URLs, Reduce language fingerprint, Send DNT, WebRTC IP handling policy, Off-The-Record for sensitive sites, Google push messaging, Block Microsoft Recall, Tor-in-private-window, .onion-only-in-Tor. |
| **Delete Data** | Like Brave's "Delete browsing data" — multi-select profiles × categories (history / cookies & site data / cache / passwords / autofill). Confirmation dialog shows exact paths and byte counts. |
| **System** | Background apps on close, hardware acceleration, warn-before-closing-multiple-tabs, close window on last tab, full-screen reminder, Brave VPN WireGuard, Memory Saver. |
| **Flags** | Edit `browser.enabled_labs_experiments` (brave://flags). Add / remove / reset all. |
| **Preferences** | Raw JSON editor for `Preferences`. Load live file or any backup, pretty-print / minify, JSON validation on save. |
| **Backups** | Timestamped `.bak-<stamp>` copies are created automatically before every write. Per-row Restore, Diff-vs-current, and Open-in-editor. |

Most shield / privacy / flag keys live either in the per-profile `Preferences`
JSON or the channel-wide `Local State` JSON. Writes are grouped by file so each
Apply produces at most one `.bak-<timestamp>` per file.

## Safety

- **Close Brave first.** The tool detects a running Brave for the selected
  channel and disables mutating actions. Brave rewrites its own prefs on exit,
  so any write made while Brave is open gets silently overwritten. When a
  mutation is disabled, hovering the disabled button shows the reason.
- **Every write is backed up** with a timestamped sibling file (e.g.
  `Preferences.bak-20260421-223243`). Restore is one click in the Backups tab.
- **Every write is verified.** After the atomic rename, the tool re-reads the
  file from disk and compares the intended values. If Brave was somehow still
  running and overwrote the file, the tool reports a loud error instead of
  silent success.
- **Never writes `Secure Preferences`.** The HMAC-signed file is left alone.
- **Flag / URL input is not validated.** Brave silently ignores unknown flag
  names or junk subscription URLs — the tool can't warn on typos.

## CLI

Some common operations are exposed as subcommands for scripting:

```
brave-offline-config-editor list
brave-offline-config-editor --channel nightly clear-cache --dry-run
brave-offline-config-editor --channel beta clear-cache --all
brave-offline-config-editor --channel stable shields list-keys
brave-offline-config-editor --channel nightly shields set trackers-and-ads aggressive
brave-offline-config-editor --channel beta backup --profile Default
```

Running the binary with no subcommand launches the GUI. `--help` lists the
full set of subcommands with examples.

On Windows the binary is compiled as a console-subsystem exe. When launched
from Explorer the console is hidden on startup so it's GUI-only; when launched
from `cmd` / PowerShell the console stays visible and `Ctrl+C` cleanly exits
the GUI.

## Install paths detected

- **Windows** — `%LOCALAPPDATA%\BraveSoftware\Brave-Browser[-Beta|-Nightly|-Dev]\` (per-user)
  and `%ProgramFiles%\BraveSoftware\...` / `%ProgramFiles(x86)%\...` (system).
- **Linux** — `~/.config/BraveSoftware/Brave-Browser[-Beta|-Nightly|-Dev]/` profiles,
  `~/.cache/BraveSoftware/...` cache, `/opt/brave.com/brave[-beta|-nightly|-dev]/` binaries.
- **macOS** — `~/Library/Application Support/BraveSoftware/...`, `~/Library/Caches/BraveSoftware/...`,
  `/Applications/Brave Browser[ Beta|...].app/`.

Flatpak / Snap Linux installs are not detected yet.

## Build

Requires Rust 1.88+ (edition 2024, let-chains).

```
# Native (Linux, macOS, or Windows)
cargo build --release
```

Cross-compile to Windows from a Linux host (requires mingw-w64):

```
rustup target add x86_64-pc-windows-gnu
sudo apt install mingw-w64
cargo build --release --target x86_64-pc-windows-gnu
```

Output: `target/release/brave-offline-config-editor[.exe]` or
`target/x86_64-pc-windows-gnu/release/brave-offline-config-editor.exe`.

`.cargo/config.toml` in this repo points cargo at the mingw linker for the
gnu target so the cross-compile just works.

## Dependencies

All widely-used crates:

- `eframe` / `egui` — GUI
- `clap` — CLI parser
- `serde`, `serde_json` — JSON parsing (with `preserve_order`)
- `dirs` — platform-specific user directories
- `sysinfo` — process enumeration (running-Brave detection)
- `ctrlc` — console control signal handler
- `chrono` — timestamp formatting
- `walkdir` — directory size calculation
- `anyhow` — error plumbing
- `windows-sys` (Windows only) — console hiding

## License

Mozilla Public License 2.0 — see [LICENSE](LICENSE).
