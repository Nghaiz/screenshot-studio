# Cook guide — one new chat session per phase

Run the phases **in order**, **one per new chat session**, from the project root `D:\Screenshot_Picture`.
Each session: paste the command, let it finish, check that the Accept line printed PASS and the phase PR is
merged with green CI on GitHub, then close the session.

## Before phase 1.1 (once)

Phase 0 set new user env vars (`RUSTUP_HOME`, `CARGO_HOME`, `SCCACHE_DIR`, `HF_HOME`, PATH).
**Quit VS Code completely (every window) and reopen it**, otherwise the new Claude session will not see Rust.
Check in a new terminal: `cargo --version` and `node scripts/verify-rust-machine.mjs` (all PASS).

## Do the non-`phase-*` files need to be mentioned?

| File | What it is | Do you need to attach it? |
|---|---|---|
| `README.md` | Plan index: decision register D1–D11, lane ownership, deferred decisions, human steps | Yes: the line below tells the agent to read it |
| `contracts.md` + `contract-tauri-ipc.md`, `contract-sidecar-jsonrpc.md`, `contract-sqlite-schema.md` | Shared shapes between Rust / UI / Python | Linked from the phases that need them; the line below covers phases that do not link them (1.4, 2.3, 3.3, 4.2) |
| `design-intent.md` | Shared UI/UX, naming, paths decisions (themes, motion, layout) | Same as above; UI phases 3.3 and 4.2 do not link it, so the line below matters |
| `cook-guide.md` | This file | No |

So: **always paste the common context line** after the command. You do not need to attach anything else.

### Common context line (paste after every command)

```
Context: read the plan README.md (Decision register, Contract, Deferred decisions, Human steps) and design-intent.md first; the contract*.md files are the source of truth — do not edit them unless this phase is 1.1 or an integration phase (1.4, 2.3, 3.4, 4.3). Stay inside this phase's Owns: globs. Start from an up-to-date main (git switch main && git pull --ff-only), then work on branch phase-<id>. Run the phase's Accept command and show its output. Then push the branch and open a PR into main on the PUBLIC repo Nghaiz/screenshot-studio (gh pr create --base main, title "phase <id>: <goal>"), wait for CI to go green (fix red checks on the same branch), squash-merge it (gh pr merge --squash --delete-branch), and finish with git switch main && git pull --ff-only. Public repo: never commit secrets, .env files, screenshots or anything from the OneDrive folder. Personal project: no Plane.
```

## Commands

Plan dir: `D:\Screenshot_Picture\plans\261123-screenshot-studio`

