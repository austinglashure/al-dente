// Runs the force layout in the worker and applies the positions.

import type { CanvasGraph } from './model';
import type { LayoutRequest, LayoutResponse } from './layout.worker';

let worker: Worker | null = null;
let runs = 0;

/** How many layouts have started in this page (overlays must not add any).
 *  Example: `layoutRuns()` is 1 after the first canvas loads. */
export function layoutRuns(): number {
  return runs;
}

/** Whether a layout is running (its positions are not applied yet).
 *  Example: right after `layout(…)`, `layoutPending()` is true. */
export function layoutPending(): boolean {
  return worker !== null;
}

/** Stops a layout in progress (its positions are never applied).
 *  Example: after replaying a workspace, `cancelLayout()` before applying its positions. */
export function cancelLayout(): void {
  worker?.terminate();
  worker = null;
}

/** Lays out `g` in a worker using only the edges `edgeFilter` keeps (hidden
 *  certainty levels do not pull nodes together); calls `done` afterwards.
 *  With `free`, only those nodes move: the rest stay where they are, so the
 *  picture does not jump when nodes are added.
 *  Example: `layout(graph, () => sigma.refresh(), (k) => shown(k), new Set(added))`. */
export function layout(g: CanvasGraph, done: () => void, edgeFilter: (key: string) => boolean = () => true, free?: Set<string>): void {
  runs += 1;
  worker?.terminate();
  worker = new Worker(new URL('./layout.worker.ts', import.meta.url), { type: 'module' });
  const req: LayoutRequest = {
    nodes: g.mapNodes((id, a) => ({ id, x: a.x, y: a.y, fixed: a.pinned === true || (free !== undefined && !free.has(id)) })),
    edges: g.filterEdges((k) => edgeFilter(k)).map((k) => [g.source(k), g.target(k)] as [string, string]),
    iterations: g.order > 1000 ? 60 : 200,
  };
  worker.onmessage = (ev: MessageEvent<LayoutResponse>) => {
    // One bulk update (one renderer refresh), not one event per node.
    const moved = ev.data;
    g.updateEachNodeAttributes((id, a) => (moved[id] ? { ...a, x: moved[id].x, y: moved[id].y } : a), { attributes: ['x', 'y'] });
    worker?.terminate();
    worker = null;
    done();
  };
  worker.postMessage(req);
}
