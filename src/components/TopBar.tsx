// Top bar: search placeholder (real search is phase 2.2), layout switch, size slider, sort, settings, palette.
import type { SortField } from "@bindings/SortField";
import { useT } from "../i18n";
import { THUMB_SIZE_MAX, THUMB_SIZE_MIN, THUMB_SIZE_STEP } from "../lib/defaults";
import { clampThumb, LAYOUTS, SORT_FIELDS, usePrefs } from "../state/prefs";
import { useView } from "../state/view";

export const SEARCH_INPUT_ID = "ss-search";

export function TopBar() {
  const t = useT();
  const { layout, thumbSize, sort, set } = usePrefs();
  const view = useView((s) => s.view);
  const timeline = view === "timeline";

  return (
    <header className="ss-panel flex flex-wrap items-center gap-2 px-3 py-2">
      <input
        id={SEARCH_INPUT_ID}
        type="search"
        className="ss-input min-w-48 flex-1"
        placeholder={t("top.search")}
        aria-label={t("top.search")}
        title={t("top.searchSoon")}
        readOnly
      />
      <div role="group" aria-label={t("top.layout")} className="flex gap-1">
        {LAYOUTS.map((l) => (
          <button key={l} type="button" className="ss-btn" aria-pressed={layout === l} disabled={timeline} onClick={() => set({ layout: l })}>
            {t(`layout.${l}`)}
          </button>
        ))}
      </div>
      <label className="flex items-center gap-2 text-xs">
        <span className="ss-muted">{t("top.size")}</span>
        <input
          type="range"
          min={THUMB_SIZE_MIN}
          max={THUMB_SIZE_MAX}
          step={THUMB_SIZE_STEP / 4}
          value={thumbSize}
          disabled={layout === "list" && !timeline}
          onChange={(e) => set({ thumbSize: clampThumb(Number(e.target.value)) })}
        />
      </label>
      <label className="flex items-center gap-1 text-xs">
        <span className="ss-muted">{t("top.sort")}</span>
        <select
          className="ss-input"
          value={sort.field}
          disabled={timeline}
          onChange={(e) => set({ sort: { ...sort, field: e.target.value as SortField } })}
        >
          {SORT_FIELDS.map((f) => (
            <option key={f} value={f}>
              {t(`sort.${f}`)}
            </option>
          ))}
        </select>
        <button
          type="button"
          className="ss-btn"
          disabled={timeline || sort.field === "random"}
          aria-label={t(sort.dir === "asc" ? "sort.asc" : "sort.desc")}
          title={t(sort.dir === "asc" ? "sort.asc" : "sort.desc")}
          onClick={() => set({ sort: { ...sort, dir: sort.dir === "asc" ? "desc" : "asc" } })}
        >
          {sort.dir === "asc" ? "↑" : "↓"}
        </button>
      </label>
      <button type="button" className="ss-btn" onClick={() => useView.getState().setSettings(true)}>
        {t("top.settings")}
      </button>
      <button type="button" className="ss-btn" onClick={() => useView.getState().setPalette(true)}>
        {t("top.palette")}
      </button>
    </header>
  );
}
