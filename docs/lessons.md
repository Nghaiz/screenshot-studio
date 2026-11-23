# Lessons (project-local)

Non-obvious gotchas found while building Screenshot Studio. Owner rule: lessons stay in this repo, nothing goes
to a kit. Newest first. One entry: what happened, why, what to do.

## 2026-11-24 — phase 3.1

### A stdin pipe read blocks DLL loading in other threads (Windows)
- **Seen:** the real sidecar answered `initialize` only after the NEXT line arrived on stdin. `faulthandler` showed the
  worker thread stuck inside `import numpy` (`_multiarray_umath`, a native DLL load, reached from `import torch`) while
  the main thread sat in `sys.stdin.buffer.readline()`. Fake engines never showed it; plain `os.fstat(0)`,
  `isatty(0)` and a child process inheriting stdin did not stall in isolation.
- **Why (likely):** Windows serializes synchronous I/O per file object, and something in that DLL's initialization
  touches the standard input handle while the pending `ReadFile` holds it.
- **Do:** a stdio server that loads native libraries in worker threads reads its protocol from a private duplicate of
  fd 0 and gives everyone else NUL (`ss_sidecar/__main__.py` `claim_stdio`). The real-model tests catch a regression:
  they hung (120 s timeout) before the fix.

### RapidOCR's PP-OCRv6 model cannot output most Vietnamese tone letters
- **Seen:** `điều kiện` read as `điu kin`; `Xin chào thế giới` as `Xin chào th gii`. The rec model's dictionary (onnx
  metadata `character`, 18,708 entries) lacks 44 precomposed letters (`ả ạ ằ ẳ ẵ ắ ặ ầ ẩ ẫ ấ ậ ẻ ẽ ẹ ề ể ễ ế ệ ỉ ị ỏ ọ ổ ỗ ố ộ
  ờ ở ỡ ớ ợ ủ ụ ừ ử ữ ứ ự ỳ ỷ ỹ ỵ`); `ă â đ ê ô ơ ư` and the Latin-1 accents are there. The decoder drops what it
  cannot name, so the error is silent.
- **Do:** pick test text from letters the model has (`test_ocr.py`), and keep `test_vietnamese_tone_letters_known_gap`
  until a model that has them is wired in (then invert it). `latin_PP-OCRv5` is worse (no `đ ư ơ`).

### open_clip's SigLIP i18n tokenizer needs `transformers`
- `ViT-B-16-SigLIP-i18n-256` sets `hf_tokenizer_name`, so `open_clip.get_tokenizer` calls
  `transformers.AutoTokenizer`; `open_clip_torch` does not depend on it. Added to `sidecar/pyproject.toml`.

### The uv venv's `python.exe` is a launcher: one Python = two processes
- `D:\...\sidecar-venv\Scripts\python.exe` starts the real interpreter as a child. Killing a hung sidecar or test run
  means stopping both PIDs (`Get-CimInstance Win32_Process` shows the pair with the same command line).

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

### Second pass: Chromium purges on idle, Blink keeps every encoded thumbnail, and WebView2's LOW target barely helps
- **Idle purge (measured, no flags):** after visiting every thumbnail the tree falls from 751 to 596 MB within ~8 s
  of no input, by itself. The first pass sampled right after the last screen, so it saw the peak only. `p2.mjs` now
  samples for 10 s after each scroll and prints the peak and the after-idle value.
- **What stays (dump after idle, after 2 forced GCs, after a simulated critical pressure signal; unchanged by all
  three):** renderer `partition_alloc` allocated objects 214 MB. 5,662 grid thumbnails on disk average 33.6 KB
  (186 MB in total), so Blink keeps the ENCODED bytes of every thumbnail it has shown. Only 27 `<img>` elements were
  alive, all in the DOM (CDP `Runtime.queryObjects`), so neither React nor detached nodes hold them. `cc/image_memory`
  read 456 MB through all of it while private bytes fell, so most of what it reports is not process private memory.
- **`ICoreWebView2_19::SetMemoryUsageTargetLevel` (LOW), tried (webview2-com 0.39.1 + windows-core 0.62.2, the
  versions Tauri links; `with_webview` -> `controller().CoreWebView2()?.cast::<ICoreWebView2_19>()`):** LOW held from
  start: first screen 323-333 MB vs 376-381 MB, visit-all + idle 555 MB vs 596 MB; a 3 s LOW pulse during idle: the
  same 555 MB. The ~40-50 MB comes off the GPU process only; the renderer's 214 MB stays. Fast scroll and its p95
  (4.3 ms) did not change. Microsoft advises LOW for an inactive app, the only way it paid here was holding it on the
  visible window, and it never got visit-all near 500 MB, so it was reverted (no new direct dependency).
- **Do:** the remaining levers are the encoded size (#29 estimated 7.3 KB per 320 px thumbnail from 60 samples;
  the real library averages 33.6 KB, 4.6x more) or loading thumbnails some way Blink does not retain. Both are
  later-phase decisions; `p2.mjs` gates the plan's literal whole-library scroll (Deferred decision #31).

### Square grid cells cropped and upscaled every thumbnail (`object-fit: cover`)
- **Seen (real library, CDP, devicePixelRatio 1.5 on this laptop):** Grid at slider 320: cells 322 px square, median
  on-screen scale 5.0 physical px per thumbnail px (wide screenshots: their middle cropped and blown up); at 400:
  8.9. Masonry (box = image aspect): 1.5 at 320, which is the DPR alone. List: 0.6.
- **Do:** square cells (Grid, Timeline) use `contain` over the cell surface (`.ss-square`, `src/index.css`). With it
  the Grid's median is 0.89x at the owner's saved size 180, 1.51x at 320 (the DPR alone), 2.29x at 400. Sharper
  than that at 320-400 needs a bigger thumbnail; the 1280 preview (204 KB encoded) in the grid would multiply the
  retained encoded bytes above ~6x, so it was not used (Deferred decision #32).
- **It costs memory:** A/B on one build, the old rule injected over CDP (`addStyleTag`), 2 runs each: gated scroll
  peak 500/500 MB with `contain` vs 476/482 MB with `cover`; visit-all peak 798/795 vs 741/741 MB. Measure a CSS
  change to image display against the memory gate before shipping it; an A/B over CDP needs no rebuild.

### Windows Search finds the installed app (step 6, checked by the orchestrator with cua-driver, 2026-11-24 ~02:40)
- Win+S, typed `Screenshot Studio`: "Best match: Screenshot Studio, App" with the new icon (top result, web
  suggestions below). Enter launched it; `list_windows` showed `screenshot-studio.exe` with the window
  "Screenshot Studio" and the single-instance helper window `dev.nghaiz.screenshotstudio-siw`; `verify_state`
  (window exists) satisfied; closed with the UIA Close button. `p2.mjs` `start-menu` stays the automated proxy.

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
