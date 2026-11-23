// Duplicate finder (feature group 4): exact copies (same content hash) or similar images (dHash distance <= the
// slider), computed by the backend without the sidecar. Each group shows its best image first; "keep best" and the
// selection send `trash_images` only (in-app trash, undoable). It never offers permanent delete.
import { useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { DuplicateGroup } from "@bindings/DuplicateGroup";
import type { DuplicateMode } from "@bindings/DuplicateMode";
import { trashCopies } from "../../actions/smart";
import { useDuplicates, useNumberSetting } from "../../data/smart";
import { useT } from "../../i18n";
import { DUP_HAMMING_DEFAULT, DUP_HAMMING_MAX, DUP_SLIDER_DEBOUNCE_MS, SMART_THUMB_PX } from "../../lib/defaults";
import { toAppError } from "../../lib/ipc";
import { useView } from "../../state/view";
import { SmartThumb } from "./SmartThumb";

const MODES = ["exact", "similar"] as const satisfies readonly DuplicateMode[];
/** Group row: header + thumbnail + 2 caption lines + padding (fixed, so the virtualizer needs no measuring). */
const ROW_HEIGHT = SMART_THUMB_PX + 104;
const ROW_GAP = 8;

/** The group with its best image first, the others in the backend's order. */
export function bestFirst(g: DuplicateGroup): DuplicateGroup {
  return { ...g, imageIds: [g.bestId, ...g.imageIds.filter((id) => id !== g.bestId)] };
}

/** Every image of the group except the best one: what "keep best" sends to the trash. */
export const othersOf = (g: DuplicateGroup): number[] => g.imageIds.filter((id) => id !== g.bestId);

function useDebounced<T>(value: T, ms: number): T {
  const [v, setV] = useState(value);
  useEffect(() => {
    const id = setTimeout(() => setV(value), ms);
    return () => clearTimeout(id);
  }, [value, ms]);
  return v;
}

export default function DuplicatesView() {
  const t = useT();
  const [mode, setMode] = useState<DuplicateMode>("exact");
  // The slider starts at the `dupHamming` setting (design default 6) until the user moves it.
  const settingHamming = useNumberSetting("dupHamming", DUP_HAMMING_DEFAULT);
  const [moved, setHamming] = useState<number | null>(null);
  const hamming = moved ?? settingHamming;
  const applied = useDebounced(hamming, DUP_SLIDER_DEBOUNCE_MS);
  const q = useDuplicates(mode, applied);
  const groups = useMemo(() => (q.data?.groups ?? []).map(bestFirst), [q.data]);
  const extra = useMemo(() => groups.flatMap(othersOf), [groups]);
  const [picked, setPicked] = useState<ReadonlySet<number>>(() => new Set());
  // Only picks still shown count (after a trash or a mode change the groups are read again).
  const shown = useMemo(() => new Set(groups.flatMap((g) => g.imageIds)), [groups]);
  const chosen = useMemo(() => [...picked].filter((id) => shown.has(id)), [picked, shown]);

  const toggle = (id: number) =>
    setPicked((p) => {
      const next = new Set(p);
      if (!next.delete(id)) next.add(id);
      return next;
    });
  const trashChosen = async () => {
    await trashCopies(chosen);
    setPicked(new Set());
  };

  return (
    <section aria-label={t("dup.title")} className="ss-panel flex min-h-0 flex-1 flex-col gap-2 p-3 text-sm">
      <div className="flex flex-wrap items-center gap-2">
        <div role="group" aria-label={t("dup.mode")} className="flex gap-1">
          {MODES.map((m) => (
            <button key={m} type="button" className="ss-btn" aria-pressed={mode === m} onClick={() => setMode(m)}>
              {t(m === "exact" ? "dup.exact" : "dup.similar")}
            </button>
          ))}
        </div>
        {mode === "similar" && (
          <label className="flex items-center gap-2 text-xs" title={t("dup.hammingHelp")}>
            <span className="ss-muted">{t("dup.hamming", { n: hamming })}</span>
            <input type="range" min={0} max={DUP_HAMMING_MAX} step={1} value={hamming} aria-label={t("dup.hamming", { n: hamming })}
              onChange={(e) => setHamming(Number(e.target.value))} />
          </label>
        )}
        {q.data && <span className="ss-muted text-xs" data-testid="dup-summary">{t("dup.summary", { groups: groups.length, extra: extra.length })}</span>}
        <span className="flex-1" />
        <button type="button" className="ss-btn" disabled={extra.length === 0} onClick={() => setPicked(new Set(extra))}>{t("dup.selectOthers")}</button>
        <button type="button" className="ss-btn" disabled={chosen.length === 0} onClick={() => setPicked(new Set())}>{t("dup.clear")}</button>
        <button type="button" className="ss-btn ss-btn-primary" disabled={chosen.length === 0} onClick={() => void trashChosen()}>
          {t("dup.trashSelected", { count: chosen.length })}
        </button>
      </div>
      <p className="ss-muted m-0 text-xs">{t("dup.safety")}</p>
      {q.isPending ? (
        <p className="ss-muted m-0" aria-busy="true">{t("dup.loading")}</p>
      ) : q.isError ? (
        <p className="m-0" role="alert">{t("dup.failed", { message: toAppError(q.error).message })}</p>
      ) : groups.length === 0 ? (
        <p className="ss-muted m-0">{t("dup.none")}</p>
      ) : (
        <GroupList groups={groups} picked={picked} onToggle={toggle} />
      )}
    </section>
  );
}

function GroupList({ groups, picked, onToggle }: { groups: DuplicateGroup[]; picked: ReadonlySet<number>; onToggle: (id: number) => void }) {
  const t = useT();
  const ref = useRef<HTMLDivElement>(null);
  const v = useVirtualizer({ count: groups.length, getScrollElement: () => ref.current, estimateSize: () => ROW_HEIGHT + ROW_GAP, overscan: 2 });
  return (
    <div ref={ref} className="min-h-0 flex-1 overflow-y-auto" role="list" aria-label={t("dup.title")}>
      <div style={{ height: v.getTotalSize(), position: "relative" }}>
        {v.getVirtualItems().map((r) => {
          const g = groups[r.index];
          return (
            <div key={g.groupId} role="listitem" data-group={g.groupId} className="absolute inset-x-0 top-0"
              style={{ transform: `translateY(${r.start}px)`, height: ROW_HEIGHT }}>
              <GroupRow group={g} n={r.index + 1} picked={picked} onToggle={onToggle} />
            </div>
          );
        })}
      </div>
    </div>
  );
}

function GroupRow({ group, n, picked, onToggle }: { group: DuplicateGroup; n: number; picked: ReadonlySet<number>; onToggle: (id: number) => void }) {
  const t = useT();
  const others = othersOf(group);
  const compare = (id: number) => useView.getState().openLightbox({ mode: "compare", ids: [group.bestId, id] });
  return (
    <div className="ss-group flex h-full flex-col gap-2 rounded p-2">
      <div className="flex items-center gap-2">
        <span className="font-semibold">{t("dup.group", { n, count: group.imageIds.length })}</span>
        <span className="flex-1" />
        <button type="button" className="ss-btn" onClick={() => void trashCopies(others)}>{t("dup.keepBest", { count: others.length })}</button>
      </div>
      <ul className="m-0 flex min-h-0 list-none gap-3 overflow-x-auto p-0" aria-label={t("dup.group", { n, count: group.imageIds.length })}>
        {group.imageIds.map((id) => {
          const best = id === group.bestId;
          return (
            <li key={id} className="relative shrink-0" data-image-id={id} data-best={best}>
              {best ? (
                <SmartThumb id={id} size={SMART_THUMB_PX} />
              ) : (
                <button type="button" className="block p-0 text-left" aria-label={t("dup.compare", { name: `#${id}` })} onClick={() => compare(id)}
                  style={{ background: "none", border: 0, color: "inherit", cursor: "zoom-in" }}>
                  <SmartThumb id={id} size={SMART_THUMB_PX} />
                </button>
              )}
              <label className="ss-pick absolute left-1 top-1 flex items-center gap-1 rounded px-1 text-xs">
                <input type="checkbox" checked={picked.has(id)} onChange={() => onToggle(id)} aria-label={t("dup.pick", { name: `#${id}` })} />
                {best && <span data-testid="dup-best">★ {t("dup.bestBecause", { reason: t(`dup.reason.${group.reason}`) })}</span>}
              </label>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
