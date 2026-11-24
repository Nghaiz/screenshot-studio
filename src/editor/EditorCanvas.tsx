// The editing surface: the whole oriented image on a downscaled canvas (MAX_EDIT_EDGE), the crop as a dimmed
// overlay, and pointer drawing for the active tool. Canvas pixels map to oriented pixels through `scale`.
import { useEffect, useRef, useState, type PointerEvent } from "react";
import { CROP_PRESETS, dragRect, isTiny, nextStep, orientedSize, type CropPreset, type EditDoc, type Shape } from "./model";
import { renderDoc } from "./render";
import type { EditSource } from "./source";

/** Longest edge of the on-screen editing canvas, px; the save renders at full size (phase 4.2 risk). */
export const MAX_EDIT_EDGE = 1600;

export type Tool = "crop" | "arrow" | "box" | "highlight" | "text" | "step" | "blur" | "pixelate";

export interface Brush {
  color: string;
  /** Text for the text tool (typed in the toolbar, placed by a click). */
  text: string;
}

interface Props {
  source: EditSource;
  doc: EditDoc;
  tool: Tool;
  preset: CropPreset;
  brush: Brush;
  label: string;
  onCommit: (doc: EditDoc) => void;
  /** A click with the text tool while the text is empty. */
  onNeedText: () => void;
}

type Pt = { x: number; y: number };

export const displayScale = (doc: EditDoc) => {
  const { w, h } = orientedSize(doc);
  return Math.min(1, MAX_EDIT_EDGE / Math.max(w, h));
};

/** Stroke width and marker size that read well at the image's size. */
export function strokeFor(doc: EditDoc): { width: number; text: number; step: number } {
  const { w, h } = orientedSize(doc);
  const side = Math.min(w, h);
  return { width: Math.max(3, Math.round(side / 220)), text: Math.max(16, Math.round(side / 28)), step: Math.max(24, Math.round(side / 22)) };
}

export function shapeFor(doc: EditDoc, tool: Tool, a: Pt, b: Pt, brush: Brush): Shape | null {
  const k = strokeFor(doc);
  if (tool === "arrow") {
    return Math.hypot(b.x - a.x, b.y - a.y) < 4 ? null : { kind: "arrow", x1: a.x, y1: a.y, x2: b.x, y2: b.y, color: brush.color, width: k.width };
  }
  if (tool === "text") return brush.text.trim() ? { kind: "text", x: b.x, y: b.y, text: brush.text.trim(), color: brush.color, size: k.text } : null;
  if (tool === "step") return { kind: "step", x: b.x, y: b.y, n: nextStep(doc), color: brush.color, size: k.step };
  if (tool === "crop") return null;
  const rect = dragRect(doc, a, b);
  if (isTiny(rect)) return null;
  if (tool === "box") return { kind: "box", rect, color: brush.color, width: k.width };
  if (tool === "highlight") return { kind: "highlight", rect, color: brush.color };
  return { kind: tool, rect };
}

/** Applies a finished drag (or click) with `tool` to `doc`; null when it changes nothing. */
export function applyGesture(doc: EditDoc, tool: Tool, preset: CropPreset, a: Pt, b: Pt, brush: Brush): EditDoc | null {
  if (tool === "crop") {
    const rect = dragRect(doc, a, b, CROP_PRESETS[preset]);
    return isTiny(rect) ? null : { ...doc, crop: rect };
  }
  const shape = shapeFor(doc, tool, a, b, brush);
  return shape ? { ...doc, shapes: [...doc.shapes, shape] } : null;
}

export function EditorCanvas({ source, doc, tool, preset, brush, label, onCommit, onNeedText }: Props) {
  const ref = useRef<HTMLCanvasElement>(null);
  const [drag, setDrag] = useState<{ a: Pt; b: Pt } | null>(null);
  const scale = displayScale(doc);

  const shown = drag ? (applyGesture(doc, tool, preset, drag.a, drag.b, brush) ?? doc) : doc;
  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    renderDoc(canvas, source.image, shown, { scale, cropped: false });
    if (shown.crop) drawCropOverlay(canvas, shown, scale);
  }, [source, shown, scale]);

  const toDoc = (e: PointerEvent<HTMLCanvasElement>): Pt => {
    const canvas = e.currentTarget;
    const r = canvas.getBoundingClientRect();
    const sx = canvas.width / r.width;
    const sy = canvas.height / r.height;
    return { x: ((e.clientX - r.left) * sx) / scale, y: ((e.clientY - r.top) * sy) / scale };
  };

  return (
    <canvas
      ref={ref}
      role="img"
      aria-label={label}
      data-testid="editor-canvas"
      className="max-h-full max-w-full touch-none rounded"
      style={{ cursor: tool === "text" || tool === "step" ? "copy" : "crosshair", boxShadow: "var(--ss-shadow)" }}
      onPointerDown={(e) => {
        e.currentTarget.setPointerCapture?.(e.pointerId);
        const p = toDoc(e);
        setDrag({ a: p, b: p });
      }}
      onPointerMove={(e) => drag && setDrag({ a: drag.a, b: toDoc(e) })}
      onPointerUp={(e) => {
        if (!drag) return;
        const b = toDoc(e);
        setDrag(null);
        if (tool === "text" && !brush.text.trim()) return onNeedText();
        const next = applyGesture(doc, tool, preset, drag.a, b, brush);
        if (next) onCommit(next);
      }}
      onPointerCancel={() => setDrag(null)}
    />
  );
}

/** Dims everything outside the crop and outlines it. */
function drawCropOverlay(canvas: HTMLCanvasElement, doc: EditDoc, scale: number) {
  const ctx = canvas.getContext("2d");
  if (!ctx || !doc.crop) return;
  const c = doc.crop;
  const [x, y, w, h] = [c.x * scale, c.y * scale, c.w * scale, c.h * scale];
  ctx.save();
  ctx.fillStyle = "rgb(0 0 0 / 0.55)";
  ctx.beginPath();
  ctx.rect(0, 0, canvas.width, canvas.height);
  ctx.rect(x, y, w, h);
  ctx.fill("evenodd");
  ctx.strokeStyle = "#ffffff";
  ctx.lineWidth = 2;
  ctx.setLineDash([8, 6]);
  ctx.strokeRect(x, y, w, h);
  ctx.restore();
}
