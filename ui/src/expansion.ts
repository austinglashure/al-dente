// Aggregates expanded in place on the canvas: which pages are loaded, the
// edges the aggregate had before (restored on collapse), and how far the
// camera is zoomed out. Pure functions over the graphology graph, tested
// without a browser.

import { dropNodes, isAggregate, labelFor, mergePayload, newGraph, type CanvasGraph } from './model';
import type { GEdge, GNode, Payload } from './types';

export { clusterSize } from './model';

/** One aggregate expanded in place. */
export interface Expansion {
  /** The expansion this aggregate was loaded by, if any (collapsed with it). */
  parent: string | null;
  /** Children loaded so far, in page order. */
  loaded: string[];
  /** Interop exits and holes the pages brought in. */
  extras: string[];
  /** How many children the aggregate has. */
  total: number;
  /** The aggregate's edges before it was expanded. */
  stash: GEdge[];
}

/** Expanded aggregates by ID. */
export type Expansions = Map<string, Expansion>;

const isLeaf = (n: GNode) => n.kind === 'platform' || n.kind === 'hole';

/** The aggregate's label while some children are not loaded.
 *  Example: `restLabel({name: 'n', kind: 'namespace'}, 2)` is `'n (+2 more)'`. */
function restLabel(n: GNode, left: number): string {
  return left > 0 ? `${labelFor(n)} (+${left.toLocaleString('en-US')} more)` : labelFor(n);
}

/** Adds one `/expand` page for `id` to the canvas: children start at the
 *  aggregate's position; the aggregate's edges are replaced by the page's
 *  (which roll the children not loaded yet into it). With children left the
 *  aggregate stays as "+N more"; otherwise it is hidden. Returns the IDs of
 *  the nodes added.
 *  Example: a first page of 1 child out of 3 adds that child and relabels the aggregate `n (+2 more)`. */
export function applyPage(g: CanvasGraph, ex: Expansions, id: string, page: Payload, random: () => number = Math.random): string[] {
  if (!g.hasNode(id)) return [];
  let record = ex.get(id);
  if (!record) {
    const parent = [...ex.entries()].find(([, r]) => r.loaded.includes(id))?.[0] ?? null;
    record = { parent, loaded: [], extras: [], total: g.getNodeAttribute(id, 'node').members ?? 0, stash: g.edges(id).map((k) => g.getEdgeAttribute(k, 'edge')) };
    ex.set(id, record);
  }
  for (const k of g.edges(id)) g.dropEdge(k);
  const rest = page.nodes.find((n) => n.id === id);
  if (rest?.members !== undefined) record.total = rest.members;
  const added = mergePayload(g, { ...page, nodes: page.nodes.filter((n) => n.id !== id) }, id, random);
  for (const a of added) {
    if (isLeaf(g.getNodeAttribute(a, 'node'))) record.extras.push(a);
    else record.loaded.push(a);
  }
  const node = g.getNodeAttribute(id, 'node');
  g.mergeNodeAttributes(id, { expanded: rest === undefined, label: restLabel(node, rest ? remaining(ex, id) : 0) });
  return added;
}

/** Collapses an expanded aggregate: first the aggregates expanded inside
 *  it, then its children, the interop exits and holes nothing else uses,
 *  and its page edges; its earlier edges come back where both ends remain.
 *  Example: collapsing a fully expanded namespace leaves it with its old edges. */
export function collapse(g: CanvasGraph, ex: Expansions, id: string): void {
  const record = ex.get(id);
  if (!record) return;
  for (const [child, r] of [...ex.entries()]) if (r.parent === id) collapse(g, ex, child);
  dropNodes(g, record.loaded);
  dropNodes(g, record.extras.filter((x) => g.hasNode(x) && g.degree(x) === 0));
  ex.delete(id);
  if (!g.hasNode(id)) return;
  for (const k of g.edges(id)) g.dropEdge(k);
  g.mergeNodeAttributes(id, { expanded: false, label: labelFor(g.getNodeAttribute(id, 'node')) });
  mergePayload(g, { nodes: [], edges: record.stash, truncated: false, aggregated: null });
}

/** The `offset` of the next page for an expanded aggregate.
 *  Example: after one page of 200, `nextOffset(ex, id)` is 200. */
export function nextOffset(ex: Expansions, id: string): number {
  return ex.get(id)?.loaded.length ?? 0;
}

/** How many children of an expanded aggregate are not loaded yet.
 *  Example: 3 children, 1 loaded: 2. */
export function remaining(ex: Expansions, id: string): number {
  const r = ex.get(id);
  return r ? Math.max(r.total - r.loaded.length, 0) : 0;
}

/** Semantic zoom level from Sigma's camera ratio (larger is further out):
 *  0 close, 1 middle, 2 far.
 *  Example: `zoomLevel(6)` is 2. */
export function zoomLevel(ratio: number): 0 | 1 | 2 {
  return ratio < 1.5 ? 0 : ratio < 4 ? 1 : 2;
}

/** Whether an edge is too light to draw at a zoom level: `possible` edges
 *  from the middle level, and edges standing for fewer than 3 calls when far.
 *  Example: a single certain call is drawn at level 1, hidden at level 2. */
export function edgeHiddenAtZoom(e: GEdge, level: 0 | 1 | 2): boolean {
  if (level === 0) return false;
  if (e.certainty === 'possible') return true;
  return level === 2 && (e.count ?? 1) < 3;
}

/** Whether a node's label is hidden at a zoom level: only far out, and
 *  never for aggregates or holes.
 *  Example: a function's label is hidden at level 2; a namespace's is not. */
export function labelHiddenAtZoom(n: GNode, level: 0 | 1 | 2): boolean {
  return level === 2 && !isAggregate(n) && n.kind !== 'hole';
}

/** What `batched` needs from the renderer (Sigma's `setGraph`). */
export interface Attachable {
  setGraph: (graph: CanvasGraph) => void;
}

const placeholder: CanvasGraph = newGraph();

/** Runs `change` with the renderer detached from `g`, then reattaches it:
 *  one full redraw instead of one per dropped node or edge (Sigma 3.0.3
 *  re-indexes every node on each drop event).
 *  Example: `batched(sigma, graph, () => collapse(graph, ex, id))`. */
export function batched(renderer: Attachable | null, g: CanvasGraph, change: () => void): void {
  if (!renderer) {
    change();
    return;
  }
  renderer.setGraph(placeholder);
  try {
    change();
  } finally {
    renderer.setGraph(g);
  }
}
