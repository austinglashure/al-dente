// Which details sections are folded, remembered per browser. A convenience
// only: unreadable storage means everything starts open.

const KEY = 'aldente.details.folded';

type Store = Pick<Storage, 'getItem' | 'setItem'>;

/** The folded section keys, or none if storage is missing or garbled.
 *  Example: after `writeFolded(s, new Set(['caller']))`, `readFolded(s)` is `Set {'caller'}`. */
export function readFolded(s: Store): Set<string> {
  try {
    const v: unknown = JSON.parse(s.getItem(KEY) ?? '[]');
    return new Set(Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : []);
  } catch {
    return new Set();
  }
}

/** Remembers the folded section keys; silently does nothing if storage fails.
 *  Example: `writeFolded(localStorage, new Set(['tests']))`. */
export function writeFolded(s: Store, folded: Set<string>): void {
  try {
    s.setItem(KEY, JSON.stringify([...folded].sort()));
  } catch {
    // Private window or blocked storage: folding still works for this page.
  }
}
