import { describe, expect, it } from 'vitest';
import { readFolded, writeFolded } from './folded';

/** An in-memory stand-in for localStorage. */
function memory(): Pick<Storage, 'getItem' | 'setItem'> {
  const m = new Map<string, string>();
  return { getItem: (k) => m.get(k) ?? null, setItem: (k, v) => void m.set(k, v) };
}

describe('folded details sections', () => {
  it('round-trips through storage', () => {
    const s = memory();
    expect(readFolded(s)).toEqual(new Set());
    writeFolded(s, new Set(['caller', 'tests']));
    expect(readFolded(s)).toEqual(new Set(['caller', 'tests']));
  });

  it('falls back to nothing folded when storage is broken or garbled', () => {
    const throwing = {
      getItem: () => {
        throw new Error('blocked');
      },
      setItem: () => {
        throw new Error('blocked');
      },
    };
    expect(readFolded(throwing)).toEqual(new Set());
    expect(() => writeFolded(throwing, new Set(['x']))).not.toThrow();
    const s = memory();
    s.setItem('aldente.details.folded', '{not json');
    expect(readFolded(s)).toEqual(new Set());
    s.setItem('aldente.details.folded', '[1, "caller"]');
    expect(readFolded(s)).toEqual(new Set(['caller']));
  });
});
