// Ctrl+K palette commands, as data. Safety (design-intent § Safety): the palette can reach the trash view,
// the in-app trash and the Recycle Bin confirm dialog, NEVER `request_permanent_delete` / `delete_permanently`
// nor the permanent delete dialog. browse.test.tsx and delete-dialogs.test.tsx run every command below
// against the mock and assert that.
import type { SidecarStatus } from "@bindings/SidecarStatus";
import { askRecycle, notify, pickAndAddFolder, reindex, restoreImages, trashImages, undo } from "../actions/images";
import { copyImage } from "../actions/share";
import { runAiOnLibrary } from "../actions/smart";
import { cached } from "../data/organize";
import type { ImageList } from "../data/images";
import type { MessageKey, Translate } from "../i18n";
import { LANGUAGES } from "../i18n/languages";
import { clampThumb, LAYOUTS, usePrefs } from "../state/prefs";
import { useView } from "../state/view";
import { THUMB_SIZE_STEP } from "../lib/defaults";
import { qk } from "../lib/query";
import { runnableTasks } from "../lib/sidecar";
import { DENSITIES, MOTION_LEVELS, THEME_IDS } from "../themes";
import { compareSelected } from "./gridKeys";
import { targetIds } from "./targets";

export interface CommandContext {
  list: ImageList;
}

export interface PaletteCommand {
  id: string;
  label: (t: Translate) => string;
  shortcut?: string;
  run: (ctx: CommandContext) => void | Promise<void>;
}

const simple = (id: string, key: MessageKey, run: PaletteCommand["run"], shortcut?: string): PaletteCommand => ({
  id,
  label: (t) => t(key),
  run,
  shortcut,
});

const prefs = () => usePrefs.getState();
const view = () => useView.getState();

/** Opens an organize dialog for the selection (or the focused image). */
const dialogFor = (kind: "tags" | "album" | "rename" | "move" | "copy" | "convert" | "stitch" | "zip" | "pdf") => ({ list }: CommandContext) => {
  const ids = targetIds(list.items);
  if (ids.length === 0) return notify("actions.nothingSelected");
  if (kind === "move" || kind === "copy") view().openDialog({ kind: "move", ids, mode: kind, destDir: null });
  else if (kind === "zip" || kind === "pdf") view().openDialog({ kind: "export", ids, format: kind });
  else view().openDialog({ kind, ids });
};

