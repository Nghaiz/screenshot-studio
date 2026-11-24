// Phase 4.2 share/export and live UI: copy OCR text sends the OCR string to `writeText`; the export dialog sends
// `export_zip` with the selection and follows the job; the tray popup lists `get_recent` and a click sends
// `open-image` to the main window, which shows that image even outside the current filter.
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { createQueryClient } from "../Root";
import { useView } from "../state/view";
import { TRAY_RECENT_LIMIT, TrayRoot } from "../tray/TrayRecent";
import { callsOf, cell, lastCall, renderApp, reqOf, useMockApp } from "./harness";

const app = useMockApp({ language: "en" });
const mock = () => app.get();
const OCR = "Hóa đơn #1042 — tổng 450.000đ\nhttps://example.test/invoice";

const idAt = (grid: HTMLElement, index: number) => {
  const name = cell(grid, index).getAttribute("aria-label");
  const img = mock().images.find((i) => i.fileName === name);
  if (!img) throw new Error(`no mock image named ${name}`);
  return img.id;
};

describe("share (phase 4.2)", () => {
  it("Copy text (OCR) calls writeText with the image's OCR string", async () => {
    const grid = await renderApp();
    mock().setOcr(idAt(grid, 0), OCR);
    act(() => useView.getState().setSidePanel("details"));
    const share = await screen.findByRole("group", { name: "Share" });
    fireEvent.click(within(share).getByRole("button", { name: "Copy text (OCR)" }));
    await waitFor(() => expect(callsOf(mock(), "plugin:clipboard-manager|write_text")).toHaveLength(1));
    expect((lastCall(mock(), "plugin:clipboard-manager|write_text")?.payload as { text: string }).text).toBe(OCR);
    await screen.findByText("Text of the image copied");
  });

  it("Copy text is off for an image without OCR text, and sends nothing", async () => {
    await renderApp();
    act(() => useView.getState().setSidePanel("details"));
    const share = await screen.findByRole("group", { name: "Share" });
    expect(within(share).getByRole("button", { name: "Copy text (OCR)" })).toHaveProperty("disabled", true);
    expect(callsOf(mock(), "plugin:clipboard-manager|write_text")).toHaveLength(0);
  });

  it("the export dialog sends export_zip with the selection and the chosen file, then shows the job finishing", async () => {
    const grid = await renderApp();
    const ids = [idAt(grid, 0), idAt(grid, 2), idAt(grid, 5)];
    act(() => useView.getState().setSelection(ids, 0));
    fireEvent.click(within(screen.getByRole("toolbar", { name: "Organize" })).getByRole("button", { name: "ZIP" }));
    const dialog = screen.getByRole("dialog", { name: "Export 3 images to ZIP" });
    const start = within(dialog).getByRole("button", { name: "Export" });
    expect(start).toHaveProperty("disabled", true); // no destination yet

    fireEvent.click(within(dialog).getByRole("button", { name: "Choose…" }));
    await waitFor(() => expect(within(dialog).getByTestId("export-dest").textContent).toBe("D:\\Mock\\Export\\screenshots.zip"));
    fireEvent.click(start);
    await waitFor(() => expect(callsOf(mock(), "export_zip")).toHaveLength(1));
    expect(reqOf(lastCall(mock(), "export_zip"))).toEqual({ imageIds: ids, destPath: "D:\\Mock\\Export\\screenshots.zip" });
    expect(callsOf(mock(), "export_pdf")).toHaveLength(0);

    await waitFor(() => expect(within(dialog).getByTestId("export-progress").textContent).toBe("Exported 3 images"));
    fireEvent.click(within(dialog).getByRole("button", { name: "Show in Explorer" }));
    await waitFor(() => expect(callsOf(mock(), "plugin:opener|reveal_item_in_dir")).toHaveLength(1));
  });

  it("the PDF export sends the page options", async () => {
    const grid = await renderApp();
    const ids = [idAt(grid, 1), idAt(grid, 3)];
    act(() => useView.getState().setSelection(ids, 1));
    fireEvent.click(within(screen.getByRole("toolbar", { name: "Organize" })).getByRole("button", { name: "PDF" }));
    const dialog = screen.getByRole("dialog", { name: "Export 2 images to PDF" });
    fireEvent.click(within(within(dialog).getByRole("group", { name: "Images per page" })).getByRole("button", { name: "4" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Choose…" }));
    await waitFor(() => expect(within(dialog).getByTestId("export-dest").textContent).toContain("screenshots.pdf"));
    fireEvent.click(within(dialog).getByRole("button", { name: "Export" }));
    await waitFor(() => expect(callsOf(mock(), "export_pdf")).toHaveLength(1));
    expect(reqOf(lastCall(mock(), "export_pdf"))).toEqual({ imageIds: ids, destPath: "D:\\Mock\\Export\\screenshots.pdf", pageSize: "a4", perPage: 4 });
  });
});

describe("live (phase 4.2)", () => {
  it("open-image shows that image in the lightbox; new-screenshot puts a 'new' badge on it until opened", async () => {
    const grid = await renderApp();
    const id = idAt(grid, 0);
    const name = cell(grid, 0).getAttribute("aria-label");
    await act(() => mock().emit("new-screenshot", { imageId: id, fileName: name ?? "" }));
    await waitFor(() => expect(within(cell(grid, 0)).getByTestId("new-badge").textContent).toBe("New"));

    await act(() => mock().emit("open-image", { imageId: id }));
    await screen.findByRole("dialog", { name: `Viewing ${name}` });
    await waitFor(() => expect(within(cell(grid, 0)).queryByTestId("new-badge")).toBeNull());
  });

  it("the tray popup lists get_recent and a click sends open-image to the main window", async () => {
    render(<TrayRoot client={createQueryClient()} />);
    const list = await screen.findByRole("region", { name: "Recent screenshots" });
    const rows = await within(list).findAllByRole("button", { name: /\.(png|jpg|jpeg|webp)/i });
    expect(rows).toHaveLength(TRAY_RECENT_LIMIT);
    expect(reqOf(lastCall(mock(), "get_recent"))).toEqual({ limit: TRAY_RECENT_LIMIT });
    const first = (lastCall(mock(), "get_recent")?.result as { id: number }[])[0];
    fireEvent.click(rows[0]);
    await waitFor(() => expect(callsOf(mock(), "plugin:event|emit_to")).toHaveLength(1));
    const sent = lastCall(mock(), "plugin:event|emit_to")?.payload as { target: unknown; event: string; payload: unknown };
    expect(sent.event).toBe("open-image");
    expect(sent.payload).toEqual({ imageId: first.id });
    expect(JSON.stringify(sent.target)).toContain("main");
  });
});
