// One gallery item. Loaded items show the 320 px thumbnail; unloaded indices show a shimmer placeholder.
//
// Memory (Deferred decision #31, measured phase 2.2 in the release app on 6,300 fixture images): a fast
// scroll that starts a fetch and decode for every cell it passes pushes Chromium's decoded-image memory
// (renderer + GPU process) to ~813 MB. A cell therefore gets its <img> only after it has stayed mounted
// THUMB_SETTLE_MS, so rows that fly past are never fetched or decoded. With that, a fast 20 s full scroll
// peaks at 476-521 MB, and paging through every screen (all 6,300 decoded) at 494-501 MB. The first
// screen alone is ~420 MB of WebView2 cost; more headroom needs a Chromium-side cap (phase 2.3).
import { memo, useEffect, useState, type CSSProperties, type DragEvent, type MouseEvent } from "react";
import type { ImageSummary } from "@bindings/ImageSummary";
import { useT } from "../i18n";
import { GRID_THUMB_WIDTH, THUMB_SETTLE_MS } from "../lib/defaults";
import { thumbUrl } from "../lib/ssimg";
import { useView } from "../state/view";

export interface CellProps {
  index: number;
  item: ImageSummary | undefined;
  focused: boolean;
  selected: boolean;
  style: CSSProperties;
  variant?: "tile" | "row";
  onPick: (index: number, e: MouseEvent) => void;
  onOpen: (index: number) => void;
  onDragStart?: (index: number, e: DragEvent) => void;
  /** Set only where cells are drop targets (album view: drop = reorder). */
  onDropOn?: (index: number, e: DragEvent) => void;
}

export const cellDomId = (index: number) => `ss-cell-${index}`;

/** True once `key` has been the same for `ms` (resets when the cell is reused for another image). */
export function useSettled(key: number | undefined, ms: number): boolean {
  const [settledKey, setSettledKey] = useState<number | undefined>(undefined);
  useEffect(() => {
    if (key === undefined) return;
    const id = setTimeout(() => setSettledKey(key), ms);
    return () => clearTimeout(id);
  }, [key, ms]);
  return key !== undefined && settledKey === key;
}

function CellImpl({ index, item, focused, selected, style, variant = "tile", onPick, onOpen, onDragStart, onDropOn }: CellProps) {
  const t = useT();
  const ready = item && item.thumbState === "ok";
  const settled = useSettled(ready ? item.id : undefined, THUMB_SETTLE_MS);
  const fresh = useView((s) => item !== undefined && s.fresh.has(item.id));
  return (
    <div
      id={cellDomId(index)}
      role="gridcell"
      data-item=""
      data-index={index}
      data-focused={focused}
      aria-selected={selected}
      aria-label={item?.fileName ?? undefined}
      className={`ss-cell ${variant === "row" ? "flex items-center gap-3 px-2" : ""}`}
      style={style}
      onClick={(e) => onPick(index, e)}
      onDoubleClick={() => onOpen(index)}
      draggable={!!item && !!onDragStart}
      onDragStart={onDragStart && ((e) => onDragStart(index, e))}
      onDragOver={onDropOn && ((e) => e.preventDefault())}
      onDrop={onDropOn && ((e) => onDropOn(index, e))}
    >
      <div className={variant === "row" ? "h-10 w-16 shrink-0 overflow-hidden rounded" : "h-full w-full"}>
        {ready && settled ? (
          <img src={thumbUrl(item, GRID_THUMB_WIDTH)} alt="" loading="lazy" decoding="async" draggable={false} />
        ) : ready || !item || item.thumbState === "pending" ? (
          <div className="ss-shimmer h-full w-full" />
        ) : (
          <div className="ss-muted flex h-full w-full items-center justify-center text-lg" aria-hidden>
            {item.thumbState === "offline" ? "☁" : "⚠"}
          </div>
        )}
      </div>
      {variant === "row" && item && <RowText item={item} />}
      {variant === "tile" && item && <Badges item={item} cloudLabel={t("grid.cloudOnly")} />}
      {fresh && <span className="ss-new-badge" data-testid="new-badge">{t("grid.new")}</span>}
    </div>
  );
}

function RowText({ item }: { item: ImageSummary }) {
  return (
    <div className="min-w-0 flex-1 text-sm">
      <div className="truncate">{item.fileName}</div>
      <div className="ss-muted truncate text-xs">
        {new Date(item.takenMs).toLocaleString()} · {item.width ?? "?"}×{item.height ?? "?"} · {formatBytes(item.sizeBytes)}
        {item.folder ? ` · ${item.folder}` : ""}
      </div>
    </div>
  );
}

function Badges({ item, cloudLabel }: { item: ImageSummary; cloudLabel: string }) {
  if (!item.favorite && item.rating === 0 && item.cloudState !== "cloudOnly") return null;
  return (
    <div className="pointer-events-none absolute inset-x-1 bottom-1 flex items-center gap-1 text-xs" style={{ textShadow: "0 1px 2px #000" }}>
      {item.favorite && <span aria-hidden>★</span>}
      {item.rating > 0 && <span aria-hidden>{"•".repeat(item.rating)}</span>}
      {item.cloudState === "cloudOnly" && <span title={cloudLabel}>☁</span>}
    </div>
  );
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(0)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
}

export const Cell = memo(CellImpl);