| # | Command | Gate / note |
|---|---|---|
| 1 | `/t1k:cook D:\Screenshot_Picture\plans\261123-screenshot-studio\phase-1.1-scaffold-contracts.md` | DONE 2026-11-23 (`3466191`, `e818019`; CI green on `318d3e4`) |
| 2 | `/t1k:cook D:\Screenshot_Picture\plans\261123-screenshot-studio\phase-1.2-core-index.md` | Rust core lane |
| 3 | `/t1k:cook D:\Screenshot_Picture\plans\261123-screenshot-studio\phase-1.3-ui-browse.md` | UI lane, themes |
| 4 | `/t1k:cook D:\Screenshot_Picture\plans\261123-screenshot-studio\phase-1.4-p1-integration.md` | **Human:** approve first read-only index of the OneDrive folder; decide the "download cloud-only files" button |
| 5 | `/t1k:cook D:\Screenshot_Picture\plans\261123-screenshot-studio\phase-2.1-core-organize.md` | File ops, permanent delete |
| 6 | `/t1k:cook D:\Screenshot_Picture\plans\261123-screenshot-studio\phase-2.2-ui-organize.md` | |
| 7 | `/t1k:cook D:\Screenshot_Picture\plans\261123-screenshot-studio\phase-2.3-installer.md` | **Human:** approve install to `D:\IOT_Tools\apps\ScreenshotStudio`. After this, "Screenshot Studio" is in Windows Search. DONE 2026-11-24 (PR #7): installed to D:, Start Menu shortcut, found in Windows Search, single-instance, WebView2 data on D:; `p2.mjs` 11/11 PASS (gated scroll peak 496-497 MB, no headroom; visit-all 790-797 MB is INFO only: orchestrator decision in Deferred decision #31, reversible) |
| 8 | `/t1k:cook D:\Screenshot_Picture\plans\261123-screenshot-studio\phase-3.1-sidecar.md` | AI downloads (~4–5 GB on D:) already approved on 2026-11-23: say so if asked. DONE 2026-11-24 (PR #8): venv + SigLIP weights on D:, `--selftest` ok on CUDA; OCR spike 87.4% (RapidOCR kept) with a known Vietnamese tone-letter gap (Deferred decision #19); lane Accept 3/3 PASS. 3.1b DONE 2026-11-24 (PR #9): gap closed by the `rapidocr+easyocr` pipeline, 96.8% Vietnamese char accuracy, English unchanged |
| 9 | `/t1k:cook D:\Screenshot_Picture\plans\261123-screenshot-studio\phase-3.2-core-smart.md` || DONE 2026-11-24 (PR #10): sidecar supervisor + AI jobs + semantic search + duplicates/sessions/issues/stats; lane Accept 2/2 PASS (6 + 5 tests, fake sidecar, no venv). Real-sidecar smoke on a temp 6-image library: ready on CUDA in 13.7 s, OCR+embed 6/6 in 16.6 s, `hoa don` finds the Vietnamese receipt. For 3.4: the app now starts the sidecar at launch, so the p2 memory gate counts the Python tree; bundled resources must map to `<resources>/sidecar` and `<resources>/contracts/sidecar-jsonrpc.schema.json`; uv-launcher console window with `CREATE_NO_WINDOW` unverified |
| 10 | `/t1k:cook D:\Screenshot_Picture\plans\261123-screenshot-studio\phase-3.3-ui-smart.md` | DONE 2026-11-24 (PR #11): Settings > AI, degraded mode (per-capability gates, calm `starting`, banner once for absent/error), duplicate finder, cleanup wizard (trash / Recycle Bin only), sessions, SVG stats dashboard, category names, sidebar folders from `byFolder`; lane Accept 2/2 PASS (11 + 8 tests), full suite 75/75, build OK. For 3.4: memory gate not re-measured (new views are lazy); visual check was mock-only, not the real library; auto-redact control is 4.2 (`useAiGate("pii")` ready) |
| 11 | `/t1k:cook D:\Screenshot_Picture\plans\261123-screenshot-studio\phase-3.4-p3-integration.md` | DONE 2026-11-24 (PR #12): sidecar bundled (`bundle.resources` -> `<resources>/sidecar` + `<resources>/contracts`, a CI step checks the copy against git); `p3.mjs` 17/17 PASS (fixture OCR search incl. `hoa don`, planted exact/similar duplicates, sidecar absent; installed app: bundled files, `ready` on CUDA from the install dir, no console window, sidecar exits with the app); p2 11/11 PASS with the sidecar subtree cut out of the memory gate, peak 463/472/476 MB after the fix (shimmer paused while scrolling + WebView2 memory target LOW; budget unchanged). Real-library AI pass: 5,662 done / 0 failed / 598 cloud-only per task in 2 h 32 min on CUDA. For 4.x: the uninstaller leaves the sidecar's `__pycache__` in the install dir (4.3); 24-37 MB memory headroom; `similar` duplicates lump near-blank images together. 3.4b DONE 2026-11-24 (PR #13): smart views and Settings > AI open in 12-29 ms (were 306-325 ms on a React Suspense fallback; now eager imports + a labelled loading state, `p3.mjs` `smart-view-ready` budget 150 ms); low-information dHashes left out of `similar` (Deferred decision #33: real library 219 -> 213 groups, 330 -> 306 copies, exact unchanged); sidecar bytecode under `<data dir>\sidecar-pycache`, install dir clean (`no-pycache-in-install-dir`); p3 22/22, p2 11/11 x3 (peak 471/476/478 MB). Open: the cua-driver-seen 2-5 s / whole-window blank did not reproduce over CDP. 3.4c DONE 2026-11-24 (PR #14, stopped before merge for the orchestrator's cua-driver re-check of the duplicates view): `similar` means near-identical copies (Deferred decision #35): dHash candidates must also fit a resize's aspect, and stored OCR text (>= 0.90) and embeddings (cosine >= 0.96) must agree, or without either the same size and dHash/pHash within 2 bits; one rule for the duplicates view, "keep the best", the cleanup wizard and Stats. Real library 213 -> 16 groups, 306 -> 16 copies, largest 27 -> 2 (the 27-image ER group and the 9-image group gone; exact unchanged); `find_duplicates` similar 72-75 -> 23-25 ms (52-56 ms at the slider's maximum 20, was 18-20), `get_stats` 82-88 -> 33-36 ms; p3 23/23 (new `dupes-similar-no-ai`; `smart-view-ready` 8-29 ms), p2 11/11 x3 (peak 473/474/470 MB) |
| 12 | `/t1k:cook D:\Screenshot_Picture\plans\261123-screenshot-studio\phase-4.1-core-live.md` | |
| 13 | `/t1k:cook D:\Screenshot_Picture\plans\261123-screenshot-studio\phase-4.2-ui-editor.md` | |
| 14 | `/t1k:cook D:\Screenshot_Picture\plans\261123-screenshot-studio\phase-4.3-release.md` | **Human:** approve reinstall of the final build |

Phase 3.1 only depends on 1.1, so it can run any time after 1.1 if you want AI earlier.
Do **not** run two sessions at the same time: they share one working tree and would overwrite each other.

## If a session goes wrong

- Accept fails after 3 tries → stop, start a new session with `/t1k:fix <phase file> <the failing output>`.
- A lane needs a contract change → it must stop; run the next integration phase or ask in a new session.
- Session context gets long mid-phase → `/t1k:handoff save`, new session, `/t1k:handoff resume`.
