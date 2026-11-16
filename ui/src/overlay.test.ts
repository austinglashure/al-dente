import { describe, expect, it } from 'vitest';
import { COVERAGE_COLORS, bind, heat, metricValues, namespaceOf, nodeMeasures, overlayData, rollUp, styler } from './overlay';
import type { CoverageReport, HotspotsReport, MetricsReport } from './types';

const coverage: CoverageReport = {
  tests: 1,
  symbols: [
    { id: 'clj:.//a/f', status: 'direct', distance: 1, tests: [], test_count: 1, measured: null },
    { id: 'clj:.//a/g', status: 'untested', distance: null, tests: [], test_count: 0, measured: null },
    { id: 'clj:.//b/h', status: 'untested', distance: null, tests: [], test_count: 0, measured: null },
  ],
};
const hotspots: HotspotsReport = {
  excluded: 0,
  findings: [{ kind: 'risky-untested', subject: 'clj:.//a/g', nodes: ['clj:.//a/g'], score: 0.9, evidence: {}, reason: 'x.' }],
  risk: [
    { id: 'clj:.//a/f', risk: 0.1, importance: 1, exposure: 0.1, status: 'direct', measured: null, churn: null },
    { id: 'clj:.//a/g', risk: 0.8, importance: 0.8, exposure: 1, status: 'untested', measured: null, churn: null },
  ],
};

describe('overlays', () => {
  it('finds namespaces and scales heat', () => {
    expect(namespaceOf('clj:.//shop.core/checkout')).toBe('clj:.//shop.core');
    expect(namespaceOf('clj:.//shop.pricing/price@:book')).toBe('clj:.//shop.pricing');
    expect(namespaceOf('clj:.//shop.core')).toBeNull();
    expect(heat(0)).toBe('#fff5f0');
    expect(heat(1)).toBe('#99000d');
    expect(rollUp(new Map([['clj:.//a/f', 0.2], ['clj:.//a/g', 0.5]]), 'max').get('clj:.//a')).toBe(0.5);
  });

  it('styles coverage: fill for tested, pale for untested, a marker for risky untested', () => {
    const style = styler('coverage', overlayData(coverage, hotspots, null), []);
    expect(style('clj:.//a/f')).toEqual({ color: COVERAGE_COLORS.direct, tag: 'direct' });
    expect(style('clj:.//a/g')).toEqual({ color: COVERAGE_COLORS.untested, tag: 'risky-untested', mark: '⚠ ' });
    expect(style('clj:.//b/h')?.tag).toBe('untested');
    // Namespaces by tested share.
    expect(style('clj:.//a')?.tag).toBe('partly-tested');
    expect(style('clj:.//b')?.tag).toBe('untested');
    expect(style('ext:clj//clojure.core/map')).toBeNull();
  });

  it('scales risk by the largest visible value and rolls it up to namespaces', () => {
    const style = styler('risk', overlayData(coverage, hotspots, null), ['clj:.//a/f', 'clj:.//a/g']);
    expect(style('clj:.//a/g')).toMatchObject({ color: heat(1), tag: 'risk-high' });
    expect(style('clj:.//a/f')?.tag).toBe('risk-low');
    expect(style('clj:.//a')?.color).toBe(heat(1));
    expect(styler('none', overlayData(null, null, null), [])('x')).toBeNull();
  });

  it('binds size and colour to measures over the overlay', () => {
    const report: MetricsReport = {
      first: { nodes: [{ id: 'n1', fan_in: 2, fan_out: 0, fan_in_weighted: 2, fan_out_weighted: 0, pagerank: 0.4, pagerank_reverse: 0.1, hub: 0, authority: 1, core: 1, component: null }], components: [] },
      second: { nodes: [{ id: 'n1', betweenness: 0.5, closeness: 1, eigenvector: 0, community: 0, dominates: 0, articulation: false, god_score: 0.2 }], sampling: { approximate: false, pivots: 1 } },
    };
    expect(metricValues(report, 'pagerank').get('n1')).toBe(0.4);
    expect(metricValues(report, 'betweenness').get('n1')).toBe(0.5);
    const sized = bind(() => ({ color: 'red', tag: 't' }), metricValues(report, 'fan_in'), null, ['n1']);
    expect(sized('n1')).toEqual({ color: 'red', tag: 't', size: 18 });
    const colored = bind(() => null, null, metricValues(report, 'pagerank'), ['n1']);
    expect(colored('n1')?.color).toBe(heat(1));
  });
});

describe('nodeMeasures', () => {
  it('lists one node’s measures in METRICS order from both report parts', () => {
    const report = {
      first: { nodes: [{ id: 'a', fan_in: 3, fan_out: 1, pagerank: 0.5 }], components: [] },
      second: { nodes: [{ id: 'a', betweenness: 2 }], sampling: { approximate: false, pivots: 0 } },
    } as unknown as MetricsReport;
    expect(nodeMeasures(report, 'a')).toEqual([
      { name: 'fan_in', value: 3 },
      { name: 'fan_out', value: 1 },
      { name: 'pagerank', value: 0.5 },
      { name: 'betweenness', value: 2 },
    ]);
    expect(nodeMeasures(report, 'b')).toEqual([]);
  });
});

describe('rollUp to units', () => {
  it('adds each symbol to its namespace and to its unit', () => {
    const up = rollUp(new Map([['clj:services/orders//o.core/a', 2], ['clj:services/orders//o.util/b', 5], ['clj:.//x/c', 1]]), 'max');
    expect(up.get('clj:services/orders//o.core')).toBe(2);
    expect(up.get('unit:services/orders')).toBe(5);
    expect(up.get('unit:.')).toBe(1);
  });
});
