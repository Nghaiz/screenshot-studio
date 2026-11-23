// Zoom (wheel, +/-/0 via props) and pan (drag) of one image. Zoom > 1 swaps the 1280 preview for the
// original (design-intent § Defaults). In eyedropper mode a click picks a colour (read from the 1280 preview).
import { useRef, useState, type PointerEvent, type WheelEvent } from "react";

export const ZOOM_MIN = 1;
export const ZOOM_MAX = 8;
export const clampZoom = (z: number) => Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, z));

interface Props {
  previewSrc: string;
  fullSrc: string;
  alt: string;
  zoom: number;
  onZoom: (z: number) => void;
  eyedropper: boolean;
  onPick: (src: string, fx: number, fy: number) => void;
}

export function ZoomPan({ previewSrc, fullSrc, alt, zoom, onZoom, eyedropper, onPick }: Props) {
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const drag = useRef<{ x: number; y: number; px: number; py: number } | null>(null);
  const src = zoom > 1 ? fullSrc : previewSrc;
  const shownPan = zoom > 1 ? pan : { x: 0, y: 0 };

  const onWheel = (e: WheelEvent) => onZoom(clampZoom(zoom * (e.deltaY < 0 ? 1.15 : 1 / 1.15)));
  const onDown = (e: PointerEvent<HTMLImageElement>) => {
    if (eyedropper) {
      const r = e.currentTarget.getBoundingClientRect();
      onPick(previewSrc, (e.clientX - r.left) / r.width, (e.clientY - r.top) / r.height);
      return;
    }
    if (zoom <= 1) return;
    drag.current = { x: e.clientX, y: e.clientY, px: pan.x, py: pan.y };
    e.currentTarget.setPointerCapture(e.pointerId);
  };
  const onMove = (e: PointerEvent) => {
    const d = drag.current;
    if (d) setPan({ x: d.px + (e.clientX - d.x) / zoom, y: d.py + (e.clientY - d.y) / zoom });
  };

  return (
    <div className="relative flex min-h-0 flex-1 items-center justify-center overflow-hidden" onWheel={onWheel}>
      <img
        src={src}
        alt={alt}
        draggable={false}
        className="max-h-full max-w-full select-none object-contain"
        style={{
          transform: `scale(${zoom}) translate(${shownPan.x}px, ${shownPan.y}px)`,
          transition: drag.current ? "none" : "transform var(--ss-dur)",
          cursor: eyedropper ? "crosshair" : zoom > 1 ? "grab" : "zoom-in",
        }}
        onPointerDown={onDown}
        onPointerMove={onMove}
        onPointerUp={() => (drag.current = null)}
        onDoubleClick={() => !eyedropper && onZoom(zoom > 1 ? 1 : 2)}
      />
    </div>
  );
}
