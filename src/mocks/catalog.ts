// In-memory versions of the read commands over the mock catalog, with the filter semantics of
// `ss-core::filter` (phase 2.1): empty array / null = no constraint, `folders` matches a folder and below,
// `tagMatch` any/all, colour = RGB distance <= tolerance% of the largest, `semantic` needs the sidecar.
// The mock has no OCR text and no categories, so `ocr` text, `hasText` and `categories` filters match nothing.
import type { AppError } from "@bindings/AppError";
import type { CountBy } from "@bindings/CountBy";
import type { ImagePage } from "@bindings/ImagePage";
import type { ImageQuery } from "@bindings/ImageQuery";
import type { ImageSummary } from "@bindings/ImageSummary";
import type { TimelineBucket } from "@bindings/TimelineBucket";
import type { TimelineBucketSize } from "@bindings/TimelineBucketSize";

export const appError = (kind: AppError["kind"], message: string): AppError => ({ kind, message, detail: "mock IPC" });

/** Organize data the filters need (tags and album membership live in the mock's organize state). */
export interface FilterContext {
  tagsOf: (imageId: number) => ReadonlySet<number>;
  albumMembers: (albumId: number) => readonly number[] | null;
  noteOf: (imageId: number) => string | null;
}

const NO_ORGANIZE: FilterContext = { tagsOf: () => new Set(), albumMembers: () => null, noteOf: () => null };
const MAX_RGB_DISTANCE = Math.sqrt(3 * 255 * 255);

export const fold = (s: string) => s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();

function rgb(hex: string): [number, number, number] | null {
  const m = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex);
  return m ? [parseInt(m[1], 16), parseInt(m[2], 16), parseInt(m[3], 16)] : null;
}

function textMatcher(q: ImageQuery, ctx: FilterContext): ((i: ImageSummary) => boolean) | null {
  const text = q.text?.trim();
  if (!text) return null;
  if (q.textMode === "semantic") throw appError("sidecarUnavailable", "semantic search needs the AI sidecar");
  const words = fold(text).split(/\s+/).filter((w) => /[\p{L}\p{N}]/u.test(w));
  if (words.length === 0) return () => false;
  return (i) => {
    const fields = q.textMode === "ocr" ? [] : q.textMode === "filename" ? [i.fileName] : [i.fileName, ctx.noteOf(i.id) ?? ""];
    const hay = fold(fields.join(" "));
    return words.every((w) => hay.includes(w));
  };
}

function inFolder(folder: string, wanted: readonly string[]): boolean {
  const f = folder.toLowerCase();
  return wanted.some((w) => (w === "" ? f === "" : f === w.toLowerCase() || f.startsWith(`${w.toLowerCase()}/`) || f.startsWith(`${w.toLowerCase()}\\`)));
}

export function filterImages(all: readonly ImageSummary[], q: ImageQuery, ctx: FilterContext = NO_ORGANIZE): ImageSummary[] {
  if (q.limit < 1 || q.limit > 500) throw appError("invalidArgument", "limit must be 1..500");
  if (q.colorTolerance < 0 || q.colorTolerance > 100) throw appError("invalidArgument", "colorTolerance must be 0..100");
  const text = textMatcher(q, ctx);
  const color = q.color === null ? null : rgb(q.color);
  if (q.color !== null && !color) throw appError("invalidArgument", `invalid colour ${q.color}`);
  const album = q.albumId === null ? null : new Set(ctx.albumMembers(q.albumId) ?? []);
  const between = (v: number | null, lo: number | null, hi: number | null) =>
    (lo === null || (v !== null && v >= lo)) && (hi === null || (v !== null && v <= hi));
  return all.filter((i) => {
    const tags = ctx.tagsOf(i.id);
    return (
      (q.trash === "include" || (q.trash === "only" ? i.trashedAt !== null : i.trashedAt === null)) &&
      (q.includeMissing || !i.missing) &&
      between(i.takenMs, q.dateFrom, q.dateTo) &&
      between(i.width, q.minWidth, q.maxWidth) &&
      between(i.height, q.minHeight, q.maxHeight) &&
      between(i.sizeBytes, q.minSize, q.maxSize) &&
      (q.aspects.length === 0 || (i.aspect !== null && q.aspects.includes(i.aspect))) &&
      (q.folders.length === 0 || inFolder(i.folder, q.folders)) &&
      (q.tagIds.length === 0 || (q.tagMatch === "all" ? q.tagIds.every((t) => tags.has(t)) : q.tagIds.some((t) => tags.has(t)))) &&
      (album === null || album.has(i.id)) &&
      (color === null || colorClose(i.dominantRgb, color, q.colorTolerance)) &&
      q.categories.length === 0 &&
      (q.hasText === null || i.hasText === q.hasText) &&
      (!q.favoriteOnly || i.favorite) &&
      i.rating >= q.minRating &&
      (text === null || text(i))
    );
  });
}

