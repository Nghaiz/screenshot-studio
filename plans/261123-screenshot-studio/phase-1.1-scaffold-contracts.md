# Phase 1.1: Repo, workspace scaffold, all dependencies, contracts in code (serial)

Lane: serial
Depends-on: 0
Effort: M
Creates: `scripts/accept/lane-tests.mjs`, `.gitignore`, `Cargo.toml`, `crates/ss-core/`, `crates/ss-core/src/contract/`, `crates/ss-core/bindings/`, `crates/ss-core/tests/contract_test.rs`, `contracts/`, `src-tauri/`, `src/`, `src/__tests__/contract.test.ts`, `sidecar/pyproject.toml`, `package.json`, `vite.config.ts`, `tsconfig.json`, `vitest.config.ts`, `docs/lessons.md`
AX: AX-01 `lane-tests.mjs <test files>` prints one `PASS|FAIL <file> <counts>` line per file, exit 0 only when every file ran >= 1 test with none skipped, 1 otherwise, 2 on usage
Accept: `node scripts/accept/lane-tests.mjs crates/ss-core/tests/contract_test.rs src/__tests__/contract.test.ts`

## Goal

A local git repo with a buildable empty app (`cargo tauri dev` opens a themed blank window), every dependency of the
whole plan already declared (D3), and the three contracts transcribed into code with an executable contract test.
No lane starts until this phase is committed on `main` (lanes branch from it).

## Steps

1. Precondition: phase 0 verifier passes in a NEW terminal (Human step "restart terminals"). `git init -b main` in
   `D:\Screenshot_Picture` (local only; NO `git remote add`, ever). `.gitignore`: `target/`, `node_modules/`, `dist/`,
   `tests/fixtures/out/`, `.venv/`, `*.sqlite*`, `crates/ss-core/bindings/` is NOT ignored (bindings are committed).
   First commit = `plans/` as-is.
2. Scaffold Tauri 2 + React + TS + Vite (`npm create tauri-app@latest` into a temp dir, then move the pieces; or
   `cargo tauri init` over a Vite React-TS app). Product name `Screenshot Studio`, identifier `dev.nghaiz.screenshotstudio`.
3. Root virtual workspace `Cargo.toml` `[workspace] members = ["crates/ss-core", "src-tauri"]`, `resolver = "3"`,
   shared `[workspace.dependencies]`. No `TS_RS_EXPORT_DIR` and no `#[ts(export)]`: bindings are written only by
   `SS_WRITE_BINDINGS=1 cargo test -p ss-core --test contract_test` (ts-rs `export_all_to`), see contracts.md.
4. Declare ALL dependencies now (ledger rows 1-26 and 38; use `std::thread::available_parallelism`, not `num_cpus`):
   - `ss-core`: rusqlite (bundled), notify, notify-debouncer-full, image, fast_image_resize, image_hasher, trash, ts-rs,
     walkdir, rayon, blake3, serde, serde_json, tokio (process, rt-multi-thread, sync, time, io-util), thiserror,
     zip, printpdf, jsonschema (dev, for the contract test), tempfile (dev). Bin target `ssctl` (`src/bin/ssctl.rs`, stub).
   - `src-tauri`: tauri (features `tray-icon`), tauri-plugin-{fs,dialog,clipboard-manager,global-shortcut,single-instance,autostart,notification,opener,drag}, ss-core.
   - npm: `@tauri-apps/api`, `@tauri-apps/plugin-{fs,dialog,clipboard-manager,global-shortcut,autostart,notification,opener}`,
     `@crabnebula/tauri-plugin-drag`, react, react-dom, `@tanstack/react-virtual`, `@tanstack/react-query`, zustand, motion, tailwindcss, `@tailwindcss/vite`;
     dev: typescript, vite, `@vitejs/plugin-react`, vitest, jsdom, `@testing-library/react`, `@testing-library/user-event`, playwright.
   - `sidecar/pyproject.toml` (uv project, Python 3.13): rapidocr, onnxruntime, open_clip_torch, torch (index
     `https://download.pytorch.org/whl/cu132` via `[tool.uv.sources]`), pillow, numpy, jsonschema; dev: pytest. Only the
     manifest here; `uv sync` happens in 3.1 after the download approval.
   Use `cargo add` / `npm install` so current versions resolve; confirm plugin init APIs with context7 before writing builder code.
