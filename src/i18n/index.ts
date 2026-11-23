// Typed dictionaries (Deferred decision #22). `t(key, vars)` replaces `{name}` placeholders.
import type { ErrorKind } from "@bindings/ErrorKind";
import { usePrefs } from "../state/prefs";
import { en } from "./en";
import { LANGUAGES, type Language } from "./languages";
import { vi, type MessageKey } from "./vi";

export { LANGUAGES, type Language, type MessageKey };
export const DICTIONARIES: Record<Language, { [K in MessageKey]: string }> = { vi, en };

export type Vars = Record<string, string | number>;
export type Translate = (key: MessageKey, vars?: Vars) => string;

export function translate(lang: Language, key: MessageKey, vars?: Vars): string {
  const text = DICTIONARIES[lang][key];
  if (!vars) return text;
  return text.replace(/\{(\w+)\}/g, (m, name: string) => (name in vars ? String(vars[name]) : m));
}

export function useT(): Translate {
  const lang = usePrefs((s) => s.language);
  return (key, vars) => translate(lang, key, vars);
}

export const errorKey = (kind: ErrorKind): MessageKey => `error.${kind}`;
