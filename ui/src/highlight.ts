// Client-side syntax highlighting for Clojure source (`06-gui.md`: source
// is shown as text nodes only; highlighting happens on the text). Tokens
// are plain strings with a kind; nothing here produces markup.

export type TokenKind = 'plain' | 'comment' | 'string' | 'keyword' | 'number' | 'bracket';

export interface Token {
  kind: TokenKind;
  text: string;
}

const BRACKETS = new Set(['(', ')', '[', ']', '{', '}']);

/** Appends text to the line's tokens, merging with the last token of the same kind.
 *  Example: pushing plain "a" then plain "b" gives one plain "ab". */
function push(line: Token[], kind: TokenKind, text: string): void {
  const last = line.at(-1);
  if (last && last.kind === kind && kind !== 'bracket') last.text += text;
  else line.push({ kind, text });
}

/** Tokens per line; strings may span lines.
 *  Example: `highlight('(f :a)')` is `[[bracket "(", plain "f ", keyword ":a", bracket ")"]]`. */
export function highlight(source: string): Token[][] {
  const out: Token[][] = [];
  let inString = false;
  for (const text of source.split('\n')) {
    const line: Token[] = [];
    let i = 0;
    while (i < text.length) {
      const c = text[i] ?? '';
      if (inString) {
        let j = i;
        while (j < text.length && text[j] !== '"') j += text[j] === '\\' ? 2 : 1;
        const end = Math.min(j + 1, text.length);
        push(line, 'string', text.slice(i, end));
        if (j < text.length) inString = false;
        i = end;
      } else if (c === '"') {
        inString = true;
        push(line, 'string', '"');
        i += 1;
      } else if (c === ';') {
        push(line, 'comment', text.slice(i));
        i = text.length;
      } else if (BRACKETS.has(c)) {
        push(line, 'bracket', c);
        i += 1;
      } else if (c === ':' && /[^\s()[\]{}"]/.test(text[i + 1] ?? ' ')) {
        const m = /^:[^\s()[\]{}",;]+/.exec(text.slice(i));
        push(line, 'keyword', m?.[0] ?? c);
        i += m?.[0].length ?? 1;
      } else if (/[0-9]/.test(c) && !/[\w\-*!?>]/.test(text[i - 1] ?? ' ')) {
        const m = /^[0-9][0-9./xXa-fA-FNMr]*/.exec(text.slice(i));
        push(line, 'number', m?.[0] ?? c);
        i += m?.[0].length ?? 1;
      } else {
        push(line, 'plain', c);
        i += 1;
      }
    }
    out.push(line);
  }
  return out;
}
