// Cleanup wizard: proposes exact (optionally similar) duplicate copies, blurry and tiny images for the in-app trash,
// and old in-app trash for the Windows Recycle Bin, one step at a time, then a review step whose Apply is the explicit
// confirmation. Only reversible actions (phase 3.3 risks): `trash_images` and `recycle_images`, both undoable; never
// `request_permanent_delete`. Each group's best image is never proposed as a duplicate copy.
import { useMemo, useState, type ReactNode } from "react";
import type { DuplicateResult } from "@bindings/DuplicateResult";
import { applyCleanup } from "../../actions/smart";
import { useDuplicates, useIssues, useNumberSetting, useOldTrash } from "../../data/smart";
import { useT, type MessageKey } from "../../i18n";
import { DUP_HAMMING_DEFAULT, OLD_TRASH_DAYS, WIZARD_PREVIEW_COUNT } from "../../lib/defaults";
import { toAppError } from "../../lib/ipc";
import { SmartThumb } from "./SmartThumb";

export type Candidate = "duplicates" | "blurry" | "tiny" | "oldTrash";
type Destination = "trash" | "recycle";

/** The proposal steps, in order; `dest` is where an included step's images go. */
const STEPS: { id: Candidate; title: MessageKey; body: MessageKey; dest: Destination }[] = [
  { id: "duplicates", title: "clean.dupTitle", body: "clean.dupBody", dest: "trash" },
  { id: "blurry", title: "clean.blurTitle", body: "clean.blurBody", dest: "trash" },
  { id: "tiny", title: "clean.tinyTitle", body: "clean.tinyBody", dest: "trash" },
  { id: "oldTrash", title: "clean.oldTitle", body: "clean.oldBody", dest: "recycle" },
];
const DEFAULT_INCLUDE: Record<Candidate, boolean> = { duplicates: true, blurry: false, tiny: false, oldTrash: false };
const PREVIEW_PX = 96;

/** Copies proposed from duplicate groups: every non-best member, minus any image that is the best of some group. */
export function duplicateCopies(...results: (DuplicateResult | undefined)[]): number[] {
  const groups = results.flatMap((r) => r?.groups ?? []);
  const keep = new Set(groups.map((g) => g.bestId));
  const out = new Set<number>();
  for (const g of groups) for (const id of g.imageIds) if (!keep.has(id)) out.add(id);
  return [...out];
}

interface StepData {
  ids: number[] | null;
  error: string | null;
}

