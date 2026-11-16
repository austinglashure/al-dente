// The workspace in the running app: every canvas change is an `Op` that is
// applied, logged and snapshotted, so it can be undone and redone; the log
// and positions go to the URL hash after each change; a hash or a saved
// name replays into the same view.

import { useCallback, useEffect, useRef, useState, type RefObject } from 'react';
import type { SerializedGraph } from 'graphology-types';
import { batched, cloneExpansions, type Attachable, type Expansions } from './expansion';
import { cancelLayout } from './layout';
import type { CanvasGraph, EdgeAttrs, NodeAttrs } from './model';
import { decode, encode, fromHash, record, redo as redoStep, toHash, undo as undoStep, type Op, type Timeline, type Workspace } from './workspace';

/** Everything needed to put the canvas back as it was. `E` is the app
 *  state outside the graph (breadcrumb, filters, hide toggles). */
interface Snap<E> {
  ops: Op[];
  graph: SerializedGraph<NodeAttrs, EdgeAttrs>;
  expansions: Expansions;
  extra: E;
}

interface Deps<E> {
  graph: CanvasGraph;
  expansions: Expansions;
  renderer: RefObject<Attachable | null>;
  /** Performs one step on the canvas. */
  apply: (op: Op) => Promise<void>;
  /** The app state outside the graph, and how to put it back. */
  capture: () => E;
  restore: (e: E) => void;
  redraw: () => void;
  onNotice: (message: string | null) => void;
  /** Bumped on every redraw; the URL follows it (positions settle after layout). */
  version: number;
}

const SAVED = 'aldente.saved';

/** Undo keeps this many steps (each holds a copy of the canvas). */
const HISTORY = 30;

/** Reads the saved workspaces (name → encoded), empty if storage is unavailable.
 *  Example: `readSaved()` is `{ 'billing only': 'q1Yq…' }`. */
function readSaved(): Record<string, string> {
  try {
    return JSON.parse(localStorage.getItem(SAVED) ?? '{}') as Record<string, string>;
  } catch {
    return {};
  }
}

/** A copy of a serialized graph whose attribute objects are its own, so the
 *  live graph never writes into a snapshot.
 *  Example: `fresh(g.export())`. */
function fresh(s: SerializedGraph<NodeAttrs, EdgeAttrs>): SerializedGraph<NodeAttrs, EdgeAttrs> {
  return {
    ...s,
    nodes: s.nodes.map((n) => ({ ...n, attributes: n.attributes ? { ...n.attributes } : undefined })),
    edges: s.edges.map((e) => ({ ...e, attributes: e.attributes ? { ...e.attributes } : undefined })),
  };
}

/** The workspace controller.
 *  Example: `const ws = useWorkspace({ graph, expansions, renderer, apply, capture, restore, redraw, onNotice, version })`. */
