// Stitch 2+ images into one (`stitch_images`; a new file next to the first image, in selection order).
import { useState } from "react";
import type { StitchDirection } from "@bindings/StitchDirection";
import { stitchImages } from "../../actions/edit";
import { useT } from "../../i18n";
import { useView } from "../../state/view";
import { Modal } from "./Modal";

export const STITCH_GAP_DEFAULT = 0;
export const STITCH_BG_DEFAULT = "#ffffff";

export function StitchDialog({ ids }: { ids: number[] }) {
  const t = useT();
  const [direction, setDirection] = useState<StitchDirection>("vertical");
  const [gap, setGap] = useState(STITCH_GAP_DEFAULT);
  const [background, setBackground] = useState(STITCH_BG_DEFAULT);
  const [busy, setBusy] = useState(false);
  const title = t("stitch.title", { count: ids.length });

  const run = async () => {
    setBusy(true);
    const out = await stitchImages({ imageIds: ids, direction, gap, background });
    setBusy(false);
    if (out) useView.getState().closeDialog();
  };

  return (
    <Modal label={title}>
      <h2 className="m-0 text-base font-semibold">{title}</h2>
      {ids.length < 2 && <p role="alert" className="m-0 text-xs">{t("stitch.needTwo")}</p>}
      <div role="radiogroup" aria-label={t("stitch.direction")} className="flex gap-3">
        {(["vertical", "horizontal"] as const).map((d) => (
          <label key={d} className="flex items-center gap-1">
            <input type="radio" name="stitch-direction" checked={direction === d} onChange={() => setDirection(d)} />
            {t(`stitch.${d}`)}
          </label>
        ))}
      </div>
      <label className="flex items-center gap-2">
        {t("stitch.gap")}
        <input className="ss-input w-20" type="number" min={0} max={200} value={gap}
          onChange={(e) => setGap(Math.max(0, Math.min(200, Math.round(Number(e.target.value) || 0))))} />
      </label>
      <label className="flex items-center gap-2">
        {t("stitch.background")}
        <input type="color" value={background} onChange={(e) => setBackground(e.target.value)} />
      </label>
      <div className="flex justify-end gap-2">
        <button type="button" className="ss-btn" onClick={() => useView.getState().closeDialog()}>{t("actions.cancel")}</button>
        <button type="button" className="ss-btn ss-btn-primary" disabled={busy || ids.length < 2} onClick={() => void run()}>{t("stitch.run")}</button>
      </div>
    </Modal>
  );
}
