// Organize actions: tags, albums, notes, saved smart filters, real-folder move/copy and bulk rename.
// Image-level changes are journaled by the backend (Undo toast); deleting a tag/album/smart filter is
// catalog-only and not undoable, so it goes through a confirm dialog that names what is lost (rule 7).
import type { Album } from "@bindings/Album";
import type { MoveMode } from "@bindings/MoveMode";
import type { OnConflict } from "@bindings/OnConflict";
import type { RenameItem } from "@bindings/RenameItem";
import type { SmartFilter } from "@bindings/SmartFilter";
import type { Tag } from "@bindings/Tag";
import { refresh } from "../data/organize";
import { call } from "../lib/ipc";
import { buildQuery, filterOf, qk } from "../lib/query";
import { usePrefs } from "../state/prefs";
import { useView } from "../state/view";
import { attempt, notify, nothingSelected, reportResult, t } from "./report";

const view = () => useView.getState();

// --- tags --------------------------------------------------------------------------------------------

export async function createTag(name: string, color: string): Promise<Tag | null> {
  const tag = await attempt(() => call("create_tag", { name: name.trim(), color }));
  if (tag) refresh(qk.tags);
  return tag;
}

export async function updateTag(tagId: number, name: string, color: string): Promise<Tag | null> {
  const tag = await attempt(() => call("update_tag", { tagId, name: name.trim(), color }));
  if (tag) refresh(qk.tags);
  return tag;
}

/** Adds and removes tags on images in one journaled batch. */
export async function setTags(ids: number[], add: number[], remove: number[]): Promise<boolean> {
  if (nothingSelected(ids)) return false;
  if (add.length === 0 && remove.length === 0) return true;
  const r = await attempt(() => call("set_tags", { imageIds: ids, add, remove }));
  if (!r) return false;
  reportResult(r, "organize.tagsUpdated");
  refresh(qk.tags, ["image"]);
  return true;
}

export function askDeleteTag(tag: Tag): void {
  view().openDialog({
    kind: "confirm",
    request: {
      title: t("confirm.deleteTagTitle", { name: tag.name }),
      body: t("confirm.deleteTagBody", { name: tag.name, count: tag.count }),
      confirmLabel: t("confirm.delete"),
      run: async () => {
        const r = await attempt(() => call("delete_tag", { tagId: tag.id }));
        if (!r) return;
        const f = view().filter;
        if (f.tagIds.includes(tag.id)) view().setFilter({ tagIds: f.tagIds.filter((id) => id !== tag.id) });
        notify("confirm.tagDeleted", { name: tag.name });
        refresh(qk.tags, ["image"]);
      },
    },
  });
}

// --- albums ------------------------------------------------------------------------------------------

export async function createAlbum(name: string): Promise<Album | null> {
  const album = await attempt(() => call("create_album", { name: name.trim() }));
  if (album) refresh(qk.albums);
  return album;
}

export async function renameAlbum(albumId: number, name: string): Promise<Album | null> {
  const album = await attempt(() => call("rename_album", { albumId, name: name.trim() }));
  if (album) refresh(qk.albums);
  return album;
}

export async function addToAlbum(album: Pick<Album, "id" | "name">, ids: number[]): Promise<boolean> {
  if (nothingSelected(ids)) return false;
  const r = await attempt(() => call("album_add", { albumId: album.id, imageIds: ids }));
  if (!r) return false;
  reportResult(r, "organize.addedToAlbum", { name: album.name });
  refresh(qk.albums, ["image"]);
  return true;
}

export async function removeFromAlbum(albumId: number, ids: number[]): Promise<void> {
  if (nothingSelected(ids)) return;
  const r = await attempt(() => call("album_remove", { albumId, imageIds: ids }));
  if (!r) return;
  reportResult(r, "organize.removedFromAlbum");
  view().clearSelection();
  refresh(qk.albums, ["image"]);
}

/** `order` is the album's full new image order (contract: `album_reorder` takes the new order). */
export async function reorderAlbum(albumId: number, order: number[]): Promise<void> {
  const r = await attempt(() => call("album_reorder", { albumId, imageIds: order }));
  if (r) reportResult(r, "organize.reordered");
}

export function askDeleteAlbum(album: Album): void {
  view().openDialog({
    kind: "confirm",
    request: {
      title: t("confirm.deleteAlbumTitle", { name: album.name }),
      body: t("confirm.deleteAlbumBody", { name: album.name, count: album.count }),
      confirmLabel: t("confirm.delete"),
      run: async () => {
        const r = await attempt(() => call("delete_album", { albumId: album.id }));
        if (!r) return;
        if (view().filter.albumId === album.id) view().setFilter({ albumId: null });
        notify("confirm.albumDeleted", { name: album.name });
        refresh(qk.albums, ["image"]);
      },
    },
  });
}

// --- note --------------------------------------------------------------------------------------------

export async function setNote(imageId: number, note: string): Promise<void> {
  const value = note.trim() ? note : null;
  const r = await attempt(() => call("set_note", { imageId, note: value }));
  if (!r) return;
  reportResult(r, "organize.noteSaved");
  refresh(qk.image(imageId));
}

// --- smart filters -----------------------------------------------------------------------------------

/** Saves the current filter + sort as a named smart filter (`id` set = overwrite that one). */
export async function saveSmartFilter(name: string, id: number | null = null): Promise<SmartFilter | null> {
  const v = view();
  const query = { ...buildQuery({ view: v.view === "trash" ? "trash" : "library", filter: v.filter, sort: usePrefs.getState().sort }), cursor: null };
  const saved = await attempt(() => call("save_smart_filter", { id, name: name.trim(), query }));
  if (!saved) return null;
  notify("organize.filterSaved", { name: saved.name });
  refresh(qk.smartFilters);
  return saved;
}

export function applySmartFilter(f: SmartFilter): void {
  const v = view();
  if (v.view === "timeline" || f.query.trash === "only") v.setView(f.query.trash === "only" ? "trash" : "library");
  view().replaceFilter(filterOf(f.query));
  if (f.query.sort.field !== "relevance" || f.query.text) usePrefs.getState().set({ sort: f.query.sort });
}

export function askDeleteSmartFilter(f: SmartFilter): void {
  view().openDialog({
    kind: "confirm",
    request: {
      title: t("confirm.deleteFilterTitle", { name: f.name }),
      body: t("confirm.deleteFilterBody", { name: f.name }),
      confirmLabel: t("confirm.delete"),
      run: async () => {
        const r = await attempt(() => call("delete_smart_filter", { id: f.id }));
        if (!r) return;
        notify("confirm.filterDeleted", { name: f.name });
        refresh(qk.smartFilters);
      },
    },
  });
}

// --- real folders ------------------------------------------------------------------------------------

export async function moveImages(ids: number[], destDir: string, mode: MoveMode, onConflict: OnConflict): Promise<boolean> {
  if (nothingSelected(ids)) return false;
  const r = await attempt(() => call("move_images", { imageIds: ids, destDir, mode, onConflict }));
  if (!r) return false;
  reportResult(r, mode === "move" ? "organize.moved" : "organize.copied", { dest: destDir });
  if (mode === "move") view().clearSelection();
  refresh(qk.stats, qk.appInfo);
  return true;
}

// --- bulk rename -------------------------------------------------------------------------------------

export async function renameImages(items: RenameItem[]): Promise<boolean> {
  if (items.length === 0) return true;
  const r = await attempt(() => call("rename_images", { items }));
  if (!r) return false;
  reportResult(r, "organize.renamed");
  refresh(["image"]);
  return true;
}
