// Share and edit actions for one image: edit, copy image, copy OCR text, drag-out to Explorer (press on the
// handle and drag), reveal in Explorer, open with the default app. Used by the details panel and the lightbox.
import type { ImageDetail } from "@bindings/ImageDetail";
import { copyImage, copyOcrText, dragOut, openWithDefault, revealInExplorer } from "../actions/share";
import { useImageDetail } from "../data/library";
import { useT } from "../i18n";
import { useView } from "../state/view";

export function ShareButtons({ d, compact = false }: { d: ImageDetail; compact?: boolean }) {
  const t = useT();
  const hasText = !!d.ocrText?.trim();
  return (
    <div role="group" aria-label={t("share.title")} className={`flex flex-wrap gap-1 ${compact ? "" : "text-xs"}`}>
      <button type="button" className="ss-btn" onClick={() => useView.getState().openEditor(d.id)}>{t("share.edit")}</button>
      <button type="button" className="ss-btn" onClick={() => void copyImage(d.id)}>{t("share.copyImage")}</button>
      <button type="button" className="ss-btn" disabled={!hasText} title={hasText ? undefined : t("share.noText")}
        onClick={() => void copyOcrText(d.ocrText)}>{t("share.copyText")}</button>
      <button type="button" className="ss-btn cursor-grab" title={t("share.dragHint")}
        onPointerDown={(e) => {
          if (e.button !== 0) return;
          e.preventDefault();
          void dragOut([d.absPath]);
        }}>
        ⇱ {t("share.dragOut")}
      </button>
      <button type="button" className="ss-btn" onClick={() => void revealInExplorer(d.absPath)}>{t("share.reveal")}</button>
      <button type="button" className="ss-btn" onClick={() => void openWithDefault(d.absPath)}>{t("share.openWith")}</button>
    </div>
  );
}

/** The same buttons for an image known by id (fetches its detail for the path and the OCR text). */
export function ShareButtonsFor({ id, compact }: { id: number; compact?: boolean }) {
  const detail = useImageDetail(id);
  return detail.data ? <ShareButtons d={detail.data} compact={compact} /> : null;
}
