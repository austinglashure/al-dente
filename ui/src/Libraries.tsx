// The libraries list (left panel) and one library's call sites (right
// panel). Repository strings are rendered as React text.

import type { LibrarySummary, LibraryUsage, UsageRow } from './types';

/** The list of libraries in use; clicking one selects it.
 *  Example: `<LibraryList libs={report.libraries} selected={id} onSelect={pick} />`. */
export function LibraryList({ libs, selected, onSelect }: { libs: LibrarySummary[]; selected: string | null; onSelect: (id: string) => void }) {
  return (
    <details className="libraries" data-testid="libraries">
      <summary>Libraries ({libs.length})</summary>
      <ul>
        {libs.map((l) => (
          <li key={l.id} className={l.id === selected ? 'selected' : ''}>
            <button type="button" data-lib-id={l.id} onClick={() => onSelect(l.id)} title={l.id}>
              {l.coordinate}
            </button>{' '}
            <span className="muted">
              {l.ecosystem} · {l.symbols} sym · {l.namespaces} ns
            </span>
          </li>
        ))}
      </ul>
    </details>
  );
}

/** `file:line:col`, or a dash.
 *  Example: `site(row)` is `'src/a.cljs:7:18'`. */
function site(r: UsageRow): string {
  return r.site ? `${r.site.file}:${r.site.line}:${r.site.col}` : '–';
}

/** One library's attribution, call sites grouped by namespace, and the
 *  namespaces that require it without calling it.
 *  Example: `<LibraryPanel u={usage} highlighted={3} onClose={close} onFollow={follow} />`. */
export function LibraryPanel({ u, highlighted, onClose, onFollow }: { u: LibraryUsage; highlighted: number; onClose: () => void; onFollow: (id: string) => void }) {
  const rows = (g: { id: string; rows: UsageRow[] }) => (
    <li key={g.id}>
      <button type="button" onClick={() => onFollow(g.id)} title={g.id}>
        {g.id}
      </button>
      <ul>
        {g.rows.map((r, i) => (
          <li key={i} className="mono" data-testid="lib-site">
            {site(r)} {r.kind} {r.certainty ?? ''} → {r.to}
          </li>
        ))}
      </ul>
    </li>
  );
  return (
    <section data-testid="library-panel">
      <div className="details-head">
        <h2 title={u.library}>{u.library}</h2>
        <button type="button" className="close" onClick={onClose} aria-label="Close" title="Close">
          ×
        </button>
      </div>
      <p className="muted">
        {highlighted} using node{highlighted === 1 ? '' : 's'} highlighted on the canvas
      </p>
      <h3>Attributed by</h3>
      <ul>
        {u.attribution.map((a) => (
          <li key={a.provides} className="mono">
            {a.provides} ({a.certainty ?? '–'}, {a.reason ?? '–'})
          </li>
        ))}
      </ul>
      <h3>Used in ({u.groups.length})</h3>
      <ul>{u.groups.map(rows)}</ul>
      {u.require_only.length > 0 && (
        <>
          <h3>Required, never called ({u.require_only.length})</h3>
          <ul data-testid="require-only">{u.require_only.map(rows)}</ul>
        </>
      )}
    </section>
  );
}
