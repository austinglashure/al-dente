// The Sigma.js canvas. Repository strings reach it only as node labels,
// which Sigma draws on a canvas, never as HTML.

import { useEffect, useLayoutEffect, useRef } from 'react';
import type Graph from 'graphology';
import Sigma from 'sigma';
import type { EdgeAttrs, NodeAttrs } from './model';

/** Nodes and edges to emphasize; everything else is drawn faded. */
export interface Highlight {
  nodes: Set<string>;
  edges: Set<string>;
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
  testId?: string;
}

const FADED_NODE = '#e6e6e6';
const FADED_EDGE = '#f0f0f0';

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
  testId = 'canvas',
}: Props<N, E>) {
  const container = useRef<HTMLDivElement>(null);
  const sigma = useRef<Sigma<N, E> | null>(null);
  const handlers = useRef({ onSelect, onDoubleClick, onDeselect, onHover, isHidden, isEdgeHidden, selected, highlight });
  useLayoutEffect(() => {
    handlers.current = { onSelect, onDoubleClick, onDeselect, onHover, isHidden, isEdgeHidden, selected, highlight };
  });

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
        return {
          ...attrs,
          hidden: h.isHidden(id),
          highlighted: id === h.selected || (h.highlight?.nodes.has(id) ?? false),
          ...(faded ? { color: FADED_NODE, label: '' } : {}),
        };
      },
      edgeReducer: (key, attrs) => {
        const h = handlers.current;
        const faded = h.highlight !== null && !h.highlight.edges.has(key);
        return { ...attrs, hidden: h.isEdgeHidden(key), ...(faded ? { color: FADED_EDGE } : {}) };
      },
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
    return () => {
      observer.disconnect();
      s.kill();
      sigma.current = null;
    };
  }, [graph]);

  useEffect(() => {
    sigma.current?.refresh();
  }, [version, selected, highlight]);

  return (
    <div
      ref={container}
      className="canvas"
      data-testid={testId}
      data-visible-edges={visibleEdges}
      data-highlighted={highlight ? highlight.nodes.size : 0}
    />
  );
}
