// The Sigma.js canvas. Repository strings reach it only as node labels,
// which Sigma draws on a canvas, never as HTML.

import { useEffect, useLayoutEffect, useRef } from 'react';
import type Graph from 'graphology';
import Sigma from 'sigma';
import { inBox, normalize, type Point } from './boxSelect';
import { edgeHiddenAtZoom, labelHiddenAtZoom, zoomLevel } from './expansion';
import { shownLabel } from './nodeLabel';
import type { EdgeAttrs, NodeAttrs } from './model';
import type { NodeStyle } from './overlay';

/** Nodes and edges to emphasize; everything else is drawn faded. */
export interface Highlight {
  nodes: Set<string>;
  edges: Set<string>;
  /** Of those, the ones to stress (for example the shortest paths). */
  strongNodes?: Set<string>;
  strongEdges?: Set<string>;
}

interface Props<N extends NodeAttrs, E extends EdgeAttrs> {
  graph: Graph<N, E>;
  /** Bumped whenever the graph or the selection changes, to redraw. */
  /** Changes whenever what is drawn changes; a redraw follows. */
  version: number | string;
  selected: string | null;
  isHidden: (id: string) => boolean;
  /** Whether an edge (by key) is hidden by the certainty toggles. */
  isEdgeHidden: (key: string) => boolean;
  /** How many edges are drawn, exposed as `data-visible-edges`. */
  visibleEdges: number;
  onSelect: (id: string) => void;
  onDoubleClick: (id: string) => void;
  /** Called when the empty canvas is clicked. */
  onDeselect: () => void;
  /** Called with a node when the pointer enters it, and `null` when it leaves. */
  onHover?: (id: string | null) => void;
  /** When set, only these are drawn at full strength. */
  highlight?: Highlight | null;
  /** Overlay styling per node (colour, size, label marker); never moves nodes. */
  styleOf?: ((id: string) => NodeStyle | null) | null;
  /** Hide light edges and function labels when zoomed out (the main canvas). */
  semanticZoom?: boolean;
  /** Called with an edge's key when it is clicked. */
  onEdgeClick?: (key: string) => void;
  /** Called with the visible nodes inside a Shift-drag box. */
  onBoxSelect?: (ids: string[]) => void;
  /** Receives the renderer when it starts and `null` when it stops (for batched changes). */
  onRenderer?: (s: Sigma<N, E> | null) => void;
  testId?: string;
  /** Extra `data-*` attributes on the canvas element (for tests). */
  data?: Record<string, string | number>;
}

const FADED_NODE = '#e6e6e6';
const FADED_EDGE = '#f0f0f0';
const STRONG_EDGE = '#08519c';

/** Renders `graph`; click selects, double-click expands or loads neighbors.
 *  Example: `<GraphView graph={g} version={v} selected={id} … />`. */
