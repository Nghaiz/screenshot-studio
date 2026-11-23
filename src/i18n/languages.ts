// Leaf module (no imports) so both the prefs store and the i18n layer can use it without an import cycle.
export const LANGUAGES = ["vi", "en"] as const;
export type Language = (typeof LANGUAGES)[number];
