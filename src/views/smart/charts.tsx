// Hand-drawn SVG charts for the dashboard (no chart library). Single series, so the marks wear the theme accent and
// there is no legend (the card title names the series); text wears text tokens, never the series colour. Marks: columns
// at most 24 px wide with a 4 px rounded data end, square at the baseline, a 2 px gap between neighbours; hairline,
// solid, recessive gridlines; a hover tooltip per mark (`<title>`) over a hit area larger than the mark. Every chart has
// a table view (ChartCard), so nothing is colour- or hover-only.
import { useState, type ReactNode } from "react";
import { useT } from "../../i18n";

export interface Datum {
  key: string;
  label: string;
  value: number;
}

/** 0, the middle and a clean top (1, 2, 2.5, 5 x 10^n) at or above `max`. */
export function niceTicks(max: number): [number, number, number] {
  if (max <= 0) return [0, 1, 2];
  const pow = 10 ** Math.floor(Math.log10(max));
  const top = [1, 2, 2.5, 5, 10].map((m) => m * pow).find((v) => v >= max) ?? 10 * pow;
  return [0, top / 2, top];
}

/** A column whose top corners are rounded by `r` and whose base sits square on the baseline. */
function columnPath(x: number, y: number, w: number, h: number, r: number): string {
  const rr = Math.max(0, Math.min(r, w / 2, h));
  const b = y + h;
  return `M${x},${b}V${y + rr}Q${x},${y} ${x + rr},${y}H${x + w - rr}Q${x + w},${y} ${x + w},${y + rr}V${b}Z`;
}

const W = 760;
const H = 200;
const PAD_L = 44;
const PAD_R = 8;
const PAD_T = 10;
const PAD_B = 24;
const MAX_BAR = 24;
const BAR_GAP = 2;
const RADIUS = 4;

/** Columns over time (keys sorted oldest first). `tick(d)` returns an x-axis label for the columns that get one. */
export function Columns({ data, label, format, tick }: {
  data: readonly Datum[];
  label: string;
  format: (n: number) => string;
  tick: (d: Datum, i: number) => string | null;
}) {
  const ticks = niceTicks(Math.max(0, ...data.map((d) => d.value)));
  const top = ticks[2];
  const plotH = H - PAD_T - PAD_B;
  const slot = (W - PAD_L - PAD_R) / Math.max(1, data.length);
  const barW = Math.max(1, Math.min(MAX_BAR, slot - BAR_GAP));
  const y = (v: number) => PAD_T + plotH * (1 - v / top);
  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="ss-chart block h-auto w-full" role="img" aria-label={label}>
      {ticks.map((v) => (
        <g key={v}>
          <line x1={PAD_L} x2={W - PAD_R} y1={y(v)} y2={y(v)} style={{ stroke: "var(--ss-border)" }} strokeWidth={1} />
          <text x={PAD_L - 6} y={y(v)} textAnchor="end" dominantBaseline="middle" fontSize={11} style={{ fill: "var(--ss-muted)" }}>
            {format(v)}
          </text>
        </g>
      ))}
      {data.map((d, i) => {
        const x = PAD_L + i * slot + (slot - barW) / 2;
        const h = plotH * (d.value / top);
        const label = tick(d, i);
        return (
          <g key={d.key} className="ss-bar" data-key={d.key}>
            <title>{`${d.label}: ${format(d.value)}`}</title>
            <rect x={PAD_L + i * slot} y={PAD_T} width={slot} height={plotH} fill="transparent" />
            {d.value > 0 && <path d={columnPath(x, y(d.value), barW, h, RADIUS)} style={{ fill: "var(--ss-accent)" }} />}
            {label && (
              <text x={x + barW / 2} y={H - 6} textAnchor="middle" fontSize={11} style={{ fill: "var(--ss-muted)" }}>
                {label}
              </text>
            )}
          </g>
        );
      })}
    </svg>
  );
}

/** Horizontal bars, largest first, value at the tip. */
export function BarList({ data, format }: { data: readonly Datum[]; format: (n: number) => string }) {
  const max = Math.max(1, ...data.map((d) => d.value));
  return (
    <ul className="ss-chart m-0 flex list-none flex-col gap-1.5 p-0 text-xs">
      {data.map((d) => (
        <li key={d.key} className="ss-bar grid grid-cols-[minmax(0,9rem)_1fr_auto] items-center gap-2" data-key={d.key} title={`${d.label}: ${format(d.value)}`}>
          <span className="truncate">{d.label}</span>
          <svg width="100%" height={12} aria-hidden>
            <rect x={0} y={2} height={8} rx={RADIUS} width={`${Math.max(0.5, (d.value / max) * 100)}%`} style={{ fill: "var(--ss-accent)" }} />
          </svg>
          <span className="tabular-nums">{format(d.value)}</span>
        </li>
      ))}
    </ul>
  );
}

/** A titled card with a chart / table switch; `empty` replaces both when there is no data. */
export function ChartCard({ title, data, format, empty, children }: {
  title: string;
  data: readonly Datum[];
  format: (n: number) => string;
  empty: string;
  children: ReactNode;
}) {
  const t = useT();
  const [table, setTable] = useState(false);
  return (
    <section aria-label={title} className="ss-panel flex min-w-0 flex-col gap-2 p-3">
      <div className="flex items-center gap-2">
        <h2 className="m-0 flex-1 text-sm font-semibold">{title}</h2>
        {data.length > 0 && (
          <button type="button" className="ss-btn text-xs" aria-pressed={table} onClick={() => setTable(!table)}>
            {t(table ? "stats.chart" : "stats.table")}
          </button>
        )}
      </div>
      {data.length === 0 ? (
        <p className="ss-muted m-0 text-xs">{empty}</p>
      ) : table ? (
        <table className="w-full text-xs">
          <thead>
            <tr className="ss-muted text-left">
              <th className="font-normal">{t("stats.colKey")}</th>
              <th className="text-right font-normal">{t("stats.colCount")}</th>
            </tr>
          </thead>
          <tbody>
            {data.map((d) => (
              <tr key={d.key}>
                <td>{d.label}</td>
                <td className="text-right tabular-nums">{format(d.value)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        children
      )}
    </section>
  );
}
