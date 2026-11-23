# Phase 4.3: Final integration, reinstall, success metrics (serial, human-gated)

Lane: serial
Depends-on: 4.1, 4.2
Gate: human (approve re-installing the final build)
Effort: M
Creates: `scripts/accept/p4.mjs`, `docs/README.md`
AX: AX-01 `p4.mjs` prints `PASS|FAIL <check> <value>` lines and a final metrics table, exit 0 only when all pass
Accept: `node scripts/accept/p4.mjs`

## Goal

Merge the P4 lanes, rebuild and reinstall, and prove the P4 design checks (new screenshot appears live; edit round-trip
saves) plus every success metric of the design.

## Steps

1. Merge `lane/core`, `lane/ui` (D7); full suites; `cargo tauri build`; human-approved reinstall with the same `/D=` command.
2. `scripts/accept/p4.mjs` (first: FAIL if a `screenshot-studio.exe` process is already running; then it launches the INSTALLED
   exe with `SS_DATA_DIR=<temp>` and a temp fixture root added with `ssctl --data-dir <temp> add-root`; the owner's real catalog is
   only read; D10):
   - p1, p2, p3 Accept scripts run as children and pass (regression guard).
   - Live: Node `fs.copyFile` of a generated PNG into the watched fixture root; within 15 s (10 s root poll + 2 s debounce)
     `ssctl --data-dir <temp> list --since <t> --json` contains it.
   - Edit round-trip: `ssctl edit <id> --crop 0,0,200,200 --mode copy --json`, then `--mode overwrite`, then `ssctl undo <batchId from the overwrite> --json`
     (surface pinned in `design-intent.md`, implemented by 4.1); blake3 hashes match the expected before/after.
   - Metrics table: indexed count == files on disk (real root, read-only), cold start < 2 s, query < 200 ms (timed
     `ssctl search`), grid fps (CDP probe from p1), journal coverage (the 2.1 `fileops_test`/`delete_safety_test`
     re-run through lane-tests.mjs proves every file op journals and `purge` does not), Start Menu shortcut present.
3. Real-machine checks with `cua-driver`: Node copies a fixture PNG into the temp watched root while the installed app runs, then
   confirm the toast + live grid update on screen; Windows Search launch once more.
4. `docs/README.md`: how to build, install, run AI setup, where data lives, how to uninstall; `docs/lessons.md` updated.

## Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Reinstall wipes app data | low | high | data dir is outside the install dir (D:\IOT_Tools\data); p4 checks the catalog count before and after |
| Regression in earlier phases | medium | medium | p4 reruns p1-p3 Accepts |
