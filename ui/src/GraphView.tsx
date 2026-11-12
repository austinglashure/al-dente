// The Sigma.js canvas. Repository strings reach it only as node labels,
// which Sigma draws on a canvas, never as HTML.

import { useEffect, useLayoutEffect, useRef } from 'react';
import Sigma from 'sigma';
import type { CanvasGraph, EdgeAttrs, NodeAttrs } from './model';

interface Props {
  graph: CanvasGraph;
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
}

/** Renders `graph`; click selects, double-click expands or loads neighbors.
 *  Example: `<GraphView graph={g} version={v} selected={id} … />`. */
export function GraphView({ graph, version, selected, isHidden, isEdgeHidden, visibleEdges, onSelect, onDoubleClick, onDeselect }: Props) {
  const container = useRef<HTMLDivElement>(null);
  const sigma = useRef<Sigma<NodeAttrs, EdgeAttrs> | null>(null);
  const handlers = useRef({ onSelect, onDoubleClick, onDeselect, isHidden, isEdgeHidden, selected });
  useLayoutEffect(() => {
    handlers.current = { onSelect, onDoubleClick, onDeselect, isHidden, isEdgeHidden, selected };
  });

  useEffect(() => {
    if (!container.current) return;
    const s = new Sigma<NodeAttrs, EdgeAttrs>(graph, container.current, {
      allowInvalidContainer: true,
      renderEdgeLabels: false,
      defaultEdgeType: 'arrow',
      labelRenderedSizeThreshold: 6,
      nodeReducer: (id, attrs) => {
        const h = handlers.current;
        return { ...attrs, hidden: h.isHidden(id), highlighted: id === h.selected };
      },
      edgeReducer: (key, attrs) => ({ ...attrs, hidden: handlers.current.isEdgeHidden(key) }),
    });
    s.on('clickNode', ({ node }) => handlers.current.onSelect(node));
    s.on('doubleClickNode', (e) => {
      e.preventSigmaDefault();
      handlers.current.onDoubleClick(e.node);
    });
    s.on('clickStage', () => handlers.current.onDeselect());
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
  }, [version, selected]);

  return <div ref={container} className="canvas" data-testid="canvas" data-visible-edges={visibleEdges} />;
}
