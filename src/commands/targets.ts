import type { ImageSummary } from "@bindings/ImageSummary";
import { useView } from "../state/view";

/** Images an action applies to: the selection when there is one, else the focused image. */
export function targetIds(items: readonly ImageSummary[]): number[] {
  const { selection, focusIndex } = useView.getState();
  if (selection.size > 0) return [...selection];
  const focused = items[focusIndex];
  return focused ? [focused.id] : [];
}

/** Loaded ids between two indices (inclusive), for Shift+click / Shift+arrow range selection. */
export function rangeIds(items: readonly ImageSummary[], a: number, b: number): number[] {
  const ids: number[] = [];
  for (let i = Math.min(a, b); i <= Math.max(a, b); i++) {
    const it = items[i];
    if (it) ids.push(it.id);
  }
  return ids;
}
