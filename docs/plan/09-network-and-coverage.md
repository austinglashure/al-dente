# Network analysis and coverage

Covers use cases 5 and 8.

## Graphs the measures run on

Three projections, each computed at `symbol`, `namespace` and `unit` level:

- **invocation**: `calls`, `references`, `dispatches`, `registers`, weighted by certainty (1.0, 0.6, 0.25 by default);
- **structure**: `requires` between namespaces, `depends` between units;
- **undirected** versions of the above for community and core measures.

Tests and externals are excluded by default and can be included. Excluding tests matters: otherwise test helpers look central.

## Measures

| Measure | What it tells you | Notes |
|---|---|---|
| Fan-in, fan-out (weighted and raw) | How much depends on this; how much this depends on | The base signal for everything else. |
| PageRank on the reversed invocation graph | Code that much important code ends up calling: must be bulletproof | Weighted; damping 0.85. |
| HITS hubs and authorities | Hubs orchestrate, authorities are used | High hub score with high fan-out is the god-function signature. |
| Betweenness (Brandes) | Bottlenecks that many paths run through | Exact below 20,000 nodes; above that, sample k pivots and report the estimate as approximate. |
| Harmonic closeness | How near a node is to everything else | Harmonic form handles disconnected graphs. |
| Eigenvector centrality | Connected to well-connected code | Run per strongly connected component; report non-convergence instead of garbage. |
| Strongly connected components | Circular dependencies | Report each component larger than one, its size, and a small set of edges whose removal breaks it (greedy feedback-arc heuristic, preferring low-certainty and low-count edges). |
| k-core decomposition | The dense kernel of the codebase | Undirected. |
| Communities (Leiden; Louvain is acceptable first) | The modules the code actually forms | Compare with namespace and unit boundaries; report namespaces split across communities and communities spanning units. |

Additional measures the handoff invited:

| Measure | What it tells you |
|---|---|
| Dominators from entry points | Single points of failure: code every path from an entry point to some region must pass through. |
| Articulation points and bridges | Nodes or edges whose removal disconnects part of the graph. |
| Instability and abstractness per namespace and unit (afferent and efferent coupling) | Which modules are stable foundations and which are volatile; flags stable modules that depend on unstable ones. |
| Layering violations | Derive layers from the condensation of the namespace graph; list edges that point upward. |
| Dead code | Symbols unreachable from any entry point or test at `possible` certainty. Because the graph over-approximates, what is still unreachable is a strong signal. |
| Churn and co-change (git history) | Files and symbols that change often, and pairs that change together without a static edge between them (hidden coupling). Static: reads history, runs nothing. |
| God-object score | A composite: high fan-out, high hub score, large body, membership in the top core, many distinct namespaces touched. |

## Findings, not just numbers

`aldente hotspots` turns measures into ranked findings with reasons. Each finding has a kind, a subject, a score, the evidence and a suggestion.

| Finding | Rule of thumb (thresholds are percentiles within the repository, configurable) |
|---|---|
| God object | God-object score in the top 1 percent. |
| Must be bulletproof | Reverse PageRank in the top 2 percent or dominator of more than N percent of entry-point reach. |
| Bottleneck | Betweenness in the top 1 percent and an articulation point. |
| Cycle | Any strongly connected component at namespace or unit level; symbol-level components above a size threshold. |
| Hidden coupling | Co-change above threshold with no static path within two hops. |
| Risky and untested | High risk score (below) with no test coverage. |
| Misplaced code | A symbol whose community is mostly in another namespace. |
| Dead code | As above. |

Percentile thresholds keep the report meaningful on both a small library and a large monorepo.

## Performance

Run measures lazily and cache results in the snapshot per (projection, level, parameters). At symbol level on a large repository run fan, PageRank, HITS, SCC, k-core and communities exactly; sample betweenness and closeness. Namespace and unit levels are always exact.

## Coverage

No existing coverage reports are assumed.

### Static estimate (default)

1. Identify tests (rule R14).
2. From each test symbol, search forward through invocation edges, excluding other tests.
3. For each non-test symbol record the minimum distance from any test and the path certainty, plus the list of covering tests (capped).

| Status | Definition |
|---|---|
| `direct` | Referenced by a test at distance 1. |
| `indirect` | Reachable from a test within the depth limit (default 4) at `likely` or better. |
| `weak` | Reachable only through `possible` edges or beyond the depth limit. |
| `untested` | Not reachable from any test. |

Namespace and unit coverage are the weighted share of their symbols in each status. This is an estimate of "some test exercises a path that could reach this", and the UI and CLI label it "estimated".

### Measured coverage (opt-in, Phase 11)

- `aldente coverage --import <file>` merges an existing report: Cloverage (EDN, LCOV, Codecov JSON), LCOV for ClojureScript and later Istanbul and coverage.py. Line data maps to symbols by source range; a symbol's measured coverage is covered lines over instrumented lines.
- `aldente coverage measure --allow-exec` runs the tests through the execution gate.
- When both exist, measured wins and the estimate is kept for comparison. Large disagreements (estimated direct, measured zero) are listed, since they point at analysis mistakes or dead tests.

### Risk score

```text
risk = importance × exposure
importance = normalized blend of reverse PageRank, fan-in and dominator reach
exposure   = 1.0 untested · 0.7 weak · 0.3 indirect · 0.1 direct   (or 1 − measured coverage)
```

Optionally multiplied by churn when history is available. The top of this list is the answer to "heavily used but poorly tested", and it is the default coverage overlay in the GUI.
