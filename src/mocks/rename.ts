// Mock of the rename pattern (contract tokens: {name} {n} {n:03} {date:YYYYMMDD} {time:HHmmss} {w} {h} {ext}).
// An unknown token is an `invalidArgument`, as in ss-core.
import type { ImageSummary } from "@bindings/ImageSummary";
import { appError } from "./catalog";

const pad = (n: number, w = 2) => String(n).padStart(w, "0");

export function renderPattern(pattern: string, img: ImageSummary, n: number): string {
  const dot = img.fileName.lastIndexOf(".");
  const stem = dot > 0 ? img.fileName.slice(0, dot) : img.fileName;
  const ext = dot > 0 ? img.fileName.slice(dot + 1) : "";
  const d = new Date(img.takenMs);
  const out = pattern.replace(/\{([^}]*)\}/g, (_m, token: string) => {
    const counter = /^n(?::(0\d+))?$/.exec(token);
    if (counter) return counter[1] ? pad(n, Number(counter[1])) : String(n);
    switch (token) {
      case "name":
        return stem;
      case "ext":
        return ext;
      case "w":
        return String(img.width ?? 0);
      case "h":
        return String(img.height ?? 0);
      case "date:YYYYMMDD":
        return `${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}`;
      case "time:HHmmss":
        return `${pad(d.getHours())}${pad(d.getMinutes())}${pad(d.getSeconds())}`;
      default:
        throw appError("invalidArgument", `unknown rename token {${token}}`);
    }
  });
  if (!out.trim()) throw appError("invalidArgument", "the pattern renders an empty name");
  // Like ss-core: the extension is kept when the pattern does not write one.
  return pattern.includes("{ext}") || !ext ? out : `${out}.${ext}`;
}
