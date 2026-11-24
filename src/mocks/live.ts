// Mock of the phase 4.1 commands the 4.2 UI sends: edit save (raw body + headers), convert, stitch, ZIP/PDF export
// jobs, clipboard image, recent items, PII boxes. Same deterministic catalog as the rest of the mock.
import type { AppError } from "@bindings/AppError";
import type { CommandMap } from "@bindings/commands";
import type { ImageSummary } from "@bindings/ImageSummary";
import type { JournalOp } from "@bindings/JournalOp";
import type { OpResult } from "@bindings/OpResult";
import type { PiiFinding } from "@bindings/PiiFinding";
import type { EventMap, EventName } from "../lib/events";
import { appError } from "./catalog";
import { MOCK_NOW_MS } from "./data";

/** Steps and spacing of a mock export job. */
export const EXPORT_STEPS = 4;
export const EXPORT_STEP_MS = 50;

const PNG_SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

/** Width and height from a PNG's IHDR chunk; throws when the bytes are not a PNG (as the real decoder would). */
export function pngSize(bytes: Uint8Array): { width: number; height: number } {
  if (bytes.length < 24 || PNG_SIGNATURE.some((b, i) => bytes[i] !== b)) throw appError("invalidArgument", "the body is not a PNG");
  const v = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return { width: v.getUint32(16), height: v.getUint32(20) };
}

export interface LiveDeps {
  images: ImageSummary[];
  byId: Map<number, ImageSummary>;
  get: (id: number) => ImageSummary;
  mutate: (op: JournalOp, ids: number[], apply: (img: ImageSummary) => void) => OpResult;
  changed: (reason: EventMap["library-changed"]["reason"], imageIds: number[] | null) => void;
  fire: <E extends EventName>(event: E, payload: EventMap[E]) => Promise<void>;
  timers: Set<ReturnType<typeof setTimeout>>;
  nextJobId: () => number;
}

export function createLive(d: LiveDeps) {
  const ocr = new Map<number, string>();
  const pii = new Map<number, PiiFinding[]>();
  const later = (ms: number, fn: () => void) => {
    const id = setTimeout(() => {
      d.timers.delete(id);
      fn();
    }, ms);
    d.timers.add(id);
  };
  const extOf = (f: string) => (f === "jpeg" ? "jpg" : f);
  const addCopy = (from: ImageSummary, patch: Partial<ImageSummary>): ImageSummary => {
    const id = Math.max(...d.images.map((i) => i.id)) + 1;
    const img: ImageSummary = { ...from, id, takenMs: MOCK_NOW_MS, mtimeMs: MOCK_NOW_MS, favorite: false, rating: 0, trashedAt: null, ...patch };
    d.images.push(img);
    d.byId.set(id, img);
    d.changed("fileOp", [id]);
    return img;
  };

  /** `save_edited_image`: `body` = PNG bytes, `headers` = the `x-*` headers the UI sent. */
  function saveEdited(body: unknown, headers: Record<string, string>): ImageSummary {
    if (!(body instanceof Uint8Array)) throw appError("invalidArgument", "save_edited_image takes the PNG bytes as a raw body");
    const id = Number(headers["x-image-id"]);
    const mode = headers["x-mode"];
    const format = headers["x-format"];
    if (!Number.isInteger(id)) throw appError("invalidArgument", "header x-image-id is missing");
    if (mode !== "copy" && mode !== "overwrite") throw appError("invalidArgument", `header x-mode has an unknown value \`${mode}\``);
    if (!["png", "jpeg", "webp"].includes(format)) throw appError("invalidArgument", `header x-format has an unknown value \`${format}\``);
    const size = pngSize(body);
    const src = d.get(id);
    if (mode === "overwrite") {
      d.mutate("editOverwrite", [id], (i) => Object.assign(i, size, { mtimeMs: i.mtimeMs + 1, sizeBytes: body.length }));
      return { ...d.get(id) };
    }
    const stem = src.fileName.replace(/\.[^.]+$/, "");
    return addCopy(src, { ...size, fileName: `${stem} (edited).${extOf(format)}`, sizeBytes: body.length });
  }

  const handlers: { [K in keyof CommandMap]?: (req: CommandMap[K]["req"]) => CommandMap[K]["res"] } = {
    convert_images: ({ imageIds, format, output }) => {
      imageIds.forEach(d.get);
      if (output === "overwrite") return d.mutate("convert", imageIds, (i) => (i.mtimeMs += 1));
      imageIds.forEach((id) => {
        const src = d.get(id);
        addCopy(src, { fileName: `${src.fileName.replace(/\.[^.]+$/, "")}.${extOf(format)}` });
      });
      return { batchId: null, affected: imageIds.length, failed: [] };
    },
    stitch_images: ({ imageIds, direction, gap }) => {
      if (imageIds.length < 2) throw appError("invalidArgument", "stitch needs at least 2 images");
      const parts = imageIds.map(d.get);
      const w = parts.map((p) => p.width ?? 0);
      const h = parts.map((p) => p.height ?? 0);
      const sum = (a: number[]) => a.reduce((x, y) => x + y, 0) + gap * (a.length - 1);
      const size = direction === "vertical" ? { width: Math.max(...w), height: sum(h) } : { width: sum(w), height: Math.max(...h) };
      return addCopy(parts[0], { ...size, fileName: `${parts[0].fileName.replace(/\.[^.]+$/, "")} (stitched).png` });
    },
    export_zip: ({ imageIds, destPath }) => exportJob(imageIds, destPath),
    export_pdf: ({ imageIds, destPath, perPage }) => {
      if (![1, 2, 4].includes(perPage)) throw appError("invalidArgument", `perPage must be 1, 2 or 4, got ${perPage}`);
      return exportJob(imageIds, destPath);
    },
    copy_image_to_clipboard: ({ imageId }) => {
      d.get(imageId);
      return null;
    },
    get_recent: ({ limit }) =>
      [...d.images].filter((i) => !i.missing && i.trashedAt === null).sort((a, b) => b.takenMs - a.takenMs).slice(0, limit),
    detect_pii: ({ imageId }) => {
      d.get(imageId);
      return { findings: pii.get(imageId) ?? [] };
    },
  };

  function exportJob(imageIds: number[], destPath: string) {
    if (imageIds.length === 0) throw appError("invalidArgument", "nothing to export");
    if (!destPath) throw appError("invalidArgument", "no destination");
    imageIds.forEach(d.get);
    const jobId = d.nextJobId();
    const total = imageIds.length;
    for (let step = 1; step <= EXPORT_STEPS; step++) {
      later(step * EXPORT_STEP_MS, () => {
        const finished = step === EXPORT_STEPS;
        void d.fire("job-progress", {
          jobId, kind: "export", stage: finished ? "done" : "write", done: Math.round((total * step) / EXPORT_STEPS), total,
          currentPath: null, finished, error: null as AppError | null,
        });
      });
    }
    return { jobId };
  }

  return {
    handlers,
    saveEdited,
    ocrText: (id: number) => ocr.get(id) ?? null,
    setOcr: (id: number, text: string) => {
      ocr.set(id, text);
      d.get(id).hasText = text.trim().length > 0;
    },
    setPii: (id: number, findings: PiiFinding[]) => void pii.set(id, findings),
  };
}
