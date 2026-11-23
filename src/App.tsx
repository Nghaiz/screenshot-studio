// App root: applies the appearance, wires backend events, and picks first-run vs the main shell.
import { useLayoutEffect } from "react";
import { MotionConfig } from "motion/react";
import { FirstRun } from "./components/FirstRun";
import { AnimatedBackground } from "./components/Overlays";
import { useBackendEvents } from "./data/events";
import { useAppInfo } from "./data/library";
import { useT } from "./i18n";
import { toAppError } from "./lib/ipc";
import { Shell } from "./Shell";
import { usePrefs } from "./state/prefs";
import { applyAppearance, effectiveMotion } from "./themes";

export default function App() {
  const t = useT();
  const { theme, accent, motion, density, language } = usePrefs();
  useLayoutEffect(() => applyAppearance({ theme, accent, motion, density }), [theme, accent, motion, density]);
  useLayoutEffect(() => {
    document.documentElement.lang = language;
  }, [language]);
  useBackendEvents();
  const info = useAppInfo();

  let body;
  if (info.isPending) body = <div className="ss-shimmer h-full w-full" aria-busy="true" />;
  else if (info.isError) {
    const e = toAppError(info.error);
    body = (
      <div className="flex h-full items-center justify-center p-6">
        <p className="ss-panel px-6 py-4 text-sm">{t("grid.error", { message: `${e.message}${e.detail ? ` (${e.detail})` : ""}` })}</p>
      </div>
    );
  } else if (info.data.libraryRoots.length === 0) body = <FirstRun />;
  else body = <Shell info={info.data} />;

  return (
    <MotionConfig reducedMotion={effectiveMotion(motion) === "off" ? "always" : "never"}>
      <AnimatedBackground />
      {body}
    </MotionConfig>
  );
}
