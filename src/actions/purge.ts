// Permanent delete (D8, design-intent § Safety). The ONLY module that calls `request_permanent_delete` /
// `delete_permanently`, and only `PermanentDeleteDialog` imports it: no shortcut, palette command or other
// action reaches it (organize.test / delete-dialogs.test / browse.test assert that).
import type { OpResult } from "@bindings/OpResult";
import type { PermanentDeleteTicket } from "@bindings/PermanentDeleteTicket";
import { refresh } from "../data/organize";
import { call } from "../lib/ipc";
import { qk } from "../lib/query";

/** The word the user must type, exactly (contract: `typedConfirmation` must equal "DELETE"). */
export const PURGE_WORD = "DELETE";

export class PurgeMismatch extends Error {
  constructor(readonly ticket: PermanentDeleteTicket) {
    super("the backend's delete set differs from the one shown");
  }
}

/**
 * Step 1 asks the backend for a 60 s token bound to `ids`; step 2 redeems it with the typed word. The ticket
 * is requested at confirm time (not when the dialog opens), so Cancel sends nothing and the token never
 * expires while the user reads. If the ticket's count or size differs from what the dialog showed, nothing
 * is deleted: the user must review again.
 */
export async function deletePermanently(ids: number[], typed: string, shown: { count: number; totalBytes: number }): Promise<OpResult> {
  if (typed !== PURGE_WORD) throw new Error("typed confirmation does not match");
  const ticket = await call("request_permanent_delete", { imageIds: ids });
  if (ticket.count !== shown.count || ticket.totalBytes !== shown.totalBytes) throw new PurgeMismatch(ticket);
  const r = await call("delete_permanently", { token: ticket.token, typedConfirmation: typed });
  refresh(qk.stats, qk.tags, qk.albums, qk.appInfo);
  return r;
}
