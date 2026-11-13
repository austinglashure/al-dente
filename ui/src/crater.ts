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

/** The nodes from `id` back to a seed along `via`, and the canvas edges
 *  between consecutive ones (either direction: in a reverse crater the
 *  edges point toward the seed). Empty for a node not in the crater.
 *  Example: in `a → b → c` seeded at `a`, `whyPath(r, g, 'c').nodes` is `['c', 'b', 'a']`. */
export function whyPath(r: CraterResult, g: CraterGraph, id: string): { nodes: string[]; edges: string[] } {
  const via = new Map(r.reach.map((x) => [x.id, x.via]));
  if (!via.has(id)) return { nodes: [], edges: [] };
  const nodes: string[] = [];
  const edges: string[] = [];
  let cur: string | null | undefined = id;
  while (cur && !nodes.includes(cur)) {
    nodes.push(cur);
    const next: string | null | undefined = via.get(cur);
    if (next && g.hasNode(cur) && g.hasNode(next)) {
      edges.push(...g.outEdges(next, cur), ...g.outEdges(cur, next));
    }
    cur = next;
  }
  return { nodes, edges };
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
