import { describe, expect, it } from 'vitest';
import { parseViewPrefs } from './viewPrefs';

describe('parseViewPrefs', () => {
  it('defaults to units and top 250', () => {
    expect(parseViewPrefs(null)).toEqual({ view: 'units', top: 250 });
  });
  it('keeps a valid saved choice', () => {
    expect(parseViewPrefs('{"view":"namespaces","top":500}')).toEqual({ view: 'namespaces', top: 500 });
  });
  it('drops invalid parts and garbage', () => {
    expect(parseViewPrefs('{"view":"galaxy","top":7}')).toEqual({ view: 'units', top: 250 });
    expect(parseViewPrefs('not json')).toEqual({ view: 'units', top: 250 });
  });
});
