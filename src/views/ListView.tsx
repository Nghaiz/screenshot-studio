// Detail list: one row per image (thumbnail, name, date, size, folder).
import { useEffect, useLayoutEffect } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { LIST_ROW_HEIGHT } from "../lib/defaults";
import type { LayoutProps } from "./types";

export function ListView({ scrollRef, height, list, gap, navRef, renderCell }: LayoutProps) {
  const { total } = list;
  const v = useVirtualizer({
    count: total,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => LIST_ROW_HEIGHT,
    overscan: 8,
    paddingStart: gap,
    paddingEnd: gap,
  });
  const virtual = v.getVirtualItems();
  const last = virtual.length ? virtual[virtual.length - 1].index : 0;
  const { ensureLoaded } = list;
  useEffect(() => ensureLoaded(last), [ensureLoaded, last]);

  const perPage = Math.max(1, Math.floor(height / LIST_ROW_HEIGHT));
  useLayoutEffect(() => {
    navRef.current = {
      up: (i) => Math.max(0, i - 1),
      down: (i) => Math.min(total - 1, i + 1),
      pageUp: (i) => Math.max(0, i - perPage),
      pageDown: (i) => Math.min(total - 1, i + perPage),
      scrollTo: (i) => v.scrollToIndex(i, { align: "auto" }),
    };
  });

  return (
    <div style={{ height: v.getTotalSize(), position: "relative", margin: `0 ${gap}px` }}>
      {virtual.map((vi) =>
        renderCell(
          vi.index,
          { position: "absolute", top: 0, left: 0, right: 0, height: LIST_ROW_HEIGHT - 4, transform: `translateY(${vi.start}px)` },
          "row",
        ),
      )}
    </div>
  );
}
