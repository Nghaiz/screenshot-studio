// Main layout once a library root exists: sidebar | top bar / filters / action bar / gallery / status bar |
// side panel, plus overlays and the one open organize dialog.
import { useMemo } from "react";
import type { AppInfo } from "@bindings/AppInfo";
import { useGlobalKeys } from "./commands/globalKeys";
import { ActionBar } from "./components/ActionBar";
import { CommandPalette } from "./components/CommandPalette";
import { Dialogs } from "./components/dialogs/Dialogs";
import { FilterPanel } from "./components/FilterPanel";
import { Lightbox } from "./components/lightbox/Lightbox";
import { ConfirmRecycle, Toasts } from "./components/Overlays";
import { SettingsPanel } from "./components/SettingsPanel";
import { Sidebar } from "./components/Sidebar";
import { SidePanel } from "./components/SidePanels";
import { StatusBar } from "./components/StatusBar";
import { TopBar } from "./components/TopBar";
import { useImageList } from "./data/images";
import { buildQuery } from "./lib/query";
import { usePrefs } from "./state/prefs";
import { useView } from "./state/view";
import { Gallery } from "./views/Gallery";

export function Shell({ info }: { info: AppInfo }) {
  const view = useView((s) => s.view);
  const filter = useView((s) => s.filter);
  const sort = usePrefs((s) => s.sort);
  const layout = usePrefs((s) => s.layout);
  const query = useMemo(() => buildQuery({ view, filter, sort }), [view, filter, sort]);
  // Justified and masonry need every aspect ratio; an album needs its whole order to be reordered.
  const eager = view !== "timeline" && (layout === "masonry" || layout === "justified" || filter.albumId !== null);
  const list = useImageList(query, eager);
  const ctx = useMemo(() => ({ list }), [list]);
  useGlobalKeys();

  return (
    <div className="flex h-full gap-2 p-2">
      <Sidebar roots={info.libraryRoots} />
      <main className="flex min-w-0 flex-1 flex-col gap-2">
        <TopBar />
        <FilterPanel />
        <ActionBar list={list} />
        <Gallery list={list} />
        <StatusBar total={list.total} />
      </main>
      <SidePanel list={list} />
      <Lightbox list={list} />
      <CommandPalette ctx={ctx} />
      <SettingsPanel />
      <ConfirmRecycle />
      <Dialogs items={list.items} />
      <Toasts />
    </div>
  );
}
