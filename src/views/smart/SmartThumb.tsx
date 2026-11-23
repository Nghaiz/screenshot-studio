// One image of a smart tool (duplicate group member, cleanup preview), known only by id: `get_image` gives the
// facts and the `mtimeMs` the thumbnail URL needs. The 320 thumbnail is shown smaller (no new size is generated).
import { useImageDetail } from "../../data/library";
import { useT } from "../../i18n";
import { GRID_THUMB_WIDTH } from "../../lib/defaults";
import { thumbUrl } from "../../lib/ssimg";
import { formatBytes } from "../../components/Cell";

export function SmartThumb({ id, size, caption = true }: { id: number; size: number; caption?: boolean }) {
  const t = useT();
  const d = useImageDetail(id);
  const box = { width: size, height: size };
  if (d.isPending) return <div className="ss-shimmer shrink-0 rounded" style={box} aria-busy="true" />;
  if (!d.data) {
    return (
      <div className="ss-muted flex shrink-0 items-center justify-center rounded text-xs" style={{ ...box, background: "var(--ss-border)" }}>
        {t("dup.gone")}
      </div>
    );
  }
  const img = d.data;
  return (
    <figure className="m-0 flex shrink-0 flex-col gap-1" style={{ width: size }} title={img.fileName}>
      <div className="ss-smart-thumb flex items-center justify-center overflow-hidden rounded" style={box}>
        {img.thumbState === "ok" ? (
          <img src={thumbUrl(img, GRID_THUMB_WIDTH)} alt={img.fileName} loading="lazy" decoding="async" draggable={false} />
        ) : (
          <span aria-hidden className="ss-muted text-lg">{img.thumbState === "offline" ? "☁" : "⚠"}</span>
        )}
      </div>
      {caption && (
        <figcaption className="text-xs leading-tight">
          <span className="block truncate">{img.fileName}</span>
          <span className="ss-muted block truncate">
            {img.width ?? "?"}×{img.height ?? "?"} · {formatBytes(img.sizeBytes)} · {new Date(img.takenMs).toLocaleDateString()}
          </span>
        </figcaption>
      )}
    </figure>
  );
}
