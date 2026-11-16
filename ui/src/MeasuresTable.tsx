// The measures table: every node of a level with its measures, sortable,
// rendering only the rows in view so it stays fast with many thousands.
// Clicking rows selects them; the selection is highlighted on the canvas.

import { useEffect, useMemo, useState } from 'react';
import type { Api } from './api';
import type { MetricsReport } from './types';

const ROW = 22;
const HEIGHT = 330;
const COLUMNS = ['fan_in', 'fan_out', 'pagerank', 'betweenness', 'god_score'] as const;
type Column = (typeof COLUMNS)[number];

/** One table row. */
export interface MeasureRow {
  id: string;
  values: Record<Column, number>;
}

/** Rows from a metrics report, sorted descending by `sort`, then by ID.
 *  Example: `measureRows(report, 'pagerank')[0]` has the highest PageRank. */
export function measureRows(r: MetricsReport, sort: Column): MeasureRow[] {
  const second = new Map(r.second.nodes.map((n) => [n.id, n]));
  const rows = r.first.nodes.map((f) => {
    const s = second.get(f.id);
    return {
      id: f.id,
      values: { fan_in: f.fan_in, fan_out: f.fan_out, pagerank: f.pagerank, betweenness: s?.betweenness ?? 0, god_score: s?.god_score ?? 0 },
    };
  });
  return rows.sort((a, b) => b.values[sort] - a.values[sort] || a.id.localeCompare(b.id));
}

/** The rows of `total` that are in view at `scrollTop`, with one spare row each side.
 *  Example: `windowOf(1000, 0)` is `{ start: 0, end: 17 }`. */
export function windowOf(total: number, scrollTop: number): { start: number; end: number } {
  const start = Math.max(0, Math.floor(scrollTop / ROW) - 1);
  const end = Math.min(total, start + Math.ceil(HEIGHT / ROW) + 2);
  return { start, end };
}

/** A short label for a node ID (the part after `//`).
 *  Example: `short('clj:.//shop.core/checkout')` is `'shop.core/checkout'`. */
function short(id: string): string {
  const i = id.indexOf('//');
  return i >= 0 ? id.slice(i + 2) : id;
}

interface Props {
  api: Api;
  selected: Set<string>;
  onSelected: (ids: Set<string>) => void;
  onFollow: (id: string) => void;
  onError: (e: unknown) => void;
  /** Load only once the panel is open: measures are slow on big graphs. */
  active: boolean;
}

/** The measures table panel.
 *  Example: `<MeasuresTable api={api} selected={sel} onSelected={setSel} … />`. */
export function MeasuresTable({ api, selected, onSelected, onFollow, onError, active }: Props) {
  const [level, setLevel] = useState<'symbol' | 'namespace' | 'unit'>('symbol');
  const [byLevel, setByLevel] = useState<Partial<Record<typeof level, MetricsReport>>>({});
  const report = byLevel[level] ?? null;
  const [sort, setSort] = useState<Column>('pagerank');
  const [scroll, setScroll] = useState(0);
  const loaded = report !== null;
  useEffect(() => {
    if (loaded || !active) return;
    api
      .metrics(level)
      .then((r) => setByLevel((m) => ({ ...m, [level]: r })))
      .catch(onError);
  }, [api, level, loaded, active, onError]);
  const rows = useMemo(() => (report ? measureRows(report, sort) : []), [report, sort]);
  const { start, end } = windowOf(rows.length, scroll);
  const toggle = (id: string, add: boolean) => {
    const next = new Set(add ? selected : []);
    if (next.has(id) && add) next.delete(id);
    else next.add(id);
    onSelected(next);
  };
  return (
    <div className="measures" data-testid="measures">
      <div className="measures-head">
        <select data-testid="measures-level" value={level} onChange={(e) => setLevel(e.target.value as typeof level)}>
          <option value="symbol">functions</option>
          <option value="namespace">namespaces</option>
          <option value="unit">units</option>
        </select>
        <span className="muted">
          {report ? `${rows.length} rows` : 'computing…'}
          {report?.second.sampling.approximate ? ' · betweenness estimated' : ''}
        </span>
        {selected.size > 0 && (
          <button type="button" className="small" onClick={() => onSelected(new Set())}>
            clear {selected.size}
          </button>
        )}
      </div>
      <div className="measures-cols">
        <span className="name">node</span>
        {COLUMNS.map((c) => (
          <button key={c} type="button" data-sort={c} aria-pressed={sort === c} onClick={() => setSort(c)} title={`Sort by ${c.replace('_', ' ')}`}>
            {c.replace('_', ' ')}
          </button>
        ))}
      </div>
      <div className="measures-body" style={{ height: HEIGHT }} onScroll={(e) => setScroll(e.currentTarget.scrollTop)} data-testid="measures-body">
        <div style={{ height: rows.length * ROW, position: 'relative' }}>
          {rows.slice(start, end).map((r, i) => (
            <div
              key={r.id}
              className={`measures-row${selected.has(r.id) ? ' selected' : ''}`}
              style={{ top: (start + i) * ROW, height: ROW }}
              data-row-id={r.id}
              onClick={(e) => toggle(r.id, e.ctrlKey || e.metaKey || e.shiftKey)}
              onDoubleClick={() => onFollow(r.id)}
              title={`${r.id}\nClick selects (Ctrl/Shift adds); double-click shows it on the canvas.`}
            >
              <span className="name">{short(r.id)}</span>
              {COLUMNS.map((c) => (
                <span key={c}>{Number.isInteger(r.values[c]) ? r.values[c] : r.values[c].toFixed(4)}</span>
              ))}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
