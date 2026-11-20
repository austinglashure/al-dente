// The ranking list: which measures a level can be ranked by, what each one
// means, and the ordering itself.

export type RankLevel = 'namespace' | 'unit' | 'symbol';

/** One node with its measures, as `/rankings` gives it. */
export interface RankingRow {
  id: string;
  name: string;
  values: Record<string, number>;
}

/** What `/rankings` returns. */
export interface RankingReport {
  level: RankLevel;
  rows: RankingRow[];
}

/** A measure the list can rank by. */
export interface Measure {
  key: string;
  label: string;
  group: 'Size' | 'Coupling' | 'Centrality';
  /** One sentence for the line under the list. */
  about: string;
  /** Levels it exists at; all when absent. */
  levels?: RankLevel[];
}

export const MEASURES: Measure[] = [
  { key: 'lines', label: 'lines', group: 'Size', about: 'Lines up to the last definition in each file (comments after it are not counted).' },
  { key: 'symbols', label: 'symbols', group: 'Size', about: 'How many functions, vars, macros and other definitions it holds.', levels: ['namespace', 'unit'] },
  { key: 'callers', label: 'callers', group: 'Coupling', about: 'How many distinct others call into it.' },
  { key: 'callees', label: 'callees', group: 'Coupling', about: 'How many distinct others it calls.' },
  { key: 'calls_in', label: 'calls in', group: 'Coupling', about: 'Calls coming in, weighted by certainty (one caller can make many).' },
  { key: 'calls_out', label: 'calls out', group: 'Coupling', about: 'Calls going out, weighted by certainty.' },
  {
    key: 'instability',
    label: 'instability',
    group: 'Coupling',
    about: 'Callees over callers plus callees: 0 is a stable foundation others lean on, 1 leans on others and nothing leans on it.',
    levels: ['namespace', 'unit'],
  },
  { key: 'pagerank', label: 'PageRank', group: 'Centrality', about: 'Where calls end up: code that much important code relies on. Make it bulletproof.' },
  { key: 'pagerank_reverse', label: 'PageRank (reverse)', group: 'Centrality', about: 'Where calls start: orchestrators that drive much of the code.' },
  { key: 'hub', label: 'hub', group: 'Centrality', about: 'Calls many well-used things: coordinators.' },
  { key: 'authority', label: 'authority', group: 'Centrality', about: 'Called by many coordinators: the building blocks.' },
  { key: 'core', label: 'k-core', group: 'Centrality', about: 'How deep in the densely connected centre of the code it sits.' },
  { key: 'betweenness', label: 'betweenness', group: 'Centrality', about: 'How many paths between other code run through it: bottlenecks.' },
  { key: 'closeness', label: 'closeness', group: 'Centrality', about: 'How few steps it takes to reach everything else.' },
];

/** The measures offered at a level.
 *  Example: `measuresFor('symbol')` has no `instability`. */
export function measuresFor(level: RankLevel): Measure[] {
  return MEASURES.filter((m) => !m.levels || m.levels.includes(level));
}

/** Rows that have `key`, ordered by it (ties by name), optionally only those
 *  whose ID is in `scope`.
 *  Example: `rank(rows, 'callers', 'desc')` puts the most-called first. */
export function rank(rows: RankingRow[], key: string, order: 'asc' | 'desc', scope?: Set<string>): RankingRow[] {
  const sign = order === 'desc' ? -1 : 1;
  return rows
    .filter((r) => key in r.values && (!scope || scope.has(r.id)))
    .sort((a, b) => sign * ((a.values[key] ?? 0) - (b.values[key] ?? 0)) || a.name.localeCompare(b.name));
}
