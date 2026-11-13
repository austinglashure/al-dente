// The measures guide shown under Help: what each graph measure, finding
// and coverage status means when the graph is a codebase. Data only; the
// dialog renders it. Diagrams use one small example call graph, and
// measuresGuide.test.ts checks that each diagram's claim holds on it.

/** A node of the example graph, positioned in a 370 × 185 drawing. */
export interface ExampleNode {
  id: string;
  x: number;
  y: number;
}

/** The example call graph: `[caller, callee]` edges; `entry` is where the program starts. */
export const EXAMPLE: { nodes: ExampleNode[]; edges: [string, string][]; entry: string } = {
  entry: 'main',
  nodes: [
    { id: 'main', x: 28, y: 95 },
    { id: 'handler', x: 110, y: 95 },
    { id: 'log', x: 185, y: 24 },
    { id: 'router', x: 185, y: 125 },
    { id: 'auth', x: 265, y: 85 },
    { id: 'session', x: 342, y: 85 },
    { id: 'parse', x: 265, y: 160 },
    { id: 'db', x: 342, y: 160 },
  ],
  edges: [
    ['main', 'handler'],
    ['main', 'log'],
    ['handler', 'router'],
    ['handler', 'log'],
    ['router', 'auth'],
    ['router', 'parse'],
    ['router', 'log'],
    ['auth', 'session'],
    ['session', 'auth'],
    ['auth', 'db'],
    ['parse', 'db'],
  ],
};

/** What a diagram marks on the example: highlighted nodes, or coloured groups. */
export interface Diagram {
  highlight?: string[];
  groups?: string[][];
  caption: string;
}

/** One measure, finding or status. */
export interface GuideEntry {
  /** The name the API and CLI use (`pagerank`, `risky-untested`, `weak`). */
  id: string;
  title: string;
  /** What is computed, in one or two sentences. */
  what: string;
  /** What it tells you about the code. */
  meaning: string;
  /** Where it misleads. */
  caveat?: string;
  diagram?: Diagram;
}

/** A group of entries under one heading. */
export interface GuideSection {
  id: string;
  title: string;
  intro?: string;
  entries: GuideEntry[];
}

