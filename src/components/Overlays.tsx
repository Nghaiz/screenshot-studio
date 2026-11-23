// Small overlays: Recycle Bin confirm (Shift+Delete), toasts with Undo, the animated background layer.
import { useEffect } from "react";
import { confirmRecycle, undo } from "../actions/images";
import { useT } from "../i18n";
import { TOAST_MS } from "../lib/defaults";
import { usePrefs } from "../state/prefs";
import { useView, type Toast } from "../state/view";
import { effectiveMotion } from "../themes";

/** Recycle Bin is undoable, so this is a plain confirm; the default (focused) button is Cancel. */
export function ConfirmRecycle() {
  const t = useT();
  const ids = useView((s) => s.confirmRecycle);
  if (!ids) return null;
  const cancel = () => useView.getState().askRecycle(null);
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center" style={{ background: "color-mix(in srgb, black 50%, transparent)" }}>
      <div role="alertdialog" aria-modal="true" aria-label={t("actions.recycleTitle")} className="ss-panel flex max-w-md flex-col gap-3 p-5"
        onKeyDown={(e) => e.key === "Escape" && cancel()}>
        <h2 className="m-0 text-base font-semibold">{t("actions.recycleTitle")}</h2>
        <p className="m-0 text-sm">{t("actions.recycleBody", { count: ids.length })}</p>
        <div className="flex justify-end gap-2">
          <button type="button" className="ss-btn" autoFocus onClick={cancel}>
            {t("actions.cancel")}
          </button>
          <button type="button" className="ss-btn ss-btn-primary" onClick={() => void confirmRecycle(ids)}>
            {t("actions.recycleConfirm")}
          </button>
        </div>
      </div>
    </div>
  );
}

export function Toasts() {
  const toasts = useView((s) => s.toasts);
  return (
    <div className="pointer-events-none fixed bottom-12 left-1/2 z-50 flex -translate-x-1/2 flex-col items-center gap-2">
      {toasts.map((t) => (
        <ToastItem key={t.id} toast={t} />
      ))}
    </div>
  );
}

function ToastItem({ toast }: { toast: Toast }) {
  const t = useT();
  useEffect(() => {
    const id = setTimeout(() => useView.getState().dismissToast(toast.id), TOAST_MS);
    return () => clearTimeout(id);
  }, [toast.id]);
  return (
    <div role="status" className="ss-panel pointer-events-auto flex items-center gap-3 px-4 py-2 text-sm"
      style={toast.tone === "error" ? { borderColor: "#ef4444" } : undefined}>
      <span>{toast.text}</span>
      {toast.undoBatchId !== null && (
        <button type="button" className="ss-btn" onClick={() => {
          useView.getState().dismissToast(toast.id);
          void undo(toast.undoBatchId);
        }}>
          {t("actions.undo")}
        </button>
      )}
    </div>
  );
}

/** Three slow blobs behind the UI; hidden when the toggle is off or motion is off. */
export function AnimatedBackground() {
  const on = usePrefs((s) => s.animatedBg && effectiveMotion(s.motion) !== "off");
  if (!on) return null;
  const blob = (left: string, top: string, color: string, delay: string) => (
    <div className="ss-blob" style={{ left, top, background: color, animationDelay: delay }} />
  );
  return (
    <div aria-hidden className="pointer-events-none fixed inset-0 -z-10 overflow-hidden">
      {blob("-10%", "-15%", "var(--ss-accent)", "0s")}
      {blob("55%", "40%", "color-mix(in srgb, var(--ss-accent) 40%, var(--ss-text))", "-9s")}
      {blob("20%", "65%", "color-mix(in srgb, var(--ss-accent) 60%, transparent)", "-17s")}
    </div>
  );
}
