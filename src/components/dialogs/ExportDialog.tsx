// ZIP / PDF export of the selection: destination (OS save dialog), PDF page options, then the job's progress
// (`job-progress` kind `export`) with Cancel while it runs and Reveal once it is done.
import { useEffect, useState } from "react";
import type { JobProgress } from "@bindings/JobProgress";
import { pickExportPath, revealInExplorer, startExport, type ExportKind, type PdfOptions } from "../../actions/share";
import { attempt } from "../../actions/report";
import { useT } from "../../i18n";
import { call } from "../../lib/ipc";
import { useView } from "../../state/view";
import { Modal } from "./Modal";

const PER_PAGE = [1, 2, 4] as const;

export function ExportDialog({ ids, kind }: { ids: number[]; kind: ExportKind }) {
  const t = useT();
  const [dest, setDest] = useState<string | null>(null);
  const [pdf, setPdf] = useState<PdfOptions>({ pageSize: "a4", perPage: 1 });
  const [jobId, setJobId] = useState<number | null>(null);
  const live = useView((s) => (jobId === null ? undefined : s.jobs[jobId]));
  // The status bar drops a finished job after a few seconds; the dialog keeps showing its last state.
  const [seen, setSeen] = useState<JobProgress | undefined>(undefined);
  useEffect(() => {
    if (live) setSeen(live);
  }, [live]);
  const job = live ?? seen;
  const close = () => useView.getState().closeDialog();
  const title = t(kind === "zip" ? "export.zipTitle" : "export.pdfTitle", { count: ids.length });

  const choose = async () => {
    const p = await pickExportPath(kind);
    if (p) setDest(p);
  };
  const run = async () => {
    if (!dest) return;
    const id = await startExport(kind, ids, dest, pdf);
    if (id !== null) setJobId(id);
  };

  const running = jobId !== null && !job?.finished;
  const pct = job && job.total > 0 ? Math.round((job.done / job.total) * 100) : 0;
  return (
    <Modal label={title}>
      <h2 className="m-0 text-base font-semibold">{title}</h2>
      <div className="flex items-center gap-2">
        <span className="ss-muted min-w-0 flex-1 truncate text-xs" data-testid="export-dest">{dest ?? t("export.noDest")}</span>
        <button type="button" className="ss-btn" disabled={jobId !== null} onClick={() => void choose()}>{t("export.choose")}</button>
      </div>
      {kind === "pdf" && (
        <div className="flex flex-wrap items-center gap-3">
          <label className="flex items-center gap-1">
            {t("export.pageSize")}
            <select className="ss-input" value={pdf.pageSize} disabled={jobId !== null}
              onChange={(e) => setPdf({ ...pdf, pageSize: e.target.value as PdfOptions["pageSize"] })}>
              <option value="a4">A4</option>
              <option value="fit">{t("export.fit")}</option>
            </select>
          </label>
          <span role="group" aria-label={t("export.perPage")} className="flex items-center gap-1">
            {t("export.perPage")}
            {PER_PAGE.map((n) => (
              <button key={n} type="button" className="ss-btn" aria-pressed={pdf.perPage === n} disabled={jobId !== null}
                onClick={() => setPdf({ ...pdf, perPage: n })}>{n}</button>
            ))}
          </span>
        </div>
      )}
      {jobId !== null && (
        <div className="flex flex-col gap-1" role="status" aria-live="polite">
          <div className="h-2 overflow-hidden rounded-full" style={{ background: "var(--ss-border)" }}>
            <div className="h-full" style={{ width: `${job?.finished && !job.error ? 100 : pct}%`, background: "var(--ss-accent)" }} />
          </div>
          <span className="text-xs" data-testid="export-progress">
            {job?.error ? t("export.failed", { reason: job.error.message })
              : job?.finished ? t("export.done", { count: ids.length })
              : t("export.progress", { done: job?.done ?? 0, total: job?.total ?? ids.length })}
          </span>
        </div>
      )}
      <div className="flex justify-end gap-2">
        {running && jobId !== null && (
          <button type="button" className="ss-btn" onClick={() => void attempt(() => call("cancel_job", { jobId }))}>{t("export.cancelJob")}</button>
        )}
        {job?.finished && !job.error && dest && (
          <button type="button" className="ss-btn" onClick={() => void revealInExplorer(dest)}>{t("share.reveal")}</button>
        )}
        <button type="button" className="ss-btn" onClick={close}>{jobId === null ? t("actions.cancel") : t("export.close")}</button>
        {jobId === null && (
          <button type="button" className="ss-btn ss-btn-primary" disabled={!dest} onClick={() => void run()}>{t("export.start")}</button>
        )}
      </div>
    </Modal>
  );
}
