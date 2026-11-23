// Masonry: fixed-width columns, natural heights. TanStack Virtual `lanes` places each item in the shortest
// column and virtualizes by offset, so only the visible band is in the DOM.
import { useEffect, useLayoutEffect } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { aspectOf, uniformGeometry } from "./layout";
import type { LayoutProps } from "./types";

export function MasonryGrid({ scrollRef, width, height, list, thumbSize, gap, navRef, renderCell }: LayoutProps) {
  const { items, total } = list;
  const { cols, cell } = uniformGeometry(total, width, thumbSize, gap);
  const v = useVirtualizer({
    count: total,
    lanes: cols,
    getScrollElement: () => scrollRef.current,
    estimateSize: (i) => cell / aspectOf(items[i]) + gap,
    overscan: 4,
    paddingStart: gap,
    paddingEnd: gap,
  });
  useEffect(() => v.measure(), [v, items, cell, cols]);

  const virtual = v.getVirtualItems();
  const last = virtual.length ? virtual[virtual.length - 1].index : 0;
  const { ensureLoaded } = list;
  useEffect(() => ensureLoaded(last), [ensureLoaded, last]);

  const perPage = Math.max(1, Math.floor(height / thumbSize)) * cols;
  useLayoutEffect(() => {
    navRef.current = {
      up: (i) => (i - cols >= 0 ? i - cols : i),
      down: (i) => (i + cols < total ? i + cols : i),
      pageUp: (i) => Math.max(0, i - perPage),
      pageDown: (i) => Math.min(total - 1, i + perPage),
      scrollTo: (i) => v.scrollToIndex(i, { align: "auto" }),
    };
  });

  return (
    <div style={{ height: v.getTotalSize(), position: "relative", margin: `0 ${gap}px` }}>
      {virtual.map((vi) =>
        renderCell(vi.index, {
          position: "absolute",
          top: 0,
          left: vi.lane * (cell + gap),
          width: cell,
          height: vi.size - gap,
          transform: `translateY(${vi.start}px)`,
        }),
      )}
    </div>
  );
}
