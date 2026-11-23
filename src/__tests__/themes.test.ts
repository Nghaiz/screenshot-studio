// Theme engine + i18n contract (phase 1.3): every theme defines every --ss-* variable; vi/en key parity.
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import process from "node:process";
import { afterEach, describe, expect, it } from "vitest";
import { en } from "../i18n/en";
import { translate } from "../i18n";
import { vi } from "../i18n/vi";
import { DEFAULT_PREFS, parsePrefs } from "../state/prefs";
import { applyAppearance, contrastOn, SS_VARS, THEME_IDS } from "../themes";

// Read from disk: vitest replaces CSS module content (even `?raw`) with "" unless CSS processing is on.
// Vitest's cwd is the repo root (where vitest.config.ts lives), as contract.test.ts relies on too.
const THEMES_DIR = join(process.cwd(), "src", "themes");
const cssFiles = readdirSync(THEMES_DIR).filter((f) => f.endsWith(".css"));
const cssFor = (id: string) => readFileSync(join(THEMES_DIR, `${id}.css`), "utf8");

/** `--ss-x: value;` declarations inside the theme's own `[data-theme="id"]` block. */
function declaredVars(css: string, id: string): Map<string, string> {
  const block = css.match(new RegExp(`\\[data-theme="${id}"\\]\\s*\\{([\\s\\S]*?)\\n\\}`));
  if (!block) throw new Error(`theme ${id}: no [data-theme="${id}"] block`);
  const vars = new Map<string, string>();
  for (const m of block[1].matchAll(/(--ss-[\w-]+)\s*:\s*([^;]+);/g)) vars.set(m[1], m[2].trim());
  return vars;
}

describe("themes", () => {
  afterEach(() => {
    document.documentElement.removeAttribute("style");
  });

  it("ships exactly the 11 theme ids of design-intent.md, one file each", () => {
    expect(THEME_IDS).toHaveLength(11);
    expect(THEME_IDS[0]).toBe("aurora-glass");
    const onDisk = cssFiles.map((f) => f.replace(/\.css$/, "")).sort();
    expect(onDisk).toEqual([...THEME_IDS].sort());
  });

  it.each(THEME_IDS)("%s defines every --ss-* variable with a non-empty value", (id) => {
    const vars = declaredVars(cssFor(id), id);
    for (const v of SS_VARS) {
      expect(vars.has(v), `${id} is missing ${v}`).toBe(true);
      expect(vars.get(v), `${id} ${v} is empty`).not.toBe("");
    }
    expect(Number.isFinite(parseFloat(vars.get("--ss-motion-scale") ?? "")), `${id} motion scale is a number`).toBe(true);
  });

  it("applies theme, motion, density and a custom accent to <html>", () => {
    const root = document.documentElement;
    applyAppearance({ theme: "ocean", accent: "#ff0000", motion: "subtle", density: "compact" }, root);
    expect(root.dataset.theme).toBe("ocean");
    expect(root.dataset.motion).toBe("subtle");
    expect(root.dataset.density).toBe("compact");
    expect(root.style.getPropertyValue("--ss-accent")).toBe("#ff0000");
    expect(root.style.getPropertyValue("--ss-accent-contrast")).toBe(contrastOn("#ff0000"));
    applyAppearance({ theme: "ocean", accent: null, motion: "off", density: "compact" }, root);
    expect(root.style.getPropertyValue("--ss-accent")).toBe("");
    expect(root.style.getPropertyValue("--ss-motion-intensity")).toBe("0");
  });

  it("picks readable text on an accent", () => {
    expect(contrastOn("#ffff00")).toBe("#000000");
    expect(contrastOn("#0067c0")).toBe("#ffffff");
    expect(() => contrastOn("red")).toThrow();
  });

  it("falls back per field on stored prefs it does not recognise", () => {
    expect(parsePrefs(null)).toEqual(DEFAULT_PREFS);
    const p = parsePrefs({ theme: "not-a-theme", thumbSize: 9999, language: "en", layout: "masonry", sort: { field: "bogus", dir: "asc" } });
    expect(p.theme).toBe(DEFAULT_PREFS.theme);
    expect(p.thumbSize).toBe(400);
    expect(p.language).toBe("en");
    expect(p.layout).toBe("masonry");
    expect(p.sort).toEqual({ field: "taken", dir: "asc" });
  });
});

describe("i18n", () => {
  const placeholders = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

  it("vi and en have the same key set", () => {
    const viKeys = Object.keys(vi).sort();
    const enKeys = Object.keys(en).sort();
    expect(viKeys.length).toBeGreaterThan(50);
    expect(enKeys).toEqual(viKeys);
  });

  it("every key has non-empty text and the same placeholders in both languages", () => {
    for (const key of Object.keys(vi) as (keyof typeof vi)[]) {
      expect(vi[key].trim(), `vi ${key}`).not.toBe("");
      expect(en[key].trim(), `en ${key}`).not.toBe("");
      expect(placeholders(en[key]), key).toEqual(placeholders(vi[key]));
    }
  });

  it("has a name for every theme and interpolates variables", () => {
    for (const id of THEME_IDS) expect(Object.keys(vi)).toContain(`theme.${id}`);
    expect(translate("en", "status.items", { count: 10000 })).toBe("10000 images");
    expect(translate("vi", "status.items", { count: 3 })).toBe("3 ảnh");
  });
});
