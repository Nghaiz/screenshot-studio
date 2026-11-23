// UI side of the Rust <-> UI contract (contracts.md): the generated bindings compile, every command has a
// typed sample request, and `call()` wraps it as `{ req }` (D4, rule 1) for every non-raw-body command.
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";
import process from "node:process";
import { afterEach, describe, expect, it } from "vitest";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { COMMANDS, CONTRACT_VERSION, type CommandMap } from "@bindings/commands";
import type { ImageQuery } from "@bindings/ImageQuery";
import { call, RAW_BODY_COMMANDS, saveEditedImage, type ReqCommand } from "../lib/ipc";

// Vitest runs with cwd = the project root (where vitest.config.ts lives); jsdom makes import.meta.url non-file.
const repoRoot = process.cwd();
if (!existsSync(join(repoRoot, "tsconfig.json"))) {
  throw new Error(`contract.test.ts must run from the repo root; cwd is ${repoRoot}`);
}

const query: ImageQuery = {
  text: "hóa đơn", textMode: "any", dateFrom: null, dateTo: 1_763_900_000_000, minWidth: null, maxWidth: null,
  minHeight: 800, maxHeight: null, aspects: ["phone", "pc"], minSize: null, maxSize: null, folders: ["1ERD"],
  tagIds: [1, 2], tagMatch: "all", albumId: null, color: "#1E1E2E", colorTolerance: 20, categories: [],
  favoriteOnly: false, minRating: 0, hasText: null, trash: "exclude", includeMissing: false,
  sort: { field: "taken", dir: "desc" }, cursor: null, limit: 200,
};

/** One sample `req` per command. The mapped type makes `tsc` reject a missing command or a wrong shape. */
const SAMPLES: { [K in ReqCommand]: CommandMap[K]["req"] } = {
  get_app_info: null,
  add_library_root: { path: "C:\\Users\\me\\OneDrive\\Pictures\\Screenshots" },
  remove_library_root: { rootId: 1 },
  start_index: { rootId: null },
  download_cloud_only: { rootId: 1 },
  cancel_job: { jobId: 7 },
  query_images: query,
  get_image: { id: 42 },
  get_timeline: { query, bucket: "month" },
  get_settings: null,
  set_setting: { key: "hotkey", value: "Ctrl+Alt+S" },
  list_tags: null,
  create_tag: { name: "work", color: "#FF8800" },
  update_tag: { tagId: 3, name: "work", color: "#00AAFF" },
  delete_tag: { tagId: 3 },
  set_tags: { imageIds: [1, 2], add: [3], remove: [] },
  list_albums: null,
  create_album: { name: "Trips" },
  rename_album: { albumId: 4, name: "Trips 2026" },
  delete_album: { albumId: 4 },
  album_add: { albumId: 4, imageIds: [1, 2] },
  album_remove: { albumId: 4, imageIds: [2] },
  album_reorder: { albumId: 4, imageIds: [2, 1] },
  set_favorite: { imageIds: [1], value: true },
  set_rating: { imageIds: [1], rating: 5 },
  set_note: { imageId: 1, note: null },
  list_smart_filters: null,
  save_smart_filter: { id: null, name: "Phones", query },
  delete_smart_filter: { id: 9 },
  move_images: { imageIds: [1], destDir: "D:\\Sorted", mode: "copy", onConflict: "rename" },
  preview_rename: { imageIds: [1, 2], pattern: "{date:YYYYMMDD}_{n:03}.{ext}" },
  rename_images: { items: [{ imageId: 1, newName: "a.png" }] },
  trash_images: { imageIds: [1] },
  restore_images: { imageIds: [1] },
  recycle_images: { imageIds: [1] },
  request_permanent_delete: { imageIds: [1, 2] },
  delete_permanently: { token: "tok", typedConfirmation: "DELETE" },
  undo: { batchId: null },
  list_journal: { limit: 50 },
  get_sidecar_status: null,
  restart_sidecar: null,
  start_ai: { tasks: ["ocr", "embed"], imageIds: null },
  find_duplicates: { mode: "similar", hamming: 6, rootId: null },
  get_sessions: { query, gapMinutes: 30 },
  detect_issues: { rootId: null },
  get_counts: null,
  get_stats: null,
  detect_pii: { imageId: 1 },
  convert_images: { imageIds: [1], format: "webp", quality: 85, maxEdge: null, output: "copy" },
  stitch_images: { imageIds: [1, 2], direction: "vertical", gap: 8, background: "#000000" },
  export_zip: { imageIds: [1, 2], destPath: "D:\\out.zip" },
  export_pdf: { imageIds: [1, 2], destPath: "D:\\out.pdf", pageSize: "a4", perPage: 2 },
  copy_image_to_clipboard: { imageId: 1 },
  get_recent: { limit: 10 },
};

describe("contract: generated bindings", () => {
  it("type-check the whole UI against the committed bindings (tsc --noEmit)", () => {
    const r = spawnSync("npx tsc --noEmit -p tsconfig.json", {
      cwd: repoRoot, encoding: "utf8", shell: true,
    });
    expect(r.status, `${r.stdout}\n${r.stderr}`).toBe(0);
  }, 120_000);

  it("expose the contract version and a sample for every non-raw command", () => {
    expect(CONTRACT_VERSION).toBe(1);
    const raw = COMMANDS.filter((c) => c.rawBody).map((c) => c.name);
    expect(raw).toEqual([...RAW_BODY_COMMANDS]);
    const wanted = COMMANDS.filter((c) => !c.rawBody).map((c) => c.name).sort();
    expect(Object.keys(SAMPLES).sort()).toEqual(wanted);
    for (const c of COMMANDS.filter((c) => !c.rawBody)) {
      expect(SAMPLES[c.name as ReqCommand] === null, `${c.name}: hasReq`).toBe(!c.hasReq);
    }
  });
});

describe("contract: IPC call shape (D4)", () => {
  afterEach(() => clearMocks());

  it("wraps every command's req as { req } and sends no args for no-req commands", async () => {
    const seen: { cmd: string; payload: unknown }[] = [];
    mockIPC((cmd, payload) => {
      seen.push({ cmd, payload });
      return null;
    });
    for (const [cmd, req] of Object.entries(SAMPLES) as [ReqCommand, unknown][]) {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      await (call as any)(cmd, ...(req === null ? [] : [req]));
    }
    expect(seen.map((s) => s.cmd)).toEqual(Object.keys(SAMPLES));
    for (const { cmd, payload } of seen) {
      const req = SAMPLES[cmd as ReqCommand];
      if (req === null) expect(payload ?? {}, cmd).toEqual({});
      else expect(payload, cmd).toEqual({ req });
    }
  });

  it("sends save_edited_image as a raw body, not wrapped", async () => {
    let got: unknown;
    mockIPC((cmd, payload) => {
      if (cmd === "save_edited_image") got = payload;
      return null;
    });
    const png = new Uint8Array([137, 80, 78, 71]);
    await saveEditedImage(png, { imageId: 1, mode: "copy", format: "png", quality: null });
    expect(got).toBeInstanceOf(Uint8Array);
    expect(Array.from(got as Uint8Array)).toEqual([137, 80, 78, 71]);
  });
});
