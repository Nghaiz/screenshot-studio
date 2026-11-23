// Add to album (pick or create), album create/rename, and "save as smart filter".
import { useState } from "react";
import { addToAlbum, createAlbum, renameAlbum, saveSmartFilter } from "../../actions/organize";
import { useAlbums } from "../../data/organize";
import { useT } from "../../i18n";
import { useView } from "../../state/view";
import { Buttons, Modal } from "./Modal";

const close = () => useView.getState().closeDialog();

export function AlbumPickDialog({ ids }: { ids: number[] }) {
  const t = useT();
  const albums = useAlbums();
  const [name, setName] = useState("");
  const createAndAdd = async () => {
    if (!name.trim()) return;
    const album = await createAlbum(name);
    if (album && (await addToAlbum(album, ids))) close();
  };
  return (
    <Modal label={t("albums.addTitle", { count: ids.length })}>
      <ul className="m-0 flex max-h-72 list-none flex-col gap-1 overflow-y-auto p-0">
        {albums.data?.map((a) => (
          <li key={a.id}>
            <button type="button" className="ss-btn w-full justify-between"
              onClick={async () => {
                if (await addToAlbum(a, ids)) close();
              }}>
              <span>{a.name}</span>
              <span className="ss-muted text-xs">{a.count}</span>
            </button>
          </li>
        ))}
        {albums.data?.length === 0 && <li className="ss-muted">{t("nav.noAlbums")}</li>}
      </ul>
      <div className="flex gap-2">
        <input className="ss-input flex-1" aria-label={t("albums.newName")} placeholder={t("albums.newName")} value={name}
          onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void createAndAdd()} />
        <button type="button" className="ss-btn" disabled={!name.trim()} onClick={() => void createAndAdd()}>{t("albums.createAndAdd")}</button>
      </div>
      <Buttons>
        <button type="button" className="ss-btn" onClick={close}>{t("actions.cancel")}</button>
      </Buttons>
    </Modal>
  );
}

export function AlbumEditDialog({ albumId }: { albumId: number | null }) {
  const t = useT();
  const albums = useAlbums();
  const existing = albumId === null ? null : (albums.data?.find((a) => a.id === albumId) ?? null);
  const [name, setName] = useState(existing?.name ?? "");
  const save = async () => {
    if (!name.trim()) return;
    const ok = existing ? await renameAlbum(existing.id, name) : await createAlbum(name);
    if (ok) close();
  };
  return (
    <NameDialog label={existing ? t("albums.rename", { name: existing.name }) : t("albums.create")} field={t("albums.name")}
      name={name} setName={setName} onSave={save} />
  );
}

export function SaveFilterDialog() {
  const t = useT();
  const [name, setName] = useState("");
  const save = async () => {
    if (name.trim() && (await saveSmartFilter(name))) close();
  };
  return <NameDialog label={t("filter.saveTitle")} field={t("filter.saveName")} name={name} setName={setName} onSave={save} />;
}

function NameDialog(props: { label: string; field: string; name: string; setName: (v: string) => void; onSave: () => Promise<void> }) {
  const t = useT();
  return (
    <Modal label={props.label}>
      <label className="flex flex-col gap-1">
        <span className="ss-muted text-xs">{props.field}</span>
        <input className="ss-input" autoFocus value={props.name} onChange={(e) => props.setName(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && void props.onSave()} />
      </label>
      <Buttons>
        <button type="button" className="ss-btn" onClick={close}>{t("actions.cancel")}</button>
        <button type="button" className="ss-btn ss-btn-primary" disabled={!props.name.trim()} onClick={() => void props.onSave()}>{t("actions.save")}</button>
      </Buttons>
    </Modal>
  );
}
