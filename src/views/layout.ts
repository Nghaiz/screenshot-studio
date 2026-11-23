// Pure layout math (no DOM), so it is unit-tested and the views virtualize ROWS, not items (phase 1.3 risks).
import type { ImageSummary } from "@bindings/ImageSummary";
import type { TimelineBucket } from "@bindings/TimelineBucket";
import { FALLBACK_ASPECT } from "../lib/defaults";

/** A contiguous run of item indices `[start, end)` laid out as one row. */
export interface Row {
  start: number;
  end: number;
}

export function aspectOf(item: ImageSummary | undefined): number {
  if (!item || !item.width || !item.height) return FALLBACK_ASPECT;
  return item.width / item.height;
}

// ---- uniform grid -----------------------------------------------------------------------------

export interface UniformGeometry {
  cols: number;
  /** Cell edge in px (cells stretch so the row fills the width exactly). */
  cell: number;
  rows: number;
}

export function uniformGeometry(count: number, width: number, thumb: number, gap: number): UniformGeometry {
  const cols = Math.max(1, Math.floor((width + gap) / (thumb + gap)));
  const cell = Math.max(1, (width - gap * (cols - 1)) / cols);
  return { cols, cell, rows: Math.ceil(count / cols) };
}

// ---- justified rows ---------------------------------------------------------------------------

export interface JustifiedRow extends Row {
  height: number;
  widths: number[];
}

/**
 * Greedy justified layout: add items until the row at `targetHeight` would overflow `width`, then scale the
 * row so it fills the width exactly. The last row keeps `targetHeight` (never stretched).
 */
export function justifiedRows(
  count: number,
  aspect: (index: number) => number,
  width: number,
  targetHeight: number,
  gap: number,
): JustifiedRow[] {
  const rows: JustifiedRow[] = [];
  let start = 0;
  let sum = 0;
  for (let i = 0; i < count; i++) {
    sum += aspect(i);
    const n = i - start + 1;
    if (sum * targetHeight + gap * (n - 1) >= width) {
      const height = (width - gap * (n - 1)) / sum;
      rows.push(makeRow(start, i + 1, height, aspect));
      start = i + 1;
      sum = 0;
    }
  }
  if (start < count) rows.push(makeRow(start, count, targetHeight, aspect));
  return rows;
}

function makeRow(start: number, end: number, height: number, aspect: (i: number) => number): JustifiedRow {
  const widths: number[] = [];
  for (let i = start; i < end; i++) widths.push(aspect(i) * height);
  return { start, end, height, widths };
}

// ---- timeline -------------------------------------------------------------------------------

export type TimelineRow =
  | { kind: "header"; startMs: number; count: number; firstIndex: number }
  | ({ kind: "items" } & Row);

/** Buckets in display order (newest first) -> a header row per bucket, then rows of `cols` items. */
export function timelineRows(buckets: readonly TimelineBucket[], cols: number): TimelineRow[] {
  const rows: TimelineRow[] = [];
  let index = 0;
  for (const b of buckets) {
    rows.push({ kind: "header", startMs: b.startMs, count: b.count, firstIndex: index });
    for (let s = index; s < index + b.count; s += cols) rows.push({ kind: "items", start: s, end: Math.min(s + cols, index + b.count) });
    index += b.count;
  }
  return rows;
}

// ---- navigation -----------------------------------------------------------------------------

/** Index of the row holding `index` (rows sorted by `start`; header rows are skipped). */
export function rowOf(rows: readonly (Row | TimelineRow)[], index: number): number {
  let lo = 0;
  let hi = rows.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const r = rows[mid];
    const [s, e] = "kind" in r && r.kind === "header" ? [r.firstIndex, r.firstIndex] : [(r as Row).start, (r as Row).end];
    if (index < s) hi = mid - 1;
    else if (index >= e) lo = mid + 1;
    else return mid;
  }
  return -1;
}

const isItemRow = (r: Row | TimelineRow): r is Row => !("kind" in r) || r.kind === "items";

/** Moves `delta` item rows from the row of `index`, keeping the column offset (clamped to the row length). */
export function moveRows(rows: readonly (Row | TimelineRow)[], index: number, delta: number): number {
  const from = rowOf(rows, index);
  if (from < 0) return index;
  const cur = rows[from] as Row;
  const offset = index - cur.start;
  let target = from;
  let left = Math.abs(delta);
  const step = Math.sign(delta);
  while (left > 0) {
    let next = target + step;
    while (next >= 0 && next < rows.length && !isItemRow(rows[next])) next += step;
    if (next < 0 || next >= rows.length) break;
    target = next;
    left--;
  }
  const r = rows[target] as Row;
  return Math.min(r.start + offset, r.end - 1);
}

/** `count` uniform rows of `cols` as Row objects (for navigation over the uniform grid). */
export function uniformRows(count: number, cols: number): Row[] {
  const rows: Row[] = [];
  for (let s = 0; s < count; s += cols) rows.push({ start: s, end: Math.min(s + cols, count) });
  return rows;
}
