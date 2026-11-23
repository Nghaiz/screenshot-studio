// In-memory versions of the read commands over the mock catalog. Filters the UI does not send yet are
// REJECTED with `invalidArgument` (as ss-core does), never silently ignored.
import type { AppError } from "@bindings/AppError";
import type { CountBy } from "@bindings/CountBy";
import type { ImagePage } from "@bindings/ImagePage";
import type { ImageQuery } from "@bindings/ImageQuery";
import type { ImageSummary } from "@bindings/ImageSummary";
import type { TimelineBucket } from "@bindings/TimelineBucket";
import type { TimelineBucketSize } from "@bindings/TimelineBucketSize";

export const appError = (kind: AppError["kind"], message: string): AppError => ({ kind, message, detail: "mock IPC" });

function checkSupported(q: ImageQuery): void {
  const unsupported: [string, boolean][] = [
    ["text", !!q.text?.trim()],
    ["width/height", q.minWidth !== null || q.maxWidth !== null || q.minHeight !== null || q.maxHeight !== null],
    ["aspects", q.aspects.length > 0],
    ["minSize/maxSize", q.minSize !== null || q.maxSize !== null],
    ["tagIds", q.tagIds.length > 0],
    ["albumId", q.albumId !== null],
    ["color/categories", q.color !== null || q.categories.length > 0],
    ["hasText", q.hasText !== null],
  ];
  const hit = unsupported.find(([, set]) => set);
  if (hit) throw appError("invalidArgument", `filter \`${hit[0]}\` is not supported by the mock`);
  if (q.limit < 1 || q.limit > 500) throw appError("invalidArgument", "limit must be 1..500");
}

export function filterImages(all: readonly ImageSummary[], q: ImageQuery): ImageSummary[] {
  checkSupported(q);
  return all.filter(
    (i) =>
      (q.trash === "include" || (q.trash === "only" ? i.trashedAt !== null : i.trashedAt === null)) &&
      (q.includeMissing || !i.missing) &&
      (q.dateFrom === null || i.takenMs >= q.dateFrom) &&
      (q.dateTo === null || i.takenMs <= q.dateTo) &&
      (q.folders.length === 0 || q.folders.includes(i.folder)) &&
      (!q.favoriteOnly || i.favorite) &&
      i.rating >= q.minRating,
  );
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
  };
  const field = q.sort.field;
  if (field === "relevance") throw appError("invalidArgument", "sort `relevance` needs a text query");
  const k = key[field];
  const dir = field === "random" ? 1 : q.sort.dir === "asc" ? 1 : -1;
  return (a, b) => {
    const x = k(a);
    const y = k(b);
    return x < y ? -dir : x > y ? dir : a.id - b.id;
  };
}

/** Offset cursor (opaque to the UI, like the real keyset cursor). */
export function queryImages(all: readonly ImageSummary[], q: ImageQuery): ImagePage {
  const rows = filterImages(all, q).sort(compare(q));
  const offset = q.cursor === null ? 0 : Number.parseInt(q.cursor, 10);
  if (!Number.isInteger(offset) || offset < 0) throw appError("invalidArgument", "invalid cursor");
  const end = Math.min(rows.length, offset + q.limit);
  return { items: rows.slice(offset, end), total: rows.length, nextCursor: end < rows.length ? String(end) : null };
}

/** Local-time day/month buckets, oldest first, `startMs` = local midnight (as ss-core `get_timeline`). */
export function timeline(all: readonly ImageSummary[], q: ImageQuery, bucket: TimelineBucketSize): TimelineBucket[] {
  const counts = new Map<number, number>();
  for (const i of filterImages(all, q)) {
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
