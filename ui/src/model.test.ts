import { describe, expect, it } from 'vitest';
import { dropNodes, edgeKey, hidden, isTest, labelFor, mergePayload, newGraph } from './model';
import { apiPath, tokenFrom } from './api';
import type { GNode, Payload } from './types';

const node = (id: string, kind = 'symbol', extra: Partial<GNode> = {}): GNode => ({ id, kind, name: id.split('/').pop() ?? id, ...extra });

describe('mergePayload', () => {
  it('adds new nodes and edges once, skipping contains and dangling edges', () => {
    const g = newGraph();
    const p: Payload = {
      nodes: [node('clj:.//a'), node('clj:.//a/f')],
      edges: [
        { from: 'clj:.//a', to: 'clj:.//a/f', kind: 'contains' },
        { from: 'clj:.//a/f', to: 'clj:.//a', kind: 'calls', certainty: 'certain', reason: 'direct' },
        { from: 'clj:.//a/f', to: 'clj:.//missing', kind: 'calls' },
      ],
      truncated: false,
      aggregated: null,
    };
    expect(mergePayload(g, p, undefined, () => 0.5)).toEqual(['clj:.//a', 'clj:.//a/f']);
    expect(g.order).toBe(2);
    expect(g.size).toBe(1);
    expect(mergePayload(g, p)).toEqual([]);
    expect(g.size).toBe(1);
  });

  it('places new nodes near the anchor', () => {
    const g = newGraph();
    mergePayload(g, { nodes: [node('a')], edges: [], truncated: false, aggregated: null }, undefined, () => 0.5);
    g.setNodeAttribute('a', 'x', 100);
    mergePayload(g, { nodes: [node('b')], edges: [], truncated: false, aggregated: null }, 'a', () => 0.5);
    expect(g.getNodeAttribute('b', 'x')).toBe(100);
    dropNodes(g, ['b', 'zzz']);
    expect(g.hasNode('b')).toBe(false);
  });
});

describe('toggles', () => {
  it('hides tests and externals on request', () => {
    expect(isTest(node('clj:.//shop.pricing-test/food-is-taxed', 'symbol', { subkind: 'test' }))).toBe(true);
    expect(isTest(node('clj:.//shop.pricing-test', 'namespace'))).toBe(true);
    expect(hidden(node('ext:clj//clojure.core/map', 'external'), { hideTests: false, hideExternals: true })).toBe(true);
    expect(hidden(node('clj:.//a/f'), { hideTests: true, hideExternals: true })).toBe(false);
  });
});

describe('api helpers', () => {
  it('reads the token and builds paths', () => {
    expect(tokenFrom('?token=abc', null)).toBe('abc');
    expect(tokenFrom('', 'stored')).toBe('stored');
    expect(apiPath('expand', { id: 'clj:.//a', visible: ['x', 'y'] })).toBe('/api/v1/expand?id=clj%3A.%2F%2Fa&visible=x&visible=y');
    expect(edgeKey({ from: 'a', to: 'b', kind: 'calls' })).toBe('a|calls|b||');
  });
});

describe('labels', () => {
  it('names units by their last directory', () => {
    expect(labelFor(node('unit:modules/reitit-core', 'unit', { name: 'modules/reitit-core' }))).toBe('reitit-core');
    expect(labelFor(node('unit:.', 'unit', { name: '.' }))).toBe('(root)');
    expect(labelFor(node('clj:.//a/f'))).toBe('f');
  });
});
