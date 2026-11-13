// Pure helpers for the crater view: its canvas graph, the "why is this
// here" path, checklist export, and library highlighting. Unit-tested
// without a browser.

import Graph from 'graphology';
import { certaintyColor, certaintyWidth, edgeKey, kindColor, labelFor } from './model';
import type { ChecklistRow, CraterResult, GEdge, GNode, LibraryUsage } from './types';

/** How a crater node is drawn. */
export type Role = 'seed' | 'pinned' | 'faint' | 'boundary' | 'plain';

export interface CraterNodeAttrs {
  label: string;
  x: number;
  y: number;
  size: number;
  color: string;
  node: GNode;
  role: Role;
}

export interface CraterEdgeAttrs {
  size: number;
  color: string;
  edge: GEdge;
}

export type CraterGraph = Graph<CraterNodeAttrs, CraterEdgeAttrs>;

const ROLE_COLORS: Record<Exclude<Role, 'plain'>, string> = {
  seed: '#08306b',
  pinned: '#e6550d',
  faint: '#d9d9d9',
  boundary: '#fd8d3c',
};

/** The role of each node: the pinned path first, then seeds, faint
 *  callers and boundary nodes.
 *  Example: a seed that is also on the pinned path is `pinned`. */
function roleOf(r: CraterResult, id: string): Role {
  if (r.pinned?.path.includes(id)) return 'pinned';
  if (r.seeds.includes(id)) return 'seed';
  if (r.pinned?.faint.includes(id)) return 'faint';
  if (r.boundary.includes(id)) return 'boundary';
  return 'plain';
}

/** The crater as a graph for the canvas. Boundary nodes get a `⇥` after
 *  their label; trace-observed overlay edges are drawn thick and orange.
 *  Positions start at 0 and come from the layout.
 *  Example: `craterCanvas(result).order` is `result.graph.nodes.length`. */
export function craterCanvas(r: CraterResult): CraterGraph {
  const g: CraterGraph = new Graph({ type: 'directed', multi: true });
  for (const n of r.graph.nodes) {
    if (g.hasNode(n.id)) continue;
    const role = roleOf(r, n.id);
    g.addNode(n.id, {
      label: role === 'boundary' ? `${labelFor(n)} ⇥` : labelFor(n),
      x: 0,
      y: 0,
      size: role === 'seed' || role === 'pinned' ? 7 : 4,
      color: role === 'plain' ? kindColor(n.kind) : ROLE_COLORS[role],
      node: n,
      role,
    });
  }
  // Overlay edges are in the reverse payload already; forward craters
  // carry them only in `pinned`.
  for (const e of [...r.graph.edges, ...(r.pinned?.overlay ?? [])]) {
    if (e.kind === 'contains' || !g.hasNode(e.from) || !g.hasNode(e.to)) continue;
    const key = edgeKey(e);
    if (g.hasEdge(key)) continue;
    const observed = e.reason === 'trace-observed';
    g.addDirectedEdgeWithKey(key, e.from, e.to, {
      size: observed ? 3 : certaintyWidth(e.certainty),
      color: observed ? ROLE_COLORS.pinned : certaintyColor(e.certainty),
      edge: e,
    });
  }
  return g;
}

/** `file:line:col`, or empty.
 *  Example: `where(row)` is `'src/a.clj:3:1'`. */
function where(row: ChecklistRow): string {
  return row.loc ? `${row.loc.file}:${row.loc.line}:${row.loc.col}` : '';
}

/** One CSV cell: quoted when needed, and with a leading `'` when it would
 *  start a spreadsheet formula (repository names are untrusted).
 *  Example: `csvCell('=1+1')` is `"'=1+1"`; `csvCell('a,b')` is `'"a,b"'`. */
function csvCell(s: string): string {
  const safe = /^[=+\-@\t\r]/.test(s) ? `'${s}` : s;
  return /[",\r\n]/.test(safe) ? `"${safe.replace(/"/g, '""')}"` : safe;
}

/** The checklist as CSV (RFC 4180, CRLF line ends).
 *  Example: first line `location,symbol,distance,certainty,coverage,entry_point,id`. */
export function checklistCsv(rows: ChecklistRow[]): string {
  const head = 'location,symbol,distance,certainty,coverage,entry_point,id';
  const body = rows.map((r) =>
    [where(r), r.name, String(r.distance), r.path_certainty, r.coverage ?? '', r.entry_point ? 'yes' : '', r.id].map(csvCell).join(','),
  );
  return [head, ...body].join('\r\n') + '\r\n';
}

/** One Markdown table cell: pipes escaped, newlines folded to spaces.
 *  Example: `mdCell('a|b\nc')` is `'a\\|b c'`. */
function mdCell(s: string): string {
  return s.replace(/\\/g, '\\\\').replace(/\|/g, '\\|').replace(/[\r\n]+/g, ' ');
}

/** The checklist as a Markdown table.
 *  Example: a row prints as ``| `src/a.clj:3:1` | a/f | 0 | certain |  | yes |``. */
export function checklistMarkdown(rows: ChecklistRow[]): string {
  const lines = ['| Location | Symbol | Distance | Certainty | Coverage | Entry point |', '|---|---|---|---|---|---|'];
  for (const r of rows) {
    const at = where(r);
    const loc = at ? `\`${at.replace(/`/g, '')}\`` : '';
    lines.push(`| ${loc} | ${mdCell(r.name)} | ${r.distance} | ${r.path_certainty} | ${r.coverage ?? ''} | ${r.entry_point ? 'yes' : ''} |`);
  }
  return lines.join('\n') + '\n';
}

/** The unit a Clojure namespace, symbol or impl ID belongs to.
 *  Example: `unitOf('clj:svc/web//web.views/page')` is `'unit:svc/web'`. */
