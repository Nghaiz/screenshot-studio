// Backend events -> cache invalidation and job progress (contract-tauri-ipc § Events).
import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { subscribe } from "../lib/events";
import { qk } from "../lib/query";
import { useView } from "../state/view";

/** How long a finished job stays visible in the status bar. */
const FINISHED_JOB_LINGER_MS = 4000;

export function useBackendEvents(): void {
  const qc = useQueryClient();
  useEffect(() => {
    const offChanged = subscribe("library-changed", (e) => {
      void qc.invalidateQueries({ queryKey: qk.imagesAll });
      void qc.invalidateQueries({ queryKey: qk.timelineAll });
      void qc.invalidateQueries({ queryKey: qk.counts });
      void qc.invalidateQueries({ queryKey: qk.stats });
      void qc.invalidateQueries({ queryKey: qk.appInfo });
      if (e.imageIds === null) void qc.invalidateQueries({ queryKey: ["image"] });
      else for (const id of e.imageIds) void qc.invalidateQueries({ queryKey: qk.image(id) });
    });
    const offJob = subscribe("job-progress", (p) => {
      const view = useView.getState();
      view.jobProgress(p);
      if (p.finished) setTimeout(() => useView.getState().clearJob(p.jobId), FINISHED_JOB_LINGER_MS);
    });
    return () => {
      offChanged();
      offJob();
    };
  }, [qc]);
}
