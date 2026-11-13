// The crater view: a seed's (or a stack trace's) impact subgraph laid out
// in layers, with direction, depth and certainty controls, a "why is this
// here" path on hover, and a checklist tab with CSV and Markdown export.
// Repository strings are rendered as React text or canvas labels only.

import { useCallback, useEffect, useMemo, useState } from 'react';
import type { Api, CraterQuery } from './api';
import { checklistCsv, checklistMarkdown, checklistRows, craterCanvas, whyPath, type CraterGraph } from './crater';
import { elkLayout } from './elkLayout';
import { GraphView, type Highlight } from './GraphView';
import type { CraterResult } from './types';

/** What a crater is seeded from. */
export type CraterRequest = { kind: 'seeds'; ids: string[]; label: string; direction: CraterQuery['direction'] } | { kind: 'trace'; text: string; label: string };

interface Props {
  api: Api;
  request: CraterRequest;
  onClose: () => void;
  onSelect: (id: string) => void;
}

type Status = { kind: 'loading' } | { kind: 'layout'; nodes: number } | { kind: 'ready' } | { kind: 'error'; message: string };

/** Starts a browser download of `text` as `name`.
 *  Example: `download('checklist.csv', 'text/csv', csv)`. */
function download(name: string, type: string, text: string): void {
  const url = URL.createObjectURL(new Blob([text], { type }));
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}

/** The crater view.
 *  Example: `<CraterView api={api} request={{kind: 'seeds', ids: [id], label, direction: 'forward'}} … />`. */
