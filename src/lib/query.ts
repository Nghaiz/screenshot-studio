// ImageQuery construction and TanStack Query keys. Every list query is keyed by its ImageQuery (cursor = null).
import type { ImageQuery } from "@bindings/ImageQuery";
import type { SortSpec } from "@bindings/SortSpec";
import { PAGE_LIMIT } from "./defaults";

export type View = "library" | "timeline" | "trash";

/** The user-chosen part of a query (search bar, filter panel, sidebar); sort and paging come from elsewhere. */
export type FilterSpec = Omit<ImageQuery, "sort" | "cursor" | "limit" | "includeMissing">;

export const DEFAULT_SORT: SortSpec = { field: "taken", dir: "desc" };

/** A filter with no constraint (empty arrays and nulls mean "no constraint", contract § Shared types). */
export function emptyFilter(): FilterSpec {
  return {
    text: null, textMode: "any", dateFrom: null, dateTo: null, minWidth: null, maxWidth: null, minHeight: null,
    maxHeight: null, aspects: [], minSize: null, maxSize: null, folders: [], tagIds: [], tagMatch: "any",
    albumId: null, color: null, colorTolerance: 0, categories: [], favoriteOnly: false, minRating: 0, hasText: null,
    trash: "exclude",
  };
}

export function emptyQuery(): ImageQuery {
  return { ...emptyFilter(), includeMissing: false, sort: { ...DEFAULT_SORT }, cursor: null, limit: PAGE_LIMIT };
}

/** Takes the filter fields of a saved query (a smart filter stores a whole ImageQuery). */
export function filterOf(q: ImageQuery): FilterSpec {
  const { sort: _s, cursor: _c, limit: _l, includeMissing: _m, ...filter } = q;
  return { ...emptyFilter(), ...filter };
}

export interface BrowseState {
  view: View;
  filter: FilterSpec;
  sort: SortSpec;
}

/** Number of filter-panel constraints in effect (the search text and the sidebar folder are not counted). */
export function activeFilterCount(f: FilterSpec): number {
  const e = emptyFilter();
  const keys = Object.keys(e) as (keyof FilterSpec)[];
  const skip = new Set<keyof FilterSpec>(["text", "textMode", "folders", "tagMatch", "colorTolerance"]);
  return keys.filter((k) => !skip.has(k) && JSON.stringify(f[k]) !== JSON.stringify(e[k])).length;
}

export function buildQuery(s: BrowseState): ImageQuery {
  const text = s.filter.text?.trim() ? s.filter.text.trim() : null;
  const q: ImageQuery = { ...emptyQuery(), ...s.filter, text };
  if (s.view === "trash") q.trash = "only";
  // The timeline groups by date, so it always sorts by date taken, newest first. `relevance` exists only
  // while a text search is active (the backend refuses it otherwise), so without text it reads as the default.
  if (s.view === "timeline") q.sort = { ...DEFAULT_SORT };
  else if (s.sort.field === "relevance" && text === null) q.sort = { ...DEFAULT_SORT };
  else q.sort = s.sort;
  return q;
}

export const qk = {
  imagesAll: ["images"] as const,
  images: (q: ImageQuery) => ["images", q] as const,
  image: (id: number) => ["image", id] as const,
  timelineAll: ["timeline"] as const,
  timeline: (q: ImageQuery) => ["timeline", q] as const,
  appInfo: ["appInfo"] as const,
  counts: ["counts"] as const,
  stats: ["stats"] as const,
  tags: ["tags"] as const,
  albums: ["albums"] as const,
  smartFilters: ["smartFilters"] as const,
  journal: ["journal"] as const,
};
