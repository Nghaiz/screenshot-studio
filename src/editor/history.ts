// Undo/redo inside the editor (separate from the catalog journal: nothing is written until Save).
import { useCallback, useState } from "react";

/** Steps kept; older ones are dropped. */
export const EDITOR_HISTORY_LIMIT = 100;

export interface History<T> {
  present: T;
  canUndo: boolean;
  canRedo: boolean;
  push: (next: T) => void;
  undo: () => void;
  redo: () => void;
  reset: (value: T) => void;
}

interface Stacks<T> {
  past: T[];
  present: T;
  future: T[];
}

export function useHistory<T>(initial: T): History<T> {
  const [s, set] = useState<Stacks<T>>({ past: [], present: initial, future: [] });
  const push = useCallback(
    (next: T) => set((h) => ({ past: [...h.past, h.present].slice(-EDITOR_HISTORY_LIMIT), present: next, future: [] })),
    [],
  );
  const undo = useCallback(
    () => set((h) => (h.past.length === 0 ? h : { past: h.past.slice(0, -1), present: h.past[h.past.length - 1], future: [h.present, ...h.future] })),
    [],
  );
  const redo = useCallback(
    () => set((h) => (h.future.length === 0 ? h : { past: [...h.past, h.present], present: h.future[0], future: h.future.slice(1) })),
    [],
  );
  const reset = useCallback((value: T) => set({ past: [], present: value, future: [] }), []);
  return { present: s.present, canUndo: s.past.length > 0, canRedo: s.future.length > 0, push, undo, redo, reset };
}
