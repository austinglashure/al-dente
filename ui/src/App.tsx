// The GUI: a canvas with a breadcrumb of what is open, a node list, search
// and toggles, and a details panel with tools. Every repository string is
// rendered as React text.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { Api, initToken } from './api';
import { libraryHighlight } from './crater';
import { CraterView, type CraterRequest } from './CraterView';
import { Details, type Tools } from './Details';
import { GraphView } from './GraphView';
import { CycleList, HotspotList } from './Hotspots';
import { MeasuresTable } from './MeasuresTable';
import { OverlayControls, useReports, useStyler } from './Overlays';
import type { Metric, OverlayKind } from './overlay';
import { CanvasBar } from './CanvasBar';
import { LeftPanel } from './LeftPanel';
import { LibraryPanel } from './Libraries';
import { SourcePanel } from './SourcePanel';
import { HelpDialog } from './HelpDialog';
import { TraceDialog } from './TraceDialog';
import { applyPage, batched, collapse, nextOffset, remaining, type Attachable, type Expansions } from './expansion';
import { layout, layoutRuns } from './layout';
import { edgeShown, hasSource, hidden, isAggregate, labelFor, mergePayload, newGraph, type HideOptions, type Levels } from './model';
import { Splitter, usePanels } from './panels';
import type { BlindSpot, GNode, LibrarySummary, LibraryUsage, Neighbor, Payload, ShowResult, TreeDir, UnitInfo } from './types';
import { dirChildren, findDir } from './tree';
import { loadViewPrefs, saveViewPrefs } from './viewPrefs';

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
  const [hide, setHide] = useState<HideOptions>({ hideTests: false, hideExternals: true, hideInterop: false, hideCore: false });
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
  const [crater, setCrater] = useState<CraterRequest | null>(null);
  const [traceOpen, setTraceOpen] = useState(false);
  const [helpOpen, setHelpOpen] = useState(false);
  const [libs, setLibs] = useState<LibrarySummary[]>([]);
  const [libUsage, setLibUsage] = useState<LibraryUsage | null>(null);
  const [overlay, setOverlay] = useState<OverlayKind>('risk');
  const [sizeBy, setSizeBy] = useState<Metric | null>(null);
  const [colorBy, setColorBy] = useState<Metric | null>(null);
  const [measureSel, setMeasureSel] = useState<Set<string>>(() => new Set());
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

  // End-to-end tests read node positions from here.
  useEffect(() => {
    (window as unknown as { aldenteCanvas?: unknown }).aldenteCanvas = graph;
  }, [graph]);

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
  // Aggregates expanded in place on the current canvas.
  const expansions = useMemo<Expansions>(() => new Map(), []);
  // The canvas renderer, detached while a batch of changes is applied.
  const renderer = useRef<Attachable | null>(null);
  const fail = useCallback((e: unknown) => setError(e instanceof Error ? e.message : String(e)), []);
  // Set once the first canvas is drawn; slow reports wait for it.
  const [firstShown, setFirstShown] = useState(false);
  const [measuresOpen, setMeasuresOpen] = useState(false);
  const reports = useReports(api, overlay, sizeBy !== null || colorBy !== null, fail, firstShown);

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
          batched(renderer.current, graph, () => {
            graph.clear();
            expansions.clear();
            mergePayload(graph, p);
          });
          setCrumbs([...keep, crumb]);
          setFirstShown(true);
          setError(null);
          relayout();
          redraw();
        })
        .catch(fail);
    },
    [graph, expansions, relayout, redraw, fail],
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
    api
      .libs()
      .then((r) => setLibs(r.libraries))
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

  /** Expands an aggregate in place, or loads its next page if it is
   *  expanded already; only the new nodes move. */
  const expandHere = useCallback(
    (id: string) => {
      if (!graph.hasNode(id)) {
        return api
          .expand(id, graph.nodes())
          .then((p) => {
            mergePayload(graph, p);
            relayout();
          })
          .catch(fail);
      }
      return api
        .expandPage(id, graph.nodes(), nextOffset(expansions, id))
        .then((p) => {
          let added: string[] = [];
          batched(renderer.current, graph, () => {
            added = applyPage(graph, expansions, id, p);
          });
          layout(graph, redraw, edgeVisible, new Set(added));
          redraw();
        })
        .catch(fail);
    },
    [api, graph, expansions, relayout, redraw, edgeVisible, fail],
  );

  /** Collapses an aggregate expanded in place back into one node. */
  const collapseHere = useCallback(
    (id: string) => {
      batched(renderer.current, graph, () => collapse(graph, expansions, id));
      redraw();
    },
    [graph, expansions, redraw],
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

  const openCrater = useCallback((n: GNode, direction: 'forward' | 'reverse') => {
    setCrater({ kind: 'seeds', ids: [n.id], label: labelFor(n), direction });
  }, []);

  const tools: Tools = {
    open,
    expandHere: (id) => void expandHere(id),
    collapse: collapseHere,
    neighbors: (id) => void neighbors(id),
    focus,
    follow,
    crater: openCrater,
  };

  /** Selects a library: lists its call sites and highlights its users. */
  const pickLibrary = useCallback(
    (id: string) => {
      api
        .lib(id)
        .then((u) => {
          setLibUsage(u);
          redraw();
        })
        .catch(fail);
    },
    [api, fail, redraw],
  );
  const libHighlight = useMemo(
    () =>
      measureSel.size > 0
        ? { nodes: new Set(graph.nodes().filter((id) => measureSel.has(id))), edges: new Set<string>() }
        : libUsage
          ? { nodes: libraryHighlight(libUsage, graph.nodes()), edges: new Set<string>() }
          : null,
    // `version` changes whenever the canvas does.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [libUsage, measureSel, graph, version],
  );

  const onDoubleClick = useCallback(
    (id: string) => {
      const n = graph.hasNode(id) ? graph.getNodeAttribute(id, 'node') : null;
      if (n?.kind === 'directory') open(n);
      else if (n && isAggregate(n)) {
        if (expansions.has(id)) collapseHere(id);
        else void expandHere(id);
      } else void neighbors(id);
    },
    [graph, expansions, open, neighbors, expandHere, collapseHere],
  );

  const isHidden = useCallback(
    (id: string) => graph.hasNode(id) && (graph.getNodeAttribute(id, 'expanded') === true || hidden(graph.getNodeAttribute(id, 'node'), hide)),
    [graph, hide],
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
    .filterNodes((_id, a) => a.expanded !== true)
    .map((id) => graph.getNodeAttribute(id, 'node'))
    .filter((n) => !hidden(n, hide))
    .sort((a, b) => a.kind.localeCompare(b.kind) || labelFor(a).localeCompare(labelFor(b)));
  const styleOf = useStyler(reports, overlay, sizeBy, colorBy, visible.map((n) => n.id));
  const styledUntested = visible.filter((n) => {
    const tag = styleOf(n.id)?.tag;
    return tag === 'untested' || tag === 'risky-untested';
  }).length;
  const showCycle = (c: { nodes: string[]; size: number }) => show({ label: `cycle of ${c.size}`, load: () => api.subgraph(c.nodes) }, crumbs);

  return (
    <div
      className="app"
      style={{
        gridTemplateColumns: `${panels.leftOpen ? `${panels.left}px 5px` : ''} 1fr ${panels.rightOpen ? `5px ${panels.right}px` : ''}`,
      }}
    >
      {panels.leftOpen && (
        <LeftPanel
          api={api}
          hide={hide}
          onHide={(next) => {
            setHide(next);
            redraw();
          }}
          levels={levels}
          counts={counts}
          onLevel={setLevel}
          notice={notice}
          error={error}
          onError={fail}
          libs={libs}
          selectedLib={libUsage?.library ?? null}
          onPickLib={pickLibrary}
          spots={spots}
          tree={view === 'namespaces' && crumbs.length <= 1 ? tree : null}
          collapsed={collapsed}
          onToggleDir={toggleDir}
          onSetCollapsed={setCollapsed}
          visible={visible}
          selected={selected}
          onSelect={select}
          onFollow={follow}
          onOpen={open}
          overlays={
            <OverlayControls overlay={overlay} onOverlay={setOverlay} sizeBy={sizeBy} onSizeBy={setSizeBy} colorBy={colorBy} onColorBy={setColorBy} />
          }
          views={
            <>
              <HotspotList report={reports.hotspots} onFollow={follow} />
              <CycleList api={api} onShow={showCycle} onError={fail} />
            </>
          }
        />
      )}
      {panels.leftOpen && <Splitter side="left" width={panels.left} onResize={(left) => setPanels({ left })} />}
      <main className="center">
        {crater ? (
          <CraterView key={JSON.stringify(crater)} api={api} request={crater} onClose={() => setCrater(null)} onSelect={select} hideCoreInitially={!!hide.hideCore} />
        ) : (
        <>
        <CanvasBar
          crumbs={crumbs}
          onCrumb={(i) => {
            const c = crumbs[i];
            if (c) show(c, crumbs.slice(0, i));
          }}
          view={view}
          onView={switchView}
          top={top}
          onTop={(n) => {
            setTop(n);
            setNotice(null);
          }}
          onPasteTrace={() => setTraceOpen(true)}
          onHelp={() => setHelpOpen(true)}
          panels={panels}
          setPanels={setPanels}
        />
        <GraphView
          graph={graph}
          version={version}
          selected={selected}
          isHidden={isHidden}
          isEdgeHidden={isEdgeHidden}
          visibleEdges={visibleEdges}
          onSelect={select}
          onDoubleClick={onDoubleClick}
          onDeselect={deselect}
          highlight={libHighlight}
          styleOf={styleOf}
          semanticZoom
          onRenderer={(s) => {
            renderer.current = s;
          }}
          data={{ nodes: graph.order, overlay, 'styled-untested': styledUntested, 'layout-runs': layoutRuns() }}
        />
        </>
        )}
      </main>
      {panels.rightOpen && <Splitter side="right" width={panels.right} onResize={(right) => setPanels({ right })} />}
      {panels.rightOpen && (
      <aside className="right">
        {libUsage && (
          <LibraryPanel
            u={libUsage}
            highlighted={libHighlight?.nodes.size ?? 0}
            onClose={() => {
              setLibUsage(null);
              redraw();
            }}
            onFollow={follow}
          />
        )}
        {details ? (
          <Details
            d={details}
            unit={units.get(details.node.id)}
            tools={tools}
            expansion={expansions.has(details.node.id) ? { left: remaining(expansions, details.node.id) } : null}
            onClose={deselect}
          />
        ) : (
          <div className="muted" data-testid="hint">
            <p>Click a node to see what it is, what it contains and what it is connected to.</p>
            <p>Double-click a unit or namespace to expand it in place, and again to collapse it. Open shows only its contents; the bar above the canvas takes you back.</p>
            <p>A unit is a directory with its own build file (deps.edn, project.clj, shadow-cljs.edn, bb.edn). Callers, callees and source appear once you select a function.</p>
          </div>
        )}
        {source && <SourcePanel key={selected ?? ''} source={source} />}
        <details className="measures-panel" data-testid="measures-panel" onToggle={(e) => setMeasuresOpen(e.currentTarget.open)}>
          <summary>Measures</summary>
          <MeasuresTable api={api} selected={measureSel} onSelected={setMeasureSel} onFollow={follow} onError={fail} active={measuresOpen} />
        </details>
      </aside>
      )}
      {helpOpen && <HelpDialog api={api} onClose={() => setHelpOpen(false)} />}
      {traceOpen && (
        <TraceDialog
          api={api}
          onClose={() => setTraceOpen(false)}
          onPin={(text) => {
            setTraceOpen(false);
            setCrater({ kind: 'trace', text, label: 'a pasted trace' });
          }}
        />
      )}
    </div>
  );
}
