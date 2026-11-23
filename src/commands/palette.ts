// Ctrl+K palette commands, as data. Safety (design-intent § Safety): the palette can reach the trash view,
// the in-app trash and the Recycle Bin confirm dialog, NEVER `request_permanent_delete` / `delete_permanently`.
// browse.test.tsx runs every command below against the mock and asserts that.
import { askRecycle, notify, pickAndAddFolder, reindex, trashImages, undo } from "../actions/images";
import type { ImageList } from "../data/images";
import type { MessageKey, Translate } from "../i18n";
import { LANGUAGES } from "../i18n/languages";
import { clampThumb, LAYOUTS, usePrefs } from "../state/prefs";
import { useView } from "../state/view";
import { THUMB_SIZE_STEP } from "../lib/defaults";
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

export const PALETTE_COMMANDS: readonly PaletteCommand[] = [
  simple("go.library", "cmd.goLibrary", () => view().setView("library")),
  simple("go.timeline", "cmd.goTimeline", () => view().setView("timeline")),
  simple("go.trash", "cmd.goTrash", () => view().setView("trash")),
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
  simple("undo", "cmd.undo", () => undo(null), "Ctrl+Z"),
  simple("library.reindex", "cmd.reindex", () => reindex()),
  simple("library.addFolder", "cmd.addFolder", async () => {
    await pickAndAddFolder();
  }),
  simple("settings.open", "cmd.settings", () => view().setSettings(true)),
];

/** Case- and diacritic-insensitive substring match on the translated label. */
export function filterCommands(commands: readonly PaletteCommand[], text: string, t: Translate): PaletteCommand[] {
  const fold = (s: string) => s.normalize("NFD").replace(/\p{M}/gu, "").replace(/đ/g, "d").replace(/Đ/g, "d").toLowerCase();
  const q = fold(text.trim());
  return q ? commands.filter((c) => fold(c.label(t)).includes(q)) : [...commands];
}