function colorClose(hex: string | null, want: [number, number, number], tolerance: number): boolean {
  const c = hex ? rgb(hex) : null;
  if (!c) return false;
  const d = Math.hypot(c[0] - want[0], c[1] - want[1], c[2] - want[2]);
  return d <= (tolerance / 100) * MAX_RGB_DISTANCE;
}

const hash = (id: number) => Math.imul(id ^ 0x9e3779b9, 0x85ebca6b) >>> 0;

function compare(q: ImageQuery): (a: ImageSummary, b: ImageSummary) => number {
  const key: Record<string, (i: ImageSummary) => number | string> = {
    taken: (i) => i.takenMs,
    name: (i) => i.fileName.toLowerCase(),
    size: (i) => i.sizeBytes,
    width: (i) => i.width ?? 0,
    rating: (i) => i.rating,
    random: (i) => hash(i.id),
    // The mock has no FTS rank: shorter file names first stands in for relevance.
    relevance: (i) => -i.fileName.length,
  };
  const field = q.sort.field;
  if (field === "relevance" && !q.text?.trim()) throw appError("invalidArgument", "sort `relevance` needs a text query");
  const k = key[field];
  const dir = field === "random" ? 1 : q.sort.dir === "asc" ? 1 : -1;
  return (a, b) => {
    const x = k(a);
    const y = k(b);
    return x < y ? -dir : x > y ? dir : a.id - b.id;
  };
}

/** Offset cursor (opaque to the UI, like the real keyset cursor). */
export function queryImages(all: readonly ImageSummary[], q: ImageQuery, ctx?: FilterContext): ImagePage {
  const rows = filterImages(all, q, ctx).sort(compare(q));
  const offset = q.cursor === null ? 0 : Number.parseInt(q.cursor, 10);
  if (!Number.isInteger(offset) || offset < 0) throw appError("invalidArgument", "invalid cursor");
  const end = Math.min(rows.length, offset + q.limit);
  return { items: rows.slice(offset, end), total: rows.length, nextCursor: end < rows.length ? String(end) : null };
}

/** Local-time day/month buckets, oldest first, `startMs` = local midnight (as ss-core `get_timeline`). */
export function timeline(all: readonly ImageSummary[], q: ImageQuery, bucket: TimelineBucketSize, ctx?: FilterContext): TimelineBucket[] {
  const counts = new Map<number, number>();
  for (const i of filterImages(all, q, ctx)) {
    const d = new Date(i.takenMs);
    const start = bucket === "month" ? new Date(d.getFullYear(), d.getMonth(), 1) : new Date(d.getFullYear(), d.getMonth(), d.getDate());
    counts.set(start.getTime(), (counts.get(start.getTime()) ?? 0) + 1);
  }
  return [...counts.entries()].sort((a, b) => a[0] - b[0]).map(([startMs, count]) => ({ startMs, count }));
}

export function countBy(items: readonly ImageSummary[], key: (i: ImageSummary) => string): CountBy[] {
  const m = new Map<string, number>();
  for (const i of items) m.set(key(i), (m.get(key(i)) ?? 0) + 1);
  return [...m.entries()].sort((a, b) => b[1] - a[1]).map(([k, count]) => ({ key: k, count }));
}
