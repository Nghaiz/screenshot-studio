// Left sidebar: views, smart tools, library roots and sub-folders (drop targets for move/copy; the list is
// `get_stats().byFolder`), then the organize sections.
import type { LibraryRoot } from "@bindings/LibraryRoot";
import { askRemoveRoot } from "../actions/images";
import { useFolders } from "../data/library";
import { useT, type MessageKey } from "../i18n";
import { dropTarget } from "../lib/dnd";
import type { View } from "../lib/query";
import { useView } from "../state/view";
import { NavItem, Section, ToolButton } from "./sidebarParts";
import { SidebarOrganize } from "./SidebarOrganize";

const VIEWS: { id: View; key: MessageKey; icon: string }[] = [
  { id: "library", key: "nav.library", icon: "▦" },
  { id: "timeline", key: "nav.timeline", icon: "◷" },
  { id: "trash", key: "nav.trash", icon: "🗑" },
];
/** Smart tools: they work without the AI sidecar (duplicates, blur and stats are computed by the backend). */
const TOOLS: { id: View; key: MessageKey; icon: string }[] = [
  { id: "duplicates", key: "nav.duplicates", icon: "⧉" },
  { id: "cleanup", key: "nav.cleanup", icon: "✧" },
  { id: "stats", key: "nav.stats", icon: "▤" },
];

const lastSegment = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;
/** Absolute folder path of a sub-folder of `root` ("" = the root itself), as `move_images` wants it. */
export const folderPath = (root: LibraryRoot, folder: string) => (folder ? `${root.path.replace(/[\\/]+$/, "")}\\${folder.replace(/\//g, "\\")}` : root.path);

/** A drop opens the move dialog prefilled (Ctrl = copy), so a stray drop never moves files by itself. */
const moveDrop = (destDir: string) =>
  dropTarget((ids, ctrl) => useView.getState().openDialog({ kind: "move", ids, mode: ctrl ? "copy" : "move", destDir }));

export function Sidebar({ roots }: { roots: readonly LibraryRoot[] }) {
  const t = useT();
  const view = useView((s) => s.view);
  const folders = useView((s) => s.filter.folders);
  const current = folders.length === 1 ? folders[0] : null;
  const list = useFolders();
  // Folder names from the catalog are relative to their root; with one root the absolute path is known.
  const single = roots.length === 1 ? roots[0] : null;

  return (
    <aside className="ss-panel flex w-60 shrink-0 flex-col gap-4 overflow-y-auto p-3 text-sm" aria-label={t("app.title")}>
      <div className="px-2 text-base font-semibold" style={{ color: "var(--ss-accent)" }}>
        {t("app.title")}
      </div>
      <nav className="flex flex-col gap-0.5">
        {VIEWS.map((v) => (
          <NavItem key={v.id} active={view === v.id} onClick={() => useView.getState().setView(v.id)}>
            <span aria-hidden className="w-5 text-center">{v.icon}</span>
            {t(v.key)}
          </NavItem>
        ))}
      </nav>

      <Section title={t("nav.smart")}>
        <nav className="flex flex-col gap-0.5" aria-label={t("nav.smart")}>
          {TOOLS.map((v) => (
            <NavItem key={v.id} active={view === v.id} onClick={() => useView.getState().setView(v.id)}>
              <span aria-hidden className="w-5 text-center">{v.icon}</span>
              {t(v.key)}
            </NavItem>
          ))}
        </nav>
      </Section>

      <Section title={t("nav.roots")}>
        {roots.map((r) => (
          <NavItem
            key={r.id}
            active={false}
            label={r.path}
            onClick={() => useView.getState().setFolder(null)}
            drop={moveDrop(r.path)}
            tools={<ToolButton label={t("nav.removeRoot", { path: r.path })} onClick={() => askRemoveRoot(r)}>✕</ToolButton>}
          >
            <span className="flex-1 truncate" title={r.path}>
              {r.protectedRoot && <span aria-hidden>🔒 </span>}
              {lastSegment(r.path)}
            </span>
            <span className="ss-muted text-xs">{r.imageCount}</span>
          </NavItem>
        ))}
      </Section>

      <Section title={t("nav.folders")}>
        <NavItem active={current === null} onClick={() => useView.getState().setFolder(null)}>
          {t("nav.allFolders")}
        </NavItem>
        {list.folders?.map((f) => (
          <NavItem key={f.name} active={current === f.name} onClick={() => useView.getState().setFolder(f.name)}
            drop={single ? moveDrop(folderPath(single, f.name)) : undefined}>
            <span className="flex-1 truncate">{f.name || t("nav.rootFolder")}</span>
            <span className="ss-muted text-xs">{f.count}</span>
          </NavItem>
        ))}
        {list.error && (
          <p className="ss-muted m-0 flex items-center gap-2 px-2 text-xs" role="alert">
            <span className="flex-1">{t("nav.foldersUnavailable", { reason: list.error.message })}</span>
            <ToolButton label={t("nav.retry")} onClick={list.retry}>↻</ToolButton>
          </p>
        )}
      </Section>

      <SidebarOrganize />
    </aside>
  );
}
