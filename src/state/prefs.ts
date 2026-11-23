// UI preferences, persisted in localStorage (D9). Backend settings live in the `setting` table, not here.
import { create } from "zustand";
import type { SortSpec } from "@bindings/SortSpec";
import type { SortField } from "@bindings/SortField";
import { readJson, writeJson } from "../lib/storage";
import { THUMB_SIZE_DEFAULT, THUMB_SIZE_MAX, THUMB_SIZE_MIN } from "../lib/defaults";
import { LANGUAGES, type Language } from "../i18n/languages";
import { DEFAULT_THEME, DENSITIES, isThemeId, MOTION_LEVELS, type Density, type MotionLevel, type ThemeId } from "../themes";

export const LAYOUTS = ["grid", "masonry", "justified", "list"] as const;
export type Layout = (typeof LAYOUTS)[number];
/** `relevance` needs a text query (2.2), so it is not offered here. */
export const SORT_FIELDS = ["taken", "name", "size", "width", "rating", "random"] as const satisfies readonly SortField[];

export type { Language };

export interface Prefs {
  theme: ThemeId;
  accent: string | null;
  motion: MotionLevel;
  animatedBg: boolean;
  density: Density;
  language: Language;
  layout: Layout;
  thumbSize: number;
  sort: SortSpec;
}

export const PREFS_KEY = "ss.prefs.v1";

export const DEFAULT_PREFS: Prefs = {
  theme: DEFAULT_THEME,
  accent: null,
  motion: "full",
  animatedBg: true,
  density: "comfortable",
  language: "vi",
  layout: "grid",
  thumbSize: THUMB_SIZE_DEFAULT,
  sort: { field: "taken", dir: "desc" },
};

const oneOf = <T extends string>(list: readonly T[], v: unknown, d: T): T =>
  typeof v === "string" && (list as readonly string[]).includes(v) ? (v as T) : d;

export const clampThumb = (n: number) => Math.min(THUMB_SIZE_MAX, Math.max(THUMB_SIZE_MIN, Math.round(n)));

/** Validates stored prefs field by field; an unknown or stale value falls back to that field's default. */
export function parsePrefs(raw: unknown): Prefs {
  const r = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const d = DEFAULT_PREFS;
  const sort = (r.sort && typeof r.sort === "object" ? r.sort : {}) as Record<string, unknown>;
  return {
    theme: isThemeId(r.theme) ? r.theme : d.theme,
    accent: typeof r.accent === "string" && /^#[0-9a-fA-F]{6}$/.test(r.accent) ? r.accent : null,
    motion: oneOf(MOTION_LEVELS, r.motion, d.motion),
    animatedBg: typeof r.animatedBg === "boolean" ? r.animatedBg : d.animatedBg,
    density: oneOf(DENSITIES, r.density, d.density),
    language: oneOf(LANGUAGES, r.language, d.language),
    layout: oneOf(LAYOUTS, r.layout, d.layout),
    thumbSize: typeof r.thumbSize === "number" ? clampThumb(r.thumbSize) : d.thumbSize,
    sort: { field: oneOf(SORT_FIELDS, sort.field, d.sort.field), dir: oneOf(["asc", "desc"] as const, sort.dir, d.sort.dir) },
  };
}

interface PrefsStore extends Prefs {
  set: (patch: Partial<Prefs>) => void;
  reset: () => void;
}

export const usePrefs = create<PrefsStore>((set) => ({
  ...parsePrefs(readJson(PREFS_KEY)),
  set: (patch) => set(patch),
  reset: () => set(DEFAULT_PREFS),
}));

const pickPrefs = (s: Prefs): Prefs => ({
  theme: s.theme, accent: s.accent, motion: s.motion, animatedBg: s.animatedBg, density: s.density,
  language: s.language, layout: s.layout, thumbSize: s.thumbSize, sort: s.sort,
});

usePrefs.subscribe((s) => writeJson(PREFS_KEY, pickPrefs(s)));
