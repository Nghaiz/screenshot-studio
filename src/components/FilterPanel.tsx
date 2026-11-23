// Filter panel: every ImageQuery filter of feature group 2 as a control. Each control writes one field of
// the view's FilterSpec; Shell turns it into the exact `query_images` req (organize.test snapshots it).
import { useEffect, useRef, useState, type ReactNode } from "react";
import type { AspectClass } from "@bindings/AspectClass";
import type { TagMatch } from "@bindings/TagMatch";
import type { TrashFilter } from "@bindings/TrashFilter";
import { useFolders } from "../data/library";
import { useTags } from "../data/organize";
import { useT } from "../i18n";
import { DEFAULT_COLOR_TOLERANCE } from "../lib/defaults";
import { useView } from "../state/view";

export const ASPECTS = ["phone", "pc", "square", "wide", "other"] as const satisfies readonly AspectClass[];
const TRASH_FILTERS = ["exclude", "include", "only"] as const satisfies readonly TrashFilter[];
const MB = 1024 * 1024;
const DEFAULT_FILTER_COLOR = "#3B82F6";

const pad = (n: number) => String(n).padStart(2, "0");
/** Local calendar date of an epoch ms, as `<input type=date>` wants it. */
export const toDateInput = (ms: number | null) => {
  if (ms === null) return "";
  const d = new Date(ms);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
};
/** `YYYY-MM-DD` -> local midnight (start) or the last millisecond of that day (end). */
export function fromDateInput(v: string, end: boolean): number | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(v);
  if (!m) return null;
  const [y, mo, d] = [Number(m[1]), Number(m[2]) - 1, Number(m[3])];
  return end ? new Date(y, mo, d, 23, 59, 59, 999).getTime() : new Date(y, mo, d).getTime();
}
const intOrNull = (v: string) => (v.trim() === "" || !Number.isFinite(Number(v)) ? null : Math.max(0, Math.round(Number(v))));
const mbToBytes = (v: string) => (v.trim() === "" || !Number.isFinite(Number(v)) ? null : Math.max(0, Math.round(Number(v) * MB)));
const bytesToMb = (n: number | null) => (n === null ? "" : String(Math.round((n / MB) * 100) / 100));

export function FilterPanel() {
  const open = useView((s) => s.filtersOpen);
  if (!open) return null;
  return <FilterBody />;
}

