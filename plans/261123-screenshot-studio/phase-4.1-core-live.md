# Phase 4.1: Core live features, export, edit backend (lane core)

Lane: core
Depends-on: 3.4
Steps: lane-decides
Effort: L
Owns: `crates/**`, `src-tauri/**` (except the frozen contract paths in README D2)
Creates: `crates/ss-core/tests/watcher_test.rs`, `crates/ss-core/tests/edit_test.rs`, `crates/ss-core/tests/export_test.rs`
Accept: `node scripts/accept/lane-tests.mjs crates/ss-core/tests/watcher_test.rs crates/ss-core/tests/edit_test.rs crates/ss-core/tests/export_test.rs`

## Goal

Rust side of P4: live watcher with toast, tray with recent items, global hotkey, autostart toggle, single-instance
"open image", ZIP/PDF export, edit save (copy/overwrite with backup + undo), convert/resize/compress, stitch, clipboard image.

## Scope

- Watcher: `notify-debouncer-full` (2 s) on every root while `watcherEnabled`; re-reads `library_root` every
  `WATCHER_ROOT_POLL_S = 10` so roots added by `ssctl` are picked up; new file -> wait until size stable,
  index + thumbnail it, emit `new-screenshot` + `library-changed`, OS toast via the notification plugin (click ->
  `open-image`). Renames/deletes from Explorer update rows (missing / moved).
- Tray (`tray-icon` feature): menu Open, Recent (last 8 via `get_recent`, opens image), Pause watcher, Quit; left-click
  toggles a small "recent" popup window created with `WebviewWindowBuilder` (label `tray-recent`, already in the capability `windows`; UI in 4.2).
- Global shortcut from setting `hotkey` (default Ctrl+Alt+S) shows/focuses the main window; re-registered on change;
  registration failure -> `AppInfo.warnings` `hotkeyUnavailable`, not fatal.
- Autostart: `set_setting("autostart", true|false)` calls the plugin enable/disable; default false (Deferred #23);
  tests check `is_enabled` only and restore the original state.
- Edit: `save_edited_image` raw-body command (PNG bytes from the UI) -> copy (`<name> (edited).<ext>`) or overwrite with
  backup under `<data>\edit-backups\` and journal `editOverwrite` (undo restores the backup); `convert_images`
  (overwrite uses the same backup + undo), `stitch_images` per contract; overwrite of a protected-root image allowed only through this journaled path.
- Export: `export_zip` (zip crate, stored PNGs), `export_pdf` (printpdf, a4|fit, 1/2/4 per page) as jobs.
- `ssctl` rows of phase 4.1 in `design-intent.md` (`edit`, `undo <batchId>`).
- Clipboard: `copy_image_to_clipboard` decodes to RGBA and calls the clipboard-manager plugin `write_image`.

## Tests

`watcher_test.rs`: dropping a file into a watched temp root yields a row + `new-screenshot` within 5 s; a partially
written file is not indexed until stable. `edit_test.rs`: overwrite + undo restores identical bytes (blake3); copy
creates a new row; stitch output dimensions equal the sum + gaps. `export_test.rs`: zip lists every file; PDF page count
matches `perPage` math.

## Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Overwrite loses the original | low | high | named before this phase: backup written and fsynced before overwrite; undo test |
| OneDrive sync events flood the watcher | medium | low | debouncer + ignore attribute-only changes |
| Hotkey already taken by another app | medium | low | report in settings, let the user rebind |
