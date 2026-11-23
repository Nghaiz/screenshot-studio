// Phase 3.3 actions: the AI job, sidecar restart and settings, copying the setup commands, and the reversible smart
// cleanups. Safety (design-intent § Safety, phase 3.3 risks): the duplicate finder and the cleanup wizard only reach
// `trash_images` (in-app trash) and `recycle_images` (Recycle Bin, undoable) through actions/images.ts. Nothing here
// imports actions/purge.ts or can send `request_permanent_delete` / `delete_permanently`; smart.test.tsx asserts it.
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import type { AiDevice } from "@bindings/AiDevice";
import type { AiTask } from "@bindings/AiTask";
import { refresh } from "../data/organize";
import { call } from "../lib/ipc";
import { qk } from "../lib/query";
import { useView } from "../state/view";
import { confirmRecycle, trashImages } from "./images";
import { attempt, notify } from "./report";

/** Starts the AI job over every image still missing a task's result; the status bar follows its `job-progress`. */
export async function runAiOnLibrary(tasks: readonly AiTask[]): Promise<void> {
  if (tasks.length === 0) return notify("ai.notReady");
  const r = await attempt(() => call("start_ai", { tasks: [...tasks], imageIds: null }));
  if (!r) return;
  // Shown at once; the first `job-progress` event replaces it.
  useView.getState().jobProgress({ jobId: r.jobId, kind: "ai", stage: "queued", done: 0, total: 0, currentPath: null, finished: false, error: null });
  notify("ai.started");
}

export async function cancelJob(jobId: number): Promise<void> {
  await attempt(() => call("cancel_job", { jobId }));
}

export async function restartSidecar(): Promise<void> {
  await attempt(() => call("restart_sidecar"));
  refresh(qk.sidecar);
}

export async function setAiDevice(device: AiDevice): Promise<void> {
  // `set_setting` answers null, so success is marked explicitly (attempt() returns null on failure).
  const ok = await attempt(async () => {
    await call("set_setting", { key: "aiDevice", value: device });
    return true;
  });
  if (!ok) return;
  notify("ai.deviceSaved");
  refresh(qk.settings);
}

/** Copies the setup commands exactly as shown. */
export async function copySetupCommand(text: string): Promise<void> {
  const ok = await attempt(async () => {
    await writeText(text);
    return true;
  });
  if (ok) notify("ai.copied");
}

/** Duplicate finder: the chosen copies go to the in-app trash (journaled, Undo in the toast). */
export async function trashCopies(ids: readonly number[]): Promise<void> {
  await trashImages([...ids]);
  refresh(qk.duplicatesAll, qk.stats, qk.issues);
}

export interface CleanupPlan {
  /** Duplicates, blurry, tiny: to the in-app trash. */
  trash: readonly number[];
  /** Old in-app trash: to the Windows Recycle Bin (undoable). */
  recycle: readonly number[];
}

/** The wizard's Apply: its review step is the explicit confirmation of both moves. */
export async function applyCleanup(plan: CleanupPlan): Promise<void> {
  if (plan.trash.length > 0) await trashImages([...plan.trash]);
  if (plan.recycle.length > 0) await confirmRecycle([...plan.recycle]);
  refresh(qk.duplicatesAll, qk.stats, qk.issues, qk.oldTrash);
}
