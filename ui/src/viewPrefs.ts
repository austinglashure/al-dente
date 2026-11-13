// Which overview the GUI opens on, remembered per browser like the panels.

/** The overview and how many namespaces the global map shows. */
export interface ViewPrefs {
  view: 'units' | 'namespaces';
  top: number;
}

/** The choices the Top selector offers. */
export const TOP_CHOICES = [100, 250, 500, 2000] as const;

const DEFAULTS: ViewPrefs = { view: 'units', top: 250 };
const KEY = 'aldente-view';

/** Reads saved preferences, falling back to the defaults for anything
 *  missing or invalid.
 *  Example: `parseViewPrefs('{"view":"namespaces"}')` is `{view: 'namespaces', top: 250}`. */
export function parseViewPrefs(raw: string | null): ViewPrefs {
  try {
    const p = JSON.parse(raw ?? '{}') as Partial<ViewPrefs>;
    return {
      view: p.view === 'namespaces' || p.view === 'units' ? p.view : DEFAULTS.view,
      top: (TOP_CHOICES as readonly number[]).includes(p.top as number) ? (p.top as number) : DEFAULTS.top,
    };
  } catch {
    return DEFAULTS;
  }
}

/** The saved preferences, or the defaults when storage is unavailable.
 *  Example: on a first visit gives `{view: 'units', top: 250}`. */
export function loadViewPrefs(): ViewPrefs {
  try {
    return parseViewPrefs(window.localStorage.getItem(KEY));
  } catch {
    return DEFAULTS;
  }
}

/** Saves the preferences when storage is available.
 *  Example: `saveViewPrefs({view: 'namespaces', top: 500})`. */
export function saveViewPrefs(prefs: ViewPrefs): void {
  try {
    window.localStorage.setItem(KEY, JSON.stringify(prefs));
  } catch {
    // Not saved; the GUI still works for this visit.
  }
}
