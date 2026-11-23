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

/** `get_stats`: aggregates computed by the backend (sidebar folders, dashboard, category counts). */
export const useStats = () => useQuery({ queryKey: qk.stats, queryFn: () => call("get_stats"), retry: false });

/**
 * Sub-folders come from `get_stats().byFolder` (folder relative to its root, "" = the root itself). A failed read
 * shows its reason with a retry instead of a guessed list; `library-changed` re-reads it too.
 */
export function useFolders() {
  const q = useStats();
  return {
    folders: q.data?.byFolder.map((f) => ({ name: f.key, count: f.count })) ?? null,
    error: q.error ? toAppError(q.error) : null,
    retry: () => void q.refetch(),
  };
}

export function useImageDetail(id: number | null) {
  return useQuery({
    queryKey: qk.image(id ?? -1),
    enabled: id !== null,
    queryFn: () => call("get_image", { id: id as number }),
  });
}
