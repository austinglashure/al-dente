// Overlays: per-node styling from risk, coverage, churn or any measure.
// Overlays only restyle nodes; they never move them (no re-layout) and
// never touch edges, so the certainty toggles keep working. Pure functions.

import type { ChurnReport, CoverageReport, CoverageStatus, HotspotsReport, MetricsReport } from './types';

export type OverlayKind = 'none' | 'risk' | 'coverage' | 'churn';

/** How an overlay changes one node. `tag` names the style for tests and
 *  the legend (`untested`, `risky-untested`, …). */
export interface NodeStyle {
  color?: string;
  size?: number;
  /** Prefix for the label (the high-risk-untested marker). */
  mark?: string;
  tag?: string;
}

/** Everything the overlays read, by node ID. */
export interface OverlayData {
  coverage: Map<string, CoverageStatus>;
  risk: Map<string, number>;
  /** Symbols with a risky-untested finding. */
  riskyUntested: Set<string>;
  churn: Map<string, number>;
}

export const COVERAGE_COLORS: Record<CoverageStatus, string> = {
  direct: '#006d2c',
  indirect: '#74c476',
  weak: '#fec44f',
  untested: '#f0f0f0',
};

/** The namespace node ID of a symbol or impl ID, or null.
 *  Example: `namespaceOf('clj:.//shop.core/checkout')` is `'clj:.//shop.core'`. */
export function namespaceOf(id: string): string | null {
  const base = id.indexOf('//');
  const slash = id.lastIndexOf('/');
  return base >= 0 && slash > base + 1 ? id.slice(0, slash) : null;
}

/** A light-to-dark red for `t` in 0..1.
 *  Example: `heat(0)` is `'#fff5f0'`; `heat(1)` is `'#99000d'`. */
export function heat(t: number): string {
  const stops = [
    [0xff, 0xf5, 0xf0],
    [0xfc, 0xae, 0x91],
    [0xfb, 0x6a, 0x4a],
    [0xde, 0x2d, 0x26],
    [0x99, 0x00, 0x0d],
  ];
  const x = Math.min(1, Math.max(0, t)) * (stops.length - 1);
  const i = Math.min(stops.length - 2, Math.floor(x));
  const f = x - i;
  const a = stops[i] ?? [0, 0, 0];
  const b = stops[i + 1] ?? a;
  const c = a.map((v, k) => Math.round(v + ((b[k] ?? v) - v) * f));
  return `#${c.map((v) => v.toString(16).padStart(2, '0')).join('')}`;
}

/** Builds the overlay data from the API reports (any may be missing).
 *  Example: `overlayData(coverage, hotspots, null).coverage.get(id)` is `'direct'`. */
export function overlayData(coverage: CoverageReport | null, hotspots: HotspotsReport | null, churn: ChurnReport | null): OverlayData {
  return {
    coverage: new Map((coverage?.symbols ?? []).map((s) => [s.id, s.status])),
    risk: new Map((hotspots?.risk ?? []).map((r) => [r.id, r.risk])),
    riskyUntested: new Set((hotspots?.findings ?? []).filter((f) => f.kind === 'risky-untested').flatMap((f) => f.nodes)),
    churn: new Map((churn?.symbols ?? []).map((s) => [s.id, s.commits])),
  };
}

/** Per-namespace roll-ups of symbol values: the maximum (risk) or the sum
 *  (churn), and the tested share (coverage).
 *  Example: two symbols of `ns` with risk 0.2 and 0.5 give `ns` 0.5. */
export function rollUp(values: Map<string, number>, how: 'max' | 'sum'): Map<string, number> {
  const out = new Map<string, number>();
  for (const [id, v] of values) {
    const ns = namespaceOf(id);
    if (!ns) continue;
    const prev = out.get(ns) ?? 0;
    out.set(ns, how === 'max' ? Math.max(prev, v) : prev + v);
  }
  return out;
}

/** A styler for one overlay: node ID → style, or null for "no change".
 *  Values are scaled by the largest value among `visible` nodes, so the
 *  picture uses the whole colour range on any canvas.
 *  Example: `styler('coverage', data, ids)('clj:.//a/f')` is `{ color: …, tag: 'direct' }`. */
