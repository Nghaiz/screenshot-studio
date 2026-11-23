// Paged image list for one ImageQuery (TanStack Query infinite query, keyset cursor from the backend).
import { useCallback, useEffect, useMemo } from "react";
import { useInfiniteQuery } from "@tanstack/react-query";
import type { AppError } from "@bindings/AppError";
import type { ImageQuery } from "@bindings/ImageQuery";
import type { ImageSummary } from "@bindings/ImageSummary";
import { call, toAppError } from "../lib/ipc";
import { PREFETCH_MARGIN } from "../lib/defaults";
import { qk } from "../lib/query";

export interface ImageList {
  query: ImageQuery;
  items: readonly ImageSummary[];
  total: number;
  loading: boolean;
  error: AppError | null;
  /** Call with the last visible index; fetches the next page when it is near the loaded end. */
  ensureLoaded: (index: number) => void;
}

/** `eager` loads every page in the background (justified and masonry need every aspect ratio). */
export function useImageList(query: ImageQuery, eager: boolean): ImageList {
  const q = useInfiniteQuery({
    queryKey: qk.images(query),
    queryFn: ({ pageParam }) => call("query_images", { ...query, cursor: pageParam }),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last.nextCursor,
  });
  const { data, hasNextPage, isFetchingNextPage, fetchNextPage } = q;
  const items = useMemo(() => data?.pages.flatMap((p) => p.items) ?? [], [data]);
  const total = data?.pages[0]?.total ?? 0;

  const ensureLoaded = useCallback(
    (index: number) => {
      if (index >= items.length - PREFETCH_MARGIN && hasNextPage && !isFetchingNextPage) void fetchNextPage();
    },
    [items.length, hasNextPage, isFetchingNextPage, fetchNextPage],
  );

  useEffect(() => {
    if (eager && hasNextPage && !isFetchingNextPage && !q.isError) void fetchNextPage();
  }, [eager, hasNextPage, isFetchingNextPage, fetchNextPage, q.isError]);

  return {
    query,
    items,
    total,
    loading: q.isPending,
    error: q.error ? toAppError(q.error) : null,
    ensureLoaded,
  };
}
