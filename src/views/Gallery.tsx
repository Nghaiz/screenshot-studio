// The browsing surface: one scroll container (role=grid, keyboard focus stays on it; the focused cell is
// announced via aria-activedescendant), a layout body chosen by the prefs, and an optional date scrubber.
// Pointer: click / Shift / Ctrl selection, lasso on the background, in-app drag of cells (onto sidebar
// targets, or onto another cell to reorder an album).
import { useCallback, useEffect, useMemo, useRef, type DragEvent, type MouseEvent } from "react";
import { reorderAlbum } from "../actions/organize";
import { notify } from "../actions/report";
import { handleGridKey } from "../commands/gridKeys";
import { rangeIds } from "../commands/targets";
import { Cell, cellDomId } from "../components/Cell";
import { EmptyState } from "../components/EmptyState";
import { activeFilterCount } from "../lib/query";
import { SCROLL_IDLE_MS } from "../lib/defaults";
import { readDragIds, setDragIds, isImageDrag } from "../lib/dnd";
import type { ImageList } from "../data/images";
import { useTimeline } from "../data/library";
import { useSessions } from "../data/smart";
import { translate, useT } from "../i18n";
import { usePrefs } from "../state/prefs";
import { useView } from "../state/view";
import { DENSITY_GAP } from "../themes";
import { JustifiedGrid } from "./JustifiedGrid";
import { Lasso, useLasso } from "./Lasso";
import { ListView } from "./ListView";
import { MasonryGrid } from "./MasonryGrid";
import { Scrubber } from "./Scrubber";
import { monthGroups, sessionGroups, TimelineGrid } from "./TimelineGrid";
import type { LayoutProps, Nav, RenderCell } from "./types";
import { UniformGrid } from "./UniformGrid";
import { useElementSize } from "./useElementSize";
import { useScrollingFlag } from "./useScrollingFlag";

