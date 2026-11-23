// Top bar: search (text + mode), filters toggle, layout switch (timeline: month/session grouping), size slider, sort,
// side panels, settings, palette. Smart tools (duplicates, cleanup, dashboard) keep only the panel buttons.
import { useEffect, useState } from "react";
import type { SortField } from "@bindings/SortField";
import type { TextMode } from "@bindings/TextMode";
import { useAiGate } from "../data/smart";
import { useT } from "../i18n";
import { SEARCH_DEBOUNCE_MS, THUMB_SIZE_MAX, THUMB_SIZE_MIN, THUMB_SIZE_STEP } from "../lib/defaults";
import { activeFilterCount, isToolView } from "../lib/query";
import type { AiGate } from "../lib/sidecar";
import { clampThumb, LAYOUTS, SORT_FIELDS, TIMELINE_GROUPS, usePrefs } from "../state/prefs";
import { useView } from "../state/view";
import { gateReason } from "./AiNotice";

export const SEARCH_INPUT_ID = "ss-search";
export const TEXT_MODES = ["any", "filename", "ocr", "semantic"] as const satisfies readonly TextMode[];
const OPEN: AiGate = { enabled: true, block: null };

export function TopBar() {
  const view = useView((s) => s.view);
  return isToolView(view) ? <ToolTopBar /> : <BrowseTopBar />;
}

function BrowseTopBar() {
  const t = useT();
  const { layout, thumbSize, sort, timelineGroup, set } = usePrefs();
  const view = useView((s) => s.view);
  const filter = useView((s) => s.filter);
  const filtersOpen = useView((s) => s.filtersOpen);
  // The sidecar capability each AI search mode needs (contract-sidecar-jsonrpc § Degradation).
  const semantic = useAiGate("embed");
  const ocr = useAiGate("ocr");
  const gateOf = (m: TextMode): AiGate => (m === "semantic" ? semantic : m === "ocr" ? ocr : OPEN);
  const blocked = TEXT_MODES.map(gateOf).find((g) => !g.enabled);
  const timeline = view === "timeline";
  const hasText = !!filter.text?.trim();
  const sortFields: readonly SortField[] = hasText ? [...SORT_FIELDS, "relevance"] : SORT_FIELDS;
  const sortField = sort.field === "relevance" && !hasText ? "taken" : sort.field;
  const active = activeFilterCount(filter);
  const v = useView.getState;

  // A semantic search left over from a sidecar that went away falls back to `any` visibly (the select shows it):
  // the backend refuses `semantic` unless the sidecar is ready.
  useEffect(() => {
    if (filter.textMode === "semantic" && !semantic.enabled) v().dropSemantic();
  }, [filter.textMode, semantic.enabled, v]);

  return (
    <header className="ss-panel flex flex-wrap items-center gap-2 px-3 py-2">
      <SearchBox />
      <select
        className="ss-input"
        aria-label={t("search.mode")}
        title={blocked ? (gateReason(t, blocked) ?? undefined) : undefined}
        value={filter.textMode}
        onChange={(e) => v().setFilter({ textMode: e.target.value as TextMode })}
      >
        {TEXT_MODES.map((m) => {
          const g = gateOf(m);
          const name = t(`search.${m}`);
          return (
            <option key={m} value={m} disabled={!g.enabled} title={gateReason(t, g) ?? undefined} data-ai-block={g.block ?? undefined}>
              {g.enabled ? name : t(g.block === "starting" ? "search.aiStarting" : "search.needsAi", { mode: name })}
            </option>
          );
        })}
      </select>
      <button type="button" className="ss-btn" aria-pressed={filtersOpen} aria-controls="ss-filter-panel" onClick={() => v().setFiltersOpen(!filtersOpen)}>
        {t("filter.toggle")}
        {active > 0 && <span data-testid="filter-count">({active})</span>}
      </button>
      {timeline ? (
        <div role="group" aria-label={t("top.groupBy")} className="flex items-center gap-1">
          <span className="ss-muted text-xs">{t("top.groupBy")}</span>
          {TIMELINE_GROUPS.map((g) => (
            <button key={g} type="button" className="ss-btn" aria-pressed={timelineGroup === g} onClick={() => set({ timelineGroup: g })}>
              {t(g === "month" ? "timeline.groupMonth" : "timeline.groupSession")}
            </button>
          ))}
        </div>
      ) : (
        <div role="group" aria-label={t("top.layout")} className="flex gap-1">
          {LAYOUTS.map((l) => (
            <button key={l} type="button" className="ss-btn" aria-pressed={layout === l} onClick={() => set({ layout: l })}>
              {t(`layout.${l}`)}
            </button>
          ))}
        </div>
      )}
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
      <PanelButtons />
    </header>
  );
}

/** Smart tools have their own controls; the bar keeps the side panels (History undoes what they did). */
function ToolTopBar() {
  const t = useT();
  const view = useView((s) => s.view);
  const title = view === "duplicates" ? t("nav.duplicates") : view === "cleanup" ? t("nav.cleanup") : t("nav.stats");
  return (
    <header className="ss-panel flex flex-wrap items-center gap-2 px-3 py-2">
      <h1 className="m-0 flex-1 text-base font-semibold">{title}</h1>
      <PanelButtons />
    </header>
  );
}

function PanelButtons() {
  const t = useT();
  const sidePanel = useView((s) => s.sidePanel);
  const v = useView.getState;
  return (
    <>
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
    </>
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
