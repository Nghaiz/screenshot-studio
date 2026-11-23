// Outcome reporting shared by every action: translated toasts, the error envelope, OpResult with Undo.
import type { OpResult } from "@bindings/OpResult";
import { errorKey, translate, type MessageKey, type Vars } from "../i18n";
import { toAppError } from "../lib/ipc";
import { usePrefs } from "../state/prefs";
import { useView } from "../state/view";

export const t = (key: MessageKey, vars?: Vars) => translate(usePrefs.getState().language, key, vars);

/** An informational toast in the current language. */
export function notify(key: MessageKey, vars?: Vars): void {
  useView.getState().toast({ text: t(key, vars), tone: "info", undoBatchId: null });
}

export function reportError(e: unknown): void {
  const err = toAppError(e);
  useView.getState().toast({ text: `${t(errorKey(err.kind))}: ${err.message}`, tone: "error", undoBatchId: null });
}

/** Success toast carries "Undo" when the op wrote a journal batch; partial failures get their own toast. */
export function reportResult(r: OpResult, okKey: MessageKey, vars?: Vars): void {
  const toast = useView.getState().toast;
  if (r.affected > 0) toast({ text: t(okKey, { count: r.affected, ...vars }), tone: "info", undoBatchId: r.batchId });
  if (r.failed.length > 0) {
    const first = r.failed[0];
    toast({ text: t("actions.partial", { failed: r.failed.length, reason: first.message }), tone: "error", undoBatchId: null });
  }
}

export function nothingSelected(ids: readonly number[]): boolean {
  if (ids.length > 0) return false;
  notify("actions.nothingSelected");
  return true;
}

/** Runs one command, reporting a rejection as an error toast. Returns null when it failed. */
export async function attempt<T>(fn: () => Promise<T>): Promise<T | null> {
  try {
    return await fn();
  } catch (e) {
    reportError(e);
    return null;
  }
}
