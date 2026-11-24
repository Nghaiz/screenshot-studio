// Marks a scroll container with `data-scrolling` while it scrolls, cleared `idleMs` after the last scroll event.
// Written straight onto the DOM node: React state per scroll event would re-render the whole grid on every frame.
// CSS keys off it (`[data-scrolling] .ss-shimmer` in index.css: placeholder shimmers hold still mid-scroll).
import { useEffect, type RefObject } from "react";

export const SCROLLING_ATTR = "scrolling";

export function useScrollingFlag(ref: RefObject<HTMLElement | null>, idleMs: number): void {
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const onScroll = () => {
      if (timer === undefined) el.dataset[SCROLLING_ATTR] = "";
      else clearTimeout(timer);
      timer = setTimeout(() => {
        timer = undefined;
        delete el.dataset[SCROLLING_ATTR];
      }, idleMs);
    };
    el.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      el.removeEventListener("scroll", onScroll);
      clearTimeout(timer);
      delete el.dataset[SCROLLING_ATTR];
    };
  }, [ref, idleMs]);
}
