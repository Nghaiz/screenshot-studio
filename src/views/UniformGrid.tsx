// Uniform square grid; the virtualizer works on rows of `cols` cells.
import { useEffect, useLayoutEffect } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { uniformGeometry } from "./layout";
import type { LayoutProps } from "./types";

export function UniformGrid({ scrollRef, width, height, list, thumbSize, gap, navRef, renderCell }: LayoutProps) {
  const g = uniformGeometry(list.total, width, thumbSize, gap);
  const rowH = g.cell + gap;
  const v = useVirtualizer({
    count: g.rows,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowH,
    overscan: 3,
    paddingStart: gap,
    paddingEnd: gap,
  });
  useEffect(() => v.measure(), [v, rowH]);

  const rows = v.getVirtualItems();
  const lastRow = rows.length ? rows[rows.length - 1].index : 0;
  const { ensureLoaded } = list;
  useEffect(() => ensureLoaded((lastRow + 1) * g.cols), [ensureLoaded, lastRow, g.cols]);

  const perPage = Math.max(1, Math.floor(height / rowH)) * g.cols;
  useLayoutEffect(() => {
    navRef.current = {
      up: (i) => (i - g.cols >= 0 ? i - g.cols : i),
      down: (i) => (i + g.cols < list.total ? i + g.cols : i),
      pageUp: (i) => Math.max(0, i - perPage),
      pageDown: (i) => Math.min(list.total - 1, i + perPage),
      scrollTo: (i) => v.scrollToIndex(Math.floor(i / g.cols), { align: "auto" }),
    };
  });

  return (
    <div className="ss-square" style={{ height: v.getTotalSize(), position: "relative", margin: `0 ${gap}px` }}>
      {rows.map((r) => {
        const start = r.index * g.cols;
        const end = Math.min(start + g.cols, list.total);
        const cells = [];
        for (let i = start; i < end; i++) cells.push(renderCell(i, { width: g.cell, height: g.cell, flex: "none" }));
        return (
          <div
            key={r.key}
            role="row"
            className="absolute left-0 top-0 flex"
            style={{ transform: `translateY(${r.start}px)`, height: g.cell, gap }}
          >
            {cells}
          </div>
        );
      })}
    </div>
  );
}
