// The bar above the canvas: the breadcrumb of what is open, the overview
// switch, the Top selector, "Paste trace", Help and the panel toggles.

import type { Panels } from './panels';
import { TOP_CHOICES } from './viewPrefs';

/** One level of the breadcrumb, as the bar sees it. */
export interface CrumbLabel {
  label: string;
}

interface Props {
  crumbs: CrumbLabel[];
  /** Goes back to crumb `i`. */
  onCrumb: (i: number) => void;
  view: 'units' | 'namespaces';
  onView: (next: 'units' | 'namespaces') => void;
  top: number;
  onTop: (n: number) => void;
  onPasteTrace: () => void;
  onHelp: () => void;
  panels: Panels;
  setPanels: (p: Partial<Panels>) => void;
}

/** The bar above the canvas.
 *  Example: `<CanvasBar crumbs={[{ label: 'Repository' }]} view="units" … />`. */
export function CanvasBar(p: Props) {
  return (
    <nav className="breadcrumb" data-testid="breadcrumb" aria-label="What is open">
      {p.crumbs.map((c, i) => (
        <span key={`${i}-${c.label}`}>
          {i > 0 && <span className="muted"> › </span>}
          {i < p.crumbs.length - 1 ? (
            <button type="button" onClick={() => p.onCrumb(i)}>
              {c.label}
            </button>
          ) : (
            <strong>{c.label}</strong>
          )}
        </span>
      ))}
      <span className="view-switch" role="group" aria-label="Overview">
        <button type="button" data-testid="view-units" aria-pressed={p.view === 'units'} onClick={() => p.onView('units')} title="Units and how they depend on each other">
          Units
        </button>
        <button
          type="button"
          data-testid="view-namespaces"
          aria-pressed={p.view === 'namespaces'}
          onClick={() => p.onView('namespaces')}
          title="Every namespace in the repository on one map (no externals)"
        >
          All namespaces
        </button>
      </span>
      {p.view === 'namespaces' && (
        <label className="top-select" title="How many namespaces to show, busiest first">
          Top{' '}
          <select data-testid="top" value={p.top} onChange={(e) => p.onTop(Number(e.target.value))}>
            {TOP_CHOICES.map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </select>
        </label>
      )}
      <button type="button" className="paste-trace" data-testid="paste-trace" onClick={p.onPasteTrace} title="Paste a stack trace and pin a crater to it">
        Paste trace
      </button>
      <button type="button" className="help-button" data-testid="help" onClick={p.onHelp} title="What the measures, findings and coverage statuses mean">
        Help
      </button>
      <span className="panel-toggles">
        <button
          type="button"
          data-testid="toggle-left"
          aria-pressed={p.panels.leftOpen}
          onClick={() => p.setPanels({ leftOpen: !p.panels.leftOpen })}
          title={p.panels.leftOpen ? 'Hide the list' : 'Show the list'}
        >
          {p.panels.leftOpen ? '◀ list' : '▶ list'}
        </button>
        <button
          type="button"
          data-testid="toggle-right"
          aria-pressed={p.panels.rightOpen}
          onClick={() => p.setPanels({ rightOpen: !p.panels.rightOpen })}
          title={p.panels.rightOpen ? 'Hide details' : 'Show details'}
        >
          {p.panels.rightOpen ? 'details ▶' : 'details ◀'}
        </button>
      </span>
    </nav>
  );
}
