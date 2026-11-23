// Settings > AI (lazy-loaded): sidecar status, device, capabilities, the one-time setup commands shown verbatim with a
// copy button, "Run AI on library" (`start_ai`) and its progress from `job-progress`.
import type { AiDevice } from "@bindings/AiDevice";
import type { JobProgress } from "@bindings/JobProgress";
import type { ReactNode } from "react";
import { cancelJob, copySetupCommand, restartSidecar, runAiOnLibrary, setAiDevice } from "../../actions/smart";
import { useSettings, useSidecarStatus } from "../../data/smart";
import { useT } from "../../i18n";
import { DEFAULT_SIDECAR_VENV } from "../../lib/defaults";
import { CAPABILITIES, runnableTasks, setupCommand } from "../../lib/sidecar";
import { useView } from "../../state/view";
import { taskName } from "../AiNotice";

const DEVICES = ["auto", "cuda", "cpu"] as const satisfies readonly AiDevice[];

/** The venv the `sidecarVenv` setting names (null or blank = the default venv), as ss-core `venv_from_setting`. */
export function venvOf(value: unknown): string {
  return typeof value === "string" && value.trim() ? value.trim() : DEFAULT_SIDECAR_VENV;
}

export default function AiSettings() {
  const t = useT();
  const status = useSidecarStatus();
  const settings = useSettings();
  const jobs = useView((s) => s.jobs);
  const job = Object.values(jobs).find((j) => j.kind === "ai" && !j.finished) ?? null;
  const state = status?.state ?? "starting";
  const venv = venvOf(settings.data?.sidecarVenv);
  const device = (DEVICES as readonly unknown[]).includes(settings.data?.aiDevice) ? (settings.data?.aiDevice as AiDevice) : "auto";
  const command = setupCommand(venv);
  const tasks = runnableTasks(status);
  const ready = state === "ready";

  return (
    <section aria-label={t("ai.title")} className="flex flex-col gap-4">
      <Block label={t("ai.status")}>
        <p className="m-0 flex items-center gap-2" data-testid="ai-state">
          <span aria-hidden className={`ss-dot ${state === "starting" ? "ss-pulse" : ""}`} data-state={state} />
          {t(`ai.state.${state}`)}
          {status?.version && <span className="ss-muted text-xs">{t("ai.version", { version: status.version })}</span>}
        </p>
        {status?.detail && <p className="ss-muted m-0 break-words text-xs" data-testid="ai-detail">{status.detail}</p>}
        <button type="button" className="ss-btn self-start" onClick={() => void restartSidecar()}>{t("ai.restart")}</button>
      </Block>

      <Block label={t("ai.device")}>
        <p className="m-0">{status?.device ? status.device.toUpperCase() : t("ai.deviceNone")}</p>
        <label className="flex items-center gap-2 text-xs">
          <span className="ss-muted">{t("ai.devicePref")}</span>
          <select className="ss-input" aria-label={t("ai.devicePref")} value={device} disabled={!settings.data}
            onChange={(e) => void setAiDevice(e.target.value as AiDevice)}>
            {DEVICES.map((d) => (
              <option key={d} value={d}>{t(`ai.device.${d}`)}</option>
            ))}
          </select>
        </label>
      </Block>

      <Block label={t("ai.capabilities")}>
        <ul className="m-0 flex list-none flex-wrap gap-1 p-0" aria-label={t("ai.capabilities")}>
          {CAPABILITIES.map((c) => {
            const on = ready && !!status?.capabilities.includes(c);
            return (
              <li key={c} className="ss-chip" data-on={on} aria-label={`${t(`ai.cap.${c}`)}: ${t(on ? "ai.capOn" : "ai.capOff")}`}>
                <span aria-hidden>{on ? "✓" : "–"}</span> {t(`ai.cap.${c}`)}
              </li>
            );
          })}
        </ul>
      </Block>

      <Block label={t("ai.run")}>
        <p className="ss-muted m-0 text-xs">{t("ai.runHelp")}</p>
        {tasks.length > 0 && <p className="ss-muted m-0 text-xs">{t("ai.runTasks", { tasks: tasks.map((x) => taskName(t, x)).join(", ") })}</p>}
        {job ? (
          <JobRow job={job} />
        ) : (
          <button type="button" className="ss-btn ss-btn-primary self-start" disabled={tasks.length === 0}
            title={tasks.length === 0 ? t(state === "starting" ? "ai.why.starting" : "ai.notReady") : undefined}
            onClick={() => void runAiOnLibrary(tasks)}>
            {t("ai.run")}
          </button>
        )}
      </Block>

      <Setup command={command} open={!ready} />
    </section>
  );
}

function JobRow({ job }: { job: JobProgress }) {
  const t = useT();
  const pct = job.total > 0 ? Math.round((job.done / job.total) * 100) : 0;
  return (
    <div className="flex flex-col gap-1" data-testid="ai-job">
      <div className="flex items-center gap-2 text-xs">
        <span className="flex-1">
          {job.total > 0 ? t("ai.progress", { task: taskName(t, job.stage), done: job.done, total: job.total }) : t("ai.queued")}
        </span>
        <button type="button" className="ss-btn" onClick={() => void cancelJob(job.jobId)}>{t("ai.cancel")}</button>
      </div>
      <div className="h-1.5 w-full overflow-hidden rounded" style={{ background: "var(--ss-border)" }}
        role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct} aria-label={t("ai.run")}>
        <div className="h-full" style={{ width: `${pct}%`, background: "var(--ss-accent)", transition: "width var(--ss-dur)" }} />
      </div>
    </div>
  );
}

/** Shown open while the sidecar is not ready (that is when it is needed), folded once it is. */
function Setup({ command, open }: { command: string; open: boolean }) {
  const t = useT();
  const body = (
    <>
      <p className="ss-muted m-0 text-xs">{t("ai.setupHelp")}</p>
      <pre className="ss-code m-0 overflow-x-auto whitespace-pre p-2 text-xs" data-testid="ai-setup-command">{command}</pre>
      <button type="button" className="ss-btn self-start" onClick={() => void copySetupCommand(command)}>{t("ai.copy")}</button>
    </>
  );
  if (open) return <Block label={t("ai.setup")}>{body}</Block>;
  return (
    <details className="flex flex-col gap-1.5">
      <summary className="ss-muted cursor-pointer text-xs font-semibold uppercase tracking-wide">{t("ai.setup")}</summary>
      <div className="flex flex-col gap-1.5 pt-1.5">{body}</div>
    </details>
  );
}

function Block({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <span className="ss-muted text-xs font-semibold uppercase tracking-wide">{label}</span>
      {children}
    </div>
  );
}
