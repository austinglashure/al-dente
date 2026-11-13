// The hotspot findings and the cycle list (left panel). Repository strings
// are rendered as React text.

import { useEffect, useState } from 'react';
import type { Api } from './api';
import type { Component, HotspotsReport } from './types';

/** A short label for a node ID (the part after `//`).
 *  Example: `short('clj:.//shop.core/checkout')` is `'shop.core/checkout'`. */
function short(id: string): string {
  const i = id.indexOf('//');
  return i >= 0 ? id.slice(i + 2) : id;
}

/** Ranked findings; clicking one shows its node on the canvas.
 *  Example: `<HotspotList report={r} onFollow={follow} />`. */
export function HotspotList({ report, onFollow }: { report: HotspotsReport | null; onFollow: (id: string) => void }) {
  return (
    <details className="hotspots" data-testid="hotspots">
      <summary>Hotspots ({report ? report.findings.length : '…'})</summary>
      {report === null ? (
        <p className="muted small-print">Computing…</p>
      ) : (
        <ol>
          {report.findings.map((f, i) => {
            const node = f.nodes[0];
            return (
              <li key={`${f.kind}-${f.subject}-${i}`} title={f.reason}>
                <span className={`badge badge-${f.kind}`}>{f.kind.replace(/-/g, ' ')}</span>{' '}
                {node ? (
                  <button type="button" data-hotspot={node} onClick={() => onFollow(node)}>
                    {f.nodes.length > 1 ? `${f.nodes.length} nodes from ${short(node)}` : short(node)}
                  </button>
                ) : (
                  <span>{f.subject}</span>
                )}
                <div className="muted small-print">{f.reason}</div>
              </li>
            );
          })}
        </ol>
      )}
    </details>
  );
}

/** Cycles at a level, each with "show this cycle".
 *  Example: `<CycleList api={api} onShow={(c) => …} onError={fail} />`. */
export function CycleList({ api, onShow, onError }: { api: Api; onShow: (c: Component) => void; onError: (e: unknown) => void }) {
  const [level, setLevel] = useState<'namespace' | 'unit' | 'symbol'>('namespace');
  const [open, setOpen] = useState(false);
  const [byLevel, setByLevel] = useState<Partial<Record<typeof level, Component[]>>>({});
  const cycles = byLevel[level] ?? null;
  const loaded = cycles !== null;
  useEffect(() => {
    if (!open || loaded) return;
    api
      .cycles(level)
      .then((r) => setByLevel((m) => ({ ...m, [level]: r.components })))
      .catch(onError);
  }, [api, level, open, loaded, onError]);
  return (
    <details className="cycles" data-testid="cycles" onToggle={(e) => setOpen(e.currentTarget.open)}>
      <summary>Cycles{cycles ? ` (${cycles.length})` : ''}</summary>
      <label className="small-print">
        Level{' '}
        <select data-testid="cycles-level" value={level} onChange={(e) => setLevel(e.target.value as typeof level)}>
          <option value="namespace">namespaces (requires)</option>
          <option value="unit">units (depends)</option>
          <option value="symbol">functions (calls)</option>
        </select>
      </label>
      {cycles?.length === 0 && <p className="muted small-print">No cycles at this level.</p>}
      <ul>
        {cycles?.map((c, i) => (
          <li key={c.nodes.join(',')}>
            {c.size} nodes: {c.nodes.slice(0, 3).map(short).join(', ')}
            {c.size > 3 ? ', …' : ''}{' '}
            <button type="button" className="small" data-show-cycle={i} onClick={() => onShow(c)} title={`Cut suggestion: ${c.cut.map((e) => `${short(e.from)} → ${short(e.to)}`).join('; ')}`}>
              show this cycle
            </button>
          </li>
        ))}
      </ul>
    </details>
  );
}