function FilterBody() {
  const t = useT();
  const f = useView((s) => s.filter);
  const inTrash = useView((s) => s.view === "trash");
  const tags = useTags();
  const { folders } = useFolders();
  const set = useView.getState().setFilter;
  const toggleIn = <T,>(list: readonly T[], v: T) => (list.includes(v) ? list.filter((x) => x !== v) : [...list, v]);

  return (
    <section id="ss-filter-panel" aria-label={t("filter.title")} className="ss-panel grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-3 p-3 text-xs">
      <Field label={t("filter.date")}>
        <div className="flex items-center gap-1">
          <input type="date" className="ss-input" aria-label={t("filter.dateFrom")} value={toDateInput(f.dateFrom)} onChange={(e) => set({ dateFrom: fromDateInput(e.target.value, false) })} />
          <span>–</span>
          <input type="date" className="ss-input" aria-label={t("filter.dateTo")} value={toDateInput(f.dateTo)} onChange={(e) => set({ dateTo: fromDateInput(e.target.value, true) })} />
        </div>
      </Field>

      <Field label={t("filter.resolution")}>
        <div className="grid grid-cols-2 gap-1">
          <NumberInput label={t("filter.minWidth")} value={f.minWidth} onChange={(v) => set({ minWidth: intOrNull(v) })} />
          <NumberInput label={t("filter.maxWidth")} value={f.maxWidth} onChange={(v) => set({ maxWidth: intOrNull(v) })} />
          <NumberInput label={t("filter.minHeight")} value={f.minHeight} onChange={(v) => set({ minHeight: intOrNull(v) })} />
          <NumberInput label={t("filter.maxHeight")} value={f.maxHeight} onChange={(v) => set({ maxHeight: intOrNull(v) })} />
        </div>
      </Field>

      <Field label={t("filter.aspect")}>
        <div className="flex flex-wrap gap-1">
          {ASPECTS.map((a) => (
            <button key={a} type="button" className="ss-btn" aria-pressed={f.aspects.includes(a)} onClick={() => set({ aspects: toggleIn(f.aspects, a) })}>
              {t(`aspect.${a}`)}
            </button>
          ))}
        </div>
      </Field>

      <Field label={t("filter.size")}>
        <div className="grid grid-cols-2 gap-1">
          <NumberInput label={t("filter.minSizeMb")} value={bytesToMb(f.minSize)} step="0.1" onChange={(v) => set({ minSize: mbToBytes(v) })} />
          <NumberInput label={t("filter.maxSizeMb")} value={bytesToMb(f.maxSize)} step="0.1" onChange={(v) => set({ maxSize: mbToBytes(v) })} />
        </div>
      </Field>

      <Field label={t("filter.folder")}>
        <select className="ss-input" aria-label={t("filter.folder")} value={f.folders[0] ?? "\u0000"}
          onChange={(e) => set({ folders: e.target.value === "\u0000" ? [] : [e.target.value] })}>
          <option value={"\u0000"}>{t("nav.allFolders")}</option>
          {[...new Set([...(folders?.map((x) => x.name) ?? []), ...f.folders])].map((name) => (
            <option key={name} value={name}>{name || t("nav.rootFolder")}</option>
          ))}
        </select>
      </Field>

      <Field label={t("filter.tags")}>
        <div className="flex flex-wrap gap-1">
          {(tags.data ?? []).map((tag) => (
            <button key={tag.id} type="button" className="ss-btn" aria-pressed={f.tagIds.includes(tag.id)} onClick={() => set({ tagIds: toggleIn(f.tagIds, tag.id) })}>
              <span aria-hidden className="inline-block h-2.5 w-2.5 rounded-full" style={{ background: tag.color }} />
              {tag.name}
            </button>
          ))}
          {tags.data?.length === 0 && <span className="ss-muted">{t("filter.noTags")}</span>}
        </div>
        <select className="ss-input" aria-label={t("filter.tagMatch")} value={f.tagMatch} onChange={(e) => set({ tagMatch: e.target.value as TagMatch })}>
          <option value="any">{t("filter.tagAny")}</option>
          <option value="all">{t("filter.tagAll")}</option>
        </select>
      </Field>

      <Field label={t("filter.color")}>
        <div className="flex items-center gap-2">
          <label className="flex items-center gap-2">
            <input type="checkbox" checked={f.color !== null}
              onChange={(e) => set(e.target.checked ? { color: DEFAULT_FILTER_COLOR, colorTolerance: f.colorTolerance || DEFAULT_COLOR_TOLERANCE } : { color: null, colorTolerance: 0 })} />
            {t("filter.colorOn")}
          </label>
          <input type="color" aria-label={t("filter.colorPick")} disabled={f.color === null} value={f.color ?? DEFAULT_FILTER_COLOR} onChange={(e) => set({ color: e.target.value.toUpperCase() })} />
        </div>
        <input type="range" min={0} max={100} aria-label={t("filter.colorTolerance")} disabled={f.color === null} value={f.colorTolerance}
          onChange={(e) => set({ colorTolerance: Number(e.target.value) })} />
      </Field>

      <Field label={t("filter.category")}>
        <DraftInput label={t("filter.category")} placeholder={t("filter.categoryHint")} value={f.categories.join(", ")}
          onChange={(v) => set({ categories: v.split(",").map((s) => s.trim()).filter(Boolean) })} />
      </Field>

      <Field label={t("filter.other")}>
        <label className="flex items-center gap-2">
          <input type="checkbox" checked={f.favoriteOnly} onChange={(e) => set({ favoriteOnly: e.target.checked })} />
          {t("filter.favoriteOnly")}
        </label>
        <select className="ss-input" aria-label={t("filter.minRating")} value={f.minRating} onChange={(e) => set({ minRating: Number(e.target.value) })}>
          {[0, 1, 2, 3, 4, 5].map((n) => (
            <option key={n} value={n}>{n === 0 ? t("filter.anyRating") : t("filter.ratingAtLeast", { n })}</option>
          ))}
        </select>
        <select className="ss-input" aria-label={t("filter.hasText")} value={f.hasText === null ? "any" : f.hasText ? "yes" : "no"}
          onChange={(e) => set({ hasText: e.target.value === "any" ? null : e.target.value === "yes" })}>
          <option value="any">{t("filter.hasTextAny")}</option>
          <option value="yes">{t("filter.hasTextYes")}</option>
          <option value="no">{t("filter.hasTextNo")}</option>
        </select>
        <select className="ss-input" aria-label={t("filter.trash")} disabled={inTrash} value={inTrash ? "only" : f.trash}
          onChange={(e) => set({ trash: e.target.value as TrashFilter })}>
          {TRASH_FILTERS.map((x) => (
            <option key={x} value={x}>{t(`filter.trash_${x}`)}</option>
          ))}
        </select>
      </Field>

      <div className="flex items-end gap-2">
        <button type="button" className="ss-btn" onClick={() => useView.getState().resetFilter()}>
          {t("filter.reset")}
        </button>
        <button type="button" className="ss-btn ss-btn-primary" onClick={() => useView.getState().openDialog({ kind: "saveFilter" })}>
          {t("filter.save")}
        </button>
      </div>
    </section>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <span className="ss-muted font-semibold uppercase tracking-wide">{label}</span>
      {children}
    </div>
  );
}

function NumberInput(props: { label: string; value: number | string | null; step?: string; onChange: (v: string) => void }) {
  return <DraftInput type="number" step={props.step ?? "1"} label={props.label} placeholder={props.label} value={props.value === null ? "" : String(props.value)} onChange={props.onChange} />;
}

/**
 * An input that keeps what the user typed ("1." or "a, ") while the filter holds the parsed value. It takes the
 * filter's value only while it is not being edited (reset, smart filter applied, another control).
 */
function DraftInput(props: { label: string; placeholder: string; value: string; type?: string; step?: string; onChange: (v: string) => void }) {
  const ref = useRef<HTMLInputElement>(null);
  const [draft, setDraft] = useState(props.value);
  useEffect(() => {
    if (document.activeElement !== ref.current) setDraft(props.value);
  }, [props.value]);
  return (
    <input ref={ref} type={props.type ?? "text"} min={props.type === "number" ? 0 : undefined} step={props.step} className="ss-input w-full"
      aria-label={props.label} placeholder={props.placeholder} value={draft}
      onChange={(e) => {
        setDraft(e.target.value);
        props.onChange(e.target.value);
      }}
      onBlur={() => setDraft(props.value)} />
  );
}