export function Gallery({ list }: { list: ImageList }) {
  const t = useT();
  const { layout, thumbSize, density, language, timelineGroup } = usePrefs();
  const viewName = useView((s) => s.view);
  const focusIndex = useView((s) => s.focusIndex);
  const selection = useView((s) => s.selection);
  const scrollRequest = useView((s) => s.scrollRequest);
  const scrollRef = useRef<HTMLDivElement>(null);
  const navRef = useRef<Nav | null>(null);
  const size = useElementSize(scrollRef);
  useScrollingFlag(scrollRef, SCROLL_IDLE_MS); // shimmers hold still mid-scroll (index.css, Deferred decision #31)
  const gap = DENSITY_GAP[density];
  const isTimeline = viewName === "timeline";
  const timeline = useTimeline(list.query, isTimeline);
  const bySession = isTimeline && timelineGroup === "session";
  const sessions = useSessions(list.query, bySession);

  useEffect(() => {
    if (scrollRequest > 0) navRef.current?.scrollTo(focusIndex);
    // Deliberately keyed on scrollRequest only: an explicit request scrolls (keyboard, scrubber), a click does not.
  }, [scrollRequest]); // focusIndex is read at request time on purpose

  useEffect(() => {
    if (list.total > 0 && focusIndex > list.total - 1) useView.getState().focus(list.total - 1, false);
  }, [list.total, focusIndex]);

  const onPick = useCallback(
    (index: number, e: MouseEvent) => {
      const v = useView.getState();
      const item = list.items[index];
      v.focus(index, false);
      scrollRef.current?.focus({ preventScroll: true });
      if (!item) return;
      if (e.shiftKey) v.setSelection(rangeIds(list.items, v.anchorIndex ?? index, index));
      else if (e.ctrlKey || e.metaKey) {
        v.toggleSelected(item.id);
        useView.setState({ anchorIndex: index });
      } else v.setSelection([item.id], index);
    },
    [list.items],
  );
  const onOpen = useCallback((index: number) => useView.getState().openLightbox({ mode: "view", index }), []);
  const onDragStart = useCallback(
    (index: number, e: DragEvent) => {
      const item = list.items[index];
      if (!item) return;
      const sel = useView.getState().selection;
      setDragIds(e, sel.has(item.id) ? [...sel] : [item.id]);
    },
    [list.items],
  );
  const albumId = useView((s) => s.filter.albumId);
  const onDropOn = useCallback(
    (index: number, e: DragEvent) => {
      if (albumId === null || !isImageDrag(e)) return;
      e.preventDefault();
      const dragged = readDragIds(e);
      if (!dragged) return;
      const order = albumOrder(list.items.map((i) => i.id), dragged, list.items[index]?.id ?? null);
      if (list.items.length < list.total) notify("organize.reorderNeedsAll");
      else if (order) void reorderAlbum(albumId, order);
    },
    [albumId, list.items, list.total],
  );
  const lasso = useLasso(scrollRef, list.items);
  // Session headers once the sessions are known; month headers meanwhile (same order, so the indices agree). Keyed on
  // the language, not on `t` (a new function every render would rebuild the rows and re-measure every render).
  const groups = useMemo(
    () =>
      bySession && sessions.data
        ? sessionGroups(sessions.data, language, (key, vars) => translate(language, key, vars))
        : monthGroups(timeline.data ?? [], language),
    [bySession, sessions.data, timeline.data, language],
  );

  const renderCell: RenderCell = (index, style, variant) => {
    const item = list.items[index];
    return (
      <Cell
        key={item?.id ?? `p${index}`}
        index={index}
        item={item}
        focused={index === focusIndex}
        selected={item ? selection.has(item.id) : false}
        style={style}
        variant={variant}
        onPick={onPick}
        onOpen={onOpen}
        onDragStart={onDragStart}
        onDropOn={albumId === null ? undefined : onDropOn}
      />
    );
  };

  if (list.error) {
    return <Centered text={t("grid.error", { message: `${list.error.message}${list.error.detail ? ` (${list.error.detail})` : ""}` })} />;
  }
  if (!list.loading && list.total === 0) {
    if (viewName === "trash") return <EmptyState text={t("grid.emptyTrash")} />;
    if (activeFilterCount(useView.getState().filter) > 0) {
      const reset = <button type="button" className="ss-btn" onClick={() => useView.getState().resetFilter()}>{t("cmd.resetFilters")}</button>;
      return <EmptyState text={t("grid.emptyFiltered")} action={reset} />;
    }
    return <EmptyState text={t("grid.empty")} />;
  }

  const props: LayoutProps = { scrollRef, width: Math.max(0, size.width - 2 * gap), height: size.height, list, thumbSize, gap, navRef, renderCell };
  const buckets = timeline.data ?? [];
  const body = isTimeline ? (
    <TimelineGrid {...props} groups={groups} />
  ) : layout === "masonry" ? (
    <MasonryGrid {...props} />
  ) : layout === "justified" ? (
    <JustifiedGrid {...props} />
  ) : layout === "list" ? (
    <ListView {...props} />
  ) : (
    <UniformGrid {...props} />
  );

  return (
    <div className="flex min-h-0 flex-1 gap-2">
      <div
        ref={scrollRef}
        role="grid"
        aria-label={t("grid.label")}
        aria-rowcount={list.total}
        aria-activedescendant={list.total > 0 ? cellDomId(focusIndex) : undefined}
        aria-multiselectable
        tabIndex={0}
        className="relative min-h-0 flex-1 overflow-y-auto overflow-x-hidden outline-none"
        onKeyDown={(e) => {
          if (handleGridKey(e, list, navRef.current)) e.preventDefault();
        }}
        {...lasso.handlers}
      >
        {size.width > 0 && body}
      </div>
      <Lasso rect={lasso.rect} />
      {isTimeline && buckets.length > 0 && (
        <Scrubber buckets={buckets} lang={language} onJump={(first) => useView.getState().focus(first)} />
      )}
    </div>
  );
}

/**
 * New album order after dropping `dragged` onto the image `targetId`: the dragged ids (in their current
 * order) move to just before the target. Null when nothing would change.
 */
export function albumOrder(current: readonly number[], dragged: readonly number[], targetId: number | null): number[] | null {
  const moving = new Set(dragged);
  if (targetId === null || moving.has(targetId)) return null;
  const rest = current.filter((id) => !moving.has(id));
  const block = current.filter((id) => moving.has(id));
  const at = rest.indexOf(targetId);
  const next = [...rest.slice(0, at), ...block, ...rest.slice(at)];
  return next.every((id, i) => id === current[i]) ? null : next;
}

function Centered({ text }: { text: string }) {
  return (
    <div className="flex flex-1 items-center justify-center p-8">
      <p className="ss-panel ss-muted max-w-lg px-6 py-4 text-center text-sm">{text}</p>
    </div>
  );
}
