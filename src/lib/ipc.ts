// The only way the UI calls Rust (contract-tauri-ipc.md rule 1).
// Command names and req/res types come from the generated `@bindings/commands` (never hand-mirrored).
import { invoke } from "@tauri-apps/api/core";
import type { CommandMap } from "@bindings/commands";
import type { AppError } from "@bindings/AppError";
import type { EditMode } from "@bindings/EditMode";
import type { ErrorKind } from "@bindings/ErrorKind";
import type { ImageFormat } from "@bindings/ImageFormat";
import type { ImageSummary } from "@bindings/ImageSummary";

/** Commands that take a raw binary body instead of `{ req }`; excluded from the req-wrapping test. */
export const RAW_BODY_COMMANDS = ["save_edited_image"] as const;
export type RawBodyCommand = (typeof RAW_BODY_COMMANDS)[number];
export type ReqCommand = Exclude<keyof CommandMap, RawBodyCommand>;

type ReqArgs<K extends ReqCommand> = CommandMap[K]["req"] extends null ? [] : [req: CommandMap[K]["req"]];

/** `call('query_images', query)` -> `invoke('query_images', { req: query })`; no-arg commands send no args. */
export function call<K extends ReqCommand>(cmd: K, ...args: ReqArgs<K>): Promise<CommandMap[K]["res"]> {
  return args.length === 0 ? invoke(cmd) : invoke(cmd, { req: args[0] });
}

export interface SaveEditedImageHeaders {
  imageId: number;
  mode: EditMode;
  format: ImageFormat;
  quality: number | null;
}

/** The one raw-body command: PNG bytes as the body, parameters as `x-*` headers. */
export function saveEditedImage(png: Uint8Array, h: SaveEditedImageHeaders): Promise<ImageSummary> {
  const headers: Record<string, string> = {
    "x-image-id": String(h.imageId),
    "x-mode": h.mode,
    "x-format": h.format,
  };
  if (h.quality !== null) headers["x-quality"] = String(h.quality);
  return invoke("save_edited_image", png, { headers });
}

const ERROR_KINDS: readonly ErrorKind[] = [
  "notFound", "io", "db", "invalidArgument", "conflict", "sidecarUnavailable", "cancelled",
  "permissionDenied", "protectedRoot", "confirmationRequired", "internal",
];

export function isAppError(e: unknown): e is AppError {
  if (!e || typeof e !== "object") return false;
  const o = e as Record<string, unknown>;
  return typeof o.kind === "string" && (ERROR_KINDS as readonly string[]).includes(o.kind) && typeof o.message === "string";
}

/**
 * Maps any rejection to the contract envelope (rule 5). A non-envelope rejection (a Tauri transport error,
 * a JS exception) becomes `internal` with its text kept in `detail`, so nothing is swallowed.
 */
export function toAppError(e: unknown): AppError {
  if (isAppError(e)) return { kind: e.kind, message: e.message, detail: e.detail ?? null };
  const text = e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
  return { kind: "internal", message: "unexpected IPC failure", detail: text };
}
