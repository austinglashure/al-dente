import { describe, expect, it } from 'vitest';
import { applyPage, batched, clusterSize, collapse, edgeHiddenAtZoom, labelHiddenAtZoom, nextOffset, remaining, zoomLevel, type Expansions } from './expansion';
import { edgeKey, mergePayload, newGraph } from './model';
import type { GEdge, GNode, Payload } from './types';

const node = (id: string, kind = 'symbol', extra: Partial<GNode> = {}): GNode => ({ id, kind, name: id.split('/').pop() ?? id, ...extra });
const calls = (from: string, to: string, count?: number): GEdge => ({ from, to, kind: 'calls', certainty: 'certain', reason: 'direct', ...(count ? { count } : {}) });
const payload = (nodes: GNode[], edges: GEdge[], truncated = false): Payload => ({ nodes, edges, truncated, aggregated: null });

/** A canvas with namespaces `n` (3 members) and `o`, and `n -calls-> o` (count 3). */
function canvas() {
  const g = newGraph();
  mergePayload(g, payload([node('n', 'namespace', { members: 3 }), node('o', 'namespace', { members: 1 })], [calls('n', 'o', 3)]), undefined, () => 0.5);
  g.mergeNodeAttributes('n', { x: 100, y: 50 });
  return g;
}

describe('applyPage and collapse', () => {
  it('a first page adds children near the aggregate, keeps it as the rest, and swaps its edges', () => {
    const g = canvas();
    const ex: Expansions = new Map();
    const page = payload([node('n/a'), node('n', 'namespace', { members: 3 })], [calls('n/a', 'o'), calls('n', 'o', 2)], true);
    expect(applyPage(g, ex, 'n', page, () => 0.5)).toEqual(['n/a']);
    expect(g.getNodeAttributes('n/a')).toMatchObject({ x: 100, y: 50 });
    expect(g.getNodeAttribute('n', 'expanded')).toBe(false);
    expect(g.getNodeAttribute('n', 'label')).toBe('n (+2 more)');
    expect(g.edges().map((k) => g.getEdgeAttribute(k, 'edge').count ?? 1)).toEqual([1, 2]);
    expect(nextOffset(ex, 'n')).toBe(1);
    expect(remaining(ex, 'n')).toBe(2);
  });

  it('the last page hides the aggregate; collapse restores it and its edges', () => {
    const g = canvas();
    const ex: Expansions = new Map();
    applyPage(g, ex, 'n', payload([node('n/a'), node('n', 'namespace', { members: 3 })], [calls('n/a', 'o'), calls('n', 'o', 2)], true));
    applyPage(g, ex, 'n', payload([node('n/b'), node('n/c')], [calls('n/b', 'o'), calls('n/c', 'o')]));
    expect(g.getNodeAttribute('n', 'expanded')).toBe(true);
    expect(g.degree('n')).toBe(0);
    expect(remaining(ex, 'n')).toBe(0);
    collapse(g, ex, 'n');
    expect(g.nodes().sort()).toEqual(['n', 'o']);
    expect(g.getNodeAttribute('n', 'expanded')).toBe(false);
    expect(g.getNodeAttribute('n', 'label')).toBe('n');
    expect(g.edges()).toEqual([edgeKey(calls('n', 'o', 3))]);
    expect(g.getNodeAttributes('n')).toMatchObject({ x: 100, y: 50 });
    expect(ex.size).toBe(0);
  });

  it('collapsing a unit collapses the namespaces expanded inside it first', () => {
    const g = newGraph();
    mergePayload(g, payload([node('unit:u', 'unit', { members: 1 })], []));
    const ex: Expansions = new Map();
    applyPage(g, ex, 'unit:u', payload([node('n', 'namespace', { members: 1 })], []));
    applyPage(g, ex, 'n', payload([node('n/a')], []));
    collapse(g, ex, 'unit:u');
    expect(g.nodes()).toEqual(['unit:u']);
    expect(ex.size).toBe(0);
  });

  it('collapse drops interop exits the expansion brought in, unless something else still uses them', () => {
    const g = canvas();
    const ex: Expansions = new Map();
    applyPage(g, ex, 'n', payload([node('n/a'), node('n/b'), node('n/c'), node('java:X#m', 'platform'), node('java:Y#m', 'platform')], [calls('n/a', 'java:X#m'), calls('n/b', 'java:Y#m')]));
    mergePayload(g, payload([], [calls('o', 'java:Y#m')]));
    collapse(g, ex, 'n');
    expect(g.hasNode('java:X#m')).toBe(false);
    expect(g.hasNode('java:Y#m')).toBe(true);
  });
});

describe('clusterSize', () => {
  it('grows with the member count, slowly, and leaves symbols alone', () => {
    expect(clusterSize(node('a', 'namespace', { members: 0 }))).toBe(8);
    expect(clusterSize(node('a', 'namespace', { members: 5000 }))).toBeGreaterThan(clusterSize(node('b', 'namespace', { members: 50 })));
    expect(clusterSize(node('a', 'namespace', { members: 5000 }))).toBeLessThanOrEqual(24);
    expect(clusterSize(node('a/f'))).toBe(4);
  });
});

describe('semantic zoom', () => {
  it('levels by camera ratio: close, middle, far', () => {
    expect(zoomLevel(1)).toBe(0);
    expect(zoomLevel(2)).toBe(1);
    expect(zoomLevel(6)).toBe(2);
  });

  it('hides light edges further out, never aggregate-free heavy ones', () => {
    expect(edgeHiddenAtZoom(calls('a', 'b'), 0)).toBe(false);
    expect(edgeHiddenAtZoom({ ...calls('a', 'b'), certainty: 'possible' }, 1)).toBe(true);
    expect(edgeHiddenAtZoom(calls('a', 'b'), 1)).toBe(false);
    expect(edgeHiddenAtZoom(calls('a', 'b'), 2)).toBe(true);
    expect(edgeHiddenAtZoom(calls('a', 'b', 8), 2)).toBe(false);
  });

  it('hides symbol labels when far out, keeps aggregates and holes', () => {
    expect(labelHiddenAtZoom(node('a/f'), 1)).toBe(false);
    expect(labelHiddenAtZoom(node('a/f'), 2)).toBe(true);
    expect(labelHiddenAtZoom(node('a', 'namespace'), 2)).toBe(false);
    expect(labelHiddenAtZoom(node('hole:x', 'hole'), 2)).toBe(false);
  });
});

describe('batched', () => {
  it('detaches the renderer while the changes run and reattaches it, even after a throw', () => {
    const g = newGraph();
    const seen: unknown[] = [];
    const renderer = { graph: g as unknown, setGraph(x: unknown) { this.graph = x; seen.push(x === g ? 'graph' : 'placeholder'); } };
    batched(renderer, g, () => {
      expect(renderer.graph).not.toBe(g);
    });
    expect(seen).toEqual(['placeholder', 'graph']);
    expect(() => batched(renderer, g, () => { throw new Error('boom'); })).toThrow('boom');
    expect(renderer.graph).toBe(g);
    batched(null, g, () => seen.push('ran without a renderer'));
    expect(seen.at(-1)).toBe('ran without a renderer');
  });
});
