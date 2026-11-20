// Operations on the main canvas: replace it with a loaded view (keeping a
// breadcrumb), open a unit, namespace or directory, expand or collapse an
// aggregate in place, drop nodes, add a node's neighbors, focus on a node.

import { useCallback, useRef, type RefObject } from 'react';
import type { Api } from './api';
import { applyPage, batched, collapse, nextOffset, type Attachable, type Expansions } from './expansion';
import { layout } from './layout';
import { dropNodes, labelFor, mergePayload, type CanvasGraph } from './model';
import type { GNode, Payload } from './types';

/** One level of the breadcrumb: its label and how to load its canvas. */
export interface Crumb {
  label: string;
  load: () => Promise<Payload>;
}

interface Deps {
  api: Api;
  graph: CanvasGraph;
  renderer: RefObject<Attachable | null>;
  expansions: Expansions;
  crumbsRef: RefObject<Crumb[]>;
  setCrumbs: (c: Crumb[]) => void;
  setFirstShown: (shown: boolean) => void;
  setError: (e: string | null) => void;
  fail: (e: unknown) => void;
  relayout: () => void;
  redraw: () => void;
  edgeVisible: (key: string) => boolean;
}

/** The canvas operations, each a stable callback.
 *  Example: `openOnCanvas('unit:services/orders')` shows that unit's namespaces
 *  as a new breadcrumb level. */
export function useCanvasOps({ api, graph, renderer, expansions, crumbsRef, setCrumbs, setFirstShown, setError, fail, relayout, redraw, edgeVisible }: Deps) {
  /** Replaces the canvas with `load`'s payload and makes it the last crumb. */
  const show = useCallback(
    (crumb: Crumb, keep: Crumb[]): Promise<void> =>
      crumb
        .load()
        .then((p) => {
          batched(renderer.current, graph, () => {
            graph.clear();
            expansions.clear();
            mergePayload(graph, p);
          });
          setCrumbs([...keep, crumb]);
          setFirstShown(true);
          setError(null);
          relayout();
          redraw();
        })
        .catch(fail),
    [graph, renderer, expansions, setCrumbs, setFirstShown, setError, relayout, redraw, fail],
  );

  /** Opens a unit (its namespaces) or a namespace (its functions, with the
   *  namespaces it talks to as context) on a canvas of its own. */
  const openOnCanvas = useCallback(
    (id: string): Promise<void> => {
      const n: GNode = graph.hasNode(id)
        ? graph.getNodeAttribute(id, 'node')
        : id.startsWith('dir:')
          ? { id, kind: 'directory', name: id.slice('dir:'.length) }
          : { id, kind: id.startsWith('unit:') ? 'unit' : 'namespace', name: id.split('//').pop() ?? id };
      let load: () => Promise<Payload>;
      if (n.kind === 'directory') {
        load = () => api.dirGraph(id.slice('dir:'.length)).then((r) => r.data);
      } else if (n.kind === 'unit') {
        load = () => api.unitGraph(n.id).then((r) => r.data);
      } else {
        const context = graph.filterNodes((_id, a) => a.node.kind === 'namespace' || a.node.kind === 'external').map((id) => graph.getNodeAttribute(id, 'node'));
        load = () =>
          api.expand(n.id, context.map((c) => c.id)).then((p) => {
            const ends = new Set(p.edges.flatMap((e) => [e.from, e.to]));
            return { ...p, nodes: [...p.nodes, ...context.filter((c) => ends.has(c.id))] };
          });
      }
      return show({ label: labelFor(n), load }, crumbsRef.current);
    },
    [api, graph, show, crumbsRef],
  );

  // Aggregates with a page request in flight: a second click meanwhile is
  // ignored instead of asking for the same page again.
  const expanding = useRef(new Set<string>());

  /** Expands an aggregate in place, or loads its next page if it is
   *  expanded already; only the new nodes move. */
  const expandHere = useCallback(
    (id: string): Promise<void> => {
      if (expanding.current.has(id)) return Promise.resolve();
      if (!graph.hasNode(id)) {
        return api
          .expand(id, graph.nodes())
          .then((p) => {
            mergePayload(graph, p);
            relayout();
          })
          .catch(fail);
      }
      expanding.current.add(id);
      return api
        .expandPage(id, graph.nodes(), nextOffset(expansions, id))
        .then((p) => {
          let added: string[] = [];
          batched(renderer.current, graph, () => {
            added = applyPage(graph, expansions, id, p);
          });
          layout(graph, redraw, edgeVisible, new Set(added));
          redraw();
        })
        .catch(fail)
        .finally(() => expanding.current.delete(id));
    },
    [api, graph, renderer, expansions, relayout, redraw, edgeVisible, fail],
  );

  /** Collapses an aggregate expanded in place back into one node. */
  const collapseHere = useCallback(
    (id: string) => {
      batched(renderer.current, graph, () => collapse(graph, expansions, id));
      redraw();
    },
    [graph, renderer, expansions, redraw],
  );

  /** Removes nodes from the canvas in one batch. */
  const drop = useCallback(
    (ids: string[]) => {
      batched(renderer.current, graph, () => dropNodes(graph, ids));
      redraw();
    },
    [graph, renderer, redraw],
  );

  const neighbors = useCallback(
    (id: string) =>
      api
        .neighborhood(id)
        .then((p) => {
          mergePayload(graph, p, graph.hasNode(id) ? id : undefined);
          relayout();
        })
        .catch(fail),
    [api, graph, relayout, fail],
  );

  const focusOn = useCallback(
    (id: string): Promise<void> => {
      const label = graph.hasNode(id) ? labelFor(graph.getNodeAttribute(id, 'node')) : id;
      return show({ label: `${label} (focus)`, load: () => api.neighborhood(id) }, crumbsRef.current);
    },
    [api, graph, show, crumbsRef],
  );

  return { show, openOnCanvas, expandHere, collapseHere, drop, neighbors, focusOn };
}
