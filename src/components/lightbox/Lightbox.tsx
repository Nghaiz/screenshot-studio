// Lightbox overlay (modal dialog). Esc closes; focus moves into the dialog and back to the grid on close.
import { useEffect, useRef } from "react";
import { AnimatePresence, motion } from "motion/react";
import type { ImageList } from "../../data/images";
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

  const name = lb.mode === "view" || lb.mode === "quick" ? (list.items[lb.index]?.fileName ?? "") : "";
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
        </motion.div>
      )}
    </AnimatePresence>
  );
}
