# Lessons (project-local)

Non-obvious gotchas found while building Screenshot Studio. Owner rule: lessons stay in this repo, nothing goes
to a kit. Newest first. One entry: what happened, why, what to do.

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
