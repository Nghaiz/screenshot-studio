// Draws an EditDoc with Canvas 2D. The editor shows the whole oriented image at a downscaled `scale` (crop drawn
// as an overlay by the editor); the save path renders only the crop at scale 1, once (phase 4.2 risk: large images).
import { orientedSize, outputRect, type EditDoc, type Rect, type Shape } from "./model";

/** Blur radius and pixel block size, as a fraction of the region's shorter side (min 4 px). */
const BLUR_FRACTION = 0.08;
const PIXEL_FRACTION = 0.06;
const MIN_EFFECT_PX = 4;
/** Highlighter opacity. */
const HIGHLIGHT_ALPHA = 0.35;

export interface RenderOptions {
  /** Output pixels per oriented pixel. */
  scale: number;
  /** true: only the crop (the saved image); false: the whole oriented image. */
  cropped: boolean;
}

export function context2d(canvas: HTMLCanvasElement): CanvasRenderingContext2D {
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("editor: this webview has no 2D canvas context");
  return ctx;
}

/** Sizes `canvas` and draws `doc` on it. */
export function renderDoc(canvas: HTMLCanvasElement, source: CanvasImageSource, doc: EditDoc, opts: RenderOptions): void {
  const full = orientedSize(doc);
  const area: Rect = opts.cropped ? outputRect(doc) : { x: 0, y: 0, w: full.w, h: full.h };
  canvas.width = Math.max(1, Math.round(area.w * opts.scale));
  canvas.height = Math.max(1, Math.round(area.h * opts.scale));
  const ctx = context2d(canvas);
  // Oriented pixel (x, y) -> canvas pixel ((x - area.x) * scale, (y - area.y) * scale).
  const base = () => ctx.setTransform(opts.scale, 0, 0, opts.scale, -area.x * opts.scale, -area.y * opts.scale);

  base();
  ctx.translate(full.w / 2, full.h / 2);
  const [a, b, c, d] = doc.orient;
  ctx.transform(a, c, b, d, 0, 0);
  ctx.drawImage(source, -doc.srcW / 2, -doc.srcH / 2, doc.srcW, doc.srcH);

  for (const s of doc.shapes) {
    base();
    drawShape(ctx, s, opts.scale, area);
  }
  ctx.setTransform(1, 0, 0, 1, 0, 0);
}

/** Draws one shape; `ctx` is in oriented pixels. Blur and pixelate act on what is already drawn under them. */
export function drawShape(ctx: CanvasRenderingContext2D, s: Shape, scale: number, area: Rect): void {
  switch (s.kind) {
    case "arrow":
      return drawArrow(ctx, s.x1, s.y1, s.x2, s.y2, s.color, s.width);
    case "box":
      ctx.strokeStyle = s.color;
      ctx.lineWidth = s.width;
      ctx.strokeRect(s.rect.x, s.rect.y, s.rect.w, s.rect.h);
      return;
    case "highlight":
      ctx.globalAlpha = HIGHLIGHT_ALPHA;
      ctx.fillStyle = s.color;
      ctx.fillRect(s.rect.x, s.rect.y, s.rect.w, s.rect.h);
      ctx.globalAlpha = 1;
      return;
    case "text":
      ctx.fillStyle = s.color;
      ctx.font = `600 ${s.size}px system-ui, sans-serif`;
      ctx.textBaseline = "top";
      ctx.fillText(s.text, s.x, s.y);
      return;
    case "step":
      ctx.fillStyle = s.color;
      ctx.beginPath();
      ctx.arc(s.x, s.y, s.size / 2, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = "#ffffff";
      ctx.font = `700 ${Math.round(s.size * 0.55)}px system-ui, sans-serif`;
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText(String(s.n), s.x, s.y);
      ctx.textAlign = "start";
      return;
    case "blur":
    case "pixelate":
      return obscure(ctx, s.kind, s.rect, scale, area);
  }
}

function drawArrow(ctx: CanvasRenderingContext2D, x1: number, y1: number, x2: number, y2: number, color: string, width: number) {
  const angle = Math.atan2(y2 - y1, x2 - x1);
  const head = width * 4;
  ctx.strokeStyle = color;
  ctx.fillStyle = color;
  ctx.lineWidth = width;
  ctx.lineCap = "round";
  ctx.beginPath();
  ctx.moveTo(x1, y1);
  ctx.lineTo(x2 - Math.cos(angle) * head * 0.8, y2 - Math.sin(angle) * head * 0.8);
  ctx.stroke();
  ctx.beginPath();
  ctx.moveTo(x2, y2);
  ctx.lineTo(x2 - head * Math.cos(angle - Math.PI / 7), y2 - head * Math.sin(angle - Math.PI / 7));
  ctx.lineTo(x2 - head * Math.cos(angle + Math.PI / 7), y2 - head * Math.sin(angle + Math.PI / 7));
  ctx.closePath();
  ctx.fill();
}

/** Copies the region (canvas pixels) to a scratch canvas, then draws it back blurred or as coarse blocks. */
function obscure(ctx: CanvasRenderingContext2D, kind: "blur" | "pixelate", r: Rect, scale: number, area: Rect) {
  const sx = Math.round((r.x - area.x) * scale);
  const sy = Math.round((r.y - area.y) * scale);
  const sw = Math.max(1, Math.round(r.w * scale));
  const sh = Math.max(1, Math.round(r.h * scale));
  const side = Math.min(sw, sh);
  const scratch = document.createElement("canvas");
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  if (kind === "pixelate") {
    const block = Math.max(MIN_EFFECT_PX, Math.round(side * PIXEL_FRACTION));
    scratch.width = Math.max(1, Math.ceil(sw / block));
    scratch.height = Math.max(1, Math.ceil(sh / block));
    const sc = context2d(scratch);
    sc.drawImage(ctx.canvas, sx, sy, sw, sh, 0, 0, scratch.width, scratch.height);
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(scratch, 0, 0, scratch.width, scratch.height, sx, sy, sw, sh);
    ctx.imageSmoothingEnabled = true;
    return;
  }
  const radius = Math.max(MIN_EFFECT_PX, Math.round(side * BLUR_FRACTION));
  scratch.width = sw;
  scratch.height = sh;
  context2d(scratch).drawImage(ctx.canvas, sx, sy, sw, sh, 0, 0, sw, sh);
  ctx.save();
  ctx.beginPath();
  ctx.rect(sx, sy, sw, sh);
  ctx.clip();
  ctx.filter = `blur(${radius}px)`;
  // Drawn a little larger so the blur does not fade to transparent at the region's edges.
  ctx.drawImage(scratch, sx - radius, sy - radius, sw + radius * 2, sh + radius * 2);
  ctx.restore();
}

/** Renders the saved image (crop at full size) and encodes it as PNG bytes. */
export async function exportPng(source: CanvasImageSource, doc: EditDoc): Promise<Uint8Array> {
  const canvas = document.createElement("canvas");
  renderDoc(canvas, source, doc, { scale: 1, cropped: true });
  const blob = await new Promise<Blob>((resolve, reject) =>
    canvas.toBlob((b) => (b ? resolve(b) : reject(new Error("editor: the canvas could not be encoded as PNG"))), "image/png"),
  );
  return new Uint8Array(await blob.arrayBuffer());
}
