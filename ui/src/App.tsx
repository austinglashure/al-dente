// The GUI: a canvas with a breadcrumb of what is open, a node list, search
// and toggles, and a details panel with tools. Every repository string is
// rendered as React text.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { Api, initToken } from './api';
import { Details, type Tools } from './Details';
import { GraphView } from './GraphView';
import { layout } from './layout';
import { certaintyColor, edgeShown, hasSource, hidden, isAggregate, labelFor, mergePayload, newGraph, type Levels } from './model';
import { Splitter, usePanels } from './panels';
import type { BlindSpot, GNode, Neighbor, Payload, ShowResult, TreeDir, UnitInfo } from './types';
import { TreeView, dirChildren, findDir, topDirIds } from './tree';
import { TOP_CHOICES, loadViewPrefs, saveViewPrefs } from './viewPrefs';

/** One level of the breadcrumb: its label and how to load its canvas. */
interface Crumb {
  label: string;
  load: () => Promise<Payload>;
}

/** The app. */
export function App() {
  const api = useMemo(() => new Api(initToken()), []);
  const graph = useMemo(() => newGraph(), []);
  const [version, setVersion] = useState(0);
  const [crumbs, setCrumbs] = useState<Crumb[]>([]);
  const [units, setUnits] = useState<Map<string, UnitInfo>>(new Map());
  const [selected, setSelected] = useState<string | null>(null);
  const [details, setDetails] = useState<ShowResult | null>(null);
  const [source, setSource] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [hideTests, setHideTests] = useState(false);
  const [hideExternals, setHideExternals] = useState(true);
  const [hideInterop, setHideInterop] = useState(false);
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<GNode[]>([]);
  const [panels, setPanels] = usePanels();
  const [spots, setSpots] = useState<BlindSpot[]>([]);
  const [levels, setLevels] = useState<Levels>({ certain: true, likely: true, possible: true });
  const [initialPrefs] = useState(loadViewPrefs);
  const [view, setView] = useState<'units' | 'namespaces'>(initialPrefs.view);
  const [top, setTop] = useState(initialPrefs.top);
  useEffect(() => saveViewPrefs({ view, top }), [view, top]);
  const [namespaceCount, setNamespaceCount] = useState<number | null>(null);
  const [tree, setTree] = useState<TreeDir | null>(null);
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set());
  const collapsedRef = useRef(collapsed);
  // Read at load time, so the root views (and the effect that loads the
  // first one) do not change identity when these do.
  const topRef = useRef(top);
  const namespaceCountRef = useRef(namespaceCount);
  useLayoutEffect(() => {
    topRef.current = top;
    namespaceCountRef.current = namespaceCount;
    collapsedRef.current = collapsed;
  }, [top, namespaceCount, collapsed]);

  const redraw = useCallback(() => setVersion((v) => v + 1), []);
  // A ref, so callbacks that read the levels keep their identity and do not
  // re-trigger effects (such as the initial load) when a toggle changes.
  const levelsRef = useRef(levels);
  useLayoutEffect(() => {
    levelsRef.current = levels;
  }, [levels]);
  const edgeVisible = useCallback(
    (key: string) => graph.hasEdge(key) && edgeShown(graph.getEdgeAttribute(key, 'edge'), levelsRef.current),
    [graph, levelsRef],
  );
  const relayout = useCallback(() => layout(graph, redraw, edgeVisible), [graph, redraw, edgeVisible]);
  const fail = useCallback((e: unknown) => setError(e instanceof Error ? e.message : String(e)), []);

  /** The two root views: units (the default) and every namespace at once. */
  const roots = useMemo(() => {
    const fitted = ({ data, warnings }: { data: Payload; warnings: { code: string }[] }) => {
      if (data.truncated) setNotice('The graph was too large and has been truncated.');
      else if (warnings.some((w) => w.code === 'coarser') && data.aggregated)
        setNotice(`Showing the ${data.aggregated.level} level to stay within the size limit.`);
      return data;
    };
    return {
      units: { label: 'Repository', load: () => api.graph().then(fitted) } as Crumb,
      namespaces: {
        label: 'All namespaces',
        load: () =>
          api.treeGraph([...collapsedRef.current], topRef.current).then(({ data }) => {
            if (data.truncated) {
              const shown = data.nodes.filter((n) => n.kind === 'namespace').length;
              const total = namespaceCountRef.current;
              const of = total === null ? '' : ` of ${total}`;
              setNotice(`Showing the ${shown} busiest${of} namespaces (most calls in and out).`);
            }
            return data;
          }),
      } as Crumb,
    };
  }, [api]);

  /** Replaces the canvas with `load`'s payload and makes it the last crumb. */
  const show = useCallback(
    (crumb: Crumb, keep: Crumb[]) => {
      crumb
        .load()
        .then((p) => {
          graph.clear();
          mergePayload(graph, p);
          setCrumbs([...keep, crumb]);
          setError(null);
          relayout();
          redraw();
        })
        .catch(fail);
    },
    [graph, relayout, redraw, fail],
  );

  useEffect(() => {
    api
      .snapshot()
      .then((s) => {
        setUnits(new Map(s.units.map((u) => [u.id, u])));
        setNamespaceCount(s.summary.nodes?.namespace ?? null);
      })
      .catch(fail);
    api
      .blindspots()
      .then((r) => setSpots(r.blindspots))
      .catch(fail);
    show(roots[initialPrefs.view], []);
  }, [api, show, fail, roots, initialPrefs]);

  // A new Top value or a folded/unfolded directory reloads the namespace
  // map; not on the first render, which the first-view effect loads.
  const mounted = useRef(false);
  useEffect(() => {
    if (!mounted.current) {
      mounted.current = true;
      return;
    }
    if (view === 'namespaces') show(roots.namespaces, []);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [top, collapsed]);

  // The tree is loaded the first time the namespace view is shown.
  useEffect(() => {
    if (view === 'namespaces' && tree === null) api.tree().then(setTree).catch(fail);
  }, [view, tree, api, fail]);

  /** Folds or unfolds one directory of the tree. */
  const toggleDir = useCallback((id: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  /** Switches the canvas to the unit overview or the global namespace map. */
  const switchView = useCallback(
    (next: 'units' | 'namespaces') => {
      setView(next);
      setNotice(null);
      show(roots[next], []);
    },
    [roots, show],
  );

  const select = useCallback(
    (id: string) => {
      setSelected(id);
      setDetails(null);
      setSource(null);
      if (id.startsWith('dir:')) {
        // Directories are not stored: describe one from the tree and the
        // edges on the canvas.
        const node: GNode = graph.hasNode(id)
          ? graph.getNodeAttribute(id, 'node')
          : { id, kind: 'directory', name: (tree && findDir(tree, id)?.label) ?? id };
        const neighbor = (key: string, other: string): Neighbor => {
          const e = graph.getEdgeAttribute(key, 'edge');
          const o = graph.hasNode(other) ? graph.getNodeAttribute(other, 'node') : null;
          return { id: other, name: o ? labelFor(o) : other, kind: e.kind, certainty: e.certainty ?? null, reason: e.reason ?? null, sites: e.sites ?? [], count: e.count ?? null };
        };
        const has = graph.hasNode(id);
        setDetails({
          node,
          callers: has ? graph.inEdges(id).map((k) => neighbor(k, graph.source(k))) : [],
          callees: has ? graph.outEdges(id).map((k) => neighbor(k, graph.target(k))) : [],
          children: tree ? dirChildren(tree, id) : [],
        });
        return;
      }
      api.node(id).then(setDetails).catch(fail);
      if (hasSource(id)) {
        api
          .source(id)
          .then(setSource)
          .catch(() => setSource(''));
      }
    },
    [api, fail, graph, tree],
  );

  const deselect = useCallback(() => {
    setSelected(null);
    setDetails(null);
    setSource(null);
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') deselect();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [deselect]);

  /** Opens a unit (its namespaces) or a namespace (its functions, with the
   *  namespaces it talks to as context) on a canvas of its own. */
  const open = useCallback(
    (n: GNode) => {
      if (n.kind === 'directory') {
        toggleDir(n.id);
        return;
      }
      let load: () => Promise<Payload>;
      if (n.kind === 'unit') {
        load = () => api.unitGraph(n.id).then((r) => r.data);
      } else {
        const context = graph.filterNodes((_id, a) => a.node.kind === 'namespace' || a.node.kind === 'external').map((id) => graph.getNodeAttribute(id, 'node'));
        load = () =>
          api.expand(n.id, context.map((c) => c.id)).then((p) => {
            const ends = new Set(p.edges.flatMap((e) => [e.from, e.to]));
            return { ...p, nodes: [...p.nodes, ...context.filter((c) => ends.has(c.id))] };
          });
      }
      show({ label: labelFor(n), load }, crumbs);
      select(n.id);
    },
    [api, graph, crumbs, show, select, toggleDir],
  );

  const expandHere = useCallback(
    (id: string) =>
      api
        .expand(id, graph.nodes())
        .then((p) => {
          mergePayload(graph, p, id);
          if (p.truncated) setNotice('Only the best-connected contents are shown.');
          relayout();
        })
        .catch(fail),
    [api, graph, relayout, fail],
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

  const focus = useCallback(
    (id: string) => {
      const label = graph.hasNode(id) ? labelFor(graph.getNodeAttribute(id, 'node')) : id;
      show({ label: `${label} (focus)`, load: () => api.neighborhood(id) }, crumbs);
    },
    [api, graph, crumbs, show],
  );

  const follow = useCallback(
    (id: string) => {
      const go = graph.hasNode(id) ? Promise.resolve() : neighbors(id);
      void go.then(() => select(id));
    },
    [graph, neighbors, select],
  );

  const tools: Tools = { open, expandHere: (id) => void expandHere(id), neighbors: (id) => void neighbors(id), focus, follow };

  const onDoubleClick = useCallback(
    (id: string) => {
      const n = graph.hasNode(id) ? graph.getNodeAttribute(id, 'node') : null;
      if (n && isAggregate(n)) open(n);
      else void neighbors(id);
    },
    [graph, open, neighbors],
  );

  const isHidden = useCallback(
    (id: string) => graph.hasNode(id) && hidden(graph.getNodeAttribute(id, 'node'), { hideTests, hideExternals, hideInterop }),
    [graph, hideTests, hideExternals, hideInterop],
  );

  const isEdgeHidden = useCallback((key: string) => !edgeVisible(key), [edgeVisible]);
  const counts = { certain: 0, likely: 0, possible: 0 };
  let visibleEdges = 0;
  graph.forEachEdge((key, a) => {
    const c = a.edge.certainty as keyof Levels | undefined;
    if (c && c in counts) counts[c] += 1;
    if (edgeShown(a.edge, levels)) visibleEdges += 1;
  });
  const setLevel = (level: keyof Levels, on: boolean) => {
    setLevels((l) => ({ ...l, [level]: on }));
  };
  useEffect(() => {
    relayout();
  }, [levels, relayout]);

  const visible = graph
    .mapNodes((_id, a) => a.node)
    .filter((n) => !hidden(n, { hideTests, hideExternals, hideInterop }))
    .sort((a, b) => a.kind.localeCompare(b.kind) || labelFor(a).localeCompare(labelFor(b)));

  const runSearch = (e: React.FormEvent) => {
    e.preventDefault();
    if (!query.trim()) return;
    api
      .search(query)
      .then((r) => setResults(r.matches))
      .catch(fail);
  };

  return (
    <div
      className="app"
      style={{
        gridTemplateColumns: `${panels.leftOpen ? `${panels.left}px 5px` : ''} 1fr ${panels.rightOpen ? `5px ${panels.right}px` : ''}`,
      }}
    >
      {panels.leftOpen && (
      <aside className="left">
        <form onSubmit={runSearch} className="search">
          <input data-testid="search" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search functions…" aria-label="Search" />
        </form>
        {results.length > 0 && (
          <ul className="results" data-testid="results">
            {results.map((n) => (
              <li key={n.id}>
                <button type="button" data-node-id={n.id} onClick={() => follow(n.id)} title={n.id}>
                  {n.name} <span className="muted">{n.namespace ?? n.kind}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
        <label>
          <input
            type="checkbox"
            data-testid="hide-tests"
            checked={hideTests}
            onChange={(e) => {
              setHideTests(e.target.checked);
              redraw();
            }}
          />{' '}
          Hide tests
        </label>
        <label>
          <input
            type="checkbox"
            data-testid="hide-externals"
            checked={hideExternals}
            onChange={(e) => {
              setHideExternals(e.target.checked);
              redraw();
            }}
          />{' '}
          Hide external libraries
        </label>
        <label>
          <input
            type="checkbox"
            data-testid="hide-interop"
            checked={hideInterop}
            onChange={(e) => {
              setHideInterop(e.target.checked);
              redraw();
            }}
          />{' '}
          Hide interop exits (Java, JS)
        </label>
        <fieldset className="legend" data-testid="legend">
          <legend>Edges by certainty</legend>
          {(['certain', 'likely', 'possible'] as const).map((level) => (
            <label key={level} data-testid={`legend-${level}`}>
              <input type="checkbox" data-testid={`show-${level}`} checked={levels[level]} onChange={(e) => setLevel(level, e.target.checked)} />{' '}
              <span className={`swatch swatch-${level}`} style={{ borderTopColor: certaintyColor(level) }} /> {level} ({counts[level]})
            </label>
          ))}
        </fieldset>
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
        <details className="blindspots" data-testid="blindspots" open={spots.length > 0 && spots.length <= 20}>
          <summary>Blind spots ({spots.length})</summary>
          <p className="muted small-print">What the analyzer cannot see through. Click one to show it.</p>
          <ul>
            {spots.map((b, i) => (
              <li key={`${b.category}-${b.node ?? ''}-${i}`}>
                <span className={`badge badge-${b.category}`}>{b.category}</span>{' '}
                {b.node ? (
                  <button type="button" data-node-id={b.node} onClick={() => b.node && follow(b.node)} title={b.message}>
                    {b.site ? `${b.site.file}:${b.site.line}` : b.node}
                  </button>
                ) : (
                  <span title={b.message}>{b.site ? `${b.site.file}:${b.site.line}` : '–'}</span>
                )}
              </li>
            ))}
          </ul>
        </details>
        {view === 'namespaces' && crumbs.length <= 1 && tree ? (
          <TreeView
            root={tree}
            collapsed={collapsed}
            selected={selected}
            onToggle={toggleDir}
            onSelect={select}
            onExpandAll={() => setCollapsed(new Set())}
            onCollapseAll={() => setCollapsed(new Set(topDirIds(tree)))}
          />
        ) : (
        <>
        <h2>On the canvas ({visible.length})</h2>
        <ul className="nodes" data-testid="node-list">
          {visible.map((n) => (
            <li key={n.id} className={n.id === selected ? 'selected' : ''}>
              <button type="button" className="name" data-node-id={n.id} onClick={() => select(n.id)} title={`${n.kind}: ${n.id}`}>
                {labelFor(n)}
              </button>
              {isAggregate(n) && (
                <button type="button" className="small" data-open={n.id} onClick={() => open(n)} title="Show only this">
                  open
                </button>
              )}
            </li>
          ))}
        </ul>
        </>
        )}
      </aside>
      )}
      {panels.leftOpen && <Splitter side="left" width={panels.left} onResize={(left) => setPanels({ left })} />}
      <main className="center">
        <nav className="breadcrumb" data-testid="breadcrumb" aria-label="What is open">
          {crumbs.map((c, i) => (
            <span key={`${i}-${c.label}`}>
              {i > 0 && <span className="muted"> › </span>}
              {i < crumbs.length - 1 ? (
                <button type="button" onClick={() => show(c, crumbs.slice(0, i))}>
                  {c.label}
                </button>
              ) : (
                <strong>{c.label}</strong>
              )}
            </span>
          ))}
          <span className="view-switch" role="group" aria-label="Overview">
            <button type="button" data-testid="view-units" aria-pressed={view === 'units'} onClick={() => switchView('units')} title="Units and how they depend on each other">
              Units
            </button>
            <button
              type="button"
              data-testid="view-namespaces"
              aria-pressed={view === 'namespaces'}
              onClick={() => switchView('namespaces')}
              title="Every namespace in the repository on one map (no externals)"
            >
              All namespaces
            </button>
          </span>
          {view === 'namespaces' && (
            <label className="top-select" title="How many namespaces to show, busiest first">
              Top{' '}
              <select
                data-testid="top"
                value={top}
                onChange={(e) => {
                  setTop(Number(e.target.value));
                  setNotice(null);
                }}
              >
                {TOP_CHOICES.map((n) => (
                  <option key={n} value={n}>
                    {n}
                  </option>
                ))}
              </select>
            </label>
          )}
          <span className="panel-toggles">
            <button
              type="button"
              data-testid="toggle-left"
              aria-pressed={panels.leftOpen}
              onClick={() => setPanels({ leftOpen: !panels.leftOpen })}
              title={panels.leftOpen ? 'Hide the list' : 'Show the list'}
            >
              {panels.leftOpen ? '◀ list' : '▶ list'}
            </button>
            <button
              type="button"
              data-testid="toggle-right"
              aria-pressed={panels.rightOpen}
              onClick={() => setPanels({ rightOpen: !panels.rightOpen })}
              title={panels.rightOpen ? 'Hide details' : 'Show details'}
            >
              {panels.rightOpen ? 'details ▶' : 'details ◀'}
            </button>
          </span>
        </nav>
        <GraphView graph={graph} version={version} selected={selected} isHidden={isHidden} isEdgeHidden={isEdgeHidden} visibleEdges={visibleEdges} onSelect={select} onDoubleClick={onDoubleClick} onDeselect={deselect} />
      </main>
      {panels.rightOpen && <Splitter side="right" width={panels.right} onResize={(right) => setPanels({ right })} />}
      {panels.rightOpen && (
      <aside className="right">
        {details ? (
          <Details d={details} unit={units.get(details.node.id)} tools={tools} onClose={deselect} />
        ) : (
          <div className="muted" data-testid="hint">
            <p>Click a node to see what it is, what it contains and what it is connected to.</p>
            <p>Double-click a unit or namespace (or use Open) to show only its contents; the bar above the canvas takes you back.</p>
            <p>A unit is a directory with its own build file (deps.edn, project.clj, shadow-cljs.edn, bb.edn). Callers, callees and source appear once you select a function.</p>
          </div>
        )}
        {source && <SourcePanel key={selected ?? ''} source={source} />}
      </aside>
      )}
    </div>
  );
}

/** The selected node's source with a button that copies it. Keyed by the
 *  selection, so "Copied" resets when the selection changes.
 *  Example: `<SourcePanel source="(defn f [] 1)" />`. */
function SourcePanel({ source }: { source: string }) {
  const [copied, setCopied] = useState(false);
  const copy = () => {
    void navigator.clipboard
      ?.writeText(source)
      .then(() => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1500);
      })
      .catch(() => setCopied(false));
  };
  return (
    <section>
      <div className="details-head">
        <h2>Source</h2>
        <button type="button" data-testid="copy-source" onClick={copy} title="Copy the source to the clipboard">
          {copied ? 'Copied' : 'Copy'}
        </button>
      </div>
      <pre data-testid="source">{source}</pre>
    </section>
  );
}
