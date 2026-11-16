// The workspace: what the canvas shows, as the steps that built it plus
// node positions, so a view can be bookmarked, reloaded and saved by name.
// Encoded as deflate + base64url in the URL hash; too long for a URL, it is
// kept in browser storage under a short key instead. Also the undo and redo
// timeline.

import type { Filters } from './filters';

/** One step that changes the canvas. Replaying the steps on the same
 *  snapshot rebuilds the view. */
export type Op =
  | { op: 'view'; view: 'units' | 'namespaces' }
  | { op: 'open'; id: string }
  | { op: 'focus'; id: string }
  | { op: 'expand'; id: string }
  | { op: 'collapse'; id: string }
  | { op: 'neighbors'; id: string }
  | { op: 'hide'; ids: string[] }
  | { op: 'keep'; ids: string[] }
  | { op: 'pin'; id: string; on: boolean }
  | { op: 'filters'; filters: Filters }
  /** The hide toggles (tests, externals, interop, core). */
  | { op: 'quick'; hide: QuickHide }
  /** Back to breadcrumb `to` (0 is the root canvas). */
  | { op: 'back'; to: number }
  /** A cycle's nodes on a canvas of their own. */
  | { op: 'cycle'; ids: string[] };

/** The hide toggles, as `HideOptions` in `model.ts`. */
export interface QuickHide {
  hideTests: boolean;
  hideExternals: boolean;
  hideInterop?: boolean;
  hideCore?: boolean;
}

/** A bookmarkable view. Positions are rounded to whole units. */
export interface Workspace {
  v: 1;
  ops: Op[];
  positions: Record<string, [number, number]>;
}

/** Hashes longer than this go to browser storage (links get cut beyond a
 *  few thousand characters in chat tools and some servers). */
export const URL_LIMIT = 4000;

const STORAGE_PREFIX = 'aldente.ws.';

/** The part of `Storage` the workspace uses. */
export interface KeyValue {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
}

/** Bytes through a (de)compression stream.
 *  Example: `await pipe(bytes, new CompressionStream('deflate-raw'))`. */
async function pipe(bytes: Uint8Array, stream: CompressionStream | DecompressionStream): Promise<Uint8Array> {
  const writer = stream.writable.getWriter();
  void writer.write(bytes as BufferSource).catch(() => undefined);
  void writer.close().catch(() => undefined);
  const chunks: Uint8Array[] = [];
  const reader = stream.readable.getReader();
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value as Uint8Array);
  }
  const out = new Uint8Array(chunks.reduce((n, c) => n + c.length, 0));
  let at = 0;
  for (const c of chunks) {
    out.set(c, at);
    at += c.length;
  }
  return out;
}

/** base64url without padding.
 *  Example: `toBase64Url(new Uint8Array([255]))` is `'_w'`. */
function toBase64Url(bytes: Uint8Array): string {
  let s = '';
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

/** The bytes of a base64url string.
 *  Example: `fromBase64Url('_w')` is `Uint8Array [255]`. */
function fromBase64Url(s: string): Uint8Array {
  const b64 = s.replace(/-/g, '+').replace(/_/g, '/');
  return Uint8Array.from(atob(b64 + '='.repeat((4 - (b64.length % 4)) % 4)), (c) => c.charCodeAt(0));
}

/** A workspace as a URL-safe string.
 *  Example: `await encode(ws)` is something like `'q1YqUypSsjLUUUpSsjI…'`. */
export async function encode(ws: Workspace): Promise<string> {
  return toBase64Url(await pipe(new TextEncoder().encode(JSON.stringify(ws)), new CompressionStream('deflate-raw')));
}

/** The workspace in an encoded string, or null if it is not one (a stale
 *  or hand-edited link must not break the page).
 *  Example: `await decode('garbage')` is `null`. */
export async function decode(s: string): Promise<Workspace | null> {
  try {
    const ws = JSON.parse(new TextDecoder().decode(await pipe(fromBase64Url(s), new DecompressionStream('deflate-raw')))) as Workspace;
    return ws.v === 1 && Array.isArray(ws.ops) && typeof ws.positions === 'object' ? ws : null;
  } catch {
    return null;
  }
}

/** A short key for a long encoded workspace (FNV-1a, 32 bits, hex).
 *  Example: `shortKey('abc')` is `'1a47e90b'`. */
function shortKey(s: string): string {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i += 1) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(16).padStart(8, '0');
}

/** The URL hash for a workspace: `#w=<encoded>`, or `#ws=<key>` with the
 *  workspace stored under that key when it is too long; `stored` names the key.
 *  Example: a three-step view gives `{ hash: '#w=…', stored: null }`. */
export async function toHash(ws: Workspace, storage: KeyValue): Promise<{ hash: string; stored: string | null }> {
  const enc = await encode(ws);
  const hash = `#w=${enc}`;
  if (hash.length <= URL_LIMIT) return { hash, stored: null };
  const key = shortKey(enc);
  storage.setItem(STORAGE_PREFIX + key, enc);
  return { hash: `#ws=${key}`, stored: key };
}

/** The workspace a URL hash names. `missing` is the storage key when the
 *  hash points at one this browser does not have.
 *  Example: `await fromHash('#ws=1a47e90b', emptyStorage)` is `{ ws: null, missing: '1a47e90b' }`. */
export async function fromHash(hash: string, storage: KeyValue): Promise<{ ws: Workspace | null; missing: string | null }> {
  if (hash.startsWith('#w=')) return { ws: await decode(hash.slice(3)), missing: null };
  if (hash.startsWith('#ws=')) {
    const key = hash.slice(4);
    const enc = storage.getItem(STORAGE_PREFIX + key);
    return enc === null ? { ws: null, missing: key } : { ws: await decode(enc), missing: null };
  }
  return { ws: null, missing: null };
}

/** Undo and redo: the states before and after the present one. */
export interface Timeline<S> {
  past: S[];
  present: S;
  future: S[];
}

/** A new present state; the old one goes to the past (at most `cap` kept)
 *  and the redo side is cleared.
 *  Example: `record({past: [], present: 0, future: [5]}, 1)` is `{past: [0], present: 1, future: []}`. */
export function record<S>(t: Timeline<S>, next: S, cap = 50): Timeline<S> {
  return { past: [...t.past, t.present].slice(-cap), present: next, future: [] };
}

/** One step back, or the same timeline at the start.
 *  Example: `undo({past: [0], present: 1, future: []})` is `{past: [], present: 0, future: [1]}`. */
export function undo<S>(t: Timeline<S>): Timeline<S> {
  const prev = t.past.at(-1);
  return prev === undefined ? t : { past: t.past.slice(0, -1), present: prev, future: [t.present, ...t.future] };
}

/** One step forward, or the same timeline at the end.
 *  Example: `redo({past: [], present: 0, future: [1]})` is `{past: [0], present: 1, future: []}`. */
export function redo<S>(t: Timeline<S>): Timeline<S> {
  const [next, ...rest] = t.future;
  return next === undefined ? t : { past: [...t.past, t.present], present: next, future: rest };
}
