// Grid keyboard map (design-intent.md § Look and motion, "Keyboard first"). Delete = in-app trash,
// Shift+Delete = Recycle Bin behind a confirm dialog; NO key reaches permanent delete.
import type { KeyboardEvent } from "react";
import { askRecycle, setFavorite, setRating, trashImages } from "../actions/images";
import type { ImageList } from "../data/images";
import { useView } from "../state/view";
import type { Nav } from "../views/types";
import { rangeIds, targetIds } from "./targets";

const MOVES: Record<string, (nav: Nav, i: number, last: number) => number> = {
  ArrowLeft: (_n, i) => i - 1,
  ArrowRight: (_n, i) => i + 1,
  ArrowUp: (n, i) => n.up(i),
  ArrowDown: (n, i) => n.down(i),
  PageUp: (n, i) => n.pageUp(i),
  PageDown: (n, i) => n.pageDown(i),
  Home: () => 0,
  End: (_n, _i, last) => last,
};

/** Returns true when the key was handled (the caller then prevents the default). */
export function handleGridKey(e: KeyboardEvent, list: ImageList, nav: Nav | null): boolean {
  const view = useView.getState();
  const { items, total } = list;
  if (total === 0) return false;
  const i = Math.min(view.focusIndex, total - 1);

  const move = MOVES[e.key];
  if (move && nav && !e.ctrlKey && !e.altKey) {
    const next = Math.max(0, Math.min(total - 1, move(nav, i, total - 1)));
    if (e.shiftKey) {
      const anchor = view.anchorIndex ?? i;
      view.setSelection(rangeIds(items, anchor, next), anchor);
    }
    view.focus(next);
    return true;
  }

  const ids = () => targetIds(items);
  switch (e.key) {
    case "Enter":
      view.openLightbox({ mode: "view", index: i });
      return true;
    case " ":
      view.openLightbox({ mode: "quick", index: i });
      return true;
    case "Escape":
      if (view.selection.size === 0) return false;
      view.clearSelection();
      return true;
    case "Delete":
      if (e.shiftKey) askRecycle(ids());
      else void trashImages(ids());
      return true;
  }

  if (e.ctrlKey && (e.key === "a" || e.key === "A")) {
    view.setSelection(items.map((it) => it.id), 0);
    return true;
  }
  if (e.ctrlKey || e.altKey || e.metaKey) return false;

  if (e.key === "f" || e.key === "F") {
    const target = ids();
    const favorites = new Set(items.filter((it) => it.favorite).map((it) => it.id));
    const all = target.every((id) => favorites.has(id));
    void setFavorite(target, !all);
    return true;
  }
  if (/^[0-5]$/.test(e.key)) {
    void setRating(ids(), Number(e.key));
    return true;
  }
  if (e.key === "c" || e.key === "C") return compareSelected();
  return false;
}

/** Opens the side-by-side compare when exactly two images are selected. */
export function compareSelected(): boolean {
  const view = useView.getState();
  const sel = [...view.selection];
  if (sel.length !== 2) return false;
  view.openLightbox({ mode: "compare", ids: [sel[0], sel[1]] });
  return true;
}
