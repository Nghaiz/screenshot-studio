// Organize read queries (tags, albums, saved smart filters, undo history) and the shared QueryClient handle
// the actions use to refresh them after a mutation.
import { useQuery, type QueryClient } from "@tanstack/react-query";
import { call } from "../lib/ipc";
import { qk } from "../lib/query";

/** How many journal batches the history panel shows. */
export const JOURNAL_LIMIT = 50;

let client: QueryClient | null = null;

/** Root registers its client so actions (plain functions, no hooks) can refresh queries after a mutation. */
export function registerQueryClient(qc: QueryClient): void {
  client = qc;
}

/** Refetches the given query keys; also every image list, because a mutation changes what lists show. */
export function refresh(...keys: (readonly unknown[])[]): void {
  if (!client) return;
  for (const queryKey of [qk.imagesAll, qk.counts, qk.journal, ...keys]) void client.invalidateQueries({ queryKey });
}

/** Refetches only the given query keys (settings and other changes that do not alter any image list). */
export function invalidate(...keys: (readonly unknown[])[]): void {
  if (!client) return;
  for (const queryKey of keys) void client.invalidateQueries({ queryKey });
}

/** The cached value of a query, for actions outside React (palette commands); undefined when never read. */
export function cached<T>(key: readonly unknown[]): T | undefined {
  return client?.getQueryData<T>(key);
}

export const useTags = () => useQuery({ queryKey: qk.tags, queryFn: () => call("list_tags") });
export const useAlbums = () => useQuery({ queryKey: qk.albums, queryFn: () => call("list_albums") });
export const useSmartFilters = () => useQuery({ queryKey: qk.smartFilters, queryFn: () => call("list_smart_filters") });

export function useJournal(enabled: boolean) {
  return useQuery({ queryKey: qk.journal, enabled, queryFn: () => call("list_journal", { limit: JOURNAL_LIMIT }) });
}
