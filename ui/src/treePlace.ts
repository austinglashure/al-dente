// Where nodes go when a directory of the tree graph folds or unfolds, so the
// picture stays put: survivors keep their place, a folded directory sits at
// the centre of what it replaced, unfolded members come back where they were
// (or next to their directory).

export interface Point {
  x: number;
  y: number;
}

/** Spread of members placed around their directory, in graph units. */
const SPREAD = 4;

/** The mean of some points, or null for none.
 *  Example: `centroid([{x:0,y:0},{x:10,y:20}])` is `{x:5,y:10}`. */
function centroid(points: Point[]): Point | null {
  if (points.length === 0) return null;
  const x = points.reduce((s, p) => s + p.x, 0) / points.length;
  const y = points.reduce((s, p) => s + p.y, 0) / points.length;
  return { x, y };
}

/** A point near `p`, `random` giving values in [0, 1).
 *  Example: `near({x:0,y:0}, () => 0.5)` is `{x:0,y:0}`. */
function near(p: Point, random: () => number): Point {
  return { x: p.x + (random() - 0.5) * SPREAD, y: p.y + (random() - 0.5) * SPREAD };
}

/** Positions for the nodes of the new tree graph.
 *  `before`: positions on the canvas now; `remembered`: positions nodes had
 *  before an earlier fold; `membersOf(dir)`: every ID inside a directory.
 *  Example: folding `dir:services` whose two namespaces sat at (0,0) and
 *  (10,20) puts `dir:services` at (5,10) and leaves every other node alone. */
export function placeTreeNodes(
  ids: string[],
  before: Map<string, Point>,
  remembered: Map<string, Point>,
  membersOf: (dir: string) => Set<string>,
  random: () => number = Math.random,
): Map<string, Point> {
  const out = new Map<string, Point>();
  const middle = centroid([...before.values()]) ?? { x: 0, y: 0 };
  const oldDirs = [...before.keys()].filter((id) => id.startsWith('dir:'));
  for (const id of ids) {
    const kept = before.get(id) ?? remembered.get(id);
    if (kept) {
      out.set(id, kept);
      continue;
    }
    if (id.startsWith('dir:')) {
      const replaced = [...membersOf(id)].flatMap((m) => before.get(m) ?? []);
      const c = centroid(replaced);
      if (c) {
        out.set(id, c);
        continue;
      }
    }
    // Unfolded: next to the innermost directory on the canvas that held it.
    const holders = oldDirs.filter((d) => membersOf(d).has(id)).sort((a, b) => b.length - a.length);
    const holder = holders[0] ? before.get(holders[0]) : undefined;
    out.set(id, near(holder ?? middle, random));
  }
  return out;
}
