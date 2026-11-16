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
  /** An aggregate expanded in place with nothing left to show: hidden. */
  expanded?: boolean;
  /** Pinned by the user: layouts never move it. */
  pinned?: boolean;
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
  directory: '#807dba',
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
 *  Example: `certaintyColor('possible')` is `'#bdbdbd'`. */
export function certaintyColor(certainty: string | undefined): string {
  switch (certainty) {
    case 'possible':
      return '#bdbdbd';
    case 'likely':
      return '#6baed6';
    default:
      return '#252525';
  }
}

/** Base line width by certainty: certain thickest, possible thinnest.
 *  Example: `certaintyWidth('possible')` is `0.5`. */
export function certaintyWidth(certainty: string | undefined): number {
  return certainty === 'possible' ? 0.5 : certainty === 'likely' ? 1 : 1.6;
}

/** The certainty levels shown; edges without a certainty are always shown. */
export type Levels = Record<'certain' | 'likely' | 'possible', boolean>;

/** Whether an edge is shown at these levels.
 *  Example: `edgeShown({certainty: 'possible'} as GEdge, {certain: true, likely: true, possible: false})` is `false`. */
export function edgeShown(e: GEdge, levels: Levels): boolean {
  const c = e.certainty as keyof Levels | undefined;
  return c === undefined || levels[c] !== false;
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

/** Whether a node is the Clojure or ClojureScript core library
 *  (`clojure.core`, `cljs.core`, their functions and macros), not other
 *  `clojure.*` namespaces.
 *  Example: `ext:clj//clojure.core/let` is; `ext:clj//clojure.string/join` is not. */
export function isCore(n: GNode): boolean {
  return n.kind === 'external' && /^ext:[a-z]+\/\/(clojure|cljs)\.core(\/|$)/.test(n.id);
}

/** Which kinds of node the toggles hide. */
export interface HideOptions {
  hideTests: boolean;
  hideExternals: boolean;
  /** Platform nodes: Java and JavaScript interop exits. */
  hideInterop?: boolean;
  /** `clojure.core` and `cljs.core` (`let`, `or`, `map`, …). */
  hideCore?: boolean;
}

/** Whether a node is hidden by the toggles.
 *  Example: `hidden(externalNode, { hideTests: false, hideExternals: true })` is `true`. */
export function hidden(n: GNode, opts: HideOptions): boolean {
  return (
    (opts.hideExternals && n.kind === 'external') ||
    (opts.hideTests && isTest(n)) ||
    (opts.hideInterop === true && n.kind === 'platform') ||
    (opts.hideCore === true && isCore(n))
  );
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
      label: labelFor(n),
      x: base.x + (random() - 0.5) * 10,
      y: base.y + (random() - 0.5) * 10,
      size: clusterSize(n),
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
      size: certaintyWidth(e.certainty) * (e.count ? Math.min(1 + Math.log2(e.count), 5) : 1),
      color: certaintyColor(e.certainty),
      edge: e,
    });
  }
  return added;
}

/** Drawn size of a node: aggregates grow with the log of their member
 *  count (capped), everything else by kind.
 *  Example: `clusterSize({kind: 'namespace', members: 0})` is 8; a symbol is 4. */
export function clusterSize(n: GNode): number {
  const base = n.kind === 'unit' || n.kind === 'directory' ? 9 : n.kind === 'namespace' ? 8 : n.kind === 'hole' ? 7 : 4;
  if (n.members === undefined || !isAggregate(n)) return base;
  return Math.min(base + 1.5 * Math.log2(1 + n.members), 24);
}

/** Removes nodes (and their edges) from the canvas.
 *  Example: `dropNodes(g, ['a'])` removes `a` if present. */
export function dropNodes(g: CanvasGraph, ids: string[]): void {
  for (const id of ids) {
    if (g.hasNode(id)) g.dropNode(id);
  }
}

/** The label shown for a node: units by their last directory (`(root)` for
 *  the repository root), everything else by name.
 *  Example: `labelFor({kind: 'unit', name: 'modules/reitit-core'})` is `'reitit-core'`. */
export function labelFor(n: GNode): string {
  if (n.kind === 'hole') return `⚠ ${n.name}`;
  if (n.kind === 'directory') return `${n.name}/`;
  if (n.kind !== 'unit') return n.name;
  if (n.name === '.') return '(root)';
  return n.name.split('/').pop() ?? n.name;
}

/** Whether double-clicking a node expands it (aggregates) rather than
 *  loading its neighborhood.
 *  Example: `isAggregate({kind: 'namespace'} as GNode)` is `true`. */
export function isAggregate(n: GNode): boolean {
  return n.kind === 'unit' || n.kind === 'namespace' || n.kind === 'directory';
}

/** Whether a node ID can have source to show: symbols, impls and holes do;
 *  units, namespaces, externals, libraries and platform nodes do not. Holes
 *  have a location; a namespace ID has no `/` after its `//`.
 *  Example: `hasSource('clj:.//shop.core/checkout')` is `true`;
 *  `hasSource('clj:.//shop.core')` is `false`. */
export function hasSource(id: string): boolean {
  if (id.startsWith('hole:')) return true;
  if (/^(unit|ext|java|js|lib|repo):/.test(id)) return false;
  const at = id.indexOf('//');
  return at >= 0 && id.slice(at + 2).includes('/');
}
