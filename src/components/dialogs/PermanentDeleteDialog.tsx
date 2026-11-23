// Permanent delete, exactly per design-intent § Safety: its own dialog, count + total size + the first 6
// thumbnails, the user types DELETE, the red button stays disabled until then and is never focused by default
// (focus starts in the text field; Enter there does nothing), never batched with another action. Opened only
// by a pointer click on the trash bar's button: no key and no palette command opens it.
import { useState } from "react";
import type { ImageSummary } from "@bindings/ImageSummary";
import { deletePermanently, PURGE_WORD, PurgeMismatch } from "../../actions/purge";
import { useT } from "../../i18n";
import { GRID_THUMB_WIDTH, PURGE_PREVIEW_COUNT } from "../../lib/defaults";
import { toAppError } from "../../lib/ipc";
import { thumbUrl } from "../../lib/ssimg";
import { useView } from "../../state/view";
import { formatBytes } from "../Cell";
import { Buttons, Modal } from "./Modal";

const DANGER = "#dc2626";

export function PermanentDeleteDialog({ ids, items }: { ids: number[]; items: readonly ImageSummary[] }) {
  const t = useT();
  const [typed, setTyped] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const byId = new Map(items.map((i) => [i.id, i]));
  const shownItems = ids.map((id) => byId.get(id)).filter((i): i is ImageSummary => i !== undefined);
  const complete = shownItems.length === ids.length;
  const shown = { count: shownItems.length, totalBytes: shownItems.reduce((a, i) => a + i.sizeBytes, 0) };
  const armed = typed === PURGE_WORD && complete && !busy;
  const close = () => useView.getState().closeDialog();

  const run = async () => {
    if (!armed) return;
    setBusy(true);
    setError(null);
    try {
      const r = await deletePermanently(ids, typed, shown);
      close();
      useView.getState().clearSelection();
      // No Undo on this toast: the purge batch is journaled as `undoable: false`.
      useView.getState().toast({ text: t("purge.done", { count: r.affected }), tone: "info", undoBatchId: null });
      if (r.failed.length > 0) {
        useView.getState().toast({ text: t("actions.partial", { failed: r.failed.length, reason: r.failed[0].message }), tone: "error", undoBatchId: null });
      }
    } catch (e) {
      setTyped("");
      setBusy(false);
      if (e instanceof PurgeMismatch) setError(t("purge.mismatch", { count: e.ticket.count, size: formatBytes(e.ticket.totalBytes) }));
      else setError(toAppError(e).message);
    }
  };

  return (
    <Modal label={t("purge.title", { count: ids.length })} role="alertdialog" wide>
      <p className="m-0 font-semibold" style={{ color: DANGER }}>{t("purge.warning")}</p>
      <p className="m-0" data-testid="purge-summary">
        {t("purge.summary", { count: shown.count, size: formatBytes(shown.totalBytes) })}
      </p>
      <div className="grid grid-cols-6 gap-1.5" aria-label={t("purge.preview")}>
        {shownItems.slice(0, PURGE_PREVIEW_COUNT).map((i) => (
          <div key={i.id} className="aspect-square overflow-hidden rounded" style={{ background: "var(--ss-border)" }} title={i.fileName}>
            {i.thumbState === "ok" && <img src={thumbUrl(i, GRID_THUMB_WIDTH)} alt={i.fileName} className="h-full w-full object-cover" />}
          </div>
        ))}
      </div>
      {!complete && <p role="alert" className="m-0 text-xs" style={{ color: DANGER }}>{t("purge.incomplete")}</p>}
      <label className="flex flex-col gap-1">
        <span>{t("purge.typePrompt", { word: PURGE_WORD })}</span>
        <input
          className="ss-input font-mono"
          autoFocus
          autoComplete="off"
          spellCheck={false}
          aria-label={t("purge.typeLabel", { word: PURGE_WORD })}
          value={typed}
          onChange={(e) => setTyped(e.target.value)}
          onKeyDown={(e) => {
            // Enter never submits: the only way to delete is a click on the red button.
            if (e.key === "Enter") e.preventDefault();
          }}
        />
      </label>
      {error && <p role="alert" className="m-0 text-xs" style={{ color: DANGER }}>{error}</p>}
      <Buttons>
        <button type="button" className="ss-btn" onClick={close}>{t("actions.cancel")}</button>
        <button
          type="button"
          className="ss-btn"
          data-testid="purge-confirm"
          disabled={!armed}
          style={{ background: DANGER, borderColor: DANGER, color: "#fff" }}
          onClick={() => void run()}
        >
          {t("purge.confirm", { count: ids.length })}
        </button>
      </Buttons>
    </Modal>
  );
}
