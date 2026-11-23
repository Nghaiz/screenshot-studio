# Phase 1.3: UI shell, theme engine, browse views (lane ui)

Lane: ui
Depends-on: 1.1
Steps: lane-decides
Effort: L
Owns: `src/**`
Creates: `src/__tests__/browse.test.tsx`, `src/__tests__/themes.test.ts`, `src/mocks/ipc.ts`
Accept: `node scripts/accept/lane-tests.mjs src/__tests__/browse.test.tsx src/__tests__/themes.test.ts`

## Goal

The browsing experience of P1 against the generated bindings and a mock IPC layer (no Rust needed): app shell, theme
engine with 11 themes, virtualized grid that stays smooth at 10,000 mock items, lightbox, timeline, keyboard navigation,
Ctrl+K palette, vi/en. Follows `design-intent.md` exactly.

## Scope (lane decides the step order)

- `src/lib/ipc.ts`: typed wrappers over `invoke` for every command, `{ req }` wrapping (D4), `AppError` mapping;
  `src/lib/ssimg.ts` as the only URL builder; `src/lib/events.ts` typed `listen` wrappers.
- `src/mocks/ipc.ts`: `mockIPC` implementation returning deterministic data for 10,000 images (placeholder thumbnails
  as generated data URLs); toggled by `import.meta.env.VITE_MOCK_IPC` for `vite dev` in a browser and used by tests.
- Data layer: TanStack Query keyed by `ImageQuery`; `library-changed` invalidates; Zustand for selection/view state;
  UI prefs in `localStorage` (D9).
- Theme engine `src/themes/` (one file per theme id from `design-intent.md`, CSS variables only), custom accent,
  animated background component (toggle), motion intensity + reduced-motion, density.
- Views: grid (uniform), masonry, justified rows, list; thumbnail size slider; timeline with date scrubber
  (`get_timeline` buckets); lightbox (zoom/pan, filmstrip, slideshow, side-by-side compare of 2 selected, eyedropper
  via canvas pixel read on the 1280 preview), keyboard map from `design-intent.md`, Ctrl+K palette (commands list as data).
- Layout: sidebar (library roots, folders `1ERD`/`4ERD`, later tags/albums slots), top bar (search box placeholder for 2.2),
  status bar (index progress from `job-progress`). First-run screen offering the default root.
- i18n: `src/i18n/vi.ts`, `en.ts`, typed keys; a test asserts both have the same key set.

## Tests

`browse.test.tsx`: renders the grid with 10,000 mock items and asserts the DOM holds < 300 item nodes; arrow keys move
focus; Enter opens the lightbox; Delete calls `trash_images` (not any delete) via the mock; Ctrl+K opens the palette, and no palette
command calls `request_permanent_delete` (asserted over the whole command list).
`themes.test.ts`: every theme defines every `--ss-*` variable; vi/en key parity.

## Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Justified/masonry layout + virtualization jank | medium | medium | compute row layout in a pure function (tested), virtualize rows not items |
| Mock diverges from real backend | medium | medium | mock types come from `@bindings`; 1.4 runs the real backend |
| Blur/glass effects cost frames | medium | medium | `backdrop-filter` only on chrome panels, never per thumbnail; measured in 1.4 |
