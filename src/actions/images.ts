// User actions on images and library roots. Each calls exactly one command and reports the outcome as a toast.
// "Delete" is the in-app trash (D8). Nothing here can reach `request_permanent_delete` / `delete_permanently`:
// that flow lives only in `actions/purge.ts`, used only by the permanent delete dialog (design-intent § Safety).
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { LibraryRoot } from "@bindings/LibraryRoot";
import { refresh } from "../data/organize";
import { call } from "../lib/ipc";
import { qk } from "../lib/query";
import { useView } from "../state/view";
import { attempt, notify, nothingSelected, reportError, reportResult, t } from "./report";

export { notify };

export async function trashImages(ids: number[]): Promise<void> {
  if (nothingSelected(ids)) return;
  const r = await attempt(() => call("trash_images", { imageIds: ids }));
  if (!r) return;
  reportResult(r, "actions.trashed");
  useView.getState().clearSelection();
  refresh();
}

export async function restoreImages(ids: number[]): Promise<void> {
  if (nothingSelected(ids)) return;
  const r = await attempt(() => call("restore_images", { imageIds: ids }));
  if (!r) return;
  reportResult(r, "actions.restored");
  useView.getState().clearSelection();
  refresh();
}

/** Opens the confirm dialog; `confirmRecycle` performs the call. */
export function askRecycle(ids: number[]): void {
  if (nothingSelected(ids)) return;
  useView.getState().askRecycle(ids);
}

export async function confirmRecycle(ids: number[]): Promise<void> {
  useView.getState().askRecycle(null);
  const r = await attempt(() => call("recycle_images", { imageIds: ids }));
  if (!r) return;
  reportResult(r, "actions.recycled");
  useView.getState().clearSelection();
  refresh();
}

export async function setFavorite(ids: number[], value: boolean): Promise<void> {
  if (nothingSelected(ids)) return;
  const r = await attempt(() => call("set_favorite", { imageIds: ids, value }));
  if (r) reportResult(r, "actions.favorited");
}

export async function setRating(ids: number[], rating: number): Promise<void> {
  if (nothingSelected(ids)) return;
  const r = await attempt(() => call("set_rating", { imageIds: ids, rating }));
  if (r) reportResult(r, "actions.rated");
}

/** `batchId` null = the latest undoable batch (Ctrl+Z). */
export async function undo(batchId: number | null): Promise<void> {
  const r = await attempt(() => call("undo", { batchId }));
  if (!r) return;
  notify("actions.undone");
  if (r.failed.length > 0) {
    useView.getState().toast({ text: t("actions.partial", { failed: r.failed.length, reason: r.failed[0].message }), tone: "error", undoBatchId: null });
  }
  refresh(qk.tags, qk.albums);
}

export async function reindex(): Promise<void> {
  await attempt(() => call("start_index", { rootId: null }));
}

/** Adds a library root and starts indexing it. Returns false when it failed (error already shown). */
export async function addFolder(path: string): Promise<boolean> {
  try {
    const root = await call("add_library_root", { path });
    await call("start_index", { rootId: root.id });
    return true;
  } catch (e) {
    reportError(e);
    return false;
  }
}

export async function pickAndAddFolder(): Promise<boolean> {
  const picked = await pickFolder();
  return picked === null ? false : addFolder(picked);
}

/** The OS folder picker; null when cancelled or when the dialog failed (error shown). */
export async function pickFolder(): Promise<string | null> {
  const picked = await attempt(() => openDialog({ directory: true, multiple: false }));
  return typeof picked === "string" ? picked : null;
}

/** Catalog-only (contract rule 7): asks first, naming what is lost; the files on disk are never touched. */
export function askRemoveRoot(root: LibraryRoot): void {
  useView.getState().openDialog({
    kind: "confirm",
    request: {
      title: t("confirm.removeRootTitle"),
      body: t("confirm.removeRootBody", { path: root.path, count: root.imageCount }),
      confirmLabel: t("confirm.remove"),
      run: async () => {
        const r = await attempt(() => call("remove_library_root", { rootId: root.id }));
        if (!r) return;
        notify("confirm.rootRemoved", { path: root.path });
        refresh(qk.appInfo, qk.tags, qk.albums, qk.stats, qk.timelineAll);
      },
    },
  });
}
