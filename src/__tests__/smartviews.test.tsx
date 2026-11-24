// Phase 3.4b: the smart views open in the click's own commit. Its own file on purpose: React.lazy caches a resolved
// chunk on the lazy object for the rest of the module graph, so after another test had opened a lazy view these reads
// passed with lazy imports too (seen: smart.test.tsx stayed green with the old lazy ToolView). In a fresh file the
// synchronous `getBy` reads below fail on a lazy view (seen red with the old ToolView).
import { readFileSync } from "node:fs";
import { join } from "node:path";
import process from "node:process";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { cmds, renderApp, useMockApp } from "./harness";

const app = useMockApp({ language: "en" });
const mock = () => app.get();

describe("smart views open at once (phase 3.4b)", () => {
  // React holds a Suspense fallback for >= 300 ms; the views are eager imports now, so each one is in the DOM in the
  // same commit as the click (a lazy chunk fails these synchronous `getBy` reads), and data that is still on its way
  // shows a labelled loading state, never an empty pane.
  it("Dashboard, Duplicates, Cleanup and Settings > AI render in the click's own commit", async () => {
    await renderApp();
    await waitFor(() => expect(cmds(mock())).toContain("get_stats")); // the sidebar's folder list reads it at start
    await screen.findByRole("button", { name: /^4ERD/ });
    fireEvent.click(screen.getByRole("button", { name: "Dashboard" }));
    expect(within(screen.getByRole("region", { name: "Dashboard" })).getByTestId("tile-Images")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Duplicates" }));
    const dup = screen.getByRole("region", { name: "Duplicates" });
    const loading = within(dup).getByRole("status");
    expect(loading.getAttribute("aria-busy")).toBe("true");
    expect(loading.textContent).toBe("Looking for duplicates…");
    await within(dup).findByTestId("dup-summary");
    expect(within(dup).queryByRole("status")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Cleanup" }));
    expect(within(screen.getByRole("region", { name: "Cleanup" })).getByRole("button", { name: "Start" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    fireEvent.click(within(screen.getByRole("dialog", { name: "Settings" })).getByRole("tab", { name: "AI" }));
    expect(within(screen.getByRole("dialog", { name: "Settings" })).getByTestId("ai-state")).toBeTruthy();
    expect(document.querySelector(".ss-shimmer[aria-busy]")).toBeNull();
  });

  it("the loading state's dot holds still with motion off and under prefers-reduced-motion", () => {
    // Read from disk: vitest replaces CSS content with "" unless CSS processing is on (themes.test.ts).
    const css = readFileSync(join(process.cwd(), "src", "index.css"), "utf8");
    expect(css).toMatch(/\[data-motion="off"\]\s+\.ss-pulse\s*\{\s*animation:\s*none;\s*\}/);
    expect(css).toMatch(/@media \(prefers-reduced-motion: reduce\)\s*\{[^@]*\.ss-pulse[^}]*\{\s*animation:\s*none;\s*\}/);
  });
});
