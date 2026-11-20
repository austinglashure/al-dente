// The GUI: a canvas with a breadcrumb of what is open, a node list, search
// and toggles, and a details panel with tools. Every repository string is
// rendered as React text.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { Api, initToken } from './api';
import { libraryHighlight } from './crater';
import { CraterView, type CraterRequest } from './CraterView';
import { Details, type NodeFacts, type Tools } from './Details';
import { GraphView } from './GraphView';
import { CycleList, HotspotList } from './Hotspots';
import { MeasuresTable } from './MeasuresTable';
import { OverlayControls, useReports, useStyler } from './Overlays';
import type { Metric, OverlayKind } from './overlay';
import { CanvasBar } from './CanvasBar';
import { LeftPanel } from './LeftPanel';
import { LibraryPanel } from './Libraries';
import { EdgeSites } from './EdgeSites';
import { SourcePanel } from './SourcePanel';
import { HelpDialog } from './HelpDialog';
import { TraceDialog } from './TraceDialog';
import { applyPage, batched, collapse, nextOffset, remaining, type Attachable, type Expansions } from './expansion';
import { FilterPanel } from './FilterPanel';
import { NO_FILTERS, passes, type Filters } from './filters';
import { useWorkspace } from './useWorkspace';
import type { Op } from './workspace';
import { WorkspacePanel } from './WorkspacePanel';
import { layout, layoutPending, layoutRuns } from './layout';
import { dropNodes, edgeShown, hasSource, hidden, isAggregate, labelFor, mergePayload, newGraph, type HideOptions, type Levels } from './model';
import { metricValues, nodeMeasures } from './overlay';
import { Splitter, usePanels } from './panels';
import type { BlindSpot, GEdge, GNode, LibrarySummary, LibraryUsage, Neighbor, Payload, ShowResult, TreeDir, UnitInfo, UnitLinks } from './types';
import { dirChildren, dirMembers, findDir } from './tree';
import { placeTreeNodes, type Point } from './treePlace';
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
  const [crumbs, setCrumbState] = useState<Crumb[]>([]);
  // The breadcrumb as of the last change, read by steps replayed back to back
  // (before React re-renders); `crumbs` mirrors it for drawing.
  const crumbsRef = useRef<Crumb[]>([]);
  const setCrumbs = useCallback((c: Crumb[]) => {
    crumbsRef.current = c;
    setCrumbState(c);
  }, []);
  const [filters, setFilters] = useState<Filters>(NO_FILTERS);
  // Nodes marked by box select, for Hide and Keep only.
  const [marked, setMarked] = useState<Set<string>>(() => new Set());
  const [units, setUnits] = useState<Map<string, UnitInfo>>(new Map());
  const [selected, setSelected] = useState<string | null>(null);
  const [details, setDetails] = useState<ShowResult | null>(null);
  const [source, setSource] = useState<{ text: string; first: number } | null>(null);
  // A call site's line in context, an edge's sites, and what "Open in
  // editor" did last.
  const [siteView, setSiteView] = useState<{ file: string; line: number; text: string; first: number } | null>(null);
  const [edgeSel, setEdgeSel] = useState<string | null>(null);
  const [editorNote, setEditorNote] = useState<string | null>(null);
  const [wantMetrics, setWantMetrics] = useState(false);
  const [unitLinks, setUnitLinks] = useState<UnitLinks | null>(null);
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
  const reports = useReports(api, overlay, wantMetrics || sizeBy !== null || colorBy !== null || filters.measures.length > 0, fail, firstShown);
  /** A node's value for a measure, for the filter panel's minimums. */
  const measureOf = useMemo(() => {
    const { metrics } = reports;
    const byMetric = new Map(filters.measures.map((m) => [m.metric, metrics ? metricValues(metrics, m.metric as Metric) : new Map<string, number>()]));
    return (metric: string, id: string): number | undefined => byMetric.get(metric)?.get(id);
  }, [reports, filters.measures]);

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
    (crumb: Crumb, keep: Crumb[]): Promise<void> =>
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
        .catch(fail),
    [graph, expansions, setCrumbs, relayout, redraw, fail],
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
  }, [api, fail]);

  // Positions nodes had before a fold, so unfolding brings them back.
  const remembered = useRef(new Map<string, Point>());
  /** Reloads the namespace map after a fold or unfold without moving the
   *  picture: nodes that stay keep their place, a folded directory sits at
   *  the centre of what it replaced, and unfolded members return where they
   *  were. No layout runs. */
  const refold = useCallback(
    (): Promise<void> =>
      roots.namespaces
        .load()
        .then((p) => {
          const before = new Map<string, Point>();
          graph.forEachNode((id, a) => before.set(id, { x: a.x, y: a.y }));
          for (const [id, xy] of before) remembered.current.set(id, xy);
          const membersOf = (dir: string) => (tree ? dirMembers(tree, dir) : new Set<string>());
          const placed = placeTreeNodes(p.nodes.map((n) => n.id), before, remembered.current, membersOf);
          batched(renderer.current, graph, () => {
            graph.clear();
            expansions.clear();
            mergePayload(graph, p);
            graph.forEachNode((id) => {
              const xy = placed.get(id);
              if (xy) graph.mergeNodeAttributes(id, xy);
            });
          });
          setError(null);
          redraw();
        })
        .catch(fail),
    [roots, graph, tree, expansions, redraw, fail],
  );
  // A new Top value reloads the namespace map; a folded or unfolded
  // directory refolds it in place. Neither on the first render, which the
  // first-view effect loads.
  const mounted = useRef(false);
  useEffect(() => {
    if (!mounted.current) return;
    if (view === 'namespaces') void show(roots.namespaces, []);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [top]);
  useEffect(() => {
    if (!mounted.current) {
      mounted.current = true;
      return;
    }
    if (view !== 'namespaces') return;
    // Drilled into something: go back to the map, as before.
    if (crumbsRef.current.length > 1) void show(roots.namespaces, []);
    else void refold();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [collapsed]);

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


  const select = useCallback(
    (id: string) => {
      setSelected(id);
      setDetails(null);
      setSource(null);
      setSiteView(null);
      setEdgeSel(null);
      setEditorNote(null);
      setUnitLinks(null);
      if (id.startsWith('unit:')) api.unitLinks(id).then(setUnitLinks).catch(fail);
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
          .sourceAt(id)
          .then(setSource)
          .catch(() => setSource({ text: '', first: 1 }));
      }
    },
    [api, fail, graph, tree],
  );

  const deselect = useCallback(() => {
    setSelected(null);
    setDetails(null);
    setSource(null);
    setSiteView(null);
    setEdgeSel(null);
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
  const openOnCanvas = useCallback(
    (id: string): Promise<void> => {
      const n: GNode = graph.hasNode(id)
        ? graph.getNodeAttribute(id, 'node')
        : id.startsWith('dir:')
          ? { id, kind: 'directory', name: id.slice('dir:'.length) }
          : { id, kind: id.startsWith('unit:') ? 'unit' : 'namespace', name: id.split('//').pop() ?? id };
      let load: () => Promise<Payload>;
      if (n.kind === 'directory') {
        load = () => api.dirGraph(id.slice('dir:'.length)).then((r) => r.data);
      } else if (n.kind === 'unit') {
        load = () => api.unitGraph(n.id).then((r) => r.data);
      } else {
        const context = graph.filterNodes((_id, a) => a.node.kind === 'namespace' || a.node.kind === 'external').map((id) => graph.getNodeAttribute(id, 'node'));
        load = () =>
          api.expand(n.id, context.map((c) => c.id)).then((p) => {
            const ends = new Set(p.edges.flatMap((e) => [e.from, e.to]));
            return { ...p, nodes: [...p.nodes, ...context.filter((c) => ends.has(c.id))] };
          });
      }
      return show({ label: labelFor(n), load }, crumbsRef.current);
    },
    [api, graph, show],
  );

  // Aggregates with a page request in flight: a second click meanwhile is
  // ignored instead of asking for the same page again.
  const expanding = useRef(new Set<string>());

  /** Expands an aggregate in place, or loads its next page if it is
   *  expanded already; only the new nodes move. */
  const expandHere = useCallback(
    (id: string): Promise<void> => {
      if (expanding.current.has(id)) return Promise.resolve();
      if (!graph.hasNode(id)) {
        return api
          .expand(id, graph.nodes())
          .then((p) => {
            mergePayload(graph, p);
            relayout();
          })
          .catch(fail);
      }
      expanding.current.add(id);
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
        .catch(fail)
        .finally(() => expanding.current.delete(id));
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

  /** Removes nodes from the canvas in one batch. */
  const drop = useCallback(
    (ids: string[]) => {
      batched(renderer.current, graph, () => dropNodes(graph, ids));
      redraw();
    },
    [graph, redraw],
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

  const focusOn = useCallback(
    (id: string): Promise<void> => {
      const label = graph.hasNode(id) ? labelFor(graph.getNodeAttribute(id, 'node')) : id;
      return show({ label: `${label} (focus)`, load: () => api.neighborhood(id) }, crumbsRef.current);
    },
    [api, graph, show],
  );

  /** Performs one workspace step on the canvas (the workspace logs it). */
  const apply = useCallback(
    async (op: Op): Promise<void> => {
      switch (op.op) {
        case 'view':
          setView(op.view);
          setNotice(null);
          return show(roots[op.view], []);
        case 'open':
          return openOnCanvas(op.id);
        case 'focus':
          return focusOn(op.id);
        case 'expand':
          return expandHere(op.id);
        case 'collapse':
          return collapseHere(op.id);
        case 'neighbors':
          return neighbors(op.id);
        case 'hide':
          return drop(op.ids);
        case 'keep': {
          const keep = new Set(op.ids);
          return drop(graph.filterNodes((id) => !keep.has(id)));
        }
        case 'pin':
          if (graph.hasNode(op.id)) graph.setNodeAttribute(op.id, 'pinned', op.on);
          redraw();
          return;
        case 'filters':
          setFilters(op.filters);
          redraw();
          return;
        case 'quick':
          setHide(op.hide);
          redraw();
          return;
        case 'back': {
          const c = crumbsRef.current[op.to];
          return c ? show(c, crumbsRef.current.slice(0, op.to)) : undefined;
        }
        case 'cycle':
          return show({ label: `cycle of ${op.ids.length}`, load: () => api.subgraph(op.ids) }, crumbsRef.current);
      }
    },
    [api, graph, roots, show, openOnCanvas, focusOn, expandHere, collapseHere, neighbors, drop, redraw],
  );

  const ws = useWorkspace({
    graph,
    expansions,
    renderer,
    apply,
    capture: () => ({ crumbs: crumbsRef.current, filters, hide, view }),
    restore: (e: { crumbs: Crumb[]; filters: Filters; hide: HideOptions; view: 'units' | 'namespaces' }) => {
      setCrumbs(e.crumbs);
      setFilters(e.filters);
      setHide(e.hide);
      setView(e.view);
    },
    redraw,
    onNotice: setNotice,
    version,
  });
  const run = ws.run;
  const startWorkspace = ws.start;
  const fromUrl = ws.fromUrl;

  // The first view: the workspace in the URL if there is one, else the
  // remembered root view.
  const started = useRef(false);
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    void fromUrl().then((found) => {
      if (!found) return show(roots[initialPrefs.view], []).then(() => startWorkspace({ op: 'view', view: initialPrefs.view }));
    });
  }, [fromUrl, show, roots, initialPrefs, startWorkspace]);

  const open = useCallback(
    (n: GNode) => {
      void run({ op: 'open', id: n.id }).then(() => select(n.id));
    },
    [run, select],
  );

  const follow = useCallback(
    (id: string) => {
      const go = graph.hasNode(id) ? Promise.resolve() : run({ op: 'neighbors', id });
      void go.then(() => select(id));
    },
    [graph, run, select],
  );

  /** Shows a call site's line in context. */
  const showSite = useCallback(
    (site: { file: string; line: number }) =>
      void api
        .siteSource(site.file, site.line)
        .then((r) => setSiteView({ file: site.file, line: site.line, ...r }))
        .catch(fail),
    [api, fail],
  );

  /** Every other node of the same kind and subkind on the canvas. */
  const othersLike = useCallback(
    (id: string) => {
      const n = graph.getNodeAttribute(id, 'node');
      return graph.filterNodes((o, a) => o !== id && a.node.kind === n.kind && a.node.subkind === n.subkind);
    },
    [graph],
  );

  const openCrater = useCallback((n: GNode, direction: 'forward' | 'reverse') => {
    setCrater({ kind: 'seeds', ids: [n.id], label: labelFor(n), direction });
  }, []);

  const tools: Tools = {
    open,
    unfold: toggleDir,
    expandHere: (id) => void run({ op: 'expand', id }),
    collapse: (id) => void run({ op: 'collapse', id }),
    neighbors: (id) => void run({ op: 'neighbors', id }),
    focus: (id) => void run({ op: 'focus', id }),
    follow,
    crater: openCrater,
    hide: (id) => {
      deselect();
      void run({ op: 'hide', ids: [id] });
    },
    hideLike: (id) => void run({ op: 'hide', ids: othersLike(id) }),
    pin: (id) => void run({ op: 'pin', id, on: !(graph.hasNode(id) && graph.getNodeAttribute(id, 'pinned') === true) }),
    keepOnly: (id) => void run({ op: 'keep', ids: [...new Set([id, ...marked])] }),
    openEditor: (n) =>
      void api
        .openInEditor({ id: n.id })
        .then((r) => {
          if (r.opened) setEditorNote(`Opened ${r.location} in your editor.`);
          else {
            void navigator.clipboard?.writeText(r.location).catch(() => undefined);
            setEditorNote(`No editor is set (add editor = […] to config.toml); copied ${r.location}.`);
          }
        })
        .catch(fail),
    site: showSite,
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

  /** What the coverage, hotspot and measure reports say about the selection. */
  const facts: NodeFacts | null = useMemo(() => {
    const id = details?.node.id;
    if (!id) return null;
    return {
      coverage: reports.coverage?.symbols.find((c) => c.id === id) ?? null,
      findings: reports.hotspots?.findings.filter((f) => f.subject === id || f.nodes.includes(id)) ?? [],
      measures: reports.metrics ? nodeMeasures(reports.metrics, id) : null,
      onLoadMeasures: () => setWantMetrics(true),
    };
  }, [details, reports]);

  // Keyboard navigation (06-gui): ← caller, → callee (strongest first), e
  // expand or collapse, h hide, f focus, c crater, / search. Not while typing.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement;
      if (typing || e.ctrlKey || e.metaKey || e.altKey) return;
      if (e.key === '/') {
        e.preventDefault();
        document.querySelector<HTMLInputElement>('[data-testid="search"]')?.focus();
        return;
      }
      const d = details;
      if (!d) return;
      const id = d.node.id;
      const go = (list: Neighbor[]) => {
        const next = list[0];
        if (next) follow(next.id);
      };
      switch (e.key) {
        case 'ArrowLeft':
          go(d.callers);
          break;
        case 'ArrowRight':
          go(d.callees);
          break;
        case 'e':
          if (isAggregate(d.node)) void run({ op: expansions.has(id) ? 'collapse' : 'expand', id });
          break;
        case 'h':
          deselect();
          void run({ op: 'hide', ids: [id] });
          break;
        case 'f':
          void run({ op: 'focus', id });
          break;
        case 'c':
          openCrater(d.node, 'forward');
          break;
        default:
          return;
      }
      e.preventDefault();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [details, follow, run, expansions, deselect, openCrater]);

  const onDoubleClick = useCallback(
    (id: string) => {
      const n = graph.hasNode(id) ? graph.getNodeAttribute(id, 'node') : null;
      if (n?.kind === 'directory') toggleDir(n.id);
      else if (n && isAggregate(n)) void run({ op: expansions.has(id) ? 'collapse' : 'expand', id });
      else void run({ op: 'neighbors', id });
    },
    [graph, expansions, toggleDir, run],
  );

  const isHidden = useCallback(
    (id: string) => {
      if (!graph.hasNode(id)) return false;
      const a = graph.getNodeAttributes(id);
      return a.expanded === true || hidden(a.node, hide) || !passes(a.node, filters, measureOf);
    },
    [graph, hide, filters, measureOf],
  );

  const isEdgeHidden = useCallback((key: string) => !edgeVisible(key), [edgeVisible]);
  // The same filters, for whatever other graph is on screen (the crater).
  const craterNodeHidden = useCallback((n: GNode) => hidden(n, hide) || !passes(n, filters, measureOf), [hide, filters, measureOf]);
  const craterEdgeShown = useCallback((e: GEdge) => edgeShown(e, levels), [levels]);
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
    .filterNodes((id) => !isHidden(id))
    .map((id) => graph.getNodeAttribute(id, 'node'))
    .sort((a, b) => a.kind.localeCompare(b.kind) || labelFor(a).localeCompare(labelFor(b)));
  const styleOf = useStyler(reports, overlay, sizeBy, colorBy, visible.map((n) => n.id));
  const styledUntested = visible.filter((n) => {
    const tag = styleOf(n.id)?.tag;
    return tag === 'untested' || tag === 'risky-untested';
  }).length;
  const showCycle = (c: { nodes: string[]; size: number }) => void run({ op: 'cycle', ids: c.nodes });

  // Undo and redo from the keyboard (not while typing).
  const { undo, redo } = ws;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement || e.target instanceof HTMLSelectElement;
      if (typing || !(e.ctrlKey || e.metaKey)) return;
      const key = e.key.toLowerCase();
      if (key === 'z' && !e.shiftKey) undo();
      else if ((key === 'z' && e.shiftKey) || key === 'y') redo();
      else return;
      e.preventDefault();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [undo, redo]);

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
          onHide={(next) => void run({ op: 'quick', hide: next })}
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
          workspace={
            <>
              <WorkspacePanel
                canUndo={ws.canUndo}
                canRedo={ws.canRedo}
                onUndo={ws.undo}
                onRedo={ws.redo}
                saved={ws.saved}
                onSave={(name) => void ws.save(name)}
                onLoad={(name) => void ws.load(name)}
                onRemove={ws.remove}
              />
              <FilterPanel filters={filters} units={[...units.values()]} onChange={(f) => void run({ op: 'filters', filters: f })} />
            </>
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
          <CraterView
            key={JSON.stringify(crater)}
            api={api}
            request={crater}
            onClose={() => setCrater(null)}
            onSelect={select}
            nodeHidden={craterNodeHidden}
            edgeShown={craterEdgeShown}
            filterVersion={JSON.stringify([hide, filters, levels])}
          />
        ) : (
        <>
        <CanvasBar
          crumbs={crumbs}
          onCrumb={(i) => {
            if (crumbs[i]) void run({ op: 'back', to: i });
          }}
          view={view}
          onView={(next) => void run({ op: 'view', view: next })}
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
        {crumbs.length === 1 && view === 'units' && units.size > 1 && (
          <p className="network-note" data-testid="network-note">
            Links between units are code-level: calls into another unit&apos;s namespaces and <code>:local/root</code> dependencies (orange). Network links between services (HTTP, queues, databases) are added in the surface view (Phase 6).
          </p>
        )}
        {marked.size > 0 && (
          <div className="selection-bar" data-testid="selection-bar">
            {marked.size} selected{' '}
            <button type="button" data-action="hide-marked" onClick={() => void run({ op: 'hide', ids: [...marked] }).then(() => setMarked(new Set()))}>
              Hide
            </button>{' '}
            <button type="button" data-action="keep-marked" onClick={() => void run({ op: 'keep', ids: [...marked] }).then(() => setMarked(new Set()))}>
              Keep only
            </button>{' '}
            <button type="button" data-action="clear-marked" onClick={() => setMarked(new Set())}>
              Clear
            </button>
          </div>
        )}
        <GraphView
          graph={graph}
          onBoxSelect={(ids) => setMarked(new Set(ids))}
          version={version}
          selected={selected}
          isHidden={isHidden}
          isEdgeHidden={isEdgeHidden}
          visibleEdges={visibleEdges}
          onSelect={select}
          onDoubleClick={onDoubleClick}
          onEdgeClick={(key) => {
            setEdgeSel(key);
            setSiteView(null);
          }}
          onDeselect={deselect}
          highlight={marked.size > 0 ? { nodes: marked, edges: new Set<string>() } : libHighlight}
          styleOf={styleOf}
          semanticZoom
          onRenderer={(s) => {
            renderer.current = s;
          }}
          data={{ nodes: graph.order, 'layout-pending': layoutPending() ? 1 : 0, overlay, 'styled-untested': styledUntested, 'layout-runs': layoutRuns() }}
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
        {edgeSel && graph.hasEdge(edgeSel) && <EdgeSites edge={graph.getEdgeAttribute(edgeSel, 'edge')} onSite={showSite} onClose={() => setEdgeSel(null)} />}
        {details ? (
          <Details
            d={details}
            unit={units.get(details.node.id)}
            tools={tools}
            expansion={expansions.has(details.node.id) ? { left: remaining(expansions, details.node.id) } : null}
            facts={facts}
            editorNote={editorNote}
            unitLinks={unitLinks?.unit === details.node.id ? unitLinks : null}
            onClose={deselect}
          />
        ) : (
          <div className="muted" data-testid="hint">
            <p>Click a node to see what it is, what it contains and what it is connected to.</p>
            <p>Double-click a unit or namespace to expand it in place, and again to collapse it. Open shows only its contents; the bar above the canvas takes you back.</p>
            <p>A unit is a directory with its own build file (deps.edn, project.clj, shadow-cljs.edn, bb.edn). Callers, callees and source appear once you select a function.</p>
          </div>
        )}
        {source && <SourcePanel key={selected ?? ''} source={source.text} first={source.first} />}
        {siteView && (
          <SourcePanel
            key={`${siteView.file}:${siteView.line}`}
            title={`${siteView.file}:${siteView.line}`}
            source={siteView.text}
            first={siteView.first}
            mark={siteView.line}
            testId="site-source"
          />
        )}
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
