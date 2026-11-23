// Mock organize backend (phase 2.2 UI tests and `VITE_MOCK_IPC`): tags, albums, notes, smart filters and the
// undo journal, with the contract's rules: one journal batch per image-level mutation, catalog-only container
// deletes are not journaled. File operations are in `fileops.ts`.
import type { Album } from "@bindings/Album";
import type { CommandMap } from "@bindings/commands";
import type { ImageSummary } from "@bindings/ImageSummary";
import type { JournalEntry } from "@bindings/JournalEntry";
import type { JournalOp } from "@bindings/JournalOp";
import type { LibraryChangeReason } from "@bindings/LibraryChangeReason";
import type { LibraryRoot } from "@bindings/LibraryRoot";
import type { OpResult } from "@bindings/OpResult";
import type { SmartFilter } from "@bindings/SmartFilter";
import type { Tag } from "@bindings/Tag";
import { appError, type FilterContext } from "./catalog";
import { fileOpHandlers } from "./fileops";

export interface MockStore {
  images: ImageSummary[];
  byId: Map<number, ImageSummary>;
  roots: LibraryRoot[];
  get: (id: number) => ImageSummary;
  changed: (reason: LibraryChangeReason, ids: number[] | null) => void;
}

export type Handlers = { [K in keyof CommandMap]?: (req: CommandMap[K]["req"]) => CommandMap[K]["res"] };
type TagRow = Omit<Tag, "count">;
type AlbumRow = Omit<Album, "count" | "coverImageId"> & { ids: number[] };

