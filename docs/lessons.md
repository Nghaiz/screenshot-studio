# Lessons (project-local)

Non-obvious gotchas found while building Screenshot Studio. Owner rule: lessons stay in this repo, nothing goes
to a kit. Newest first. One entry: what happened, why, what to do.

## 2026-11-24 — phase 2.3

### Browsing the whole real library costs ~720 MB: Chromium keeps ~2,000 decoded thumbnails
- **Seen:** `p2.mjs` `memory`: 723-748 MB (app + WebView2 private) after paging screen by screen through the real
  library (5,662 loadable 320 px thumbnails), against the 500 MB budget. A 10 s top-to-bottom scroll (the 1.4
  measurement) ends at 476-489 MB, because rows that fly past never load (2.2's settle delay).
- **Why (memory-infra dump over CDP `Tracing.requestMemoryDump`):** the renderer's compositor image decode cache
  (`cc/image_memory`) held 436 MB after ~1,800 thumbnails and does not shrink on GC or a simulated critical
  memory-pressure signal. It caps by entry COUNT (~2,000), so memory = 2,000 x decoded size (320x180x4 = 230 KB).
  On top sits a fixed WebView2 cost of 380-417 MB at the first screen, of which the GPU process alone is 230-300 MB
  (NVIDIA RTX 4060 Laptop, the only adapter; most of it is driver memory Chromium does not track).
- **2.2 missed it** because its 6,300 fixture PNGs are 96-160 px: their decoded thumbnails are tiny.
- **Tried, measured, not kept:** `--force-gpu-mem-discardable-limit-mb=32` and `--force-gpu-mem-available-mb=96`:
  no change. `--disable-gpu-rasterization`: GPU process -140 MB, grid p50 4.2 -> 12.6 ms. `--disable-gpu`: 433 MB
  but 9 fps. `--enable-low-end-device-mode`: decode cache halved (218 MB), total still ~560 MB, broad side effects.
  Grid thumbnails as canvases: an unaccelerated canvas is painted into the grid's tiles as an image (615 MB of
  decode cache, worse); `drawImage(bitmap)` on an accelerated canvas fills the GPU transfer cache (256 MB);
  `putImageData` into an accelerated canvas (CORS read of `ssimg`) removed `cc/image_memory` entirely, but the
  renderer still ended at ~290 MB after GC (allocator high-water) and the total at 703 MB, within noise of `<img>`.
- **Do:** treat the 500 MB budget as an owner decision (the fixed WebView2 cost leaves ~100 MB of headroom on this
  machine). Measure memory on the real library, never on small fixtures. `memory-infra` dumps name the owner of the
  bytes; private-bytes totals alone do not.

### A silent NSIS install writes a Desktop shortcut, and this Desktop is in OneDrive
- **Seen:** Tauri's installer.nsi (tauri-bundler 2.10) creates `$DESKTOP\<product>.lnk` on every `/S` or `/P` install;
  `/NS` drops the Start Menu shortcut too. `[Environment]::GetFolderPath('Desktop')` is `C:\Users\Nghaiz\OneDrive\Desktop`.
- **Do:** `src-tauri/windows/installer-hooks.nsh` forces no-shortcut mode in PREINSTALL and creates only the Start
  Menu shortcut in POSTINSTALL. `p2.mjs` `no-desktop-lnk` fails if one appears.

### The installed exe is not byte-identical to target\release
- **Seen:** 3 bytes differ. The bundler stamps `__TAURI_BUNDLE_TYPE_VAR_NSS` into the copy it packs and restores
  `__TAURI_BUNDLE_TYPE_VAR_UNK` in `target\release\screenshot-studio.exe` afterwards.
- **Do:** compare after normalizing that one marker (`p2.mjs` `installed-exe`).

### Windows reuses PIDs: a process tree from ParentProcessId can include strangers
- **Seen:** `treeMemory` counted a 24 MB `NhNotifSys.exe` (started at 00:15, over an hour before the app) under the
  WebView2 crashpad handler, whose PID had once belonged to its real, dead parent.
- **Do:** a child must have started after its parent (`run.mjs` `treeMemory` compares CreationDate).

### WebView2 data and the main window
- Tauri puts the WebView2 profile in `%LOCALAPPDATA%\<identifier>\EBWebView` unless the window builder gets
  `data_directory`, and a config window cannot take an absolute one: the main window is `create: false` and built in
  `setup` (`src-tauri/src/window.rs`). WebView2 appends `EBWebView` to the folder you give it.
