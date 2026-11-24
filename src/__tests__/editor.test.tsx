// Phase 4.2 editor: crop + arrow + blur render a PNG of the crop's FULL size (the editing canvas is downscaled) and
// `save_edited_image` gets those PNG bytes as a raw body with the right `x-*` headers; overwrite asks once and
// explains the backup. jsdom has no canvas: fakeCanvas.ts records the 2D calls and encodes a real PNG in toBlob.
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { pngSize } from "../mocks/live";
import { useView } from "../state/view";
import { installFakeCanvas, type FakeCanvas } from "./fakeCanvas";
import { callsOf, cell, renderApp, useMockApp } from "./harness";

/** The original the editor loads: 3200 x 2000, twice MAX_EDIT_EDGE, so the editing canvas is drawn at 0.5. */
const SRC = { w: 3200, h: 2000 };
const FAKE_IMAGE = { fake: "original" };
vi.mock("../editor/source", () => ({ loadEditSource: async () => ({ image: FAKE_IMAGE, width: 3200, height: 2000 }) }));

const app = useMockApp({ language: "en" });
const mock = () => app.get();
let canvas: FakeCanvas;
beforeEach(() => {
  canvas = installFakeCanvas();
});
afterEach(() => canvas.uninstall());

async function openEditor() {
  const grid = await renderApp();
  const name = cell(grid, 0).getAttribute("aria-label");
  const img = mock().images.find((i) => i.fileName === name);
  if (!img) throw new Error(`no mock image named ${name}`);
  fireEvent.click(within(screen.getByRole("toolbar", { name: "Organize" })).getByRole("button", { name: "Edit" }));
  const editor = await screen.findByRole("dialog", { name: `Editing ${img.fileName}` });
  const surface = (await within(editor).findByTestId("editor-canvas")) as HTMLCanvasElement;
  return { img, editor, surface };
}

/** A pointer drag on the editing canvas, in canvas pixels (= client px, see fakeCanvas.ts). */
function drag(surface: HTMLCanvasElement, from: [number, number], to: [number, number]) {
  fireEvent.pointerDown(surface, { clientX: from[0], clientY: from[1], pointerId: 1, button: 0 });
  fireEvent.pointerMove(surface, { clientX: to[0], clientY: to[1], pointerId: 1 });
  fireEvent.pointerUp(surface, { clientX: to[0], clientY: to[1], pointerId: 1 });
}

const tool = (editor: HTMLElement, name: string) =>
  fireEvent.click(within(within(editor).getByRole("group", { name: "Tools" })).getByRole("button", { name }));

