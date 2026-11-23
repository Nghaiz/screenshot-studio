// Justified rows (Google-Photos style): row layout from the pure `justifiedRows`, rows virtualized.
import { useEffect, useLayoutEffect, useMemo } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { aspectOf, justifiedRows, moveRows, rowOf } from "./layout";
import type { LayoutProps } from "./types";

export function JustifiedGrid({ scrollRef, width, height, list, thumbSize, gap, navRef, renderCell }: LayoutProps) {
  const { items, total } = list;
  const rows = useMemo(() => justifiedRows(total, (i) => aspectOf(items[i]), width, thumbSize, gap), [total, items, width, thumbSize, gap]);
  const v = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: (i) => rows[i].height + gap,
    overscan: 3,
    paddingStart: gap,
    paddingEnd: gap,
  });
  useEffect(() => v.measure(), [v, rows]);

  const virtual = v.getVirtualItems();
  const lastEnd = virtual.length ? rows[virtual[virtual.length - 1].index].end : 0;
  const { ensureLoaded } = list;
  useEffect(() => ensureLoaded(lastEnd), [ensureLoaded, lastEnd]);

  const perPage = Math.max(1, Math.floor(height / (thumbSize + gap)));
  useLayoutEffect(() => {
    navRef.current = {
      up: (i) => moveRows(rows, i, -1),
      down: (i) => moveRows(rows, i, 1),
      pageUp: (i) => moveRows(rows, i, -perPage),
      pageDown: (i) => moveRows(rows, i, perPage),
      scrollTo: (i) => {
        const r = rowOf(rows, i);
        if (r >= 0) v.scrollToIndex(r, { align: "auto" });
      },
    };
  });

  return (
    <div style={{ height: v.getTotalSize(), position: "relative", margin: `0 ${gap}px` }}>
      {virtual.map((vr) => {
        const row = rows[vr.index];
        const cells = [];
        for (let i = row.start; i < row.end; i++) {
          cells.push(renderCell(i, { width: row.widths[i - row.start], height: row.height, flex: "none" }));
        }
        return (
          <div key={vr.key} role="row" className="absolute left-0 top-0 flex" style={{ transform: `translateY(${vr.start}px)`, gap }}>
            {cells}
          </div>
        );
      })}
    </div>
  );
}
