import { describe, expect, it } from 'vitest';
import { highlight } from './highlight';

describe('highlight', () => {
  it('splits a line into comment, string, keyword, number and bracket tokens', () => {
    const [line] = highlight('(defn f [x] (str "a;b" :k 42)) ; done');
    expect(line?.map((t) => [t.kind, t.text])).toEqual([
      ['bracket', '('],
      ['plain', 'defn f '],
      ['bracket', '['],
      ['plain', 'x'],
      ['bracket', ']'],
      ['plain', ' '],
      ['bracket', '('],
      ['plain', 'str '],
      ['string', '"a;b"'],
      ['plain', ' '],
      ['keyword', ':k'],
      ['plain', ' '],
      ['number', '42'],
      ['bracket', ')'],
      ['bracket', ')'],
      ['plain', ' '],
      ['comment', '; done'],
    ]);
  });

  it('carries a string across lines and keeps escaped quotes inside it', () => {
    const lines = highlight('(def s "one\n\\"two\\"\nthree") 1');
    expect(lines[1]).toEqual([{ kind: 'string', text: '\\"two\\"' }]);
    expect(lines[2]?.[0]).toEqual({ kind: 'string', text: 'three"' });
    expect(lines[2]?.at(-1)).toEqual({ kind: 'number', text: '1' });
  });

  it('keeps markup as plain text', () => {
    expect(highlight('<script>alert(1)</script>')[0]?.map((t) => t.text).join('')).toBe('<script>alert(1)</script>');
  });
});
