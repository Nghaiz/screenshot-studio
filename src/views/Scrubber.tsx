// Date scrubber: one button per month bucket (newest first); a year label where the year changes.
import type { TimelineBucket } from "@bindings/TimelineBucket";
import { useT } from "../i18n";
import { monthLabel } from "./TimelineGrid";

interface Props {
  buckets: readonly TimelineBucket[];
  lang: string;
  onJump: (firstIndex: number) => void;
}

export function Scrubber({ buckets, lang, onJump }: Props) {
  const t = useT();
  const max = Math.max(1, ...buckets.map((b) => b.count));
  let index = 0;
  let lastYear = -1;
  return (
    <nav aria-label={t("timeline.label")} className="ss-panel flex w-28 shrink-0 flex-col overflow-y-auto py-2 text-xs">
      {buckets.map((b) => {
        const first = index;
        index += b.count;
        const d = new Date(b.startMs);
        const year = d.getFullYear();
        const showYear = year !== lastYear;
        lastYear = year;
        const label = monthLabel(b.startMs, lang);
        return (
          <div key={b.startMs}>
            {showYear && <div className="px-3 pt-2 font-semibold">{year}</div>}
            <button
              type="button"
              className="flex w-full items-center gap-2 px-3 py-0.5 text-left hover:opacity-100"
              style={{ opacity: 0.8 }}
              title={t("timeline.jump", { label, count: b.count })}
              aria-label={t("timeline.jump", { label, count: b.count })}
              onClick={() => onJump(first)}
            >
              <span className="w-14 shrink-0 whitespace-nowrap">{d.toLocaleDateString(lang, { month: "short" })}</span>
              <span className="h-1 rounded" style={{ width: `${(b.count / max) * 32}px`, background: "var(--ss-accent)" }} />
            </button>
          </div>
        );
      })}
    </nav>
  );
}
