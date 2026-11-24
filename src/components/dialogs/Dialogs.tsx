// Renders the one open organize dialog (view store `dialog`).
import type { ImageSummary } from "@bindings/ImageSummary";
import { useView } from "../../state/view";
import { AlbumEditDialog, AlbumPickDialog, SaveFilterDialog } from "./AlbumDialogs";
import { ConfirmDialog } from "./ConfirmDialog";
import { ConvertDialog } from "./ConvertDialog";
import { ExportDialog } from "./ExportDialog";
import { MoveDialog } from "./MoveDialog";
import { PermanentDeleteDialog } from "./PermanentDeleteDialog";
import { RenameDialog } from "./RenameDialog";
import { StitchDialog } from "./StitchDialog";
import { TagAssignDialog, TagEditDialog } from "./TagDialogs";

export function Dialogs({ items }: { items: readonly ImageSummary[] }) {
  const d = useView((s) => s.dialog);
  if (!d) return null;
  switch (d.kind) {
    case "tags":
      return <TagAssignDialog ids={d.ids} />;
    case "tagEdit":
      return <TagEditDialog tagId={d.tagId} />;
    case "album":
      return <AlbumPickDialog ids={d.ids} />;
    case "albumEdit":
      return <AlbumEditDialog albumId={d.albumId} />;
    case "move":
      return <MoveDialog ids={d.ids} mode={d.mode} destDir={d.destDir} />;
    case "rename":
      return <RenameDialog ids={d.ids} />;
    case "saveFilter":
      return <SaveFilterDialog />;
    case "confirm":
      return <ConfirmDialog request={d.request} />;
    case "purge":
      return <PermanentDeleteDialog ids={d.ids} items={items} />;
    case "convert":
      return <ConvertDialog ids={d.ids} />;
    case "stitch":
      return <StitchDialog ids={d.ids} />;
    case "export":
      return <ExportDialog ids={d.ids} kind={d.format} />;
  }
}
