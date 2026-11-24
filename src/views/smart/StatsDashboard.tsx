// Stats dashboard: aggregates from `get_stats` only (computed in SQL by the backend, never counted on the client, phase
// 3.3 risks), as stat tiles and hand-drawn SVG charts (charts.tsx).
import type { CountBy } from "@bindings/CountBy";
import { formatBytes } from "../../components/Cell";
import { ViewLoading } from "../../components/ViewLoading";
import { useStats } from "../../data/library";
import { useT, type Translate } from "../../i18n";
import { categoryName } from "../../lib/categories";
import { toAppError } from "../../lib/ipc";
import { usePrefs } from "../../state/prefs";
import { ASPECTS } from "../../components/FilterPanel";
import { BarList, ChartCard, Columns, type Datum } from "./charts";

const isAspect = (k: string): k is (typeof ASPECTS)[number] => (ASPECTS as readonly string[]).includes(k);

/** `byMonth` key "YYYY-MM" -> a local date on the 1st of that month. */
const monthDate = (key: string) => {
  const [y, m] = key.split("-").map(Number);
  return new Date(y, (m || 1) - 1, 1);
};

const rows = (list: readonly CountBy[], label: (k: string) => string): Datum[] =>
  list.map((c) => ({ key: c.key, label: label(c.key), value: c.count }));
/** Oldest month first, sorted here: the contract does not fix the order of `byMonth`. */
export const oldestFirst = (list: readonly CountBy[]) => [...list].sort((a, b) => a.key.localeCompare(b.key));
/** Bar lists read largest first whatever order the backend used (it sends `byFolder` by name, for the sidebar). */
export const largestFirst = (d: readonly Datum[]) => [...d].sort((a, b) => b.value - a.value || a.key.localeCompare(b.key));

export default function StatsDashboard() {
  const t = useT();
  const lang = usePrefs((s) => s.language);
  const q = useStats();
  const num = (n: number) => n.toLocaleString(lang);

  if (q.isPending) return <ViewLoading panel label={t("stats.loading")} />;
  if (q.isError) {
    return <p className="ss-panel m-0 p-4 text-sm" role="alert">{t("stats.failed", { message: toAppError(q.error).message })}</p>;
  }
  const s = q.data;
  const tiles: [string, string][] = [
    [t("stats.total"), num(s.total)],
    [t("stats.size"), formatBytes(s.totalBytes)],
    [t("stats.dupExact"), num(s.duplicatesExact)],
    [t("stats.dupSimilar"), num(s.duplicatesSimilar)],
    [t("stats.cloudOnly"), num(s.cloudOnly)],
    [t("stats.trashed"), num(s.trashed)],
    [t("stats.pending"), num(s.thumbPending)],
    [t("stats.missing"), num(s.missing)],
  ];
  const months = rows(oldestFirst(s.byMonth), (k) =>
    monthDate(k).toLocaleDateString(lang, { month: "short", year: "numeric" }),
  );

  return (
    <section aria-label={t("stats.title")} className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto">
      <dl className="m-0 grid grid-cols-[repeat(auto-fill,minmax(150px,1fr))] gap-2">
        {tiles.map(([label, value]) => (
          <div key={label} className="ss-panel flex flex-col gap-0.5 px-3 py-2">
            <dt className="ss-muted text-xs">{label}</dt>
            <dd className="m-0 text-xl font-semibold tabular-nums" data-testid={`tile-${label}`}>{value}</dd>
          </div>
        ))}
      </dl>
      <ChartCard title={t("stats.byMonth")} data={months} format={num} empty={t("stats.empty")}>
        <Columns data={months} label={t("stats.byMonth")} format={num} tick={(d, i) => monthTick(d, i, months.length, lang)} />
      </ChartCard>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(320px,1fr))] gap-3">
        <List title={t("stats.byAspect")} data={rows(s.byAspect, (k) => aspectLabel(t, k))} format={num} empty={t("stats.empty")} />
        <List title={t("stats.byFolder")} data={rows(s.byFolder, (k) => k || t("nav.rootFolder"))} format={num} empty={t("stats.empty")} />
        <List title={t("stats.byCategory")} data={rows(s.byCategory, (k) => categoryName(t, k))} format={num} empty={t("stats.noCategories")} />
        <List title={t("stats.topTags")} data={rows(s.topTags, (k) => k)} format={num} empty={t("nav.noTags")} />
      </div>
    </section>
  );
}

function List(props: { title: string; data: Datum[]; format: (n: number) => string; empty: string }) {
  return (
    <ChartCard title={props.title} data={props.data} format={props.format} empty={props.empty}>
      <BarList data={largestFirst(props.data)} format={props.format} />
    </ChartCard>
  );
}

const aspectLabel = (t: Translate, k: string) => (isAspect(k) ? t(`aspect.${k}`) : t("stats.unknown"));

/**
 * Up to 12 months: every month's short name. Longer: the year under each January, and under the first column when
 * the next January is at least 4 columns away (so two year labels never collide).
 */
function monthTick(d: Datum, i: number, n: number, lang: string): string | null {
  const date = monthDate(d.key);
  if (n <= 12) return date.toLocaleDateString(lang, { month: "short" });
  return date.getMonth() === 0 || (i === 0 && date.getMonth() <= 8) ? String(date.getFullYear()) : null;
}
