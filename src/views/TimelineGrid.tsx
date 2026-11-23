// Timeline: month header rows + uniform item rows, built from `get_timeline` buckets (sort = taken desc),
// with a date scrubber on the right that jumps to a month.
import { useEffect, useLayoutEffect, useMemo } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { TimelineBucket } from "@bindings/TimelineBucket";
import { TIMELINE_HEADER_HEIGHT } from "../lib/defaults";
import { moveRows, rowOf, timelineRows, uniformGeometry } from "./layout";
import type { LayoutProps } from "./types";

export const monthLabel = (ms: number, lang: string) =>
  new Date(ms).toLocaleDateString(lang, { month: "long", year: "numeric" });

interface Props extends LayoutProps {
  buckets: readonly TimelineBucket[];
  lang: string;
}

export function TimelineGrid({ scrollRef, width, height, list, thumbSize, gap, navRef, renderCell, buckets, lang }: Props) {
  const { cols, cell } = uniformGeometry(list.total, width, thumbSize, gap);
  const rows = useMemo(() => timelineRows(buckets, cols), [buckets, cols]);
  const v = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: (i) => (rows[i].kind === "header" ? TIMELINE_HEADER_HEIGHT : cell + gap),
    overscan: 3,
    paddingStart: gap,
    paddingEnd: gap,
  });
  useEffect(() => v.measure(), [v, rows, cell]);

  const virtual = v.getVirtualItems();
  const lastVisible = virtual.length ? rows[virtual[virtual.length - 1].index] : undefined;
  const lastIndex = lastVisible ? (lastVisible.kind === "header" ? lastVisible.firstIndex : lastVisible.end) : 0;
  const { ensureLoaded } = list;
  useEffect(() => ensureLoaded(lastIndex), [ensureLoaded, lastIndex]);

  const perPage = Math.max(1, Math.floor(height / (cell + gap)));
  useLayoutEffect(() => {
    navRef.current = {
      up: (i) => moveRows(rows, i, -1),
      down: (i) => moveRows(rows, i, 1),
      pageUp: (i) => moveRows(rows, i, -perPage),
      pageDown: (i) => moveRows(rows, i, perPage),
      scrollTo: (i) => {
        const r = rowOf(rows, i);
        if (r < 0) return;
        const prev = rows[r - 1];
        const first = prev && prev.kind === "header" && prev.firstIndex === i;
        v.scrollToIndex(first ? r - 1 : r, { align: first ? "start" : "auto" });
      },
    };
  });

  return (
    <div style={{ height: v.getTotalSize(), position: "relative", margin: `0 ${gap}px` }}>
      {virtual.map((vr) => {
        const row = rows[vr.index];
        const style = { transform: `translateY(${vr.start}px)` };
        if (row.kind === "header") {
          return (
            <h2 key={vr.key} className="absolute left-0 top-0 flex items-end gap-2 text-base font-semibold" style={{ ...style, height: TIMELINE_HEADER_HEIGHT - 8 }}>
              {monthLabel(row.startMs, lang)} <span className="ss-muted text-xs font-normal">{row.count}</span>
            </h2>
          );
        }
        const cells = [];
        for (let i = row.start; i < row.end; i++) cells.push(renderCell(i, { width: cell, height: cell, flex: "none" }));
        return (
          <div key={vr.key} role="row" className="absolute left-0 top-0 flex" style={{ ...style, height: cell, gap }}>
            {cells}
          </div>
        );
      })}
    </div>
  );
}
