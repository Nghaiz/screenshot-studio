// Main layout once a library root exists: sidebar | top bar / AI notice / filters / action bar / gallery (or a smart
// tool, lazy-loaded) / status bar | side panel, plus overlays and the one open organize dialog.
import { useMemo, type ReactNode } from "react";
import { motion, type Transition } from "motion/react";
import type { AppInfo } from "@bindings/AppInfo";
import { useGlobalKeys } from "./commands/globalKeys";
import { ActionBar } from "./components/ActionBar";
import { AiBanner } from "./components/AiNotice";
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
import { useMotion } from "./components/useMotion";
import { useImageList } from "./data/images";
import { buildQuery, isToolView } from "./lib/query";
import { usePrefs } from "./state/prefs";
import { useView } from "./state/view";
import { Gallery } from "./views/Gallery";
import { ToolView } from "./views/ToolView";
import { Editor } from "./editor/Editor";

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
  const m = useMotion();

  return (
    <div className="flex h-full gap-2 p-2">
      <Sidebar roots={info.libraryRoots} />
      <main className="flex min-w-0 flex-1 flex-col gap-2">
        <TopBar />
        <AiBanner />
        {isToolView(view) ? (
          <ViewTransition id={view} fade={m.fade}>
            <ToolView view={view} />
          </ViewTransition>
        ) : (
          <>
            <FilterPanel />
            <ActionBar list={list} />
            <ViewTransition id={view} fade={m.fade}>
              <Gallery list={list} />
            </ViewTransition>
          </>
        )}
        <StatusBar total={list.total} />
      </main>
      <SidePanel list={list} />
      <Lightbox list={list} />
      <CommandPalette ctx={ctx} />
      <SettingsPanel />
      <ConfirmRecycle />
      <Editor />
      <Dialogs items={list.items} />
      <Toasts />
    </div>
  );
}

/**
 * Fades a view in when it changes (phase 4.2 polish). Mounts in the click's own commit, no exit wait: the smart views
 * must render at once (src/__tests__/smartviews.test.tsx); with motion off the transition is instant.
 */
function ViewTransition({ id, fade, children }: { id: string; fade: Transition; children: ReactNode }) {
  return (
    <motion.div key={id} className="flex min-h-0 flex-1 flex-col" initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} transition={fade}>
      {children}
    </motion.div>
  );
}
