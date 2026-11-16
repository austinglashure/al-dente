// JSON shapes from the API. The Rust types in aldente-model are the source
// of truth; `aldente schema <name>` prints them.

export interface Loc {
  file: string;
  line: number;
  col: number;
  end_line: number;
  end_col: number;
}

export interface Site {
  file: string;
  line: number;
  col: number;
}

export interface GNode {
  id: string;
  kind: string;
  subkind?: string;
  name: string;
  namespace?: string;
  unit?: string;
  langs?: string[];
  flags?: string[];
  loc?: Loc;
  doc?: string;
  signature?: string;
  /** Aggregates in graph payloads: how many children they contain. */
  members?: number;
}

export interface GEdge {
  from: string;
  to: string;
  kind: string;
  certainty?: string;
  reason?: string;
  lang?: string;
  sites?: Site[];
  count?: number;
}

export interface Payload {
  nodes: GNode[];
  edges: GEdge[];
  truncated: boolean;
  aggregated: { level: string } | null;
}

export interface Neighbor {
  id: string;
  name: string;
  kind: string;
  certainty: string | null;
  reason: string | null;
  sites: Site[];
  count: number | null;
}

export interface Child {
  id: string;
  name: string;
  kind: string;
  subkind: string | null;
}

export interface ShowResult {
  node: GNode;
  callers: Neighbor[];
  callees: Neighbor[];
  children: Child[];
}

export interface FindResult {
  matches: GNode[];
  truncated: boolean;
}

export interface SourceRoot {
  path: string;
  kind: string;
  manifest: string | null;
  alias: string | null;
}

export interface UnitInfo {
  id: string;
  dir: string;
  manifests: string[];
  langs: string[];
  roots: SourceRoot[];
  service: boolean;
  files: number;
}

export interface SnapshotOverview {
  summary: { units: number; files: number; nodes?: Record<string, number> };
  units: UnitInfo[];
}

export interface Envelope<T> {
  schema: string;
  ok: boolean;
  data: T | null;
  warnings: { code: string; message: string }[];
  error: { code: string; message: string; candidates?: string[] | null } | null;
}

export interface BlindSpot {
  category: string;
  node: string | null;
  site: Site | null;
  message: string;
}

export interface BlindspotReport {
  blindspots: BlindSpot[];
  diagnostics: { path: string | null; code: string; message: string }[];
}

/** A row of the directory tree: a directory, or a chain of directories that
 *  each hold only the next one, shown as one row. */
export interface TreeDir {
  id: string | null;
  path: string;
  label: string;
  dirs: TreeDir[];
  files: TreeFile[];
}

/** A source file in the tree. */
export interface TreeFile {
  path: string;
  name: string;
  namespaces: string[];
  status: 'analyzed' | 'parse-error' | 'not-analyzed';
}

/** How one crater node was reached. */
export interface Reach {
  id: string;
  distance: number;
  path_certainty: 'certain' | 'likely' | 'possible';
  via: string | null;
  direction: 'forward' | 'reverse' | 'both';
}

/** What a stack trace says about a crater. */
export interface PinnedTrace {
  path: string[];
  overlay: GEdge[];
  faint: string[];
  unmapped: { index: number; raw: string }[];
  missing: { from: string; to: string }[];
}

/** An impact crater (`aldente schema crater`). */
export interface CraterResult {
  seeds: string[];
  direction: 'forward' | 'reverse' | 'both';
  depth: number;
  min_certainty: string;
  graph: Payload;
  reach: Reach[];
  counts: { by_certainty: Record<string, number>; by_unit: Record<string, number> };
  boundary: string[];
  hubs: string[];
  stopped: string[];
  hidden_below_min: number;
  truncated: boolean;
  diff: unknown;
  pinned: PinnedTrace | null;
}

/** One place to verify after a change. */
export interface ChecklistRow {
  id: string;
  kind: string;
  name: string;
  loc: Loc | null;
  distance: number;
  path_certainty: string;
  coverage: string | null;
  entry_point: boolean;
}

/** `aldente schema checklist`. */
export interface Checklist {
  seeds: string[];
  direction: string;
  depth: number;
  min_certainty: string;
  rows: ChecklistRow[];
  hidden_below_min: number;
  truncated: boolean;
}

/** One canonical stack frame (`aldente schema trace`). */
export interface Frame {
  index: number;
  lang: string;
  namespace: string | null;
  name: string | null;
  anonymous: boolean;
  file: string | null;
  line: number | null;
  col: number | null;
  raw: string;
  noise: boolean;
  node: string | null;
  match: 'exact' | 'name' | 'line' | 'none';
}

