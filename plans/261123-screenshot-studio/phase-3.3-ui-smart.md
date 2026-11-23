# Phase 3.3: UI smart features and degraded mode (lane ui)

Lane: ui
Depends-on: 2.3
Steps: lane-decides
Effort: M
Owns: `src/**`
Creates: `src/__tests__/smart.test.tsx`, `src/__tests__/degraded.test.tsx`
Accept: `node scripts/accept/lane-tests.mjs src/__tests__/smart.test.tsx src/__tests__/degraded.test.tsx`

## Goal

UI of feature group 4 plus the AI-dependent search controls, with an explicit degraded mode when the sidecar is absent.

## Scope

- Settings > AI: sidecar status (`SidecarStatus`), device, setup command shown verbatim (copy button), "Run AI on library"
  (`start_ai`), progress.
- Degraded mode: every AI control (semantic search mode, OCR text filter, has-text, categories, colour, auto-redact)
  disabled with a tooltip + one visible banner per session when `state != ready`.
- Duplicate finder (exact / similar slider, groups with "keep best", select-others -> in-app trash), cleanup wizard
  (duplicates, blurry, tiny, old in-app trash) that only ever proposes in-app trash or Recycle Bin, never permanent delete,
  sessions view (group headers in the timeline), stats dashboard (charts drawn with SVG, no chart library), category chips.

## Tests

`smart.test.tsx`: duplicate group renders best first; "trash others" calls `trash_images` with the non-best ids; wizard
never calls `request_permanent_delete`. `degraded.test.tsx`: with mocked `SidecarStatus.state = "absent"` the semantic
mode is disabled, the banner shows once, and filename search still works.

## Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Cleanup wizard deletes too much | low | high | named before this phase: wizard limited to reversible actions, tested |
| Dashboard heavy on 5,600 items | low | low | aggregates come from `get_stats`, not client-side |
