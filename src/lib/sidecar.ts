// AI sidecar gating (contract-sidecar-jsonrpc § Degradation): `SidecarStatus` decides, per capability, whether an AI
// control is usable, and if not, why. Pure functions; the hooks are in `data/smart.ts`, the notices in AiNotice.tsx.
import type { AiTask } from "@bindings/AiTask";
import type { SidecarCapability } from "@bindings/sidecar/SidecarCapability";
import type { SidecarState } from "@bindings/SidecarState";
import type { SidecarStatus } from "@bindings/SidecarStatus";
import { DEFAULT_HF_HOME, SOURCE_DIR } from "./defaults";

export type { SidecarCapability };

/** Every capability, in display order (contract `initialize` result). */
export const CAPABILITIES = ["ocr", "embed", "classify", "color", "pii"] as const satisfies readonly SidecarCapability[];

/** The AI job's tasks and the capability each needs (same pairing as ss-core `ai::capability_of`). */
export const AI_TASKS = ["ocr", "embed", "classify", "color"] as const satisfies readonly AiTask[];
export const TASK_CAPABILITY: Record<AiTask, SidecarCapability> = { ocr: "ocr", embed: "embed", classify: "classify", color: "color" };

/** Why a control is off: the sidecar state, or `missing` when it is ready without that capability. */
export type AiBlock = Exclude<SidecarState, "ready"> | "missing";
export interface AiGate {
  enabled: boolean;
  block: AiBlock | null;
}

const OPEN: AiGate = { enabled: true, block: null };

/** A status not known yet reads as `starting`: the app launches the sidecar at start-up, so that is the calm guess. */
export function aiGate(status: SidecarStatus | undefined, cap: SidecarCapability): AiGate {
  if (!status) return { enabled: false, block: "starting" };
  if (status.state !== "ready") return { enabled: false, block: status.state };
  return status.capabilities.includes(cap) ? OPEN : { enabled: false, block: "missing" };
}

/** States that earn the once-per-session banner. `starting` is normal for ~15 s after launch; `disabled` is chosen. */
export const NOTICE_STATES: readonly SidecarState[] = ["absent", "error"];
export const isNoticeState = (s: SidecarStatus | undefined) => s !== undefined && NOTICE_STATES.includes(s.state);

/** The tasks "Run AI on library" sends: those whose capability the ready sidecar has; none otherwise. */
export function runnableTasks(status: SidecarStatus | undefined): AiTask[] {
  if (status?.state !== "ready") return [];
  return AI_TASKS.filter((task) => status.capabilities.includes(TASK_CAPABILITY[task]));
}

/**
 * The one-time setup, shown verbatim in Settings > AI and copied as is (PowerShell; `sidecar/README.md` § Setup,
 * plus the source dir and the model cache so it runs from any prompt). `venv` = the `sidecarVenv` setting in force.
 */
export function setupCommand(venv: string): string {
  return [
    `$env:UV_PROJECT_ENVIRONMENT = "${venv}"`,
    `$env:HF_HOME = "${DEFAULT_HF_HOME}"`,
    `cd "${SOURCE_DIR}"`,
    "uv sync --project sidecar",
    `cd sidecar; & "$env:UV_PROJECT_ENVIRONMENT\\Scripts\\python.exe" -m ss_sidecar --prefetch`,
  ].join("\n");
}