export default function CleanupWizard() {
  const t = useT();
  const [step, setStep] = useState(0); // 0 intro, 1..STEPS.length the proposals, then the review
  const [include, setInclude] = useState(DEFAULT_INCLUDE);
  const [similar, setSimilar] = useState(false);
  const [applied, setApplied] = useState(false);
  const [busy, setBusy] = useState(false);
  const started = step > 0;
  const exact = useDuplicates("exact", 0, started);
  const near = useDuplicates("similar", null, started && similar); // null = the `dupHamming` setting
  const hamming = useNumberSetting("dupHamming", DUP_HAMMING_DEFAULT);
  const issues = useIssues(started);
  const old = useOldTrash(started);

  const data: Record<Candidate, StepData> = useMemo(() => {
    const err = (e: unknown) => (e ? toAppError(e).message : null);
    const dupReady = exact.data && (!similar || near.data);
    return {
      duplicates: { ids: dupReady ? duplicateCopies(exact.data, similar ? near.data : undefined) : null, error: err(exact.error ?? (similar ? near.error : null)) },
      blurry: { ids: issues.data?.blurry ?? null, error: err(issues.error) },
      tiny: { ids: issues.data?.tiny ?? null, error: err(issues.error) },
      oldTrash: { ids: old.data ?? null, error: err(old.error) },
    };
  }, [exact.data, exact.error, near.data, near.error, similar, issues.data, issues.error, old.data, old.error]);

  const plan = useMemo(() => {
    const trash = new Set<number>();
    const recycle = new Set<number>();
    for (const s of STEPS) {
      if (!include[s.id]) continue;
      for (const id of data[s.id].ids ?? []) (s.dest === "trash" ? trash : recycle).add(id);
    }
    return { trash: [...trash], recycle: [...recycle] };
  }, [include, data]);

  const total = STEPS.length + 2;
  const reset = () => {
    setStep(0);
    setApplied(false);
    setInclude(DEFAULT_INCLUDE);
  };
  const apply = async () => {
    setBusy(true);
    try {
      await applyCleanup(plan);
      setApplied(true);
    } finally {
      setBusy(false);
    }
  };

  let body;
  if (applied) {
    body = (
      <>
        <p className="m-0" role="status">{t("clean.done")}</p>
        <Nav><button type="button" className="ss-btn" onClick={reset}>{t("clean.again")}</button></Nav>
      </>
    );
  } else if (step === 0) {
    body = (
      <>
        <p className="m-0 max-w-2xl">{t("clean.intro")}</p>
        <Nav><button type="button" className="ss-btn ss-btn-primary" onClick={() => setStep(1)}>{t("clean.start")}</button></Nav>
      </>
    );
  } else if (step <= STEPS.length) {
    const s = STEPS[step - 1];
    const d = data[s.id];
    body = (
      <>
        <h2 className="m-0 text-base font-semibold">{t(s.title)}</h2>
        <p className="m-0 max-w-2xl">{t(s.body, { days: OLD_TRASH_DAYS })}</p>
        <p className="ss-muted m-0 text-xs">{t(s.dest === "trash" ? "clean.toTrash" : "clean.toRecycle")}</p>
        {s.id === "duplicates" && (
          <label className="flex items-center gap-2">
            <input type="checkbox" checked={similar} onChange={(e) => setSimilar(e.target.checked)} />
            {t("clean.similar", { n: hamming })}
          </label>
        )}
        {d.error ? (
          <p className="m-0" role="alert">{t("clean.failed", { message: d.error })}</p>
        ) : d.ids === null ? (
          <p className="ss-muted m-0" aria-busy="true">{t("clean.loading")}</p>
        ) : (
          <>
            <label className="flex items-center gap-2 font-semibold">
              <input type="checkbox" checked={include[s.id] && d.ids.length > 0} disabled={d.ids.length === 0}
                onChange={(e) => setInclude((x) => ({ ...x, [s.id]: e.target.checked }))} />
              {t("clean.include")} · <span data-testid="clean-count">{t("clean.count", { count: d.ids.length })}</span>
            </label>
            <Preview ids={d.ids} />
          </>
        )}
        <Nav>
          <button type="button" className="ss-btn" onClick={() => setStep(step - 1)}>{t("clean.back")}</button>
          <button type="button" className="ss-btn ss-btn-primary" onClick={() => setStep(step + 1)}>{t("clean.next")}</button>
        </Nav>
      </>
    );
  } else {
    const nothing = plan.trash.length === 0 && plan.recycle.length === 0;
    body = (
      <>
        <h2 className="m-0 text-base font-semibold">{t("clean.reviewTitle")}</h2>
        {nothing ? (
          <p className="ss-muted m-0">{t("clean.nothing")}</p>
        ) : (
          <ul className="m-0 flex flex-col gap-1 pl-5" data-testid="clean-review">
            {plan.trash.length > 0 && <li>{t("clean.reviewTrash", { count: plan.trash.length })}</li>}
            {plan.recycle.length > 0 && <li>{t("clean.reviewRecycle", { count: plan.recycle.length })}</li>}
          </ul>
        )}
        <p className="ss-muted m-0 text-xs">{t("clean.safety")}</p>
        <Nav>
          <button type="button" className="ss-btn" onClick={() => setStep(step - 1)}>{t("clean.back")}</button>
          <button type="button" className="ss-btn ss-btn-primary" disabled={nothing || busy} onClick={() => void apply()}>{t("clean.apply")}</button>
        </Nav>
      </>
    );
  }

  return (
    <section aria-label={t("clean.title")} className="ss-panel flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-4 text-sm">
      <div className="flex items-center gap-3">
        <span className="ss-muted text-xs">{t("clean.step", { n: applied ? total : step + 1, total })}</span>
        <div aria-hidden className="flex gap-1">
          {Array.from({ length: total }, (_, i) => (
            <span key={i} className="ss-step-dot" data-on={applied || i <= step} />
          ))}
        </div>
      </div>
      {body}
    </section>
  );
}

function Preview({ ids }: { ids: readonly number[] }) {
  const t = useT();
  if (ids.length === 0) return null;
  const shown = ids.slice(0, WIZARD_PREVIEW_COUNT);
  return (
    <div className="flex flex-wrap items-end gap-2">
      {shown.map((id) => (
        <SmartThumb key={id} id={id} size={PREVIEW_PX} caption={false} />
      ))}
      {ids.length > shown.length && <span className="ss-muted text-xs">{t("clean.more", { count: ids.length - shown.length })}</span>}
    </div>
  );
}

function Nav({ children }: { children: ReactNode }) {
  return <div className="flex gap-2 pt-1">{children}</div>;
}
