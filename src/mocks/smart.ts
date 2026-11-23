// Mock of the phase 3.2 smart commands over the mock catalog: sidecar status / restart, the AI job (`job-progress`
// events), duplicates (deterministic planted groups, best = most pixels, then newest, then lowest id), sessions
// (ss-core `smart::sessions::split`), blurry / tiny issues. Types come from @bindings only.
import type { CommandMap } from "@bindings/commands";
import type { DuplicateGroup } from "@bindings/DuplicateGroup";
import type { DuplicateMode } from "@bindings/DuplicateMode";
import type { DuplicateReason } from "@bindings/DuplicateReason";
import type { ImageSession } from "@bindings/ImageSession";
import type { ImageSummary } from "@bindings/ImageSummary";
import type { SidecarStatus } from "@bindings/SidecarStatus";
import type { EventMap, EventName } from "../lib/events";
import { appError, filterImages, type FilterContext } from "./catalog";

/** What `get_sidecar_status` answers unless a test says otherwise: no venv (the CI and fresh-machine case). */
export const MOCK_SIDECAR_ABSENT: SidecarStatus = {
  state: "absent",
  detail: "no Python at mock\\sidecar-venv\\Scripts\\python.exe (setting sidecarVenv)",
  capabilities: [],
  device: null,
  version: null,
};
export const MOCK_SIDECAR_READY: SidecarStatus = {
  state: "ready", detail: null, capabilities: ["ocr", "embed", "classify", "color", "pii"], device: "cuda", version: "0.1.0-mock",
};

/** Planted exact groups: EXACT_GROUPS groups starting at image id 3, every 53rd id, a triple every 3rd group. */
const EXACT_GROUPS = 12;
const EXACT_STRIDE = 53;
/** `similar` adds 2 pairs per unit of Hamming distance (capped), from image id 5003 on. */
const SIMILAR_PER_HAMMING = 2;
const SIMILAR_MAX = 30;
const SIMILAR_START = 5003;
const SIMILAR_STRIDE = 61;
/** Issue rules over the mock catalog (no real pixels): every 211th id is blurry, every 307th id is tiny. */
const BLURRY_EVERY = 211;
const TINY_EVERY = 307;
const SESSION_GAP_DEFAULT_MIN = 30;
const AI_STEPS = 10;
const AI_STEP_MS = 150;

type Fire = <E extends EventName>(event: E, payload: EventMap[E]) => Promise<void>;

export interface SmartDeps {
  images: readonly ImageSummary[];
  byId: ReadonlyMap<number, ImageSummary>;
  settings: Record<string, unknown>;
  ctx: FilterContext;
  fire: Fire;
  timers: Set<ReturnType<typeof setTimeout>>;
  nextJobId: () => number;
}