/** One exception with its frames and causes. */
export interface Trace {
  exception: { type: string | null; message: string | null; data: string | null };
  frames: Frame[];
  omitted: number;
  causes: Trace[];
}

export interface TraceReport {
  format: string;
  traces: Trace[];
}

/** One library in use (`aldente schema libs`). */
export interface LibrarySummary {
  id: string;
  ecosystem: string;
  coordinate: string;
  attribution: { provides: string; certainty: string | null; reason: string | null }[];
  symbols: number;
  namespaces: number;
  units: number;
  versions: { unit: string; version: string | null }[];
}

export interface UsageRow {
  from: string;
  kind: string;
  to: string;
  certainty: string | null;
  reason: string | null;
  site: Site | null;
}

/** `aldente schema libs`. */
export interface LibrariesReport {
  libraries: LibrarySummary[];
}

/** `aldente schema lib-usage`. */
export interface LibraryUsage {
  library: string;
  by: string;
  attribution: LibrarySummary['attribution'];
  groups: { id: string; rows: UsageRow[] }[];
  require_only: { id: string; rows: UsageRow[] }[];
}

export interface NodeMeasures {
  id: string;
  fan_in: number;
  fan_out: number;
  fan_in_weighted: number;
  fan_out_weighted: number;
  pagerank: number;
  pagerank_reverse: number;
  hub: number;
  authority: number;
  core: number;
  component: number | null;
}

export interface NodeMeasures2 {
  id: string;
  betweenness: number;
  closeness: number;
  eigenvector: number;
  community: number;
  dominates: number;
  articulation: boolean;
  god_score: number;
}

export interface CutEdge {
  from: string;
  to: string;
  weight: number;
  count: number;
}

export interface Component {
  size: number;
  nodes: string[];
  cut: CutEdge[];
  cut_minimal: boolean;
}

export interface MetricsReport {
  first: { nodes: NodeMeasures[]; components: Component[] };
  second: { nodes: NodeMeasures2[]; sampling: { approximate: boolean; pivots: number } };
}

export interface CyclesReport {
  projection: { kind: string; level: string };
  components: Component[];
}

export type CoverageStatus = 'direct' | 'indirect' | 'weak' | 'untested';

export interface SymbolCoverage {
  id: string;
  status: CoverageStatus;
  distance: number | null;
  tests: string[];
  test_count: number;
  measured: number | null;
}

export interface CoverageReport {
  tests: number;
  symbols: SymbolCoverage[];
}

export interface Finding {
  kind: string;
  subject: string;
  nodes: string[];
  score: number;
  evidence: Record<string, number>;
  reason: string;
}

export interface RiskRow {
  id: string;
  risk: number;
  importance: number;
  exposure: number;
  status: CoverageStatus;
  measured: number | null;
  churn: number | null;
}

/** `GET /api/v1/defaults`: the hotspot rules in use (keys as in
 *  rules/hotspots.toml) and analyzer defaults, quoted by the Help guide. */
export interface Defaults {
  certainty_weights: { certain: number; likely: number; possible: number };
  coverage_depth: number;
  hotspots: {
    exclude: { segments: string[] };
    'god-object': { percentile: number };
    'must-be-bulletproof': { 'pagerank-percentile': number; 'dominator-share': number };
    bottleneck: { 'betweenness-percentile': number; articulation: boolean };
    cycle: { 'symbol-min-size': number };
    'hidden-coupling': { 'min-support': number; 'min-confidence': number };
    'risky-untested': { 'risk-percentile': number; statuses: string[]; 'skip-entry-points': boolean };
    'misplaced-code': { 'min-community': number; 'min-share': number };
    'dead-code': { enabled: boolean };
    risk: { pagerank: number; 'fan-in': number; dominator: number; untested: number; weak: number; indirect: number; direct: number };
  };
}

export interface HotspotsReport {
  findings: Finding[];
  /** Findings left out by the rules' `[exclude]` patterns. */
  excluded: number;
  risk: RiskRow[];
}

export interface ChurnReport {
  history: 'full' | 'shallow' | 'none';
  commits: number;
  files: { path: string; commits: number }[];
  symbols: { id: string; commits: number }[];
}

/** What `POST /open` did. */
export interface OpenResult {
  opened: boolean;
  /** `path:line:col`. */
  location: string;
}
