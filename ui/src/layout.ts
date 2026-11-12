// Runs the force layout in the worker and applies the positions.

import type { CanvasGraph } from './model';
import type { LayoutRequest, LayoutResponse } from './layout.worker';

let worker: Worker | null = null;

/** Lays out `g` in a worker using only the edges `edgeFilter` keeps (hidden
 *  certainty levels do not pull nodes together); calls `done` afterwards.
 *  Example: `layout(graph, () => sigma.refresh(), (k) => shown(k))`. */
export function layout(g: CanvasGraph, done: () => void, edgeFilter: (key: string) => boolean = () => true): void {
  worker?.terminate();
  worker = new Worker(new URL('./layout.worker.ts', import.meta.url), { type: 'module' });
  const req: LayoutRequest = {
    nodes: g.mapNodes((id, a) => ({ id, x: a.x, y: a.y })),
    edges: g.filterEdges((k) => edgeFilter(k)).map((k) => [g.source(k), g.target(k)] as [string, string]),
    iterations: g.order > 1000 ? 60 : 200,
  };
  worker.onmessage = (ev: MessageEvent<LayoutResponse>) => {
    for (const [id, p] of Object.entries(ev.data)) {
      if (g.hasNode(id)) g.mergeNodeAttributes(id, p);
    }
    done();
  };
  worker.postMessage(req);
}