- The old profile was moved once with `robocopy /E /MOVE` so the UI's `localStorage` (theme, density) survived.
- `/D=` works with `installMode: currentUser`; `cargo tauri build` writes to the workspace `target\`, not `src-tauri\target\`.

### Icon rendering needs a matching Playwright browser
- Playwright 1.65.1 wants Chromium revision 1255; this machine has 1254, and Edge's `msedge.exe` is absent (only
  WebView2 parts). `src-tauri/icons/source/render.mjs` takes `SS_CHROMIUM=<chrome.exe>` for another build.

## 2026-11-23 — phase 2.1

### FTS5 `remove_diacritics` does not fold `đ` / `Đ`
- **Seen:** a note "hóa đơn" was not found by the search `hoa don`, while `anh chup` finds `Ảnh chụp.png`.
- **Why:** `unicode61 remove_diacritics 2` strips combining marks after Unicode decomposition. `ả`, `ụ`, `ơ`
  decompose; `đ` (U+0111) is its own letter with no decomposition, so it stays `đ`.
- **Do:** users must type `đ` for words that contain it. Folding it needs a custom tokenizer or a second
  normalized column (a schema change, so an Owner decision); nothing does that yet.

### The `trash` crate panics on a thread that already holds COM in another mode
- **Seen in the source (trash 5.4.0, `windows.rs`):** every call runs `CoInitializeEx` (apartment-threaded) on
  the calling thread and `panic!`s if it fails, which it does on a thread already initialized as MTA.
- **Do:** `ss-core::ops::fsio` runs every Recycle Bin call on a fresh `std::thread`; never call `trash::*`
  directly from a Tauri/tokio pooled thread.

### A guard test can pass for the wrong reason: break the guard and watch it go red
- **Seen:** with every delete guard removed one at a time (scratch mutation script), "an unknown token is
  accepted" stayed GREEN: the test tried bogus tokens while no real ticket existed, so there was nothing to
  wrongly redeem.
- **Do:** a test that proves "X is refused" needs a live X it could wrongly reach. The test now holds a live
  ticket while trying bogus tokens; all 7 delete-guard mutations go red.

### `found invalid metadata files for crate ss_core` after many quick rebuilds
- **Seen:** after ~10 back-to-back `cargo test` rebuilds of ss-core, every test crate failed with E0786 and
  `found staticlib serde_with_macros instead of rlib`; `cargo clean -p ss-core` did not fix it.
- **Why (likely):** sccache served a bad artifact; the source compiled fine moments before.
- **Do:** rebuild with `RUSTC_WRAPPER=""` (bypasses sccache), as for the release app build above.
- **Seen again** with `RUSTC_WRAPPER=""` too, after more rebuilds with rust-analyzer running in VS Code:
  `only metadata stub found for rlib dependency core`, `can't find crate for std`. Cause unknown (sccache
  alone does not explain it). The same tree passed on CI, so trust the clean CI run over a churned local
  `target/`; a full `cargo clean` is the local fix.

### The Recycle Bin records the canonical path, not the path you deleted
- **Seen:** CI only: Recycle Bin undo answered "no longer in the Recycle Bin" right after a successful recycle.
- **Why:** the runner's temp dir resolves through an 8.3 short name (`C:\Users\RUNNER~1\...`). The root kept
  that spelling, `trash` canonicalizes before deleting, so the bin's original path is the long name and a
  plain string compare never matches. Locally the user name has no short form, so it never showed.
- **Do:** compare canonical paths (`fsio::unrecycle`); `fileops_test` reproduces the same mismatch with a
  root added as `...\x\..\x` (seen red on the old compare). Any path compare against what Windows records
  needs `canonicalize` on both sides.

## 2026-11-23 — phase 1.4

### Debug `ssctl` is far too slow for a real-library index
- **Seen:** the debug `ssctl index` made ~1.3 thumbnails/s on real 1080p+ PNGs (5,661 files would take ~70 min).
- **Why:** the dev profile leaves `image`, `png`/`fdeflate` and `fast_image_resize` unoptimized. Decoding is
  the hot path, and unoptimized decode runs 10-30x slower.
- **Do:** use `cargo build --release -p ss-core --bin ssctl` for a large index. Stopping an index part-way is
  safe: pending thumbnails stay `pending` and the next `ssctl index` resumes them.

### The index can die from system commit exhaustion, not from a bug
- **Seen:** `memory allocation of 4926244 bytes failed`, exit 0xC0000409, at thumbnail 919 of 5,661.
- **Why:** the machine had 0.5 GB of its 47.7 GB commit limit free while a release `rustc` LTO build ran
  next to the index (RAM was not full, the page-file-backed commit limit was).
- **Do:** do not run a release build and a big index at the same time; check free commit with
  `(Get-CimInstance Win32_OperatingSystem).FreeVirtualMemory` before a long run.

### Every thumbnail answered 400 in the real app: `convertFileSrc` percent-encodes `/`
- **Seen:** release app on a 50-image fixture: the grid showed placeholders, the console showed one
  `400 (Bad Request)` per thumbnail.
- **Why:** `src/lib/ssimg.ts` builds URLs with `convertFileSrc('thumb/<id>/<w>', 'ssimg')`, which applies
  `encodeURIComponent` to the path, so `ssimg` received `/thumb%2F12%2F320`, and `ssimg::parse` split on a
  literal `/`. Each lane was green alone: 1.2 tests called `ssimg("/thumb/12/320")`, 1.3 tests mocked the URLs.
