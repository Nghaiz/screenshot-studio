// Tag assign (one journaled `set_tags` batch) and tag create/edit dialogs.
import { useState } from "react";
import { createTag, setTags, updateTag } from "../../actions/organize";
import { useImageDetail } from "../../data/library";
import { useTags } from "../../data/organize";
import { useT } from "../../i18n";
import { DEFAULT_TAG_COLOR } from "../../lib/defaults";
import { useView } from "../../state/view";
import { Buttons, Modal } from "./Modal";

type Change = "keep" | "add" | "remove";
const NEXT: Record<Change, Change> = { keep: "add", add: "remove", remove: "keep" };

/**
 * Each tag cycles keep -> add -> remove. With one image its current tags are shown (and the first click on a
 * tag it has removes it); with many images nothing is assumed about who has what.
 */
export function TagAssignDialog({ ids }: { ids: number[] }) {
  const t = useT();
  const tags = useTags();
  const detail = useImageDetail(ids.length === 1 ? ids[0] : null);
  const has = new Set(detail.data?.tags.map((x) => x.id) ?? []);
  const [changes, setChanges] = useState<Record<number, Change>>({});
  const [newName, setNewName] = useState("");
  const close = () => useView.getState().closeDialog();
  const change = (id: number): Change => changes[id] ?? "keep";
  const cycle = (id: number) => {
    let next = NEXT[change(id)];
    if (has.has(id) && next === "add") next = "remove"; // adding a tag the image already has is a no-op
    setChanges({ ...changes, [id]: next });
  };
  const add = Object.entries(changes).filter(([, c]) => c === "add").map(([id]) => Number(id));
  const remove = Object.entries(changes).filter(([, c]) => c === "remove").map(([id]) => Number(id));

  const createAndAdd = async () => {
    if (!newName.trim()) return;
    const tag = await createTag(newName, DEFAULT_TAG_COLOR);
    if (!tag) return;
    setNewName("");
    setChanges((c) => ({ ...c, [tag.id]: "add" }));
  };

  return (
    <Modal label={t("tags.assignTitle", { count: ids.length })}>
      <div className="flex flex-wrap gap-1.5" role="group" aria-label={t("nav.tags")}>
        {tags.data?.map((tag) => {
          const c = change(tag.id);
          return (
            <button key={tag.id} type="button" className="ss-btn" data-change={c} aria-pressed={c !== "keep"} onClick={() => cycle(tag.id)}
              style={c === "remove" ? { textDecoration: "line-through", borderColor: "#ef4444" } : undefined}>
              <span aria-hidden className="inline-block h-2.5 w-2.5 rounded-full" style={{ background: tag.color }} />
              {tag.name}
              {has.has(tag.id) && c === "keep" && <span aria-label={t("tags.has")}>✓</span>}
              {c === "add" && <span aria-hidden>＋</span>}
              {c === "remove" && <span aria-hidden>−</span>}
            </button>
          );
        })}
        {tags.data?.length === 0 && <p className="ss-muted m-0">{t("nav.noTags")}</p>}
      </div>
      <p className="ss-muted m-0 text-xs">{t("tags.cycleHint")}</p>
      <div className="flex gap-2">
        <input className="ss-input flex-1" aria-label={t("tags.newName")} placeholder={t("tags.newName")} value={newName}
          onChange={(e) => setNewName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void createAndAdd()} />
        <button type="button" className="ss-btn" disabled={!newName.trim()} onClick={() => void createAndAdd()}>
          {t("tags.create")}
        </button>
      </div>
      <Buttons>
        <button type="button" className="ss-btn" onClick={close}>{t("actions.cancel")}</button>
        <button type="button" className="ss-btn ss-btn-primary" disabled={add.length + remove.length === 0}
          onClick={async () => {
            if (await setTags(ids, add, remove)) close();
          }}>
          {t("tags.apply")}
        </button>
      </Buttons>
    </Modal>
  );
}

export function TagEditDialog({ tagId }: { tagId: number | null }) {
  const t = useT();
  const tags = useTags();
  const existing = tagId === null ? null : (tags.data?.find((x) => x.id === tagId) ?? null);
  const [name, setName] = useState(existing?.name ?? "");
  const [color, setColor] = useState(existing?.color ?? DEFAULT_TAG_COLOR);
  const close = () => useView.getState().closeDialog();
  const save = async () => {
    if (!name.trim()) return;
    const ok = existing ? await updateTag(existing.id, name, color) : await createTag(name, color);
    if (ok) close();
  };
  return (
    <Modal label={existing ? t("tags.edit", { name: existing.name }) : t("tags.create")}>
      <label className="flex flex-col gap-1">
        <span className="ss-muted text-xs">{t("tags.name")}</span>
        <input className="ss-input" autoFocus value={name} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void save()} />
      </label>
      <label className="flex items-center gap-2">
        <span className="ss-muted text-xs">{t("tags.color")}</span>
        <input type="color" value={color} onChange={(e) => setColor(e.target.value.toUpperCase())} />
      </label>
      <Buttons>
        <button type="button" className="ss-btn" onClick={close}>{t("actions.cancel")}</button>
        <button type="button" className="ss-btn ss-btn-primary" disabled={!name.trim()} onClick={() => void save()}>{t("actions.save")}</button>
      </Buttons>
    </Modal>
  );
}