export const GUIDE: GuideSection[] = [
  {
    id: 'basics',
    title: 'Basic counts',
    intro:
      'Each function is a node; an edge means "A calls or references B". Edges count by certainty: certain 1.0, likely 0.6, possible 0.25, so a guessed call counts for less but still counts. Tests and libraries are left out unless you ask for them.',
    entries: [
      {
        id: 'fan_in',
        title: 'Fan-in',
        what: 'How many distinct functions call this one. The weighted version adds up the certainty weights of those calls instead.',
        meaning: 'How many places break if you change its signature or behaviour. High fan-in: keep the interface stable and change it with care.',
        diagram: { highlight: ['log'], caption: 'log has three callers: main, handler and router.' },
      },
      {
        id: 'fan_out',
        title: 'Fan-out',
        what: 'How many distinct functions this one calls.',
        meaning: 'How much it depends on, or how much it orchestrates. High fan-out: it breaks when other code changes, and it is hard to test on its own.',
        diagram: { highlight: ['router'], caption: 'router calls three functions: auth, parse and log.' },
      },
    ],
  },
  {
    id: 'importance',
    title: 'Importance: where the weight of the code collects',
    entries: [
      {
        id: 'pagerank',
        title: 'PageRank',
        what: 'Follow calls at random from anywhere; PageRank is the share of time you spend at each function. A function called by important functions scores higher than one called by many trivial ones.',
        meaning: 'The foundations: code that much important code ends up relying on. A bug here spreads widely. This is the main route to the "must be bulletproof" finding.',
        caveat: 'Small utilities (string helpers, logging) can rank high because everything reaches them. That is accurate, but not always a worry.',
        diagram: { highlight: ['log', 'db'], caption: 'Calls lead to log and db and stop there, so importance collects in them.' },
      },
      {
        id: 'pagerank_reverse',
        title: 'Reverse PageRank',
        what: 'PageRank with every edge turned around.',
        meaning: 'The orchestrators: functions where control starts, such as handlers, pipelines and main functions.',
        diagram: { highlight: ['main', 'handler'], caption: 'Control starts at main and handler and flows out from them.' },
      },
      {
        id: 'hub',
        title: 'Hub score (HITS)',
        what: 'A function is a good hub when it calls many good authorities. Computed together with the authority score.',
        meaning: 'Coordinators that tie the important pieces together: routers, middleware stacks, top-level workflows.',
      },
      {
        id: 'authority',
        title: 'Authority score (HITS)',
        what: 'A function is a good authority when many good hubs call it.',
        meaning: 'Core services that the coordinators lean on. Like PageRank, but it separates the "calls things" and "is called" roles more sharply.',
      },
    ],
  },
  {
    id: 'position',
    title: 'Position: where control passes through',
    entries: [
      {
        id: 'betweenness',
        title: 'Betweenness and articulation points',
        what: 'Betweenness: of all the shortest call paths between pairs of functions, the share that go through this one. An articulation point is a function whose removal splits the graph into separate pieces.',
        meaning: 'A chokepoint between parts of the system: a change here reaches across a boundary. High betweenness on an articulation point is the "bottleneck" finding, the single bridge between two regions.',
        caveat: 'On large repositories betweenness is estimated from a sample of starting points; the table says when.',
        diagram: { highlight: ['router'], caption: 'Every path from the left side to auth, session, parse and db goes through router; remove it and the graph falls in two.' },
      },
      {
        id: 'closeness',
        title: 'Closeness',
        what: 'For every function that can reach this one, add 1 ÷ the number of hops it takes.',
        meaning: 'How exposed a function is to the rest of the code: high closeness means changes elsewhere reach it in few steps.',
      },
      {
        id: 'dominates',
        title: 'Dominators',
        what: 'A dominates B when every path from an entry point (a main function, an exported function, a test) to B passes through A. The number is how many functions this one dominates.',
        meaning: 'If it fails, everything it dominates becomes unreachable. Dominating a large share of the code is the other route to "must be bulletproof".',
        caveat: 'Entry points trivially dominate code only they reach, so findings leave them out.',
        diagram: { highlight: ['router', 'auth', 'session', 'parse', 'db'], caption: 'From main, auth, session, parse and db can only be reached through router: router dominates four functions.' },
      },
    ],
  },
  {
    id: 'structure',
    title: 'Clusters and structure',
    entries: [
      {
        id: 'core',
        title: 'k-core number',
        what: 'Ignore direction. Repeatedly remove functions with fewer than k connections until none are left; a function’s core number is the largest k at which it survives.',
        meaning: 'High: it sits in the densely interconnected heart of the code, the part that is hardest to change one piece at a time.',
        diagram: { highlight: ['main', 'handler', 'log', 'router', 'auth', 'parse', 'db'], caption: 'session has only one neighbour, so it is peeled off first; everything else survives at k = 2.' },
      },
      {
        id: 'communities',
        title: 'Communities',
        what: 'Groups of functions that call each other much more than they call anything outside the group (found with the Louvain method).',
        meaning: 'The modules the code actually has, whatever the namespaces say. Compare them with your namespaces: a function whose group lives in another namespace is "misplaced code"; a namespace spread over several groups is probably doing several jobs.',
        caveat: 'Louvain has a random element; the seed is fixed so results repeat, but small changes to the code can move borderline functions.',
        diagram: { groups: [['main', 'handler', 'log', 'router'], ['auth', 'session', 'parse', 'db']], caption: 'Five calls inside the left group and four inside the right, but only two between them.' },
      },
      {
        id: 'cycle',
        title: 'Cycles',
        what: 'Sets of functions, namespaces or units where each one can reach all the others. For each cycle, Al Dente suggests the cheapest edges to cut to break it.',
        meaning: 'You cannot understand, test or extract one member without the others. Namespace and unit cycles matter most: they block splitting code into libraries.',
        diagram: { highlight: ['auth', 'session'], caption: 'auth calls session and session calls auth back.' },
      },
      {
        id: 'eigenvector',
        title: 'Eigenvector centrality',
        what: 'Computed only inside cycles: a member scores high when it is connected to other high-scoring members.',
        meaning: 'The most central member of a tangle, usually the best place to start untangling it.',
      },
    ],
  },
  {
    id: 'combined',
    title: 'Combined scores',
    entries: [
      {
        id: 'god_score',
        title: 'God-object score',
        what: 'The average of five percentile ranks: fan-out, hub score, size, core number, and how many namespaces it calls into.',
        meaning: 'One function doing too much across too many areas. Splitting it by responsibility usually helps.',
      },
      {
        id: 'risk',
        title: 'Risk',
        what: 'Importance × exposure. Importance blends PageRank (50%), weighted fan-in (30%) and dominator reach (20%). Exposure comes from test coverage: untested 1.0, weak 0.7, indirect 0.3, direct 0.1. With history, frequently changed code counts up to twice as much.',
        meaning: 'Important code that tests do not protect. The risk overlay colours it; the highest scores become "risky untested" findings.',
        caveat: 'Coverage is estimated from the graph, not measured, unless you import a coverage report.',
      },
    ],
  },
  {
    id: 'findings',
    title: 'Hotspot findings',
    intro: 'Findings turn the measures into ranked, actionable items. Thresholds are in rules/hotspots.toml and can be changed with --rules. Example, benchmark and dev code is left out by default and counted instead.',
    entries: [
      { id: 'must-be-bulletproof', title: 'Must be bulletproof', what: 'Top PageRank, or dominating a large share of the code.', meaning: 'Test it directly and harden its error handling: failures here spread.' },
      { id: 'bottleneck', title: 'Bottleneck', what: 'Top betweenness on an articulation point.', meaning: 'Keep its interface stable; a change here ripples across a boundary.' },
      { id: 'god-object', title: 'God object', what: 'Top god-object score.', meaning: 'Split it by responsibility.' },
      { id: 'risky-untested', title: 'Risky untested', what: 'Top risk with an untested or weak coverage estimate. Entry points and dead code are left out.', meaning: 'Add a test that calls it.' },
      { id: 'cycle-finding', title: 'Cycle', what: 'Every namespace or unit cycle, and function cycles of four or more.', meaning: 'Break it at the suggested edge.' },
      { id: 'misplaced-code', title: 'Misplaced code', what: 'A function whose community lives mostly in another namespace.', meaning: 'Consider moving it there.' },
      { id: 'hidden-coupling', title: 'Hidden coupling', what: 'Files that keep changing in the same commits although no call or require joins them (needs --with-churn).', meaning: 'An implicit dependency: a shared format, a config key, copied logic. Make it explicit or remove it.' },
      { id: 'dead-code', title: 'Dead code', what: 'Nothing reaches it from an entry point, a test or load-time code, even counting uncertain calls.', meaning: 'Delete it, or find the caller the analyzer cannot see.' },
    ],
  },
  {
    id: 'coverage',
    title: 'Coverage estimates',
    intro: 'Estimated from the graph: starting from each test, and from code in test namespaces that runs when tests load, follow calls forward. Helper functions in test namespaces are passed through, not counted as tests.',
    entries: [
      { id: 'direct', title: 'Direct', what: 'A test calls it.', meaning: 'Some test exercises it on purpose.' },
      { id: 'indirect', title: 'Indirect', what: 'Reachable from a test within four hops, through likely or certain calls.', meaning: 'Probably exercised, as a side effect of testing something else.' },
      { id: 'weak', title: 'Weak', what: 'Reachable from a test only through possible calls or beyond four hops.', meaning: 'Might be exercised; do not count on it.' },
      { id: 'untested', title: 'Untested', what: 'No test reaches it.', meaning: 'Nothing in the test suite runs it, as far as the graph can tell.' },
    ],
  },
  {
    id: 'together',
    title: 'Reading them together',
    entries: [
      {
        id: 'together-lenses',
        title: 'Different lenses on the same question',
        what: 'What matters: PageRank and dominators. What connects things: betweenness and articulation points. What is tangled: cycles and k-core. What is in the wrong place: communities. What is unprotected: risk.',
        meaning: 'A function that shows up under several lenses is worth a look first.',
        caveat: 'Percentiles are within one repository: "top 1%" in a small library can still be modest. And every measure inherits the uncertainty of the edges; the certainty toggles change the graph the numbers describe.',
      },
    ],
  },
];

/** The entry with this ID, if any.
 *  Example: `entry('pagerank')?.title` is `'PageRank'`. */
export function entry(id: string): GuideEntry | undefined {
  return GUIDE.flatMap((s) => s.entries).find((e) => e.id === id);
}
