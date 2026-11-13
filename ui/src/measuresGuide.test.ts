import { describe, expect, it } from 'vitest';
import { EXAMPLE, GUIDE, entry } from './measuresGuide';
import { METRICS } from './overlay';

// The finding kinds of `aldente hotspots` (crates/aldente-model/src/hotspots.rs).
const FINDING_KINDS = ['god-object', 'must-be-bulletproof', 'bottleneck', 'cycle', 'hidden-coupling', 'risky-untested', 'misplaced-code', 'dead-code'];
const COVERAGE = ['direct', 'indirect', 'weak', 'untested'];

const callees = (id: string) => EXAMPLE.edges.filter(([a]) => a === id).map(([, b]) => b);
const callers = (id: string) => EXAMPLE.edges.filter(([, b]) => b === id).map(([a]) => a);

/** Nodes reachable from `start` over directed edges, never entering `without`. */
function reach(start: string, without: string | null): Set<string> {
  const seen = new Set([start]);
  const queue = [start];
  for (let v = queue.shift(); v !== undefined; v = queue.shift()) {
    for (const w of callees(v)) {
      if (w !== without && !seen.has(w)) {
        seen.add(w);
        queue.push(w);
      }
    }
  }
  return seen;
}

/** Whether the example stays connected (ignoring direction) once `gone` is removed. */
function connectedWithout(gone: string): boolean {
  const rest = EXAMPLE.nodes.map((n) => n.id).filter((id) => id !== gone);
  const start = rest[0] ?? '';
  const seen = new Set([start]);
  const queue = [start];
  for (let v = queue.shift(); v !== undefined; v = queue.shift()) {
    for (const w of [...callees(v), ...callers(v)]) {
      if (w !== gone && !seen.has(w)) {
        seen.add(w);
        queue.push(w);
      }
    }
  }
  return seen.size === rest.length;
}

describe('measures guide', () => {
  it('explains every measure the GUI can bind, every finding kind and every coverage status', () => {
    for (const m of METRICS) expect(entry(m), m).toBeDefined();
    for (const k of FINDING_KINDS) expect(entry(k), k).toBeDefined();
    for (const c of COVERAGE) expect(entry(c), c).toBeDefined();
  });

  it('has unique entry IDs and only highlights nodes of the example', () => {
    const ids = GUIDE.flatMap((s) => s.entries.map((e) => e.id));
    expect(new Set(ids).size).toBe(ids.length);
    const nodes = new Set(EXAMPLE.nodes.map((n) => n.id));
    for (const [a, b] of EXAMPLE.edges) expect(nodes.has(a) && nodes.has(b), `${a} -> ${b}`).toBe(true);
    for (const e of GUIDE.flatMap((s) => s.entries)) {
      for (const id of [...(e.diagram?.highlight ?? []), ...(e.diagram?.groups ?? []).flat()]) expect(nodes.has(id), `${e.id}: ${id}`).toBe(true);
    }
  });

  // The diagrams make claims about the example graph; check them.
  it('fan-in and fan-out diagrams highlight the unique maximum', () => {
    const max = (f: (id: string) => string[]) => {
      const counts = EXAMPLE.nodes.map((n) => [n.id, f(n.id).length] as const).sort((a, b) => b[1] - a[1]);
      const [first, second] = counts;
      expect(first?.[1]).toBeGreaterThan(second?.[1] ?? Infinity);
      return first?.[0];
    };
    expect(entry('fan_in')?.diagram?.highlight).toEqual([max(callers)]);
    expect(entry('fan_out')?.diagram?.highlight).toEqual([max(callees)]);
  });

  it('the dominator diagram shows exactly what becomes unreachable without the node', () => {
    const d = entry('dominates')?.diagram;
    const [dominator = '', ...dominated] = d?.highlight ?? [];
    const all = reach(EXAMPLE.entry, null);
    const without = reach(EXAMPLE.entry, dominator);
    const lost = [...all].filter((id) => id !== dominator && !without.has(id)).sort();
    expect(dominated.sort()).toEqual(lost);
  });

  it('the bottleneck node is an articulation point and the others in its diagram are not', () => {
    const [cut = ''] = entry('betweenness')?.diagram?.highlight ?? [];
    expect(connectedWithout(cut)).toBe(false);
    expect(connectedWithout('log')).toBe(true);
  });

  it('the cycle diagram highlights nodes that reach each other', () => {
    const members = entry('cycle')?.diagram?.highlight ?? [];
    expect(members.length).toBeGreaterThan(1);
    for (const a of members) for (const b of members) if (a !== b) expect(reach(a, null).has(b), `${a} reaches ${b}`).toBe(true);
  });

  it('the PageRank diagram highlights only sinks, and reverse PageRank only nodes nothing calls but the entry', () => {
    for (const id of entry('pagerank')?.diagram?.highlight ?? []) expect(callees(id), id).toEqual([]);
    for (const id of entry('pagerank_reverse')?.diagram?.highlight ?? []) expect(callers(id).every((c) => c === EXAMPLE.entry || c === 'main'), id).toBe(true);
  });
});
