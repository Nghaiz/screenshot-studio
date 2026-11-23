// localStorage access for UI prefs (D9). Storage can be absent or throw (private mode, blocked site data);
// reads then return null and writes report to the console, so the app still renders with defaults.
export function readJson(key: string): unknown {
  try {
    const raw = window.localStorage.getItem(key);
    return raw === null ? null : (JSON.parse(raw) as unknown);
  } catch (e) {
    console.warn(`storage: cannot read ${key}`, e);
    return null;
  }
}

export function writeJson(key: string, value: unknown): void {
  try {
    window.localStorage.setItem(key, JSON.stringify(value));
  } catch (e) {
    console.warn(`storage: cannot write ${key}`, e);
  }
}
