// Action bar over the grid: organize actions for the selection (or the focused image), and in the trash view
// Restore / Recycle Bin / Delete permanently. The permanent delete button is the ONLY way to open that dialog,
// and it is a pointer click: it has no shortcut and no palette command (design-intent § Safety).
import type { ImageList } from "../data/images";
import { askRecycle, restoreImages, setFavorite, setRating, trashImages } from "../actions/images";
import { removeFromAlbum } from "../actions/organize";
import { copyImage } from "../actions/share";
import { targetIds } from "../commands/targets";
import { useAlbums } from "../data/organize";
import { useT } from "../i18n";
import { useView } from "../state/view";

export function ActionBar({ list }: { list: ImageList }) {
  const t = useT();
  const view = useView((s) => s.view);
  const selected = useView((s) => s.selection.size);
  const albumId = useView((s) => s.filter.albumId);
  const albums = useAlbums();
  const album = albumId === null ? null : albums.data?.find((a) => a.id === albumId);
  const v = useView.getState;
  const ids = () => targetIds(list.items);
  if (list.total === 0) return null;

  if (view === "trash") {
    return (
      <div className="ss-panel flex flex-wrap items-center gap-2 px-3 py-1.5 text-xs" role="toolbar" aria-label={t("bar.trash")}>
        <span className="ss-muted">{selected > 0 ? t("status.selected", { count: selected }) : t("bar.focused")}</span>
        <button type="button" className="ss-btn" onClick={() => void restoreImages(ids())}>{t("bar.restore")}</button>
        <button type="button" className="ss-btn" onClick={() => askRecycle(ids())}>{t("bar.recycle")}</button>
        <span className="flex-1" />
        <button type="button" className="ss-btn" style={{ borderColor: "#dc2626", color: "#dc2626" }}
          onClick={() => {
            const target = ids();
            if (target.length > 0) v().openDialog({ kind: "purge", ids: target });
          }}>
          {t("bar.purge")}
        </button>
      </div>
    );
  }

  return (
    <div className="ss-panel flex flex-wrap items-center gap-1.5 px-3 py-1.5 text-xs" role="toolbar" aria-label={t("bar.organize")}>
      {album && <span className="font-semibold">{t("bar.album", { name: album.name })}</span>}
      <span className="ss-muted">{selected > 0 ? t("status.selected", { count: selected }) : t("bar.focused")}</span>
      <button type="button" className="ss-btn" onClick={() => open("tags")}>{t("bar.tags")}</button>
      <button type="button" className="ss-btn" onClick={() => open("album")}>{t("bar.addToAlbum")}</button>
      {album && <button type="button" className="ss-btn" onClick={() => void removeFromAlbum(album.id, ids())}>{t("bar.removeFromAlbum")}</button>}
      <button type="button" className="ss-btn" onClick={() => open("move")}>{t("bar.move")}</button>
      <button type="button" className="ss-btn" onClick={() => open("copy")}>{t("bar.copy")}</button>
      <button type="button" className="ss-btn" onClick={() => open("rename")}>{t("bar.rename")}</button>
      <span role="group" aria-label={t("bar.share")} className="flex gap-1.5">
        <button type="button" className="ss-btn" onClick={() => {
          const first = ids()[0];
          if (first !== undefined) v().openEditor(first);
        }}>{t("share.edit")}</button>
        <button type="button" className="ss-btn" onClick={() => {
          const first = ids()[0];
          if (first !== undefined) void copyImage(first);
        }}>{t("share.copyImage")}</button>
        <button type="button" className="ss-btn" onClick={() => open("convert")}>{t("bar.convert")}</button>
        <button type="button" className="ss-btn" onClick={() => open("stitch")}>{t("bar.stitch")}</button>
        <button type="button" className="ss-btn" onClick={() => open("zip")}>{t("bar.exportZip")}</button>
        <button type="button" className="ss-btn" onClick={() => open("pdf")}>{t("bar.exportPdf")}</button>
      </span>
      <button type="button" className="ss-btn" aria-label={t("bar.favorite")} title={t("bar.favorite")} onClick={() => {
        const target = ids();
        const favs = new Set(list.items.filter((i) => i.favorite).map((i) => i.id));
        void setFavorite(target, !target.every((id) => favs.has(id)));
      }}>★</button>
      <span role="group" aria-label={t("bar.rating")} className="flex">
        {[1, 2, 3, 4, 5].map((n) => (
          <button key={n} type="button" className="px-0.5 text-sm" style={{ color: "var(--ss-accent)" }} aria-label={t("bar.rate", { n })} title={t("bar.rate", { n })}
            onClick={() => void setRating(ids(), n)}>☆</button>
        ))}
      </span>
      <button type="button" className="ss-btn" onClick={() => void trashImages(ids())}>{t("bar.trashIt")}</button>
      {selected > 0 && <button type="button" className="ss-btn" onClick={() => v().clearSelection()}>{t("cmd.clearSelection")}</button>}
    </div>
  );

  function open(kind: "tags" | "album" | "move" | "copy" | "rename" | "convert" | "stitch" | "zip" | "pdf") {
    const target = ids();
    if (target.length === 0) return;
    if (kind === "move" || kind === "copy") v().openDialog({ kind: "move", ids: target, mode: kind, destDir: null });
    else if (kind === "zip" || kind === "pdf") v().openDialog({ kind: "export", ids: target, format: kind });
    else v().openDialog({ kind, ids: target });
  }
}
