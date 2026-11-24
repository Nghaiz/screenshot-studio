// Backend settings (the `setting` table, D9) and the library-root actions of Settings > Library.
import type { SettingKey } from "@bindings/SettingKey";
import { invalidate } from "../data/organize";
import { call } from "../lib/ipc";
import { qk } from "../lib/query";
import { attempt, notify } from "./report";

/** Stores one setting; the backend applies it (watcher, hotkey re-registration, autostart). False when it failed. */
export async function setSetting(key: SettingKey, value: unknown): Promise<boolean> {
  const ok = await attempt(async () => {
    await call("set_setting", { key, value });
    return true;
  });
  // The app info carries the warnings a setting can clear or raise (hotkeyUnavailable, watcherFailed).
  invalidate(qk.settings, qk.appInfo);
  return ok === true;
}

/** Reads every cloud-only file of the root so OneDrive downloads it (never pins; contract rule 9). */
export async function downloadCloudOnly(rootId: number): Promise<void> {
  const job = await attempt(() => call("download_cloud_only", { rootId }));
  if (job) notify("settings.downloadStarted");
}

const MODIFIERS: Record<string, string> = { Control: "Ctrl", Alt: "Alt", Shift: "Shift", Meta: "Super" };

/**
 * A global-shortcut accelerator ("Ctrl+Alt+S") from a key press, or null while only modifiers are held or when no
 * modifier is held (a bare key would swallow ordinary typing system-wide).
 */
export function acceleratorOf(e: { key: string; code: string; ctrlKey: boolean; altKey: boolean; shiftKey: boolean; metaKey: boolean }): string | null {
  if (e.key in MODIFIERS) return null;
  const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(Boolean) as string[];
  if (mods.length === 0) return null;
  let key: string;
  if (/^Key[A-Z]$/.test(e.code)) key = e.code.slice(3);
  else if (/^Digit[0-9]$/.test(e.code)) key = e.code.slice(5);
  else if (/^F([1-9]|1[0-9]|2[0-4])$/.test(e.key)) key = e.key;
  else if (e.key.length === 1) key = e.key.toUpperCase();
  else key = e.key;
  return [...mods, key].join("+");
}
