// Appearance settings (all stored in localStorage, D9): theme, accent, motion, animated background,
// density, language.
import type { ReactNode } from "react";
import { LANGUAGES, useT } from "../i18n";
import { usePrefs } from "../state/prefs";
import { useView } from "../state/view";
import { DENSITIES, MOTION_LEVELS, prefersReducedMotion, THEME_IDS } from "../themes";

export function SettingsPanel() {
  const open = useView((s) => s.settingsOpen);
  if (!open) return null;
  return <SettingsBody />;
}

function SettingsBody() {
  const t = useT();
  const p = usePrefs();
  const close = () => useView.getState().setSettings(false);
  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={t("settings.title")}
      className="ss-panel fixed right-4 top-16 z-30 flex max-h-[80vh] w-80 flex-col gap-4 overflow-y-auto p-4 text-sm"
      onKeyDown={(e) => e.key === "Escape" && close()}
    >
      <div className="flex items-center">
        <h2 className="m-0 flex-1 text-base font-semibold">{t("settings.title")}</h2>
        <button type="button" className="ss-btn" onClick={close} aria-label={t("settings.close")}>
          ✕
        </button>
      </div>

      <Field label={t("settings.theme")}>
        <div className="grid grid-cols-2 gap-1">
          {THEME_IDS.map((id) => (
            <button key={id} type="button" className="ss-btn justify-start" aria-pressed={p.theme === id} onClick={() => p.set({ theme: id })}>
              {t(`theme.${id}`)}
            </button>
          ))}
        </div>
      </Field>

      <Field label={t("settings.accent")}>
        <div className="flex items-center gap-2">
          <input
            type="color"
            aria-label={t("settings.accent")}
            value={p.accent ?? "#7dd3fc"}
            onChange={(e) => p.set({ accent: e.target.value })}
          />
          <button type="button" className="ss-btn" aria-pressed={p.accent === null} onClick={() => p.set({ accent: null })}>
            {t("settings.accentReset")}
          </button>
        </div>
      </Field>

      <Field label={t("settings.motion")}>
        <Choice values={MOTION_LEVELS} current={p.motion} label={(m) => t(`motion.${m}`)} onPick={(motion) => p.set({ motion })} />
        {prefersReducedMotion() && <p className="ss-muted m-0 text-xs">{t("settings.reducedMotion")}</p>}
      </Field>

      <label className="flex items-center gap-2">
        <input type="checkbox" checked={p.animatedBg} onChange={(e) => p.set({ animatedBg: e.target.checked })} />
        {t("settings.animatedBg")}
      </label>

      <Field label={t("settings.density")}>
        <Choice values={DENSITIES} current={p.density} label={(d) => t(`density.${d}`)} onPick={(density) => p.set({ density })} />
      </Field>

      <Field label={t("settings.language")}>
        <Choice values={LANGUAGES} current={p.language} label={(l) => t(`lang.${l}`)} onPick={(language) => p.set({ language })} />
      </Field>
    </div>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <span className="ss-muted text-xs font-semibold uppercase tracking-wide">{label}</span>
      {children}
    </div>
  );
}

function Choice<T extends string>(props: { values: readonly T[]; current: T; label: (v: T) => string; onPick: (v: T) => void }) {
  return (
    <div className="flex flex-wrap gap-1">
      {props.values.map((v) => (
        <button key={v} type="button" className="ss-btn" aria-pressed={props.current === v} onClick={() => props.onPick(v)}>
          {props.label(v)}
        </button>
      ))}
    </div>
  );
}
