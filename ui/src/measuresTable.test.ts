import { describe, expect, it } from 'vitest';
import { measureRows, windowOf } from './MeasuresTable';
import type { MetricsReport } from './types';

const node = (id: string, pagerank: number, fan_in: number) => ({ id, fan_in, fan_out: 0, fan_in_weighted: fan_in, fan_out_weighted: 0, pagerank, pagerank_reverse: 0, hub: 0, authority: 0, core: 0, component: null });

describe('measures table', () => {
  it('sorts rows descending, ties by ID', () => {
    const r: MetricsReport = {
      first: { nodes: [node('b', 0.2, 1), node('a', 0.2, 3), node('c', 0.5, 0)], components: [] },
      second: { nodes: [], sampling: { approximate: false, pivots: 3 } },
    };
    expect(measureRows(r, 'pagerank').map((x) => x.id)).toEqual(['c', 'a', 'b']);
    expect(measureRows(r, 'fan_in').map((x) => x.id)).toEqual(['a', 'b', 'c']);
  });

  it('renders only the rows in view', () => {
    expect(windowOf(1000, 0)).toEqual({ start: 0, end: 17 });
    expect(windowOf(1000, 22 * 500)).toEqual({ start: 499, end: 516 });
    expect(windowOf(5, 0)).toEqual({ start: 0, end: 5 });
  });
});
