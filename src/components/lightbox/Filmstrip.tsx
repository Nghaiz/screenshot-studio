// Neighbouring thumbnails around the current image (a window, not the whole list).
import type { ImageSummary } from "@bindings/ImageSummary";
import { useT } from "../../i18n";
import { GRID_THUMB_WIDTH } from "../../lib/defaults";
import { thumbUrl } from "../../lib/ssimg";

const RADIUS = 10;

export function Filmstrip({ items, index, onGo }: { items: readonly ImageSummary[]; index: number; onGo: (i: number) => void }) {
  const t = useT();
  const from = Math.max(0, index - RADIUS);
  const to = Math.min(items.length, index + RADIUS + 1);
  return (
    <div role="listbox" aria-label={t("lightbox.filmstrip")} className="flex h-16 shrink-0 justify-center gap-1 overflow-hidden">
      {items.slice(from, to).map((it, k) => {
        const i = from + k;
        return (
          <button
            key={it.id}
            type="button"
            role="option"
            aria-selected={i === index}
            className="h-14 w-20 shrink-0 overflow-hidden rounded"
            style={{ outline: i === index ? "2px solid var(--ss-accent)" : "none", opacity: i === index ? 1 : 0.6 }}
            onClick={() => onGo(i)}
          >
            <img src={thumbUrl(it, GRID_THUMB_WIDTH)} alt={it.fileName} className="h-full w-full object-cover" draggable={false} />
          </button>
        );
      })}
    </div>
  );
}
