# Screenshot Studio

A Windows desktop app for browsing, searching and cleaning up a screenshot library: Tauri 2 (Rust) with a
React/TypeScript UI and an optional Python sidecar for OCR, semantic search and image classification.
Everything runs locally. Nothing is uploaded.

This page covers building, installing, the AI setup, where data lives and how to uninstall. Hard-won gotchas are
in [lessons.md](lessons.md). The design and phase plan are in `plans/261123-screenshot-studio/`.

## Layout

| Path | What |
|---|---|
| `crates/ss-core/` | Catalog, indexer, file ops, AI supervisor, duplicates, watcher, export, edit; the `ssctl` CLI (`src/bin/ssctl.rs`) |
| `src-tauri/` | The Tauri app: commands, `ssimg` image scheme, tray, hotkey, NSIS config |
| `src/` | React UI (themes, grid, lightbox, organize, smart views, editor, settings) |
| `sidecar/` | Python JSON-RPC sidecar (`ss_sidecar`): OCR, SigLIP embeddings, zero-shot, colour, PII |
| `contracts/` | JSON schemas and fixtures shared by Rust, TS and Python |
| `scripts/accept/` | Phase Accept scripts (`p1.mjs` ... `p4.mjs`, `lane-tests.mjs`) |

## Build

Prerequisites (phase 0 sets them up machine-wide with `scripts/setup-rust-machine.ps1`): Rust stable (MSVC),
VS Build Tools with the C++ workload, Node 26, `cargo install --locked tauri-cli`, and `npm ci` in the repo.

```powershell
npm ci
$env:RUSTC_WRAPPER = ""      # sccache cannot pass the app crate's 41,000-char rustc command line (lessons.md)
$env:CARGO_BUILD_JOBS = "2"  # the default job count can run the machine out of commit memory (lessons.md)
cargo tauri build            # runs `npm run build`, then the release app and the NSIS installer
```

Output: `target\release\bundle\nsis\Screenshot Studio_<version>_x64-setup.exe`. A cold release build takes about
25 minutes; incremental ones take a few. Also run the CLI build when you need `ssctl`:
`cargo build --release -p ss-core --bin ssctl`.

Tests: `cargo test --workspace --locked` and `npx vitest run` (run vitest from `D:\`, upper-case drive letter,
or it finds no suites; `node scripts/accept/lane-tests.mjs <files>` handles that for you).

## Install

```powershell
& "target\release\bundle\nsis\Screenshot Studio_1.0.0_x64-setup.exe" /S /D=D:\IOT_Tools\apps\ScreenshotStudio
```

- Per-user install (no UAC) into `D:\IOT_Tools\apps\ScreenshotStudio`. NSIS keeps the previous folder on an
  upgrade, so installing a new version over the old one with the same command is the update path.
- It adds a Start Menu shortcut, so "Screenshot Studio" shows up in Windows Search. On purpose there is no
  Desktop shortcut: this Desktop sits inside OneDrive (`src-tauri/windows/installer-hooks.nsh`).
- Single instance: launching it again brings the running window forward. Ctrl+Alt+S does the same from anywhere
  (configurable in Settings).
- Close the app before installing over it.

## First run

Settings > Library > Add folder. The default suggestion is `C:\Users\Nghaiz\OneDrive\Pictures\Screenshots`, which
is a protected root: the app reads it, and writes to it only through journaled, undoable actions you start in the
UI. Files that OneDrive keeps cloud-only are indexed from metadata alone. They get thumbnails only after you press
"Download cloud-only" in Settings > Library, which can pull a lot onto C:. The app never pins or unpins OneDrive files.

While "Watch for new screenshots" is on (Settings > Library), new screenshots show up within a few seconds, with a toast. Screenshots taken while
the app was closed (or the watcher paused) are picked up by one quiet rescan when it starts again: they
appear in the library without a toast.

## AI setup (optional)

Without the sidecar the app works fine: browsing, file-name and note search, organize, exact duplicates and
editing all work, and the AI features are disabled with a notice. To turn them on:

```powershell
$env:UV_PROJECT_ENVIRONMENT = "D:\IOT_Tools\data\ScreenshotStudio\sidecar-venv"
$env:HF_HOME = "D:\DevCache\huggingface"
uv sync --project sidecar                                     # torch cu132 + onnxruntime + OCR, ~3-5 GB
& "$env:UV_PROJECT_ENVIRONMENT\Scripts\python.exe" -m ss_sidecar --prefetch   # model weights into HF_HOME
& "$env:UV_PROJECT_ENVIRONMENT\Scripts\python.exe" -m ss_sidecar --selftest   # one JSON line, "ok": true
```

The installed app starts the sidecar from that venv (setting `sidecarVenv`), with the sidecar source bundled in
`<install dir>\sidecar`. It uses CUDA when available, else the CPU. Then run Settings > AI > "Run AI on library".
The first full pass over ~5,700 images took about 2.5 hours on an RTX 4060 laptop GPU; OCR is most of that time.

## Where data lives

| What | Where |
|---|---|
| App | `D:\IOT_Tools\apps\ScreenshotStudio` (exe, bundled sidecar source, uninstaller) |
| Catalog (SQLite), thumbnails, settings | `D:\IOT_Tools\data\ScreenshotStudio\` (`catalog.sqlite`, `thumbs\`) |
| Backups of overwritten edits | `D:\IOT_Tools\data\ScreenshotStudio\edit-backups\` (undo restores from here) |
| WebView2 profile | `D:\IOT_Tools\data\ScreenshotStudio\webview\` |
| Sidecar venv and its bytecode cache | `D:\IOT_Tools\data\ScreenshotStudio\sidecar-venv\`, `...\sidecar-pycache\` |
| AI models | `D:\DevCache\huggingface` |
| UI preferences (theme, density, language, layout) | WebView2 `localStorage`, inside the profile above |
| Your screenshots | wherever the library roots point; the app never moves them on its own |

`SS_DATA_DIR` points the app and `ssctl` at another data dir. Tests and Accept scripts use it with temp folders.

## `ssctl`

A CLI over the same core, for scripting and checks: every subcommand prints one JSON document with `--json`.
`ssctl --help` lists them (add-root, index, list, stats, search, journal, ai, dupes, status, edit, undo, get/set).
It never exposes permanent delete, move, rename or the Recycle Bin, and it refuses file writes under a protected
root or any OneDrive path.

## Uninstall

Settings > Apps > Installed apps > Screenshot Studio > Uninstall, or run
`D:\IOT_Tools\apps\ScreenshotStudio\uninstall.exe`. This removes the app folder and the Start Menu shortcut.

Your data stays: the catalog, thumbnails, backups and the sidecar venv in `D:\IOT_Tools\data\ScreenshotStudio`,
and the models in `D:\DevCache\huggingface`. Delete those folders by hand to remove everything. Your screenshots
are never touched by an uninstall.

## Acceptance

`node scripts/accept/p4.mjs` runs the release check: p1-p3 again, a live watcher check and an edit round-trip
on the installed app with a temp data dir, and the design's success metrics (indexed count equals files on disk,
cold start under 2 s, search under 200 ms, grid fps, journal coverage, Start Menu shortcut). It needs the
current build installed and Screenshot Studio closed, and it takes about half an hour.
