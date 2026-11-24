// Quick editor overlay (view store `editorId`): crop (free + presets), rotate/flip, annotate (arrow, box, text,
// highlight, numbered steps), blur/pixelate, auto-redact from `detect_pii`, undo/redo, then Save copy or Overwrite.
// Overwrite asks once and explains the backup; nothing is written before Save.
import { useEffect, useState } from "react";
import type { ImageDetail } from "@bindings/ImageDetail";
import type { ImageFormat } from "@bindings/ImageFormat";
import { detectPii, formatOf, saveEdit } from "../actions/edit";
import { notify } from "../actions/report";
import { useAppInfo, useImageDetail } from "../data/library";
import { useT } from "../i18n";
import { toAppError } from "../lib/ipc";
import { useView } from "../state/view";
import { EditorCanvas, type Brush, type Tool } from "./EditorCanvas";
import { EditorToolbar } from "./EditorToolbar";
import { useHistory } from "./history";
import { newDoc, outputRect, sourceRectToOriented, type CropPreset, type EditDoc } from "./model";
import { loadEditSource, type EditSource } from "./source";

export function Editor() {
  const t = useT();
  const id = useView((s) => s.editorId);
  const detail = useImageDetail(id);
  if (id === null) return null;
  return (
    <div role="dialog" aria-modal="true" aria-label={t("editor.label", { name: detail.data?.fileName ?? "…" })}
      className="fixed inset-0 z-[45] flex flex-col gap-2 p-3" style={{ background: "color-mix(in srgb, black 82%, var(--ss-accent) 4%)" }}>
      {detail.data ? <EditorBody key={detail.data.id} item={detail.data} /> : <div className="ss-shimmer flex-1 rounded" aria-busy="true" />}
    </div>
  );
}

type Loaded = { state: "loading" } | { state: "error"; message: string } | { state: "ready"; source: EditSource };

function EditorBody({ item }: { item: ImageDetail }) {
  const t = useT();
  const [loaded, setLoaded] = useState<Loaded>({ state: "loading" });
  useEffect(() => {
    let live = true;
    loadEditSource(item).then(
      (source) => live && setLoaded({ state: "ready", source }),
      (e: unknown) => live && setLoaded({ state: "error", message: toAppError(e).detail ?? toAppError(e).message }),
    );
    return () => {
      live = false;
    };
  }, [item]);
  const close = () => useView.getState().closeEditor();

  if (loaded.state === "loading") return <div className="ss-shimmer flex-1 rounded" aria-busy="true" />;
  if (loaded.state === "error") {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-3">
        <p role="alert" className="ss-panel m-0 max-w-xl px-5 py-3 text-sm">{t("editor.loadFailed", { reason: loaded.message })}</p>
        <button type="button" className="ss-btn" onClick={close}>{t("actions.cancel")}</button>
      </div>
    );
  }
  return <Workbench item={item} source={loaded.source} />;
}

function Workbench({ item, source }: { item: ImageDetail; source: EditSource }) {
  const t = useT();
  const history = useHistory<EditDoc>(newDoc(source.width, source.height));
  const doc = history.present;
  const [tool, setTool] = useState<Tool>("arrow");
  const [preset, setPreset] = useState<CropPreset>("free");
  const [brush, setBrush] = useState<Brush>({ color: "#ef4444", text: "" });
  const [format, setFormat] = useState<ImageFormat>(formatOf(item.fileName));
  const [askOverwrite, setAskOverwrite] = useState(false);
  const [askDiscard, setAskDiscard] = useState(false);
  const [busy, setBusy] = useState(false);
  const dataDir = useAppInfo().data?.dataDir ?? "";
  const dirty = history.canUndo;

  const close = () => useView.getState().closeEditor();
  const requestClose = () => (dirty ? setAskDiscard(true) : close());
  const save = async (mode: "copy" | "overwrite") => {
    setAskOverwrite(false);
    setBusy(true);
    const saved = await saveEdit(item.id, source.image, doc, mode, format);
    setBusy(false);
    if (saved) close();
  };
  const autoRedact = async () => {
    const r = await detectPii(item.id);
    if (!r) return;
    if (r.findings.length === 0) return notify("editor.noPii");
    const boxes = r.findings.map((f) => sourceRectToOriented(doc, { x: f.box[0], y: f.box[1], w: f.box[2] - f.box[0], h: f.box[3] - f.box[1] }));
    history.push({ ...doc, shapes: [...doc.shapes, ...boxes.map((rect) => ({ kind: "blur" as const, rect }))] });
    notify("editor.redacted", { count: boxes.length });
  };
  const out = outputRect(doc);

  return (
    <div className="flex h-full flex-col gap-2"
      onKeyDown={(e) => {
        e.stopPropagation();
        const k = e.key.toLowerCase();
        if (e.key === "Escape") requestClose();
        else if (e.ctrlKey && (k === "y" || (k === "z" && e.shiftKey))) history.redo();
        else if (e.ctrlKey && k === "z") history.undo();
        else return;
        e.preventDefault();
      }}>
      <EditorToolbar
        name={item.fileName} doc={doc} tool={tool} preset={preset} brush={brush} format={format} busy={busy} history={history}
        onTool={setTool} onPreset={setPreset} onBrush={setBrush} onFormat={setFormat} onDoc={history.push}
        onAutoRedact={() => void autoRedact()} onSaveCopy={() => void save("copy")} onOverwrite={() => setAskOverwrite(true)}
        onCancel={requestClose}
      />
      <div className="flex min-h-0 flex-1 items-center justify-center overflow-hidden">
        <EditorCanvas source={source} doc={doc} tool={tool} preset={preset} brush={brush} label={t("editor.canvas", { name: item.fileName })}
          onCommit={history.push} onNeedText={() => notify("editor.typeTextFirst")} />
      </div>
      <p className="ss-muted m-0 text-center text-xs" data-testid="editor-size">
        {t("editor.outputSize", { w: Math.round(out.w), h: Math.round(out.h) })}
      </p>
      {askOverwrite && (
        <Ask title={t("editor.overwriteTitle")} body={t("editor.overwriteBody", { dir: `${dataDir}\\edit-backups` })}
          confirm={t("editor.overwriteConfirm")} onConfirm={() => void save("overwrite")} onCancel={() => setAskOverwrite(false)} />
      )}
      {askDiscard && (
        <Ask title={t("editor.discardTitle")} body={t("editor.discardBody")} confirm={t("editor.discard")}
          onConfirm={close} onCancel={() => setAskDiscard(false)} />
      )}
    </div>
  );
}

function Ask(p: { title: string; body: string; confirm: string; onConfirm: () => void; onCancel: () => void }) {
  const t = useT();
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center" style={{ background: "color-mix(in srgb, black 50%, transparent)" }}>
      <div role="alertdialog" aria-modal="true" aria-label={p.title} className="ss-panel flex max-w-md flex-col gap-3 p-5"
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === "Escape") p.onCancel();
        }}>
        <h2 className="m-0 text-base font-semibold">{p.title}</h2>
        <p className="m-0 text-sm" data-testid="ask-body">{p.body}</p>
        <div className="flex justify-end gap-2">
          <button type="button" className="ss-btn" autoFocus onClick={p.onCancel}>{t("actions.cancel")}</button>
          <button type="button" className="ss-btn ss-btn-primary" onClick={p.onConfirm}>{p.confirm}</button>
        </div>
      </div>
    </div>
  );
}
