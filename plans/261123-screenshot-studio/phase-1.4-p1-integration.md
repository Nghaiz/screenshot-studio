# Phase 1.4: P1 integration — real backend, real library, 60 fps (serial, human-gated)

Lane: serial
Depends-on: 1.2, 1.3
Gate: human (approve the first index of the OneDrive Screenshots library; decide on "Download cloud-only files")
Effort: M
Creates: `scripts/accept/p1.mjs`, `scripts/accept/lib/cdp.mjs`, `scripts/accept/lib/run.mjs`
AX: AX-01 `p1.mjs` prints `PASS|FAIL <check> <value>` lines and exits 0 only if every check passed; it starts and stops the dev app itself
Accept: `node scripts/accept/p1.mjs`

## Goal

Merge the core and ui lane branches, run the app against the real backend, index the real library read-only, and
measure the P1 success checks from the design: indexed count == files on disk, grid scroll >= 55 fps p50 (target 60).

## Steps

1. `git merge --no-ff lane/core` then `lane/ui` into `main` (D7); resolve only integration glue; rerun `cargo test` and
   `bunx vitest run`.
2. Human step (approved at plan approval or now): add `C:\Users\Nghaiz\OneDrive\Pictures\Screenshots` in the app; index.
   Cloud-only files are counted from metadata; the owner decides whether to press "Download cloud-only files".
3. `scripts/accept/lib/run.mjs`: starts `cargo tauri dev` (or the built debug exe) with
   `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` and `SS_DATA_DIR` pointing at a temp dir for
   fixture checks, waits for the window, stops it at the end.
4. `scripts/accept/p1.mjs` checks (first: FAIL if a `screenshot-studio.exe` process is already running, D10):
   - `lane-tests.mjs` over every test file of phases 1.1-1.3 exits 0 (child process; no shell chaining).
   - Fixture run: 2,000 generated fixtures indexed -> `ssctl stats --json` count == 2,000 and thumbnails ok.
   - Real library (read-only, uses the real data dir): `ssctl stats --json` image count == recursive count of image
     files under the root (Node `fs.readdir` recursive, metadata only, no reads; extension list from `ssctl extensions`);
     FAIL if the real root is not registered or the disk count is 0. Measured 2026-11-23: 5,683 PNG (22 cloud-only).
   - FPS: Playwright `connectOverCDP('http://127.0.0.1:9222')`, scroll the grid top-to-bottom over 10 s with
     `requestAnimationFrame` timestamps; PASS when p50 frame time <= 18.18 ms (>= 55 fps) and p95 <= 33 ms; prints both.
   - Cold start: time from process start to first grid paint < 2 s after the first index (design metric), printed.
5. Record measured numbers in README Forecast `Re-score`.

## Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| WebView2 ignores the CDP env var in dev | low | medium | fall back to the built debug exe; both documented in run.mjs |
| Real count differs (non-PNG, hidden, temp files) | medium | low | count rule = the same extension list as `ss-core::scan`, shared via `ssctl` output of the extension list |
| fps below target | medium | medium | kill criterion in Forecast; profile before P2 |
