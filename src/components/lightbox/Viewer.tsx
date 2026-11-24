// Single-image viewer: toolbar, zoom/pan, filmstrip, slideshow, eyedropper. `quick` = Space quick-look.
import { useEffect, useState, type KeyboardEvent } from "react";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { setFavorite, trashImages } from "../../actions/images";
import type { ImageList } from "../../data/images";
import { useT } from "../../i18n";
import { PREVIEW_WIDTH, SLIDESHOW_MS } from "../../lib/defaults";
import { fullUrl, thumbUrl } from "../../lib/ssimg";
import { useView } from "../../state/view";
import { ShareButtonsFor } from "../ShareButtons";
import { pickColor } from "./eyedropper";
import { Filmstrip } from "./Filmstrip";
import { clampZoom, ZoomPan } from "./ZoomPan";

export function Viewer({ list, index, quick, single = false }: { list: ImageList; index: number; quick: boolean; single?: boolean }) {
  const t = useT();
  const [zoom, setZoom] = useState(1);
  const [playing, setPlaying] = useState(false);
  const [dropper, setDropper] = useState(false);
  const item = list.items[index];

  const go = (i: number) => {
    if (single) return;
    const next = Math.max(0, Math.min(list.total - 1, i));
    list.ensureLoaded(next);
    setZoom(1);
    const v = useView.getState();
    v.focus(next);
    v.openLightbox({ mode: quick ? "quick" : "view", index: next });
  };

  useEffect(() => {
    if (!playing) return;
    const id = setInterval(() => (index + 1 < list.total ? go(index + 1) : setPlaying(false)), SLIDESHOW_MS);
    return () => clearInterval(id);
  });

  const pick = async (src: string, fx: number, fy: number) => {
    const toast = useView.getState().toast;
    try {
      const hex = await pickColor(src, fx, fy);
      await writeText(hex);
      toast({ text: t("lightbox.picked", { hex }), tone: "info", undoBatchId: null });
    } catch (e) {
      console.warn(e);
      toast({ text: t("lightbox.eyedropperBlocked"), tone: "error", undoBatchId: null });
    }
  };

  const zoomBy = (f: number) => setZoom((z) => clampZoom(z * f));
  const onKey = (e: KeyboardEvent) => {
    const keys: Record<string, () => void> = {
      ArrowLeft: () => go(index - 1),
      ArrowRight: () => go(index + 1),
      "+": () => zoomBy(1.25),
      "=": () => zoomBy(1.25),
      "-": () => zoomBy(1 / 1.25),
      "0": () => setZoom(1),
      " ": () => (quick ? useView.getState().closeLightbox() : setPlaying((p) => !p)),
      f: () => item && void setFavorite([item.id], !item.favorite),
      Delete: () => item && !e.shiftKey && void trashImages([item.id]),
    };
    const fn = keys[e.key];
    if (!fn) return;
    e.preventDefault();
    e.stopPropagation();
    fn();
  };

  return (
    <div className="flex h-full flex-col gap-2" onKeyDown={onKey}>
      <div className="ss-panel flex items-center gap-2 px-3 py-2 text-sm">
        <span className="min-w-0 flex-1 truncate">{item?.fileName ?? "…"}</span>
        <span className="ss-muted">
          {index + 1} / {list.total}
        </span>
        <Btn label={t("lightbox.prev")} onClick={() => go(index - 1)} text="‹" />
        <Btn label={t("lightbox.next")} onClick={() => go(index + 1)} text="›" />
        <Btn label={t("lightbox.zoomOut")} onClick={() => zoomBy(1 / 1.25)} text="−" />
        <Btn label={t("lightbox.zoomReset")} onClick={() => setZoom(1)} text={`${Math.round(zoom * 100)}%`} />
        <Btn label={t("lightbox.zoomIn")} onClick={() => zoomBy(1.25)} text="+" />
        <button type="button" className="ss-btn" aria-pressed={playing} onClick={() => setPlaying((p) => !p)}>
          {playing ? t("lightbox.pause") : t("lightbox.slideshow")}
        </button>
        <button type="button" className="ss-btn" aria-pressed={dropper} onClick={() => setDropper((d) => !d)}>
          {t("lightbox.eyedropper")}
        </button>
        <Btn label={t("lightbox.close")} onClick={() => useView.getState().closeLightbox()} text="✕" />
      </div>
      {!quick && item && <div className="flex justify-center"><ShareButtonsFor id={item.id} compact /></div>}
      {item ? (
        <ZoomPan
          previewSrc={thumbUrl(item, PREVIEW_WIDTH)}
          fullSrc={fullUrl(item)}
          alt={item.fileName}
          zoom={zoom}
          onZoom={setZoom}
          eyedropper={dropper}
          onPick={(src, fx, fy) => void pick(src, fx, fy)}
        />
      ) : (
        <div className="ss-shimmer flex-1 rounded" />
      )}
      {!quick && !single && <Filmstrip items={list.items} index={index} onGo={go} />}
    </div>
  );
}

function Btn({ label, text, onClick }: { label: string; text: string; onClick: () => void }) {
  return (
    <button type="button" className="ss-btn" onClick={onClick} aria-label={label} title={label}>
      {text}
    </button>
  );
}
