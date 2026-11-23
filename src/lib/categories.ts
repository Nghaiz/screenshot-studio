// Zero-shot category labels. The SSOT is ss-core `smart::categories::LABELS` (the stored `category.label` is the label
// string itself); the UI keeps the same list, in the same display order, only to key their vi/en display names.
// A label the backend reports that is not listed here still shows, under its raw name (`categoryName`).
import type { MessageKey, Translate } from "../i18n";

export const CATEGORY_LABELS = [
  "chat", "code", "game", "browser", "document", "map", "receipt", "social", "photo", "settings", "error dialog", "meme",
] as const;
export type CategoryLabel = (typeof CATEGORY_LABELS)[number];

export const isCategoryLabel = (v: string): v is CategoryLabel => (CATEGORY_LABELS as readonly string[]).includes(v);

export const categoryKey = (label: CategoryLabel): MessageKey => `category.${label}`;

export function categoryName(t: Translate, label: string): string {
  return isCategoryLabel(label) ? t(categoryKey(label)) : label;
}

/** Known labels first (display order), then any other label seen in the data or in the filter, in first-seen order. */
export function categoryChoices(...seen: readonly (readonly string[])[]): string[] {
  const out: string[] = [...CATEGORY_LABELS];
  for (const list of seen) for (const l of list) if (!out.includes(l)) out.push(l);
  return out;
}
