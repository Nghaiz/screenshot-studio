// Editor toolbar: tools, crop presets, orientation, colour/text, auto-redact (gated on the sidecar's `pii`),
// undo/redo, output format and the two save buttons.
import type { ImageFormat } from "@bindings/ImageFormat";
import { gateReason } from "../components/AiNotice";
import { useAiGate } from "../data/smart";
import { useT, type MessageKey } from "../i18n";
import type { Brush, Tool } from "./EditorCanvas";
import type { History } from "./history";
import { CROP_PRESETS, orient, presetCrop, type CropPreset, type EditDoc, type OrientOp } from "./model";

const TOOLS: Tool[] = ["crop", "arrow", "box", "text", "highlight", "step", "blur", "pixelate"];
const ORIENT: { op: OrientOp; key: MessageKey; icon: string }[] = [
  { op: "rotateCcw", key: "editor.rotateCcw", icon: "⟲" },
  { op: "rotateCw", key: "editor.rotateCw", icon: "⟳" },
  { op: "flipH", key: "editor.flipH", icon: "⇋" },
  { op: "flipV", key: "editor.flipV", icon: "⇵" },
];
const FORMATS: ImageFormat[] = ["png", "jpeg", "webp"];
const PRESETS = Object.keys(CROP_PRESETS) as CropPreset[];

interface Props {
  name: string;
  doc: EditDoc;
  tool: Tool;
  preset: CropPreset;
  brush: Brush;
  format: ImageFormat;
  busy: boolean;
  history: History<EditDoc>;
  onTool: (t: Tool) => void;
  onPreset: (p: CropPreset) => void;
  onBrush: (b: Brush) => void;
  onFormat: (f: ImageFormat) => void;
  onDoc: (d: EditDoc) => void;
  onAutoRedact: () => void;
  onSaveCopy: () => void;
  onOverwrite: () => void;
  onCancel: () => void;
}

export function EditorToolbar(p: Props) {
  const t = useT();
  const pii = useAiGate("pii");
  const pickPreset = (preset: CropPreset) => {
    p.onPreset(preset);
    p.onTool("crop");
    const ratio = CROP_PRESETS[preset];
    if (ratio !== null) p.onDoc({ ...p.doc, crop: presetCrop(p.doc, ratio) });
  };
  return (
    <div className="ss-panel flex flex-col gap-2 px-3 py-2 text-xs">
      <div className="flex flex-wrap items-center gap-1.5">
        <h2 className="m-0 mr-2 min-w-0 max-w-64 truncate text-sm font-semibold">{p.name}</h2>
        <span role="group" aria-label={t("editor.tools")} className="flex flex-wrap gap-1">
          {TOOLS.map((tool) => (
            <button key={tool} type="button" className="ss-btn" aria-pressed={p.tool === tool} onClick={() => p.onTool(tool)}>
              {t(`editor.tool.${tool}`)}
            </button>
          ))}
        </span>
        <span className="flex-1" />
        <button type="button" className="ss-btn" disabled={!p.history.canUndo} onClick={p.history.undo} title="Ctrl+Z">{t("editor.undo")}</button>
        <button type="button" className="ss-btn" disabled={!p.history.canRedo} onClick={p.history.redo} title="Ctrl+Y">{t("editor.redo")}</button>
      </div>
      <div className="flex flex-wrap items-center gap-1.5">
        <span role="group" aria-label={t("editor.cropPresets")} className="flex gap-1">
          {PRESETS.map((preset) => (
            <button key={preset} type="button" className="ss-btn" aria-pressed={p.tool === "crop" && p.preset === preset} onClick={() => pickPreset(preset)}>
              {preset === "free" ? t("editor.cropFree") : preset}
            </button>
          ))}
          <button type="button" className="ss-btn" disabled={!p.doc.crop} onClick={() => p.onDoc({ ...p.doc, crop: null })}>{t("editor.cropClear")}</button>
        </span>
        <span role="group" aria-label={t("editor.orientation")} className="flex gap-1">
          {ORIENT.map((o) => (
            <button key={o.op} type="button" className="ss-btn" aria-label={t(o.key)} title={t(o.key)} onClick={() => p.onDoc(orient(p.doc, o.op))}>{o.icon}</button>
          ))}
        </span>
        <input type="color" aria-label={t("editor.color")} value={p.brush.color} onChange={(e) => p.onBrush({ ...p.brush, color: e.target.value })} />
        <input className="ss-input w-40" aria-label={t("editor.text")} placeholder={t("editor.textPlaceholder")} value={p.brush.text}
          onChange={(e) => p.onBrush({ ...p.brush, text: e.target.value })} />
        <span title={gateReason(t, pii) ?? undefined} data-ai-block={pii.block ?? undefined}>
          <button type="button" className="ss-btn" disabled={!pii.enabled} onClick={p.onAutoRedact}>{t("editor.autoRedact")}</button>
        </span>
        <span className="flex-1" />
        <label className="flex items-center gap-1">
          {t("editor.format")}
          <select className="ss-input" value={p.format} onChange={(e) => p.onFormat(e.target.value as ImageFormat)}>
            {FORMATS.map((f) => <option key={f} value={f}>{f.toUpperCase()}</option>)}
          </select>
        </label>
        <button type="button" className="ss-btn" onClick={p.onCancel}>{t("actions.cancel")}</button>
        <button type="button" className="ss-btn" disabled={p.busy} onClick={p.onOverwrite}>{t("editor.overwrite")}</button>
        <button type="button" className="ss-btn ss-btn-primary" disabled={p.busy} onClick={p.onSaveCopy}>{t("editor.saveCopy")}</button>
      </div>
    </div>
  );
}
