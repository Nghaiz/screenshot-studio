// First-run screen: shown while no library root exists. Offers the default OneDrive Screenshots folder
// (design-intent § Names and paths) or a folder picked with the system dialog.
import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { addFolder, pickAndAddFolder } from "../actions/images";
import { useT } from "../i18n";
import { DEFAULT_LIBRARY_ROOT } from "../lib/defaults";
import { qk } from "../lib/query";

export function FirstRun() {
  const t = useT();
  const qc = useQueryClient();
  const [busy, setBusy] = useState(false);
  const run = async (fn: () => Promise<boolean>) => {
    setBusy(true);
    const ok = await fn();
    setBusy(false);
    if (ok) await qc.invalidateQueries({ queryKey: qk.appInfo });
  };

  return (
    <main className="flex h-full items-center justify-center p-6">
      <section className="ss-panel flex max-w-xl flex-col gap-4 p-8">
        <h1 className="m-0 text-2xl font-semibold" style={{ color: "var(--ss-accent)" }}>
          {t("firstRun.title")}
        </h1>
        <p className="m-0">{t("firstRun.body")}</p>
        <code className="ss-panel block break-all px-3 py-2 text-sm">{DEFAULT_LIBRARY_ROOT}</code>
        <p className="ss-muted m-0 text-xs">{t("firstRun.protected")}</p>
        <div className="flex flex-wrap gap-2">
          <button type="button" className="ss-btn ss-btn-primary" disabled={busy} onClick={() => void run(() => addFolder(DEFAULT_LIBRARY_ROOT))}>
            {busy ? t("firstRun.adding") : t("firstRun.useDefault")}
          </button>
          <button type="button" className="ss-btn" disabled={busy} onClick={() => void run(pickAndAddFolder)}>
            {t("firstRun.choose")}
          </button>
        </div>
      </section>
    </main>
  );
}