export function createOrganize(store: MockStore, now: () => number) {
  const tags: TagRow[] = [];
  const imageTags = new Map<number, Set<number>>();
  const albums: AlbumRow[] = [];
  const notes = new Map<number, string>();
  const filters: SmartFilter[] = [];
  const journal: { entry: JournalEntry; undo: () => void }[] = [];
  let seq = 0;
  const next = () => ++seq;

  const tagsOf = (id: number) => imageTags.get(id) ?? new Set<number>();
  const tagCount = (tagId: number) => [...imageTags.values()].filter((s) => s.has(tagId)).length;
  const tagOut = (t: TagRow): Tag => ({ ...t, count: tagCount(t.id) });
  const albumOut = (a: AlbumRow): Album => ({ id: a.id, name: a.name, createdAt: a.createdAt, count: a.ids.length, coverImageId: a.ids[0] ?? null });
  const findTag = (id: number) => tags.find((t) => t.id === id) ?? fail(`tag ${id} not found`);
  const findAlbum = (id: number) => albums.find((a) => a.id === id) ?? fail(`album ${id} not found`);
  function fail(message: string): never {
    throw appError("notFound", message);
  }
  const ok = (batchId: number | null, affected: number): OpResult => ({ batchId, affected, failed: [] });

  /** Records one journal batch; `undo` restores the state before it. */
  function record(op: JournalOp, count: number, undo: () => void, undoable = true): number {
    const batchId = next();
    journal.push({ entry: { batchId, op, count, createdAt: now(), undoable, undoneAt: null }, undo });
    return batchId;
  }

  /** Applies `apply` to each image and journals the previous field values. */
  function mutate(op: JournalOp, ids: number[], apply: (img: ImageSummary) => void): OpResult {
    const before = ids.map((id) => ({ ...store.get(id) }));
    ids.forEach((id) => apply(store.get(id)));
    const batchId = record(op, ids.length, () => before.forEach((b) => Object.assign(store.get(b.id), b)));
    store.changed("fileOp", ids);
    return ok(batchId, ids.length);
  }

  function undo(batchId: number | null): OpResult {
    const row = batchId === null
      ? [...journal].reverse().find((j) => j.entry.undoable && j.entry.undoneAt === null)
      : journal.find((j) => j.entry.batchId === batchId);
    if (!row) throw appError("notFound", "nothing to undo");
    if (!row.entry.undoable || row.entry.undoneAt !== null) throw appError("conflict", `batch ${row.entry.batchId} cannot be undone`);
    row.undo();
    row.entry.undoneAt = now();
    store.changed("undo", null);
    return ok(row.entry.batchId, row.entry.count);
  }

  const snapshotTags = () => new Map([...imageTags].map(([k, v]) => [k, new Set(v)]));

  const handlers: Handlers = {
    list_tags: () => tags.map(tagOut),
    create_tag: ({ name, color }) => {
      if (!name.trim()) throw appError("invalidArgument", "tag name is empty");
      if (tags.some((t) => t.name.toLowerCase() === name.toLowerCase())) throw appError("conflict", `tag ${name} exists`);
      const tag = { id: next(), name, color };
      tags.push(tag);
      return tagOut(tag);
    },
    update_tag: ({ tagId, name, color }) => {
      const tag = findTag(tagId);
      Object.assign(tag, { name, color });
      return tagOut(tag);
    },
    delete_tag: ({ tagId }) => {
      const n = tagCount(tagId);
      findTag(tagId);
      tags.splice(tags.findIndex((t) => t.id === tagId), 1);
      imageTags.forEach((s) => s.delete(tagId));
      store.changed("fileOp", null);
      return ok(null, n);
    },
    set_tags: ({ imageIds, add, remove }) => {
      [...add, ...remove].forEach(findTag);
      imageIds.forEach(store.get);
      const before = snapshotTags();
      for (const id of imageIds) {
        const s = new Set(tagsOf(id));
        add.forEach((t) => s.add(t));
        remove.forEach((t) => s.delete(t));
        imageTags.set(id, s);
      }
      const batchId = record(add.length > 0 ? "tag" : "untag", imageIds.length, () => {
        imageTags.clear();
        before.forEach((v, k) => imageTags.set(k, v));
      });
      store.changed("fileOp", imageIds);
      return ok(batchId, imageIds.length);
    },
    list_albums: () => albums.map(albumOut),
    create_album: ({ name }) => {
      const album = { id: next(), name, createdAt: now(), ids: [] };
      albums.push(album);
      return albumOut(album);
    },
    rename_album: ({ albumId, name }) => albumOut(Object.assign(findAlbum(albumId), { name })),
    delete_album: ({ albumId }) => {
      const a = findAlbum(albumId);
      albums.splice(albums.indexOf(a), 1);
      return ok(null, a.ids.length);
    },
    album_add: ({ albumId, imageIds }) => albumChange(albumId, "albumAdd", imageIds, (a) => [...a.ids, ...imageIds.filter((id) => !a.ids.includes(id))]),
    album_remove: ({ albumId, imageIds }) => albumChange(albumId, "albumRemove", imageIds, (a) => a.ids.filter((id) => !imageIds.includes(id))),
    album_reorder: ({ albumId, imageIds }) => {
      const a = findAlbum(albumId);
      if (imageIds.length !== a.ids.length || !a.ids.every((id) => imageIds.includes(id))) {
        throw appError("invalidArgument", "album_reorder needs every image of the album exactly once");
      }
      return albumChange(albumId, "albumReorder", imageIds, () => [...imageIds]);
    },
    set_note: ({ imageId, note }) => {
      store.get(imageId);
      const before = notes.get(imageId);
      if (note === null) notes.delete(imageId);
      else notes.set(imageId, note);
      const batchId = record("note", 1, () => (before === undefined ? notes.delete(imageId) : notes.set(imageId, before)));
      store.changed("fileOp", [imageId]);
      return ok(batchId, 1);
    },
    list_smart_filters: () => filters.map((f) => ({ ...f })),
    save_smart_filter: ({ id, name, query }) => {
      const existing = id === null ? undefined : filters.find((f) => f.id === id);
      if (id !== null && !existing) fail(`smart filter ${id} not found`);
      if (existing) return Object.assign(existing, { name, query });
      const f = { id: next(), name, query, createdAt: now() };
      filters.push(f);
      return f;
    },
    delete_smart_filter: ({ id }) => {
      const at = filters.findIndex((f) => f.id === id);
      if (at < 0) fail(`smart filter ${id} not found`);
      filters.splice(at, 1);
      return ok(null, 1);
    },
    list_journal: ({ limit }) => ({ items: journal.slice(-limit).reverse().map((j) => ({ ...j.entry })) }),
    undo: ({ batchId }) => undo(batchId),
  };

  function albumChange(albumId: number, op: JournalOp, imageIds: number[], next: (a: AlbumRow) => number[]): OpResult {
    const a = findAlbum(albumId);
    imageIds.forEach(store.get);
    const before = [...a.ids];
    a.ids = next(a);
    const batchId = record(op, imageIds.length, () => (a.ids = before));
    store.changed("fileOp", imageIds);
    return ok(batchId, imageIds.length);
  }

  const filterContext: FilterContext = {
    tagsOf,
    albumMembers: (id) => albums.find((a) => a.id === id)?.ids ?? null,
    noteOf: (id) => notes.get(id) ?? null,
  };

  /** Organize fields of `get_image`. */
  const detailOf = (id: number) => ({
    note: notes.get(id) ?? null,
    tags: tags.filter((t) => tagsOf(id).has(t.id)).map(tagOut),
    albums: albums.filter((a) => a.ids.includes(id)).map((a) => ({ id: a.id, name: a.name })),
  });

  Object.assign(handlers, fileOpHandlers({ store, record, mutate, next, ok }));
  return { handlers, mutate, filterContext, detailOf };
}