export const PALETTE_COMMANDS: readonly PaletteCommand[] = [
  simple("go.library", "cmd.goLibrary", () => view().setView("library")),
  simple("go.timeline", "cmd.goTimeline", () => view().setView("timeline")),
  simple("go.trash", "cmd.goTrash", () => view().setView("trash")),
  // Smart tools: reversible actions only (in-app trash / Recycle Bin), never permanent delete.
  simple("go.duplicates", "cmd.goDuplicates", () => view().setView("duplicates")),
  simple("go.cleanup", "cmd.goCleanup", () => view().setView("cleanup")),
  simple("go.stats", "cmd.goStats", () => view().setView("stats")),
  ...LAYOUTS.map<PaletteCommand>((l) => ({
    id: `layout.${l}`,
    label: (t) => t("cmd.layout", { name: t(`layout.${l}`) }),
    run: () => prefs().set({ layout: l }),
  })),
  ...THEME_IDS.map<PaletteCommand>((id) => ({
    id: `theme.${id}`,
    label: (t) => t("cmd.theme", { name: t(`theme.${id}`) }),
    run: () => prefs().set({ theme: id }),
  })),
  ...MOTION_LEVELS.map<PaletteCommand>((m) => ({
    id: `motion.${m}`,
    label: (t) => t("cmd.motion", { name: t(`motion.${m}`) }),
    run: () => prefs().set({ motion: m }),
  })),
  ...DENSITIES.map<PaletteCommand>((d) => ({
    id: `density.${d}`,
    label: (t) => t("cmd.density", { name: t(`density.${d}`) }),
    run: () => prefs().set({ density: d }),
  })),
  ...LANGUAGES.map<PaletteCommand>((lang) => ({
    id: `language.${lang}`,
    label: (t) => t("cmd.language", { name: t(`lang.${lang}`) }),
    run: () => prefs().set({ language: lang }),
  })),
  simple("bg.toggle", "cmd.toggleAnimatedBg", () => prefs().set({ animatedBg: !prefs().animatedBg })),
  simple("thumbs.bigger", "cmd.biggerThumbs", () => prefs().set({ thumbSize: clampThumb(prefs().thumbSize + THUMB_SIZE_STEP) })),
  simple("thumbs.smaller", "cmd.smallerThumbs", () => prefs().set({ thumbSize: clampThumb(prefs().thumbSize - THUMB_SIZE_STEP) })),
  simple("select.all", "cmd.selectAll", ({ list }) => view().setSelection(list.items.map((i) => i.id), 0), "Ctrl+A"),
  simple("select.clear", "cmd.clearSelection", () => view().clearSelection(), "Esc"),
  simple("viewer.open", "cmd.openViewer", ({ list }) => {
    if (list.total > 0) view().openLightbox({ mode: "view", index: Math.min(view().focusIndex, list.total - 1) });
  }, "Enter"),
  simple("viewer.compare", "cmd.compare", () => {
    if (!compareSelected()) notify("actions.compareNeedsTwo");
  }, "C"),
  simple("images.trash", "cmd.trash", ({ list }) => trashImages(targetIds(list.items)), "Delete"),
  simple("images.recycle", "cmd.recycle", ({ list }) => askRecycle(targetIds(list.items)), "Shift+Delete"),
  simple("images.restore", "cmd.restore", ({ list }) => restoreImages(targetIds(list.items))),
  simple("images.tags", "cmd.tags", dialogFor("tags")),
  simple("images.album", "cmd.addToAlbum", dialogFor("album")),
  simple("images.move", "cmd.move", dialogFor("move")),
  simple("images.copy", "cmd.copy", dialogFor("copy")),
  simple("images.rename", "cmd.rename", dialogFor("rename")),
  simple("images.edit", "cmd.edit", ({ list }) => {
    const first = targetIds(list.items)[0];
    if (first === undefined) return notify("actions.nothingSelected");
    view().openEditor(first);
  }),
  simple("images.copyImage", "cmd.copyImage", ({ list }) => {
    const first = targetIds(list.items)[0];
    if (first === undefined) return notify("actions.nothingSelected");
    return copyImage(first);
  }),
  simple("images.convert", "cmd.convert", dialogFor("convert")),
  simple("images.stitch", "cmd.stitch", dialogFor("stitch")),
  simple("images.exportZip", "cmd.exportZip", dialogFor("zip")),
  simple("images.exportPdf", "cmd.exportPdf", dialogFor("pdf")),
  simple("filters.toggle", "cmd.toggleFilters", () => view().setFiltersOpen(!view().filtersOpen)),
  simple("filters.reset", "cmd.resetFilters", () => view().resetFilter()),
  simple("filters.save", "cmd.saveFilter", () => view().openDialog({ kind: "saveFilter" })),
  simple("panel.details", "cmd.details", () => view().setSidePanel(view().sidePanel === "details" ? "none" : "details")),
  simple("panel.history", "cmd.history", () => view().setSidePanel(view().sidePanel === "history" ? "none" : "history")),
  simple("undo", "cmd.undo", () => undo(null), "Ctrl+Z"),
  simple("library.reindex", "cmd.reindex", () => reindex()),
  simple("library.addFolder", "cmd.addFolder", async () => {
    await pickAndAddFolder();
  }),
  simple("settings.open", "cmd.settings", () => view().setSettings(true, "appearance")),
  simple("settings.library", "cmd.librarySettings", () => view().setSettings(true, "library")),
  simple("settings.ai", "cmd.aiSettings", () => view().setSettings(true, "ai")),
  simple("ai.run", "cmd.runAi", () => runAiOnLibrary(runnableTasks(cached<SidecarStatus>(qk.sidecar)))),
];

/** Case- and diacritic-insensitive substring match on the translated label. */
export function filterCommands(commands: readonly PaletteCommand[], text: string, t: Translate): PaletteCommand[] {
  const fold = (s: string) => s.normalize("NFD").replace(/\p{M}/gu, "").replace(/đ/g, "d").replace(/Đ/g, "d").toLowerCase();
  const q = fold(text.trim());
  return q ? commands.filter((c) => fold(c.label(t)).includes(q)) : [...commands];
}
