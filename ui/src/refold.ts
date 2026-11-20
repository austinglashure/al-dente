// The canvas update after a directory folds or unfolds: the new namespace map
// replaces the old one, with positions from `placeTreeNodes` so the picture
// stays put. No layout runs.

import { batched, type Attachable, type Expansions } from './expansion';
import { mergePayload, type CanvasGraph } from './model';
import { placeTreeNodes, type Point } from './treePlace';
import type { Payload } from './types';

/** Replaces the canvas with `p`, keeping positions. `remembered` collects
 *  every position seen, so members come back where they were.
 *  Example: folding `services/` leaves every node outside it where it was. */
export function refoldCanvas(
  graph: CanvasGraph,
  renderer: Attachable | null,
  expansions: Expansions,
  p: Payload,
  membersOf: (dir: string) => Set<string>,
  remembered: Map<string, Point>,
): void {
  const before = new Map<string, Point>();
  graph.forEachNode((id, a) => before.set(id, { x: a.x, y: a.y }));
  for (const [id, xy] of before) remembered.set(id, xy);
  const placed = placeTreeNodes(
    p.nodes.map((n) => n.id),
    before,
    remembered,
    membersOf,
  );
  batched(renderer, graph, () => {
    graph.clear();
    expansions.clear();
    mergePayload(graph, p);
    graph.forEachNode((id) => {
      const xy = placed.get(id);
      if (xy) graph.mergeNodeAttributes(id, xy);
    });
  });
}
