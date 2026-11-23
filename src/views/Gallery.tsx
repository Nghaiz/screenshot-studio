// The browsing surface: one scroll container (role=grid, keyboard focus stays on it; the focused cell is
// announced via aria-activedescendant), a layout body chosen by the prefs, and an optional date scrubber.
import { useCallback, useEffect, useRef, type MouseEvent } from "react";
import { handleGridKey } from "../commands/gridKeys";
import { rangeIds } from "../commands/targets";
import { Cell, cellDomId } from "../components/Cell";
import type { ImageList } from "../data/images";
import { useTimeline } from "../data/library";
import { useT } from "../i18n";
import { usePrefs } from "../state/prefs";
import { useView } from "../state/view";
import { DENSITY_GAP } from "../themes";
import { JustifiedGrid } from "./JustifiedGrid";
import { ListView } from "./ListView";
import { MasonryGrid } from "./MasonryGrid";
import { Scrubber } from "./Scrubber";
import { TimelineGrid } from "./TimelineGrid";
import type { LayoutProps, Nav, RenderCell } from "./types";
import { UniformGrid } from "./UniformGrid";
import { useElementSize } from "./useElementSize";

export function Gallery({ list }: { list: ImageList }) {
  const t = useT();
  const { layout, thumbSize, density, language } = usePrefs();
  const viewName = useView((s) => s.view);
  const focusIndex = useView((s) => s.focusIndex);
  const selection = useView((s) => s.selection);
  const scrollRequest = useView((s) => s.scrollRequest);
  const scrollRef = useRef<HTMLDivElement>(null);
  const navRef = useRef<Nav | null>(null);
  const size = useElementSize(scrollRef);
  const gap = DENSITY_GAP[density];
  const isTimeline = viewName === "timeline";
  const timeline = useTimeline(list.query, isTimeline);

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
      />
    );
  };

  if (list.error) {
    return <Centered text={t("grid.error", { message: `${list.error.message}${list.error.detail ? ` (${list.error.detail})` : ""}` })} />;
  }
  if (!list.loading && list.total === 0) return <Centered text={t("grid.empty")} />;

  const props: LayoutProps = { scrollRef, width: Math.max(0, size.width - 2 * gap), height: size.height, list, thumbSize, gap, navRef, renderCell };
  const buckets = timeline.data ?? [];
  const body = isTimeline ? (
    <TimelineGrid {...props} buckets={buckets} lang={language} />
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
      >
        {size.width > 0 && body}
      </div>
      {isTimeline && buckets.length > 0 && (
        <Scrubber buckets={buckets} lang={language} onJump={(first) => useView.getState().focus(first)} />
      )}
    </div>
  );
}

function Centered({ text }: { text: string }) {
  return (
    <div className="flex flex-1 items-center justify-center p-8">
      <p className="ss-panel ss-muted max-w-lg px-6 py-4 text-center text-sm">{text}</p>
    </div>
  );
}
