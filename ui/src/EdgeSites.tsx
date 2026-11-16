// An edge's call sites: clicking an edge on the canvas lists them, and a
// site shows that line in context. Repository strings are React text.

import type { GEdge, Site } from './types';

/** The selected edge and its sites.
 *  Example: `<EdgeSites edge={e} onSite={show} onClose={close} />`. */
export function EdgeSites({ edge, onSite, onClose }: { edge: GEdge; onSite: (s: Site) => void; onClose: () => void }) {
  const sites = edge.sites ?? [];
  return (
    <section data-testid="edge-sites">
      <div className="details-head">
        <h2>Edge</h2>
        <button type="button" className="close" onClick={onClose} aria-label="Close">
          ×
        </button>
      </div>
      <p className="mono">
        {edge.from} → {edge.to}
      </p>
      <p className="muted">
        {edge.kind}
        {edge.certainty ? ` · ${edge.certainty}` : ''}
        {edge.reason ? ` · ${edge.reason}` : ''}
        {edge.count ? ` · stands for ${edge.count} calls (expand both ends to see their sites)` : ''}
      </p>
      {sites.length > 0 ? (
        <ul className="neighbors">
          {sites.map((s) => (
            <li key={`${s.file}:${s.line}:${s.col}`}>
              <button type="button" className="mono" data-testid="edge-site" onClick={() => onSite(s)}>
                {s.file}:{s.line}:{s.col}
              </button>
            </li>
          ))}
        </ul>
      ) : (
        <p className="muted">No call sites recorded on this edge.</p>
      )}
    </section>
  );
}
