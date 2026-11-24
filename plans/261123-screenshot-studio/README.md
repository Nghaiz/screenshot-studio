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
Re-score (phase 3.1, 2026-11-24, `node scripts/accept/lane-tests.mjs sidecar/tests/test_contract.py sidecar/tests/test_ocr.py sidecar/tests/test_embed.py` 3/3 PASS, 32 + 5 + 5 tests):
- Vietnamese OCR (uncertainty 1): MEASURED. RapidOCR 3.10.0 defaults (PP-OCRv6 small, onnxruntime CPU) on 20 LOCAL owner
  screenshots with Vietnamese text (60 sampled at random from 5,587 local files, copied read-only to %TEMP%, deleted
  after): **87.4% character accuracy** (272 edits / 2,156 chars; per image 84.7-91.7%) on 5 hand-transcribed ones,
  line-matched Levenshtein after NFC; 89.3% with diacritics stripped. Above 85%: RapidOCR stays (Deferred decision
  #19). Most errors are the 44 tone-marked letters the model cannot emit (#19 known gap).
- OCR time per image on CPU (Ryzen 9 7945HX, after warm-up): the 20 Vietnamese screenshots mean 1.03 s, median 0.98 s,
  max 2.06 s (30 lines on average); all 60 sampled: mean 0.92 s. About 1.6 h of CPU for 6,260 images. GPU OCR not
  measured (CPU onnxruntime per #19).
- CUDA (uncertainty 3): MEASURED PASS. torch 2.15.0+cu132 sees the RTX 4060 Laptop GPU (CUDA 13.2, driver 610.88);
  `python -m ss_sidecar --selftest` -> `{"capabilities":["ocr","embed","classify","color","pii"],"device":"cuda","ok":true,"errors":[]}`
  (13.8 s); protocol `initialize` answers in ~11 s (budget 120 s). SigLIP runs under fp16 autocast on CUDA.
- Semantic sanity (generated images): text "cat" vs a drawn cat 0.102, vs a text screenshot -0.015; Vietnamese
  "con mèo" 0.083 vs 0.016; zero-shot cat/document both right (>= 0.997).
- New risks: (1) the OCR gap above; (2) a library DLL load (numpy/OpenBLAS) stalled while the main thread blocked
  reading the stdin pipe, fixed by giving libraries NUL as stdin (docs/lessons.md); (3) the sidecar validates params
  against `contracts/sidecar-jsonrpc.schema.json` at run time, so the bundled app (3.4/4.3) must ship that file or set
  `SS_CONTRACT_SCHEMA`. OCR forecast row: English and Vietnamese without tone-marked letters PASS (`test_ocr.py`);
  words with them cannot be found. P3 confidence unchanged at 45%.
Re-score (phase 3.1b, 2026-11-24, same Accept, 3/3 PASS, 42 + 6 + 5 tests): Vietnamese OCR gap closed (Deferred decision #19).
- Same 5 hand-transcribed Vietnamese screenshots (re-copied read-only to %TEMP%, deleted after), line-matched
  character accuracy after NFC: RapidOCR alone 87.4% (tone-marked letters 0/219) -> `rapidocr+easyocr` **96.8%**
  (206/219 on CPU, 205/219 on CUDA). EasyOCR alone over RapidOCR's boxes: 96.0%; EasyOCR's own detector: 93.0%.
- English, no regression: a hand-transcribed English settings screenshot 100% -> 100% (347 chars); generated English
  lines 100%; 8 URL/email/version/file-name lines in 2 fonts x 3 sizes 48/48 exact (EasyOCR alone: 34/48).
  Generated tone-letter lines (`test_ocr.py`): 0/29 -> 29/29 letters, every line verbatim.
- Time per image on the 5 (after warm-up): RapidOCR alone 0.74 s; pipeline 1.48 s on CPU, 0.88 s with EasyOCR on CUDA
  (the default whenever `device` resolves to cuda). About 1.5-2.6 h for 6,260 images.
- Cost: `easyocr` + its deps (scipy, scikit-image, python-bidi, ninja) in the venv; `latin_g2.pth` 15 MB in
  `<HF_HOME>\easyocr` (`--prefetch`; without it `ocr` is disabled with a named reason). OCR forecast row: Vietnamese
  tone-marked words now PASS in `test_ocr.py`; the real-library search check stays with 3.4.
Re-score (phase 3.4, 2026-11-24, `node scripts/accept/p3.mjs` 17/17 PASS and `node scripts/accept/p2.mjs` 11/11 PASS
in 3 runs on the installed release build):
- OCR search finds known Vietnamese and English text (fixtures): MEASURED PASS. Generated `Hoá đơn 2026 INVOICE-7731`
  (no file name carries it): `INVOICE-7731` and `hoa don` both hit it at rank 1; OCR + embed of 52 images on CUDA in
  23.6 s including sidecar start. Planted duplicates: exact = {#7, its copy}, similar = {#9, its JPEG} (+ the exact
  pair), nothing else. Sidecar absent (`sidecarVenv` -> a missing dir): status `absent`, FTS search still answers.
- App fully usable with the sidecar absent: MEASURED PASS (above; plus 3.3's degraded-mode tests in the full suites).
- Bundled sidecar (D6/#12): `bundle.resources` ships `ss_sidecar/*.py`, `ss_sidecar/engines/*.py` and the schema
  (18 + 1 files, byte-identical to git). The installed app reaches `ready` on CUDA ~15 s after launch with the dev
  checkout fallback disabled (`SS_SIDECAR_DIR` -> a missing dir); seen red with the bundled `sidecar` or `contracts`
  dir renamed (state `absent` / `error`). No console window: the venv launcher and its child interpreter share one
  console created with `CREATE_NO_WINDOW` (GetConsoleWindow = 0 for both); the sidecar exits with the app (WM_CLOSE).
- Memory (#31), app + WebView2 with the sidecar's subtree cut out: gated scroll peak 463 / 472 MB (2 runs), 476 MB in
  a third run after the AI pass (OCR text and embeddings on 5,662 images), first screen 338-340 MB, visit-all
  732-737 MB peak / 557-562 MB after idle, fps p50 4.20 ms p95 4.30 ms (unchanged). Before
  the fix: 495-519 MB (FAIL 512 in the first run), visit-all 797 / 605 MB. Sidecar (INFO, not gated): ~6.7 GB
  private (commit; CUDA/torch reserve), 2.96 GB working set, 1.64 GB dedicated GPU memory (nvidia-smi reports
  `[N/A]` per process under WDDM; the MB come from the Windows GPU Process Memory counters).
- Real-library AI pass (step 3; `ssctl ai --tasks ocr,embed,classify,color --wait` with the app closed, the
  orchestrator's route instead of the in-app "Run AI on library"): wall 9,097 s (2 h 32 min), sidecar `ready` on
  CUDA. Every task: 6,260 candidates, 5,662 done, 0 failed, 598 skipped as cloud-only. OCR took ~2 h 26 min of it
  (~1.55 s per image, the 3.1b RapidOCR + EasyOCR pipeline); embed, classify and colour together ~5 min. The Forecast
  effect "first OCR+CLIP pass 10 / 20 / 60 min" was 2.5x under even the high figure: OCR, not CLIP, sets the time.
- Real-library spot checks (counts only): `search "chiếu"` (a word read in a local screenshot) 141 hits, every one
  with OCR text and none by file name; `chieu` gives the same 141 (tone marks folded). `dupes --mode exact`: 9 groups
  of 2 (18 images), each group one content hash. `--mode similar`: 219 groups, 549 images (179 pairs, largest 27),
  every exact group inside a similar group.
- New risks: (1) the installed sidecar writes `__pycache__` under `<install dir>\sidecar` at first start, which
  the uninstaller does not list (leftover folder after an uninstall, 4.3); (2) the memory gate's headroom is 24-37 MB
  and depends on two WebView2 behaviours (LOW target, shimmer frames), docs/lessons.md phase 3.4; (3) `similar`
  groups near-blank images together: one real group of 17 has dHash popcounts 0-7, aspect 0.33-6.5 and a 1x1 px
  member, so flat images collide however different they are. With its opt-in "similar" box ticked, the cleanup
  wizard would propose 16 of them for the in-app trash. No phase owns a fix yet (e.g. skip hashes with few set bits
  in `similar` mode).
Re-score (phase 3.4b, 2026-11-24, real-library polish: `node scripts/accept/p3.mjs` 22/22 PASS and
`node scripts/accept/p2.mjs` 11/11 PASS in 3 runs on the installed release build; counts only):
- Smart views (Deferred decision #34), first open after a cold start, click to the frame after the content (CDP,
  installed app, real data dir, sidecar loading): lazy chunks 306-325 ms each, of which the chunk took 1-2 ms and the
  IPC <= 6 ms; React's >= 300 ms Suspense reveal throttle was the rest. Eager: `smart-view-ready` stats 29,
  duplicates 22 (2 frames of its loading state first), cleanup 12, settings-ai 17 ms (budget 150 ms); scratch runs
  10-28 ms. Backend, not on the open path: `get_stats` 89-93 ms (similar grouping ~70 ms of it, read at launch and
  after a library change; Stats opens from that cache), `find_duplicates` similar 75-80 ms. The 2-5 s blank and the
  whole-window blank the orchestrator saw through cua-driver did NOT reproduce over CDP (DOM content at ~0.3 s before
  the fix, sidebar present in all 993 screencast frames of 4 switches): open for the cua-driver re-check.
- `similar` duplicates (#33): 55 low-information hashes (1.0%) left out; 219 groups / 549 images / 330 copies /
  largest 27 -> 213 / 519 / 306 / 27; the group of 17 near-blank images is gone; `exact` unchanged (9 groups of 2).
- Sidecar bytecode: `PYTHONPYCACHEPREFIX` = `<data dir>\sidecar-pycache`. The install dir holds no `__pycache__`
  after two launches (`no-pycache-in-install-dir`; the 2 dirs and 16 `.pyc` from 3.4 deleted). Cost: once per data
  dir, `--selftest` 13.2 -> 40.1 s into an empty prefix (3,162 `.pyc`, 78 MB) and 13.4 s after; in the app, `ready`
  22.4 s on a fresh data dir vs ~15 s in 3.4.
- Memory (#31): gated scroll peak 471 / 476 / 478 MB (3.4: 463 / 472 / 476), first screen 339-343 MB, visit-all
  732-734 MB peak / 557-561 MB after idle, fps p50 4.20 ms p95 4.30 ms: the eager views cost nothing measurable;
  headroom 22-29 MB.
- Risks (1) and (3) of phase 3.4 are closed; (2) stands. New: the real catalog has no `blur_score` yet (indexed before
  3.2), so the cleanup wizard's first Start reads all 5,662 thumbnails (`backfill_blur`), not measured here.
  P3 confidence: done (Accept green).
Re-score (phase 3.4c, 2026-11-24, `similar` = near-identical copies, Deferred decision #35: `node scripts/accept/p3.mjs`
23/23 PASS and `node scripts/accept/p2.mjs` 11/11 PASS in 3 runs on the installed release build; counts only):
- Real library, `ssctl --data-dir D:\IOT_Tools\data\ScreenshotStudio dupes --mode similar --json` (Hamming 6):
  213 groups / 519 images / 306 copies / largest 27 -> **16 / 32 / 16 / 2**; `exact` unchanged (9 groups of 2, all 9
  inside `similar` groups). The 27-image group (one diagram layout, lowest id 23) is gone: its seed is in no group
  and 8 of its members form 4 pairs, each pair with identical OCR text. The 9-image group (lowest id 165): none of its
  members is in a group. Audit of the 16 seed-member pairs: OCR text similarity min 0.961 / p10 0.963 / median 1.000,
  cosine min 0.967 / p10 0.973 / median 1.000, none confirmed by hashes alone. At Hamming 20 (the slider's maximum):
  399 / 5,048 copies / largest 492 -> 20 / 20 / 2 (text min 0.909, cosine min 0.963). With the AI rows ignored (a
  catalog copy without `ocr_result` and `embedding`): 10 groups / 10 copies (the exact pairs and 1 same-size copy).
- Candidate stage: 875 pairs within Hamming 6, of which 59 have a shape a copy could have (434,069 and 2,321 at 20).
  pHash: populated for all 5,662 indexed images, but 461 of the 875 pairs are at pHash distance 0, so it only
  backs the hash-only rule.
- Synthetic copies (20 LOCAL owner screenshots copied read-only to %TEMP%, 6 variants each, indexed with OCR +
  embed in a temp data dir; images and catalog deleted after): candidate pairs within Hamming 6 -> confirmed:
  JPEG q85 19 -> 17, 75% resize 20 -> 16, 50% resize 20 -> 13, cursor 20 -> 17, 1.5% scroll 14 -> 12, notification
  toast 16 -> 7 (3 of the 9 misses are toasts on 280 px-high crops). With AI rows ignored: JPEG 17, cursor 18, toast
  6, scroll 2, resizes 0. Conservative by design: notification-sized overlays and 50% downscales are missed most.
- Labelled real pairs (OCR line diffs read in a scratch dir, nothing kept): 11 of one window whose content changed,
  text 0.633-0.847; 2 of one template with other content, text
  0.923 / 0.966 but cosine 0.955 / 0.908; 4 copies (a clock tick, an overlay, OCR noise), text 0.961-0.996.
- Speed, installed app on the real data dir, IPC over CDP (7 calls x 2 runs, ms): `find_duplicates` similar at
  Hamming 6 72-75 -> 23-25; at 20 (the slider's maximum) 18-20 -> 52-56 (the old figure rode on its huge groups:
  the first seeds took hundreds of images, so few BK-tree queries ran); `get_stats` 82-88 -> 33-36; exact
  4.4-5.0 -> 5.0-6.3. The candidate stage scans all 15.7M hash pairs of a flat array instead of querying a BK-tree,
  and OCR trigram profiles are built only for pairs that reach the text check (docs/lessons.md phase 3.4c).
  `smart-view-ready` (cold start, real data dir): stats 29, duplicates 23, cleanup 8, settings-ai 17 ms (3.4b: 29 / 22 /
  12 / 17). New `p3.mjs` check `dupes-similar-no-ai`: the planted JPEG of fixture #9 is found before the AI pass too.
- Memory (#31), `p2.mjs` x3: gated scroll peak 473 / 474 / 470 MB (3.4b: 471 / 476 / 478), first screen 340-344 MB,
  visit-all 729-736 MB peak / 555-562 MB after idle, fps p50 4.20 ms p95 4.30 ms: unchanged, as expected for a
  backend-only change. P3 confidence: done (Accept green); open for the orchestrator's cua-driver re-check of the
  duplicates view.
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
| 12 | Sidecar packaging | Tauri `externalBin` exe / uv venv spawned by Rust | uv venv at `D:\IOT_Tools\data\ScreenshotStudio\sidecar-venv`, sidecar source bundled as a Tauri resource (phase 3.4: `tauri.conf.json` `bundle.resources`, one glob per package dir plus the schema, landing at `<install dir>\sidecar` and `<install dir>\contracts`; CI and `p3.mjs` `bundled-files` compare them with git) | a PyInstaller bundle with torch would be several GB inside the NSIS installer | swap D6 + phase 3.2 supervisor launch code |
| 13 | NSIS install dir D: | config key / `/D=` switch / custom template | `installMode: currentUser` + `/D=D:\IOT_Tools\apps\ScreenshotStudio` on the install command; NSIS restores the previous dir on upgrade | the schema has no install-dir property (fact-fetcher, Tauri config schema 2.12.7) | add `installerHooks` later |
| 14 | Image delivery to webview | asset protocol + dynamic scope / custom scheme | custom async scheme `ssimg` | one mechanism for thumbs and originals, no broad fs scope | D5 |
| 15 | Thumbnail sizes | - | WebP long edge 320 px (grid) and 1280 px (lightbox preview); original for zoom > 100% (format and when each size is made: superseded by #29 from phase 2.1) | two sizes cover grid and lightbox | constants in `ss-core::thumbs` |
| 16 | Text search | LIKE / FTS5 | FTS5 `unicode61 remove_diacritics 2` over file name + OCR + note | diacritic-insensitive is in the design | - |
| 17 | Vector storage | vector DB / SQLite BLOB + brute force | float32 BLOB in SQLite, cosine in Rust | 5,604 x 768 floats is ~17 MB, brute force is ms | add an index later if > 100k |
| 18 | Duplicate detection location | Python / Rust | Rust (`blake3` exact + `image_hasher` dHash, Hamming <= 6 default) | works without the sidecar | constant `DUP_HAMMING_DEFAULT` |
| 19 | AI defaults | - | OCR pipeline `rapidocr+easyocr` (phase 3.1b, ORCHESTRATOR decision): RapidOCR 3.10 (PP-OCRv6 small det + rec, CPU onnxruntime) reads every line, and EasyOCR 1.7.2 `latin_g2` (vi + en, torch on the `initialize` device) re-reads the same boxes. Only the differing segments that carry a letter RapidOCR cannot emit are taken from EasyOCR; a line whose two readings disagree wholesale (difflib ratio < 0.6) is taken from EasyOCR if it has such a letter. Data: `SS_SIDECAR_OCR_PIPELINE=rapidocr` restores RapidOCR alone. Embeddings `ViT-B-16-SigLIP-i18n-256` on `device: auto` | Phase 3.1 kept RapidOCR alone (87.4% >= the 85% kill line), but its PP-OCRv6 dictionary has none of the 44 tone-marked Vietnamese letters (x2 cases: `ế ậ ờ ữ ỵ`...), so `điều` read `điu` and FTS missed those words. 3.1b measured: no RapidOCR/PaddleOCR rec dictionary covers Vietnamese (best: PP-OCRv6, 46/134 letters; `latin_PP-OCRv5` 35/134; PaddleOCR's HF models are the same families). EasyOCR `latin_g2` has 134/134, but read alone it drops dots and slashes (34/48 exact on URL/email/file-name lines vs 48/48), hence the merge. Numbers: Re-score (phase 3.1b) | `SS_SIDECAR_OCR_PIPELINE` (`ss_sidecar/config.py`); drop `easyocr` from `sidecar/pyproject.toml` with it |
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
| 31 | Memory budget | none / measure only / budget | app + WebView2 private bytes <= 500 MB after scrolling the whole library; UI side in 2.2, `ssimg` cache header in 2.1, gated by `p2.mjs` in 2.3. **Phase 2.3 (2026-11-24), ORCHESTRATOR decision the owner can reverse:** the gate is the plan's literal step-5 measurement, a 10 s top-to-bottom scroll of the whole library (`lib/cdp.mjs scrollFrames`, the method 1.4 measured 1,208 MB with), judged on the PEAK of the samples over 10 s of idle after it. Visiting every thumbnail is measured and printed (`INFO memory-visit-all`), not gated. **Phase 3.4 (2026-11-24), ORCHESTRATOR brief:** the AI sidecar the app starts at launch is not app + WebView2: `p2.mjs` cuts the subtree under the app's `python.exe` out of every sample and prints it as INFO (private bytes, GPU memory). The gate then still failed (512 MB) with the 3.3 UI, so two levers went in, budget unchanged: the grid pauses its placeholder shimmer while it scrolls (`src/views/useScrollingFlag.ts`, `[data-scrolling]` in `src/index.css`) and WebView2's memory target is LOW for the window's life (`src-tauri/src/window.rs`, `webview2-com` 0.39.1 + `windows-core` 0.62.2, the versions Tauri already links) | MEASURED in 1.4: 360 MB at first screen, 1,208 MB after a full scroll of 6,260 thumbnails; owner chose a budget fixed in 2.2 (2026-11-23). MEASURED in 2.3 (installed build, real library, 2 Accept runs): gated scroll peak 497 / 496 MB, 423-425 MB after idle (no headroom: 476-482 MB before #32's `contain`, 500 MB with it in 2 more runs); visit-all peak 790-797 MB, 604-609 MB after idle. Not reachable for visit-all: ~380-404 MB of WebView2 cost at the first screen, and Blink keeps the encoded bytes of every thumbnail it has shown (renderer 214 MB = 5,662 x 33.6 KB) through GC and memory pressure. WebView2 `SetMemoryUsageTargetLevel(LOW)` saved 40-50 MB (GPU process only) and was reverted; flags and canvas rendering also tried (docs/lessons.md 2026-11-24) | gate visit-all again: report its peak as the `memory` check in `scripts/accept/p2.mjs` (fails at ~800 MB until thumbnails shrink, #29, or the WebView2 cost drops; 732 MB since 3.4); or change the number in phase-2.3 step 5. 3.4 levers: delete `hold_low_memory_target` (window.rs) and the `[data-scrolling]` rule; never one without the other (alone, the pause raises visit-all to ~1,070 MB, docs/lessons.md phase 3.4) |
| 32 | Grid thumbnail fit (integration-observed defect, phase 2.3) | `cover` (square crop) / `contain` (whole screenshot over the cell surface) / the 1280 preview in grid cells | `contain` for square cells (Grid layout and Timeline, `.ss-square` in `src/index.css`); Masonry, Justified and List unchanged; no 1280 in the grid. ORCHESTRATOR decision, reversible | MEASURED 2026-11-24 on the real library, devicePixelRatio 1.5: `cover` showed Grid thumbnails a median 5.0x upscaled at size 320 (8.9x at 400), wide screenshots cropped to their middle; `contain`: 0.89x at the saved size 180, 1.51x at 320 (the DPR alone), 2.29x at 400. Masonry and Justified size each box by the image's aspect, so `cover` does not crop them. Cost, A/B on one build with the old rule injected over CDP: gated scroll peak +18-24 MB, visit-all peak +55 MB. The 1280 preview averages 204 KB encoded (6x a grid thumbnail) and Blink keeps every encoded thumbnail it has shown (#31), so it was not used | delete the `.ss-square .ss-cell img` rule in `src/index.css` |
| 33 | `similar` duplicates and near-blank images (integration-observed defect, phase 3.4) | keep every hash / leave low-information hashes out of `similar` / a per-pair aspect-ratio rule | a dHash with fewer than `DUP_MIN_INFO_BITS` (10) set bits, or fewer than 10 clear bits (`smart::duplicates::is_low_information`), neither seeds nor joins a `similar` group; it stays eligible for `exact`. Stats' `duplicatesSimilar` ("Bản gần giống") counts the same `groups()`, and the slider's help text says it. ORCHESTRATOR decision (conservative, reversible), phase 3.4b | MEASURED 2026-11-24 on a copy of the real catalog (5,662 hashes, counts only): the 13 hashes with 0 set bits are all 1-14 px slivers; every hash with 0-6 set bits had a partner within Hamming 6. Pairs within Hamming 6, by the lower hash's set bits, with aspect ratios within 15% of each other: 0-3 bits 30/231 (13%), 4-6 0/1, 7-9 2/9 (22%); 10-12 16/26 (62%), 13-15 28/36 (78%), 16-19 38/61 (62%), 20-24 419/444 (94%), 25+ 209/303 (69%). The 4 hashes with 58-64 set bits (left-to-right gradients: dHash sets a bit where the right cell is brighter) formed 2 pairs, both of different aspects. 55 hashes are left out (1.0%). `dupes --mode similar` 219 groups / 549 images / 330 copies / largest 27 -> 213 / 519 / 306 / 27 (the group of 17 near-blank images is gone); `exact` unchanged (9 groups of 2) | `DUP_MIN_INFO_BITS = 0` in `crates/ss-core/src/smart/duplicates.rs` |
| 34 | How the smart views and Settings > AI load (integration-observed defect, phase 3.4) | lazy chunks behind `Suspense` (3.3) / lazy + preload at idle / eager imports | eager imports (`src/views/ToolView.tsx`, `src/components/SettingsPanel.tsx`); a view is still mounted only while open. Data in flight shows `ViewLoading` (label + pulsing dot in theme tokens; the dot holds still with motion off or `prefers-reduced-motion`). Gated by `p3.mjs` `smart-view-ready` (`SMART_VIEW_READY_MAX_MS` 150 per view). ORCHESTRATOR brief, phase 3.4b | MEASURED 2026-11-24, installed app, real library, CDP, first open after a cold start: lazy 306-325 ms from click to content while the chunk loaded in 1-2 ms and IPC took <= 6 ms: react-dom 19.3.1 holds a Suspense fallback for >= 300 ms (`FALLBACK_THROTTLE_MS`), and the fallback (a bare shimmer panel) looks like an empty pane. Eager: 10-29 ms. The four chunks were 22 KB, so the preload option bought nothing but a timing dependency. Memory gate: README Re-score phase 3.4b | put `lazy` + `Suspense` back in both files (each first open then sits >= 300 ms on a fallback) |
| 35 | What `similar` duplicates mean (integration-observed defect, orchestrator's cua-driver check of 3.4b) | dHash Hamming alone (3.2-3.4b) / dHash + pHash / dHash as the candidate stage + a confirm stage from evidence the catalog already holds | `similar` = **near-identical copies** (a re-encode, a resize, or a small change such as a cursor, a notification or a few pixels of scroll), not "same layout". Candidate stage: dHash Hamming <= the slider (default 6, unchanged) and an aspect ratio a copy could have (`SHAPE_SLACK_PX` 1: within the rounding of a resize of the smaller image). Confirm stage (`smart::near_copy::confirm`), every check that applies must pass: OCR text (trigram Dice of lower-case, diacritic-folded words) >= `TEXT_MIN` 0.90 when both images have an OCR result and either has text, so a pair whose text differs from its seed's by more than OCR noise is never grouped; SigLIP cosine >= `COSINE_MIN` 0.96 when both have an embedding of the same model; when neither applies (AI absent, or no text and no embedding): the same pixel size and dHash and pHash each within `HASH_ONLY_MAX` 2 bits, so without AI a resize is not found. One rule (`smart::duplicates::groups`) for `find_duplicates` (duplicates view, its "keep the best" button, the cleanup wizard's opt-in similar step) and `get_stats` `duplicatesSimilar` ("Bản gần giống"). The candidate stage scans all pairs of a flat hash array instead of querying a BK-tree. ORCHESTRATOR decision (conservative, reversible), phase 3.4c | MEASURED 2026-11-24 (counts only; details in Re-score phase 3.4c). Real library before: 213 groups / 306 copies / largest 27, including the 27 screenshots of one diagram layout (other labels) and 9 unrelated screenshots. pHash is populated for every indexed image but is as coarse as dHash: 461 of the 875 candidate pairs have pHash distance 0. Synthetic copies of 20 local screenshots (6 variants each, copies deleted after) set the floors (JPEG cosine min 0.973, 75% resize 0.989; text >= 0.90 for 17/19 JPEGs); 13 real pairs labelled from their OCR text set the ceilings (one window whose content changed: text <= 0.847; one template with other content: cosine 0.908 and 0.955). After: 16 groups / 16 copies / largest 2 (9 of them the exact pairs); neither group remains; at Hamming 20, 399 / 5,048 / 492 -> 20 / 20 / 2; with the AI rows ignored, 10 / 10 / 2 | `confirm` returning `shape_fits(a, b)` restores hash-only grouping plus the shape check; for 3.4b exactly, also drop `shape_fits` from `candidates` (`crates/ss-core/src/smart/`) |
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
