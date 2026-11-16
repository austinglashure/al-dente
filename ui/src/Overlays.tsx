// Overlay state (which overlay, measure bindings) and its controls. The
// reports load once per session; risk is the default picture.

import { useEffect, useMemo, useState } from 'react';
import type { Api } from './api';
import { METRICS, bind, metricValues, overlayData, styler, type Metric, type NodeStyle, type OverlayKind } from './overlay';
import type { ChurnReport, CoverageReport, HotspotsReport, MetricsReport } from './types';

/** Reports the overlays and the hotspot views share. */
export interface Reports {
  coverage: CoverageReport | null;
  hotspots: HotspotsReport | null;
  churn: ChurnReport | null;
  /** Merged metrics of every level loaded so far. */
  metrics: MetricsReport | null;
}

/** Loads the reports an overlay needs, lazily (churn and metrics only when
 *  asked for), and nothing until `enabled` (after the first canvas, so slow
 *  reports never hold up the first view). Errors go to `onError`.
 *  Example: `const reports = useReports(api, overlay, needMetrics, fail, shown)`. */
export function useReports(api: Api, overlay: OverlayKind, needMetrics: boolean, onError: (e: unknown) => void, enabled: boolean): Reports {
  const [coverage, setCoverage] = useState<CoverageReport | null>(null);
  const [hotspots, setHotspots] = useState<HotspotsReport | null>(null);
  const [churn, setChurn] = useState<ChurnReport | null>(null);
  const [metrics, setMetrics] = useState<MetricsReport | null>(null);
  useEffect(() => {
    if (!enabled) return;
    api.hotspots().then(setHotspots).catch(onError);
    api.coverage().then(setCoverage).catch(onError);
  }, [api, onError, enabled]);
  useEffect(() => {
    if (enabled && overlay === 'churn' && churn === null) api.churn().then(setChurn).catch(onError);
  }, [api, overlay, churn, onError, enabled]);
  useEffect(() => {
    if (!enabled || !needMetrics || metrics !== null) return;
    Promise.all((['symbol', 'namespace', 'unit'] as const).map((l) => api.metrics(l)))
      .then((rs) =>
        setMetrics({
          first: { nodes: rs.flatMap((r) => r.first.nodes), components: [] },
          second: { nodes: rs.flatMap((r) => r.second.nodes), sampling: rs[0]?.second.sampling ?? { approximate: false, pivots: 0 } },
        }),
      )
      .catch(onError);
  }, [api, needMetrics, metrics, onError, enabled]);
  return { coverage, hotspots, churn, metrics };
}

/** The node styler for the current overlay and bindings over the visible nodes.
 *  Example: `useStyler(reports, 'coverage', null, null, ids)`. */
export function useStyler(
  reports: Reports,
  overlay: OverlayKind,
  sizeBy: Metric | null,
  colorBy: Metric | null,
  visible: string[],
): (id: string) => NodeStyle | null {
  const data = useMemo(() => overlayData(reports.coverage, reports.hotspots, reports.churn), [reports.coverage, reports.hotspots, reports.churn]);
  const key = visible.join('\n');
  return useMemo(() => {
    const ids = key ? key.split('\n') : [];
    const base = styler(overlay, data, ids);
    const size = sizeBy && reports.metrics ? metricValues(reports.metrics, sizeBy) : null;
    const color = colorBy && reports.metrics ? metricValues(reports.metrics, colorBy) : null;
    return bind(base, size, color, ids);
  }, [overlay, data, sizeBy, colorBy, reports.metrics, key]);
}

interface ControlsProps {
  overlay: OverlayKind;
  onOverlay: (o: OverlayKind) => void;
  sizeBy: Metric | null;
  onSizeBy: (m: Metric | null) => void;
  colorBy: Metric | null;
  onColorBy: (m: Metric | null) => void;
}

/** The overlay selector, the size and colour bindings, and a legend.
 *  Example: `<OverlayControls overlay="risk" … />`. */
export function OverlayControls(p: ControlsProps) {
  const metricSelect = (testId: string, value: Metric | null, on: (m: Metric | null) => void) => (
    <select data-testid={testId} value={value ?? ''} onChange={(e) => on(e.target.value ? (e.target.value as Metric) : null)}>
      <option value="">—</option>
      {METRICS.map((m) => (
        <option key={m} value={m}>
          {m.replace(/_/g, ' ')}
        </option>
      ))}
    </select>
  );
  return (
    <fieldset className="legend overlays" data-testid="overlays">
      <legend>Overlay</legend>
      <label title="Risk: heavily used and poorly tested code is darkest (the default picture). Coverage: estimated test coverage. Churn: how often code changed.">
        Show{' '}
        <select data-testid="overlay" value={p.overlay} onChange={(e) => p.onOverlay(e.target.value as OverlayKind)}>
          <option value="risk">risk (what should worry me)</option>
          <option value="coverage">coverage (estimated)</option>
          <option value="churn">churn (git history)</option>
          <option value="none">none (by kind)</option>
        </select>
      </label>
      {p.overlay === 'coverage' && (
        <p className="small-print overlay-legend">
          <span className="swatch-dot" style={{ background: '#006d2c' }} /> direct <span className="swatch-dot" style={{ background: '#74c476' }} /> indirect{' '}
          <span className="swatch-dot" style={{ background: '#fec44f' }} /> weak <span className="swatch-dot hollow" /> untested · ⚠ heavily used and untested
        </p>
      )}
      <label>Size by {metricSelect('size-by', p.sizeBy, p.onSizeBy)}</label>
      <label>Colour by {metricSelect('color-by', p.colorBy, p.onColorBy)}</label>
    </fieldset>
  );
}
