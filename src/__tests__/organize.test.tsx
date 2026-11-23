// Phase 2.2 organize UI against the mock IPC: the filter panel builds the exact ImageQuery, tags, bulk rename
// with conflicts, undo (Ctrl+Z, toast, history), smart filters, confirm dialogs for catalog-only deletions.
import { act, fireEvent, renderHook, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useSettled } from "../components/Cell";
import { THUMB_SETTLE_MS } from "../lib/defaults";
import type { ImageQuery } from "@bindings/ImageQuery";
import type { OpResult } from "@bindings/OpResult";
import { MOCK_SIDECAR_READY } from "../mocks/smart";
import { albumOrder } from "../views/Gallery";
import { callsOf, cell, cmds, lastCall, renderApp, reqOf, useMockApp } from "./harness";

const app = useMockApp({ language: "en" });
const mock = () => app.get();

describe("search and filter panel", () => {
  it("builds the exact ImageQuery from every control (snapshot of the query_images req)", async () => {
    const tag = mock().invoke("create_tag", { name: "Work", color: "#22C55E" });
    await renderApp();
    // The AI-derived filters (colour, category, has text) are usable once the sidecar is ready (degraded.test.tsx
    // covers the other states).
    await act(() => mock().emit("sidecar-status", MOCK_SIDECAR_READY));
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "  invoice " } });
    fireEvent.change(screen.getByRole("combobox", { name: "Search mode" }), { target: { value: "filename" } });
    fireEvent.click(screen.getByRole("button", { name: /^Filters/ }));
    const panel = await screen.findByRole("region", { name: "Image filters" });
    const p = within(panel);
    fireEvent.change(p.getByLabelText("From date"), { target: { value: "2026-11-01" } });
    fireEvent.change(p.getByLabelText("To date"), { target: { value: "2026-11-20" } });
    fireEvent.change(p.getByLabelText("Min width"), { target: { value: "1000" } });
    fireEvent.click(p.getByRole("button", { name: "Phone" }));
    fireEvent.change(p.getByLabelText("Min MB"), { target: { value: "0.5" } });
    fireEvent.click(await p.findByRole("button", { name: "Work" }));
    fireEvent.change(p.getByRole("combobox", { name: "Tag match" }), { target: { value: "all" } });
    fireEvent.click(p.getByRole("checkbox", { name: "Filter by colour" }));
    fireEvent.change(p.getByLabelText("Pick a colour"), { target: { value: "#ff0000" } });
    fireEvent.change(p.getByRole("slider", { name: "Colour tolerance" }), { target: { value: "30" } });
    const categories = p.getByRole("group", { name: "Category" });
    fireEvent.click(within(categories).getByRole("button", { name: "Chat" }));
    fireEvent.click(within(categories).getByRole("button", { name: "Receipt" }));
    fireEvent.click(p.getByRole("checkbox", { name: "Favourites only" }));
    fireEvent.change(p.getByRole("combobox", { name: "Minimum rating" }), { target: { value: "2" } });
    fireEvent.change(p.getByRole("combobox", { name: "Has text" }), { target: { value: "no" } });
    fireEvent.change(p.getByRole("combobox", { name: "Trash" }), { target: { value: "include" } });

    const expected: ImageQuery = {
      text: "invoice", textMode: "filename",
      dateFrom: new Date(2026, 10, 1).getTime(), dateTo: new Date(2026, 10, 20, 23, 59, 59, 999).getTime(),
      minWidth: 1000, maxWidth: null, minHeight: null, maxHeight: null, aspects: ["phone"],
      minSize: 524_288, maxSize: null, folders: [], tagIds: [tag.id], tagMatch: "all", albumId: null,
      color: "#FF0000", colorTolerance: 30, categories: ["chat", "receipt"], favoriteOnly: true, minRating: 2,
      hasText: false, trash: "include", includeMissing: false, sort: { field: "taken", dir: "desc" }, cursor: null, limit: 500,
    };
    await waitFor(() => expect(reqOf(lastCall(mock(), "query_images"))).toEqual(expected));
    expect(screen.getByTestId("filter-count").textContent).toBe("(12)");

    // Clear filters returns to the unfiltered query (already cached, so it may not be re-sent): the whole library.
    fireEvent.click(p.getByRole("button", { name: "Clear filters" }));
    await waitFor(() => expect(screen.getByTestId("status-total").textContent).toContain("10000"));
    expect(screen.queryByTestId("filter-count")).toBeNull();
    expect((screen.getByRole("searchbox") as HTMLInputElement).value).toBe("");
    expect((p.getByRole("checkbox", { name: "Favourites only" }) as HTMLInputElement).checked).toBe(false);
  });

  it("offers semantic search only when the sidecar reports `embed`", async () => {
    await renderApp();
    const option = () => within(screen.getByRole("combobox", { name: "Search mode" })).getByRole("option", { name: /meaning/ }) as HTMLOptionElement;
    expect(option().disabled).toBe(true);
    await act(() => mock().emit("sidecar-status", { state: "ready", detail: null, capabilities: ["ocr", "embed"], device: "cpu", version: "0.1" }));
    await waitFor(() => expect(option().disabled).toBe(false));
  });

  it("offers relevance sort only while a text search is active", async () => {
    await renderApp();
    const sort = () => screen.getByRole("combobox", { name: "Sort" });
    expect(within(sort()).queryByRole("option", { name: "Relevance" })).toBeNull();
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "Screenshot" } });
    await waitFor(() => expect(within(sort()).getByRole("option", { name: "Relevance" })).toBeTruthy());
    fireEvent.change(sort(), { target: { value: "relevance" } });
    await waitFor(() => expect(reqOf(lastCall(mock(), "query_images"))).toMatchObject({ text: "Screenshot", sort: { field: "relevance" } }));
  });

  it("saves the current filter as a smart filter and re-applies it from the sidebar", async () => {
    await renderApp();
    fireEvent.click(screen.getByRole("button", { name: /^Filters/ }));
    fireEvent.click(within(await screen.findByRole("region", { name: "Image filters" })).getByRole("checkbox", { name: "Favourites only" }));
    fireEvent.click(screen.getAllByRole("button", { name: "Save as smart filter…" })[0]);
    const dialog = await screen.findByRole("dialog", { name: "Save smart filter" });
    fireEvent.change(within(dialog).getByRole("textbox"), { target: { value: "Faves" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(cmds(mock())).toContain("save_smart_filter"));
    const saved = reqOf(lastCall(mock(), "save_smart_filter")) as { id: null; name: string; query: ImageQuery };
    expect(saved).toMatchObject({ id: null, name: "Faves", query: { favoriteOnly: true, cursor: null } });

    fireEvent.click(screen.getByRole("button", { name: "Clear filters" }));
    await waitFor(() => expect(reqOf(lastCall(mock(), "query_images"))).toMatchObject({ favoriteOnly: false }));
    const faves = within(screen.getByRole("region", { name: "Image filters" })).getByRole("checkbox", { name: "Favourites only" }) as HTMLInputElement;
    expect(faves.checked).toBe(false);
    fireEvent.click(await screen.findByRole("button", { name: /^Faves/ }));
    await waitFor(() => expect(faves.checked).toBe(true));
    const favorites = mock().images.filter((i) => i.favorite).length;
    await waitFor(() => expect(screen.getByTestId("status-total").textContent).toContain(String(favorites)));
  });
});

