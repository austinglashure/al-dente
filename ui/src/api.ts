// Talking to the local API. The session token comes from the URL the CLI
// opened (`/?token=…`); it is kept in sessionStorage, removed from the
// address bar, and sent as a header on every API call.

import type { BlindspotReport, Envelope, FindResult, Payload, ShowResult, SnapshotOverview } from './types';

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

  snapshot = () => this.json<SnapshotOverview>('snapshot');
  graph = () => this.envelope<Payload>('graph');
  /** Every repository namespace (and hole) at once, without externals. */
  allNamespaces = () => this.envelope<Payload>('graph', { level: 'namespace', scope: 'repo' });
  unitGraph = (unit: string) => this.envelope<Payload>('graph', { level: 'namespace', unit });
  neighborhood = (id: string) => this.json<Payload>('neighborhood', { id, depth: 1 });
  expand = (id: string, visible: string[]) => this.json<Payload>('expand', { id, visible });
  node = (id: string) => this.json<ShowResult>('node', { id });
  search = (q: string) => this.json<FindResult>('search', { q, limit: 20 });
  source = (id: string) => this.text('source', { id, context: 2 });
  blindspots = () => this.json<BlindspotReport>('blindspots');
}
