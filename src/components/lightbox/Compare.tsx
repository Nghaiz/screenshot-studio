// Side-by-side compare of the two selected images (1280 previews), with file facts under each.
import { useImageDetail } from "../../data/library";
import { useT } from "../../i18n";
import { PREVIEW_WIDTH } from "../../lib/defaults";
import { thumbUrl } from "../../lib/ssimg";
import { useView } from "../../state/view";
import { formatBytes } from "../Cell";

function Side({ id }: { id: number }) {
  const d = useImageDetail(id);
  if (!d.data) return <div className="ss-shimmer flex-1 rounded" />;
  const img = d.data;
  return (
    <figure className="m-0 flex min-w-0 flex-1 flex-col gap-2">
      <div className="flex min-h-0 flex-1 items-center justify-center">
        <img src={thumbUrl(img, PREVIEW_WIDTH)} alt={img.fileName} className="max-h-full max-w-full object-contain" />
      </div>
      <figcaption className="ss-panel px-3 py-2 text-xs">
        <div className="truncate text-sm">{img.fileName}</div>
        <div className="ss-muted">
          {img.width ?? "?"}×{img.height ?? "?"} · {formatBytes(img.sizeBytes)} · {new Date(img.takenMs).toLocaleString()}
        </div>
      </figcaption>
    </figure>
  );
}

export function Compare({ ids }: { ids: [number, number] }) {
  const t = useT();
  return (
    <div className="flex h-full flex-col gap-2">
      <div className="ss-panel flex items-center px-3 py-2 text-sm">
        <span className="flex-1">{t("lightbox.compareLabel")}</span>
        <button type="button" className="ss-btn" onClick={() => useView.getState().closeLightbox()} aria-label={t("lightbox.close")}>
          ✕
        </button>
      </div>
      <div className="flex min-h-0 flex-1 gap-3">
        <Side id={ids[0]} />
        <Side id={ids[1]} />
      </div>
    </div>
  );
}