export function createSmart(d: SmartDeps, initial: SidecarStatus) {
  let sidecar: SidecarStatus = initial;
  const live = (id: number) => {
    const i = d.byId.get(id);
    return !!i && !i.missing && i.trashedAt === null;
  };
  const later = (ms: number, fn: () => void) => {
    const id = setTimeout(() => {
      d.timers.delete(id);
      fn();
    }, ms);
    d.timers.add(id);
  };
  const setSidecar = (s: SidecarStatus) => {
    sidecar = s;
  };

  function planted(mode: DuplicateMode, hamming: number): number[][] {
    const groups: number[][] = [];
    for (let k = 0; k < EXACT_GROUPS; k++) {
      const base = 3 + k * EXACT_STRIDE;
      groups.push(k % 3 === 0 ? [base, base + 1, base + 2] : [base, base + 1]);
    }
    if (mode === "similar") {
      for (let k = 0; k < Math.min(SIMILAR_MAX, SIMILAR_PER_HAMMING * hamming); k++) {
        const base = SIMILAR_START + k * SIMILAR_STRIDE;
        groups.push([base, base + 1]);
      }
    }
    return groups.map((g) => g.filter(live)).filter((g) => g.length >= 2);
  }

  const pixels = (i: ImageSummary) => (i.width ?? 0) * (i.height ?? 0);
  function rank(ids: number[]): { best: number; reason: DuplicateReason } {
    const imgs = ids.map((id) => d.byId.get(id) as ImageSummary);
    imgs.sort((a, b) => pixels(b) - pixels(a) || b.takenMs - a.takenMs || a.id - b.id);
    const [best, next] = imgs;
    const reason: DuplicateReason = pixels(best) !== pixels(next) ? "largest" : best.takenMs !== next.takenMs ? "newest" : "largest";
    return { best: best.id, reason };
  }

  function duplicates(mode: DuplicateMode, hamming: number): DuplicateGroup[] {
    return planted(mode, hamming).map((ids, i) => {
      const r = rank(ids);
      return { groupId: i + 1, imageIds: ids, bestId: r.best, reason: r.reason };
    });
  }

  const redundant = (groups: DuplicateGroup[]) => groups.reduce((a, g) => a + g.imageIds.length - 1, 0);
  const hammingSetting = () => (typeof d.settings.dupHamming === "number" ? d.settings.dupHamming : 6);

  const handlers: { [K in keyof CommandMap]?: (req: CommandMap[K]["req"]) => CommandMap[K]["res"] } = {
    get_sidecar_status: () => sidecar,
    restart_sidecar: () => {
      // No venv stays absent; a configured sidecar goes through `starting` again, like the real supervisor.
      if (initial.state === "absent") {
        setSidecar(initial);
      } else {
        setSidecar({ ...initial, state: "starting", capabilities: [], device: null });
        later(AI_STEP_MS * 3, () => {
          setSidecar(initial);
          void d.fire("sidecar-status", initial);
        });
      }
      void d.fire("sidecar-status", sidecar);
      return sidecar;
    },
    start_ai: ({ tasks, imageIds }) => {
      if (sidecar.state !== "ready" && sidecar.state !== "starting") {
        throw appError("sidecarUnavailable", `the AI job needs the sidecar (sidecar ${sidecar.state})`);
      }
      if (tasks.length === 0) throw appError("invalidArgument", "no AI task given");
      const jobId = d.nextJobId();
      const per = imageIds?.length ?? d.images.filter((i) => !i.missing && i.cloudState !== "cloudOnly").length;
      const total = per * tasks.length;
      for (let step = 1; step <= AI_STEPS; step++) {
        later(step * AI_STEP_MS, () => {
          const finished = step === AI_STEPS;
          const stage = tasks[Math.min(tasks.length - 1, Math.floor(((step - 1) * tasks.length) / AI_STEPS))];
          void d.fire("job-progress", {
            jobId, kind: "ai", stage, done: Math.round((total * step) / AI_STEPS), total, currentPath: null, finished, error: null,
          });
          if (finished) void d.fire("library-changed", { reason: "ai", imageIds: null });
        });
      }
      return { jobId };
    },
    find_duplicates: ({ mode, hamming }) => {
      const h = hamming ?? hammingSetting();
      if (h < 0 || h > 64) throw appError("invalidArgument", `hamming must be 0..64, got ${h}`);
      return { groups: duplicates(mode, h) };
    },
    get_sessions: ({ query, gapMinutes }) => {
      const gap = (gapMinutes ?? (typeof d.settings.sessionGapMinutes === "number" ? d.settings.sessionGapMinutes : SESSION_GAP_DEFAULT_MIN)) * 60_000;
      const rows = filterImages(d.images, { ...query, cursor: null }, d.ctx).map((i) => [i.id, i.takenMs] as const);
      return { sessions: splitSessions(rows, gap, query.sort.dir === "desc") };
    },
    detect_issues: () => {
      const present = d.images.filter((i) => !i.missing && i.trashedAt === null);
      return {
        blurry: present.filter((i) => i.id % BLURRY_EVERY === 0).map((i) => i.id),
        tiny: present.filter((i) => i.id % TINY_EVERY === 0).map((i) => i.id),
      };
    },
  };

  return {
    handlers,
    setSidecar,
    sidecar: () => sidecar,
    duplicateCounts: () => ({
      duplicatesExact: redundant(duplicates("exact", 0)),
      duplicatesSimilar: redundant(duplicates("similar", hammingSetting())),
    }),
  };
}

/** As ss-core `smart::sessions::split`: chain neighbours in time no more than `gapMs` apart. */
export function splitSessions(rows: readonly (readonly [number, number])[], gapMs: number, desc: boolean): ImageSession[] {
  const sorted = [...rows].sort((a, b) => a[1] - b[1] || a[0] - b[0]);
  const out: ImageSession[] = [];
  for (const [id, t] of sorted) {
    const last = out.at(-1);
    if (last && t - last.endMs <= gapMs) {
      last.endMs = t;
      last.imageIds.push(id);
    } else out.push({ startMs: t, endMs: t, imageIds: [id] });
  }
  if (desc) {
    out.reverse();
    for (const s of out) s.imageIds.reverse();
  }
  return out;
}
