// Main layout once a library root exists: sidebar | top bar / gallery / status bar, plus overlays.
import { useMemo } from "react";
import type { AppInfo } from "@bindings/AppInfo";
import { useGlobalKeys } from "./commands/globalKeys";
import { CommandPalette } from "./components/CommandPalette";
import { Lightbox } from "./components/lightbox/Lightbox";
import { ConfirmRecycle, Toasts } from "./components/Overlays";
import { SettingsPanel } from "./components/SettingsPanel";
import { Sidebar } from "./components/Sidebar";
import { StatusBar } from "./components/StatusBar";
import { TopBar } from "./components/TopBar";
import { useImageList } from "./data/images";
import { buildQuery } from "./lib/query";
import { usePrefs } from "./state/prefs";
import { useView } from "./state/view";
import { Gallery } from "./views/Gallery";

export function Shell({ info }: { info: AppInfo }) {
  const view = useView((s) => s.view);
  const folder = useView((s) => s.folder);
  const sort = usePrefs((s) => s.sort);
  const layout = usePrefs((s) => s.layout);
  const query = useMemo(() => buildQuery({ view, folder, sort }), [view, folder, sort]);
  const eager = view !== "timeline" && (layout === "masonry" || layout === "justified");
  const list = useImageList(query, eager);
  const ctx = useMemo(() => ({ list }), [list]);
  useGlobalKeys();

  return (
    <div className="flex h-full gap-2 p-2">
      <Sidebar roots={info.libraryRoots} />
      <main className="flex min-w-0 flex-1 flex-col gap-2">
        <TopBar />
        <Gallery list={list} />
        <StatusBar total={list.total} />
      </main>
      <Lightbox list={list} />
      <CommandPalette ctx={ctx} />
      <SettingsPanel />
      <ConfirmRecycle />
      <Toasts />
    </div>
  );
}
