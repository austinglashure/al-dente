// Help: the measures guide (measuresGuide.ts) in a dialog, with a contents
// list and a small diagram of the example call graph for the main measures.
// Thresholds and weights are fetched from the server, which serves the
// rules it actually uses.

import { useEffect, useState } from 'react';
import type { Api } from './api';
import { EXAMPLE, guide, type Diagram, type GuideEntry } from './measuresGuide';
import type { Defaults } from './types';

const GROUP_COLORS = ['#c6dbef', '#fdd0a2'];
const HIGHLIGHT = '#fb6a4a';

/** The example call graph with the diagram's nodes marked.
 *  Example: `<ExampleGraph d={{ highlight: ['log'], caption: '…' }} />`. */
function ExampleGraph({ d }: { d: Diagram }) {
  const pos = new Map(EXAMPLE.nodes.map((n) => [n.id, n]));
  const fill = (id: string) => {
    if (d.highlight?.includes(id)) return HIGHLIGHT;
    const g = d.groups?.findIndex((members) => members.includes(id)) ?? -1;
    return g >= 0 ? (GROUP_COLORS[g % GROUP_COLORS.length] ?? '#eee') : '#eee';
  };
  return (
    <figure className="guide-figure">
      <svg viewBox="0 0 370 185" role="img" aria-label={d.caption}>
        <defs>
          <marker id="guide-arrow" viewBox="0 0 10 10" refX="10" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
            <path d="M0,0 L10,5 L0,10 z" fill="#888" />
          </marker>
        </defs>
        {EXAMPLE.edges.map(([a, b]) => {
          const p = pos.get(a);
          const q = pos.get(b);
          if (!p || !q) return null;
          // Stop the line at the node's edge, and bend the two edges of a 2-cycle apart.
          const dx = q.x - p.x;
          const dy = q.y - p.y;
          const len = Math.hypot(dx, dy) || 1;
          const r = 21;
          const back = EXAMPLE.edges.some(([x, y]) => x === b && y === a);
          const off = back ? 5 : 0;
          const nx = (-dy / len) * off;
          const ny = (dx / len) * off;
          return (
            <line
              key={`${a}-${b}`}
              x1={p.x + (dx / len) * r + nx}
              y1={p.y + (dy / len) * r + ny}
              x2={q.x - (dx / len) * r + nx}
              y2={q.y - (dy / len) * r + ny}
              stroke="#888"
              strokeWidth="1.2"
              markerEnd="url(#guide-arrow)"
            />
          );
        })}
        {EXAMPLE.nodes.map((n) => (
          <g key={n.id}>
            <circle cx={n.x} cy={n.y} r="20" fill={fill(n.id)} stroke="#555" strokeWidth="1" />
            <text x={n.x} y={n.y + 3} textAnchor="middle" fontSize="8.5">
              {n.id}
            </text>
          </g>
        ))}
      </svg>
      <figcaption className="muted small-print">{d.caption}</figcaption>
    </figure>
  );
}

/** One entry: name, what it computes, what it means, where it misleads.
 *  Example: `<Entry e={entry('pagerank')} />`. */
function Entry({ e }: { e: GuideEntry }) {
  return (
    <section className="guide-entry" id={`guide-${e.id}`} data-guide-entry={e.id}>
      <h4>
        {e.title} <code className="muted">{e.id}</code>
      </h4>
      <div className={e.diagram ? 'guide-row' : undefined}>
        <div>
          <p>
            <strong>What it is.</strong> {e.what}
          </p>
          <p>
            <strong>What it tells you.</strong> {e.meaning}
          </p>
          {e.caveat && (
            <p className="muted">
              <strong>Watch out.</strong> {e.caveat}
            </p>
          )}
        </div>
        {e.diagram && <ExampleGraph d={e.diagram} />}
      </div>
    </section>
  );
}

/** The help dialog.
 *  Example: `<HelpDialog api={api} onClose={() => setHelpOpen(false)} />`. */
export function HelpDialog({ api, onClose }: { api: Api; onClose: () => void }) {
  const [defaults, setDefaults] = useState<Defaults | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    api
      .defaults()
      .then((d) => live && setDefaults(d))
      .catch((e: unknown) => live && setError(e instanceof Error ? e.message : String(e)));
    return () => {
      live = false;
    };
  }, [api]);
  const sections = defaults ? guide(defaults) : [];
  return (
    <div className="modal-backdrop" role="presentation" onClick={onClose}>
      <div className="modal guide" role="dialog" aria-modal="true" aria-label="Help: measures guide" data-testid="help-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="details-head">
          <h2>Measures guide</h2>
          <button type="button" className="close" onClick={onClose} aria-label="Close" data-testid="help-close">
            ×
          </button>
        </div>
        <p className="muted small-print">What each measure, finding and coverage status means when the graph is a codebase. The diagrams use one small example call graph; red marks the nodes the measure picks out.</p>
        {error && (
          <p className="error" role="alert">
            Could not load the rules in use: {error}
          </p>
        )}
        {!defaults && !error && <p className="muted">Loading…</p>}
        <nav className="guide-contents" aria-label="Contents">
          {sections.map((s) => (
            <a key={s.id} href={`#guide-section-${s.id}`}>
              {s.title.split(':')[0]}
            </a>
          ))}
        </nav>
        {sections.map((s) => (
          <section key={s.id} id={`guide-section-${s.id}`}>
            <h3>{s.title}</h3>
            {s.intro && <p>{s.intro}</p>}
            {s.entries.map((e) => (
              <Entry key={e.id} e={e} />
            ))}
          </section>
        ))}
      </div>
    </div>
  );
}
