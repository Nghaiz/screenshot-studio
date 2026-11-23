// Right side panel: details of the focused image (tag chips, albums, favourite, stars, note) or the undo history
// (`list_journal`; every undoable batch has its own Undo button, Ctrl+Z undoes the latest one).
import { useEffect, useState } from "react";
import type { ImageDetail } from "@bindings/ImageDetail";
import { setFavorite, setRating, undo } from "../actions/images";
import { setNote, setTags } from "../actions/organize";
import type { ImageList } from "../data/images";
import { useImageDetail } from "../data/library";
import { useJournal } from "../data/organize";
import { useT } from "../i18n";
import { useView } from "../state/view";

export function SidePanel({ list }: { list: ImageList }) {
  const panel = useView((s) => s.sidePanel);
  if (panel === "none") return null;
  return (
    <aside className="ss-panel flex w-72 shrink-0 flex-col gap-3 overflow-y-auto p-3 text-sm">
      {panel === "details" ? <Details list={list} /> : <History />}
    </aside>
  );
}

function Details({ list }: { list: ImageList }) {
  const t = useT();
  const focusIndex = useView((s) => s.focusIndex);
  const item = list.items[focusIndex];
  const detail = useImageDetail(item?.id ?? null);
  if (!item) return <p className="ss-muted m-0">{t("details.none")}</p>;
  if (!detail.data) return <div className="ss-shimmer h-24 w-full" aria-busy="true" />;
  return <DetailBody key={detail.data.id} d={detail.data} />;
}

function DetailBody({ d }: { d: ImageDetail }) {
  const t = useT();
  const [note, setNoteText] = useState(d.note ?? "");
  useEffect(() => setNoteText(d.note ?? ""), [d.note]);
  return (
    <section aria-label={t("details.title")} className="flex flex-col gap-3">
      <h2 className="m-0 break-all text-sm font-semibold">{d.fileName}</h2>
      <p className="ss-muted m-0 break-all text-xs">{d.absPath}</p>
      <div className="flex items-center gap-2">
        <button type="button" className="ss-btn" aria-pressed={d.favorite} onClick={() => void setFavorite([d.id], !d.favorite)}>★ {t("bar.favorite")}</button>
        <span role="group" aria-label={t("bar.rating")}>
          {[1, 2, 3, 4, 5].map((n) => (
            <button key={n} type="button" className="px-0.5" style={{ color: "var(--ss-accent)" }} aria-label={t("bar.rate", { n })}
              onClick={() => void setRating([d.id], d.rating === n ? 0 : n)}>{n <= d.rating ? "★" : "☆"}</button>
          ))}
        </span>
      </div>
      <div className="flex flex-col gap-1">
        <span className="ss-muted text-xs font-semibold uppercase">{t("nav.tags")}</span>
        <div className="flex flex-wrap gap-1" data-testid="tag-chips">
          {d.tags.map((tag) => (
            <span key={tag.id} className="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs"
              style={{ background: `color-mix(in srgb, ${tag.color} 30%, transparent)`, border: `1px solid ${tag.color}` }}>
              {tag.name}
              <button type="button" aria-label={t("details.untag", { name: tag.name })} onClick={() => void setTags([d.id], [], [tag.id])}>×</button>
            </span>
          ))}
          <button type="button" className="ss-btn" onClick={() => useView.getState().openDialog({ kind: "tags", ids: [d.id] })}>＋</button>
        </div>
      </div>
      {d.albums.length > 0 && (
        <div className="flex flex-col gap-1">
          <span className="ss-muted text-xs font-semibold uppercase">{t("nav.albums")}</span>
          <p className="m-0 text-xs">{d.albums.map((a) => a.name).join(", ")}</p>
        </div>
      )}
      <label className="flex flex-col gap-1">
        <span className="ss-muted text-xs font-semibold uppercase">{t("details.note")}</span>
        <textarea className="ss-input min-h-20" value={note} onChange={(e) => setNoteText(e.target.value)}
          onBlur={() => note !== (d.note ?? "") && void setNote(d.id, note)} />
      </label>
    </section>
  );
}

function History() {
  const t = useT();
  const journal = useJournal(true);
  return (
    <section aria-label={t("history.title")} className="flex flex-col gap-2">
      <h2 className="m-0 text-sm font-semibold">{t("history.title")}</h2>
      {journal.isError && <p className="ss-muted m-0 text-xs">{t("nav.loadFailed")}</p>}
      {journal.data?.items.length === 0 && <p className="ss-muted m-0 text-xs">{t("history.empty")}</p>}
      <ol className="m-0 flex list-none flex-col gap-1 p-0">
        {journal.data?.items.map((e) => (
          <li key={e.batchId} className="flex items-center gap-2 text-xs" data-batch={e.batchId}>
            <span className="flex-1">
              {t(`journal.${e.op}`, { count: e.count })}
              <span className="ss-muted block">{new Date(e.createdAt).toLocaleString()}</span>
            </span>
            {e.undoneAt !== null ? (
              <span className="ss-muted">{t("history.undone")}</span>
            ) : e.undoable ? (
              <button type="button" className="ss-btn" onClick={() => void undo(e.batchId)}>{t("actions.undo")}</button>
            ) : (
              <span className="ss-muted" title={t("history.notUndoable")}>—</span>
            )}
          </li>
        ))}
      </ol>
    </section>
  );
}
