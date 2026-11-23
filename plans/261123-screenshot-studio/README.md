# Plan: Screenshot Studio (Tauri 2 + React/TS + Python sidecar)

Design (approved, not re-opened here): `plans/reports/brainstorm-261123-screenshot-studio.md`
Hardness: phases=15 new=7 modules=0 lanes=3
Flags: --auto
PRs: 0

Personal project. Remote: **public** GitHub repo `Nghaiz/screenshot-studio` (SSH), added 2026-11-23 at the owner's request after phase 1.1; every phase lands through its own PR into `main` (squash-merge after GitHub Actions CI `.github/workflows/ci.yml` is green). No Plane, no other cloud. Public repo: never commit secrets, `.env`, screenshots or files from the OneDrive folder.
Plane gate: opt-out (personal repo; the owner forbids Plane). Recorded as Deferred decision #2.

## Forecast

Cap: 70% max while the three key assumptions below are UNMEASURED (Vietnamese OCR quality, CUDA wheels on this driver, 60 fps grid in WebView2 at 5,600+ items).

| Outcome (measurable) | Confidence | Basis |
|---|---|---|
| Phase 0: `cargo`, `rustc`, MSVC `link.exe`, `sccache`, `cargo tauri` all resolve under D:, verifier exits 0 | 65% | ESTIMATED: winget id `Microsoft.VisualStudio.BuildTools` 18.12.1 MEASURED (`winget search`, 2026-11-23); rustup-init URL 200 MEASURED; risk is the SSMS-fixed shared path and UAC |
| Indexed count == image files on disk for the real library (5,683 PNG MEASURED 2026-11-23 by metadata listing; the brainstorm's 5,604 is 79 lower; 22 files cloud-only) | 65% | ESTIMATED: metadata scan needs no file content; cloud-only files counted from the directory listing |
| Grid sustains >= 55 fps p50 while scrolling 5,600+ thumbnails | 55% | ESTIMATED: TanStack Virtual keeps the DOM ~100 nodes; WebP thumbs served by a custom scheme; not measured on this machine |
| "Screenshot Studio" found in Windows Search and launches | 70% | DOCUMENTED: NSIS bundler writes a Start Menu shortcut (`bundle.windows.nsis.startMenuFolder`, Tauri config schema 2.12.7) |
| OCR search finds known Vietnamese and English text in fixture images | 45% | UNKNOWN for Vietnamese diacritics with RapidOCR default models; spike in phase 3.1 step 1 |
| Semantic search on CUDA | 50% | ESTIMATED: `torch-2.15.0+cu132-cp313-win_amd64` exists (fact-fetcher, download.pytorch.org); driver 610.88 MEASURED (`nvidia-smi`); not run yet |
| App fully usable with the sidecar absent (AI features disabled + notice) | 70% | ESTIMATED: sidecar is a separate process behind one Rust supervisor; tested by phase 3.2 with no venv |

Success rate: phase 0 65%, P1 (1.1-1.4) 55%, P2 (2.1-2.3) 65%, P3 (3.1-3.4) 45%, P4 (4.1-4.3) 60%. Overall to "P2 usable app" 65% x 55% x 65% = ~23% first pass; to full P4 23% x 45% x 60% = ~6% first pass, ESTIMATED by multiplying chained phases (first-pass means every Accept green with no rework round). Overall <= weakest critical dependency (P3 OCR quality, 45%). Rework, not abandonment, is the expected failure mode.
Effect (low/likely/high): wall time P0 1 h / 2 h / 4 h (VS Build Tools download ~3-6 GB, `cargo install` compiles); P1-P4 implementation 3 / 5 / 9 working days of agent time, ESTIMATED from 15 phases of S-L size. First full index of the real library 5 / 15 / 60 min (60 if OneDrive must hydrate 1.7 GB), ESTIMATED. First OCR+CLIP pass 10 / 20 / 60 min (60 on CPU fallback), ESTIMATED (brainstorm figure is 10-20 min on RTX 4060). May be slower: CPU fallback for AI if CUDA wheels fail; OneDrive hydration.
Uncertainties (by impact):
1. Vietnamese OCR accuracy with RapidOCR -> 20-image spike (phase 3.1 step 1), ~30 min, before the rest of phase 3.1.
2. Grid fps in WebView2 -> phase 1.3 ships a synthetic 10,000-item mock grid and phase 1.4 measures it via CDP; ~1 h, before phase 2.2.
3. CUDA torch/onnxruntime on this driver -> `python -c` import probe inside phase 3.1 step 2, ~15 min (download dominates).
4. VS Build Tools honouring `--installPath` on D: with the shared path fixed on C: -> phase 0 dry-run prints the exact command; the verifier checks `link.exe` path.
Kill / no-regression: kill P3 GPU path (keep CPU) if CUDA import fails twice; kill Vietnamese OCR via RapidOCR and switch to the PaddleOCR fallback named in the design if the spike's character accuracy is < 85%; stop P1 at 1.4 if fps p50 < 45 and fix virtualization before P2. Rollback: every phase is a local git branch merged by an integration phase; revert = `git revert` of the merge commit; phase 0 rollback = uninstall listed in `phase-0.md` § Rollback.
Re-score (phase 1.4, 2026-11-23, `node scripts/accept/p1.mjs` 8/8 PASS, release build with the UI embedded, real library):
- Indexed count == files on disk: MEASURED PASS, 6,260 = 6,260. The library grew from 5,683 when the OneDrive client
  restarted and synced 576 more placeholders (old screenshots), then one new screenshot; 598 files are cloud-only.
  The count is only right after an index: no watcher until 4.1.
- Grid fps: MEASURED PASS, p50 4.20 ms (238 fps, high-refresh display), p95 4.40 ms over a 10 s top-to-bottom scroll of
  6,260 items (252,769 px). The 55% estimate is resolved; the "stop P1 if p50 < 45 fps" kill criterion is far off.
- Cold start to first grid thumbnail: MEASURED PASS, 738 ms (design metric < 2 s; LCP timing of the first thumbnail).
- Footprint (measured, budgets per Deferred decisions #29 and #31): app exe 11.7 MB, ssctl 5.6 MB; catalog 2.8 MB;
  thumbnails 1,315.6 MB (lossless, 2 sizes; target ~41 MB from 2.1); app + WebView2 private memory 360 MB at first
  screen, 1,208 MB after a full scroll (budget <= 500 MB, gated in 2.3).
- Index speed: release `ssctl` generated 5,161 thumbnails in 73 s (the debug build: ~1.3/s).
- New risk found: the app's release build needs `RUSTC_WRAPPER=""` (sccache + 41,000-char rustc command line, os
  error 206, docs/lessons.md). Phase 1.4 P1 confidence: done (Accept green); P2 confidence unchanged at 65%.
Re-score (phase 2.3, 2026-11-24, `node scripts/accept/p2.mjs` 11/11 PASS in 2 runs, installed release build, real library):
- "Screenshot Studio" found in Windows Search and launches: MEASURED PASS. The orchestrator checked it with cua-driver:
  "Best match: Screenshot Studio, App" with the new icon, Enter launched it, closed with the UIA Close button. The 70%
  estimate is resolved; `p2.mjs` `start-menu` is the automated proxy.
- Memory (#31): gated whole-library scroll peak 496-500 MB in 4 runs against the 500 MB budget, so there is no
  headroom: UI added in 3.3 or 4.2 is likely to tip it. Visiting every thumbnail: 790-797 MB peak, not gated
  (orchestrator decision, reversible).
- Grid fps unchanged: p50 4.20 ms, p95 4.30 ms. Installer: currentUser NSIS into D:, Start Menu shortcut only,
  single instance, WebView2 profile under the data dir.
- New risk: the real 320 px thumbnails average 33.6 KB, 4.6x #29's 7.3 KB estimate, and that size is what the
  renderer keeps. P2 confidence: done (Accept green); P3 confidence unchanged at 45%.
Not checked: ts-rs 12 `#[ts(type)]` behaviour on Option/Vec fields (from library knowledge; contract_test bans `bigint` to catch it); Tauri raw-body IPC + headers API; `tauri-plugin-drag` version; Rust/Tauri build on this machine (Rust not installed yet); WebView2 CDP attach in a dev build; RapidOCR Vietnamese; OneDrive Files On-Demand state of the 5,604 files (how many are cloud-only); SigLIP i18n quality on Vietnamese queries; NSIS `/D=` with currentUser mode on D:.

## Finish ETA

`bun ~/.claude/scripts/t1k-spawn-eta.cjs plan-eta plans/261123-screenshot-studio --auto --record` (2026-11-23):
`done by 2026-11-23 16:12 UTC+07:00 [parallel; p80-chain; 6 gates x 10m; agent+gates only] n=143 basis=global validated=unvalidated (0 of 10 plans measured; backtest of 10 past plans: took 1.78x longer than this figure, hit 2/10)`
Schedules: sequential 8h19m, parallel 5h52m (3 lanes, recommended). Every phase is priced at the same global p80 (29m16s), so this
figure ignores phase size (L phases here are multi-hour); the Forecast effect line (3/5/9 working days) is the planner's estimate.
Delivery is reported `unknown` because `PRs: 0` (no remote by owner rule).

## Phases

| # | Phase | Lane | Effort | Gate | File |
|---|-------|------|--------|------|------|
| 0 | Machine-wide Rust + VS Build Tools + cargo tools + VS Code ext on D: | serial | M | human, BLOCKED | phase-0.md |
| 1.1 | git init, workspace scaffold, dependencies, contracts transcribed + contract test | serial | M | - | phase-1.1-scaffold-contracts.md |
| 1.2 | Core: scan, hash, thumbnails, SQLite catalog, `ssimg` scheme, `ssctl` | core | L | - | phase-1.2-core-index.md |
| 1.3 | UI: shell, theme engine (10+ themes), virtual grid/layouts, lightbox, timeline, palette, i18n | ui | L | - | phase-1.3-ui-browse.md |
| 1.4 | P1 integration: merge lanes, index real library, 60 fps measurement | serial | M | human | phase-1.4-p1-integration.md |
| 2.1 | Core: full query + FTS, tags/albums/favourites/notes/smart filters, file ops + undo, Recycle Bin, permanent delete | core | L | - | phase-2.1-core-organize.md |
| 2.2 | UI: search/filter panel, saved filters, organize, selection, bulk rename, trash view, delete dialogs | ui | L | - | phase-2.2-ui-organize.md |
| 2.3 | P2 integration: single-instance, NSIS installer to D:, Start Menu, Windows Search | serial | M | human | phase-2.3-installer.md |
| 3.1 | Python sidecar: JSON-RPC server, OCR, SigLIP embeddings, zero-shot, colour, PII | ai | L | human | phase-3.1-sidecar.md |
| 3.2 | Core: sidecar supervisor + AI jobs, semantic search, duplicates, sessions, blur/tiny, stats | core | L | - | phase-3.2-core-smart.md |
| 3.3 | UI: AI notices/degraded mode, duplicate finder, cleanup wizard, dashboard, smart filters | ui | M | - | phase-3.3-ui-smart.md |
| 3.4 | P3 integration: OCR hit + planted duplicate + sidecar-absent check | serial | M | - | phase-3.4-p3-integration.md |
| 4.1 | Core: watcher + toast, tray + recent, hotkey, autostart, export ZIP/PDF, edit save/convert/stitch, clipboard | core | L | - | phase-4.1-core-live.md |
| 4.2 | UI: editor (crop/rotate/annotate/blur/redact), share/export UI, tray popup, settings, motion polish | ui | L | - | phase-4.2-ui-editor.md |
| 4.3 | Final integration: rebuild + reinstall, live screenshot check, edit round-trip, success metrics | serial | M | human | phase-4.3-release.md |

Brainstorm phase mapping: P0 = 0, P1 = 1.1-1.4, P2 = 2.1-2.3, P3 = 3.1-3.4, P4 = 4.1-4.3. App usable from 2.3.

## Human steps (do first)

| Step | Unblocks | Where / exact command | Why an agent cannot | Timing |
|---|---|---|---|---|
| Approve setting user env vars `RUSTUP_HOME=D:\DevCache\tools\rust\rustup`, `CARGO_HOME=D:\DevCache\tools\rust\cargo`, `SCCACHE_DIR=D:\DevCache\cache\sccache`, `HF_HOME=D:\DevCache\huggingface`, and rustup appending `%CARGO_HOME%\bin` to user PATH | phase 0 | review `pwsh -NoProfile -File scripts\setup-rust-machine.ps1` (dry-run, prints every change), then `pwsh -NoProfile -File scripts\setup-rust-machine.ps1 -Apply` | machine config change; owner reserves it | now |
| Approve the HKLM registry value `HKLM\SOFTWARE\Policies\Microsoft\VisualStudio\Setup` `CachePath` = `D:\DevCache\cache\vs-packages` (REG_SZ) | phase 0 | written by the `-Apply` run in an elevated child process (one UAC prompt) | HKLM write needs admin + owner consent | now |
| Click the UAC prompt(s) for the elevated step (CachePath + VS Build Tools install to `D:\IOT_Tools\apps\VSBuildTools`) | phase 0 | appears during the `-Apply` run | UAC consent cannot be automated | now |
| Approve the software installs of phase 0: rustup + stable MSVC toolchain, VS Build Tools 2026 (VCTools workload, part of Windows SDK lands on C: because SSMS fixed the shared path), `cargo install --locked sccache tauri-cli`, VS Code extensions | phase 0 | same `-Apply` run | install consent | now |
| Approve the AI downloads: `torch` cu132 wheel (~3 GB) + onnxruntime + `rapidocr` into `D:\IOT_Tools\data\ScreenshotStudio\sidecar-venv`, models (~1-2 GB) into `D:\DevCache\huggingface` | phase 3.1 | `uv sync --project sidecar` + `python -m ss_sidecar --prefetch` (exact lines in phase-3.1.md) | download/install consent | now |
| Restart terminals, VS Code and the Claude session so the new user env vars are inherited | phase 1.1 | close and reopen | a running process never sees new user env vars | later: after phase 0 `-Apply` finishes |
| Approve the first index of the real library `C:\Users\Nghaiz\OneDrive\Pictures\Screenshots` (read-only), and decide whether to press "Download cloud-only files" (may hydrate up to 1.7 GB onto C:; never pins/unpins) | phase 1.4 | in-app Settings > Library > Add folder; optional button "Download cloud-only files (N MB)" | reading the owner's OneDrive library + C: disk use is the owner's call | later: needs the app built in 1.2/1.3 |
| Approve installing Screenshot Studio (NSIS, currentUser, no UAC) into `D:\IOT_Tools\apps\ScreenshotStudio` with a Start Menu shortcut | phase 2.3 | `src-tauri\target\release\bundle\nsis\Screenshot Studio_0.1.0_x64-setup.exe /S /D=D:\IOT_Tools\apps\ScreenshotStudio` | installs software on the machine | later: the installer is built in 2.3 |
| Approve re-installing the final build (same command, new version) | phase 4.3 | same as above with the 4.3 installer | installs software on the machine | later: built in 4.3 |

## Planning waves

Group A: run
Group B: run
Review: 1 reviewer
Fan-out: 3 lanes
Facts: lanes=3 new-rows=7

Group A ran `t1k-corpus-scout` and `t1k-fact-fetcher` in one message; `t1k-investigator` was not needed (no existing code). The corpus scout reported the knowledge-retrieval MCP absent in its hop, so corpus scope is `unverified`; the local grep sweep found zero hits (scope in the ledger).
Review: the single fresh-context reviewer was NOT spawned: the owner's global rule forbids review agents unless asked in the conversation (`no-unrequested-review`). Group B ran as validation lanes (classifier, data verifier, claim check), Group C ran the validators.

## Verification topology

| Lane | Verifier | Concurrent? |
|------|----------|-------------|
| core | `node scripts/accept/lane-tests.mjs <phase's crates/ss-core/tests/*_test.rs>` (runs `cargo test -p ss-core --test <name>` per file) | yes, own git worktree and own `target/` |
| ui | `node scripts/accept/lane-tests.mjs <phase's src/__tests__/*.test.ts(x)>` (runs `npx vitest run <files> --reporter=json`; jsdom + `@tauri-apps/api/mocks`, no Rust needed) | yes, own git worktree |
| ai | `node scripts/accept/lane-tests.mjs <phase's sidecar/tests/test_*.py>` (runs `<sidecar-venv>\Scripts\python.exe -m pytest <files> -rA`) | yes, own git worktree |

Lane sizing: tier=opus UNMEASURED; lanes=3; p=0.8; P(all land)=0.512 (information, not a cap)

Lane chains are sequential inside a lane: core 1.2 > 2.1 > 3.2 > 4.1; ui 1.3 > 2.2 > 3.3 > 4.2; ai 3.1 (starts right after 1.1, runs beside P1/P2). Integration phases (1.4, 2.3, 3.4, 4.3) are serial and merge the lanes.

## Contract

Owner: plan author (the integration phase lead); lanes never edit contract files, a change request stops the lane and goes to the Owner, who updates the contract and re-briefs every lane
File: contracts.md
Test: crates/ss-core/tests/contract_test.rs

Linked contract files (pinned BEFORE any fan-out, transcribed into code by phase 1.1 only):
- `contract-tauri-ipc.md` — Rust <-> UI commands, events, error envelope, `ssimg` URLs, ts-rs rules
- `contract-sidecar-jsonrpc.md` — Rust <-> Python JSON-RPC 2.0 over stdio, methods, errors, lifecycle, degradation
- `contract-sqlite-schema.md` — SQLite v1 schema (all tables for P1-P4), migrations rule
- `design-intent.md` — shared UI/UX + naming + paths decisions every lane follows

## Decision register

| # | Decision | Taken |
|---|---|---|
| D1 | Repo layout | Root virtual Cargo workspace (`Cargo.toml`, members `crates/ss-core`, `src-tauri`); UI in `src/`; sidecar in `sidecar/`; contracts transcribed to `crates/ss-core/src/contract/`, `contracts/`; TS bindings generated to `crates/ss-core/bindings/` (alias `@bindings`) |
| D2 | Lane ownership | core: `crates/**`, `src-tauri/**`; ui: `src/**`; ai: `sidecar/**`. Frozen after 1.1, edited only by the serial integration phases (1.4, 2.3, 3.4, 4.3): `package.json`, `package-lock.json`, `Cargo.lock`, every `Cargo.toml`, `sidecar/pyproject.toml`, `uv.lock`, `vite.config.ts`, `vitest.config.ts`, `tsconfig.json`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/**`, `contracts/**`, `crates/ss-core/src/contract/**`, `scripts/**`. `crates/ss-core/bindings/**` is rewritten only by `SS_WRITE_BINDINGS=1` runs of `contract_test`, done by the Owner when a contract changes; lanes never regenerate it |
| D3 | Dependencies | Phase 1.1 adds EVERY crate, npm package and Tauri plugin the whole plan needs (ledger rows 1-30). A lane that needs a new dependency stops and reports; it never edits a manifest |
| D4 | IPC shape | One `req` struct argument per command, camelCase JSON, `Result<T, AppError>`, i64 exported as TS `number` (`#[ts(type = "number")]`), enums as camelCase string unions; full list in `contract-tauri-ipc.md` |
| D5 | Images to the webview | One async custom URI scheme `ssimg` (`http://ssimg.localhost/thumb/<id>/<w>`, `/full/<id>`); no asset-protocol scope |
| D6 | Sidecar process | Rust spawns `<sidecar-venv>\Scripts\python.exe -m ss_sidecar` with `tokio::process` (not Tauri `externalBin`); absent venv = state `absent`, app keeps working |
| D7 | Delivery | Each lane commits on its own local branch in its own `git worktree`; integration phases merge with `git merge --no-ff`; no PR. Amended 2026-11-23 (owner, twice): each phase runs on branch `phase-<id>` from an up-to-date `main`, is pushed, opened as a PR into `main` on the public GitHub repo, squash-merged once CI is green, then `main` is pulled locally (Deferred decision #28) |
| D8 | Destructive ops | In-app trash (file untouched) is the default "Delete"; Recycle Bin is an explicit action (undoable); permanent delete is a two-step token + typed `DELETE` flow, never undoable, never default, never bound to a single key |
| D9 | UI state | Theme, density, motion, language, layout in `localStorage` (owner rule); library roots in the `library_root` table; hotkey, watcher, autostart and the other backend settings in the `setting` table |
| D10 | Test data | Tests and Accept scripts use generated fixture libraries under `%TEMP%` with their own `SS_DATA_DIR`; the real catalog and the OneDrive folder are only ever READ by Accept scripts (counts); `ssctl undo` takes an explicit batch id; Accept scripts that launch the app first FAIL if a Screenshot Studio process is already running (single-instance would hijack the run) |
| D11 | Lane Accept runner | Every lane phase's Accept is `node scripts/accept/lane-tests.mjs <its own test files>` (built in 1.1): it FAILS when a named test file is missing, when a runner reports 0 tests, or when any test is skipped, ignored or deselected, so a workspace-wide green from older tests can never pass a phase |

Straggler: integrate lanes that landed by about 2x the median lane time; a late lane is re-briefed on its remaining diff in a fresh worktree; the integration phase never waits on it past that, it merges what landed and the late lane joins the next integration phase.

## Reuse Ledger

Corpus sweep: `t1k-corpus-scout` reported the knowledge-retrieval MCP not present in its hop; the planner session has no knowledge-retrieval tool either. Corpus scope is therefore `unverified`. Local grep sweep (2026-11-23): `d:\Screenshot_Picture` (only `plans/`), `D:\IOT_Tools` and `D:\DevCache` to depth 3 for `*rust*`, `*setup*.ps1`, `*tauri*`, `*screenshot*`, `sccache*`, `VSBuildTools`: zero hits. Library versions: fact-fetcher, crates.io / npm / PyPI registry APIs, retrieved 2026-11-23.

| # | Capability | Verdict | Evidence |
|---|---|---|---|
| 1 | Desktop shell, commands, events, channels, tray (core feature `tray-icon`) | REUSE | crate `tauri` 2.12.7, npm `@tauri-apps/api` 2.12.5 (repo: tauri-apps/tauri) |
| 2 | File system access from UI (dialog-picked paths) | REUSE | `tauri-plugin-fs` / `@tauri-apps/plugin-fs` 2.7.1 |
| 3 | Folder/file pickers, confirm dialogs | REUSE | `tauri-plugin-dialog` / `@tauri-apps/plugin-dialog` 2.8.4 |
| 4 | Copy image / OCR text | REUSE | `tauri-plugin-clipboard-manager` 2.5.0 (permissions `clipboard-manager:allow-write-image`, `allow-write-text`) |
| 5 | Global hotkey Ctrl+Alt+S | REUSE | `tauri-plugin-global-shortcut` 2.4.1 |
| 6 | Single instance | REUSE | `tauri-plugin-single-instance` 2.5.5 (Rust only; no npm package found) |
| 7 | Autostart (default off) | REUSE | `tauri-plugin-autostart` 2.7.0 |
| 8 | Toast on new screenshot | REUSE | `tauri-plugin-notification` 2.8.1 |
| 9 | Reveal in Explorer, open-with | REUSE | `tauri-plugin-opener` 2.7.3 (`revealItemInDir`, `openPath`) |
| 10 | SQLite + FTS5 (bundled) | REUSE | crate `rusqlite` 0.43.0 feature `bundled` |
| 11 | Live folder watcher | REUSE | crates `notify` 9.0.0 + `notify-debouncer-full` 0.8.0 |
| 12 | Decode/encode PNG/JPEG/WebP, crop/rotate | REUSE | crate `image` 0.25.10 |
| 13 | Fast thumbnail resize | REUSE | crate `fast_image_resize` 6.2.0 |
| 14 | Perceptual hash (dHash/pHash) | REUSE | crate `image_hasher` 3.1.1 |
| 15 | Recycle Bin delete + restore | REUSE | crate `trash` 5.4.0 (`delete_all`, `os_limited::restore_all`, Windows supported) |
| 16 | Rust -> TS type generation | REUSE | crate `ts-rs` 12.0.1 (`#[derive(TS)] #[ts(export)]`, exported on `cargo test`) |
| 17 | Directory walk, parallelism, exact hash | REUSE | crates `walkdir`, `rayon`, `blake3`, `serde`, `serde_json`, `tokio`, `thiserror` (versions: cook resolves latest via `cargo add`) |
| 18 | ZIP / PDF export | REUSE | crates `zip`, `printpdf` (versions UNCONFIRMED; cook resolves via `cargo add` + context7) |
| 19 | Build cache | REUSE | `sccache` 0.19.0 via `build.rustc-wrapper` |
| 20 | Virtualized grid (10k+) | REUSE | npm `@tanstack/react-virtual` 3.14.28 |
| 21 | Animation | REUSE | npm `motion` 14.14.0, imported from `motion/react` ("Framer Motion is now Motion") |
| 22 | UI framework + build + styling | REUSE | npm `react`, `react-dom`, `typescript`, `vite`, `tailwindcss` + `@tailwindcss/vite`, `@tanstack/react-query`, `zustand` (cook resolves latest) |
| 23 | UI tests | REUSE | npm `vitest`, `jsdom`, `@testing-library/react`; IPC mocked with `@tauri-apps/api/mocks` `mockIPC` |
| 24 | Grid fps + end-to-end probes | REUSE | npm `playwright` `chromium.connectOverCDP` against WebView2 started with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` |
| 25 | OCR (vi + en) | REUSE | PyPI `rapidocr` 3.10.0 + `onnxruntime` (CPU default; `onnxruntime-gpu` 1.32.0 cp313 win wheel exists); fallback PaddleOCR per design |
| 26 | Semantic embeddings / zero-shot | REUSE | PyPI `open_clip_torch` 4.2.2, model `ViT-B-16-SigLIP-i18n-256` (multilingual); `torch` 2.15.0+cu132 cp313 win wheel |
| 27 | Python env + cache on D: | REUSE | `uv` 0.13.8 with `UV_CACHE_DIR`/`UV_PYTHON_INSTALL_DIR` already on D: (MEASURED, brainstorm env scan) |
| 28 | Rust toolchain install | REUSE | `rustup-init.exe` from `https://win.rustup.rs/x86_64` (HTTP 200, 2026-11-23), honours `RUSTUP_HOME`/`CARGO_HOME` |
| 29 | MSVC linker + Windows SDK | REUSE | winget `Microsoft.VisualStudio.BuildTools` 18.12.1, workload `Microsoft.VisualStudio.Workload.VCTools` `--includeRecommended`, `--installPath` |
| 38 | Drag-out of originals to Explorer | REUSE | `tauri-plugin-drag` + npm `@crabnebula/tauri-plugin-drag` (versions UNCONFIRMED; cook confirms via context7 before 1.1 adds it) |
| 39 | Lossy WebP thumbnails (phase 2.1) | REUSE | crate `webp` 0.3.1 (libwebp via `libwebp-sys` ^0.9.3; `Encoder::from_rgb(..).encode(80.0)`; crates.io + docs.rs, 2026-11-23), `default-features = false`; added to the manifests by phase 1.4 (D3) |
| 30 | Installer + Start Menu entry | EXTEND | Tauri NSIS bundler (`bundle.windows.nsis`: `installMode`, `startMenuFolder`) plus the `/D=` install-dir switch (no config key for a default install dir) |
| 31 | Machine Rust-on-D: setup + verifier | NEW | `scripts/setup-rust-machine.ps1`, `scripts/verify-rust-machine.mjs` |
| | | | reuse-search: not-found-after-corpus-and-grep |
| | | | scope: plans/ (the whole repo; nothing else exists), D:/IOT_Tools and D:/DevCache to depth 3 by grep; corpus unverified (knowledge-retrieval MCP absent) | swept: 2026-11-23 |
| 32 | Catalog/indexer core library + `ssctl` CLI | NEW | `crates/ss-core/` |
| | | | reuse-search: not-found-after-corpus-and-grep |
| | | | scope: plans/ (the whole repo; nothing else exists), D:/IOT_Tools and D:/DevCache to depth 3 by grep; corpus unverified (knowledge-retrieval MCP absent) | swept: 2026-11-23 |
| 33 | IPC / JSON-RPC / schema contracts | NEW | `contract-tauri-ipc.md`, `contract-sidecar-jsonrpc.md`, `contract-sqlite-schema.md` |
| | | | reuse-search: not-found-after-corpus-and-grep |
| | | | scope: plans/ (the whole repo; nothing else exists), D:/IOT_Tools and D:/DevCache to depth 3 by grep; corpus unverified (knowledge-retrieval MCP absent) | swept: 2026-11-23 |
| 34 | React UI (browse, organize, smart, editor) + theme engine | NEW | `src/` |
| | | | reuse-search: not-found-after-corpus-and-grep |
| | | | scope: plans/ (the whole repo; nothing else exists), D:/IOT_Tools and D:/DevCache to depth 3 by grep; corpus unverified (knowledge-retrieval MCP absent) | swept: 2026-11-23 |
| 35 | Python JSON-RPC sidecar server | NEW | `sidecar/` |
| | | | reuse-search: not-found-after-corpus-and-grep |
| | | | scope: plans/ (the whole repo; nothing else exists), D:/IOT_Tools and D:/DevCache to depth 3 by grep; corpus unverified (knowledge-retrieval MCP absent) | swept: 2026-11-23 |
| 36 | Smart grouping, duplicate groups, cleanup rules | NEW | `crates/ss-core/src/smart/` |
| | | | reuse-search: not-found-after-corpus-and-grep |
| | | | scope: plans/ (the whole repo; nothing else exists), D:/IOT_Tools and D:/DevCache to depth 3 by grep; corpus unverified (knowledge-retrieval MCP absent) | swept: 2026-11-23 |
| 37 | Phase Accept scripts (end-to-end probes) | NEW | `scripts/accept/` |
| | | | reuse-search: not-found-after-corpus-and-grep |
| | | | scope: plans/ (the whole repo; nothing else exists), D:/IOT_Tools and D:/DevCache to depth 3 by grep; corpus unverified (knowledge-retrieval MCP absent) | swept: 2026-11-23 |

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Permanent delete destroys owner files by accident | low | high | D8 two-step token + typed `DELETE`; tests only on fixture copies (D10); phase 2.1 test asserts a delete without a fresh token fails; named before 2.1 starts |
| A test or script writes into the OneDrive Screenshots folder | low | high | D10; every `ss-core` write takes `Caller` and refuses `Caller::Cli` writes under a protected root or any `\OneDrive\` path (path-based, contract-tauri-ipc rule 8); Accept scripts use temp data dirs and fixture roots only |
| OneDrive Files On-Demand hydrates 1.7 GB onto C: | medium | medium | cloud-only files indexed from metadata, thumbnails deferred until the owner presses "Download cloud-only files" (human step, phase 1.4); never pin/unpin |
| VS Build Tools ignores `--installPath` or fills C: | medium | medium | dry-run prints the command; verifier asserts `link.exe` under `D:\IOT_Tools\apps\VSBuildTools`; C: free space checked before and after (abort under 10 GB) |
| Vietnamese OCR poor with RapidOCR | medium | medium | spike first in 3.1; PaddleOCR fallback is in the design |
| CUDA wheels fail on this driver | medium | low | `device: auto` falls back to CPU; GPU optional |
| Contract drift between lanes | medium | high | contracts pinned in this dir before fan-out; `contract_test.rs` fails on stale bindings or schema/fixture mismatch; lanes cannot edit contract files (D2) |
| Grid below 60 fps | medium | medium | measured at 1.4 by CDP probe; kill criterion in Forecast |
| Scope too large | high | medium | strict phase gates; app usable at 2.3; every phase has its own Accept |

## Dependencies

Critical path: 0 > 1.1 > 1.2 > 1.4 > 2.1 > 2.3 > 3.2 > 3.4 > 4.1 > 4.3 (core lane chain plus integrations). The ui chain runs beside it; the ai lane (3.1) starts after 1.1 and has slack until 3.4.
Blocks: nothing outside this repo. Blocked by: the phase 0 human approvals (UAC, env vars, HKLM value).

## Self-improvement loop

- **Signal:** a phase `Accept:` command exits non-zero, or `contract_test.rs` goes red on drift between `contract-*.md`, the Rust types and the generated TS bindings
- **Loop:** the cook's existing per-phase Accept gate (`t1k-record-verification.cjs`) records the failure and stops the phase before integration
- **Fix:** the owning lane fixes code; a contract change goes to the Owner, who edits the `contract-*.md` file and `crates/ss-core/src/contract/` together and re-briefs lanes; lessons go to the project's own `docs/lessons.md` (owner rule: nothing goes to the kit)
- **Guard:** `contract_test.rs` and every phase Accept re-run in each later integration phase (1.4, 2.3, 3.4, 4.3 run the full `cargo test`, `bunx vitest run`, `pytest sidecar/tests/` via their Accept scripts)

## Deferred decisions

Ordered irreversible / costliest first.

| # | Question | Options | Choice taken | Why | How to reverse |
|---|----------|---------|--------------|-----|----------------|
| 1 | Permanent-delete confirmation shape | single confirm / two-step token + typed word | two-step: `request_permanent_delete` returns a 60 s token, `delete_permanently` needs token + typed `DELETE` | the action is irreversible; the brief demands a separate explicit confirmation | edit `contract-tauri-ipc.md` § File ops before 2.1 |
| 2 | Plane work-item gate | bind / opt-out | opt-out (recorded `tracked:false`) | personal repo; owner forbids Plane | none needed |
| 3 | Review wave | 1 reviewer / none | none spawned | owner rule: no review unless asked | ask for `/t1k:plan red-team` |
| 4 | Where the "global" cargo config lives | `%USERPROFILE%\.cargo\config.toml` / `%CARGO_HOME%\config.toml` | `D:\DevCache\tools\rust\cargo\config.toml` | cargo reads `$CARGO_HOME/config.toml` (doc.rust-lang.org/cargo/reference/config.html, HTTP 200, 2026-11-23); with `CARGO_HOME` on D: a file in `~/.cargo` is never read | move the file and unset `CARGO_HOME` |
| 5 | OneDrive cloud-only files | hydrate automatically / on explicit button | explicit button, off by default | 1.7 GB onto C: is the owner's call; never pin/unpin | Settings toggle "Auto-download cloud-only" (added in 1.2, default off) |
| 6 | VS Build Tools source | bootstrapper URL / winget | `winget install --id Microsoft.VisualStudio.BuildTools --override "..."` | winget id 18.12.1 MEASURED; the `aka.ms/vs/18` URL redirects to Bing (MEASURED) | edit phase-0 script |
| 7 | Cargo tools set | many / minimal | `sccache`, `tauri-cli` (`cargo install --locked`); sccache installed BEFORE `rustc-wrapper` is written | YAGNI; a wrapper set before sccache exists breaks every cargo call | add tools to the script's `$CargoTools` array |
| 8 | Toolchain components | profile default / minimal + components | `--profile minimal` + `clippy`, `rustfmt`, `rust-src` | rust-docs is large and unused; rust-src needed by rust-analyzer | `rustup component add rust-docs` |
| 9 | VS Code extensions | - | `rust-lang.rust-analyzer`, `tauri-apps.tauri-vscode`, `ms-vscode.cpptools` (MSVC debugger), `tamasfe.even-better-toml`, `ms-python.python`, `charliermarsh.ruff`, `dbaeumer.vscode-eslint`, `bradlc.vscode-tailwindcss` | Rust + Tauri + MSVC debug + the three project languages | edit `$VsCodeExtensions` |
| 10 | Setup script safety | apply by default / dry-run by default | dry-run by default, `-Apply` to change anything | machine changes need owner approval first | - |
| 11 | Where `git init` happens | phase 0 / phase 1.1 | phase 1.1 | phase 0 script is machine-wide and reusable; the repo is project-specific | - |
| 12 | Sidecar packaging | Tauri `externalBin` exe / uv venv spawned by Rust | uv venv at `D:\IOT_Tools\data\ScreenshotStudio\sidecar-venv`, sidecar source bundled as a Tauri resource | a PyInstaller bundle with torch would be several GB inside the NSIS installer | swap D6 + phase 3.2 supervisor launch code |
| 13 | NSIS install dir D: | config key / `/D=` switch / custom template | `installMode: currentUser` + `/D=D:\IOT_Tools\apps\ScreenshotStudio` on the install command; NSIS restores the previous dir on upgrade | the schema has no install-dir property (fact-fetcher, Tauri config schema 2.12.7) | add `installerHooks` later |
| 14 | Image delivery to webview | asset protocol + dynamic scope / custom scheme | custom async scheme `ssimg` | one mechanism for thumbs and originals, no broad fs scope | D5 |
| 15 | Thumbnail sizes | - | WebP long edge 320 px (grid) and 1280 px (lightbox preview); original for zoom > 100% (format and when each size is made: superseded by #29 from phase 2.1) | two sizes cover grid and lightbox | constants in `ss-core::thumbs` |
| 16 | Text search | LIKE / FTS5 | FTS5 `unicode61 remove_diacritics 2` over file name + OCR + note | diacritic-insensitive is in the design | - |
| 17 | Vector storage | vector DB / SQLite BLOB + brute force | float32 BLOB in SQLite, cosine in Rust | 5,604 x 768 floats is ~17 MB, brute force is ms | add an index later if > 100k |
| 18 | Duplicate detection location | Python / Rust | Rust (`blake3` exact + `image_hasher` dHash, Hamming <= 6 default) | works without the sidecar | constant `DUP_HAMMING_DEFAULT` |
| 19 | AI defaults | - | OCR RapidOCR on CPU onnxruntime (GPU opt-in); embeddings `ViT-B-16-SigLIP-i18n-256` on `device: auto` | multilingual (vi) + fits 8 GB VRAM; CPU OCR avoids a cuDNN install | sidecar config |
| 20 | Package manager | npm / bun | npm for installs and `npx vitest` (invoked by `lane-tests.mjs`, D11) | npm is the installed baseline | - |
| 21 | Editor rendering | canvas library / Canvas 2D | plain Canvas 2D in the UI, final PNG bytes sent to Rust as a raw IPC body | no extra dependency | add Konva if annotations get complex |
| 22 | i18n | i18next / typed dictionary | typed dictionaries `src/i18n/vi.ts`, `en.ts` | two languages, no plural complexity | - |
| 23 | Autostart and hotkey defaults | - | autostart OFF, hotkey Ctrl+Alt+S ON (configurable) | autostart writes HKCU Run; the owner turns it on | Settings |
| 25 | Lane Accept shape | bare `cargo test` / `bunx vitest run` / `pytest dir` / per-phase runner script | `lane-tests.mjs` with the phase's own test files (D11) | the validation wave showed a bare runner passes on earlier phases' tests (vacuous green) | edit each phase `Accept:` |
| 26 | Integer typing in TS bindings | per-field ts override / ts-rs config | explicit override per field shape (`number`, `number | null`, `number[]`), `bigint` banned by test | `#[ts(type)]` replaces the whole type, so `Option`/`Vec` need their own string | contract-tauri-ipc rule 3 |
| 27 | Container deletions (root/tag/album/filter) | journal + undo / confirm only | confirm dialog, not journaled, never touches files | undoing a cascade of tags/notes needs a snapshot store, YAGNI for a catalog-only loss | add a soft-delete column later |
| 28 | Remote + CI | local only / GitHub + Actions | public `Nghaiz/screenshot-studio` (owner chose public over private on 2026-11-23), SSH remote `origin`, one PR per phase, squash-merge on green CI; CI on push to `main` and on PRs: `ui` job (lane runner over `src/**/*.test.ts`, `npm run build`) and `rust` job (lane runner over the contract test, `cargo test --workspace`, `cargo build -p screenshot-studio`), both `windows-latest` | owner asked for it in the session of 2026-11-23, overriding the earlier local-only rule | `git remote remove origin` and delete the repo |
| 29 | Thumbnail format and generation | keep lossless WebP 320 + 1280 at index / lossy WebP q80 320 at index + 1280 on first view / JPEG q82 | lossy WebP q80 (crate `webp` 0.3.1, added by phase 1.4 per D3), 320 at index, 1280 generated on first `ssimg` request and cached; implemented in phase 2.1, not 1.4 | MEASURED in 1.4 (2026-11-23, 60 real screenshots, Pillow/libwebp): lossless 320 + 1280 = ~240 KB/image, ~1.3 GB for 5,683 images, ~850 ms encode/image; lossy q80 320 = 7.3 KB (~41 MB), ~41 ms; JPEG q82 320 = 11.6 KB (~65 MB). Owner chose WebP q80 and phase 2.1 over fixing it in 1.4 | contract-sqlite-schema.md § thumbnails; the `thumbsFORMAT` marker forces one regeneration |
| 30 | App lib crate types | Tauri template `staticlib` + `cdylib` + `rlib` / `rlib` only | `rlib` only (phase 1.4) | Windows desktop only; the exe links the rlib. Under `lto = true` + `codegen-units = 1` each extra crate type is another fat-LTO pass: the 1.4 release build of `screenshot_studio_lib` aborted (0xC0000409) at 1.2 GB free commit | add the types back in `src-tauri/Cargo.toml` if a mobile target is ever added |
| 31 | Memory budget | none / measure only / budget | app + WebView2 private bytes <= 500 MB after scrolling the whole library; UI side in 2.2, `ssimg` cache header in 2.1, gated by `p2.mjs` in 2.3. **Phase 2.3 (2026-11-24), ORCHESTRATOR decision the owner can reverse:** the gate is the plan's literal step-5 measurement, a 10 s top-to-bottom scroll of the whole library (`lib/cdp.mjs scrollFrames`, the method 1.4 measured 1,208 MB with), judged on the PEAK of the samples over 10 s of idle after it. Visiting every thumbnail is measured and printed (`INFO memory-visit-all`), not gated | MEASURED in 1.4: 360 MB at first screen, 1,208 MB after a full scroll of 6,260 thumbnails; owner chose a budget fixed in 2.2 (2026-11-23). MEASURED in 2.3 (installed build, real library, 2 Accept runs): gated scroll peak 497 / 496 MB, 423-425 MB after idle (no headroom: 476-482 MB before #32's `contain`, 500 MB with it in 2 more runs); visit-all peak 790-797 MB, 604-609 MB after idle. Not reachable for visit-all: ~380-404 MB of WebView2 cost at the first screen, and Blink keeps the encoded bytes of every thumbnail it has shown (renderer 214 MB = 5,662 x 33.6 KB) through GC and memory pressure. WebView2 `SetMemoryUsageTargetLevel(LOW)` saved 40-50 MB (GPU process only) and was reverted; flags and canvas rendering also tried (docs/lessons.md 2026-11-24) | gate visit-all again: report its peak as the `memory` check in `scripts/accept/p2.mjs` (fails at ~800 MB until thumbnails shrink, #29, or the WebView2 cost drops); or change the number in phase-2.3 step 5 |
| 32 | Grid thumbnail fit (integration-observed defect, phase 2.3) | `cover` (square crop) / `contain` (whole screenshot over the cell surface) / the 1280 preview in grid cells | `contain` for square cells (Grid layout and Timeline, `.ss-square` in `src/index.css`); Masonry, Justified and List unchanged; no 1280 in the grid. ORCHESTRATOR decision, reversible | MEASURED 2026-11-24 on the real library, devicePixelRatio 1.5: `cover` showed Grid thumbnails a median 5.0x upscaled at size 320 (8.9x at 400), wide screenshots cropped to their middle; `contain`: 0.89x at the saved size 180, 1.51x at 320 (the DPR alone), 2.29x at 400. Masonry and Justified size each box by the image's aspect, so `cover` does not crop them. Cost, A/B on one build with the old rule injected over CDP: gated scroll peak +18-24 MB, visit-all peak +55 MB. The 1280 preview averages 204 KB encoded (6x a grid thumbnail) and Blink keeps every encoded thumbnail it has shown (#31), so it was not used | delete the `.ss-square .ss-cell img` rule in `src/index.css` |
| 24 | Merge-up tree | tree / flat | flat lane plan (tree preflight hard-stopped: not a git repo; see `## Tree step`) | no repo yet, and never a remote or PR; lanes merge locally per D7 | re-plan after 1.1 if wanted |


## Tree step

`bun ~/.claude/scripts/t1k-tree.cjs trigger plans/261123-screenshot-studio --leaves 15 --files ...` printed
`{"tree":true,"leaves":15,"files":6,...}`. The hard preflight `bun ~/.claude/scripts/t1k-tree.cjs preflight` then exited 1 with:

```
fatal: not a git repository (or any of the parent directories): .git
Command failed: git rev-parse --path-format=absolute --git-common-dir
```

Hard stop per the tree contract: no node file was written and the plan is FLAT (lanes per D7). The project has no git
repo until phase 1.1 and will never have a remote or PR, which the merge-up tree's single root PR assumes.

## Validation wave record (Group B + C, 2026-11-23)

- `t1k-data-verifier`: library 5,683 PNG / 1,740,293,215 bytes / subfolders 1ERD (16) + 4ERD (60) / 22 cloud-only; C: 29.48 GiB free; D: 97.5 GiB free; VS shared path on C:, CachePath unset; driver 610.88; WebView2 157.0.4341.60; uv/pip env vars on D:, the four new ones absent; no cargo, no VC tools. Fixed: count (5,604 -> 5,683 measured), fps threshold (18.2 ms is 54.9 fps -> 18.18 ms).
- `t1k-list-classifier` (R1-R8 over 15 phases): 98 C / 22 DRIFT / 0 MISSING; all 22 addressed (vacuous Accepts -> D11, undefined contract types defined, ownership gaps -> D2, ssctl surface pinned with output types, phase-0 dry-run dirs).
- `t1k-investigator` claim check: counter-examples fixed (Caller-based protected-root guard, `ssctl undo` needs a batch id, convert-overwrite backup, cloud-only open rule, palette cannot reach permanent delete, `any` search never waits on the sidecar, raw-body exception documented, i64 override per shape, journal ops, capability windows, drag plugin dependency).
- `t1k-tester`: all 9 validators exit 0; Hardness MATCH.
