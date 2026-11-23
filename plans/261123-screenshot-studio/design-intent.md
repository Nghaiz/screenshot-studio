# Design intent (shared by every lane, pinned before fan-out)

## Names and paths
- Product name `Screenshot Studio`; identifier `dev.nghaiz.screenshotstudio`; binary `screenshot-studio.exe`; CLI `ssctl.exe`.
- Data dir `D:\IOT_Tools\data\ScreenshotStudio` (constant `DEFAULT_DATA_DIR` in `ss-core::paths`; env `SS_DATA_DIR` overrides, tests only).
- Install dir `D:\IOT_Tools\apps\ScreenshotStudio`. Source `D:\Screenshot_Picture`. HF models `D:\DevCache\huggingface`.
- Default library root offered on first run: `C:\Users\Nghaiz\OneDrive\Pictures\Screenshots` (subfolders `1ERD`, `4ERD` included), flagged `protectedRoot`.

## Look and motion
- Not a plain white frame: every theme has a layered background (gradient / mesh / subtle noise), glass or solid
  panels, an accent colour, and its own motion personality.
- Themes (11, ids are kebab-case): `aurora-glass` (default), `midnight-neon`, `cyberpunk`, `fluent-mica-light`,
  `fluent-mica-dark`, `sakura-pastel`, `sunset`, `ocean`, `forest`, `mono-minimal`, `high-contrast`.
- Theme engine: CSS custom properties only (`--ss-bg`, `--ss-surface`, `--ss-surface-blur`, `--ss-text`, `--ss-muted`,
  `--ss-accent`, `--ss-accent-contrast`, `--ss-border`, `--ss-radius`, `--ss-shadow`, `--ss-motion-scale`), one file per
  theme in `src/themes/`, a custom accent override, `data-theme` on `<html>`.
- Motion: `motion/react`; intensity setting `off|subtle|full` multiplies durations via `--ss-motion-scale`;
  `prefers-reduced-motion` forces `off`; animated background is a separate toggle (default on, auto-off on battery is NOT attempted).
- Density `compact|comfortable|spacious`. Languages `vi` (default) and `en`.
- Keyboard first: arrows/Home/End/PageUp/PageDown in grid, Enter opens lightbox, Space quick-look, F favourite,
  1-5 rating, Delete = in-app trash, Shift+Delete = Recycle Bin (with confirm), NO single-key permanent delete,
  Ctrl+K command palette, Ctrl+Z undo, Ctrl+F search.

## Safety
- Permanent delete: separate dialog, shows count + total size + first 6 thumbnails, user types `DELETE`, red button
  disabled until typed; never the default button; never batched with other actions.
- Never pin/unpin OneDrive files; never set file attributes. Background passes never open cloud-only files; an explicit
  user action on one image may (OneDrive downloads it on demand, as Explorer does).
- The Ctrl+K palette and every keyboard path can reach the trash view, never `request_permanent_delete`.
- Every destructive action except permanent delete appears in the undo history.

## Defaults
- Grid thumbnail 320 px, size slider 120-400 px; lightbox preview 1280 px; original for zoom > 100%.
- Session gap 30 min; dup Hamming 6; tiny = width*height < 40,000 px; blur threshold 60 (Laplacian variance).
- Hotkey Ctrl+Alt+S brings the window to front (does not take a screenshot).

## `ssctl` surface (agent-verifiable CLI over `ss-core`, pinned now, built incrementally by the core lane)

Every subcommand prints ONE JSON document on stdout with `--json`, logs on stderr, exit 0 ok / 1 error / 2 usage.
Global flag `--data-dir <path>` (else `SS_DATA_DIR`, else the default data dir).

| Subcommand | Phase |
|---|---|
| `ssctl add-root <path>` (prints `LibraryRoot`), `ssctl remove-root <id>`, `ssctl index [<rootId>] [--wait]` (prints `CatalogCounts` when done), `ssctl list [--since <ms>] [--limit N]` (prints `ImagePage`), `ssctl stats` (prints `CatalogCounts`), `ssctl extensions` (prints `string[]`), `ssctl get <key>` / `ssctl set <key> <json>` (settings) | 1.2 |
| `ssctl search <text> [--mode any|filename|ocr|semantic]` (prints `ImagePage`), `ssctl journal [--limit N]` (prints the `list_journal` result) | 2.1 |
| `ssctl ai --tasks ocr,embed,classify,color [--wait]`, `ssctl dupes --mode exact|similar` (prints the `find_duplicates` result), `ssctl status` (prints `SidecarStatus`) | 3.2 |
| `ssctl edit <id> --crop x,y,w,h --mode copy|overwrite` (prints `{ image: ImageSummary, batchId: number|null }`), `ssctl undo <batchId>` (batchId REQUIRED: the CLI never undoes "the latest" batch) | 4.1 |

`ssctl` never exposes permanent delete, move, rename or Recycle Bin (UI only). `edit`/`undo` run as `Caller::Cli`, so
any file write under a protected root or a `\OneDrive\` path is refused (contract-tauri-ipc rule 8: path-based, not row-based).
Output types are the contract types named above (camelCase JSON).
