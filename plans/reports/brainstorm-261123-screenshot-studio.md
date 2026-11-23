# Brainstorm — Screenshot Studio

Date: 2026-11-23 · Status: design APPROVED by owner · Next: `/t1k:plan`

## Problem
Owner has 5,604 PNG screenshots (1.7 GB, plus subfolders `1ERD`, `4ERD`) in
`C:\Users\Nghaiz\OneDrive\Pictures\Screenshots` (OneDrive-synced). Wants a desktop app,
launchable from Windows Search, to browse/search/filter/organize/edit them, with as many
features as possible and a rich, animated, multi-theme UI (not a plain white frame).

## Decisions (owner-confirmed)
| Topic | Decision |
|---|---|
| Stack | **Tauri 2 (Rust core) + React/TS UI + Python AI sidecar** (owner: "Python + Rust") |
| File access | Full: move, rename, Recycle Bin, **permanent delete** (separate confirm) |
| v1 advanced | OCR, duplicate/similar, smart grouping, quick editing, "as many more as possible" |
| Visual | Many themes + varied visual styles |
| Name | **Screenshot Studio** (Start Menu / Windows Search entry) |
| Install location | **Everything on D:** (C: has only 29 GB free) |
| Rust setup | Machine-wide, reusable for future projects, not project-only |

## Environment scan (2026-11-23)
- Present: Node 26.13, npm 12.2, Python 3.13, uv 0.13.8, .NET 10, git 2.56, winget 1.30,
  WebView2 157, RTX 4060 Laptop 8 GB.
- Missing: Rust, **VS C++ Build Tools** (required by MSVC Rust toolchain).
- Existing D: convention: `D:\DevCache\tools\*` (tools), `D:\DevCache\cache\*` (caches),
  `D:\IOT_Tools\apps\*` (apps), `D:\IOT_Tools\data\*` (data). Env already sets
  `UV_CACHE_DIR`, `UV_PYTHON_INSTALL_DIR`, `UV_TOOL_DIR`, `PIP_CACHE_DIR` to D:.
- VS `SharedInstallationPath` is already fixed at `C:\Program Files (x86)\Microsoft Visual Studio\Shared`
  (set by SSMS 21) and cannot be moved → a small share of VS components + Windows SDK still land on C:.

## D: drive layout
| What | Path | Mechanism |
|---|---|---|
| rustup toolchains | `D:\DevCache\tools\rust\rustup` | user env `RUSTUP_HOME` |
| cargo bin/registry | `D:\DevCache\tools\rust\cargo` | user env `CARGO_HOME`, `%CARGO_HOME%\bin` on PATH |
| sccache | `D:\DevCache\cache\sccache` | `SCCACHE_DIR`, `RUSTC_WRAPPER=sccache` in `~/.cargo/config.toml` |
| VS Build Tools | `D:\IOT_Tools\apps\VSBuildTools` | `--installPath` |
| VS package cache | `D:\DevCache\cache\vs-packages` | HKLM `...\VisualStudio\Setup\CachePath` |
| HF / AI models | `D:\DevCache\huggingface` | user env `HF_HOME` |
| Installed app | `D:\IOT_Tools\apps\ScreenshotStudio` | NSIS install dir |
| App data (DB + thumbs) | `D:\IOT_Tools\data\ScreenshotStudio` | app config, default |
| Source | `D:\Screenshot_Picture` | local git, no remote |

## Approaches evaluated
| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Electron + React | Fastest UI dev, rich effects | ~150 MB RAM, no Rust | rejected (owner wants Rust) |
| WPF/WinUI .NET | Native, light, Mica | Slower dev, harder animations | rejected |
| Local web + browser | Simplest | Not a real app, server running | rejected |
| **Tauri 2 + React + Python sidecar** | Rust perf for scan/thumb/IO, web UI freedom for themes/effects, Python ecosystem for OCR/CLIP | 3 languages, sidecar lifecycle | **chosen** |

