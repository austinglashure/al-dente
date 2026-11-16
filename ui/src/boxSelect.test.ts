import { describe, expect, it } from 'vitest';
import { inBox, normalize } from './boxSelect';

describe('box select', () => {
  it('normalizes a drag in any direction', () => {
    expect(normalize({ x: 10, y: 50 }, { x: 0, y: 20 })).toEqual({ left: 0, top: 20, right: 10, bottom: 50 });
  });

  it('returns the points inside the box, edges included', () => {
    const pts = [
      { id: 'a', x: 5, y: 30 },
      { id: 'b', x: 10, y: 50 },
      { id: 'c', x: 11, y: 30 },
    ];
    expect(inBox(pts, normalize({ x: 0, y: 20 }, { x: 10, y: 50 }))).toEqual(['a', 'b']);
  });
});
