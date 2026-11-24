// Lightbox overlay (modal dialog). Esc closes; focus moves into the dialog and back to the grid on close.
import { useEffect, useMemo, useRef } from "react";
import { AnimatePresence, motion } from "motion/react";
import type { ImageList } from "../../data/images";
import { useImageDetail } from "../../data/library";
import { toAppError } from "../../lib/ipc";
import { emptyQuery } from "../../lib/query";
import { useT } from "../../i18n";
import { useView } from "../../state/view";
import { useMotion } from "../useMotion";
import { Compare } from "./Compare";
import { Viewer } from "./Viewer";

export function Lightbox({ list }: { list: ImageList }) {
  const t = useT();
  const lb = useView((s) => s.lightbox);
  const ref = useRef<HTMLDivElement>(null);
  const m = useMotion();
  const open = lb.mode !== "closed";

  useEffect(() => {
    if (!open) return;
    const back = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    ref.current?.focus();
    return () => back?.focus();
  }, [open]);

  const shownId = lb.mode === "view" || lb.mode === "quick" ? list.items[lb.index]?.id : lb.mode === "single" ? lb.id : undefined;
  // Opening an image drops its "new" badge (phase 4.2).
  useEffect(() => {
    if (shownId !== undefined) useView.getState().seen(shownId);
  }, [shownId]);

  const single = useImageDetail(lb.mode === "single" ? lb.id : null);
  const name =
    lb.mode === "view" || lb.mode === "quick" ? (list.items[lb.index]?.fileName ?? "")
    : lb.mode === "single" ? (single.data?.fileName ?? "")
    : "";
  return (
    <AnimatePresence>
      {open && (
        <motion.div
          ref={ref}
          role="dialog"
          aria-modal="true"
          aria-label={lb.mode === "compare" ? t("lightbox.compareLabel") : t("lightbox.label", { name })}
          tabIndex={-1}
          className="fixed inset-0 z-40 p-4 outline-none"
          style={{ background: "color-mix(in srgb, black 78%, transparent)" }}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={m.fade}
          onKeyDown={(e) => {
            if (e.key !== "Escape") return;
            e.stopPropagation();
            useView.getState().closeLightbox();
          }}
        >
          {lb.mode === "compare" && <Compare ids={lb.ids} />}
          {(lb.mode === "view" || lb.mode === "quick") && <Viewer list={list} index={lb.index} quick={lb.mode === "quick"} />}
          {lb.mode === "single" && <SingleViewer id={lb.id} />}
        </motion.div>
      )}
    </AnimatePresence>
  );
}

/** One image by id (`open-image`): the viewer over a one-item list, so it may be outside the current filter. */
function SingleViewer({ id }: { id: number }) {
  const detail = useImageDetail(id);
  const one = useMemo<ImageList | null>(
    () => (detail.data ? { query: emptyQuery(), items: [detail.data], total: 1, loading: false, error: null, ensureLoaded: () => {} } : null),
    [detail.data],
  );
  if (detail.isError) return <p role="alert" className="ss-panel m-0 p-4 text-sm">{toAppError(detail.error).message}</p>;
  return one ? <Viewer list={one} index={0} quick={false} single /> : <div className="ss-shimmer h-full w-full rounded" aria-busy="true" />;
}
