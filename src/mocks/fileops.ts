// Mock file operations: move/copy into a library folder, bulk rename preview + apply, the two-step permanent
// delete (a fresh 60 s token bound to the exact id set, plus the typed word "DELETE"), remove a root.
import type { ImageSummary } from "@bindings/ImageSummary";
import type { JournalOp } from "@bindings/JournalOp";
import type { OpResult } from "@bindings/OpResult";
import { appError } from "./catalog";
import type { Handlers, MockStore } from "./organize";
import { renderPattern } from "./rename";

export const PURGE_TOKEN_TTL_MS = 60_000;

export interface FileOpsDeps {
  store: MockStore;
  record: (op: JournalOp, count: number, undo: () => void, undoable?: boolean) => number;
  mutate: (op: JournalOp, ids: number[], apply: (img: ImageSummary) => void) => OpResult;
  next: () => number;
  ok: (batchId: number | null, affected: number) => OpResult;
}

export function fileOpHandlers({ store, record, mutate, next, ok }: FileOpsDeps): Handlers {
  const tickets = new Map<string, { ids: number[]; expiresAtMs: number }>();
  const fail = (message: string): never => {
    throw appError("notFound", message);
  };
  return {
    move_images: ({ imageIds, destDir, mode }) => {
      const root = store.roots.find((r) => destDir.toLowerCase().startsWith(r.path.toLowerCase()));
      if (!root) throw appError("invalidArgument", `destination outside every library root: ${destDir}`);
      const folder = destDir.slice(root.path.length).replace(/^[\\/]+/, "").replace(/\\/g, "/");
      if (mode === "move") {
        const r = mutate("move", imageIds, (i) => Object.assign(i, { rootId: root.id, folder }));
        return r;
      }
      const copies = imageIds.map((id) => ({ ...store.get(id), id: store.images.length + next() * 100_000, rootId: root.id, folder }));
      copies.forEach((c) => {
        store.images.push(c);
        store.byId.set(c.id, c);
      });
      const batchId = record("copy", copies.length, () => copies.forEach((c) => (c.missing = true)));
      store.changed("fileOp", null);
      return ok(batchId, copies.length);
    },
    preview_rename: ({ imageIds, pattern }) => {
      const renamed = new Set(imageIds);
      const items = imageIds.map((id, i) => {
        const img = store.get(id);
        return { imageId: id, oldName: img.fileName, newName: renderPattern(pattern, img, i + 1), folderKey: `${img.rootId}|${img.folder}` };
      });
      const taken = new Set(store.images.filter((i) => !renamed.has(i.id) && !i.missing).map((i) => `${i.rootId}|${i.folder}|${i.fileName.toLowerCase()}`));
      const seen = new Map<string, number>();
      for (const it of items) {
        const k = `${it.folderKey}|${it.newName.toLowerCase()}`;
        seen.set(k, (seen.get(k) ?? 0) + 1);
      }
      return {
        items: items.map(({ folderKey, ...it }) => {
          const k = `${folderKey}|${it.newName.toLowerCase()}`;
          const changed = it.newName !== it.oldName;
          return { ...it, conflict: changed && (taken.has(k) || (seen.get(k) ?? 0) > 1) };
        }),
      };
    },
    rename_images: ({ items }) => mutate("rename", items.map((i) => i.imageId), (img) => {
      img.fileName = items.find((i) => i.imageId === img.id)?.newName ?? img.fileName;
    }),
    request_permanent_delete: ({ imageIds }) => {
      const imgs = imageIds.map(store.get);
      const token = `tok-${next()}-${Math.random().toString(36).slice(2, 10)}`;
      const expiresAtMs = Date.now() + PURGE_TOKEN_TTL_MS;
      tickets.set(token, { ids: [...imageIds], expiresAtMs });
      return { token, count: imgs.length, totalBytes: imgs.reduce((a, i) => a + i.sizeBytes, 0), expiresAtMs };
    },
    delete_permanently: ({ token, typedConfirmation }) => {
      if (typedConfirmation !== "DELETE") throw appError("confirmationRequired", "typedConfirmation must be DELETE");
      const ticket = tickets.get(token);
      tickets.delete(token);
      if (!ticket || ticket.expiresAtMs < Date.now()) throw appError("confirmationRequired", "unknown or expired token");
      const gone = new Set(ticket.ids);
      for (let i = store.images.length - 1; i >= 0; i--) if (gone.has(store.images[i].id)) store.images.splice(i, 1);
      gone.forEach((id) => store.byId.delete(id));
      const batchId = record("purge", gone.size, () => {}, false);
      store.changed("fileOp", null);
      return ok(batchId, gone.size);
    },
    remove_library_root: ({ rootId }) => {
      const at = store.roots.findIndex((r) => r.id === rootId);
      if (at < 0) fail(`root ${rootId} not found`);
      store.roots.splice(at, 1);
      const n = store.images.filter((i) => i.rootId === rootId).length;
      store.changed("fileOp", null);
      return ok(null, n);
    },
  };
}
