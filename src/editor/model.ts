// Edit document of the quick editor (Deferred decision #21: plain Canvas 2D). Pure data and pure functions.
//
// Geometry lives in ORIENTED full-resolution pixels: the source image after rotation/flip, before the crop. The
// orientation is a 2x2 integer matrix over centred coordinates (source -> oriented), so rotations and flips compose
// in any order; applying one maps the crop and every shape along with the image.

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export type ShapeKind = "arrow" | "box" | "highlight" | "text" | "step" | "blur" | "pixelate";

export type Shape =
  | { kind: "arrow"; x1: number; y1: number; x2: number; y2: number; color: string; width: number }
  | { kind: "box"; rect: Rect; color: string; width: number }
  | { kind: "highlight"; rect: Rect; color: string }
  | { kind: "text"; x: number; y: number; text: string; color: string; size: number }
  | { kind: "step"; x: number; y: number; n: number; color: string; size: number }
  | { kind: "blur"; rect: Rect }
  | { kind: "pixelate"; rect: Rect };

/** [m11, m12, m21, m22]: x' = m11 x + m12 y, y' = m21 x + m22 y (centred coordinates, y down). */
export type Orientation = readonly [number, number, number, number];
export const IDENTITY: Orientation = [1, 0, 0, 1];

export interface EditDoc {
  srcW: number;
  srcH: number;
  orient: Orientation;
  crop: Rect | null;
  shapes: Shape[];
}

export const newDoc = (srcW: number, srcH: number): EditDoc => ({ srcW, srcH, orient: IDENTITY, crop: null, shapes: [] });

/** Size of the oriented image (a quarter turn swaps the sides). */
export function orientedSize(doc: Pick<EditDoc, "srcW" | "srcH" | "orient">): { w: number; h: number } {
  return doc.orient[0] !== 0 ? { w: doc.srcW, h: doc.srcH } : { w: doc.srcH, h: doc.srcW };
}

/** The part of the oriented image that is saved: the crop, else all of it. */
export function outputRect(doc: EditDoc): Rect {
  const { w, h } = orientedSize(doc);
  return doc.crop ?? { x: 0, y: 0, w, h };
}

const mul = (a: Orientation, b: Orientation): Orientation => [
  a[0] * b[0] + a[1] * b[2], a[0] * b[1] + a[1] * b[3],
  a[2] * b[0] + a[3] * b[2], a[2] * b[1] + a[3] * b[3],
];

export type OrientOp = "rotateCw" | "rotateCcw" | "flipH" | "flipV";
const OPS: Record<OrientOp, Orientation> = {
  rotateCw: [0, -1, 1, 0],
  rotateCcw: [0, 1, -1, 0],
  flipH: [-1, 0, 0, 1],
  flipV: [1, 0, 0, -1],
};

type Pt = { x: number; y: number };
const apply = (m: Orientation, p: Pt): Pt => ({ x: m[0] * p.x + m[1] * p.y, y: m[2] * p.x + m[3] * p.y });

function mapRect(r: Rect, f: (p: Pt) => Pt): Rect {
  const a = f({ x: r.x, y: r.y });
  const b = f({ x: r.x + r.w, y: r.y + r.h });
  return { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y), w: Math.abs(b.x - a.x), h: Math.abs(b.y - a.y) };
}

function mapShape(s: Shape, f: (p: Pt) => Pt): Shape {
  switch (s.kind) {
    case "arrow": {
      const a = f({ x: s.x1, y: s.y1 });
      const b = f({ x: s.x2, y: s.y2 });
      return { ...s, x1: a.x, y1: a.y, x2: b.x, y2: b.y };
    }
    case "text":
    case "step": {
      const p = f({ x: s.x, y: s.y });
      return { ...s, x: p.x, y: p.y };
    }
    default:
      return { ...s, rect: mapRect(s.rect, f) };
  }
}

/** Rotates or flips the image; the crop and every shape follow it. */
export function orient(doc: EditDoc, op: OrientOp): EditDoc {
  const m = OPS[op];
  const before = orientedSize(doc);
  const next = { ...doc, orient: mul(m, doc.orient) };
  const after = orientedSize(next);
  const f = (p: Pt) => {
    const q = apply(m, { x: p.x - before.w / 2, y: p.y - before.h / 2 });
    return { x: q.x + after.w / 2, y: q.y + after.h / 2 };
  };
  return { ...next, crop: doc.crop && mapRect(doc.crop, f), shapes: doc.shapes.map((s) => mapShape(s, f)) };
}

/** Maps a box in SOURCE pixels (e.g. a `detect_pii` finding) into oriented pixels. */
export function sourceRectToOriented(doc: EditDoc, r: Rect): Rect {
  const o = orientedSize(doc);
  return mapRect(r, (p) => {
    const q = apply(doc.orient, { x: p.x - doc.srcW / 2, y: p.y - doc.srcH / 2 });
    return { x: q.x + o.w / 2, y: q.y + o.h / 2 };
  });
}

/** Crop presets: free (no ratio) or width:height. */
export const CROP_PRESETS = { free: null, "16:9": 16 / 9, "9:16": 9 / 16, "1:1": 1 } as const;
export type CropPreset = keyof typeof CROP_PRESETS;

/** The largest centred rect of `ratio` inside the oriented image (whole pixels). */
export function presetCrop(doc: EditDoc, ratio: number): Rect {
  const { w, h } = orientedSize(doc);
  const cw = Math.min(w, Math.round(h * ratio));
  const ch = Math.min(h, Math.round(cw / ratio));
  return { x: Math.round((w - cw) / 2), y: Math.round((h - ch) / 2), w: cw, h: ch };
}

/** A rect from two drag points, clamped to the image, optionally held to `ratio`; whole pixels. */
export function dragRect(doc: EditDoc, a: Pt, b: Pt, ratio: number | null = null): Rect {
  const { w, h } = orientedSize(doc);
  const clamp = (v: number, max: number) => Math.min(max, Math.max(0, v));
  let dx = clamp(b.x, w) - a.x;
  let dy = clamp(b.y, h) - a.y;
  if (ratio !== null) {
    const len = Math.min(Math.abs(dx), Math.abs(dy) * ratio);
    dx = Math.sign(dx || 1) * len;
    dy = Math.sign(dy || 1) * (len / ratio);
  }
  return mapRect({ x: a.x, y: a.y, w: dx, h: dy }, (p) => ({ x: Math.round(clamp(p.x, w)), y: Math.round(clamp(p.y, h)) }));
}

/** Rects smaller than this (px) are a click, not a drag, and make no shape. */
export const MIN_DRAG_PX = 4;
export const isTiny = (r: Rect) => r.w < MIN_DRAG_PX || r.h < MIN_DRAG_PX;

/** The next number of a numbered-steps marker. */
export const nextStep = (doc: EditDoc) => doc.shapes.filter((s) => s.kind === "step").length + 1;
