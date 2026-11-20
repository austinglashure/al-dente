import { describe, expect, it } from 'vitest';
import { placeTreeNodes, type Point } from './treePlace';

const members: Record<string, string[]> = {
  'dir:services': ['dir:services/orders', 'clj:o//orders.core', 'clj:b//billing.core'],
  'dir:services/orders': ['clj:o//orders.core'],
};
const membersOf = (id: string) => new Set(members[id] ?? []);
const noJitter = () => 0.5;

describe('placeTreeNodes', () => {
  it('keeps every node that was already on the canvas where it was', () => {
    const before = new Map<string, Point>([
      ['clj:c//common.util', { x: 5, y: 7 }],
      ['clj:o//orders.core', { x: 1, y: 1 }],
    ]);
    const p = placeTreeNodes(['clj:c//common.util', 'clj:o//orders.core'], before, new Map(), membersOf, noJitter);
    expect(p.get('clj:c//common.util')).toEqual({ x: 5, y: 7 });
    expect(p.get('clj:o//orders.core')).toEqual({ x: 1, y: 1 });
  });

  it('puts a collapsed directory at the centre of the nodes it replaces', () => {
    const before = new Map<string, Point>([
      ['clj:c//common.util', { x: 100, y: 100 }],
      ['clj:o//orders.core', { x: 0, y: 0 }],
      ['clj:b//billing.core', { x: 10, y: 20 }],
    ]);
    const p = placeTreeNodes(['clj:c//common.util', 'dir:services'], before, new Map(), membersOf, noJitter);
    expect(p.get('dir:services')).toEqual({ x: 5, y: 10 });
    expect(p.get('clj:c//common.util')).toEqual({ x: 100, y: 100 });
  });

  it('brings expanded members back where they were, or next to the directory', () => {
    const before = new Map<string, Point>([
      ['dir:services', { x: 50, y: 50 }],
      ['clj:c//common.util', { x: 100, y: 100 }],
    ]);
    const remembered = new Map<string, Point>([['clj:o//orders.core', { x: 3, y: 4 }]]);
    const p = placeTreeNodes(['clj:c//common.util', 'clj:o//orders.core', 'clj:b//billing.core'], before, remembered, membersOf, noJitter);
    expect(p.get('clj:o//orders.core')).toEqual({ x: 3, y: 4 });
    const b = p.get('clj:b//billing.core');
    expect(b).toBeDefined();
    expect(Math.hypot((b?.x ?? 0) - 50, (b?.y ?? 0) - 50)).toBeLessThan(10);
  });

  it('places a node with no known relative at the centre of the canvas', () => {
    const before = new Map<string, Point>([
      ['a', { x: 0, y: 0 }],
      ['b', { x: 10, y: 10 }],
    ]);
    const p = placeTreeNodes(['a', 'b', 'new'], before, new Map(), membersOf, noJitter);
    const n = p.get('new');
    expect(Math.hypot((n?.x ?? 0) - 5, (n?.y ?? 0) - 5)).toBeLessThan(10);
  });
});
