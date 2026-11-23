// Status bar: item count, selection, cloud-only count, and index progress from `job-progress` events.
import type { JobProgress } from "@bindings/JobProgress";
import { useCounts } from "../data/library";
import { useT } from "../i18n";
import { useView } from "../state/view";

export function StatusBar({ total }: { total: number }) {
  const t = useT();
  const selected = useView((s) => s.selection.size);
  const jobs = useView((s) => s.jobs);
  const counts = useCounts();
  const active = Object.values(jobs);

  return (
    <footer className="ss-panel flex items-center gap-4 px-3 py-1.5 text-xs" role="status" aria-live="polite">
      <span data-testid="status-total">{t("status.items", { count: total })}</span>
      {selected > 0 && <span>{t("status.selected", { count: selected })}</span>}
      {counts.data && counts.data.cloudOnly > 0 && <span className="ss-muted">{t("status.cloudOnly", { count: counts.data.cloudOnly })}</span>}
      <span className="flex-1" />
      {active.map((j) => (
        <JobLine key={j.jobId} job={j} />
      ))}
    </footer>
  );
}

function JobLine({ job }: { job: JobProgress }) {
  const t = useT();
  const pct = job.total > 0 ? Math.round((job.done / job.total) * 100) : 0;
  const text = job.error
    ? t("status.jobFailed", { message: job.error.message })
    : job.finished
      ? t("status.jobDone")
      : t("status.indexing", { done: job.done, total: job.total });
  return (
    <span className="flex items-center gap-2" title={job.currentPath ?? undefined}>
      <span>{text}</span>
      <span className="h-1.5 w-28 overflow-hidden rounded" style={{ background: "var(--ss-border)" }}>
        <span className="block h-full" style={{ width: `${pct}%`, background: "var(--ss-accent)", transition: "width var(--ss-dur)" }} />
      </span>
    </span>
  );
}
