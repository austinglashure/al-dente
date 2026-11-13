import { describe, expect, it } from 'vitest';
import { excludedNote } from './hotspotsText';

describe('excludedNote', () => {
  it('says how many findings the rules left out', () => {
    expect(excludedNote(0)).toBeNull();
    expect(excludedNote(1)).toBe('1 more finding in example, benchmark or dev code left out ([exclude] in the rules).');
    expect(excludedNote(12)).toBe('12 more findings in example, benchmark or dev code left out ([exclude] in the rules).');
  });
});