describe("organize", () => {
  it("tag assign calls set_tags with the selected images and the chosen tag", async () => {
    const tag = mock().invoke("create_tag", { name: "Receipts", color: "#F59E0B" });
    const grid = await renderApp();
    fireEvent.click(cell(grid, 0));
    fireEvent.click(cell(grid, 2), { ctrlKey: true });
    fireEvent.click(screen.getByRole("button", { name: "Tags…" }));
    const dialog = await screen.findByRole("dialog", { name: "Tag 2 images" });
    const apply = within(dialog).getByRole("button", { name: "Apply" }) as HTMLButtonElement;
    expect(apply.disabled).toBe(true);
    fireEvent.click(await within(dialog).findByRole("button", { name: /Receipts/ }));
    fireEvent.click(apply);
    await waitFor(() => expect(cmds(mock())).toContain("set_tags"));
    expect(reqOf(lastCall(mock(), "set_tags"))).toEqual({ imageIds: [mock().images[0].id, mock().images[2].id], add: [tag.id], remove: [] });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Tag 2 images" })).toBeNull());
    // Every journaled op ends in a toast with Undo.
    const toast = await screen.findByText("Updated tags on 2 images");
    expect(within(toast.parentElement as HTMLElement).getByRole("button", { name: "Undo" })).toBeTruthy();
  });

  it("bulk rename shows conflicts live and blocks Rename until the pattern is unique", async () => {
    const grid = await renderApp();
    fireEvent.click(cell(grid, 0));
    fireEvent.click(cell(grid, 1), { ctrlKey: true });
    fireEvent.click(screen.getByRole("button", { name: "Rename…" }));
    const dialog = await screen.findByRole("dialog", { name: "Rename 2 images" });
    const pattern = within(dialog).getByRole("textbox", { name: "Name pattern" });
    const stems = [0, 1].map((i) => mock().images[i].fileName.replace(/\.png$/, ""));

    fireEvent.change(pattern, { target: { value: "same" } });
    await waitFor(() => expect(dialog.querySelectorAll('tr[data-conflict="true"]').length).toBe(2));
    expect(within(dialog).getByTestId("rename-conflicts").textContent).toContain("2 names clash");
    expect((within(dialog).getByRole("button", { name: /^Rename \d+ images$/ }) as HTMLButtonElement).disabled).toBe(true);
    expect(reqOf(lastCall(mock(), "preview_rename"))).toEqual({ imageIds: [mock().images[0].id, mock().images[1].id], pattern: "same" });

    fireEvent.change(pattern, { target: { value: "{name}-{n}" } });
    await waitFor(() => expect(dialog.querySelectorAll('tr[data-conflict="true"]').length).toBe(0));
    const go = await within(dialog).findByRole("button", { name: "Rename 2 images" });
    expect((go as HTMLButtonElement).disabled).toBe(false);
    expect(cmds(mock())).not.toContain("rename_images");
    fireEvent.click(go);
    await waitFor(() => expect(cmds(mock())).toContain("rename_images"));
    expect(reqOf(lastCall(mock(), "rename_images"))).toEqual({
      items: [
        { imageId: mock().images[0].id, newName: `${stems[0]}-1.png` },
        { imageId: mock().images[1].id, newName: `${stems[1]}-2.png` },
      ],
    });
  });

  it("Ctrl+Z calls undo for the latest batch; the history panel undoes a chosen batch", async () => {
    const grid = await renderApp();
    grid.focus();
    fireEvent.keyDown(grid, { key: "f" });
    await waitFor(() => expect(cmds(mock())).toContain("set_favorite"));
    const favBatch = (lastCall(mock(), "set_favorite")?.result as OpResult).batchId;
    fireEvent.keyDown(grid, { key: "3" });
    await waitFor(() => expect(cmds(mock())).toContain("set_rating"));

    fireEvent.keyDown(window, { key: "z", ctrlKey: true });
    await waitFor(() => expect(cmds(mock())).toContain("undo"));
    expect(reqOf(lastCall(mock(), "undo"))).toEqual({ batchId: null });

    fireEvent.click(screen.getByRole("button", { name: "History" }));
    const history = await screen.findByRole("region", { name: "Undo history" });
    await waitFor(() => expect(within(history).getByText("Undone")).toBeTruthy());
    const favRow = history.querySelector(`[data-batch="${favBatch}"]`) as HTMLElement;
    fireEvent.click(within(favRow).getByRole("button", { name: "Undo" }));
    await waitFor(() => expect(callsOf(mock(), "undo").length).toBe(2));
    expect(reqOf(lastCall(mock(), "undo"))).toEqual({ batchId: favBatch });
  });

  it("Ctrl+Z inside a text field edits text instead of undoing an operation", async () => {
    await renderApp();
    const search = screen.getByRole("searchbox");
    search.focus();
    fireEvent.keyDown(search, { key: "z", ctrlKey: true });
    await act(async () => {});
    expect(cmds(mock())).not.toContain("undo");
  });

  it("deleting a tag asks first and names what is lost; Cancel sends nothing", async () => {
    const tag = mock().invoke("create_tag", { name: "Old", color: "#64748B" });
    mock().invoke("set_tags", { imageIds: [1, 2, 3], add: [tag.id], remove: [] });
    await renderApp();
    fireEvent.click(await screen.findByRole("button", { name: "Delete tag Old" }));
    let dialog = await screen.findByRole("alertdialog", { name: "Delete tag Old?" });
    expect(dialog.textContent).toContain("Tag Old is removed from 3 images.");
    expect(document.activeElement).toBe(within(dialog).getByRole("button", { name: "Cancel" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(cmds(mock())).not.toContain("delete_tag");

    fireEvent.click(screen.getByRole("button", { name: "Delete tag Old" }));
    dialog = await screen.findByRole("alertdialog", { name: "Delete tag Old?" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(reqOf(lastCall(mock(), "delete_tag"))).toEqual({ tagId: tag.id }));
  });

  it("an album click filters by albumId; Add to album calls album_add", async () => {
    const album = mock().invoke("create_album", { name: "Trip" });
    const grid = await renderApp();
    fireEvent.click(cell(grid, 4));
    fireEvent.click(screen.getByRole("button", { name: "Add to album…" }));
    const dialog = await screen.findByRole("dialog", { name: "Add 1 images to an album" });
    fireEvent.click(await within(dialog).findByRole("button", { name: /Trip/ }));
    await waitFor(() => expect(reqOf(lastCall(mock(), "album_add"))).toEqual({ albumId: album.id, imageIds: [mock().images[4].id] }));
    fireEvent.click(await screen.findByRole("button", { name: /^Trip/ }));
    await waitFor(() => expect(reqOf(lastCall(mock(), "query_images"))).toMatchObject({ albumId: album.id }));
  });

  it("album drag-reorder moves the dragged block before the drop target", () => {
    expect(albumOrder([1, 2, 3, 4, 5], [4], 2)).toEqual([1, 4, 2, 3, 5]);
    expect(albumOrder([1, 2, 3, 4, 5], [1, 3], 5)).toEqual([2, 4, 1, 3, 5]);
    expect(albumOrder([1, 2, 3], [2], 3)).toBeNull(); // already right before the target
    expect(albumOrder([1, 2, 3], [2], 2)).toBeNull();
  });
});

describe("grid memory (Deferred decision #31)", () => {
  it("a cell gets its thumbnail only after it stayed mounted THUMB_SETTLE_MS, per image", () => {
    vi.useFakeTimers();
    try {
      const { result, rerender, unmount } = renderHook(({ id }) => useSettled(id, THUMB_SETTLE_MS), { initialProps: { id: 7 as number | undefined } });
      expect(result.current).toBe(false);
      act(() => vi.advanceTimersByTime(THUMB_SETTLE_MS - 1));
      expect(result.current).toBe(false);
      act(() => vi.advanceTimersByTime(1));
      expect(result.current).toBe(true);
      rerender({ id: 8 }); // the virtualizer reused the cell for another image: wait again
      expect(result.current).toBe(false);
      rerender({ id: undefined });
      act(() => vi.advanceTimersByTime(THUMB_SETTLE_MS * 2));
      expect(result.current).toBe(false);
      unmount();
    } finally {
      vi.useRealTimers();
    }
  });

  it("the deferral does not hold thumbnails back once cells have settled", async () => {
    await renderApp(); // waits until thumbnails are shown
    const grid = screen.getByRole("grid");
    const shown = grid.querySelectorAll("img").length;
    expect(shown).toBeGreaterThan(0);
    expect(shown).toBeLessThanOrEqual(grid.querySelectorAll("[data-item]").length);
  });
});