export function unitOf(id: string): string | null {
  const m = /^clj:(.*?)\/\//.exec(id);
  return m ? `unit:${m[1]}` : null;
}

/** The namespace ID of a symbol or impl ID (a namespace ID is itself).
 *  Example: `namespaceOf('clj:.//a.b/f')` is `'clj:.//a.b'`. */
function namespaceOf(id: string): string | null {
  const at = id.indexOf('//');
  if (!id.startsWith('clj:') || at < 0) return null;
  const slash = id.indexOf('/', at + 2);
  return slash < 0 ? id : id.slice(0, slash);
}

/** The canvas nodes to highlight for a library: the using symbols, their
 *  namespaces and their units, whichever of those are on the canvas.
 *  Example: on a unit-level canvas, the units whose code uses the library. */
export function libraryHighlight(u: LibraryUsage, onCanvas: Iterable<string>): Set<string> {
  const users = new Set<string>();
  for (const g of [...u.groups, ...u.require_only]) {
    for (const row of g.rows) {
      users.add(row.from);
      const ns = namespaceOf(row.from);
      if (ns) users.add(ns);
      const unit = unitOf(row.from);
      if (unit) users.add(unit);
    }
  }
  const out = new Set<string>();
  for (const id of onCanvas) if (users.has(id)) out.add(id);
  return out;
}

const STRENGTH: Record<string, number> = { certain: 0, likely: 1, possible: 2 };

/** The crater's symbols and impls as checklist rows, strongest certainty
 *  first, then nearest, then by ID (the order `crater --checklist` uses).
 *  Example: for `a → b` seeded at `a`, rows `a` (0) then `b` (1). */
export function checklistRows(r: CraterResult): ChecklistRow[] {
  const nodes = new Map(r.graph.nodes.map((n) => [n.id, n]));
  const rows: ChecklistRow[] = [];
  for (const x of r.reach) {
    const n = nodes.get(x.id);
    if (!n || (n.kind !== 'symbol' && n.kind !== 'impl')) continue;
    rows.push({
      id: x.id,
      kind: n.kind,
      name: n.namespace ? `${n.namespace}/${n.name}` : n.name,
      loc: n.loc ?? null,
      distance: x.distance,
      path_certainty: x.path_certainty,
      coverage: null,
      entry_point: (n.flags ?? []).includes('entry-point'),
    });
  }
  return rows.sort(
    (a, b) =>
      (STRENGTH[a.path_certainty] ?? 3) - (STRENGTH[b.path_certainty] ?? 3) ||
      a.distance - b.distance ||
      (a.id < b.id ? -1 : a.id > b.id ? 1 : 0),
  );
}

/** What the hover highlights: everything on some path between the seed and
 *  a node, and the parts that lie on a shortest one. */
export interface PathSlice {
  nodes: Set<string>;
  edges: Set<string>;
  shortestNodes: Set<string>;
  shortestEdges: Set<string>;
}

/** Breadth-first distances from `starts` following `step`.
 *  Example: from `a` over `a → b → c`, `{a: 0, b: 1, c: 2}`. */
function distances(starts: string[], step: (n: string) => [string, string][]): Map<string, number> {
  const dist = new Map<string, number>(starts.map((s) => [s, 0]));
  const queue = [...starts];
  for (let i = 0; i < queue.length; i++) {
    const u = queue[i];
    if (u === undefined) break;
    const d = dist.get(u) ?? 0;
    for (const [, v] of step(u)) {
      if (!dist.has(v)) {
        dist.set(v, d + 1);
        queue.push(v);
      }
    }
  }
  return dist;
}

/** Every node and edge on a path from the seeds to `target` over the
 *  crater's edges (in a reverse crater, from `target` to the seeds), and
 *  those on a shortest such path. With cycles there are endlessly many
 *  paths, so this is their union: a node or edge is included when the seed
 *  reaches it and it reaches the target. A seed's own slice is the seed.
 *  Example: in `a → b → d`, `a → d`, the slice of `d` is `{a, b, d}` and the
 *  shortest part is `a → d`. */
export function allPaths(r: CraterResult, g: CraterGraph, target: string): PathSlice {
  const empty: PathSlice = { nodes: new Set(), edges: new Set(), shortestNodes: new Set(), shortestEdges: new Set() };
  if (!g.hasNode(target)) return empty;
  const seeds = r.seeds.filter((s) => g.hasNode(s));
  if (seeds.includes(target)) {
    return { nodes: new Set([target]), edges: new Set(), shortestNodes: new Set([target]), shortestEdges: new Set() };
  }
  const side = r.reach.find((x) => x.id === target)?.direction ?? r.direction;
  const reverse = side === 'reverse';
  const forwardStep = (n: string): [string, string][] => g.outEdges(n).map((k) => [k, g.target(k)]);
  const backwardStep = (n: string): [string, string][] => g.inEdges(n).map((k) => [k, g.source(k)]);
  const next = reverse ? backwardStep : forwardStep;
  const prev = reverse ? forwardStep : backwardStep;
  const fromSeed = distances(seeds, next);
  const toTarget = distances([target], prev);
  const total = fromSeed.get(target);
  if (total === undefined) return empty;
  const slice: PathSlice = { nodes: new Set(), edges: new Set(), shortestNodes: new Set(), shortestEdges: new Set() };
  for (const [u, du] of fromSeed) {
    const tu = toTarget.get(u);
    if (tu === undefined) continue;
    slice.nodes.add(u);
    if (du + tu === total) slice.shortestNodes.add(u);
    for (const [k, v] of next(u)) {
      const tv = toTarget.get(v);
      if (tv === undefined) continue;
      slice.edges.add(k);
      if (du + 1 + tv === total) slice.shortestEdges.add(k);
    }
  }
  return slice;
}
