import { describe, expect, it } from 'vitest';
import { MEASURES, measuresFor, rank, type RankingRow } from './rankings';

const rows: RankingRow[] = [
  { id: 'clj:.//a', name: 'a', values: { callers: 3, lines: 40, instability: 0.2 } },
  { id: 'clj:.//b', name: 'b', values: { callers: 9, lines: 10, instability: 0.9 } },
  { id: 'clj:.//c', name: 'c', values: { callers: 3, lines: 25 } },
  { id: 'clj:.//d', name: 'd', values: { callers: 0, lines: 5, instability: 0 } },
];

describe('rank', () => {
  it('orders by a measure, highest first, ties by name', () => {
    expect(rank(rows, 'callers', 'desc').map((r) => r.id)).toEqual(['clj:.//b', 'clj:.//a', 'clj:.//c', 'clj:.//d']);
    expect(rank(rows, 'lines', 'asc').map((r) => r.name)).toEqual(['d', 'b', 'c', 'a']);
  });

  it('leaves out rows without the measure', () => {
    expect(rank(rows, 'instability', 'desc').map((r) => r.name)).toEqual(['b', 'a', 'd']);
  });

  it('keeps only the rows in scope', () => {
    const onCanvas = new Set(['clj:.//a', 'clj:.//d']);
    expect(rank(rows, 'callers', 'desc', onCanvas).map((r) => r.name)).toEqual(['a', 'd']);
  });
});

describe('measures', () => {
  it('has a label, a group and an explanation for every measure', () => {
    for (const m of MEASURES) {
      expect(m.label.length).toBeGreaterThan(0);
      expect(m.about.length).toBeGreaterThan(10);
    }
    expect(new Set(MEASURES.map((m) => m.key)).size).toBe(MEASURES.length);
  });

  it('offers instability only for namespaces and units', () => {
    expect(measuresFor('namespace').some((m) => m.key === 'instability')).toBe(true);
    expect(measuresFor('symbol').some((m) => m.key === 'instability')).toBe(false);
  });
});
