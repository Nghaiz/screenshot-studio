// Deterministic mock catalog: N images shaped like the real library (phone + PC screenshots, a few in
// sub-folders, a few cloud-only), plus SVG placeholder thumbnails as data URLs. Types come from @bindings.
import type { AspectClass } from "@bindings/AspectClass";
import type { ImageSummary } from "@bindings/ImageSummary";
import type { LibraryRoot } from "@bindings/LibraryRoot";
import { DEFAULT_LIBRARY_ROOT } from "../lib/defaults";

/** Fixed "now" so every run (and every test) sees the same catalog. 2026-11-23 12:00 UTC. */
export const MOCK_NOW_MS = Date.UTC(2026, 10, 23, 12, 0, 0);
export const MOCK_IMAGE_COUNT = 10_000;

/** mulberry32: small seeded PRNG. */
export function prng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const SHAPES: { w: number; h: number; aspect: AspectClass; weight: number }[] = [
  { w: 1080, h: 2400, aspect: "phone", weight: 35 },
  { w: 1920, h: 1080, aspect: "pc", weight: 35 },
  { w: 2560, h: 1440, aspect: "pc", weight: 10 },
  { w: 1280, h: 900, aspect: "other", weight: 10 },
  { w: 1080, h: 1080, aspect: "square", weight: 5 },
  { w: 3440, h: 1440, aspect: "wide", weight: 5 },
];
const TOTAL_WEIGHT = SHAPES.reduce((a, s) => a + s.weight, 0);

function pickShape(r: number) {
  let x = r * TOTAL_WEIGHT;
  for (const s of SHAPES) if ((x -= s.weight) < 0) return s;
  return SHAPES[0];
}

const pad = (n: number, w = 2) => String(n).padStart(w, "0");
const hex = (n: number) => Math.round(n).toString(16).padStart(2, "0");

export function mockRoot(imageCount: number): LibraryRoot {
  return { id: 1, path: DEFAULT_LIBRARY_ROOT, addedAt: MOCK_NOW_MS - 86_400_000, imageCount, protectedRoot: true };
}

export function generateImages(count: number, rootId: number, seed = 1123): ImageSummary[] {
  const rnd = prng(seed);
  const out: ImageSummary[] = [];
  let taken = MOCK_NOW_MS;
  for (let i = 0; i < count; i++) {
    taken -= Math.round((0.3 + rnd() * 5.7) * 3_600_000);
    const shape = pickShape(rnd());
    const d = new Date(taken);
    const folderRoll = rnd();
    const cloudOnly = rnd() < 0.004;
    const hue = Math.floor(rnd() * 360);
    out.push({
      id: i + 1,
      rootId,
      folder: folderRoll < 0.01 ? "1ERD" : folderRoll < 0.04 ? "4ERD" : "",
      fileName: `Screenshot ${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}${pad(d.getMinutes())}${pad(d.getSeconds())}.png`,
      sizeBytes: Math.round(shape.w * shape.h * (0.08 + rnd() * 0.25)),
      takenMs: taken,
      width: shape.w,
      height: shape.h,
      aspect: shape.aspect,
      cloudState: cloudOnly ? "cloudOnly" : "local",
      thumbState: cloudOnly ? "offline" : rnd() < 0.005 ? "pending" : "ok",
      favorite: rnd() < 0.03,
      rating: rnd() < 0.05 ? 1 + Math.floor(rnd() * 5) : 0,
      hasText: null,
      dominantRgb: hslHex(hue, 0.6, 0.45),
      trashedAt: null,
      missing: false,
      mtimeMs: taken,
    });
  }
  return out;
}

function hslHex(h: number, s: number, l: number): string {
  const k = (n: number) => (n + h / 30) % 12;
  const a = s * Math.min(l, 1 - l);
  const f = (n: number) => 255 * (l - a * Math.max(-1, Math.min(k(n) - 3, Math.min(9 - k(n), 1))));
  return `#${hex(f(0))}${hex(f(8))}${hex(f(4))}`.toUpperCase();
}

/** SVG placeholder whose long edge is `longEdge` px, coloured from the image's dominant colour. */
export function placeholderUrl(img: ImageSummary, longEdge: number): string {
  const w0 = img.width ?? 16;
  const h0 = img.height ?? 9;
  const k = longEdge / Math.max(w0, h0);
  const w = Math.max(1, Math.round(w0 * k));
  const h = Math.max(1, Math.round(h0 * k));
  const c = img.dominantRgb ?? "#556677";
  const svg =
    `<svg xmlns='http://www.w3.org/2000/svg' width='${w}' height='${h}' viewBox='0 0 ${w} ${h}'>` +
    `<defs><linearGradient id='g' x1='0' y1='0' x2='1' y2='1'><stop offset='0' stop-color='${c}'/>` +
    `<stop offset='1' stop-color='#111827'/></linearGradient></defs><rect width='100%' height='100%' fill='url(#g)'/>` +
    `<text x='50%' y='50%' fill='white' fill-opacity='.75' font-family='Segoe UI,sans-serif' font-size='${Math.round(h / 8)}' ` +
    `text-anchor='middle' dominant-baseline='middle'>#${img.id}</text></svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}
