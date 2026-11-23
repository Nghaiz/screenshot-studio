import type { CSSProperties, ReactNode, RefObject } from "react";
import type { ImageList } from "../data/images";

/** Keyboard navigation + scrolling contract every layout provides to the gallery. */
export interface Nav {
  up: (index: number) => number;
  down: (index: number) => number;
  pageUp: (index: number) => number;
  pageDown: (index: number) => number;
  scrollTo: (index: number) => void;
}

export type RenderCell = (index: number, style: CSSProperties, variant?: "tile" | "row") => ReactNode;

export interface LayoutProps {
  scrollRef: RefObject<HTMLDivElement | null>;
  /** Content width (scroll element width minus the side gutters). */
  width: number;
  height: number;
  list: ImageList;
  thumbSize: number;
  gap: number;
  navRef: { current: Nav | null };
  renderCell: RenderCell;
}
