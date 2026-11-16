import { describe, expect, it } from 'vitest';
import { decode, encode, fromHash, record, redo, toHash, undo, URL_LIMIT, type Workspace } from './workspace';

/** An in-memory stand-in for localStorage. */
function memoryStorage() {
  const m = new Map<string, string>();
  return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v), map: m };
}

const small: Workspace = {
  v: 1,
  ops: [{ op: 'view', view: 'units' }, { op: 'expand', id: 'unit:services/orders' }, { op: 'hide', ids: ['clj:services/orders//orders.core'] }],
  positions: { 'unit:services/orders': [10, -20] },
};

describe('encode and decode', () => {
  it('round-trips a workspace through a URL-safe string', async () => {
    const s = await encode(small);
    expect(s).toMatch(/^[A-Za-z0-9_-]+$/);
    expect(await decode(s)).toEqual(small);
  });

  it('gives null for anything that is not a workspace', async () => {
    expect(await decode('not-base64!')).toBeNull();
    expect(await decode(await encode({ v: 2 } as unknown as Workspace))).toBeNull();
  });
});

describe('toHash and fromHash', () => {
  it('puts a small workspace in the URL hash', async () => {
    const storage = memoryStorage();
    const { hash, stored } = await toHash(small, storage);
    expect(hash.startsWith('#w=')).toBe(true);
    expect(stored).toBeNull();
    expect((await fromHash(hash, storage)).ws).toEqual(small);
  });

  it('keeps one too long for a URL in browser storage under a short key, and says so', async () => {
    const storage = memoryStorage();
    const positions: Workspace['positions'] = {};
    for (let i = 0; i < 3000; i += 1) positions[`clj:.//big.n${i}/f${i * 7919}`] = [i * 13.7, 1 - i * 3.1];
    const big: Workspace = { ...small, positions };
    const { hash, stored } = await toHash(big, storage);
    expect(hash).toMatch(/^#ws=[0-9a-f]{8}$/);
    expect(stored).toBe(hash.slice(4));
    expect(hash.length).toBeLessThan(URL_LIMIT);
    expect((await fromHash(hash, storage)).ws).toEqual(big);
    // Another browser (empty storage) cannot find it: no workspace, and it says the key is missing.
    expect(await fromHash(hash, memoryStorage())).toEqual({ ws: null, missing: stored });
  });

  it('ignores hashes it does not know', async () => {
    expect(await fromHash('', memoryStorage())).toEqual({ ws: null, missing: null });
    expect(await fromHash('#other', memoryStorage())).toEqual({ ws: null, missing: null });
  });
});

describe('timeline', () => {
  it('records steps, undoes to the start, redoes, and a new step clears the redo side', () => {
    let t = { past: [] as number[], present: 0, future: [] as number[] };
    for (const s of [1, 2, 3]) t = record(t, s);
    expect(t).toEqual({ past: [0, 1, 2], present: 3, future: [] });
    t = undo(undo(undo(undo(t))));
    expect(t).toEqual({ past: [], present: 0, future: [1, 2, 3] });
    t = redo(t);
    expect(t.present).toBe(1);
    t = record(t, 9);
    expect(t).toEqual({ past: [0, 1], present: 9, future: [] });
  });

  it('keeps at most `cap` past steps', () => {
    let t = { past: [] as number[], present: 0, future: [] as number[] };
    for (let s = 1; s <= 10; s += 1) t = record(t, s, 3);
    expect(t.past).toEqual([7, 8, 9]);
  });
});
