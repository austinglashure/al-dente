// Talking to the local API. The session token comes from the URL the CLI
// opened (`/?token=…`); it is kept in sessionStorage, removed from the
// address bar, and sent as a header on every API call.

import type { RankingReport, RankLevel } from './rankings';
import type {
  BlindspotReport,
  Checklist,
  ChurnReport,
  CoverageReport,
  CyclesReport,
  Defaults,
  HotspotsReport,
  MetricsReport,
  CraterResult,
  Envelope,
  FindResult,
  LibrariesReport,
  LibraryUsage,
  OpenResult,
  Payload,
  UnitLinks,
  ShowResult,
  SnapshotOverview,
  TraceReport,
  TreeDir,
} from './types';

const TOKEN_KEY = 'aldente-token';

/** Reads the token from `search` (preferred) or `stored`.
 *  Example: `tokenFrom('?token=abc', null)` is `'abc'`. */
export function tokenFrom(search: string, stored: string | null): string {
  return new URLSearchParams(search).get('token') ?? stored ?? '';
}

/** Takes the token from the page URL, stores it, and strips it from the address bar.
 *  Example: on `/?token=abc` returns `'abc'` and leaves the URL as `/`. */
export function initToken(): string {
  const token = tokenFrom(window.location.search, window.sessionStorage.getItem(TOKEN_KEY));
  if (token) {
    window.sessionStorage.setItem(TOKEN_KEY, token);
  }
  if (window.location.search.includes('token=')) {
    window.history.replaceState(null, '', window.location.pathname);
  }
  return token;
}

/** Builds an API path with query parameters; repeated values repeat the key.
 *  Example: `apiPath('expand', { id: 'a', visible: ['b', 'c'] })` is
 *  `/api/v1/expand?id=a&visible=b&visible=c`. */
export function apiPath(endpoint: string, params: Record<string, string | number | string[] | undefined> = {}): string {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v === undefined) continue;
    for (const item of Array.isArray(v) ? v : [v]) q.append(k, String(item));
  }
  const s = q.toString();
  return `/api/v1/${endpoint}${s ? `?${s}` : ''}`;
}

/** The API client for one session. */
export class Api {
  constructor(private readonly token: string) {}

  /** GETs a JSON envelope and returns its data and warnings, or throws its error message.
   *  Example: `const { data, warnings } = await api.envelope<Payload>('graph')`. */
  async envelope<T>(
    endpoint: string,
    params?: Record<string, string | number | string[] | undefined>,
  ): Promise<{ data: T; warnings: Envelope<T>['warnings'] }> {
    const res = await fetch(apiPath(endpoint, params), { headers: { 'x-aldente-token': this.token } });
    const env = (await res.json()) as Envelope<T>;
    if (!env.ok || env.data === null) {
      throw new Error(env.error?.message ?? `request failed (${res.status})`);
    }
    return { data: env.data, warnings: env.warnings ?? [] };
  }

  /** GETs a JSON envelope and returns its data, or throws its error message.
   *  Example: `await api.json<Payload>('graph', { level: 'namespace' })`. */
  async json<T>(endpoint: string, params?: Record<string, string | number | string[] | undefined>): Promise<T> {
    return (await this.envelope<T>(endpoint, params)).data;
  }

  /** GETs plain text (the `/source` endpoint).
   *  Example: `await api.text('source', { id })`. */
  async text(endpoint: string, params?: Record<string, string | number | undefined>): Promise<string> {
    const res = await fetch(apiPath(endpoint, params), { headers: { 'x-aldente-token': this.token } });
    if (!res.ok) {
      const env = (await res.json().catch(() => null)) as Envelope<unknown> | null;
      throw new Error(env?.error?.message ?? `request failed (${res.status})`);
    }
    return res.text();
  }

  /** GETs plain text and the `x-aldente-first-line` header.
   *  Example: `await api.textAt('source', { id })` is `{ text: '(defn …', first: 12 }`. */
  async textAt(endpoint: string, params: Record<string, string | number | undefined>): Promise<{ text: string; first: number }> {
    const res = await fetch(apiPath(endpoint, params), { headers: { 'x-aldente-token': this.token } });
    if (!res.ok) {
      const env = (await res.json().catch(() => null)) as Envelope<unknown> | null;
      throw new Error(env?.error?.message ?? `request failed (${res.status})`);
    }
    return { text: await res.text(), first: Number(res.headers.get('x-aldente-first-line') ?? '1') };
  }

