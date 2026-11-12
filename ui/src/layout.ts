// Runs the force layout in the worker and applies the positions.

import type { CanvasGraph } from './model';
import type { LayoutRequest, LayoutResponse } from './layout.worker';

let worker: Worker | null = null;

/** Lays out `g` in a worker; calls `done` after positions are applied.
 *  Example: `layout(graph, () => sigma.refresh())`. */
export function layout(g: CanvasGraph, done: () => void): void {
  worker?.terminate();
  worker = new Worker(new URL('./layout.worker.ts', import.meta.url), { type: 'module' });
  const req: LayoutRequest = {
    nodes: g.mapNodes((id, a) => ({ id, x: a.x, y: a.y })),
    edges: g.mapEdges((_k, _a, s, t) => [s, t] as [string, string]),
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