- **Do:** `ssimg::parse` percent-decodes first; `index_test` sends the encoded form (seen red without the fix).
  When a test stands in for the other side of a boundary, feed it the exact bytes the other side produces.

### sccache breaks the release app build: command line too long (os error 206)
- **Seen:** `could not compile screenshot-studio (lib)`; cargo's summary only showed the rustc command. Run cargo
  directly to see the cause: `sccache: caused by: The filename or extension is too long. (os error 206)`.
- **Why:** cargo 1.100 passes one `-L dependency=target\release\build\<crate>\<hash>\out` per crate (370 here),
  so the app crate's rustc command line is ~41,000 chars, above the Windows limit of 32,767. Cargo puts a long
  command line in an `@argfile` when it calls rustc itself, but sccache 0.19 spawns rustc with the full line.
- **Do:** build the app with `RUSTC_WRAPPER=""` (empty overrides `build.rustc-wrapper`). Only the app crate
  rebuilt (fingerprints do not include the wrapper). `scripts/accept/lib/run.mjs` does this. Two earlier failures
  of this crate, blamed on memory, may have been this.

### `cargo build -p a --bin x -p b` silently skips b's binary
- **Seen:** the app showed `not implemented (get_app_info lands in phase 1.2)`: a phase-1.1 exe, although cargo
  printed `Finished` and recompiled `ss-core`.
- **Why:** a `--bin` filter applies to every `-p` package, so only `ssctl` was linked and the old
  `screenshot-studio.exe` stayed on disk.
- **Do:** use `--bins`, or one cargo call per package (`scripts/accept/lib/run.mjs` does the latter). Check an
  exe's timestamp when a fix "does not show up".

### Measure speed on the release build with the UI embedded
- The debug exe needed 8.9 s to expose CDP, and debug thumbnailing was ~70x slower than release, so debug
  numbers say nothing about the product. `p1.mjs` builds `vite build` + `cargo build --release -p
  screenshot-studio --features tauri/custom-protocol` (UI embedded, page at `http://tauri.localhost`).
- A cold release build took 24 min 51 s with `-j 4`. The default job count ran out of commit memory here (see
  the commit-exhaustion entry above); set `CARGO_BUILD_JOBS=4` before `node scripts/accept/p1.mjs` on this machine.

## 2026-11-23 — phase 1.1

### Vitest finds no suite when the cwd drive letter is lowercase
- **Seen:** every test file, even `it("x", ...)` alone, failed with "Vitest failed to find the current suite",
  `Tests: no tests`. The same file passed from a temp dir.
- **Why:** VS Code starts terminals in `d:\Screenshot_Picture` (lowercase drive). Vitest then loads its runner
  under two path spellings (`d:/` and `D:/`), and `it()` registers on the wrong instance.
- **Do:** run vitest from a cwd with an upper-case drive (`cd /d D:\Screenshot_Picture`).
  `scripts/accept/lane-tests.mjs` already normalizes the drive letter for every runner it spawns, so always go
  through it for Accept runs.

### Tool shells mangle backslashes in inline scripts and heredocs
- **Seen:** `\\` written in a bash heredoc landed as `\` in JSON fixtures (invalid escape); an inline
  `node -e` with a `\\` regex failed to parse.
- **Do:** write anything containing backslashes (Windows paths in JSON, regexes) with the editor's Write tool or
  a script file, not inline shell strings. `contract_test.rs` (b) catches a broken fixture.

### Contract-test gotchas (ts-rs 12, serde, Git for Windows)
- `#[ts(type = "...")]` replaces the whole field type, so `Option<i64>` needs `"number | null"` and `Vec<i64>`
  `"number[]"`; the ts-rs default for i64 is `bigint`. `contract_test` (a) fails on any `bigint`.
- ts-rs does not parse `#[serde(transparent)]` (warning). A serde newtype struct already serializes as its
  inner value, so leave the attribute off.
- Git for Windows ships `core.autocrlf=true`; `.gitattributes` pins `eol=lf` so the byte-compare of committed
  bindings does not flip on checkout.
- `contract_test` sub-tests run in parallel: under `SS_WRITE_BINDINGS=1`, (a) rewrites `bindings/`, so no
  other sub-test may read the committed dir. They export to their own temp dir.
- Fixture floats: serde_json treats `12` and `12.0` as different `Value`s, so a round-trip of an `f64` field
  needs `12.0` in the fixture (as Python's `json` writes it).

### First full build is slow
- The first `cargo test -p ss-core` compiled ~330 crates in 10.5 min (image, printpdf, zip, rusqlite bundled).
  sccache (`D:\DevCache\cache\sccache`) makes later clean builds much faster; incremental test runs take seconds.
