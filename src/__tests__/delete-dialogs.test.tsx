// Phase 2.2 delete paths against the mock IPC (design-intent § Safety, D8): Delete = in-app trash only; the
// permanent delete dialog (count + size + 6 thumbnails, typed DELETE, red button disabled until typed, never
// focused, Enter inert, Cancel sends nothing, request_permanent_delete -> delete_permanently with its token);
// no key and no palette command reaches it.
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { ImageList } from "../data/images";
import type { PermanentDeleteTicket } from "@bindings/PermanentDeleteTicket";
import { PALETTE_COMMANDS } from "../commands/palette";
import { formatBytes } from "../components/Cell";
import { emptyQuery } from "../lib/query";
import { useView } from "../state/view";
import { cmds, lastCall, PURGE_COMMANDS, renderApp, reqOf, useMockApp } from "./harness";

const app = useMockApp({ language: "en" });
const mock = () => app.get();
const TRASHED = 8;

/** Trashes the 8 newest images, opens the trash view and selects all of them. */
async function trashView(): Promise<HTMLElement> {
  mock().invoke("trash_images", { imageIds: mock().images.slice(0, TRASHED).map((i) => i.id) });
  await renderApp();
  fireEvent.click(screen.getByRole("button", { name: "Trash" }));
  await waitFor(() => expect(screen.getByTestId("status-total").textContent).toContain(String(TRASHED)));
  const grid = screen.getByRole("grid");
  await waitFor(() => expect(grid.querySelectorAll("[data-item]").length).toBe(TRASHED));
  grid.focus();
  fireEvent.keyDown(grid, { key: "a", ctrlKey: true });
  return grid;
}

async function openPurge(): Promise<HTMLElement> {
  fireEvent.click(screen.getByRole("button", { name: "Delete permanently…" }));
  return screen.findByRole("alertdialog", { name: `Permanently delete ${TRASHED} images?` });
}

const purgeButton = (d: HTMLElement) => within(d).getByTestId("purge-confirm") as HTMLButtonElement;
const typeField = (d: HTMLElement) => within(d).getByRole("textbox", { name: "Type DELETE to confirm permanent delete" });
const noPurgeSent = () => PURGE_COMMANDS.forEach((c) => expect(cmds(mock())).not.toContain(c));

describe("Delete key", () => {
  it("calls trash_images only (in-app trash, file untouched)", async () => {
    const grid = await renderApp();
    fireEvent.keyDown(grid, { key: "Delete" });
    await waitFor(() => expect(cmds(mock())).toContain("trash_images"));
    expect(reqOf(lastCall(mock(), "trash_images"))).toEqual({ imageIds: [mock().images[0].id] });
    expect(cmds(mock())).not.toContain("recycle_images");
    noPurgeSent();
  });

  it("no key combination in the trash view opens or sends permanent delete", async () => {
    const grid = await trashView();
    for (const key of ["Delete", "Backspace", "Enter", "d", "D", "x"]) {
      for (const mods of [{}, { shiftKey: true }, { ctrlKey: true }, { ctrlKey: true, shiftKey: true }, { altKey: true }]) {
        fireEvent.keyDown(grid, { key, ...mods });
        fireEvent.keyDown(window, { key, ...mods });
        if (useView.getState().lightbox.mode !== "closed") act(() => useView.getState().closeLightbox());
        act(() => useView.getState().askRecycle(null));
      }
    }
    await act(async () => {});
    expect(screen.queryByRole("alertdialog", { name: /Permanently delete/ })).toBeNull();
    expect(useView.getState().dialog?.kind).not.toBe("purge");
    noPurgeSent();
  });

  it("no palette command opens or sends permanent delete, in the trash view either", async () => {
    await trashView();
    const list: ImageList = { query: emptyQuery(), items: mock().images.slice(0, TRASHED), total: TRASHED, loading: false, error: null, ensureLoaded: () => {} };
    for (const cmd of PALETTE_COMMANDS) {
      act(() => useView.getState().setSelection(list.items.map((i) => i.id), 0));
      await act(async () => {
        await cmd.run({ list });
      });
      expect(useView.getState().dialog?.kind, cmd.id).not.toBe("purge");
      act(() => useView.getState().closeDialog());
    }
    noPurgeSent();
  });
});

