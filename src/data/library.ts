// Small read queries: app info, counts, timeline buckets, folder list, image detail.
import { useQuery } from "@tanstack/react-query";
import type { ImageQuery } from "@bindings/ImageQuery";
import { call, toAppError } from "../lib/ipc";
import { qk } from "../lib/query";

export const useAppInfo = () => useQuery({ queryKey: qk.appInfo, queryFn: () => call("get_app_info") });

export const useCounts = () => useQuery({ queryKey: qk.counts, queryFn: () => call("get_counts") });

/** Month buckets, newest first (the backend returns them oldest first). */
export function useTimeline(query: ImageQuery, enabled: boolean) {
  return useQuery({
    queryKey: qk.timeline(query),
    enabled,
    queryFn: async () => {
      const res = await call("get_timeline", { query: { ...query, cursor: null }, bucket: "month" });
      return [...res.buckets].reverse();
    },
  });
}

/**
 * Sub-folders come from `get_stats().byFolder`. Until the backend implements `get_stats` (phase 3.2) the
 * query fails and the sidebar shows that reason instead of guessing a list.
 */
export function useFolders() {
  const q = useQuery({ queryKey: qk.stats, queryFn: () => call("get_stats"), retry: false });
  return {
    folders: q.data?.byFolder.map((f) => ({ name: f.key, count: f.count })) ?? null,
    error: q.error ? toAppError(q.error) : null,
  };
}

export function useImageDetail(id: number | null) {
  return useQuery({
    queryKey: qk.image(id ?? -1),
    enabled: id !== null,
    queryFn: () => call("get_image", { id: id as number }),
  });
}
