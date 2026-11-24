// Which window this page runs in. The tray popup is created by Rust with the default URL (src-tauri/src/tray.rs
// `RECENT_LABEL`), so it is recognised by its window label; the hash route `#/tray-recent` opens the same view in a
// plain browser (`VITE_MOCK_IPC=1 npm run dev`, then /#/tray-recent).
import { getCurrentWindow, Window } from "@tauri-apps/api/window";

export const TRAY_RECENT_LABEL = "tray-recent";
export const TRAY_RECENT_HASH = "#/tray-recent";
export const MAIN_LABEL = "main";

/** The Tauri window label, or null outside Tauri (a plain browser has no window metadata, and the API throws). */
export function windowLabel(): string | null {
  try {
    return getCurrentWindow().label;
  } catch {
    return null;
  }
}

export const isTrayRecent = (): boolean => window.location.hash === TRAY_RECENT_HASH || windowLabel() === TRAY_RECENT_LABEL;

/**
 * Brings the main window forward. The webview capability grants only read-only `core:window` permissions, so
 * `show`/`setFocus` are refused until `core:window:allow-show` + `allow-set-focus` are granted (integration
 * phase 4.3 owns `src-tauri/capabilities`); the refusal is logged, the image still opens in the main window.
 */
export async function focusMain(): Promise<void> {
  try {
    const main = await Window.getByLabel(MAIN_LABEL);
    if (!main) throw new Error("no window labelled `main`");
    await main.show();
    await main.unminimize();
    await main.setFocus();
  } catch (e) {
    console.warn("tray-recent: cannot bring the main window forward (needs core:window:allow-show/allow-set-focus)", e);
  }
}
