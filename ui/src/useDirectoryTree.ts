// The directory tree of the namespace view: loaded the first time that view
// is shown, with the set of folded directories.

import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { Api } from './api';
import type { TreeDir } from './types';

/** Tree state and folding.
 *  Example: `toggleDir('dir:services')` folds `services/`; again unfolds it. */
export function useDirectoryTree(api: Api, view: 'units' | 'namespaces', fail: (e: unknown) => void) {
  const [tree, setTree] = useState<TreeDir | null>(null);
  // The tree as of now, for callbacks that run before a re-render (a
  // directory picked from search hands its tree over and selects at once).
  const treeRef = useRef(tree);
  useEffect(() => {
    if (tree) treeRef.current = tree;
  }, [tree]);
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set());
  // Read at load time, so loaders do not change identity when it does.
  const collapsedRef = useRef(collapsed);
  useLayoutEffect(() => {
    collapsedRef.current = collapsed;
  }, [collapsed]);

  useEffect(() => {
    if (view === 'namespaces' && tree === null) api.tree().then(setTree).catch(fail);
  }, [view, tree, api, fail]);

  /** Folds or unfolds one directory of the tree. */
  const toggleDir = useCallback((id: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  /** Takes a tree loaded elsewhere (search) if none is loaded yet. */
  const adoptTree = useCallback((t: TreeDir) => {
    treeRef.current = treeRef.current ?? t;
    setTree((prev) => prev ?? t);
  }, []);

  return { tree, treeRef, collapsed, setCollapsed, collapsedRef, toggleDir, adoptTree };
}