## Architecture
- **Rust core (Tauri 2):** walkdir scan, `notify` watcher (live new screenshots), parallel WebP
  thumbnails (`image` + `fast_image_resize` + rayon), SQLite via rusqlite + FTS5 (OCR text,
  diacritic-insensitive), `image_hasher` pHash/dHash, `trash` crate + permanent delete, undo
  journal, clipboard, tray, global hotkey (Ctrl+Alt+S), single-instance, autostart.
- **UI:** React + TS + Vite + Tailwind + Framer Motion + TanStack Virtual (10k+ items smooth),
  theme engine on CSS variables, i18n vi/en.
- **Python sidecar (uv venv, JSON-RPC over stdio):** RapidOCR/PaddleOCR (vi+en), multilingual
  CLIP/SigLIP on CUDA (semantic search, zero-shot categories), dominant colour, PII detect for
  auto-redaction. Optional: app degrades gracefully (AI features disabled + visible notice) if absent.
- **Contract:** Rust↔UI via typed Tauri commands/events (shared TS types generated from Rust,
  e.g. `ts-rs`); Rust↔Python JSON-RPC schema pinned in one file before parallel work.

## Feature set (8 groups)
1. **Browse:** grid/masonry/justified/list, timeline + date scrubber, thumb-size slider, lightbox
   (zoom/pan, filmstrip, slideshow), side-by-side compare, eyedropper, keyboard nav, Ctrl+K palette.
2. **Search/filter:** filename, OCR text, semantic (CLIP), date range, resolution, aspect
   (phone/PC), file size, folder, tag, colour, category, favourite, has-text; saved smart filters; sorts.
3. **Organize:** coloured tags, virtual albums, real folders (move/copy, drag-drop), stars,
   favourites, notes, pattern bulk rename, lasso/shift select, undo, in-app trash + Recycle Bin + permanent delete.
4. **Smart:** session grouping (time gap), device/resolution, dominant colour, AI categories,
   duplicate/similar finder (keep best), blurry/tiny detection, cleanup wizard, stats dashboard.
5. **Edit:** crop, rotate/flip, annotate (arrow/box/text/highlight/numbered steps), blur/pixelate,
   OCR-based auto-redact email/phone, resize, compress/convert, stitch, save copy/overwrite.
6. **Share/export:** copy image / OCR text, drag-out, ZIP/PDF export, reveal in Explorer, open-with.
7. **Live:** watcher + toast on new screenshot, tray "recent" popup, autostart.
8. **Themes/UX:** 10+ themes (Aurora Glass, Midnight Neon, Cyberpunk, Fluent Mica Light/Dark,
   Sakura Pastel, Sunset, Ocean, Forest, Mono Minimal), custom accent, animated background toggle,
   animation intensity, reduced-motion, density, vi/en.

## Phases
| Phase | Scope | Check |
|---|---|---|
| P0 | Machine-wide Rust on D: + VS Build Tools + cargo tools + VS Code ext; `git init` | `cargo new` hello + `cargo tauri --version` build OK; paths resolve to D: |
| P1 | Tauri scaffold, scan, thumbs, DB, grid, lightbox, theme engine | 5,604 images indexed; grid scrolls at 60 fps |
| P2 | Search/filter, organize, file ops + undo, NSIS installer + Start Menu | "Screenshot Studio" found in Windows Search and launches |
| P3 | Python sidecar: OCR, CLIP, duplicates, grouping, dashboard | OCR search hits known text; dup finder finds a planted copy |
| P4 | Editor, export, tray, watcher, polish | New screenshot appears live; edit round-trip saves |

## Risks
- **OneDrive Files On-Demand:** first thumbnail pass may download all 1.7 GB; run in background,
  show progress, resumable. Never pin/unpin files on owner's behalf.
- First OCR + CLIP over 5,600 images ≈ 10–20 min on RTX 4060; background, resumable. Models ≈ 1–2 GB on D:.
- Permanent delete is irreversible → separate typed/explicit confirmation, never default.
- VS Build Tools needs UAC once; part of SDK lands on C: (shared path fixed by SSMS).
- Scope is large → strict phase gates; app usable from P2.

## Success metrics
Indexed count == files on disk; cold start < 2 s after first index; grid 60 fps at 5,600+ items;
search < 200 ms; every destructive op confirm-gated and undoable except permanent delete;
launches from Windows Search.
