# Phase 4.2: UI editor, share/export, tray popup, settings, polish (lane ui)

Lane: ui
Depends-on: 3.4
Steps: lane-decides
Effort: L
Owns: `src/**`
Creates: `src/__tests__/editor.test.tsx`, `src/__tests__/share.test.tsx`
Accept: `node scripts/accept/lane-tests.mjs src/__tests__/editor.test.tsx src/__tests__/share.test.tsx`

## Goal

Feature groups 5, 6, 7 (UI) and 8 polish: the quick editor, share/export, tray recent popup, settings, and the final
motion/theme polish.

## Scope

- Editor (Canvas 2D, Deferred #21): crop (free + presets 16:9, 9:16, 1:1), rotate/flip, annotate (arrow, box, text,
  highlight, numbered steps), blur/pixelate regions, auto-redact (boxes from `detect_pii`, disabled when the sidecar
  lacks `pii`), resize/compress/convert dialog (`convert_images`), stitch (`stitch_images`), undo/redo inside the editor,
  save copy / overwrite (overwrite asks once, explains the backup) via the raw-body `save_edited_image`.
- Share: copy image (`copy_image_to_clipboard`), copy OCR text (clipboard `writeText`), drag-out to Explorer (`@crabnebula/tauri-plugin-drag`
  `startDrag` with the original path, declared in 1.1), ZIP/PDF export dialogs with job progress, reveal in Explorer (`revealItemInDir`), open with (`openPath`).
- Live: toast click and `open-image` event open the lightbox; "new" badge on fresh items; tray popup window route
  `#/tray-recent` (compact list of recent items).
- Settings page: library roots, watcher, hotkey rebind, autostart toggle, AI (from 3.3), appearance (theme gallery with
  live preview, accent, animated background, motion intensity, density, language), data dir shown read-only.
- Polish: transitions between views, skeleton shimmers, empty states per theme, reduced-motion audit.

## Tests

`editor.test.tsx`: crop + arrow + blur produce a PNG blob of the expected size and `save_edited_image` receives raw bytes
with the right headers; overwrite path shows the backup notice. `share.test.tsx`: copy OCR text calls `writeText` with the
OCR string; export dialog sends `export_zip` with the selection.

## Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Large images slow the canvas | medium | medium | edit on a downscaled view, render the final at full size once on save |
| Raw IPC body API differs from memory | medium | low | confirm `invoke` raw body + headers via context7 before coding |
