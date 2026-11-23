// Mock backend for the UI lane (phase 1.3): `mockIPC` from @tauri-apps/api/mocks answering every command
// the browse UI sends, over a deterministic 10,000-image catalog. Used by the tests and by
// `VITE_MOCK_IPC=1 npm run dev` in a plain browser. Request/response types come from @bindings only.
import { clearMocks, mockConvertFileSrc, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { CommandMap } from "@bindings/commands";
import type { ImageSummary } from "@bindings/ImageSummary";
import type { LibraryRoot } from "@bindings/LibraryRoot";
import type { OpResult } from "@bindings/OpResult";
import { PREVIEW_WIDTH } from "../lib/defaults";
import type { EventMap, EventName } from "../lib/events";
import { setImageUrlOverride } from "../lib/ssimg";
import { appError, countBy, queryImages, timeline } from "./catalog";
import { generateImages, MOCK_IMAGE_COUNT, MOCK_NOW_MS, mockRoot, placeholderUrl } from "./data";

export interface MockCall {
  cmd: string;
  payload: unknown;
}

export interface MockHandle {
  /** Every command the UI invoked, in order (event-plugin traffic excluded). */
  calls: MockCall[];
  images: ImageSummary[];
  emit: <E extends EventName>(event: E, payload: EventMap[E]) => Promise<void>;
  uninstall: () => void;
}

export interface MockOptions {
  count?: number;
  firstRun?: boolean;
}

type Journal = { batchId: number; undo: () => void };
type Handler = (req: never) => unknown;

export function installMockIpc(opts: MockOptions = {}): MockHandle {
  const count = opts.count ?? MOCK_IMAGE_COUNT;
  const images = generateImages(count, 1);
  const byId = new Map(images.map((i) => [i.id, i]));
  const roots: LibraryRoot[] = opts.firstRun ? [] : [mockRoot(count)];
  const settings: Record<string, unknown> = {};
  const journal: Journal[] = [];
  const calls: MockCall[] = [];
  const timers = new Set<ReturnType<typeof setTimeout>>();
  let batchSeq = 0;
  let jobSeq = 0;

  const fire = <E extends EventName>(event: E, payload: EventMap[E]) => emit(event, payload);
  const changed = (reason: EventMap["library-changed"]["reason"], imageIds: number[] | null) =>
    void fire("library-changed", { reason, imageIds });
  const get = (id: number) => {
    const img = byId.get(id);
    if (!img) throw appError("notFound", `image ${id} not found`);
    return img;
  };

  /** Applies `apply` to each id, journals one undoable batch restoring the previous values. */
  function mutate(ids: number[], apply: (img: ImageSummary) => void): OpResult {
    const before = ids.map((id) => ({ ...get(id) }));
    ids.forEach((id) => apply(get(id)));
    const batchId = ++batchSeq;
    journal.push({ batchId, undo: () => before.forEach((b) => Object.assign(get(b.id), b)) });
    changed("fileOp", ids);
    return { batchId, affected: ids.length, failed: [] };
  }

  const visible = () => images.filter((i) => !i.missing);
  const counts = () => {
    const v = visible();
    return {
      total: v.length, totalBytes: v.reduce((a, i) => a + i.sizeBytes, 0),
      thumbOk: v.filter((i) => i.thumbState === "ok").length, thumbPending: v.filter((i) => i.thumbState === "pending").length,
      cloudOnly: v.filter((i) => i.cloudState === "cloudOnly").length, missing: images.length - v.length,
      trashed: v.filter((i) => i.trashedAt !== null).length, roots,
    };
  };

  const handlers: { [K in keyof CommandMap]?: (req: CommandMap[K]["req"]) => CommandMap[K]["res"] } = {
    get_app_info: () => ({
      version: "0.1.0-mock", contractVersion: 1, dataDir: "mock://data", libraryRoots: roots,
      sidecar: { state: "absent", detail: "mock", capabilities: [], device: null, version: null }, warnings: [],
    }),
    get_counts: counts,
    get_stats: () => ({
      ...counts(),
      byMonth: countBy(visible(), (i) => new Date(i.takenMs).toISOString().slice(0, 7)),
      byAspect: countBy(visible(), (i) => i.aspect ?? "other"),
      byFolder: countBy(visible(), (i) => i.folder),
      byCategory: [], topTags: [], duplicatesExact: 0, duplicatesSimilar: 0,
    }),
    query_images: (q) => queryImages(images, q),
    get_image: ({ id }) => {
      const img = get(id);
      const root = roots.find((r) => r.id === img.rootId)?.path ?? "mock";
      const absPath = [root, img.folder, img.fileName].filter(Boolean).join("\\");
      return { ...img, absPath, createdMs: img.takenMs, contentHash: null, note: null, tags: [], albums: [], ocrText: null, categories: [], blurScore: null };
    },
    get_timeline: ({ query, bucket }) => ({ buckets: timeline(images, query, bucket) }),
    get_settings: () => ({ ...settings }),
    set_setting: ({ key, value }) => {
      settings[key] = value;
      return null;
    },
    list_tags: () => [],
    list_albums: () => [],
    list_smart_filters: () => [],
    trash_images: ({ imageIds }) => mutate(imageIds, (i) => (i.trashedAt = MOCK_NOW_MS)),
    restore_images: ({ imageIds }) => mutate(imageIds, (i) => (i.trashedAt = null)),
    recycle_images: ({ imageIds }) => mutate(imageIds, (i) => (i.missing = true)),
    set_favorite: ({ imageIds, value }) => mutate(imageIds, (i) => (i.favorite = value)),
    set_rating: ({ imageIds, rating }) => mutate(imageIds, (i) => (i.rating = rating)),
    undo: ({ batchId }) => {
      const at = batchId === null ? journal.length - 1 : journal.findIndex((j) => j.batchId === batchId);
      if (at < 0) throw appError("notFound", "nothing to undo");
      const [entry] = journal.splice(at, 1);
      entry.undo();
      changed("undo", null);
      return { batchId: entry.batchId, affected: 1, failed: [] };
    },
    add_library_root: ({ path }) => {
      const root = { ...mockRoot(images.length), id: roots.length + 1, path, protectedRoot: /\\OneDrive\\/i.test(path) };
      roots.push(root);
      return root;
    },
    start_index: () => {
      const jobId = ++jobSeq;
      simulateIndex(jobId, images.length, fire, timers);
      return { jobId };
    },
    cancel_job: () => null,
  };

  mockConvertFileSrc("windows");
  mockIPC(
    (cmd, payload) => {
      calls.push({ cmd, payload });
      if (cmd === "plugin:clipboard-manager|write_text") return null;
      if (cmd === "plugin:dialog|open") return "D:\\Mock\\Screenshots";
      const h = handlers[cmd as keyof CommandMap] as Handler | undefined;
      if (!h) throw appError("internal", `mock: command \`${cmd}\` is not implemented`);
      const req = (payload as { req?: unknown } | undefined)?.req ?? null;
      // Serialize like real IPC: the UI must never hold a live reference into the mock's state.
      const res = h(req as never);
      return res === null || res === undefined ? null : JSON.parse(JSON.stringify(res));
    },
    { shouldMockEvents: true },
  );
  setImageUrlOverride((ref, what) => placeholderUrl(get(ref.id), what.kind === "thumb" ? what.width : PREVIEW_WIDTH * 2));

  return {
    calls,
    images,
    emit: fire,
    uninstall: () => {
      timers.forEach(clearTimeout);
      timers.clear();
      setImageUrlOverride(null);
      clearMocks();
    },
  };
}

/** Ten `job-progress` events over ~2 s, then `finished` and `library-changed` (throttle-compatible). */
function simulateIndex(jobId: number, total: number, fire: MockHandle["emit"], timers: Set<ReturnType<typeof setTimeout>>): void {
  for (let step = 1; step <= 10; step++) {
    const id = setTimeout(() => {
      timers.delete(id);
      const finished = step === 10;
      const done = Math.round((total * step) / 10);
      void fire("job-progress", { jobId, kind: "index", stage: finished ? "done" : "scan", done, total, currentPath: null, finished, error: null });
      if (finished) void fire("library-changed", { reason: "indexed", imageIds: null });
    }, step * 200);
    timers.add(id);
  }
}
