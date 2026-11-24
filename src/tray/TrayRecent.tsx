// The tray popup (window `tray-recent`, 4.1 creates it on a tray left-click): a compact list of the latest
// screenshots. A click sends `open-image` to the main window, which shows that image (data/events.ts).
import { useEffect } from "react";
import { QueryClientProvider, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { emitTo } from "@tauri-apps/api/event";
import type { ImageSummary } from "@bindings/ImageSummary";
import { formatBytes } from "../components/Cell";
import { useT } from "../i18n";
import { GRID_THUMB_WIDTH } from "../lib/defaults";
import { subscribe } from "../lib/events";
import { call, toAppError } from "../lib/ipc";
import { thumbUrl } from "../lib/ssimg";
import { focusMain, MAIN_LABEL } from "./window";

/** Items shown in the popup. */
export const TRAY_RECENT_LIMIT = 12;
const RECENT_KEY = ["trayRecent"] as const;

export function TrayRoot({ client }: { client: QueryClient }) {
  return (
    <QueryClientProvider client={client}>
      <TrayRecent />
    </QueryClientProvider>
  );
}

export async function openInMain(imageId: number): Promise<void> {
  await emitTo(MAIN_LABEL, "open-image", { imageId });
  await focusMain();
}

export function TrayRecent() {
  const t = useT();
  const qc = useQueryClient();
  const recent = useQuery({ queryKey: RECENT_KEY, queryFn: () => call("get_recent", { limit: TRAY_RECENT_LIMIT }) });
  useEffect(() => {
    const again = () => void qc.invalidateQueries({ queryKey: RECENT_KEY });
    const offs = [subscribe("new-screenshot", again), subscribe("library-changed", again)];
    // The popup window is hidden and shown again rather than re-created: re-read whenever it gets focus.
    window.addEventListener("focus", again);
    return () => {
      offs.forEach((off) => off());
      window.removeEventListener("focus", again);
    };
  }, [qc]);

  return (
    <section aria-label={t("tray.title")} className="ss-panel flex h-full flex-col gap-1 overflow-hidden p-2 text-sm">
      <header className="flex items-center gap-2 px-1">
        <h1 className="m-0 flex-1 text-sm font-semibold">{t("tray.title")}</h1>
        <button type="button" className="ss-btn text-xs" onClick={() => void focusMain()}>{t("tray.openApp")}</button>
      </header>
      {recent.isPending && <div className="ss-shimmer flex-1 rounded" aria-busy="true" />}
      {recent.isError && <p role="alert" className="m-0 p-2 text-xs">{t("tray.failed", { reason: toAppError(recent.error).message })}</p>}
      {recent.data?.length === 0 && <p className="ss-muted m-0 p-2 text-xs">{t("tray.empty")}</p>}
      {recent.data && recent.data.length > 0 && (
        <ul className="m-0 flex min-h-0 flex-1 list-none flex-col gap-1 overflow-y-auto p-0">
          {recent.data.map((item) => <Row key={item.id} item={item} />)}
        </ul>
      )}
    </section>
  );
}

function Row({ item }: { item: ImageSummary }) {
  return (
    <li>
      <button type="button" className="ss-tray-row flex w-full items-center gap-2 rounded px-1 py-1 text-left"
        onClick={() => void openInMain(item.id)}>
        <span className="h-10 w-16 shrink-0 overflow-hidden rounded" style={{ background: "var(--ss-surface)" }}>
          {item.thumbState === "ok" && <img src={thumbUrl(item, GRID_THUMB_WIDTH)} alt="" className="h-full w-full object-cover" />}
        </span>
        <span className="min-w-0 flex-1">
          <span className="block truncate">{item.fileName}</span>
          <span className="ss-muted block truncate text-xs">{new Date(item.takenMs).toLocaleString()} · {formatBytes(item.sizeBytes)}</span>
        </span>
      </button>
    </li>
  );
}
