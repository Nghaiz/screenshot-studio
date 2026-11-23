// Move / copy to a real folder: destination from the OS folder picker (or a sidebar drop), mode, conflict rule.
import { useState } from "react";
import type { MoveMode } from "@bindings/MoveMode";
import type { OnConflict } from "@bindings/OnConflict";
import { pickFolder } from "../../actions/images";
import { moveImages } from "../../actions/organize";
import { useT } from "../../i18n";
import { useView } from "../../state/view";
import { Buttons, Modal } from "./Modal";

export function MoveDialog(props: { ids: number[]; mode: MoveMode; destDir: string | null }) {
  const t = useT();
  const [mode, setMode] = useState<MoveMode>(props.mode);
  const [dest, setDest] = useState<string | null>(props.destDir);
  const [onConflict, setOnConflict] = useState<OnConflict>("rename");
  const [busy, setBusy] = useState(false);
  const close = () => useView.getState().closeDialog();
  const browse = async () => {
    const picked = await pickFolder();
    if (picked) setDest(picked);
  };
  const run = async () => {
    if (!dest) return;
    setBusy(true);
    const ok = await moveImages(props.ids, dest, mode, onConflict);
    setBusy(false);
    if (ok) close();
  };

  return (
    <Modal label={t(mode === "move" ? "move.titleMove" : "move.titleCopy", { count: props.ids.length })}>
      <div role="radiogroup" aria-label={t("move.mode")} className="flex gap-1">
        {(["move", "copy"] as const).map((m) => (
          <button key={m} type="button" role="radio" aria-checked={mode === m} className="ss-btn" aria-pressed={mode === m} onClick={() => setMode(m)}>
            {t(`move.${m}`)}
          </button>
        ))}
      </div>
      <div className="flex items-center gap-2">
        <span className="ss-input min-w-0 flex-1 truncate" data-testid="move-dest" title={dest ?? undefined}>{dest ?? t("move.noDest")}</span>
        <button type="button" className="ss-btn" onClick={() => void browse()}>{t("move.browse")}</button>
      </div>
      <label className="flex items-center gap-2">
        <span className="ss-muted text-xs">{t("move.onConflict")}</span>
        <select className="ss-input" value={onConflict} onChange={(e) => setOnConflict(e.target.value as OnConflict)}>
          <option value="rename">{t("move.conflictRename")}</option>
          <option value="skip">{t("move.conflictSkip")}</option>
        </select>
      </label>
      <p className="ss-muted m-0 text-xs">{t("move.undoHint")}</p>
      <Buttons>
        <button type="button" className="ss-btn" onClick={close}>{t("actions.cancel")}</button>
        <button type="button" className="ss-btn ss-btn-primary" disabled={!dest || busy} onClick={() => void run()}>
          {t(mode === "move" ? "move.confirmMove" : "move.confirmCopy")}
        </button>
      </Buttons>
    </Modal>
  );
}
