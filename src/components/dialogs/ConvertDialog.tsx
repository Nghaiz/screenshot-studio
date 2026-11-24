// Resize / compress / convert the selection (`convert_images`). Overwrite keeps a backup and is undoable; the
// dialog says so before the click.
import { useState } from "react";
import type { EditMode } from "@bindings/EditMode";
import type { ImageFormat } from "@bindings/ImageFormat";
import { convertImages, DEFAULT_QUALITY, isLossy } from "../../actions/edit";
import { useT } from "../../i18n";
import { useView } from "../../state/view";
import { Modal } from "./Modal";

const FORMATS: ImageFormat[] = ["png", "jpeg", "webp"];
/** Long-edge choices offered for resizing (null = keep the size). */
export const MAX_EDGE_CHOICES = [null, 3840, 2560, 1920, 1280] as const;

export function ConvertDialog({ ids }: { ids: number[] }) {
  const t = useT();
  const [format, setFormat] = useState<ImageFormat>("webp");
  const [quality, setQuality] = useState(DEFAULT_QUALITY);
  const [maxEdge, setMaxEdge] = useState<number | null>(null);
  const [output, setOutput] = useState<EditMode>("copy");
  const [busy, setBusy] = useState(false);
  const title = t("convert.title", { count: ids.length });

  const run = async () => {
    setBusy(true);
    const ok = await convertImages({ imageIds: ids, format, quality: isLossy(format) ? quality : null, maxEdge, output });
    setBusy(false);
    if (ok) useView.getState().closeDialog();
  };

  return (
    <Modal label={title}>
      <h2 className="m-0 text-base font-semibold">{title}</h2>
      <label className="flex items-center gap-2">
        {t("editor.format")}
        <select className="ss-input" value={format} onChange={(e) => setFormat(e.target.value as ImageFormat)}>
          {FORMATS.map((f) => <option key={f} value={f}>{f.toUpperCase()}</option>)}
        </select>
      </label>
      {isLossy(format) && (
        <label className="flex items-center gap-2">
          {t("convert.quality", { q: quality })}
          <input type="range" min={40} max={100} value={quality} onChange={(e) => setQuality(Number(e.target.value))} />
        </label>
      )}
      <label className="flex items-center gap-2">
        {t("convert.maxEdge")}
        <select className="ss-input" value={maxEdge ?? ""} onChange={(e) => setMaxEdge(e.target.value === "" ? null : Number(e.target.value))}>
          {MAX_EDGE_CHOICES.map((n) => <option key={n ?? "keep"} value={n ?? ""}>{n === null ? t("convert.keepSize") : `${n} px`}</option>)}
        </select>
      </label>
      <div role="radiogroup" aria-label={t("convert.output")} className="flex gap-3">
        {(["copy", "overwrite"] as const).map((m) => (
          <label key={m} className="flex items-center gap-1">
            <input type="radio" name="convert-output" checked={output === m} onChange={() => setOutput(m)} />
            {t(m === "copy" ? "convert.asCopy" : "convert.overwrite")}
          </label>
        ))}
      </div>
      {output === "overwrite" && <p className="ss-muted m-0 text-xs" data-testid="convert-backup">{t("convert.backupNotice")}</p>}
      <div className="flex justify-end gap-2">
        <button type="button" className="ss-btn" onClick={() => useView.getState().closeDialog()}>{t("actions.cancel")}</button>
        <button type="button" className="ss-btn ss-btn-primary" disabled={busy} onClick={() => void run()}>{t("convert.run")}</button>
      </div>
    </Modal>
  );
}
