// Theme engine (design-intent.md § Look and motion): CSS custom properties only, one file per theme in this
// folder, `data-theme` on <html>, an optional custom accent, motion intensity and density as <html> attributes.
import "./aurora-glass.css";
import "./midnight-neon.css";
import "./cyberpunk.css";
import "./fluent-mica-light.css";
import "./fluent-mica-dark.css";
import "./sakura-pastel.css";
import "./sunset.css";
import "./ocean.css";
import "./forest.css";
import "./mono-minimal.css";
import "./high-contrast.css";

/** Every theme file must define all of these (asserted by themes.test.ts). */
export const SS_VARS = [
  "--ss-bg",
  "--ss-surface",
  "--ss-surface-blur",
  "--ss-text",
  "--ss-muted",
  "--ss-accent",
  "--ss-accent-contrast",
  "--ss-border",
  "--ss-radius",
  "--ss-shadow",
  "--ss-motion-scale",
] as const;

export const THEME_IDS = [
  "aurora-glass",
  "midnight-neon",
  "cyberpunk",
  "fluent-mica-light",
  "fluent-mica-dark",
  "sakura-pastel",
  "sunset",
  "ocean",
  "forest",
  "mono-minimal",
  "high-contrast",
] as const;
export type ThemeId = (typeof THEME_IDS)[number];
export const DEFAULT_THEME: ThemeId = "aurora-glass";

export const MOTION_LEVELS = ["off", "subtle", "full"] as const;
export type MotionLevel = (typeof MOTION_LEVELS)[number];
/** Multiplier applied on top of the theme's own `--ss-motion-scale`. */
export const MOTION_INTENSITY: Record<MotionLevel, number> = { off: 0, subtle: 0.5, full: 1 };

export const DENSITIES = ["compact", "comfortable", "spacious"] as const;
export type Density = (typeof DENSITIES)[number];
/** Gap between thumbnails, px. */
export const DENSITY_GAP: Record<Density, number> = { compact: 4, comfortable: 10, spacious: 18 };

export const isThemeId = (v: unknown): v is ThemeId => typeof v === "string" && (THEME_IDS as readonly string[]).includes(v);

const HEX = /^#[0-9a-fA-F]{6}$/;

/** Black or white text on top of `hex`, by WCAG relative luminance. */
export function contrastOn(hex: string): "#000000" | "#ffffff" {
  if (!HEX.test(hex)) throw new Error(`contrastOn: expected #RRGGBB, got ${hex}`);
  const lin = (i: number) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  const l = 0.2126 * lin(1) + 0.7152 * lin(3) + 0.0722 * lin(5);
  return l > 0.4 ? "#000000" : "#ffffff";
}

export function prefersReducedMotion(): boolean {
  return typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** The motion level actually in force: `prefers-reduced-motion` forces `off`. */
export function effectiveMotion(level: MotionLevel): MotionLevel {
  return prefersReducedMotion() ? "off" : level;
}

export interface AppearanceInput {
  theme: ThemeId;
  accent: string | null;
  motion: MotionLevel;
  density: Density;
}

/** Writes the appearance onto <html>. The custom accent overrides the theme's `--ss-accent` inline. */
export function applyAppearance(a: AppearanceInput, root: HTMLElement = document.documentElement): void {
  root.dataset.theme = a.theme;
  root.dataset.motion = effectiveMotion(a.motion);
  root.dataset.density = a.density;
  root.style.setProperty("--ss-motion-intensity", String(MOTION_INTENSITY[effectiveMotion(a.motion)]));
  if (a.accent && HEX.test(a.accent)) {
    root.style.setProperty("--ss-accent", a.accent);
    root.style.setProperty("--ss-accent-contrast", contrastOn(a.accent));
  } else {
    root.style.removeProperty("--ss-accent");
    root.style.removeProperty("--ss-accent-contrast");
  }
}

/** Seconds multiplier for `motion/react` transitions: theme scale x intensity (0 = instant). */
export function motionFactor(root: HTMLElement = document.documentElement): number {
  const scale = parseFloat(getComputedStyle(root).getPropertyValue("--ss-motion-scale")) || 1;
  const intensity = parseFloat(root.style.getPropertyValue("--ss-motion-intensity"));
  return scale * (Number.isFinite(intensity) ? intensity : 1);
}
