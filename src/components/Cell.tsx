// One gallery item. Loaded items show the 320 px thumbnail; unloaded indices show a shimmer placeholder.
import { memo, type CSSProperties, type MouseEvent } from "react";
import type { ImageSummary } from "@bindings/ImageSummary";
import { useT } from "../i18n";
import { GRID_THUMB_WIDTH } from "../lib/defaults";
import { thumbUrl } from "../lib/ssimg";

export interface CellProps {
  index: number;
  item: ImageSummary | undefined;
  focused: boolean;
  selected: boolean;
  style: CSSProperties;
  variant?: "tile" | "row";
  onPick: (index: number, e: MouseEvent) => void;
  onOpen: (index: number) => void;
}

export const cellDomId = (index: number) => `ss-cell-${index}`;

function CellImpl({ index, item, focused, selected, style, variant = "tile", onPick, onOpen }: CellProps) {
  const t = useT();
  const ready = item && item.thumbState === "ok";
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
    >
      <div className={variant === "row" ? "h-10 w-16 shrink-0 overflow-hidden rounded" : "h-full w-full"}>
        {ready ? (
          <img src={thumbUrl(item, GRID_THUMB_WIDTH)} alt="" loading="lazy" decoding="async" draggable={false} />
        ) : !item || item.thumbState === "pending" ? (
          <div className="ss-shimmer h-full w-full" />
        ) : (
          <div className="ss-muted flex h-full w-full items-center justify-center text-lg" aria-hidden>
            {item.thumbState === "offline" ? "☁" : "⚠"}
          </div>
        )}
      </div>
      {variant === "row" && item && <RowText item={item} />}
      {variant === "tile" && item && <Badges item={item} cloudLabel={t("grid.cloudOnly")} />}
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
