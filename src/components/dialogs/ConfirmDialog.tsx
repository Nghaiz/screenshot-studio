// Confirm for a catalog-only deletion (remove root, delete tag/album/smart filter): not journaled, so the body
// names what is lost (contract rule 7). Cancel has the initial focus; the destructive button is never default.
import { useT } from "../../i18n";
import type { ConfirmRequest } from "../../state/view";
import { useView } from "../../state/view";
import { Buttons, Modal } from "./Modal";

export function ConfirmDialog({ request }: { request: ConfirmRequest }) {
  const t = useT();
  const close = () => useView.getState().closeDialog();
  return (
    <Modal label={request.title} role="alertdialog">
      <p className="m-0">{request.body}</p>
      <p className="ss-muted m-0 text-xs">{t("confirm.notUndoable")}</p>
      <Buttons>
        <button type="button" className="ss-btn" autoFocus onClick={close}>{t("actions.cancel")}</button>
        <button type="button" className="ss-btn" style={{ borderColor: "#ef4444", color: "#ef4444" }}
          onClick={() => {
            close();
            void request.run();
          }}>
          {request.confirmLabel}
        </button>
      </Buttons>
    </Modal>
  );
}
