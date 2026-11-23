// Eyedropper: reads one pixel of the shown image through a canvas. The image is re-loaded with
// crossOrigin="anonymous" so a cross-origin source without CORS headers fails loudly instead of tainting.

export const toHex = (r: number, g: number, b: number) =>
  `#${[r, g, b].map((c) => c.toString(16).padStart(2, "0")).join("").toUpperCase()}`;

function load(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error(`eyedropper: cannot load ${src}`));
    img.src = src;
  });
}

/** `fx`, `fy` are 0..1 fractions of the image box that was clicked. Rejects when pixels cannot be read. */
export async function pickColor(src: string, fx: number, fy: number): Promise<string> {
  const img = await load(src);
  const x = Math.min(img.naturalWidth - 1, Math.max(0, Math.floor(fx * img.naturalWidth)));
  const y = Math.min(img.naturalHeight - 1, Math.max(0, Math.floor(fy * img.naturalHeight)));
  const canvas = document.createElement("canvas");
  canvas.width = 1;
  canvas.height = 1;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) throw new Error("eyedropper: no 2d canvas context");
  ctx.drawImage(img, x, y, 1, 1, 0, 0, 1, 1);
  const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data; // throws SecurityError on a tainted canvas
  return toHex(r, g, b);
}
