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
}

export interface ShowResult {
  node: GNode;
  callers: Neighbor[];
  callees: Neighbor[];
}

export interface FindResult {
  matches: GNode[];
  truncated: boolean;
}

export interface UnitInfo {
  id: string;
  dir: string;
  files: number;
}

export interface SnapshotOverview {
  summary: { units: number; files: number };
  units: UnitInfo[];
}

export interface Envelope<T> {
  schema: string;
  ok: boolean;
  data: T | null;
  error: { code: string; message: string; candidates?: string[] | null } | null;
}
