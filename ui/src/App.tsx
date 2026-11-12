// The GUI: a graph canvas with a node list, search, toggles, a details panel
// and a source panel. Every repository string is rendered as React text.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Api, initToken } from './api';
import { GraphView } from './GraphView';
import { layout } from './layout';
import { dropNodes, hidden, isAggregate, mergePayload, newGraph } from './model';
import type { GNode, Neighbor, ShowResult } from './types';

/** The location of a node as `file:line:col`.
 *  Example: `where(node)` is `'src/shop/core.clj:12:1'`. */
function where(n: GNode): string {
  return n.loc ? `${n.loc.file}:${n.loc.line}:${n.loc.col}` : '';
}

/** The app. */
export function App() {
  const api = useMemo(() => new Api(initToken()), []);
  const graph = useMemo(() => newGraph(), []);
  const [version, setVersion] = useState(0);
  const [selected, setSelected] = useState<string | null>(null);
  const [details, setDetails] = useState<ShowResult | null>(null);
  const [source, setSource] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [hideTests, setHideTests] = useState(false);
  const [hideExternals, setHideExternals] = useState(true);
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<GNode[]>([]);
  const expanded = useRef(new Map<string, string[]>());
  const [expandedIds, setExpandedIds] = useState<ReadonlySet<string>>(new Set());

  const redraw = useCallback(() => setVersion((v) => v + 1), []);
  const relayout = useCallback(() => layout(graph, redraw), [graph, redraw]);

  const fail = useCallback((e: unknown) => setError(e instanceof Error ? e.message : String(e)), []);

  useEffect(() => {
    api
      .graph()
      .then((p) => {
        mergePayload(graph, p);
        if (p.truncated) setNotice('The graph was too large and has been truncated.');
        else if (p.aggregated) setNotice(`Showing the ${p.aggregated.level} level to stay within the size limit.`);
        relayout();
        redraw();
      })
      .catch(fail);
  }, [api, graph, relayout, redraw, fail]);

  const select = useCallback(
    (id: string) => {
      setSelected(id);
      setDetails(null);
      setSource(null);
      api.node(id).then(setDetails).catch(fail);
      api
        .source(id)
        .then(setSource)
        .catch(() => setSource(''));
    },
    [api, fail],
  );

  const toggleExpand = useCallback(
    (id: string) => {
      const children = expanded.current.get(id);
      if (children) {
        dropNodes(graph, children);
        expanded.current.delete(id);
        setExpandedIds(new Set(expanded.current.keys()));
        redraw();
        return;
      }
      api
        .expand(id, graph.nodes())
        .then((p) => {
          const added = mergePayload(graph, p, id);
          expanded.current.set(id, added);
          setExpandedIds(new Set(expanded.current.keys()));
          if (p.truncated) setNotice('Only the best-connected children are shown.');
          relayout();
        })
        .catch(fail);
    },
    [api, graph, redraw, relayout, fail],
  );

  const neighbors = useCallback(
    (id: string) =>
      api
        .neighborhood(id)
        .then((p) => {
          mergePayload(graph, p, graph.hasNode(id) ? id : undefined);
          relayout();
        })
        .catch(fail),
    [api, graph, relayout, fail],
  );

  const follow = useCallback(
    (id: string) => {
      const go = graph.hasNode(id) ? Promise.resolve() : neighbors(id);
      void go.then(() => select(id));
    },
    [graph, neighbors, select],
  );

  const onDoubleClick = useCallback(
    (id: string) => {
      const n = graph.hasNode(id) ? graph.getNodeAttribute(id, 'node') : null;
      if (n && isAggregate(n)) toggleExpand(id);
      else void neighbors(id);
    },
    [graph, toggleExpand, neighbors],
  );

  const isHidden = useCallback(
    (id: string) => graph.hasNode(id) && hidden(graph.getNodeAttribute(id, 'node'), { hideTests, hideExternals }),
    [graph, hideTests, hideExternals],
  );

  const visible = graph
    .mapNodes((_id, a) => a.node)
    .filter((n) => !hidden(n, { hideTests, hideExternals }))
    .sort((a, b) => a.id.localeCompare(b.id));

  const runSearch = (e: React.FormEvent) => {
    e.preventDefault();
    if (!query.trim()) return;
    api
      .search(query)
      .then((r) => setResults(r.matches))
      .catch(fail);
  };

  return (
    <div className="app">
      <aside className="left">
        <form onSubmit={runSearch} className="search">
          <input data-testid="search" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search symbols…" aria-label="Search" />
        </form>
        {results.length > 0 && (
          <ul className="results" data-testid="results">
            {results.map((n) => (
              <li key={n.id}>
                <button type="button" data-node-id={n.id} onClick={() => follow(n.id)}>
                  {n.name} <span className="muted">{n.kind}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
        <label>
          <input type="checkbox" data-testid="hide-tests" checked={hideTests} onChange={(e) => {
              setHideTests(e.target.checked);
              redraw();
            }} /> Hide tests
        </label>
        <label>
          <input type="checkbox" data-testid="hide-externals" checked={hideExternals} onChange={(e) => {
              setHideExternals(e.target.checked);
              redraw();
            }} /> Hide externals
        </label>
        {notice && (
          <p className="notice" data-testid="notice">
            {notice}
          </p>
        )}
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
        <h2>On the canvas ({visible.length})</h2>
        <ul className="nodes" data-testid="node-list">
          {visible.map((n) => (
            <li key={n.id} className={n.id === selected ? 'selected' : ''}>
              <button type="button" className="name" data-node-id={n.id} onClick={() => select(n.id)} title={n.id}>
                {n.name}
              </button>
              {isAggregate(n) ? (
                <button type="button" className="small" data-expand={n.id} onClick={() => toggleExpand(n.id)}>
                  {expandedIds.has(n.id) ? 'collapse' : 'expand'}
                </button>
              ) : (
                <button type="button" className="small" data-neighbors={n.id} onClick={() => void neighbors(n.id)}>
                  neighbors
                </button>
              )}
            </li>
          ))}
        </ul>
      </aside>
      <main className="center">
        <GraphView graph={graph} version={version} selected={selected} isHidden={isHidden} onSelect={select} onDoubleClick={onDoubleClick} />
      </main>
      <aside className="right">
        {details ? <Details d={details} onFollow={follow} /> : <p className="muted">Select a node.</p>}
        {source !== null && (
          <section>
            <h2>Source</h2>
            {source ? (
              <pre data-testid="source">{source}</pre>
            ) : (
              <p className="muted" data-testid="source">
                No source for this node.
              </p>
            )}
          </section>
        )}
      </aside>
    </div>
  );
}

/** The details panel: kind, location, signature, doc, callers and callees.
 *  Example: `<Details d={show} onFollow={follow} />`. */
function Details({ d, onFollow }: { d: ShowResult; onFollow: (id: string) => void }) {
  const n = d.node;
  return (
    <section data-testid="details">
      <h2 title={n.id}>{n.name}</h2>
      <p className="muted">
        {n.kind}
        {n.subkind ? ` / ${n.subkind}` : ''}
        {(n.flags ?? []).length > 0 ? ` · ${(n.flags ?? []).join(', ')}` : ''}
      </p>
      <p className="id">{n.id}</p>
      {n.loc && <p data-testid="location">{where(n)}</p>}
      {n.signature && <p className="mono">{n.signature}</p>}
      {n.doc && (
        <p className="doc" data-testid="doc">
          {n.doc}
        </p>
      )}
      <NeighborList title="Callers" list={d.callers} testid="caller" onFollow={onFollow} />
      <NeighborList title="Callees" list={d.callees} testid="callee" onFollow={onFollow} />
    </section>
  );
}

/** A list of neighbor links, strongest certainty first.
 *  Example: `<NeighborList title="Callees" list={d.callees} … />`. */
function NeighborList({ title, list, testid, onFollow }: { title: string; list: Neighbor[]; testid: string; onFollow: (id: string) => void }) {
  return (
    <>
      <h3>
        {title} ({list.length})
      </h3>
      <ul className="neighbors">
        {list.map((x) => (
          <li key={`${x.id}|${x.kind}|${x.reason ?? ''}`}>
            <button type="button" data-testid={testid} data-node-id={x.id} onClick={() => onFollow(x.id)} title={x.id}>
              {x.name}
            </button>{' '}
            <span className={`muted cert-${x.certainty ?? 'none'}`}>
              {x.kind} · {x.certainty ?? '–'} · {x.reason ?? '–'}
            </span>
          </li>
        ))}
      </ul>
    </>
  );
}
