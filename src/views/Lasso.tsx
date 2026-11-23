// Lasso selection: press on the grid background and drag; every rendered cell the rectangle touches is
// selected (Ctrl/Shift adds to the current selection). Only rendered cells can be hit: the grid is virtualized.
import { useRef, useState, type PointerEvent, type RefObject } from "react";
import type { ImageSummary } from "@bindings/ImageSummary";
import { LASSO_MIN_PX } from "../lib/defaults";
import { useView } from "../state/view";

export interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}

const rectOf = (a: { x: number; y: number }, b: { x: number; y: number }): Rect => ({
  left: Math.min(a.x, b.x),
  top: Math.min(a.y, b.y),
  width: Math.abs(a.x - b.x),
  height: Math.abs(a.y - b.y),
});

const touches = (r: Rect, d: DOMRect) => d.right >= r.left && d.left <= r.left + r.width && d.bottom >= r.top && d.top <= r.top + r.height;

export function useLasso(scrollRef: RefObject<HTMLDivElement | null>, items: readonly ImageSummary[]) {
  const [rect, setRect] = useState<Rect | null>(null);
  const start = useRef<{ x: number; y: number; base: ReadonlySet<number> } | null>(null);

  const hits = (r: Rect): number[] => {
    const ids: number[] = [];
    for (const el of scrollRef.current?.querySelectorAll<HTMLElement>("[data-item]") ?? []) {
      const item = items[Number(el.dataset.index)];
      if (item && touches(r, el.getBoundingClientRect())) ids.push(item.id);
    }
    return ids;
  };

  const handlers = {
    onPointerDown: (e: PointerEvent<HTMLDivElement>) => {
      if (e.button !== 0 || (e.target as HTMLElement).closest("[data-item]")) return;
      const el = e.currentTarget;
      if (e.clientX - el.getBoundingClientRect().left >= el.clientWidth) return; // the scrollbar, not the background
      const additive = e.ctrlKey || e.shiftKey;
      start.current = { x: e.clientX, y: e.clientY, base: additive ? new Set(useView.getState().selection) : new Set() };
      e.currentTarget.setPointerCapture?.(e.pointerId);
    },
    onPointerMove: (e: PointerEvent<HTMLDivElement>) => {
      const s = start.current;
      if (!s) return;
      const r = rectOf(s, { x: e.clientX, y: e.clientY });
      if (r.width < LASSO_MIN_PX && r.height < LASSO_MIN_PX) return;
      setRect(r);
      useView.getState().setSelection([...s.base, ...hits(r)]);
    },
    onPointerUp: (e: PointerEvent<HTMLDivElement>) => {
      const s = start.current;
      start.current = null;
      e.currentTarget.releasePointerCapture?.(e.pointerId);
      if (!s) return;
      // A plain click on the background clears the selection.
      if (rect === null && s.base.size === 0) useView.getState().clearSelection();
      setRect(null);
    },
  };
  return { rect, handlers };
}

export function Lasso({ rect }: { rect: Rect | null }) {
  if (!rect) return null;
  return (
    <div
      aria-hidden
      className="pointer-events-none fixed z-30 rounded"
      style={{
        ...rect,
        border: "1px solid var(--ss-accent)",
        background: "color-mix(in srgb, var(--ss-accent) 18%, transparent)",
      }}
    />
  );
}
