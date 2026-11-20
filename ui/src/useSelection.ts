// What is selected and what the right-hand panel shows for it: details,
// source, a call site in context, an edge's sites, unit links and what
// "Open in editor" did last.

import { useCallback, useEffect, useState, type RefObject } from 'react';
import type { Api } from './api';
import { hasSource, labelFor, type CanvasGraph } from './model';
import { dirChildren, findDir } from './tree';
import type { GNode, Neighbor, ShowResult, TreeDir, UnitLinks } from './types';

export interface SiteView {
  file: string;
  line: number;
  text: string;
  first: number;
}

/** Selection state and the actions that change it.
 *  Example: `select('clj:.//shop.core/checkout')` loads its details and source;
 *  Escape deselects. */
export function useSelection(api: Api, graph: CanvasGraph, treeRef: RefObject<TreeDir | null>, fail: (e: unknown) => void) {
  const [selected, setSelected] = useState<string | null>(null);
  const [details, setDetails] = useState<ShowResult | null>(null);
  const [source, setSource] = useState<{ text: string; first: number } | null>(null);
  const [siteView, setSiteView] = useState<SiteView | null>(null);
  const [edgeSel, setEdgeSel] = useState<string | null>(null);
  const [editorNote, setEditorNote] = useState<string | null>(null);
  const [unitLinks, setUnitLinks] = useState<UnitLinks | null>(null);

  const select = useCallback(
    (id: string) => {
      setSelected(id);
      setDetails(null);
      setSource(null);
      setSiteView(null);
      setEdgeSel(null);
      setEditorNote(null);
      setUnitLinks(null);
      if (id.startsWith('unit:')) api.unitLinks(id).then(setUnitLinks).catch(fail);
      if (id.startsWith('dir:')) {
        // Directories are not stored: describe one from the tree and the
        // edges on the canvas.
        const tree = treeRef.current;
        const node: GNode = graph.hasNode(id) ? graph.getNodeAttribute(id, 'node') : { id, kind: 'directory', name: (tree && findDir(tree, id)?.path) ?? id };
        const neighbor = (key: string, other: string): Neighbor => {
          const e = graph.getEdgeAttribute(key, 'edge');
          const o = graph.hasNode(other) ? graph.getNodeAttribute(other, 'node') : null;
          return { id: other, name: o ? labelFor(o) : other, kind: e.kind, certainty: e.certainty ?? null, reason: e.reason ?? null, sites: e.sites ?? [], count: e.count ?? null };
        };
        const has = graph.hasNode(id);
        setDetails({
          node,
          callers: has ? graph.inEdges(id).map((k) => neighbor(k, graph.source(k))) : [],
          callees: has ? graph.outEdges(id).map((k) => neighbor(k, graph.target(k))) : [],
          children: tree ? dirChildren(tree, id) : [],
        });
        return;
      }
      api.node(id).then(setDetails).catch(fail);
      if (hasSource(id)) {
        api
          .sourceAt(id)
          .then(setSource)
          .catch(() => setSource({ text: '', first: 1 }));
      }
    },
    [api, fail, graph, treeRef],
  );

  const deselect = useCallback(() => {
    setSelected(null);
    setDetails(null);
    setSource(null);
    setSiteView(null);
    setEdgeSel(null);
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') deselect();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [deselect]);

  /** Shows a call site's line in context. */
  const showSite = useCallback(
    (site: { file: string; line: number }) =>
      void api
        .siteSource(site.file, site.line)
        .then((r) => setSiteView({ file: site.file, line: site.line, ...r }))
        .catch(fail),
    [api, fail],
  );

  /** Shows an edge's call sites (and closes a site view). */
  const selectEdge = useCallback((key: string | null) => {
    setEdgeSel(key);
    setSiteView(null);
  }, []);

  return { selected, details, source, siteView, edgeSel, editorNote, unitLinks, select, deselect, showSite, selectEdge, setEdgeSel, setSiteView, setEditorNote };
}
