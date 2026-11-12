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
  onSelect: (id: string) => void;
  onDoubleClick: (id: string) => void;
}

/** Renders `graph`; click selects, double-click expands or loads neighbors.
 *  Example: `<GraphView graph={g} version={v} selected={id} … />`. */
export function GraphView({ graph, version, selected, isHidden, onSelect, onDoubleClick }: Props) {
  const container = useRef<HTMLDivElement>(null);
  const sigma = useRef<Sigma<NodeAttrs, EdgeAttrs> | null>(null);
  const handlers = useRef({ onSelect, onDoubleClick, isHidden, selected });
  useLayoutEffect(() => {
    handlers.current = { onSelect, onDoubleClick, isHidden, selected };
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
      edgeReducer: (_id, attrs) => attrs,
    });
    s.on('clickNode', ({ node }) => handlers.current.onSelect(node));
    s.on('doubleClickNode', (e) => {
      e.preventSigmaDefault();
      handlers.current.onDoubleClick(e.node);
    });
    sigma.current = s;
    return () => {
      s.kill();
      sigma.current = null;
    };
  }, [graph]);

  useEffect(() => {
    sigma.current?.refresh();
  }, [version, selected]);

  return <div ref={container} className="canvas" data-testid="canvas" />;
}
