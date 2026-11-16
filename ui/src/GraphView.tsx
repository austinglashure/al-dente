// The Sigma.js canvas. Repository strings reach it only as node labels,
// which Sigma draws on a canvas, never as HTML.

import { useEffect, useLayoutEffect, useRef } from 'react';
import type Graph from 'graphology';
import Sigma from 'sigma';
import { edgeHiddenAtZoom, labelHiddenAtZoom, zoomLevel } from './expansion';
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
  version: number;
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
  useLayoutEffect(() => {
    rendererCallback.current = onRenderer;
  });
  // The semantic zoom level; the reducers read it, and crossing a level redraws.
  const zoom = useRef<0 | 1 | 2>(0);

  useEffect(() => {
    if (!container.current) return;
    const s = new Sigma<N, E>(graph, container.current, {
      allowInvalidContainer: true,
      renderEdgeLabels: false,
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
          ...(st?.mark ? { label: `${st.mark}${attrs.label}` } : {}),
          ...(quiet ? { label: '' } : {}),
          hidden: h.isHidden(id),
          highlighted: id === h.selected || ((h.highlight?.strongNodes ?? h.highlight?.nodes)?.has(id) ?? false),
          ...(faded ? { color: FADED_NODE, label: '' } : {}),
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
    s.on('doubleClickNode', (e) => {
      e.preventSigmaDefault();
      handlers.current.onDoubleClick(e.node);
    });
    s.on('clickStage', () => handlers.current.onDeselect());
    s.on('enterNode', ({ node }) => handlers.current.onHover?.(node));
    s.on('leaveNode', () => handlers.current.onHover?.(null));
    // Panels can be resized or hidden, which changes the canvas size.
    const observer = new ResizeObserver(() => {
      s.resize();
      s.refresh();
    });
    observer.observe(container.current);
    sigma.current = s;
    rendererCallback.current?.(s);
    return () => {
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