export function useWorkspace<E>(d: Deps<E>) {
  const ops = useRef<Op[]>([]);
  const timeline = useRef<Timeline<Snap<E>> | null>(null);
  const [steps, setSteps] = useState({ back: 0, forward: 0 });
  const [saved, setSaved] = useState<string[]>(() => Object.keys(readSaved()).sort());
  const deps = useRef(d);
  useEffect(() => {
    deps.current = d;
  });

  const snap = useCallback((): Snap<E> => {
    const { graph, expansions, capture } = deps.current;
    return { ops: [...ops.current], graph: fresh(graph.export()), expansions: cloneExpansions(expansions), extra: capture() };
  }, []);

  const current = useCallback((): Workspace => {
    const positions: Workspace['positions'] = {};
    deps.current.graph.forEachNode((id, a) => {
      positions[id] = [Math.round(a.x), Math.round(a.y)];
    });
    return { v: 1, ops: ops.current, positions };
  }, []);

  /** Writes the workspace to the URL hash; with no step beyond the first
   *  view, the hash is cleared. */
  const syncUrl = useCallback(async () => {
    if (ops.current.length <= 1) {
      if (location.hash) history.replaceState(null, '', `${location.pathname}${location.search}`);
      return;
    }
    const { hash, stored } = await toHash(current(), localStorage);
    history.replaceState(null, '', `${location.pathname}${location.search}${hash}`);
    if (stored) deps.current.onNotice(`This view is too large for a link, so it is kept in this browser under the key ${stored}; the link works in this browser only.`);
  }, [current]);

  const settle = useCallback(() => {
    const t = timeline.current;
    setSteps({ back: t?.past.length ?? 0, forward: t?.future.length ?? 0 });
  }, []);

  /** Re-takes the present snapshot: positions settle after a step is
   *  recorded (the layout runs in a worker), so undo and redo would
   *  otherwise bring back the positions from before the layout. */
  const refreshPresent = useCallback(() => {
    if (timeline.current) timeline.current = { ...timeline.current, present: snap() };
  }, [snap]);

  /** Applies a step, logs it and makes it undoable. */
  const run = useCallback(
    async (op: Op) => {
      refreshPresent();
      await deps.current.apply(op);
      ops.current.push(op);
      const next = snap();
      timeline.current = timeline.current ? record(timeline.current, next, HISTORY) : { past: [], present: next, future: [] };
      settle();
      void syncUrl();
    },
    [snap, settle, syncUrl, refreshPresent],
  );

  /** Puts a snapshot back on the canvas. */
  const restore = useCallback(
    (s: Snap<E>) => {
      const { graph, expansions, renderer, restore: restoreExtra, redraw } = deps.current;
      cancelLayout();
      batched(renderer.current, graph, () => {
        graph.clear();
        graph.import(fresh(s.graph));
        expansions.clear();
        for (const [k, r] of cloneExpansions(s.expansions)) expansions.set(k, r);
      });
      ops.current = [...s.ops];
      restoreExtra(s.extra);
      redraw();
      void syncUrl();
    },
    [syncUrl],
  );

  const undo = useCallback(() => {
    if (!timeline.current || timeline.current.past.length === 0) return;
    refreshPresent();
    timeline.current = undoStep(timeline.current);
    restore(timeline.current.present);
    settle();
  }, [restore, settle, refreshPresent]);

  const redo = useCallback(() => {
    if (!timeline.current || timeline.current.future.length === 0) return;
    refreshPresent();
    timeline.current = redoStep(timeline.current);
    restore(timeline.current.present);
    settle();
  }, [restore, settle, refreshPresent]);

  /** Marks the current canvas as the start of history; `base` is the step
   *  that drew it (the first view), kept so a replay starts from it too. */
  const start = useCallback(
    (base?: Op) => {
      if (base) ops.current = [base];
      timeline.current = { past: [], present: snap(), future: [] };
      settle();
    },
    [snap, settle],
  );

  /** Rebuilds a workspace: replays its steps, then puts nodes where they were. */
  const replay = useCallback(
    async (ws: Workspace) => {
      ops.current = [];
      timeline.current = null;
      for (const op of ws.ops) {
        await deps.current.apply(op);
        ops.current.push(op);
      }
      const { graph, renderer, redraw } = deps.current;
      cancelLayout();
      batched(renderer.current, graph, () => {
        graph.updateEachNodeAttributes((id, a) => {
          const p = ws.positions[id];
          return p ? { ...a, x: p[0], y: p[1] } : a;
        });
      });
      redraw();
      start();
      void syncUrl();
    },
    [start, syncUrl],
  );

  /** Replays the workspace in the URL hash; false if there is none. */
  const fromUrl = useCallback(async (): Promise<boolean> => {
    const { ws, missing } = await fromHash(location.hash, localStorage);
    if (missing) deps.current.onNotice(`This link names a view kept in another browser (key ${missing}); showing the default view.`);
    if (!ws) return false;
    await replay(ws);
    return true;
  }, [replay]);

  const save = useCallback(
    async (name: string) => {
      const all = { ...readSaved(), [name]: await encode(current()) };
      localStorage.setItem(SAVED, JSON.stringify(all));
      setSaved(Object.keys(all).sort());
    },
    [current],
  );

  const load = useCallback(
    async (name: string) => {
      const enc = readSaved()[name];
      const ws = enc ? await decode(enc) : null;
      if (ws) await replay(ws);
    },
    [replay],
  );

  const remove = useCallback((name: string) => {
    const all = Object.fromEntries(Object.entries(readSaved()).filter(([k]) => k !== name));
    localStorage.setItem(SAVED, JSON.stringify(all));
    setSaved(Object.keys(all).sort());
  }, []);

  // Positions settle after the layout worker answers; keep the URL current.
  const { version } = d;
  useEffect(() => {
    const t = setTimeout(() => void syncUrl(), 400);
    return () => clearTimeout(t);
  }, [version, syncUrl]);

  return { run, undo, redo, start, fromUrl, save, load, remove, saved, canUndo: steps.back > 0, canRedo: steps.forward > 0 };
}