describe("permanent delete dialog", () => {
  it("shows count, total size and the first 6 thumbnails; the red button is disabled until DELETE is typed and never focused", async () => {
    await trashView();
    const d = await openPurge();
    const bytes = mock().images.slice(0, TRASHED).reduce((a, i) => a + i.sizeBytes, 0);
    expect(within(d).getByTestId("purge-summary").textContent).toBe(`${TRASHED} images · ${formatBytes(bytes)} in total`);
    expect(d.querySelectorAll("img").length).toBe(6);
    expect(purgeButton(d).disabled).toBe(true);
    expect(document.activeElement).toBe(typeField(d));
    expect(document.activeElement).not.toBe(purgeButton(d));

    for (const wrong of ["delete", "DELET", "DELETE ", " DELETE", "XÓA"]) {
      fireEvent.change(typeField(d), { target: { value: wrong } });
      expect(purgeButton(d).disabled, wrong).toBe(true);
    }
    fireEvent.change(typeField(d), { target: { value: "DELETE" } });
    expect(purgeButton(d).disabled).toBe(false);
    expect(document.activeElement).not.toBe(purgeButton(d));
    noPurgeSent(); // opening and typing send nothing
  });

  it("Enter does not trigger it", async () => {
    await trashView();
    const d = await openPurge();
    fireEvent.change(typeField(d), { target: { value: "DELETE" } });
    fireEvent.keyDown(typeField(d), { key: "Enter" });
    fireEvent.keyDown(d, { key: "Enter" });
    fireEvent.keyDown(window, { key: "Enter" });
    await act(async () => {});
    noPurgeSent();
    expect(screen.getByRole("alertdialog", { name: /Permanently delete/ })).toBeTruthy();
  });

  it("Cancel and Esc send nothing", async () => {
    await trashView();
    let d = await openPurge();
    fireEvent.change(typeField(d), { target: { value: "DELETE" } });
    fireEvent.click(within(d).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog", { name: /Permanently delete/ })).toBeNull());
    d = await openPurge();
    fireEvent.change(typeField(d), { target: { value: "DELETE" } });
    fireEvent.keyDown(typeField(d), { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("alertdialog", { name: /Permanently delete/ })).toBeNull());
    await act(async () => {});
    noPurgeSent();
  });

  it("calls request_permanent_delete, then delete_permanently with that call's token and the typed word", async () => {
    await trashView();
    const ids = mock().images.slice(0, TRASHED).map((i) => i.id);
    const d = await openPurge();
    fireEvent.change(typeField(d), { target: { value: "DELETE" } });
    fireEvent.click(purgeButton(d));
    await waitFor(() => expect(cmds(mock())).toContain("delete_permanently"));
    const order = cmds(mock()).filter((c) => PURGE_COMMANDS.includes(c));
    expect(order).toEqual(["request_permanent_delete", "delete_permanently"]);
    const request = lastCall(mock(), "request_permanent_delete");
    expect([...(reqOf(request) as { imageIds: number[] }).imageIds].sort((a, b) => a - b)).toEqual([...ids].sort((a, b) => a - b));
    const ticket = request?.result as PermanentDeleteTicket;
    expect(reqOf(lastCall(mock(), "delete_permanently"))).toEqual({ token: ticket.token, typedConfirmation: "DELETE" });
    await screen.findByText(`Permanently deleted ${TRASHED} images`);
    // The purge toast offers no Undo: the batch is `undoable: false`.
    expect(within(screen.getByText(`Permanently deleted ${TRASHED} images`).parentElement as HTMLElement).queryByRole("button", { name: "Undo" })).toBeNull();
    await waitFor(() => expect(screen.queryByRole("alertdialog", { name: /Permanently delete/ })).toBeNull());
  });

  it("deletes nothing when the backend's set differs from what the dialog showed", async () => {
    await trashView();
    const d = await openPurge();
    mock().images[0].sizeBytes += 1; // the catalog changed after the dialog was drawn
    fireEvent.change(typeField(d), { target: { value: "DELETE" } });
    fireEvent.click(purgeButton(d));
    await within(d).findByRole("alert");
    expect(cmds(mock())).toContain("request_permanent_delete");
    expect(cmds(mock())).not.toContain("delete_permanently");
    expect(purgeButton(d).disabled).toBe(true); // the typed word is cleared: review and type again
  });
});