  /** POSTs `body` as text and returns the envelope's data, or throws its error message.
   *  Example: `await api.post<TraceReport>('trace/normalize', {}, traceText)`. */
  async post<T>(endpoint: string, params: Record<string, string | number | undefined>, body: string): Promise<T> {
    const res = await fetch(apiPath(endpoint, params), {
      method: 'POST',
      headers: { 'x-aldente-token': this.token, 'content-type': 'text/plain' },
      body,
    });
    const env = (await res.json()) as Envelope<T>;
    if (!env.ok || env.data === null) {
      throw new Error(env.error?.message ?? `request failed (${res.status})`);
    }
    return env.data;
  }

  snapshot = () => this.json<SnapshotOverview>('snapshot');
  graph = () => this.envelope<Payload>('graph');
  /** The repository's directory tree. */
  tree = () => this.json<TreeDir>('tree');
  /** The repository's code at the tree's resolution: each collapsed
   *  directory one node, at most the `top` busiest nodes. */
  treeGraph = (collapsed: string[], top: number) => this.envelope<Payload>('tree/graph', { collapsed, top });
  unitGraph = (unit: string) => this.envelope<Payload>('graph', { level: 'namespace', unit });
  /** The namespaces whose files lie in `dir` (a repository path) and the calls between them. */
  dirGraph = (dir: string) => this.envelope<Payload>('graph', { level: 'namespace', dir });
  neighborhood = (id: string) => this.json<Payload>('neighborhood', { id, depth: 1 });
  expand = (id: string, visible: string[]) => this.json<Payload>('expand', { id, visible });
  /** One page of an aggregate's children; the visible IDs go in the body
   *  (a full canvas does not fit in a URL). */
  expandPage = (id: string, visible: string[], offset: number) => this.post<Payload>('expand', { id, offset }, JSON.stringify(visible));
  node = (id: string) => this.json<ShowResult>('node', { id });
  /** What a unit uses from the other units and what they use from it. */
  unitLinks = (id: string) => this.json<UnitLinks>('unit-links', { id });
  search = (q: string) => this.json<FindResult>('search', { q, limit: 20 });
  source = (id: string) => this.text('source', { id, context: 2 });
  /** A node's source and the line it starts at. */
  sourceAt = (id: string) => this.textAt('source', { id, context: 2 });
  /** The lines around a call site and the line they start at. */
  siteSource = (file: string, line: number) => this.textAt('source', { file, line, context: 4 });
  /** Opens a node or site in the configured editor, or says where it is. */
  openInEditor = (target: { id: string } | { file: string; line: number; col: number }) =>
    this.post<OpenResult>('open', {}, JSON.stringify(target));
  blindspots = () => this.json<BlindspotReport>('blindspots');
  crater = (q: CraterQuery) => this.json<CraterResult>('crater', craterParams(q));
  checklist = (q: CraterQuery) => this.json<Checklist>('checklist', craterParams(q));
  normalizeTrace = (text: string) => this.post<TraceReport>('trace/normalize', {}, text);
  craterTrace = (text: string, q: Omit<CraterQuery, 'ids'>) =>
    this.post<CraterResult>('crater/trace', { direction: q.direction, depth: q.depth, min_certainty: q.minCertainty }, text);
  libs = () => this.json<LibrariesReport>('libs');
  lib = (id: string, by: 'namespace' | 'symbol' = 'namespace') => this.json<LibraryUsage>('lib', { id, by });
  metrics = (level: 'symbol' | 'namespace' | 'unit') => this.json<MetricsReport>('metrics', { level });
  rankings = (level: RankLevel) => this.json<RankingReport>('rankings', { level });
  coverage = () => this.json<CoverageReport>('coverage');
  hotspots = () => this.json<HotspotsReport>('hotspots');
  /** The hotspot rules and analyzer defaults in use (for Help). */
  defaults = () => this.json<Defaults>('defaults');
  cycles = (level: 'symbol' | 'namespace' | 'unit') => this.json<CyclesReport>('cycles', { level });
  churn = () => this.json<ChurnReport>('churn');
  /** The named nodes and the edges between them. */
  subgraph = (ids: string[]) => this.json<Payload>('subgraph', { id: ids });
}

/** What a crater request asks for. `depth` 0 means unlimited. */
export interface CraterQuery {
  ids: string[];
  direction: 'forward' | 'reverse' | 'both';
  depth: number;
  minCertainty: 'certain' | 'likely' | 'possible';
}

/** The query parameters of `/crater` and `/checklist`.
 *  Example: `craterParams({ids: ['a'], direction: 'reverse', depth: 6, minCertainty: 'likely'})`
 *  is `{ id: ['a'], direction: 'reverse', depth: 6, min_certainty: 'likely' }`. */
export function craterParams(q: CraterQuery): Record<string, string | number | string[]> {
  return { id: q.ids, direction: q.direction, depth: q.depth, min_certainty: q.minCertainty };
}
