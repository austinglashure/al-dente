import { describe, expect, it } from 'vitest';
import { checklistCsv, checklistMarkdown, checklistRows, craterCanvas, libraryHighlight, unitOf, whyPath } from './crater';
import type { ChecklistRow, CraterResult, GNode, LibraryUsage } from './types';

const node = (id: string, kind = 'symbol'): GNode => ({ id, kind, name: id.split('/').pop() ?? id });

function result(over: Partial<CraterResult> = {}): CraterResult {
  return {
    seeds: ['a'],
    direction: 'forward',
    depth: 6,
    min_certainty: 'likely',
    graph: {
      nodes: [node('a'), node('b'), node('c'), node('ext:clj//x/y', 'external')],
      edges: [
        { from: 'a', to: 'b', kind: 'calls', certainty: 'certain', reason: 'direct' },
        { from: 'b', to: 'c', kind: 'calls', certainty: 'likely', reason: 'direct' },
        { from: 'b', to: 'ext:clj//x/y', kind: 'calls', certainty: 'certain', reason: 'direct' },
      ],
      truncated: false,
      aggregated: null,
    },
    reach: [
      { id: 'a', distance: 0, path_certainty: 'certain', via: null, direction: 'forward' },
      { id: 'b', distance: 1, path_certainty: 'certain', via: 'a', direction: 'forward' },
      { id: 'c', distance: 2, path_certainty: 'likely', via: 'b', direction: 'forward' },
      { id: 'ext:clj//x/y', distance: 2, path_certainty: 'certain', via: 'b', direction: 'forward' },
    ],
    counts: { by_certainty: {}, by_unit: {} },
    boundary: ['ext:clj//x/y'],
    hubs: [],
    stopped: [],
    hidden_below_min: 0,
    truncated: false,
    diff: null,
    pinned: null,
    ...over,
  };
}

describe('whyPath', () => {
  it('follows via back to the seed, with the edges between', () => {
    const r = result();
    const g = craterCanvas(r);
    const why = whyPath(r, g, 'c');
    expect(why.nodes).toEqual(['c', 'b', 'a']);
    expect(why.edges).toHaveLength(2);
  });

  it('works in reverse (edges point toward the seed)', () => {
    const r = result({
      direction: 'reverse',
      seeds: ['c'],
      reach: [
        { id: 'c', distance: 0, path_certainty: 'certain', via: null, direction: 'reverse' },
        { id: 'b', distance: 1, path_certainty: 'likely', via: 'c', direction: 'reverse' },
        { id: 'a', distance: 2, path_certainty: 'likely', via: 'b', direction: 'reverse' },
      ],
    });
    const why = whyPath(r, craterCanvas(r), 'a');
    expect(why.nodes).toEqual(['a', 'b', 'c']);
    expect(why.edges).toHaveLength(2);
  });

  it('stops on a cycle in via and on unknown nodes', () => {
    const r = result({
      reach: [
        { id: 'a', distance: 1, path_certainty: 'certain', via: 'b', direction: 'forward' },
        { id: 'b', distance: 1, path_certainty: 'certain', via: 'a', direction: 'forward' },
      ],
    });
    expect(whyPath(r, craterCanvas(r), 'a').nodes).toEqual(['a', 'b']);
    expect(whyPath(r, craterCanvas(r), 'zzz').nodes).toEqual([]);
  });
});

describe('craterCanvas', () => {
  it('marks boundary nodes, the pinned path, faint callers and overlay edges', () => {
    const r = result({
      pinned: {
        path: ['a', 'b'],
        overlay: [{ from: 'a', to: 'b', kind: 'calls', certainty: 'certain', reason: 'trace-observed' }],
        faint: ['c'],
        unmapped: [],
        missing: [],
      },
    });
    const g = craterCanvas(r);
    expect(g.getNodeAttribute('ext:clj//x/y', 'role')).toBe('boundary');
    expect(g.getNodeAttribute('ext:clj//x/y', 'label')).toContain('⇥');
    expect(g.getNodeAttribute('a', 'role')).toBe('pinned');
    expect(g.getNodeAttribute('c', 'role')).toBe('faint');
    const overlay = g.filterEdges((_k, a) => a.edge.reason === 'trace-observed');
    expect(overlay).toHaveLength(1);
  });
});

describe('checklist export', () => {
  const rows: ChecklistRow[] = [
    {
      id: 'clj:.//a/f',
      kind: 'symbol',
      name: 'a/f',
      loc: { file: 'src/a.clj', line: 3, col: 1, end_line: 5, end_col: 2 },
      distance: 0,
      path_certainty: 'certain',
      coverage: null,
      entry_point: true,
    },
    {
      id: 'clj:.//a/=evil',
      kind: 'symbol',
      name: '=evil|"x"\nnext',
      loc: null,
      distance: 1,
      path_certainty: 'likely',
      coverage: null,
      entry_point: false,
    },
  ];

  it('writes CSV with quoting and no formula cells', () => {
    const csv = checklistCsv(rows);
    const lines = csv.trim().split('\r\n');
    expect(lines[0]).toBe('location,symbol,distance,certainty,coverage,entry_point,id');
    expect(lines[1]).toBe('src/a.clj:3:1,a/f,0,certain,,yes,clj:.//a/f');
    expect(csv).toContain(`"'=evil|""x""\nnext"`);
    expect(csv).not.toMatch(/,=/);
  });

  it('writes a Markdown table with pipes and newlines escaped', () => {
    const md = checklistMarkdown(rows);
    expect(md.split('\n')[0]).toBe('| Location | Symbol | Distance | Certainty | Coverage | Entry point |');
    expect(md).toContain('=evil\\|"x" next');
    expect(md).toContain('| `src/a.clj:3:1` | a/f | 0 | certain |  | yes |');
  });
});

describe('library highlight', () => {
  const usage: LibraryUsage = {
    library: 'lib:npm:react',
    by: 'namespace',
    attribution: [],
    groups: [
      {
        id: 'clj:svc/web//web.views',
        rows: [{ from: 'clj:svc/web//web.views/page', kind: 'calls', to: 'ext:clj//react/createElement', certainty: 'certain', reason: 'direct', site: null }],
      },
    ],
    require_only: [{ id: 'clj:svc/web//web.state', rows: [{ from: 'clj:svc/web//web.state', kind: 'requires', to: 'ext:clj//react', certainty: null, reason: null, site: null }] }],
  };

  it('finds the unit of a namespace or symbol ID', () => {
    expect(unitOf('clj:svc/web//web.views/page')).toBe('unit:svc/web');
    expect(unitOf('clj:.//shop.core')).toBe('unit:.');
    expect(unitOf('ext:clj//react')).toBeNull();
  });

  it('highlights whatever is on the canvas at its level', () => {
    expect([...libraryHighlight(usage, ['unit:svc/web', 'unit:other'])]).toEqual(['unit:svc/web']);
    expect([...libraryHighlight(usage, ['clj:svc/web//web.views', 'clj:svc/web//web.state', 'clj:x//y'])].sort()).toEqual([
      'clj:svc/web//web.state',
      'clj:svc/web//web.views',
    ]);
    expect([...libraryHighlight(usage, ['clj:svc/web//web.views/page'])]).toEqual(['clj:svc/web//web.views/page']);
  });
});

describe('checklistRows', () => {
  it('keeps symbols and impls, strongest first, then nearest', () => {
    const rows = checklistRows(result());
    expect(rows.map((r) => [r.id, r.distance, r.path_certainty])).toEqual([
      ['a', 0, 'certain'],
      ['b', 1, 'certain'],
      ['c', 2, 'likely'],
    ]);
  });
});
