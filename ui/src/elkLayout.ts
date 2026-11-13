// Runs the layered (ELK) layout for craters in a web worker and applies the
// positions. elkjs's own worker script does the work; the API object here
// only posts the graph and receives the result, so the page stays
// responsive. elkjs is used under its GPL-3.0-or-later option (it is
// dual-licensed with EPL-2.0 from 0.12.0).

import * as elkApi from 'elkjs/lib/elk-api';
import ElkWorker from 'elkjs/lib/elk-worker.min.js?worker';
import type { ELK as ElkInstance } from 'elkjs/lib/elk-api';
import type { CraterGraph } from './crater';

type ElkConstructor = new (options: { workerFactory: () => Worker }) => ElkInstance;

/** The constructor from elkjs's CommonJS module: depending on how the
 *  bundler wraps it, the module's default export or that export's own
 *  `default`.
 *  Example: `new (elkConstructor(elkApi))({ workerFactory })`. */
function elkConstructor(m: unknown): ElkConstructor {
  let c: unknown = m;
  for (let i = 0; i < 3 && typeof c !== 'function' && c !== null && typeof c === 'object' && 'default' in c; i++) {
    c = (c as { default: unknown }).default;
  }
  if (typeof c !== 'function') throw new Error('elkjs did not load');
  return c as ElkConstructor;
}

let current: ElkInstance | null = null;

/** A node's box for ELK: wide enough for its label.
 *  Example: a 10-character label gives a 90 × 22 box. */
export function nodeBox(label: string): { width: number; height: number } {
  return { width: Math.min(20 + label.length * 7, 260), height: 22 };
}

/** ELK options; large graphs trade some quality for speed.
 *  Example: `options('RIGHT', 10)['elk.algorithm']` is `'layered'`. */
export function options(direction: string, size: number): Record<string, string> {
  const base: Record<string, string> = {
    'elk.algorithm': 'layered',
    'elk.direction': direction,
    'elk.spacing.nodeNode': '18',
    'elk.layered.spacing.nodeNodeBetweenLayers': '70',
    'elk.edgeRouting': 'POLYLINE',
  };
  if (size > 500) {
    base['elk.layered.thoroughness'] = '1';
    base['elk.layered.nodePlacement.strategy'] = 'SIMPLE';
  }
  return base;
}

/** Lays out `g` left to right (`RIGHT`) or right to left (`LEFT`) in a
 *  worker; resolves once the positions are applied (ELK's y grows down,
 *  Sigma's up, so y is flipped). A newer call stops an older one's worker.
 *  Example: `await elkLayout(graph, 'RIGHT')`. */
export async function elkLayout(g: CraterGraph, direction: 'RIGHT' | 'LEFT'): Promise<void> {
  current?.terminateWorker();
  const Elk = elkConstructor(elkApi);
  const elk = new Elk({ workerFactory: () => new ElkWorker() });
  current = elk;
  const edges = g
    .mapEdges((_k, _a, source, target) => ({ source, target }))
    .filter((e) => e.source !== e.target)
    .map((e, i) => ({ id: `e${i}`, sources: [e.source], targets: [e.target] }));
  try {
    const out = await elk.layout({
      id: 'root',
      layoutOptions: options(direction, g.order),
      children: g.mapNodes((id, a) => ({ id, ...nodeBox(a.label) })),
      edges,
    });
    for (const c of out.children ?? []) {
      if (g.hasNode(c.id)) g.mergeNodeAttributes(c.id, { x: c.x ?? 0, y: -(c.y ?? 0) });
    }
  } finally {
    elk.terminateWorker();
    if (current === elk) current = null;
  }
}