export function CraterView({ api, request, onClose, onSelect }: Props) {
  const [direction, setDirection] = useState<CraterQuery['direction']>(request.kind === 'seeds' ? request.direction : 'reverse');
  const [depth, setDepth] = useState(6);
  const [unlimited, setUnlimited] = useState(false);
  const [minCertainty, setMinCertainty] = useState<CraterQuery['minCertainty']>('likely');
  const [result, setResult] = useState<CraterResult | null>(null);
  const [graph, setGraph] = useState<CraterGraph | null>(null);
  // The status of the request `key` names; a newer key means loading.
  const [tracked, setTracked] = useState<{ key: string; status: Status }>({ key: '', status: { kind: 'loading' } });
  const [tab, setTab] = useState<'graph' | 'checklist'>('graph');
  const [hovered, setHovered] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);

  const key = `${direction}|${unlimited ? 0 : depth}|${minCertainty}`;
  const status: Status = tracked.key === key ? tracked.status : { kind: 'loading' };

  useEffect(() => {
    let live = true;
    const q = { direction, depth: unlimited ? 0 : depth, minCertainty };
    const setStatus = (s: Status) => setTracked({ key, status: s });
    const fetched = request.kind === 'seeds' ? api.crater({ ...q, ids: request.ids }) : api.craterTrace(request.text, q);
    fetched
      .then(async (r) => {
        const g = craterCanvas(r);
        if (!live) return;
        setStatus({ kind: 'layout', nodes: g.order });
        await elkLayout(g, r.direction === 'reverse' ? 'LEFT' : 'RIGHT');
        if (!live) return;
        setResult(r);
        setGraph(g);
        setStatus({ kind: 'ready' });
      })
      .catch((e: unknown) => {
        if (live) setStatus({ kind: 'error', message: e instanceof Error ? e.message : String(e) });
      });
    return () => {
      live = false;
    };
  }, [api, request, direction, depth, unlimited, minCertainty, key]);

  const highlight: Highlight | null = useMemo(() => {
    if (!result || !graph) return null;
    if (hovered) {
      const why = whyPath(result, graph, hovered);
      return { nodes: new Set(why.nodes), edges: new Set(why.edges) };
    }
    if (result.pinned && result.pinned.path.length > 0) {
      const path = new Set(result.pinned.path);
      const edges = graph.filterEdges((_k, a) => a.edge.reason === 'trace-observed');
      return { nodes: path, edges: new Set(edges) };
    }
    return null;
  }, [result, graph, hovered]);

  const rows = useMemo(() => (result ? checklistRows(result) : []), [result]);
  const select = useCallback(
    (id: string) => {
      setSelected(id);
      onSelect(id);
    },
    [onSelect],
  );
  const pinnedCount = graph ? graph.filterNodes((_id, a) => a.role === 'pinned').length : 0;
  const busy = status.kind === 'loading' || status.kind === 'layout';

  return (
    <div className="crater-view" data-testid="crater-view" data-direction={direction} data-nodes={graph?.order ?? 0} data-pinned={pinnedCount}>
      <div className="crater-bar">
        <button type="button" data-testid="crater-close" onClick={onClose} title="Back to the graph">
          ◀ Back
        </button>
        <strong title={request.label}>Crater of {request.label}</strong>
        <label>
          Direction{' '}
          <select data-testid="crater-direction" value={direction} onChange={(e) => setDirection(e.target.value as CraterQuery['direction'])}>
            <option value="forward">forward (what it reaches)</option>
            <option value="reverse">reverse (what reaches it)</option>
            <option value="both">both</option>
          </select>
        </label>
        <label title="Steps from the seed">
          Depth{' '}
          <input type="range" min={1} max={12} value={depth} disabled={unlimited} data-testid="crater-depth" onChange={(e) => setDepth(Number(e.target.value))} />{' '}
          {unlimited ? '∞' : depth}
        </label>
        <label>
          <input type="checkbox" checked={unlimited} data-testid="crater-unlimited" onChange={(e) => setUnlimited(e.target.checked)} /> unlimited
        </label>
        <label title="Leave out nodes whose best path is weaker">
          At least{' '}
          <select data-testid="crater-certainty" value={minCertainty} onChange={(e) => setMinCertainty(e.target.value as CraterQuery['minCertainty'])}>
            <option value="certain">certain</option>
            <option value="likely">likely</option>
            <option value="possible">possible</option>
          </select>
        </label>
        <span className="crater-tabs" role="tablist">
          <button type="button" role="tab" aria-selected={tab === 'graph'} data-testid="crater-tab-graph" onClick={() => setTab('graph')}>
            Graph
          </button>
          <button type="button" role="tab" aria-selected={tab === 'checklist'} data-testid="crater-tab-checklist" onClick={() => setTab('checklist')}>
            Checklist ({rows.length})
          </button>
        </span>
      </div>
      <div className="crater-status" data-testid="crater-status" aria-live="polite">
        {status.kind === 'loading' && 'Computing the crater…'}
        {status.kind === 'layout' && (
          <span data-testid="layout-status">
            <span className="spinner" aria-hidden="true" /> Laying out {status.nodes} nodes…
          </span>
        )}
        {status.kind === 'error' && (
          <span className="error" role="alert">
            {status.message}
          </span>
        )}
        {status.kind === 'ready' && result && <Summary r={result} />}
      </div>
      {result?.pinned && <PinnedPanel r={result} onSelect={select} />}
      <div className={`crater-body${busy ? ' busy' : ''}`}>
        {tab === 'graph' && graph && (
          <GraphView
            graph={graph}
            version={0}
            selected={selected}
            isHidden={() => false}
            isEdgeHidden={() => false}
            visibleEdges={graph.size}
            onSelect={select}
            onDoubleClick={select}
            onDeselect={() => setSelected(null)}
            onHover={setHovered}
            highlight={highlight}
            testId="crater-canvas"
          />
        )}
        {tab === 'graph' && hovered && result && <WhyLine r={result} id={hovered} />}
        {tab === 'checklist' && (
          <div className="checklist" data-testid="checklist">
            <div className="tools">
              <button type="button" data-testid="export-csv" onClick={() => download('crater-checklist.csv', 'text/csv', checklistCsv(rows))}>
                Export CSV
              </button>
              <button type="button" data-testid="export-md" onClick={() => download('crater-checklist.md', 'text/markdown', checklistMarkdown(rows))}>
                Export Markdown
              </button>
            </div>
            <table>
              <thead>
                <tr>
                  <th>Location</th>
                  <th>Symbol</th>
                  <th>Distance</th>
                  <th>Certainty</th>
                  <th>Entry point</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((r) => (
                  <tr key={r.id} data-node-id={r.id}>
                    <td className="mono">{r.loc ? `${r.loc.file}:${r.loc.line}:${r.loc.col}` : ''}</td>
                    <td>
                      <button type="button" onClick={() => select(r.id)}>
                        {r.name}
                      </button>
                    </td>
                    <td>{r.distance}</td>
                    <td className={`cert-${r.path_certainty}`}>{r.path_certainty}</td>
                    <td>{r.entry_point ? 'yes' : ''}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
}

/** Node counts, what `possible` would add, boundary and hubs.
 *  Example: `12 nodes · 3 more at possible · 4 boundary`. */
function Summary({ r }: { r: CraterResult }) {
  return (
    <span data-testid="crater-summary">
      {r.reach.length} nodes
      {r.hidden_below_min > 0 && ` · ${r.hidden_below_min} more at possible`}
      {r.boundary.length > 0 && (
        <span title="Externals, platform calls and holes: where the analysis cannot see further">
          {' '}
          · <span className="boundary-mark">⇥</span> {r.boundary.length} boundary
        </span>
      )}
      {r.hubs.length > 0 && ` · ${r.hubs.length} hubs not expanded`}
      {r.truncated && ' · truncated (raise the depth)'}
    </span>
  );
}

/** "Why is this here": the hovered node's path back to the seed.
 *  Example: `checkout ← price@:book ← half-off`. */
function WhyLine({ r, id }: { r: CraterResult; id: string }) {
  const via = new Map(r.reach.map((x) => [x.id, x]));
  const names = new Map(r.graph.nodes.map((n) => [n.id, n.name]));
  const steps: string[] = [];
  let cur: string | null | undefined = id;
  while (cur && !steps.includes(cur)) {
    steps.push(cur);
    cur = via.get(cur)?.via;
  }
  const x = via.get(id);
  const arrow = r.direction === 'reverse' ? ' → ' : ' ← ';
  return (
    <p className="why" data-testid="why">
      Why here: {steps.map((s) => names.get(s) ?? s).join(arrow)}
      {x && ` (${x.path_certainty}, ${x.distance} steps)`}
    </p>
  );
}

/** The trace's path, the calls the graph lacks, and unmapped frames.
 *  Example: lists `checkout`, `price@:book`, … with "not in the graph" marks. */
function PinnedPanel({ r, onSelect }: { r: CraterResult; onSelect: (id: string) => void }) {
  const p = r.pinned;
  if (!p) return null;
  const names = new Map(r.graph.nodes.map((n) => [n.id, n.name]));
  const missing = new Set(p.missing.map((m) => `${m.from} ${m.to}`));
  return (
    <div className="pinned" data-testid="pinned">
      <span className="muted">Trace path:</span>
      <ol data-testid="pinned-path">
        {p.path.map((id, i) => (
          <li key={id} data-node-id={id}>
            <button type="button" onClick={() => onSelect(id)} title={id}>
              {names.get(id) ?? id}
            </button>
            {i + 1 < p.path.length && missing.has(`${id} ${p.path[i + 1]}`) && (
              <span className="missing" title="The static graph has no edge for this call">
                {' '}
                ⚠ not in the graph
              </span>
            )}
          </li>
        ))}
      </ol>
      {p.unmapped.length > 0 && <span className="muted"> {p.unmapped.length} frames map to no node</span>}
    </div>
  );
}