export function GraphView<N extends NodeAttrs, E extends EdgeAttrs>({
  graph,
  version,
  selected,
  isHidden,
  isEdgeHidden,
  visibleEdges,
  onSelect,
  onDoubleClick,
  onDeselect,
  onHover,
  highlight = null,
  styleOf = null,
  semanticZoom = false,
  onRenderer,
  onBoxSelect,
  onEdgeClick,
  testId = 'canvas',
  data = {},
}: Props<N, E>) {
  const container = useRef<HTMLDivElement>(null);
  const sigma = useRef<Sigma<N, E> | null>(null);
  const handlers = useRef({ onSelect, onDoubleClick, onDeselect, onHover, isHidden, isEdgeHidden, selected, highlight, styleOf, semanticZoom });
  useLayoutEffect(() => {
    handlers.current = { onSelect, onDoubleClick, onDeselect, onHover, isHidden, isEdgeHidden, selected, highlight, styleOf, semanticZoom };
  });
  const rendererCallback = useRef(onRenderer);
  const boxCallback = useRef(onBoxSelect);
  const edgeCallback = useRef(onEdgeClick);
  useLayoutEffect(() => {
    rendererCallback.current = onRenderer;
    boxCallback.current = onBoxSelect;
    edgeCallback.current = onEdgeClick;
  });
  // The semantic zoom level; the reducers read it, and crossing a level redraws.
  const zoom = useRef<0 | 1 | 2>(0);
  // The node under the cursor; its label is never blanked.
  const hovered = useRef<string | null>(null);

  useEffect(() => {
    if (!container.current) return;
    const s = new Sigma<N, E>(graph, container.current, {
      allowInvalidContainer: true,
      renderEdgeLabels: false,
      // Edges are clickable when someone listens (their call sites).
      enableEdgeEvents: edgeCallback.current !== undefined,
      defaultEdgeType: 'arrow',
      labelRenderedSizeThreshold: 6,
      nodeReducer: (id, attrs) => {
        const h = handlers.current;
        const faded = h.highlight !== null && !h.highlight.nodes.has(id);
        const st = h.styleOf?.(id) ?? null;
        const quiet = h.semanticZoom && labelHiddenAtZoom(attrs.node, zoom.current) && id !== h.selected;
        return {
          ...attrs,
          ...(st?.color ? { color: st.color } : {}),
          ...(st?.size ? { size: st.size } : {}),
          label: shownLabel(attrs.label ?? '', { quiet, faded, hovered: id === hovered.current, mark: st?.mark, pinned: attrs.pinned }),
          hidden: h.isHidden(id),
          highlighted: id === h.selected || ((h.highlight?.strongNodes ?? h.highlight?.nodes)?.has(id) ?? false),
          ...(faded ? { color: FADED_NODE } : {}),
        };
      },
      edgeReducer: (key, attrs) => {
        const h = handlers.current;
        const faded = h.highlight !== null && !h.highlight.edges.has(key);
        const strong = h.highlight?.strongEdges?.has(key) ?? false;
        return {
          ...attrs,
          hidden: h.isEdgeHidden(key) || (h.semanticZoom && edgeHiddenAtZoom(attrs.edge, zoom.current)),
          ...(faded ? { color: FADED_EDGE } : {}),
          ...(strong ? { color: STRONG_EDGE, size: Math.max(attrs.size * 2, 3) } : {}),
        };
      },
    });
    const camera = s.getCamera();
    zoom.current = zoomLevel(camera.ratio);
    container.current.setAttribute('data-zoom-level', String(zoom.current));
    camera.on('updated', (state) => {
      const level = zoomLevel(state.ratio);
      if (level === zoom.current) return;
      zoom.current = level;
      container.current?.setAttribute('data-zoom-level', String(level));
      if (handlers.current.semanticZoom) s.refresh();
    });
    s.on('clickNode', ({ node }) => handlers.current.onSelect(node));
    s.on('clickEdge', ({ edge }) => edgeCallback.current?.(edge));
    s.on('doubleClickNode', (e) => {
      e.preventSigmaDefault();
      handlers.current.onDoubleClick(e.node);
    });
    s.on('clickStage', () => handlers.current.onDeselect());
    // Re-run the reducer for the one node so its label is back before Sigma
    // draws the hover.
    const relabel = (node: string) => {
      if (graph.hasNode(node)) s.refresh({ partialGraph: { nodes: [node] }, skipIndexation: true });
    };
    s.on('enterNode', ({ node }) => {
      hovered.current = node;
      relabel(node);
      handlers.current.onHover?.(node);
    });
    s.on('leaveNode', ({ node }) => {
      hovered.current = null;
      relabel(node);
      handlers.current.onHover?.(null);
    });
    // Shift-drag draws a selection box instead of moving the camera.
    const box = boxSelect(container.current, (a, b) => {
      const rect = normalize(a, b);
      const points = graph.filterNodes((id) => !handlers.current.isHidden(id)).map((id) => ({ id, ...s.graphToViewport(graph.getNodeAttributes(id)) }));
      boxCallback.current?.(inBox(points, rect));
    });
    // Panels can be resized or hidden, which changes the canvas size.
    const observer = new ResizeObserver(() => {
      s.resize();
      s.refresh();
    });
    observer.observe(container.current);
    sigma.current = s;
    rendererCallback.current?.(s);
    return () => {
      box();
      observer.disconnect();
      rendererCallback.current?.(null);
      s.kill();
      sigma.current = null;
    };
  }, [graph]);

  useEffect(() => {
    sigma.current?.refresh();
  }, [version, selected, highlight, styleOf]);

  return (
    <div
      ref={container}
      className="canvas"
      data-testid={testId}
      data-visible-edges={visibleEdges}
      data-highlighted={highlight ? highlight.nodes.size : 0}
      data-strong-edges={highlight?.strongEdges?.size ?? 0}
      {...Object.fromEntries(Object.entries(data).map(([k, v]) => [`data-${k}`, v]))}
    />
  );
}

/** Turns Shift-drag on `el` into a selection box: draws it, and calls
 *  `done` with the two corners (relative to `el`) on release. The camera
 *  never sees these drags. Returns the function that removes the listeners.
 *  Example: `const stop = boxSelect(div, (a, b) => …)`. */
function boxSelect(el: HTMLElement, done: (a: Point, b: Point) => void): () => void {
  let start: Point | null = null;
  let drawn: HTMLDivElement | null = null;
  const at = (e: MouseEvent): Point => {
    const r = el.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };
  const move = (e: MouseEvent) => {
    if (!start || !drawn) return;
    const b = normalize(start, at(e));
    Object.assign(drawn.style, { left: `${b.left}px`, top: `${b.top}px`, width: `${b.right - b.left}px`, height: `${b.bottom - b.top}px` });
  };
  const up = (e: MouseEvent) => {
    window.removeEventListener('mousemove', move);
    window.removeEventListener('mouseup', up);
    drawn?.remove();
    drawn = null;
    if (start) done(start, at(e));
    start = null;
  };
  const down = (e: MouseEvent) => {
    if (!e.shiftKey || e.button !== 0) return;
    e.stopImmediatePropagation();
    e.preventDefault();
    start = at(e);
    drawn = document.createElement('div');
    drawn.className = 'box-select';
    drawn.dataset.testid = 'box-select';
    el.appendChild(drawn);
    window.addEventListener('mousemove', move);
    window.addEventListener('mouseup', up);
  };
  el.addEventListener('mousedown', down, true);
  return () => el.removeEventListener('mousedown', down, true);
}