export function styler(kind: OverlayKind, data: OverlayData, visible: string[]): (id: string) => NodeStyle | null {
  if (kind === 'none') return () => null;
  if (kind === 'coverage') {
    const shares = coverageShares(data.coverage);
    return (id) => {
      const status = data.coverage.get(id);
      if (status) {
        const risky = status === 'untested' && data.riskyUntested.has(id);
        return { color: COVERAGE_COLORS[status], tag: risky ? 'risky-untested' : status, ...(risky ? { mark: '⚠ ' } : {}) };
      }
      const share = shares.get(id);
      if (share === undefined) return null;
      return share === 0 ? { color: COVERAGE_COLORS.untested, tag: 'untested' } : { color: share >= 0.5 ? COVERAGE_COLORS.indirect : COVERAGE_COLORS.weak, tag: 'partly-tested' };
    };
  }
  const own = kind === 'risk' ? data.risk : data.churn;
  const rolled = rollUp(own, kind === 'risk' ? 'max' : 'sum');
  const value = (id: string) => own.get(id) ?? rolled.get(id);
  const max = Math.max(0, ...visible.map((id) => value(id) ?? 0));
  return (id) => {
    const v = value(id);
    if (v === undefined) return null;
    const t = max > 0 ? v / max : 0;
    return { color: heat(t), size: 4 + 10 * t, tag: t >= 0.5 ? `${kind}-high` : `${kind}-low` };
  };
}

/** Each namespace's tested share (direct or indirect symbols / all).
 *  Example: one direct and one untested symbol give 0.5. */
function coverageShares(coverage: Map<string, CoverageStatus>): Map<string, number> {
  const tested = new Map<string, number>();
  const all = new Map<string, number>();
  for (const [id, s] of coverage) {
    const ns = namespaceOf(id);
    if (!ns) continue;
    all.set(ns, (all.get(ns) ?? 0) + 1);
    if (s === 'direct' || s === 'indirect') tested.set(ns, (tested.get(ns) ?? 0) + 1);
  }
  return new Map([...all].map(([ns, n]) => [ns, (tested.get(ns) ?? 0) / n]));
}

/** Measures `metrics` can bind node size or colour to. */
export const METRICS = [
  'fan_in',
  'fan_out',
  'pagerank',
  'pagerank_reverse',
  'hub',
  'authority',
  'core',
  'betweenness',
  'closeness',
  'eigenvector',
  'dominates',
  'god_score',
] as const;
export type Metric = (typeof METRICS)[number];

/** One measure's value per node ID from a metrics report.
 *  Example: `metricValues(report, 'pagerank').get(id)`. */
export function metricValues(report: MetricsReport, metric: Metric): Map<string, number> {
  const out = new Map<string, number>();
  for (const n of report.first.nodes) {
    const v = (n as unknown as Record<string, unknown>)[metric];
    if (typeof v === 'number') out.set(n.id, v);
  }
  for (const n of report.second.nodes) {
    const v = (n as unknown as Record<string, unknown>)[metric];
    if (typeof v === 'number') out.set(n.id, v);
  }
  return out;
}

/** Combines an overlay with measure bindings: size and colour bound to a
 *  measure override the overlay's.
 *  Example: `bind(base, sizeValues, null, ids)` sizes nodes by `sizeValues`. */
export function bind(
  base: (id: string) => NodeStyle | null,
  size: Map<string, number> | null,
  color: Map<string, number> | null,
  visible: string[],
): (id: string) => NodeStyle | null {
  const maxOf = (m: Map<string, number> | null) => (m ? Math.max(0, ...visible.map((id) => m.get(id) ?? 0)) : 0);
  const sMax = maxOf(size);
  const cMax = maxOf(color);
  return (id) => {
    let st = base(id);
    const sv = size?.get(id);
    const cv = color?.get(id);
    if (sv !== undefined) st = { ...st, size: 3 + 15 * (sMax > 0 ? sv / sMax : 0) };
    if (cv !== undefined) st = { ...st, color: heat(cMax > 0 ? cv / cMax : 0) };
    return st;
  };
}

/** One node's measures, in `METRICS` order, from both parts of a report.
 *  Example: `nodeMeasures(report, id)` is `[{ name: 'fan_in', value: 3 }, …]`. */
export function nodeMeasures(report: MetricsReport, id: string): { name: string; value: number }[] {
  const rows = [...report.first.nodes, ...report.second.nodes].filter((n) => n.id === id) as unknown as Record<string, unknown>[];
  return METRICS.flatMap((m) => {
    const v = rows.map((r) => r[m]).find((x) => typeof x === 'number');
    return typeof v === 'number' ? [{ name: m, value: v }] : [];
  });
}
