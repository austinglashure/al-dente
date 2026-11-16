import { describe, expect, it } from 'vitest';
import { NO_FILTERS, passes, type Filters } from './filters';
import type { GNode } from './types';

const fn = (id: string, extra: Partial<GNode> = {}): GNode => ({ id, kind: 'symbol', subkind: 'fn', name: id.split('/').pop() ?? id, unit: 'unit:services/orders', ...extra });
const f = (over: Partial<Filters>): Filters => ({ ...NO_FILTERS, ...over });

describe('passes', () => {
  it('lets everything through with no filters', () => {
    expect(passes(fn('clj:services/orders//o/total'), NO_FILTERS)).toBe(true);
  });

  it('filters by unit (a unit node counts as its own unit), kind or subkind, and every flag', () => {
    expect(passes(fn('a'), f({ units: ['unit:services/billing'] }))).toBe(false);
    expect(passes({ id: 'unit:services/billing', kind: 'unit', name: 'services/billing' }, f({ units: ['unit:services/billing'] }))).toBe(true);
    expect(passes(fn('a'), f({ kinds: ['namespace'] }))).toBe(false);
    expect(passes(fn('a'), f({ kinds: ['fn'] }))).toBe(true);
    expect(passes(fn('a', { flags: ['entry-point'] }), f({ flags: ['entry-point', 'test'] }))).toBe(false);
    expect(passes(fn('a', { flags: ['entry-point', 'test'] }), f({ flags: ['entry-point'] }))).toBe(true);
  });

  it('matches the name pattern anywhere in the name or ID, ignoring case', () => {
    expect(passes(fn('clj:services/orders//orders.core/Total'), f({ name: 'total' }))).toBe(true);
    expect(passes(fn('clj:services/orders//orders.core/total'), f({ name: 'orders.core' }))).toBe(true);
    expect(passes(fn('clj:services/orders//orders.core/total'), f({ name: 'billing' }))).toBe(false);
  });

  it('keeps nodes at or above every measure minimum; a node without the measure fails', () => {
    const fanIn = new Map([['a', 5], ['b', 1]]);
    const measure = (_m: string, id: string) => fanIn.get(id);
    const only = f({ measures: [{ metric: 'fan_in', min: 3 }] });
    expect(passes(fn('a'), only, measure)).toBe(true);
    expect(passes(fn('b'), only, measure)).toBe(false);
    expect(passes(fn('c'), only, measure)).toBe(false);
  });
});
