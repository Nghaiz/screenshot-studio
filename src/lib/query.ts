// ImageQuery construction and TanStack Query keys. Every list query is keyed by its ImageQuery (cursor = null).
import type { ImageQuery } from "@bindings/ImageQuery";
import type { SortSpec } from "@bindings/SortSpec";
import { PAGE_LIMIT } from "./defaults";

export type View = "library" | "timeline" | "trash";

/** A query with no constraint (empty arrays and nulls mean "no constraint", contract § Shared types). */
export function emptyQuery(): ImageQuery {
  return {
    text: null, textMode: "any", dateFrom: null, dateTo: null, minWidth: null, maxWidth: null, minHeight: null,
    maxHeight: null, aspects: [], minSize: null, maxSize: null, folders: [], tagIds: [], tagMatch: "any",
    albumId: null, color: null, colorTolerance: 0, categories: [], favoriteOnly: false, minRating: 0, hasText: null,
    trash: "exclude", includeMissing: false, sort: { field: "taken", dir: "desc" }, cursor: null, limit: PAGE_LIMIT,
  };
}

export interface BrowseState {
  view: View;
  folder: string | null;
  sort: SortSpec;
}

export function buildQuery(s: BrowseState): ImageQuery {
  const q = emptyQuery();
  q.trash = s.view === "trash" ? "only" : "exclude";
  q.folders = s.folder === null ? [] : [s.folder];
  // The timeline groups by date, so it always sorts by date taken, newest first.
  q.sort = s.view === "timeline" ? { field: "taken", dir: "desc" } : s.sort;
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
};
