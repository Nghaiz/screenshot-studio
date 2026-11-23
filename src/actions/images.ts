// User actions on images. Each calls exactly one command and reports the outcome as a toast.
// "Delete" is the in-app trash (D8). Nothing here can reach `request_permanent_delete` / `delete_permanently`:
// that flow is a separate dialog (phase 2.2), never a shortcut or a palette command (design-intent § Safety).
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type { OpResult } from "@bindings/OpResult";
import { errorKey, translate, type MessageKey } from "../i18n";
import { call, toAppError } from "../lib/ipc";
import { usePrefs } from "../state/prefs";
import { useView } from "../state/view";

const t = (key: MessageKey, vars?: Record<string, string | number>) => translate(usePrefs.getState().language, key, vars);

/** An informational toast in the current language. */
export function notify(key: MessageKey, vars?: Record<string, string | number>): void {
  useView.getState().toast({ text: t(key, vars), tone: "info", undoBatchId: null });
}

function reportError(e: unknown): void {
  const err = toAppError(e);
  useView.getState().toast({ text: `${t(errorKey(err.kind))}: ${err.message}`, tone: "error", undoBatchId: null });
}

function reportResult(r: OpResult, okKey: MessageKey): void {
  const toast = useView.getState().toast;
  if (r.affected > 0) toast({ text: t(okKey, { count: r.affected }), tone: "info", undoBatchId: r.batchId });
  if (r.failed.length > 0) toast({ text: t("actions.partial", { failed: r.failed.length }), tone: "error", undoBatchId: null });
}

function nothingSelected(ids: number[]): boolean {
  if (ids.length > 0) return false;
  notify("actions.nothingSelected");
  return true;
}

export async function trashImages(ids: number[]): Promise<void> {
  if (nothingSelected(ids)) return;
  try {
    reportResult(await call("trash_images", { imageIds: ids }), "actions.trashed");
    useView.getState().clearSelection();
  } catch (e) {
    reportError(e);
  }
}

/** Opens the confirm dialog; `confirmRecycle` performs the call. */
export function askRecycle(ids: number[]): void {
  if (nothingSelected(ids)) return;
  useView.getState().askRecycle(ids);
}

export async function confirmRecycle(ids: number[]): Promise<void> {
  useView.getState().askRecycle(null);
  try {
    reportResult(await call("recycle_images", { imageIds: ids }), "actions.recycled");
    useView.getState().clearSelection();
  } catch (e) {
    reportError(e);
  }
}

export async function setFavorite(ids: number[], value: boolean): Promise<void> {
  if (nothingSelected(ids)) return;
  try {
    reportResult(await call("set_favorite", { imageIds: ids, value }), "actions.favorited");
  } catch (e) {
    reportError(e);
  }
}

export async function setRating(ids: number[], rating: number): Promise<void> {
  if (nothingSelected(ids)) return;
  try {
    reportResult(await call("set_rating", { imageIds: ids, rating }), "actions.rated");
  } catch (e) {
    reportError(e);
  }
}

export async function undo(batchId: number | null): Promise<void> {
  try {
    const r = await call("undo", { batchId });
    notify("actions.undone");
    if (r.failed.length > 0) {
      useView.getState().toast({ text: t("actions.partial", { failed: r.failed.length }), tone: "error", undoBatchId: null });
    }
  } catch (e) {
    reportError(e);
  }
}

export async function reindex(): Promise<void> {
  try {
    await call("start_index", { rootId: null });
  } catch (e) {
    reportError(e);
  }
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
  let picked: string | string[] | null;
  try {
    picked = await openDialog({ directory: true, multiple: false });
  } catch (e) {
    reportError(e);
    return false;
  }
  if (typeof picked !== "string") return false;
  return addFolder(picked);
}
