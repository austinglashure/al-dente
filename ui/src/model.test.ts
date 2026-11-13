import { describe, expect, it } from 'vitest';
import { dropNodes, edgeKey, edgeShown, hasSource, hidden, isCore, isTest, labelFor, mergePayload, newGraph } from './model';
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

  it('hides clojure.core and cljs.core on their own toggle', () => {
    const off = { hideTests: false, hideExternals: false };
    expect(isCore(node('ext:clj//clojure.core/let', 'external'))).toBe(true);
    expect(isCore(node('ext:clj//cljs.core/str', 'external'))).toBe(true);
    expect(isCore(node('ext:clj//clojure.core', 'external'))).toBe(true);
    expect(isCore(node('ext:clj//clojure.core.async/go', 'external'))).toBe(false);
    expect(isCore(node('ext:clj//clojure.string/join', 'external'))).toBe(false);
    expect(isCore(node('clj:.//clojure.core/mine'))).toBe(false);
    expect(hidden(node('ext:clj//clojure.core/let', 'external'), off)).toBe(false);
    expect(hidden(node('ext:clj//clojure.core/let', 'external'), { ...off, hideCore: true })).toBe(true);
    expect(hidden(node('ext:clj//reagent.core/atom', 'external'), { ...off, hideCore: true })).toBe(false);
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

import { clampWidth, parsePanels } from './panels';

describe('panels', () => {
  it('clamps widths and survives bad saved data', () => {
    expect(clampWidth(10)).toBe(180);
    expect(clampWidth(5000)).toBe(720);
    expect(parsePanels('{"left": 300, "rightOpen": false}')).toEqual({ left: 300, right: 360, leftOpen: true, rightOpen: false });
    expect(parsePanels('not json').left).toBe(280);
    expect(parsePanels(null).rightOpen).toBe(true);
  });
});

describe('certainty levels', () => {
  it('hides only the unchecked levels', () => {
    const levels = { certain: true, likely: true, possible: false };
    expect(edgeShown({ from: 'a', to: 'b', kind: 'calls', certainty: 'possible' }, levels)).toBe(false);
    expect(edgeShown({ from: 'a', to: 'b', kind: 'calls', certainty: 'likely' }, levels)).toBe(true);
    expect(edgeShown({ from: 'a', to: 'b', kind: 'requires' }, levels)).toBe(true);
  });
});

describe('hasSource', () => {
  it('is false for units, namespaces and externals, true for symbols and impls', () => {
    expect(hasSource('unit:services/orders')).toBe(false);
    expect(hasSource('clj:services/orders//orders.core')).toBe(false);
    expect(hasSource('clj:.//shop.core')).toBe(false);
    expect(hasSource('ext:clj//clojure.core/map')).toBe(false);
    expect(hasSource('java:java.util.UUID')).toBe(false);
    expect(hasSource('clj:.//shop.core/checkout')).toBe(true);
    expect(hasSource('clj:.//shop.pricing/price@:book')).toBe(true);
    expect(hasSource('clj:.//shop.pricing//')).toBe(true);
    expect(hasSource('hole:dynamic-eval@src/shop/plugins.clj:12:3')).toBe(true);
  });
});
