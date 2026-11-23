// In-app drag payload: the ids being dragged (drag-out of originals to the OS is phase 4.2, a separate path).
import type { DragEvent } from "react";
import { DRAG_MIME } from "./defaults";

export function setDragIds(e: DragEvent, ids: readonly number[]): void {
  e.dataTransfer.setData(DRAG_MIME, JSON.stringify(ids));
  e.dataTransfer.effectAllowed = "copyMove";
}

/** True while an in-app image drag is over the target (types are readable during dragover, data is not). */
export function isImageDrag(e: DragEvent): boolean {
  return Array.from(e.dataTransfer.types).includes(DRAG_MIME);
}

/** The dragged ids, or null when the drop is not an in-app image drag (e.g. files from Explorer). */
export function readDragIds(e: DragEvent): number[] | null {
  const raw = e.dataTransfer.getData(DRAG_MIME);
  if (!raw) return null;
  let ids: unknown;
  try {
    ids = JSON.parse(raw);
  } catch {
    console.warn("dnd: ignoring a drop with a malformed payload", raw);
    return null;
  }
  return Array.isArray(ids) && ids.every((x) => Number.isInteger(x)) ? (ids as number[]) : null;
}

/** Props that make an element an in-app drop target. `onDropIds` gets the ids and whether Ctrl was held. */
export function dropTarget(onDropIds: (ids: number[], ctrl: boolean) => void) {
  return {
    onDragOver: (e: DragEvent) => {
      if (!isImageDrag(e)) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = e.ctrlKey ? "copy" : "move";
      e.currentTarget.setAttribute("data-drop", "true");
    },
    onDragLeave: (e: DragEvent) => e.currentTarget.removeAttribute("data-drop"),
    onDrop: (e: DragEvent) => {
      e.currentTarget.removeAttribute("data-drop");
      const ids = readDragIds(e);
      if (!ids || ids.length === 0) return;
      e.preventDefault();
      onDropIds(ids, e.ctrlKey);
    },
  };
}
