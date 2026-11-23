// Typed `listen` wrappers for the Rust -> UI events (contract-tauri-ipc § Events).
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { JobProgress } from "@bindings/JobProgress";
import type { LibraryChanged } from "@bindings/LibraryChanged";
import type { NewScreenshot } from "@bindings/NewScreenshot";
import type { OpenImage } from "@bindings/OpenImage";
import type { SidecarStatus } from "@bindings/SidecarStatus";

export type EventMap = {
  "job-progress": JobProgress;
  "library-changed": LibraryChanged;
  "sidecar-status": SidecarStatus;
  "new-screenshot": NewScreenshot;
  "open-image": OpenImage;
};
export type EventName = keyof EventMap;
export const EVENT_NAMES: EventName[] = ["job-progress", "library-changed", "sidecar-status", "new-screenshot", "open-image"];

export function on<E extends EventName>(event: E, cb: (payload: EventMap[E]) => void): Promise<UnlistenFn> {
  return listen<EventMap[E]>(event, (e) => cb(e.payload));
}

/**
 * For React effects: subscribes now, unsubscribes on cleanup even if `listen` resolves late. `onLive` runs once
 * the listener is registered: an event emitted before that is lost, so state read earlier is re-read there.
 */
export function subscribe<E extends EventName>(event: E, cb: (payload: EventMap[E]) => void, onLive?: () => void): () => void {
  let unlisten: UnlistenFn | null = null;
  let cancelled = false;
  on(event, cb).then(
    (fn) => {
      if (cancelled) return fn();
      unlisten = fn;
      onLive?.();
    },
    (e) => console.error(`events: cannot listen to ${event}`, e),
  );
  return () => {
    cancelled = true;
    unlisten?.();
  };
}
