// Box select geometry: the rectangle a Shift-drag spans and the nodes in it.

export interface Point {
  x: number;
  y: number;
}

export interface Box {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** The box between two corners, whichever way the drag went.
 *  Example: `normalize({x: 10, y: 50}, {x: 0, y: 20})` is `{left: 0, top: 20, right: 10, bottom: 50}`. */
export function normalize(a: Point, b: Point): Box {
  return { left: Math.min(a.x, b.x), top: Math.min(a.y, b.y), right: Math.max(a.x, b.x), bottom: Math.max(a.y, b.y) };
}

/** The IDs of the points inside the box (edges included), in input order.
 *  Example: `inBox([{id: 'a', x: 5, y: 30}], {left: 0, top: 20, right: 10, bottom: 50})` is `['a']`. */
export function inBox(points: (Point & { id: string })[], box: Box): string[] {
  return points.filter((p) => p.x >= box.left && p.x <= box.right && p.y >= box.top && p.y <= box.bottom).map((p) => p.id);
}
