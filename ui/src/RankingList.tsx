// The ranking list in the left panel: every namespace (or unit, or symbol)
// ordered by one measure. Repository strings are rendered as React text.

import { useEffect, useState } from 'react';
import type { Api } from './api';
import { measuresFor, rank, type RankingReport, type RankLevel } from './rankings';

const PAGE = 20;

interface Props {
  api: Api;
  /** IDs on the canvas now, for the "on canvas" scope. */
  onCanvas: Set<string>;
  onFollow: (id: string) => void;
  onError: (e: unknown) => void;
  /** False until the first graph is drawn: measures are slow on big
   *  repositories and must not delay the first view. */
  ready: boolean;
}

/** A value as shown in the list: whole numbers as they are, others to three places.
 *  Example: `shown(12)` is `'12'`, `shown(0.04567)` is `'0.046'`. */
function shown(v: number): string {
  return Number.isInteger(v) ? String(v) : v.toFixed(3);
}

/** Rank by a measure; a row brings its node onto the canvas and selects it.
 *  Example: namespaces by callers lists the most-called namespace first. */
export function RankingList({ api, onCanvas, onFollow, onError, ready }: Props) {
  const [level, setLevel] = useState<RankLevel>('namespace');
  const [key, setKey] = useState('callers');
  const [order, setOrder] = useState<'asc' | 'desc'>('desc');
  const [scope, setScope] = useState<'repo' | 'canvas'>('repo');
  const [shownCount, setShownCount] = useState(PAGE);
  const [reports, setReports] = useState<Partial<Record<RankLevel, RankingReport>>>({});
  const report = reports[level];

  useEffect(() => {
    if (!ready || reports[level]) return;
    api
      .rankings(level)
      .then((r) => setReports((m) => ({ ...m, [level]: r })))
      .catch(onError);
  }, [api, level, reports, onError, ready]);

  const measures = measuresFor(level);
  const measure = measures.find((m) => m.key === key) ?? measures[0];
  const by = measure?.key ?? '';
  const ranked = report ? rank(report.rows, by, order, scope === 'canvas' ? onCanvas : undefined) : [];

  const groups = [...new Set(measures.map((m) => m.group))];
  return (
    <section className="rankings" data-testid="rankings">
      <h3>Rank</h3>
      <div className="rank-controls">
        <select data-testid="rank-level" value={level} onChange={(e) => (setLevel(e.target.value as RankLevel), setShownCount(PAGE))} aria-label="Level">
          <option value="namespace">namespaces</option>
          <option value="unit">units</option>
          <option value="symbol">symbols</option>
        </select>{' '}
        by{' '}
        <select data-testid="rank-by" value={measure?.key} onChange={(e) => (setKey(e.target.value), setShownCount(PAGE))} aria-label="Measure">
          {groups.map((g) => (
            <optgroup key={g} label={g}>
              {measures
                .filter((m) => m.group === g)
                .map((m) => (
                  <option key={m.key} value={m.key}>
                    {m.label}
                  </option>
                ))}
            </optgroup>
          ))}
        </select>{' '}
        <select data-testid="rank-scope" value={scope} onChange={(e) => setScope(e.target.value as 'repo' | 'canvas')} aria-label="Scope">
          <option value="repo">whole repo</option>
          <option value="canvas">on canvas</option>
        </select>
      </div>
      {measure && (
        <p className="muted" data-testid="rank-about">
          {measure.about}
        </p>
      )}
      {!report ? (
        <p className="muted">Computing…</p>
      ) : (
        <table className="rank-table">
          <thead>
            <tr>
              <th>#</th>
              <th>name</th>
              <th>
                <button type="button" className="section-toggle" data-testid="rank-order" onClick={() => setOrder((o) => (o === 'desc' ? 'asc' : 'desc'))} title="Highest or lowest first">
                  {measure?.label} {order === 'desc' ? '▼' : '▲'}
                </button>
              </th>
            </tr>
          </thead>
          <tbody>
            {ranked.slice(0, shownCount).map((r, i) => (
              <tr key={r.id}>
                <td className="muted">{i + 1}</td>
                <td>
                  <button type="button" data-testid="rank-row" data-node-id={r.id} onClick={() => onFollow(r.id)} title={r.id}>
                    {r.name}
                  </button>
                </td>
                <td className="rank-value">{shown(r.values[measure?.key ?? ''] ?? 0)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {ranked.length > shownCount && (
        <button type="button" className="small" data-testid="rank-more" onClick={() => setShownCount((n) => n + PAGE)}>
          Show more ({ranked.length - shownCount} left)
        </button>
      )}
    </section>
  );
}
