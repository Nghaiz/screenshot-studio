// Top bar: search (text + mode), filters toggle, layout switch, size slider, sort, side panels, settings, palette.
import { useEffect, useState } from "react";
import type { SortField } from "@bindings/SortField";
import type { TextMode } from "@bindings/TextMode";
import { useAppInfo } from "../data/library";
import { useT } from "../i18n";
import { SEARCH_DEBOUNCE_MS, THUMB_SIZE_MAX, THUMB_SIZE_MIN, THUMB_SIZE_STEP } from "../lib/defaults";
import { activeFilterCount } from "../lib/query";
import { clampThumb, LAYOUTS, SORT_FIELDS, usePrefs } from "../state/prefs";
import { useView } from "../state/view";

export const SEARCH_INPUT_ID = "ss-search";
export const TEXT_MODES = ["any", "filename", "ocr", "semantic"] as const satisfies readonly TextMode[];
/** The sidecar capability semantic search needs (contract-sidecar-jsonrpc `embed`). */
export const SEMANTIC_CAPABILITY = "embed";

export function TopBar() {
  const t = useT();
  const { layout, thumbSize, sort, set } = usePrefs();
  const view = useView((s) => s.view);
  const filter = useView((s) => s.filter);
  const filtersOpen = useView((s) => s.filtersOpen);
  const sidePanel = useView((s) => s.sidePanel);
  const info = useAppInfo();
  const semanticReady = info.data?.sidecar.capabilities.includes(SEMANTIC_CAPABILITY) ?? false;
  const timeline = view === "timeline";
  const hasText = !!filter.text?.trim();
  const sortFields: readonly SortField[] = hasText ? [...SORT_FIELDS, "relevance"] : SORT_FIELDS;
  const sortField = sort.field === "relevance" && !hasText ? "taken" : sort.field;
  const active = activeFilterCount(filter);
  const v = useView.getState;

  // A semantic search left over from a sidecar that went away falls back to `any` visibly (the select shows it).
  useEffect(() => {
    if (filter.textMode === "semantic" && !semanticReady) v().setFilter({ textMode: "any" });
  }, [filter.textMode, semanticReady, v]);

  return (
    <header className="ss-panel flex flex-wrap items-center gap-2 px-3 py-2">
      <SearchBox />
      <select
        className="ss-input"
        aria-label={t("search.mode")}
        value={filter.textMode}
        onChange={(e) => v().setFilter({ textMode: e.target.value as TextMode })}
      >
        {TEXT_MODES.map((m) => (
          <option key={m} value={m} disabled={m === "semantic" && !semanticReady}>
            {m === "semantic" && !semanticReady ? t("search.semanticOff") : t(`search.${m}`)}
          </option>
        ))}
      </select>
      <button type="button" className="ss-btn" aria-pressed={filtersOpen} aria-controls="ss-filter-panel" onClick={() => v().setFiltersOpen(!filtersOpen)}>
        {t("filter.toggle")}
        {active > 0 && <span data-testid="filter-count">({active})</span>}
      </button>
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
          aria-label={t("top.sort")}
          value={sortField}
          disabled={timeline}
          onChange={(e) => set({ sort: { ...sort, field: e.target.value as SortField } })}
        >
          {sortFields.map((f) => (
            <option key={f} value={f}>
              {t(`sort.${f}`)}
            </option>
          ))}
        </select>
        <button
          type="button"
          className="ss-btn"
          disabled={timeline || sortField === "random"}
          aria-label={t(sort.dir === "asc" ? "sort.asc" : "sort.desc")}
          title={t(sort.dir === "asc" ? "sort.asc" : "sort.desc")}
          onClick={() => set({ sort: { ...sort, dir: sort.dir === "asc" ? "desc" : "asc" } })}
        >
          {sort.dir === "asc" ? "↑" : "↓"}
        </button>
      </label>
      <button type="button" className="ss-btn" aria-pressed={sidePanel === "details"} onClick={() => v().setSidePanel(sidePanel === "details" ? "none" : "details")}>
        {t("top.details")}
      </button>
      <button type="button" className="ss-btn" aria-pressed={sidePanel === "history"} onClick={() => v().setSidePanel(sidePanel === "history" ? "none" : "history")}>
        {t("top.history")}
      </button>
      <button type="button" className="ss-btn" onClick={() => v().setSettings(true)}>
        {t("top.settings")}
      </button>
      <button type="button" className="ss-btn" onClick={() => v().setPalette(true)}>
        {t("top.palette")}
      </button>
    </header>
  );
}

/** Text is pushed into the filter after a short pause, so typing does not query on every key. */
function SearchBox() {
  const t = useT();
  const committed = useView((s) => s.filter.text) ?? "";
  const [text, setText] = useState(committed);
  useEffect(() => setText(committed), [committed]);
  useEffect(() => {
    if (text === committed) return;
    const id = setTimeout(() => useView.getState().setFilter({ text: text.trim() ? text : null }), SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(id);
  }, [text, committed]);
  return (
    <input
      id={SEARCH_INPUT_ID}
      type="search"
      className="ss-input min-w-48 flex-1"
      placeholder={t("top.search")}
      aria-label={t("top.search")}
      value={text}
      onChange={(e) => setText(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === "Escape" && text) {
          e.stopPropagation();
          setText("");
        }
      }}
    />
  );
}
