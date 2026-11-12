// The canvas graph: merging API payloads into a graphology graph, and what
// is hidden by the toggles. Pure functions, unit-tested without a browser.

import Graph from 'graphology';
import type { GEdge, GNode, Payload } from './types';

export interface NodeAttrs {
  label: string;
  x: number;
  y: number;
  size: number;
  color: string;
  node: GNode;
}

export interface EdgeAttrs {
  size: number;
  color: string;
  edge: GEdge;
}

export type CanvasGraph = Graph<NodeAttrs, EdgeAttrs>;

/** A new, empty canvas graph (directed, parallel edges allowed).
 *  Example: `newGraph().order` is 0. */
export function newGraph(): CanvasGraph {
  return new Graph<NodeAttrs, EdgeAttrs>({ type: 'directed', multi: true });
}

const KIND_COLORS: Record<string, string> = {
  unit: '#6a51a3',
  namespace: '#2171b5',
  symbol: '#238b45',
  impl: '#41ab5d',
  external: '#969696',
  platform: '#d94801',
  hole: '#cb181d',
};

/** Node color by kind.
 *  Example: `kindColor('namespace')` is `'#2171b5'`. */
export function kindColor(kind: string): string {
  return KIND_COLORS[kind] ?? '#525252';
}

/** Edge color by certainty: certain dark, likely medium, possible light.
 *  Example: `certaintyColor('possible')` is `'#d9d9d9'`. */
export function certaintyColor(certainty: string | undefined): string {
  switch (certainty) {
    case 'possible':
      return '#d9d9d9';
    case 'likely':
      return '#969696';
    default:
      return '#525252';
  }
}

/** A stable key for an edge.
 *  Example: `edgeKey({from:'a', to:'b', kind:'calls'})` is `'a|calls|b||'`. */
export function edgeKey(e: GEdge): string {
  return `${e.from}|${e.kind}|${e.to}|${e.reason ?? ''}|${e.lang ?? ''}`;
}

/** Whether a node counts as test code.
 *  Example: a node with flag `test`, subkind `test`, or a name ending in `-test` is. */
export function isTest(n: GNode): boolean {
  return (n.flags ?? []).includes('test') || n.subkind === 'test' || n.name.endsWith('-test') || (n.namespace ?? '').endsWith('-test');
}

/** Whether a node is hidden by the toggles.
 *  Example: `hidden(externalNode, { hideTests: false, hideExternals: true })` is `true`. */
export function hidden(n: GNode, opts: { hideTests: boolean; hideExternals: boolean }): boolean {
  return (opts.hideExternals && n.kind === 'external') || (opts.hideTests && isTest(n));
}

/** Adds a payload's nodes and edges that are not on the canvas yet; new
 *  nodes start near `anchor` when given. Returns the IDs of added nodes.
 *  `contains` edges are not drawn. Edges to nodes not on the canvas are skipped.
 *  Example: merging `{nodes:[a,b], edges:[a→b]}` into an empty graph adds 2 nodes and 1 edge. */
export function mergePayload(g: CanvasGraph, p: Payload, anchor?: string, random: () => number = Math.random): string[] {
  const added: string[] = [];
  const base = anchor && g.hasNode(anchor) ? g.getNodeAttributes(anchor) : { x: 0, y: 0 };
  for (const n of p.nodes) {
    if (g.hasNode(n.id)) continue;
    g.addNode(n.id, {
      label: n.name,
      x: base.x + (random() - 0.5) * 10,
      y: base.y + (random() - 0.5) * 10,
      size: n.kind === 'unit' || n.kind === 'namespace' ? 8 : 4,
      color: kindColor(n.kind),
      node: n,
    });
    added.push(n.id);
  }
  for (const e of p.edges) {
    if (e.kind === 'contains' || !g.hasNode(e.from) || !g.hasNode(e.to)) continue;
    const key = edgeKey(e);
    if (g.hasEdge(key)) continue;
    g.addDirectedEdgeWithKey(key, e.from, e.to, {
      size: e.count ? Math.min(1 + Math.log2(e.count), 6) : 1,
      color: certaintyColor(e.certainty),
      edge: e,
    });
  }
  return added;
}

/** Removes nodes (and their edges) from the canvas.
 *  Example: `dropNodes(g, ['a'])` removes `a` if present. */
export function dropNodes(g: CanvasGraph, ids: string[]): void {
  for (const id of ids) {
    if (g.hasNode(id)) g.dropNode(id);
  }
}

/** Whether double-clicking a node expands it (aggregates) rather than
 *  loading its neighborhood.
 *  Example: `isAggregate({kind: 'namespace'} as GNode)` is `true`. */
export function isAggregate(n: GNode): boolean {
  return n.kind === 'unit' || n.kind === 'namespace';
}
