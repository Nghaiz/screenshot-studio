// Browse UI against the mock IPC (phase 1.3): virtualized 10,000-item grid, keyboard map, lightbox,
// Delete = in-app trash, Ctrl+K palette, and no keyboard or palette path to permanent delete.
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { PALETTE_COMMANDS } from "../commands/palette";
import type { ImageList } from "../data/images";
import { emptyQuery } from "../lib/query";
import { installMockIpc, type MockHandle } from "../mocks/ipc";
import { createQueryClient, Root } from "../Root";
import { DEFAULT_PREFS, usePrefs } from "../state/prefs";
import { useView } from "../state/view";
import { justifiedRows, moveRows, timelineRows, uniformGeometry, uniformRows } from "../views/layout";

const W = 1200;
const H = 800;
const FORBIDDEN = ["request_permanent_delete", "delete_permanently"];
const initialView = useView.getState();
let mock: MockHandle;

beforeAll(() => {
  // jsdom has no layout: give every element a viewport-sized box so the virtualizer has something to fill.
  Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get: () => W });
  Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => H });
  Element.prototype.scrollTo ??= function scrollTo() {};
  window.matchMedia ??= ((q: string) => ({
    matches: false, media: q, onchange: null, addListener() {}, removeListener() {},
    addEventListener() {}, removeEventListener() {}, dispatchEvent: () => false,
  })) as unknown as typeof window.matchMedia;
});

beforeEach(() => {
  localStorage.clear();
  usePrefs.setState({ ...DEFAULT_PREFS, motion: "off", animatedBg: false });
  useView.setState({ ...initialView, selection: new Set(), jobs: {}, toasts: [] }, true);
  mock = installMockIpc();
});

afterEach(() => {
  cleanup();
  mock.uninstall();
});

const itemNodes = (grid: HTMLElement) => grid.querySelectorAll("[data-item]");
const calledCmds = () => mock.calls.map((c) => c.cmd);

async function renderApp(): Promise<HTMLElement> {
  render(<Root client={createQueryClient()} />);
  const grid = await screen.findByRole("grid");
  await waitFor(() => expect(grid.querySelectorAll("img").length).toBeGreaterThan(0));
  return grid;
}

const focusedIndex = (grid: HTMLElement) => grid.querySelector('[data-focused="true"]')?.getAttribute("data-index");

