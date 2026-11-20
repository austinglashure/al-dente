// The directory tree for the namespace view: directories open and close,
// and the graph shows the code at whatever level the tree is open to.
// Every repository string is rendered as React text.

import type { Child, TreeDir, TreeFile } from './types';

/** The row with this `dir:` id.
 *  Example: `findDir(root, 'dir:services/orders')?.label` is `'orders'`. */
export function findDir(root: TreeDir, id: string): TreeDir | null {
  if (root.id === id) return root;
  for (const d of root.dirs) {
    const found = findDir(d, id);
    if (found) return found;
  }
  return null;
}

/** Every ID inside a directory: its sub-directories and the namespaces of
 *  its files, at any depth.
 *  Example: `dirMembers(root, 'dir:services')` holds `dir:services/orders`
 *  and `clj:services/orders//orders.core`. */
export function dirMembers(root: TreeDir, id: string): Set<string> {
  const out = new Set<string>();
  const walk = (d: TreeDir) => {
    for (const f of d.files) for (const ns of f.namespaces) out.add(ns);
    for (const sub of d.dirs) {
      if (sub.id) out.add(sub.id);
      walk(sub);
    }
  };
  const d = findDir(root, id);
  if (d) walk(d);
  return out;
}

/** The ids of the top-level rows (what "collapse all" collapses).
 *  Example: `['dir:libs/common/src/common', 'dir:services']`. */
export function topDirIds(root: TreeDir): string[] {
  return root.dirs.flatMap((d) => (d.id ? [d.id] : []));
}

/** Every row's id, depth first.
 *  Example: `['dir:a', 'dir:a/b', 'dir:c']`. */
export function allDirIds(root: TreeDir): string[] {
  return root.dirs.flatMap((d) => [...(d.id ? [d.id] : []), ...allDirIds(d)]);
}

/** What a row holds, as detail-panel children: its sub-directories, then
 *  its analyzed files by namespace.
 *  Example: `services` gives `[{id: 'dir:services/orders', name: 'orders/', …}]`. */
export function dirChildren(root: TreeDir, id: string): Child[] {
  const d = findDir(root, id);
  if (!d) return [];
  const dirs: Child[] = d.dirs.flatMap((c) => (c.id ? [{ id: c.id, name: `${c.label}/`, kind: 'directory', subkind: null }] : []));
  const files: Child[] = d.files.flatMap((f) => f.namespaces.map((ns) => ({ id: ns, name: f.name, kind: 'namespace', subkind: null })));
  return [...dirs, ...files];
}

/** Why a file has no row to click, for its tooltip.
 *  Example: `fileNote({status: 'not-analyzed'})` explains source roots. */
function fileNote(f: TreeFile): string {
  if (f.status === 'not-analyzed') return 'Not analyzed: outside the source roots its build declares';
  if (f.status === 'parse-error') return 'Parse error: its facts may be incomplete';
  return f.path;
}

interface TreeProps {
  root: TreeDir;
  collapsed: Set<string>;
  selected: string | null;
  onToggle: (id: string) => void;
  onSelect: (id: string) => void;
  onExpandAll: () => void;
  onCollapseAll: () => void;
}

/** The tree panel with its expand-all and collapse-all buttons.
 *  Example: `<TreeView root={tree} collapsed={new Set()} … />`. */
export function TreeView(p: TreeProps) {
  return (
    <section className="tree" data-testid="tree">
      <div className="tree-head">
        <h2>Directories</h2>
        <button type="button" className="small" data-testid="tree-expand-all" onClick={p.onExpandAll} title="Show every namespace">
          expand all
        </button>
        <button type="button" className="small" data-testid="tree-collapse-all" onClick={p.onCollapseAll} title="Fold to the top-level directories">
          collapse all
        </button>
      </div>
      <Rows dir={p.root} {...p} />
    </section>
  );
}

/** The rows inside one directory: sub-directories first, then files. */
function Rows(p: TreeProps & { dir: TreeDir }) {
  return (
    <ul>
      {p.dir.dirs.map((d) => (d.id ? <DirRow key={d.id} {...p} d={d} /> : null))}
      {p.dir.files.map((f) => (
        <FileRow key={f.path} f={f} selected={p.selected} onSelect={p.onSelect} />
      ))}
    </ul>
  );
}

/** A directory row: a toggle, its label, and (when open) its rows. */
function DirRow(p: TreeProps & { d: TreeDir }) {
  const id = p.d.id ?? '';
  const open = !p.collapsed.has(id);
  return (
    <li className={id === p.selected ? 'selected' : ''}>
      <button type="button" className="chev" data-testid="tree-toggle" data-dir={id} aria-expanded={open} onClick={() => p.onToggle(id)} title={open ? 'Collapse' : 'Expand'}>
        {open ? '▾' : '▸'}
      </button>
      <button type="button" className="name dir" data-node-id={id} onClick={() => p.onSelect(id)} title={p.d.path}>
        {p.d.label}/
      </button>
      {open && <Rows {...p} dir={p.d} />}
    </li>
  );
}

/** A file row: one button per namespace it defines; greyed when skipped. */
function FileRow({ f, selected, onSelect }: { f: TreeFile; selected: string | null; onSelect: (id: string) => void }) {
  if (f.namespaces.length === 0) {
    return (
      <li className={`file ${f.status}`}>
        <span className="muted" title={fileNote(f)}>
          {f.name}
          {f.status === 'not-analyzed' ? ' (not analyzed)' : ''}
        </span>
      </li>
    );
  }
  return (
    <>
      {f.namespaces.map((ns) => (
        <li key={ns} className={`file ${f.status} ${ns === selected ? 'selected' : ''}`}>
          <button type="button" className="name" data-node-id={ns} onClick={() => onSelect(ns)} title={`${fileNote(f)} (${ns})`}>
            {f.status === 'parse-error' ? '⚠ ' : ''}
            {f.name}
          </button>
        </li>
      ))}
    </>
  );
}
