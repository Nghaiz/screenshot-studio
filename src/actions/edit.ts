// Editor and file-conversion actions (phase 4.1 commands). Each reports its outcome as a toast; overwrite and
// convert-overwrite are journaled by the backend (backup in `<data>\edit-backups\`), so their toast carries Undo.
import type { ConvertImagesReq } from "@bindings/ConvertImagesReq";
import type { EditMode } from "@bindings/EditMode";
import type { ImageFormat } from "@bindings/ImageFormat";
import type { ImageSummary } from "@bindings/ImageSummary";
import type { StitchImagesReq } from "@bindings/StitchImagesReq";
import { refresh } from "../data/organize";
import { exportPng } from "../editor/render";
import type { EditDoc } from "../editor/model";
import { call, saveEditedImage } from "../lib/ipc";
import { useView } from "../state/view";
import { attempt, nothingSelected, reportError, reportResult, t } from "./report";

/** Quality sent for lossy formats unless the user picks another (`x-quality`, 1..100). */
export const DEFAULT_QUALITY = 90;

/** The format matching a file's extension (what "save" keeps by default). */
export function formatOf(fileName: string): ImageFormat {
  const ext = fileName.split(".").pop()?.toLowerCase();
  if (ext === "jpg" || ext === "jpeg") return "jpeg";
  if (ext === "webp") return "webp";
  return "png";
}

export const isLossy = (f: ImageFormat) => f !== "png";

/**
 * Renders the edit at full size and sends the PNG bytes as the raw body of `save_edited_image`.
 * Returns the saved image, or null when it failed (error toast shown).
 */
export async function saveEdit(
  imageId: number,
  source: CanvasImageSource,
  doc: EditDoc,
  mode: EditMode,
  format: ImageFormat,
): Promise<ImageSummary | null> {
  let png: Uint8Array;
  try {
    png = await exportPng(source, doc);
  } catch (e) {
    reportError(e);
    return null;
  }
  const quality = isLossy(format) ? DEFAULT_QUALITY : null;
  const saved = await attempt(() => saveEditedImage(png, { imageId, mode, format, quality }));
  if (!saved) return null;
  useView.getState().toast({
    text: t(mode === "overwrite" ? "editor.overwritten" : "editor.savedCopy", { name: saved.fileName }),
    tone: "info",
    undoBatchId: null,
  });
  refresh();
  return saved;
}

export async function convertImages(req: ConvertImagesReq): Promise<boolean> {
  if (nothingSelected(req.imageIds)) return false;
  const r = await attempt(() => call("convert_images", req));
  if (!r) return false;
  reportResult(r, "convert.done");
  refresh();
  return true;
}

export async function stitchImages(req: StitchImagesReq): Promise<ImageSummary | null> {
  if (req.imageIds.length < 2) {
    useView.getState().toast({ text: t("stitch.needTwo"), tone: "error", undoBatchId: null });
    return null;
  }
  const out = await attempt(() => call("stitch_images", req));
  if (!out) return null;
  useView.getState().toast({ text: t("stitch.done", { name: out.fileName }), tone: "info", undoBatchId: null });
  refresh();
  return out;
}

/** Finds PII boxes in the image (needs the sidecar's `pii` capability). Null when it failed. */
export async function detectPii(imageId: number) {
  return attempt(() => call("detect_pii", { imageId }));
}
