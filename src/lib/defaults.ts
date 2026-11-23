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

/** Pause after the last key before the search text becomes part of the query. */
export const SEARCH_DEBOUNCE_MS = 250;
/** Pause after the last key before the bulk-rename dialog asks for a new preview. */
export const RENAME_PREVIEW_DEBOUNCE_MS = 200;
/** Colour given to a new tag until the user picks one. */
export const DEFAULT_TAG_COLOR = "#7C3AED";
/** Colour-filter tolerance (0..100) when the colour filter is first turned on. */
export const DEFAULT_COLOR_TOLERANCE = 20;
/** How many thumbnails the permanent delete dialog shows (design-intent § Safety). */
export const PURGE_PREVIEW_COUNT = 6;
/** In-app drag payload type (drag-out to the OS is phase 4.2). */
export const DRAG_MIME = "application/x-ss-image-ids";
/** A pointer drag shorter than this (px) on the grid background is a click, not a lasso. */
export const LASSO_MIN_PX = 4;
/** A grid cell must stay mounted this long before its thumbnail is fetched and decoded (see Cell.tsx). */
export const THUMB_SETTLE_MS = 150;