5. Register every plugin in `src-tauri/src/lib.rs` builder (single-instance FIRST, per its docs) with no behaviour yet;
   capabilities file exactly as `contract-tauri-ipc.md` § Capabilities (incl. `windows: ["main", "tray-recent"]`); CSP as § Image URLs.
6. Transcribe contracts:
   - `crates/ss-core/src/contract/{mod.rs,ipc.rs,sidecar.rs,error.rs}`: every type in `contract-tauri-ipc.md` and
     `contract-sidecar-jsonrpc.md` with serde + `#[derive(TS)]` (no export attribute), integer overrides per rule 3, `CONTRACT_VERSION = 1`.
   - `ss-core` write APIs take `caller: Caller` (rule 8) from day one, even as stubs.
   - `contracts/sqlite/0001_init.sql` verbatim from `contract-sqlite-schema.md`.
   - `contracts/sidecar-jsonrpc.schema.json` + `contracts/fixtures/sidecar/<method>.request.json|.response.json` + `error.json`.
   - Command stubs in `src-tauri/src/commands/*.rs` for every command, returning `AppError { kind: internal, message: "not implemented" }`.
7. `crates/ss-core/tests/contract_test.rs`: (a) `export_all_to` a temp dir and assert byte-equal to committed
   `crates/ss-core/bindings/` (stale = FAIL with "run SS_WRITE_BINDINGS=1 cargo test -p ss-core --test contract_test and commit"),
   and no generated file contains `bigint`; (b) every sidecar
   fixture validates against the schema AND deserializes into the Rust types and re-serializes to equal JSON;
   (c) `0001_init.sql` applies to an in-memory DB and `PRAGMA table_xinfo(image)` lists exactly the contract columns;
   (d) `AppError` serializes to the envelope shape. Watch each sub-test go red once (break a field) before trusting it.
8. `src/__tests__/contract.test.ts`: imports every binding type, builds one sample of each command's `req`, and
   `mockIPC` round-trip asserts the `req` key wrapping (D4) for every command except those in `RAW_BODY_COMMANDS`. Wire `@bindings` alias in `vite.config.ts` + `tsconfig.json`.
9. UI skeleton only: `src/main.tsx`, `src/App.tsx` with the `aurora-glass` background so the window is not white.
10. `scripts/accept/lane-tests.mjs` (D11): takes test file paths; maps `crates/<crate>/tests/<name>.rs` -> `cargo test -p <crate> --test <name>`
    (requires `test result: ok. N passed` with N > 0 and `0 ignored`), `src/**/*.test.ts(x)` -> one `npx vitest run <files> --reporter=json`
    (requires `numTotalTests` > 0, `numPendingTests` == 0, `numTodoTests` == 0), `sidecar/tests/*.py` -> `<sidecar-venv>\Scripts\python.exe -m pytest <files> -rA`
    (venv from setting default; requires >= 1 passed and 0 skipped/deselected/xfailed). A missing file is a FAIL, never a skip. Prints
    `PASS|FAIL <file> <counts>` per file, exit 0 only if all pass. Break it once (point it at an empty test file) and watch it go red.
11. `docs/lessons.md` (project-local lessons file, owner rule). Commit on `main`. Create lane worktrees later from this commit.

## Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Contract transcription error | medium | high | contract_test (a)-(d), each seen red once |
| A later lane needs an undeclared dependency | medium | low | D3: lane stops and reports; Owner adds it in the next integration phase |
| Tauri scaffold defaults drift from plugin docs | low | medium | confirm builder/plugin APIs via context7 at write time |