describe("quick editor (phase 4.2)", () => {
  it("crop 16:9 + arrow + blur: the saved PNG is the crop at full size, sent raw with the x-* headers", async () => {
    const { img, editor, surface } = await openEditor();
    expect([surface.width, surface.height]).toEqual([SRC.w / 2, SRC.h / 2]); // edited on a downscaled view

    fireEvent.click(within(within(editor).getByRole("group", { name: "Crop ratio" })).getByRole("button", { name: "16:9" }));
    expect(within(editor).getByTestId("editor-size").textContent).toBe("Saved image: 3200 × 1800 px");

    tool(editor, "Arrow");
    drag(surface, [100, 100], [600, 400]); // oriented px (200,200) -> (1200,800)
    tool(editor, "Blur");
    drag(surface, [800, 500], [1000, 600]); // oriented rect 1600,1000 400x200

    fireEvent.change(within(editor).getByRole("combobox"), { target: { value: "png" } });
    fireEvent.click(within(editor).getByRole("button", { name: "Save copy" }));
    await waitFor(() => expect(callsOf(mock(), "save_edited_image")).toHaveLength(1));

    // The PNG the editor encoded: one export canvas at the crop's full size.
    expect(canvas.blobs).toHaveLength(1);
    const out = canvas.blobs[0];
    expect([out.width, out.height]).toEqual([3200, 1800]);
    const ops = canvas.on(out.canvas);
    // The original drawn at full size, shifted up by the crop's y (100 px), no rotation.
    expect(ops.find((c) => c.op === "drawImage")?.args).toEqual([FAKE_IMAGE, -SRC.w / 2, -SRC.h / 2, SRC.w, SRC.h]);
    expect(ops.find((c) => c.op === "setTransform")?.args).toEqual([1, 0, 0, 1, -0, -100]);
    // The arrow: shaft from its start point, then a filled head at its tip.
    expect(ops.find((c) => c.op === "moveTo")?.args).toEqual([200, 200]);
    expect(ops.some((c) => c.op === "moveTo" && c.args[0] === 1200 && c.args[1] === 800)).toBe(true);
    expect(ops.filter((c) => c.op === "stroke" || c.op === "fill").length).toBeGreaterThanOrEqual(2);
    // The blur: the region (canvas px, crop offset applied) clipped and redrawn through a blur filter.
    expect(ops.some((c) => c.op === "rect" && JSON.stringify(c.args) === JSON.stringify([1600, 900, 400, 200]))).toBe(true);
    expect(ops.find((c) => c.op === "set:filter")?.args).toEqual(["blur(16px)"]);
    expect(ops.findIndex((c) => c.op === "set:filter")).toBeGreaterThan(ops.findIndex((c) => c.op === "stroke"));

    // Sent as the raw body, byte for byte, with the contract headers.
    const sent = callsOf(mock(), "save_edited_image")[0];
    expect(sent.payload).toBeInstanceOf(Uint8Array);
    expect(Array.from(sent.payload as Uint8Array)).toEqual(Array.from(out.bytes));
    expect(pngSize(sent.payload as Uint8Array)).toEqual({ width: 3200, height: 1800 });
    expect(sent.headers).toEqual({ "x-image-id": String(img.id), "x-mode": "copy", "x-format": "png" });

    await screen.findByText(`Saved a copy: ${img.fileName.replace(/\.[^.]+$/, "")} (edited).png`);
    expect(useView.getState().editorId).toBeNull();
  });

  it("overwrite asks once, names the backup, then sends x-mode overwrite (rotation swaps the saved size)", async () => {
    const { img, editor } = await openEditor();
    fireEvent.click(within(editor).getByRole("button", { name: "Rotate right" }));
    expect(within(editor).getByTestId("editor-size").textContent).toBe("Saved image: 2000 × 3200 px");

    fireEvent.click(within(editor).getByRole("button", { name: "Overwrite…" }));
    const ask = screen.getByRole("alertdialog", { name: "Overwrite the original?" });
    expect(within(ask).getByTestId("ask-body").textContent).toBe(
      "The original is kept as a backup in mock://data\\edit-backups. Undo (Ctrl+Z or the History panel) puts it back.",
    );
    expect(callsOf(mock(), "save_edited_image")).toHaveLength(0); // nothing written before the confirm
    expect(document.activeElement?.textContent).toBe("Cancel"); // the safe button is the default

    fireEvent.click(within(ask).getByRole("button", { name: "Overwrite" }));
    await waitFor(() => expect(callsOf(mock(), "save_edited_image")).toHaveLength(1));
    const sent = callsOf(mock(), "save_edited_image")[0];
    expect(sent.headers?.["x-mode"]).toBe("overwrite");
    expect(sent.headers?.["x-image-id"]).toBe(String(img.id));
    expect(pngSize(sent.payload as Uint8Array)).toEqual({ width: 2000, height: 3200 });
    expect(screen.queryByRole("alertdialog")).toBeNull(); // asked exactly once
    await screen.findByText(`Overwrote ${img.fileName}; the original is backed up (Ctrl+Z to undo)`);
    expect(mock().images.find((i) => i.id === img.id)?.width).toBe(2000);
  });

  it("undo/redo inside the editor and auto-redact gated on the sidecar's pii capability", async () => {
    const { editor, surface } = await openEditor();
    const undo = within(editor).getByRole("button", { name: "Undo" });
    expect(undo).toHaveProperty("disabled", true);
    tool(editor, "Box");
    drag(surface, [10, 10], [200, 120]);
    expect(undo).toHaveProperty("disabled", false);
    fireEvent.click(undo);
    expect(undo).toHaveProperty("disabled", true);
    fireEvent.click(within(editor).getByRole("button", { name: "Redo" }));
    expect(undo).toHaveProperty("disabled", false);
    // The mock sidecar is absent: auto-redact is off and says why.
    const redact = within(editor).getByRole("button", { name: "Auto-redact" });
    expect(redact).toHaveProperty("disabled", true);
    expect(redact.parentElement?.getAttribute("data-ai-block")).toBe("absent");
    expect(callsOf(mock(), "detect_pii")).toHaveLength(0);
  });
});
