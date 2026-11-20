import { describe, expect, it } from 'vitest';
import { shownLabel } from './nodeLabel';

describe('shownLabel', () => {
  const plain = { quiet: false, faded: false, hovered: false };

  it('keeps the label with markers', () => {
    expect(shownLabel('checkout', plain)).toBe('checkout');
    expect(shownLabel('checkout', { ...plain, mark: '★ ', pinned: true })).toBe('📌 ★ checkout');
  });

  it('blanks labels hidden by zoom or faded by a highlight', () => {
    expect(shownLabel('checkout', { ...plain, quiet: true })).toBe('');
    expect(shownLabel('checkout', { ...plain, faded: true })).toBe('');
  });

  it('always names the node under the cursor', () => {
    expect(shownLabel('checkout', { quiet: true, faded: true, hovered: true })).toBe('checkout');
    expect(shownLabel('checkout', { quiet: true, faded: false, hovered: true, pinned: true })).toBe('📌 checkout');
  });
});
