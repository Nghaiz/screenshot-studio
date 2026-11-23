// Session view state (not persisted): current view, focus, selection, overlays, toasts, job progress.
import { create } from "zustand";
import type { JobProgress } from "@bindings/JobProgress";
import type { View } from "../lib/query";

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

export interface ViewState {
  view: View;
  folder: string | null;
  focusIndex: number;
  selection: ReadonlySet<number>;
  anchorIndex: number | null;
  lightbox: Lightbox;
  paletteOpen: boolean;
  settingsOpen: boolean;
  confirmRecycle: number[] | null;
  toasts: Toast[];
  jobs: Record<number, JobProgress>;
  /** Bumped to ask the gallery to scroll `focusIndex` into view (keyboard, timeline scrubber). */
  scrollRequest: number;

  setView: (view: View) => void;
  setFolder: (folder: string | null) => void;
  focus: (index: number, scroll?: boolean) => void;
  setSelection: (ids: Iterable<number>, anchorIndex?: number | null) => void;
  toggleSelected: (id: number) => void;
  clearSelection: () => void;
  openLightbox: (lb: Lightbox) => void;
  closeLightbox: () => void;
  setPalette: (open: boolean) => void;
  setSettings: (open: boolean) => void;
  askRecycle: (ids: number[] | null) => void;
  toast: (t: Omit<Toast, "id">) => number;
  dismissToast: (id: number) => void;
  jobProgress: (p: JobProgress) => void;
  clearJob: (jobId: number) => void;
}

let toastSeq = 0;

export const useView = create<ViewState>((set) => ({
  view: "library",
  folder: null,
  focusIndex: 0,
  selection: new Set(),
  anchorIndex: null,
  lightbox: { mode: "closed" },
  paletteOpen: false,
  settingsOpen: false,
  confirmRecycle: null,
  toasts: [],
  jobs: {},
  scrollRequest: 0,

  setView: (view) => set({ view, focusIndex: 0, selection: new Set(), anchorIndex: null, lightbox: { mode: "closed" } }),
  setFolder: (folder) => set({ folder, focusIndex: 0, selection: new Set(), anchorIndex: null }),
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
