// Force layout off the main thread. A worker file of our own (not
// graphology's blob worker) so the CSP's `default-src 'self'` allows it.

import Graph from 'graphology';
import forceAtlas2 from 'graphology-layout-forceatlas2';

export interface LayoutRequest {
  nodes: { id: string; x: number; y: number }[];
  edges: [string, string][];
  iterations: number;
}

export type LayoutResponse = Record<string, { x: number; y: number }>;

self.onmessage = (ev: MessageEvent<LayoutRequest>) => {
  const g = new Graph({ type: 'directed', multi: true });
  for (const n of ev.data.nodes) g.addNode(n.id, { x: n.x, y: n.y });
  for (const [a, b] of ev.data.edges) if (g.hasNode(a) && g.hasNode(b)) g.addEdge(a, b);
  const settings = { ...forceAtlas2.inferSettings(g), barnesHutOptimize: g.order > 500 };
  const positions: LayoutResponse = forceAtlas2(g, { iterations: ev.data.iterations, settings });
  self.postMessage(positions);
};
