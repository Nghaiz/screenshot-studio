// Degraded mode (contract-sidecar-jsonrpc § Degradation): why an AI control is off (tooltip text), the one banner per
// session while the sidecar is `absent` or `error`, and the AI status pill in the status bar. `starting` (~15 s after
// launch) is a calm state: a pulsing pill and "AI starting" tooltips, never the banner.
import { useEffect, type ReactNode } from "react";
import type { AiTask } from "@bindings/AiTask";
import type { SidecarState } from "@bindings/SidecarState";
import { useSidecarStatus } from "../data/smart";
import { useT, type MessageKey, type Translate } from "../i18n";
import { AI_NOTICE_SETTLE_MS } from "../lib/defaults";
import { AI_TASKS, isNoticeState, type AiGate } from "../lib/sidecar";
import { useView } from "../state/view";

/** Display name of an AI job stage (`job-progress.stage` is the task name); any other stage shows as is. */
export const taskName = (t: Translate, stage: string) =>
  (AI_TASKS as readonly string[]).includes(stage) ? t(`ai.task.${stage as AiTask}`) : stage;

/** Full tooltip text for a gated control; null when the control is usable. */
export function gateReason(t: Translate, gate: AiGate): string | null {
  return gate.block === null ? null : t(`ai.why.${gate.block}`);
}

/** Short visible marker next to a gated filter field. */
export function gateShort(t: Translate, gate: AiGate): string | null {
  if (gate.block === null) return null;
  return gate.block === "starting" ? t("ai.short.starting") : t("ai.short.off");
}

/**
 * Wraps gated controls so the tooltip shows even where a disabled control swallows hover; `data-ai-block` names the
 * reason for tests and styling.
 */
export function AiGated({ gate, children, className }: { gate: AiGate; children: ReactNode; className?: string }) {
  const t = useT();
  const reason = gateReason(t, gate);
  return (
    <span className={className ?? "contents"} title={reason ?? undefined} data-ai-block={gate.block ?? undefined}>
      {children}
    </span>
  );
}

export const openAiSettings = () => useView.getState().setSettings(true, "ai");

export function AiBanner() {
  const t = useT();
  const status = useSidecarStatus();
  const notice = useView((s) => s.aiNotice);
  const bad = isNoticeState(status);

  useEffect(() => {
    const v = useView.getState();
    if (notice === "pending" && bad) {
      const id = setTimeout(() => useView.getState().setAiNotice("shown"), AI_NOTICE_SETTLE_MS);
      return () => clearTimeout(id);
    }
    // The sidecar came back (restart, or the crash backoff recovered): the banner goes and stays gone.
    if (notice === "shown" && !bad) v.setAiNotice("done");
  }, [notice, bad]);

  if (notice !== "shown" || !status) return null;
  const key: MessageKey = status.state === "error" ? "ai.banner.error" : "ai.banner.absent";
  return (
    <section role="region" aria-label={t("ai.bannerLabel")} data-testid="ai-banner"
      className="ss-panel ss-notice flex flex-wrap items-center gap-3 px-3 py-2 text-sm">
      <span aria-hidden>⚠</span>
      <div className="min-w-0 flex-1">
        <p className="m-0">{t(key)}</p>
        {status.detail && <p className="ss-muted m-0 truncate text-xs" title={status.detail}>{status.detail}</p>}
      </div>
      <button type="button" className="ss-btn ss-btn-primary" onClick={openAiSettings}>{t("ai.openSettings")}</button>
      <button type="button" className="ss-btn" onClick={() => useView.getState().setAiNotice("done")}>{t("ai.dismiss")}</button>
    </section>
  );
}

const PILL: Record<SidecarState, MessageKey> = {
  ready: "ai.pill.ready",
  starting: "ai.pill.starting",
  absent: "ai.pill.absent",
  error: "ai.pill.error",
  disabled: "ai.pill.disabled",
};

export function AiStatusPill() {
  const t = useT();
  const status = useSidecarStatus();
  const state: SidecarState = status?.state ?? "starting";
  const text = t(PILL[state], { device: status?.device?.toUpperCase() ?? t("ai.deviceNone") });
  return (
    <button type="button" className="ss-pill flex items-center gap-1.5" data-state={state} data-testid="ai-pill"
      aria-label={t("ai.pillLabel", { state: text })} title={status?.detail ?? text} onClick={openAiSettings}>
      <span aria-hidden className={`ss-dot ${state === "starting" ? "ss-pulse" : ""}`} />
      <span>{text}</span>
    </button>
  );
}
