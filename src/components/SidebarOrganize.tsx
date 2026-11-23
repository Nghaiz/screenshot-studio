// Sidebar organize sections: saved smart filters, coloured tags, virtual albums. Tags and albums are drop
// targets (drop = tag / add to album, journaled, Undo in the toast); deletes go through a confirm dialog.
import { addToAlbum, applySmartFilter, askDeleteAlbum, askDeleteSmartFilter, askDeleteTag, setTags } from "../actions/organize";
import { useAlbums, useSmartFilters, useTags } from "../data/organize";
import { useT } from "../i18n";
import { dropTarget } from "../lib/dnd";
import { useView } from "../state/view";
import { NavItem, Section, ToolButton } from "./sidebarParts";

export function SidebarOrganize() {
  const t = useT();
  const filters = useSmartFilters();
  const tags = useTags();
  const albums = useAlbums();
  const filter = useView((s) => s.filter);
  const v = useView.getState;

  return (
    <>
      <Section title={t("nav.smartFilters")}
        action={<ToolButton label={t("filter.save")} onClick={() => v().openDialog({ kind: "saveFilter" })}>＋</ToolButton>}>
        {filters.data?.map((f) => (
          <NavItem key={f.id} active={false} onClick={() => applySmartFilter(f)}
            tools={<ToolButton label={t("nav.deleteFilter", { name: f.name })} onClick={() => askDeleteSmartFilter(f)}>✕</ToolButton>}>
            <span aria-hidden>⚲</span>
            <span className="flex-1 truncate">{f.name}</span>
          </NavItem>
        ))}
        {filters.data?.length === 0 && <p className="ss-muted m-0 px-2 text-xs">{t("nav.noSmartFilters")}</p>}
        {filters.isError && <p className="ss-muted m-0 px-2 text-xs">{t("nav.loadFailed")}</p>}
      </Section>

      <Section title={t("nav.tags")}
        action={<ToolButton label={t("tags.create")} onClick={() => v().openDialog({ kind: "tagEdit", tagId: null })}>＋</ToolButton>}>
        {tags.data?.map((tag) => {
          const on = filter.tagIds.includes(tag.id);
          return (
            <NavItem key={tag.id} active={on}
              onClick={() => v().setFilter({ tagIds: on ? filter.tagIds.filter((id) => id !== tag.id) : [...filter.tagIds, tag.id] })}
              drop={dropTarget((ids) => void setTags(ids, [tag.id], []))}
              tools={<>
                <ToolButton label={t("tags.edit", { name: tag.name })} onClick={() => v().openDialog({ kind: "tagEdit", tagId: tag.id })}>✎</ToolButton>
                <ToolButton label={t("tags.delete", { name: tag.name })} onClick={() => askDeleteTag(tag)}>✕</ToolButton>
              </>}>
              <span aria-hidden className="inline-block h-3 w-3 shrink-0 rounded-full" style={{ background: tag.color }} />
              <span className="flex-1 truncate">{tag.name}</span>
              <span className="text-xs opacity-70">{tag.count}</span>
            </NavItem>
          );
        })}
        {tags.data?.length === 0 && <p className="ss-muted m-0 px-2 text-xs">{t("nav.noTags")}</p>}
        {tags.isError && <p className="ss-muted m-0 px-2 text-xs">{t("nav.loadFailed")}</p>}
      </Section>

      <Section title={t("nav.albums")}
        action={<ToolButton label={t("albums.create")} onClick={() => v().openDialog({ kind: "albumEdit", albumId: null })}>＋</ToolButton>}>
        {albums.data?.map((a) => (
          <NavItem key={a.id} active={filter.albumId === a.id}
            onClick={() => v().setFilter({ albumId: filter.albumId === a.id ? null : a.id })}
            drop={dropTarget((ids) => void addToAlbum(a, ids))}
            tools={<>
              <ToolButton label={t("albums.rename", { name: a.name })} onClick={() => v().openDialog({ kind: "albumEdit", albumId: a.id })}>✎</ToolButton>
              <ToolButton label={t("albums.delete", { name: a.name })} onClick={() => askDeleteAlbum(a)}>✕</ToolButton>
            </>}>
            <span aria-hidden>▣</span>
            <span className="flex-1 truncate">{a.name}</span>
            <span className="text-xs opacity-70">{a.count}</span>
          </NavItem>
        ))}
        {albums.data?.length === 0 && <p className="ss-muted m-0 px-2 text-xs">{t("nav.noAlbums")}</p>}
        {albums.isError && <p className="ss-muted m-0 px-2 text-xs">{t("nav.loadFailed")}</p>}
      </Section>
    </>
  );
}
