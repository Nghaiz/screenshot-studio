// The ONLY place that builds image URLs (contract-tauri-ipc § Image URLs, D5).
import { convertFileSrc } from "@tauri-apps/api/core";
import type { ImageSummary } from "@bindings/ImageSummary";
import { GRID_THUMB_WIDTH, PREVIEW_WIDTH } from "./defaults";

export type ThumbWidth = typeof GRID_THUMB_WIDTH | typeof PREVIEW_WIDTH;
export type ImageRef = Pick<ImageSummary, "id" | "mtimeMs">;
export type UrlKind = { kind: "thumb"; width: ThumbWidth } | { kind: "full" };

/** Replaces the scheme in a browser without a backend (`src/mocks/ipc.ts` installs generated data URLs). */
type Override = (img: ImageRef, what: UrlKind) => string;
let override: Override | null = null;
export function setImageUrlOverride(fn: Override | null): void {
  override = fn;
}

const withVersion = (url: string, img: ImageRef) => `${url}?v=${img.mtimeMs}`;

export function thumbUrl(img: ImageRef, width: ThumbWidth): string {
  if (override) return override(img, { kind: "thumb", width });
  return withVersion(convertFileSrc(`thumb/${img.id}/${width}`, "ssimg"), img);
}

export function fullUrl(img: ImageRef): string {
  if (override) return override(img, { kind: "full" });
  return withVersion(convertFileSrc(`full/${img.id}`, "ssimg"), img);
}
