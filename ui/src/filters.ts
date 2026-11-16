// The filter panel's rules: which canvas nodes are shown. Pure, so the
// workspace can store them and tests can check them without a browser.

import type { GNode } from './types';

/** One "only nodes with <metric> at or above <min>" rule. */
export interface MeasureMin {
  metric: string;
  min: number;
}

/** What the filter panel keeps. Empty lists keep everything. */
export interface Filters {
  /** Unit IDs; a node is kept if it is in one of them (a unit node is in itself). */
  units: string[];
  /** Node kinds or symbol subkinds, e.g. `namespace`, `fn`, `test`. */
  kinds: string[];
  /** Flags a node must all have, e.g. `entry-point`. */
  flags: string[];
  /** Case-insensitive substring of the name or ID. */
  name: string;
  measures: MeasureMin[];
}

export const NO_FILTERS: Filters = { units: [], kinds: [], flags: [], name: '', measures: [] };

/** Whether any rule is set.
 *  Example: `active(NO_FILTERS)` is `false`. */
export function active(f: Filters): boolean {
  return f.units.length > 0 || f.kinds.length > 0 || f.flags.length > 0 || f.name !== '' || f.measures.length > 0;
}

/** Whether a node passes every rule; `measure` gives a node's value for a
 *  metric (a node without one fails a measure rule).
 *  Example: `passes(fn, {...NO_FILTERS, name: 'total'})` is true for `orders.core/total`. */
export function passes(n: GNode, f: Filters, measure: (metric: string, id: string) => number | undefined = () => undefined): boolean {
  if (f.units.length > 0) {
    const unit = n.kind === 'unit' ? n.id : n.unit;
    if (unit === undefined || !f.units.includes(unit)) return false;
  }
  if (f.kinds.length > 0 && !f.kinds.includes(n.kind) && !(n.subkind !== undefined && f.kinds.includes(n.subkind))) return false;
  if (f.flags.length > 0 && !f.flags.every((flag) => (n.flags ?? []).includes(flag))) return false;
  if (f.name !== '') {
    const needle = f.name.toLowerCase();
    if (!n.name.toLowerCase().includes(needle) && !n.id.toLowerCase().includes(needle)) return false;
  }
  return f.measures.every((m) => (measure(m.metric, n.id) ?? -Infinity) >= m.min);
}