describe("browse grid", () => {
  it("renders 10,000 mock images with fewer than 300 item nodes in the DOM", async () => {
    const grid = await renderApp();
    expect(grid.getAttribute("aria-rowcount")).toBe("10000");
    expect(screen.getByTestId("status-total").textContent).toContain("10000");
    const n = itemNodes(grid).length;
    expect(n).toBeGreaterThan(0);
    expect(n).toBeLessThan(300);
  });

  it("keeps the DOM bounded in every layout", async () => {
    for (const layout of ["masonry", "justified", "list"] as const) {
      act(() => usePrefs.getState().set({ layout }));
      const grid = await renderApp();
      await waitFor(() => expect(itemNodes(grid).length).toBeGreaterThan(0));
      expect(itemNodes(grid).length, layout).toBeLessThan(300);
      cleanup();
    }
  });

  it("moves focus with the arrow keys (3 columns at 1200 px, 320 px thumbs)", async () => {
    const grid = await renderApp();
    grid.focus();
    expect(focusedIndex(grid)).toBe("0");
    fireEvent.keyDown(grid, { key: "ArrowRight" });
    expect(focusedIndex(grid)).toBe("1");
    expect(grid.getAttribute("aria-activedescendant")).toBe("ss-cell-1");
    fireEvent.keyDown(grid, { key: "ArrowDown" });
    expect(focusedIndex(grid)).toBe("4");
    fireEvent.keyDown(grid, { key: "ArrowLeft" });
    fireEvent.keyDown(grid, { key: "ArrowUp" });
    expect(focusedIndex(grid)).toBe("0");
    fireEvent.keyDown(grid, { key: "ArrowUp" });
    expect(focusedIndex(grid)).toBe("0");
  });

  it("opens the lightbox on Enter for the focused image", async () => {
    const grid = await renderApp();
    fireEvent.keyDown(grid, { key: "ArrowRight" });
    fireEvent.keyDown(grid, { key: "Enter" });
    const name = mock.images[1].fileName; // newest first; index 1 = second-newest
    const dialog = await screen.findByRole("dialog", { name: new RegExp(name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")) });
    expect(dialog).toBeTruthy();
  });

  it("Delete calls trash_images (in-app trash) for the focused image, never another delete", async () => {
    const grid = await renderApp();
    fireEvent.keyDown(grid, { key: "Delete" });
    await waitFor(() => expect(calledCmds()).toContain("trash_images"));
    const call = mock.calls.find((c) => c.cmd === "trash_images");
    expect(call?.payload).toEqual({ req: { imageIds: [mock.images[0].id] } });
    expect(calledCmds()).not.toContain("recycle_images");
    for (const f of FORBIDDEN) expect(calledCmds()).not.toContain(f);
    await waitFor(() => expect(screen.getByTestId("status-total").textContent).toContain("9999"));
  });

  it("no Delete/Backspace key combination reaches permanent delete or the Recycle Bin without a confirm", async () => {
    const grid = await renderApp();
    for (const key of ["Delete", "Backspace"]) {
      for (const mods of [{}, { shiftKey: true }, { ctrlKey: true }, { ctrlKey: true, shiftKey: true }, { altKey: true }]) {
        fireEvent.keyDown(grid, { key, ...mods });
      }
    }
    await act(async () => {});
    for (const f of FORBIDDEN) expect(calledCmds()).not.toContain(f);
    expect(calledCmds()).not.toContain("recycle_images");
    expect(screen.getByRole("alertdialog")).toBeTruthy(); // Shift+Delete asked first
  });
});

describe("first run", () => {
  it("offers the default OneDrive Screenshots root and indexes it on click", async () => {
    mock.uninstall();
    mock = installMockIpc({ firstRun: true });
    render(<Root client={createQueryClient()} />);
    fireEvent.click(await screen.findByRole("button", { name: "Dùng thư mục mặc định" }));
    await waitFor(() => expect(calledCmds()).toContain("start_index"));
    const add = mock.calls.find((c) => c.cmd === "add_library_root");
    expect(add?.payload).toEqual({ req: { path: String.raw`C:\Users\Nghaiz\OneDrive\Pictures\Screenshots` } });
    expect(await screen.findByRole("grid")).toBeTruthy();
  });
});

describe("command palette", () => {
  it("opens on Ctrl+K and lists commands", async () => {
    await renderApp();
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    const palette = await screen.findByRole("dialog", { name: "Bảng lệnh" });
    expect(palette.querySelectorAll('[role="option"]').length).toBe(PALETTE_COMMANDS.length);
  });

  it("no palette command calls request_permanent_delete (whole command list run against the mock)", async () => {
    const list: ImageList = {
      query: emptyQuery(), items: mock.images.slice(0, 50), total: 50, loading: false, error: null, ensureLoaded: () => {},
    };
    useView.getState().setSelection([mock.images[0].id, mock.images[1].id], 0);
    const ids = new Set(PALETTE_COMMANDS.map((c) => c.id));
    expect(ids.size).toBe(PALETTE_COMMANDS.length);
    for (const destructive of ["images.trash", "images.recycle", "go.trash"]) expect(ids).toContain(destructive);
    for (const cmd of PALETTE_COMMANDS) {
      await act(async () => {
        await cmd.run({ list });
      });
      useView.getState().setSelection([mock.images[0].id], 0);
    }
    expect(calledCmds()).toContain("trash_images");
    for (const f of FORBIDDEN) expect(calledCmds()).not.toContain(f);
  });
});

describe("layout math", () => {
  it("uniform geometry fills the width exactly", () => {
    const g = uniformGeometry(10_000, 1180, 320, 10);
    expect(g.cols).toBe(3);
    expect(g.cell * 3 + 20).toBeCloseTo(1180);
    expect(g.rows).toBe(3334);
  });

  it("justified rows fill the width and never stretch the last row", () => {
    const aspects = [16 / 9, 9 / 20, 1, 21 / 9, 16 / 9, 4 / 3, 9 / 20];
    const rows = justifiedRows(aspects.length, (i) => aspects[i], 1000, 200, 8);
    for (const r of rows.slice(0, -1)) {
      expect(r.widths.reduce((a, b) => a + b, 0) + 8 * (r.end - r.start - 1)).toBeCloseTo(1000);
    }
    expect(rows[rows.length - 1].height).toBe(200);
    expect(rows.map((r) => r.end - r.start).reduce((a, b) => a + b, 0)).toBe(aspects.length);
  });

  it("timeline rows put a header before each bucket and index items continuously", () => {
    const rows = timelineRows([{ startMs: 2, count: 4 }, { startMs: 1, count: 2 }], 3);
    expect(rows.map((r) => r.kind)).toEqual(["header", "items", "items", "header", "items"]);
    expect(rows[3]).toMatchObject({ kind: "header", firstIndex: 4 });
    expect(rows[4]).toMatchObject({ start: 4, end: 6 });
    expect(moveRows(rows, 1, 1)).toBe(3); // skips the header, keeps the column offset clamped
    expect(moveRows(rows, 3, 1)).toBe(4);
    expect(moveRows(rows, 0, -1)).toBe(0);
  });

  it("moveRows keeps the column across uniform rows", () => {
    const rows = uniformRows(10, 4);
    expect(moveRows(rows, 5, 1)).toBe(9);
    expect(moveRows(rows, 6, 1)).toBe(9);
    expect(moveRows(rows, 9, -2)).toBe(1);
  });
});
