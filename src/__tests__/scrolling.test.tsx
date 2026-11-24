// Gallery scroll flag (phase 3.4, memory gate, Deferred decision #31): `data-scrolling` is set while the grid scrolls
// and cleared SCROLL_IDLE_MS after the last scroll event, and index.css pauses placeholder shimmers under it.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import process from "node:process";
import { act, cleanup, render } from "@testing-library/react";
import { useRef } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SCROLL_IDLE_MS } from "../lib/defaults";
import { useScrollingFlag } from "../views/useScrollingFlag";

function Scroller() {
  const ref = useRef<HTMLDivElement>(null);
  useScrollingFlag(ref, SCROLL_IDLE_MS);
  return <div ref={ref} data-testid="scroller" />;
}

const scroll = (el: HTMLElement) => act(() => void el.dispatchEvent(new Event("scroll")));
const advance = (ms: number) => act(() => void vi.advanceTimersByTime(ms));

describe("useScrollingFlag", () => {
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it("sets data-scrolling on the first scroll event and keeps it while events keep coming", () => {
    vi.useFakeTimers();
    const { getByTestId } = render(<Scroller />);
    const el = getByTestId("scroller");
    expect(el.hasAttribute("data-scrolling")).toBe(false);
    scroll(el);
    expect(el.hasAttribute("data-scrolling")).toBe(true);
    for (let i = 0; i < 5; i++) {
      advance(SCROLL_IDLE_MS - 1);
      scroll(el);
    }
    expect(el.hasAttribute("data-scrolling")).toBe(true);
  });

  it("clears it SCROLL_IDLE_MS after the last scroll event, and on unmount", () => {
    vi.useFakeTimers();
    const { getByTestId, unmount } = render(<Scroller />);
    const el = getByTestId("scroller");
    scroll(el);
    advance(SCROLL_IDLE_MS - 1);
    expect(el.hasAttribute("data-scrolling")).toBe(true);
    advance(1);
    expect(el.hasAttribute("data-scrolling")).toBe(false);
    scroll(el);
    unmount();
    expect(el.hasAttribute("data-scrolling")).toBe(false);
  });

  it("index.css pauses (does not remove) the shimmer under it", () => {
    // Read from disk: vitest replaces CSS content with "" unless CSS processing is on (themes.test.ts).
    const css = readFileSync(join(process.cwd(), "src", "index.css"), "utf8");
    expect(css).toMatch(/\[data-scrolling\]\s+\.ss-shimmer\s*\{\s*animation-play-state:\s*paused;\s*\}/);
    expect(css).not.toMatch(/\.ss-cell\s+\.ss-shimmer\s*\{[^}]*animation:\s*none/);
  });
});
