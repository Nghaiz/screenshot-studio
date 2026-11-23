// Phase 3.3 read queries: sidecar status (and the per-capability gate), duplicates, sessions, issues, old trash.
import { useQuery } from "@tanstack/react-query";
import type { DuplicateMode } from "@bindings/DuplicateMode";
import type { ImagePage } from "@bindings/ImagePage";
import type { ImageQuery } from "@bindings/ImageQuery";
import type { SettingKey } from "@bindings/SettingKey";
import type { SidecarStatus } from "@bindings/SidecarStatus";
import { call } from "../lib/ipc";
import { emptyQuery, qk } from "../lib/query";
import { aiGate, type AiGate, type SidecarCapability } from "../lib/sidecar";
import { DAY_MS, OLD_TRASH_DAYS } from "../lib/defaults";
import { useAppInfo } from "./library";

/**
 * The sidecar status in force: `get_sidecar_status`, kept current by `sidecar-status` events and re-read once the
 * event listener is live (data/events.ts), so a transition emitted before the page subscribed is never missed.
 * Falls back to the `get_app_info` snapshot while that first read is in flight. `undefined` = not known yet.
 */
export function useSidecarStatus(): SidecarStatus | undefined {
  const q = useQuery({ queryKey: qk.sidecar, queryFn: () => call("get_sidecar_status") });
  const info = useAppInfo();
  return q.data ?? info.data?.sidecar;
}

export function useAiGate(cap: SidecarCapability): AiGate {
  return aiGate(useSidecarStatus(), cap);
}

/** `get_settings` (backend `setting` table). */
export const useSettings = () => useQuery({ queryKey: qk.settings, queryFn: () => call("get_settings") });

/** A numeric backend setting, or `fallback` while unread or when it holds something else. */
export function useNumberSetting(key: SettingKey, fallback: number): number {
  const v = useSettings().data?.[key];
  return typeof v === "number" && Number.isFinite(v) ? v : fallback;
}

/**
 * `hamming` only matters in `similar` mode (exact mode sends null and shares one cache entry); null in similar mode
 * lets the backend use the `dupHamming` setting.
 */
export function useDuplicates(mode: DuplicateMode, hamming: number | null, enabled = true) {
  const h = mode === "similar" ? hamming : null;
  return useQuery({
    queryKey: qk.duplicates(mode, h),
    enabled,
    queryFn: () => call("find_duplicates", { mode, hamming: h, rootId: null }),
  });
}

/** Capture sessions for the timeline's query (same filters and order as its images; gap = setting). */
export function useSessions(query: ImageQuery, enabled: boolean) {
  return useQuery({
    queryKey: qk.sessions(query),
    enabled,
    queryFn: async () => (await call("get_sessions", { query: { ...query, cursor: null }, gapMinutes: null })).sessions,
  });
}

/** Blurry and tiny images (the first call may score blur for older rows, so it can take a moment). */
export const useIssues = (enabled: boolean) =>
  useQuery({ queryKey: qk.issues, enabled, queryFn: () => call("detect_issues", { rootId: null }) });

/** Ids of images in the in-app trash for more than OLD_TRASH_DAYS (every page of the trash is read). */
export function useOldTrash(enabled: boolean) {
  return useQuery({
    queryKey: qk.oldTrash,
    enabled,
    queryFn: async () => {
      const cutoff = Date.now() - OLD_TRASH_DAYS * DAY_MS;
      const ids: number[] = [];
      let cursor: string | null = null;
      do {
        const page: ImagePage = await call("query_images", { ...emptyQuery(), trash: "only", cursor });
        for (const i of page.items) if (i.trashedAt !== null && i.trashedAt < cutoff) ids.push(i.id);
        cursor = page.nextCursor;
      } while (cursor !== null);
      return ids;
    },
  });
}
