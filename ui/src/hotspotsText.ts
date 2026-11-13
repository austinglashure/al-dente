// Text for the hotspot list that is worth testing on its own.

/** The note under the hotspot list for findings the rules left out, or
 *  null when none were.
 *  Example: `excludedNote(2)` is `'2 more findings in example, benchmark or
 *  dev code left out ([exclude] in the rules).'`. */
export function excludedNote(excluded: number): string | null {
  if (excluded <= 0) return null;
  const noun = excluded === 1 ? 'finding' : 'findings';
  return `${excluded} more ${noun} in example, benchmark or dev code left out ([exclude] in the rules).`;
}
