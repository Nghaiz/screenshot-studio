# Phase 2.2: UI search, filters, organize, delete dialogs (lane ui)

Lane: ui
Depends-on: 1.4
Steps: lane-decides
Effort: L
Owns: `src/**`
Creates: `src/__tests__/organize.test.tsx`, `src/__tests__/delete-dialogs.test.tsx`
Accept: `node scripts/accept/lane-tests.mjs src/__tests__/organize.test.tsx src/__tests__/delete-dialogs.test.tsx`

## Goal

All search/filter and organize UI of feature groups 2 and 3, against the 2.1 commands (mocked in tests; the real
backend lands in 2.3).

## Scope

- Search bar (text mode any/filename/ocr/semantic; semantic disabled until `SidecarStatus` has `embed`), filter panel
  (date range, resolution, aspect phone/PC, size, folder, tag, colour, category, favourite, has-text, trash), sort menu,
  saved smart filters in the sidebar.
- Organize: coloured tags (create/edit/assign, tag chips), virtual albums (create, add, drag to reorder), real folders
  (move/copy via dialog folder picker and drag-drop onto sidebar folders), stars, favourite, notes; lasso + Shift/Ctrl
  selection; bulk rename dialog with live `preview_rename` and conflict highlighting.
- Undo: Ctrl+Z, toast with "Undo" after every op, history panel (`list_journal`).
- Confirm dialogs for the non-undoable catalog deletions (remove root, delete tag/album/smart filter), naming what is lost (rule 7).
- Trash view (in-app trash, restore, "Move to Recycle Bin"), and the permanent delete dialog exactly per
  `design-intent.md` § Safety: separate dialog, count + size + 6 thumbnails, typed `DELETE`, red button disabled until
  typed, never focused by default, calls `request_permanent_delete` then `delete_permanently`.

## Tests

`organize.test.tsx`: filter panel builds the exact `ImageQuery` (snapshot of the `req`); tag assign calls `set_tags`;
bulk rename shows conflicts; Ctrl+Z calls `undo`. `delete-dialogs.test.tsx`: Delete key -> `trash_images` only;
permanent delete button disabled until `DELETE` typed; Enter does not trigger it; cancel sends nothing; the
`delete_permanently` call carries the token from `request_permanent_delete`.

## Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| A keyboard path reaches permanent delete | low | high | named before this phase: dialog tests above; no shortcut binds it |
| Drag-drop from OS vs in-app conflicts | medium | low | in-app drag only in 2.2; drag-out to OS is 4.2 |
