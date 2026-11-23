// Left sidebar: views, library roots, sub-folders, and the tag/album slots that land in phase 2.2.
import type { ReactNode } from "react";
import type { LibraryRoot } from "@bindings/LibraryRoot";
import { useFolders } from "../data/library";
import { useT, type MessageKey } from "../i18n";
import type { View } from "../lib/query";
import { useView } from "../state/view";

const VIEWS: { id: View; key: MessageKey; icon: string }[] = [
  { id: "library", key: "nav.library", icon: "▦" },
  { id: "timeline", key: "nav.timeline", icon: "◷" },
  { id: "trash", key: "nav.trash", icon: "🗑" },
];

export function Sidebar({ roots }: { roots: readonly LibraryRoot[] }) {
  const t = useT();
  const view = useView((s) => s.view);
  const folder = useView((s) => s.folder);
  const { folders, error } = useFolders();

  return (
    <aside className="ss-panel flex w-56 shrink-0 flex-col gap-4 overflow-y-auto p-3 text-sm" aria-label={t("app.title")}>
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

      <Section title={t("nav.roots")}>
        {roots.map((r) => (
          <div key={r.id} className="truncate px-2 py-0.5" title={r.path}>
            {r.protectedRoot && <span aria-hidden>🔒 </span>}
            {r.path.split(/[\\/]/).filter(Boolean).pop() ?? r.path}
            <span className="ss-muted"> · {r.imageCount}</span>
          </div>
        ))}
      </Section>

      <Section title={t("nav.folders")}>
        <NavItem active={folder === null} onClick={() => useView.getState().setFolder(null)}>
          {t("nav.allFolders")}
        </NavItem>
        {folders?.map((f) => (
          <NavItem key={f.name} active={folder === f.name} onClick={() => useView.getState().setFolder(f.name)}>
            <span className="flex-1 truncate">{f.name || t("nav.rootFolder")}</span>
            <span className="ss-muted text-xs">{f.count}</span>
          </NavItem>
        ))}
        {error && <p className="ss-muted px-2 text-xs">{t("nav.foldersUnavailable", { reason: error.message })}</p>}
      </Section>

      <Section title={t("nav.tags")}>
        <p className="ss-muted px-2 text-xs">{t("nav.comingSoon")}</p>
      </Section>
      <Section title={t("nav.albums")}>
        <p className="ss-muted px-2 text-xs">{t("nav.comingSoon")}</p>
      </Section>
    </aside>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-0.5">
      <h3 className="ss-muted m-0 px-2 pb-1 text-xs font-semibold uppercase tracking-wide">{title}</h3>
      {children}
    </section>
  );
}

function NavItem({ active, onClick, children }: { active: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      aria-current={active ? "page" : undefined}
      className="flex w-full items-center gap-2 rounded px-2 py-1 text-left"
      style={active ? { background: "var(--ss-accent)", color: "var(--ss-accent-contrast)" } : { color: "var(--ss-text)" }}
      onClick={onClick}
    >
      {children}
    </button>
  );
}
