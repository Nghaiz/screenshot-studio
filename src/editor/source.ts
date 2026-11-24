// Loads the original image the editor draws from. It is fetched with crossOrigin="anonymous": `ssimg` is another
// origin than the app page, so without CORS the canvas would be tainted and the PNG export would throw at save
// time; asking for CORS makes a missing header fail here, at open, with a clear message instead.
import type { ImageSummary } from "@bindings/ImageSummary";
import { fullUrl } from "../lib/ssimg";

export interface EditSource {
  image: CanvasImageSource;
  width: number;
  height: number;
}

export function loadEditSource(item: Pick<ImageSummary, "id" | "mtimeMs">): Promise<EditSource> {
  const src = fullUrl(item);
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.decoding = "async";
    img.onload = () => resolve({ image: img, width: img.naturalWidth, height: img.naturalHeight });
    img.onerror = () => reject(new Error(`editor: cannot load the original (${src}); the image server may not allow canvas reads`));
    img.src = src;
  });
}
