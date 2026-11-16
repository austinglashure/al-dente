// The left panel: search, display toggles, the certainty legend, notices,
// libraries, blind spots and the list of what is on the canvas (or the
// directory tree in the namespace view). Every repository string is
// rendered as React text.

import { useState, type ReactNode } from 'react';
import type { Api } from './api';
import { LibraryList } from './Libraries';
import { certaintyColor, isAggregate, labelFor, type HideOptions, type Levels } from './model';
import { TreeView, topDirIds } from './tree';
import type { BlindSpot, GNode, LibrarySummary, TreeDir } from './types';

/** The hide toggles, in display order, with their labels. */
const HIDE_TOGGLES: { key: keyof HideOptions; testId: string; label: string; title?: string }[] = [
  { key: 'hideTests', testId: 'hide-tests', label: 'Hide tests' },
  { key: 'hideExternals', testId: 'hide-externals', label: 'Hide external libraries' },
  { key: 'hideInterop', testId: 'hide-interop', label: 'Hide interop exits (Java, JS)' },
  {
    key: 'hideCore',
    testId: 'hide-core',
    label: 'Hide Clojure core (let, or, map…)',
    title: 'clojure.core and cljs.core: let, or, map, str, … Shown again when unchecked.',
  },
];

interface Props {
  api: Api;
  hide: HideOptions;
  onHide: (next: HideOptions) => void;
  levels: Levels;
  counts: Record<keyof Levels, number>;
  onLevel: (level: keyof Levels, on: boolean) => void;
  notice: string | null;
  error: string | null;
  onError: (e: unknown) => void;
  libs: LibrarySummary[];
  selectedLib: string | null;
  onPickLib: (id: string) => void;
  spots: BlindSpot[];
  /** The tree, when the namespace map is the root canvas; else null. */
  tree: TreeDir | null;
  collapsed: Set<string>;
  onToggleDir: (id: string) => void;
  onSetCollapsed: (next: Set<string>) => void;
  visible: GNode[];
  selected: string | null;
  onSelect: (id: string) => void;
  onFollow: (id: string) => void;
  onOpen: (n: GNode) => void;
  /** Overlay controls, under the certainty legend. */
  overlays?: ReactNode;
  /** Hotspots and cycles, above the libraries. */
  views?: ReactNode;
  /** Undo, redo, saved workspaces and filters, under the search box. */
  workspace?: ReactNode;
}

/** The left panel.
 *  Example: `<LeftPanel api={api} hide={{ hideTests: false, … }} … />`. */
export function LeftPanel(p: Props) {
  return (
    <aside className="left">
      <SearchBox api={p.api} onError={p.onError} onFollow={p.onFollow} />
      {p.workspace}
      {HIDE_TOGGLES.map((t) => (
        <label key={t.key} title={t.title}>
          <input type="checkbox" data-testid={t.testId} checked={!!p.hide[t.key]} onChange={(e) => p.onHide({ ...p.hide, [t.key]: e.target.checked })} />{' '}
          {t.label}
        </label>
      ))}
      <fieldset className="legend" data-testid="legend">
        <legend>Edges by certainty</legend>
        {(['certain', 'likely', 'possible'] as const).map((level) => (
          <label key={level} data-testid={`legend-${level}`}>
            <input type="checkbox" data-testid={`show-${level}`} checked={p.levels[level]} onChange={(e) => p.onLevel(level, e.target.checked)} />{' '}
            <span className={`swatch swatch-${level}`} style={{ borderTopColor: certaintyColor(level) }} /> {level} ({p.counts[level]})
          </label>
        ))}
      </fieldset>
      {p.overlays}
      {p.notice && (
        <p className="notice" data-testid="notice">
          {p.notice}
        </p>
      )}
      {p.error && (
        <p className="error" role="alert">
          {p.error}
        </p>
      )}
      {p.views}
      <LibraryList libs={p.libs} selected={p.selectedLib} onSelect={p.onPickLib} />
      <BlindSpots spots={p.spots} onFollow={p.onFollow} />
      {p.tree ? (
        <TreeView
          root={p.tree}
          collapsed={p.collapsed}
          selected={p.selected}
          onToggle={p.onToggleDir}
          onSelect={p.onSelect}
          onExpandAll={() => p.onSetCollapsed(new Set())}
          onCollapseAll={() => p.onSetCollapsed(new Set(topDirIds(p.tree as TreeDir)))}
        />
      ) : (
        <NodeList visible={p.visible} selected={p.selected} onSelect={p.onSelect} onOpen={p.onOpen} />
      )}
    </aside>
  );
}

/** Search by name; clicking a match follows it onto the canvas.
 *  Example: typing `checkout` and Enter lists `shop.core/checkout`. */
function SearchBox({ api, onError, onFollow }: { api: Api; onError: (e: unknown) => void; onFollow: (id: string) => void }) {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<GNode[]>([]);
  const run = (e: React.FormEvent) => {
    e.preventDefault();
    if (!query.trim()) return;
    api
      .search(query)
      .then((r) => setResults(r.matches))
      .catch(onError);
  };
  return (
    <>
      <form onSubmit={run} className="search">
        <input data-testid="search" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search functions…" aria-label="Search" />
      </form>
      {results.length > 0 && (
        <ul className="results" data-testid="results">
          {results.map((n) => (
            <li key={n.id}>
              <button type="button" data-node-id={n.id} onClick={() => onFollow(n.id)} title={n.id}>
                {n.name} <span className="muted">{n.namespace ?? n.kind}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

/** The blind-spot list, open by default when it is short.
 *  Example: one `dynamic` spot shows as `src/shop/core.clj:12`. */
function BlindSpots({ spots, onFollow }: { spots: BlindSpot[]; onFollow: (id: string) => void }) {
  return (
    <details className="blindspots" data-testid="blindspots" open={spots.length > 0 && spots.length <= 20}>
      <summary>Blind spots ({spots.length})</summary>
      <p className="muted small-print">What the analyzer cannot see through. Click one to show it.</p>
      <ul>
        {spots.map((b, i) => (
          <li key={`${b.category}-${b.node ?? ''}-${i}`}>
            <span className={`badge badge-${b.category}`}>{b.category}</span>{' '}
            {b.node ? (
              <button type="button" data-node-id={b.node} onClick={() => b.node && onFollow(b.node)} title={b.message}>
                {b.site ? `${b.site.file}:${b.site.line}` : b.node}
              </button>
            ) : (
              <span title={b.message}>{b.site ? `${b.site.file}:${b.site.line}` : '–'}</span>
            )}
          </li>
        ))}
      </ul>
    </details>
  );
}

/** The nodes on the canvas, with "open" for units and namespaces.
 *  Example: on the unit view, one row per unit. */
function NodeList({ visible, selected, onSelect, onOpen }: { visible: GNode[]; selected: string | null; onSelect: (id: string) => void; onOpen: (n: GNode) => void }) {
  return (
    <>
      <h2>On the canvas ({visible.length})</h2>
      <ul className="nodes" data-testid="node-list">
        {visible.map((n) => (
          <li key={n.id} className={n.id === selected ? 'selected' : ''}>
            <button type="button" className="name" data-node-id={n.id} onClick={() => onSelect(n.id)} title={`${n.kind}: ${n.id}`}>
              {labelFor(n)}
            </button>
            {isAggregate(n) && (
              <button type="button" className="small" data-open={n.id} onClick={() => onOpen(n)} title="Show only this">
                open
              </button>
            )}
          </li>
        ))}
      </ul>
    </>
  );
}
