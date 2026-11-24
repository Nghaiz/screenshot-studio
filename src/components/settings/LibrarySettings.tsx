// Settings > Library (phase 4.2): library roots, watcher, global hotkey rebind, autostart, cloud-only downloads,
// and the data folder (read-only). Backend settings go through `set_setting`; the backend applies them.
import { useState, type ReactNode } from "react";
import type { AppWarningCode } from "@bindings/AppWarningCode";
import { askRemoveRoot, pickAndAddFolder } from "../../actions/images";
import { acceleratorOf, downloadCloudOnly, setSetting } from "../../actions/settings";
import { invalidate } from "../../data/organize";
import { useAppInfo, useCounts } from "../../data/library";
import { useSettings } from "../../data/smart";
import { useT } from "../../i18n";
import { qk } from "../../lib/query";

/** Contract defaults of the settings this tab shows (contract-tauri-ipc § Setting keys). */
export const SETTING_DEFAULTS = { hotkey: "Ctrl+Alt+S", watcherEnabled: true, autostart: false, autoDownloadCloudOnly: false } as const;

export default function LibrarySettings() {
  const t = useT();
  const info = useAppInfo().data;
  const counts = useCounts().data;
  const settings = useSettings().data ?? {};
  const bool = (k: keyof typeof SETTING_DEFAULTS & ("watcherEnabled" | "autostart" | "autoDownloadCloudOnly")) =>
    typeof settings[k] === "boolean" ? (settings[k] as boolean) : SETTING_DEFAULTS[k];
  const hotkey = typeof settings.hotkey === "string" ? settings.hotkey : SETTING_DEFAULTS.hotkey;
  const warning = (code: AppWarningCode) => info?.warnings.find((w) => w.code === code);
  const cloudOnly = counts?.cloudOnly ?? 0;

  return (
    <>
      <Field label={t("settings.roots")}>
        <ul className="m-0 flex list-none flex-col gap-1 p-0" data-testid="settings-roots">
          {info?.libraryRoots.map((r) => (
            <li key={r.id} className="flex items-center gap-2 text-xs">
              <span className="min-w-0 flex-1 break-all">{r.path}{r.protectedRoot ? " 🔒" : ""}</span>
              <span className="ss-muted">{r.imageCount}</span>
              <button type="button" className="ss-btn" disabled={cloudOnly === 0} onClick={() => void downloadCloudOnly(r.id)}
                title={t("settings.downloadHint")}>{t("settings.download")}</button>
              <button type="button" className="ss-btn" onClick={() => askRemoveRoot(r)}>{t("confirm.remove")}</button>
            </li>
          ))}
        </ul>
        <button type="button" className="ss-btn self-start" onClick={async () => {
          if (await pickAndAddFolder()) invalidate(qk.appInfo);
        }}>{t("cmd.addFolder")}</button>
        {cloudOnly > 0 && <p className="ss-muted m-0 text-xs">{t("settings.cloudOnlyCount", { count: cloudOnly })}</p>}
      </Field>

      <Toggle checked={bool("watcherEnabled")} onChange={(v) => void setSetting("watcherEnabled", v)} label={t("settings.watcher")} />
      {warning("watcherFailed") && <p role="alert" className="m-0 text-xs">{warning("watcherFailed")?.message}</p>}
      <Toggle checked={bool("autoDownloadCloudOnly")} onChange={(v) => void setSetting("autoDownloadCloudOnly", v)} label={t("settings.autoDownload")} />
      <Toggle checked={bool("autostart")} onChange={(v) => void setSetting("autostart", v)} label={t("settings.autostart")} />

      <Field label={t("settings.hotkey")}>
        <HotkeyField value={hotkey} onPick={(v) => void setSetting("hotkey", v)} />
        {warning("hotkeyUnavailable") && <p role="alert" className="m-0 text-xs">{t("settings.hotkeyTaken", { reason: warning("hotkeyUnavailable")?.message ?? "" })}</p>}
      </Field>

      <Field label={t("settings.dataDir")}>
        <input className="ss-input text-xs" readOnly aria-label={t("settings.dataDir")} value={info?.dataDir ?? ""} />
      </Field>
    </>
  );
}

/** Press the new combination while the field is focused; Esc leaves it unchanged. */
function HotkeyField({ value, onPick }: { value: string; onPick: (v: string) => void }) {
  const t = useT();
  const [listening, setListening] = useState(false);
  return (
    <div className="flex items-center gap-2">
      <input className="ss-input w-40" readOnly aria-label={t("settings.hotkey")} data-testid="hotkey-field"
        value={listening ? t("settings.hotkeyPress") : value}
        onFocus={() => setListening(true)} onBlur={() => setListening(false)}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === "Escape" || e.key === "Tab") return setListening(false);
          e.preventDefault();
          const acc = acceleratorOf(e);
          if (!acc) return;
          setListening(false);
          if (acc !== value) onPick(acc);
        }} />
      <button type="button" className="ss-btn" disabled={value === SETTING_DEFAULTS.hotkey} onClick={() => onPick(SETTING_DEFAULTS.hotkey)}>
        {t("settings.hotkeyReset")}
      </button>
    </div>
  );
}

function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <label className="flex items-center gap-2">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      {label}
    </label>
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
