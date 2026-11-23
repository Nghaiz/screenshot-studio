// Bulk rename: pattern -> live `preview_rename` (debounced), conflicts highlighted, Rename disabled while any
// conflict or pattern error remains; only changed names are sent to `rename_images` (one journaled batch).
import { useEffect, useState } from "react";
import type { RenamePreviewItem } from "@bindings/RenamePreviewItem";
import { renameImages } from "../../actions/organize";
import { useT } from "../../i18n";
import { RENAME_PREVIEW_DEBOUNCE_MS } from "../../lib/defaults";
import { call, toAppError } from "../../lib/ipc";
import { useView } from "../../state/view";
import { Buttons, Modal } from "./Modal";

export const DEFAULT_RENAME_PATTERN = "{name}";
const TOKENS = "{name} {n} {n:03} {date:YYYYMMDD} {time:HHmmss} {w} {h} {ext}";

type Preview = { status: "loading" } | { status: "ok"; items: RenamePreviewItem[] } | { status: "error"; message: string };

export function RenameDialog({ ids }: { ids: number[] }) {
  const t = useT();
  const [pattern, setPattern] = useState(DEFAULT_RENAME_PATTERN);
  const [preview, setPreview] = useState<Preview>({ status: "loading" });
  const [busy, setBusy] = useState(false);
  const close = () => useView.getState().closeDialog();

  useEffect(() => {
    let live = true;
    setPreview({ status: "loading" });
    const id = setTimeout(() => {
      call("preview_rename", { imageIds: ids, pattern }).then(
        (r) => live && setPreview({ status: "ok", items: r.items }),
        (e) => live && setPreview({ status: "error", message: toAppError(e).message }),
      );
    }, RENAME_PREVIEW_DEBOUNCE_MS);
    return () => {
      live = false;
      clearTimeout(id);
    };
  }, [ids, pattern]);

  const items = preview.status === "ok" ? preview.items : [];
  const conflicts = items.filter((i) => i.conflict).length;
  const changed = items.filter((i) => !i.conflict && i.newName !== i.oldName);
  const canRename = preview.status === "ok" && conflicts === 0 && changed.length > 0 && !busy;

  return (
    <Modal label={t("rename.title", { count: ids.length })} wide>
      <label className="flex flex-col gap-1">
        <span className="ss-muted text-xs">{t("rename.pattern")}</span>
        <input className="ss-input font-mono" autoFocus aria-label={t("rename.pattern")} value={pattern} onChange={(e) => setPattern(e.target.value)} />
        <span className="ss-muted text-xs">{t("rename.tokens", { tokens: TOKENS })}</span>
      </label>
      {preview.status === "error" && <p role="alert" className="m-0 text-xs" style={{ color: "#ef4444" }}>{preview.message}</p>}
      {conflicts > 0 && <p role="alert" className="m-0 text-xs" style={{ color: "#ef4444" }} data-testid="rename-conflicts">{t("rename.conflicts", { count: conflicts })}</p>}
      <div className="max-h-80 overflow-y-auto rounded" style={{ border: "1px solid var(--ss-border)" }} aria-busy={preview.status === "loading"}>
        <table className="w-full border-collapse text-xs">
          <thead>
            <tr className="ss-muted text-left">
              <th className="px-2 py-1">{t("rename.old")}</th>
              <th className="px-2 py-1">{t("rename.new")}</th>
            </tr>
          </thead>
          <tbody>
            {items.map((i) => (
              <tr key={i.imageId} data-conflict={i.conflict} style={i.conflict ? { background: "color-mix(in srgb, #ef4444 22%, transparent)" } : undefined}>
                <td className="truncate px-2 py-0.5">{i.oldName}</td>
                <td className="truncate px-2 py-0.5 font-mono">
                  {i.newName}
                  {i.conflict && <span className="ml-2 font-sans" style={{ color: "#ef4444" }}>{t("rename.conflict")}</span>}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <Buttons>
        <button type="button" className="ss-btn" onClick={close}>{t("actions.cancel")}</button>
        <button type="button" className="ss-btn ss-btn-primary" disabled={!canRename}
          onClick={async () => {
            setBusy(true);
            const ok = await renameImages(changed.map((i) => ({ imageId: i.imageId, newName: i.newName })));
            setBusy(false);
            if (ok) close();
          }}>
          {t("rename.confirm", { count: changed.length })}
        </button>
      </Buttons>
    </Modal>
  );
}
