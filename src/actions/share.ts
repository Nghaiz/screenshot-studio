// Share actions (phase 4.2): copy image / OCR text, drag-out to Explorer, reveal, open with, ZIP/PDF export jobs.
// Reveal, open-with, OCR text and drag-out work on `ImageDetail.absPath` with the plugins directly
// (contract-tauri-ipc § Commands, last paragraph); copy image and export are commands.
import { startDrag } from "@crabnebula/tauri-plugin-drag";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { save } from "@tauri-apps/plugin-dialog";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import type { PdfPageSize } from "@bindings/PdfPageSize";
import { call } from "../lib/ipc";
import { attempt, notify, nothingSelected } from "./report";

export async function copyImage(imageId: number): Promise<void> {
  const ok = await attempt(async () => {
    await call("copy_image_to_clipboard", { imageId });
    return true;
  });
  if (ok) notify("share.imageCopied");
}

export async function copyOcrText(text: string | null): Promise<void> {
  if (!text || !text.trim()) return notify("share.noText");
  const ok = await attempt(async () => {
    await writeText(text);
    return true;
  });
  if (ok) notify("share.textCopied");
}

export async function revealInExplorer(absPath: string): Promise<void> {
  await attempt(() => revealItemInDir(absPath));
}

export async function openWithDefault(absPath: string): Promise<void> {
  await attempt(() => openPath(absPath));
}

/** Starts an OS drag of the original files (the first one doubles as the drag image). */
export async function dragOut(absPaths: string[]): Promise<void> {
  if (absPaths.length === 0) return;
  await attempt(() => startDrag({ item: absPaths, icon: absPaths[0], mode: "copy" }));
}

export type ExportKind = "zip" | "pdf";

export interface PdfOptions {
  pageSize: PdfPageSize;
  perPage: 1 | 2 | 4;
}

/** The OS save dialog for an export file; null when cancelled or failed. */
export async function pickExportPath(kind: ExportKind): Promise<string | null> {
  const name = kind === "zip" ? "screenshots.zip" : "screenshots.pdf";
  const picked = await attempt(() => save({ defaultPath: name, filters: [{ name: kind.toUpperCase(), extensions: [kind] }] }));
  return typeof picked === "string" ? picked : null;
}

/** Starts the export job; returns its id (progress arrives as `job-progress` kind `export`), or null. */
export async function startExport(kind: ExportKind, imageIds: number[], destPath: string, pdf: PdfOptions): Promise<number | null> {
  if (nothingSelected(imageIds)) return null;
  const job = await attempt(() =>
    kind === "zip" ? call("export_zip", { imageIds, destPath }) : call("export_pdf", { imageIds, destPath, ...pdf }),
  );
  return job?.jobId ?? null;
}
