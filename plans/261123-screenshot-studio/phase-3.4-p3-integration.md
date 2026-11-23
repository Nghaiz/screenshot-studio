# Phase 3.4: P3 integration — OCR hit, planted duplicate, sidecar absent (serial)

Lane: serial
Depends-on: 3.1, 3.2, 3.3
Effort: M
Creates: `scripts/accept/p3.mjs`
Owns: `src-tauri/tauri.conf.json` (`bundle.resources`)
AX: AX-01 `p3.mjs` prints `PASS|FAIL <check> <value>` lines, exit 0 only when all pass
Accept: `node scripts/accept/p3.mjs`

## Goal

Merge the ai, core and ui lanes and prove the P3 design checks: OCR search hits known text, the duplicate finder finds a
planted copy, and the app works with the sidecar absent.

## Steps

1. Merge `lane/ai`, `lane/core`, `lane/ui` (D7); full test suites. Bundle `sidecar/` as a Tauri resource (D6/#12) in
   `tauri.conf.json` `bundle.resources`.
2. `scripts/accept/p3.mjs` (all on a generated fixture library in a temp `SS_DATA_DIR`, D10):
   - `lane-tests.mjs` over every test file of phases 1.1-3.3 exits 0.
   - Fixture library: 50 generated images, one with the rendered text `Hoá đơn 2026 INVOICE-7731`, one exact copy of
     image #7 and one re-encoded JPEG of image #9 planted under another name.
   - `ssctl index`, then `ssctl ai --tasks ocr,embed --json` (pinned in the `ssctl` surface of `design-intent.md`, implemented by 3.2).
   - `ssctl search "INVOICE-7731" --json` returns the invoice image; `ssctl search "hoa don" --json` (no diacritics) also returns it.
   - `ssctl dupes --mode exact --json` groups #7 with its copy; `--mode similar` groups #9 with its JPEG.
   - Sidecar absent: `ssctl set sidecarVenv "\"<temp>/no-venv\""` in the temp data dir -> `ssctl status --json` reports `absent`,
     `ssctl search "INVOICE"` still returns the filename/FTS results without error, exit 0.
3. Real library AI pass (owner approved the downloads in Human steps): run "Run AI on library" in the app; record time and
   GPU/CPU in Forecast Re-score.

## Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| OCR misses the planted text | medium | medium | text rendered large and high-contrast; failure goes back to lane ai |
| Real library AI pass slow on CPU | medium | low | runs in background, resumable; not part of Accept |
