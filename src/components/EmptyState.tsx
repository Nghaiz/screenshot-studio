// Empty states (phase 4.2 polish): a glyph per theme, drawn in the theme's accent, and a message per situation.
import type { ReactNode } from "react";
import { usePrefs } from "../state/prefs";
import type { ThemeId } from "../themes";

/** Each theme's empty-state glyph (its motion personality in one character). */
export const THEME_EMPTY_GLYPH: Record<ThemeId, string> = {
  "aurora-glass": "✦",
  "midnight-neon": "☾",
  cyberpunk: "⌬",
  "fluent-mica-light": "▢",
  "fluent-mica-dark": "▣",
  "sakura-pastel": "✿",
  sunset: "☀",
  ocean: "≋",
  forest: "❦",
  "mono-minimal": "○",
  "high-contrast": "◆",
};

export function EmptyState({ text, action }: { text: string; action?: ReactNode }) {
  const theme = usePrefs((s) => s.theme);
  return (
    <div className="flex flex-1 items-center justify-center p-8" data-testid="empty-state" data-theme-glyph={theme}>
      <div className="ss-panel ss-empty flex max-w-lg flex-col items-center gap-3 px-8 py-6 text-center">
        <span aria-hidden className="ss-empty-glyph">{THEME_EMPTY_GLYPH[theme]}</span>
        <p className="ss-muted m-0 text-sm">{text}</p>
        {action}
      </div>
    </div>
  );
}
