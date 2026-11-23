// UI defaults from design-intent.md § Defaults / § Names and paths.

/** Offered on first run (design-intent.md § Names and paths); the backend flags it `protectedRoot`. */
export const DEFAULT_LIBRARY_ROOT = String.raw`C:\Users\Nghaiz\OneDrive\Pictures\Screenshots`;

export const THUMB_SIZE_DEFAULT = 320;
export const THUMB_SIZE_MIN = 120;
export const THUMB_SIZE_MAX = 400;
export const THUMB_SIZE_STEP = 40;

/** `ssimg` widths the backend serves (contract-tauri-ipc § Image URLs): anything else is a 400. */
export const GRID_THUMB_WIDTH = 320;
export const PREVIEW_WIDTH = 1280;

/** `query_images` page size (contract range 1..500). */
export const PAGE_LIMIT = 500;
/** Start fetching the next page when the visible range is this close to the loaded end. */
export const PREFETCH_MARGIN = 200;

export const LIST_ROW_HEIGHT = 56;
export const TIMELINE_HEADER_HEIGHT = 44;
/** Aspect ratio assumed for items whose page is not loaded yet or that have no dimensions. */
export const FALLBACK_ASPECT = 16 / 9;

export const SLIDESHOW_MS = 3000;
export const TOAST_MS = 5000;
