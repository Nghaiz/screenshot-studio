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
/** The gallery counts as scrolling until this long after its last scroll event (views/useScrollingFlag.ts). */
export const SCROLL_IDLE_MS = 150;

// ---- phase 3.3: AI sidecar, smart tools ----------------------------------------------------------

/** `sidecarVenv: null` means this venv (ss-core `sidecar::config::DEFAULT_SIDECAR_VENV`). */
export const DEFAULT_SIDECAR_VENV = String.raw`D:\IOT_Tools\data\ScreenshotStudio\sidecar-venv`;
/** Model cache the app hands the sidecar when HF_HOME is unset (ss-core `DEFAULT_HF_HOME`, design-intent). */
export const DEFAULT_HF_HOME = String.raw`D:\DevCache\huggingface`;
/** Source checkout holding `sidecar/` (design-intent § Names and paths). */
export const SOURCE_DIR = String.raw`D:\Screenshot_Picture`;
/**
 * The degraded-mode banner appears only after the sidecar has stayed `absent`/`error` this long, so a passing
 * state (the supervisor's initial "not started" before launch) never flashes it. `starting` never shows it.
 */
export const AI_NOTICE_SETTLE_MS = 1000;
/** Similar-duplicate Hamming distance (design-intent § Defaults, setting `dupHamming`). */
export const DUP_HAMMING_DEFAULT = 6;
/** Slider range of the similar mode; the backend accepts 0..64, past ~20 dHash groups unrelated screenshots. */
export const DUP_HAMMING_MAX = 20;
/** Pause after the last slider move before `find_duplicates` runs again. */
export const DUP_SLIDER_DEBOUNCE_MS = 250;
/** Thumbnail edge in the duplicate finder and the cleanup previews, px (the 320 thumbnail, shown smaller). */
export const SMART_THUMB_PX = 150;
/** In-app trash older than this is offered to the Recycle Bin by the cleanup wizard. */
export const OLD_TRASH_DAYS = 30;
/** Thumbnails shown per cleanup step. */
export const WIZARD_PREVIEW_COUNT = 8;
export const DAY_MS = 86_400_000;
