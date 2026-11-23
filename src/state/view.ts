// Session view state (not persisted): current view, filter, focus, selection, overlays, toasts, job progress.
import { create } from "zustand";
import type { JobProgress } from "@bindings/JobProgress";
import type { MoveMode } from "@bindings/MoveMode";
import { emptyFilter, type FilterSpec, type View } from "../lib/query";

export type Lightbox =
  | { mode: "closed" }
  | { mode: "view" | "quick"; index: number }
  | { mode: "compare"; ids: [number, number] };

export interface Toast {
  id: number;
  text: string;
  tone: "info" | "error";
  undoBatchId: number | null;
}

/** A catalog-only deletion (not journaled, contract rule 7): the dialog names what is lost before `run`. */
export interface ConfirmRequest {
  title: string;
  body: string;
  confirmLabel: string;
  run: () => Promise<void>;
}

/** One modal at a time. Permanent delete has its own kind and is opened only by a pointer click. */
export type Dialog =
  | { kind: "tags"; ids: number[] }
  | { kind: "tagEdit"; tagId: number | null }
  | { kind: "album"; ids: number[] }
  | { kind: "albumEdit"; albumId: number | null }
  | { kind: "move"; ids: number[]; mode: MoveMode; destDir: string | null }
  | { kind: "rename"; ids: number[] }
  | { kind: "saveFilter" }
  | { kind: "confirm"; request: ConfirmRequest }
  | { kind: "purge"; ids: number[] };

export type SidePanel = "none" | "details" | "history";

export interface ViewState {
  view: View;
  filter: FilterSpec;
  filtersOpen: boolean;
  focusIndex: number;
  selection: ReadonlySet<number>;
  anchorIndex: number | null;
  lightbox: Lightbox;
  paletteOpen: boolean;
  settingsOpen: boolean;
  sidePanel: SidePanel;
  dialog: Dialog | null;
  confirmRecycle: number[] | null;
  toasts: Toast[];
  jobs: Record<number, JobProgress>;
  /** Bumped to ask the gallery to scroll `focusIndex` into view (keyboard, timeline scrubber). */
  scrollRequest: number;

  setView: (view: View) => void;
  setFolder: (folder: string | null) => void;
  setFilter: (patch: Partial<FilterSpec>) => void;
  replaceFilter: (filter: FilterSpec) => void;
  resetFilter: () => void;
  setFiltersOpen: (open: boolean) => void;
  focus: (index: number, scroll?: boolean) => void;
  setSelection: (ids: Iterable<number>, anchorIndex?: number | null) => void;
  toggleSelected: (id: number) => void;
  clearSelection: () => void;
  openLightbox: (lb: Lightbox) => void;
  closeLightbox: () => void;
  setPalette: (open: boolean) => void;
  setSettings: (open: boolean) => void;
  setSidePanel: (panel: SidePanel) => void;
  openDialog: (dialog: Dialog) => void;
  closeDialog: () => void;
  askRecycle: (ids: number[] | null) => void;
  toast: (t: Omit<Toast, "id">) => number;
  dismissToast: (id: number) => void;
  jobProgress: (p: JobProgress) => void;
  clearJob: (jobId: number) => void;
}

let toastSeq = 0;

/** A new result set: focus the first item, drop the selection. */
const fresh = { focusIndex: 0, anchorIndex: null };

export const useView = create<ViewState>((set) => ({
  view: "library",
  filter: emptyFilter(),
  filtersOpen: false,
  focusIndex: 0,
  selection: new Set(),
  anchorIndex: null,
  lightbox: { mode: "closed" },
  paletteOpen: false,
  settingsOpen: false,
  sidePanel: "none",
  dialog: null,
  confirmRecycle: null,
  toasts: [],
  jobs: {},
  scrollRequest: 0,

  setView: (view) => set({ view, ...fresh, selection: new Set(), lightbox: { mode: "closed" } }),
  setFolder: (folder) => set((s) => ({ filter: { ...s.filter, folders: folder === null ? [] : [folder] }, ...fresh, selection: new Set() })),
  setFilter: (patch) => set((s) => ({ filter: { ...s.filter, ...patch }, ...fresh, selection: new Set() })),
  replaceFilter: (filter) => set({ filter, ...fresh, selection: new Set() }),
  resetFilter: () => set({ filter: emptyFilter(), ...fresh, selection: new Set() }),
  setFiltersOpen: (filtersOpen) => set({ filtersOpen }),
  focus: (index, scroll = true) =>
    set((s) => ({ focusIndex: Math.max(0, index), scrollRequest: scroll ? s.scrollRequest + 1 : s.scrollRequest })),
  setSelection: (ids, anchorIndex) =>
    set((s) => ({ selection: new Set(ids), anchorIndex: anchorIndex === undefined ? s.anchorIndex : anchorIndex })),
  toggleSelected: (id) =>
    set((s) => {
      const next = new Set(s.selection);
      if (!next.delete(id)) next.add(id);
      return { selection: next };
    }),
  clearSelection: () => set({ selection: new Set(), anchorIndex: null }),
  openLightbox: (lightbox) => set({ lightbox, paletteOpen: false }),
  closeLightbox: () => set({ lightbox: { mode: "closed" } }),
  setPalette: (paletteOpen) => set({ paletteOpen }),
  setSettings: (settingsOpen) => set({ settingsOpen }),
  setSidePanel: (sidePanel) => set({ sidePanel }),
  openDialog: (dialog) => set({ dialog, paletteOpen: false }),
  closeDialog: () => set({ dialog: null }),
  askRecycle: (confirmRecycle) => set({ confirmRecycle }),
  toast: (t) => {
    const id = ++toastSeq;
    set((s) => ({ toasts: [...s.toasts.slice(-3), { ...t, id }] }));
    return id;
  },
  dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
  jobProgress: (p) => set((s) => ({ jobs: { ...s.jobs, [p.jobId]: p } })),
  clearJob: (jobId) =>
    set((s) => {
      const jobs = { ...s.jobs };
      delete jobs[jobId];
      return { jobs };
    }),
}));
