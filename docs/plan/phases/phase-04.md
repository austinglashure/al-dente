# Phase 4. Measures and coverage

Goal (M4): the tool says what needs refactoring and what must be bulletproof, and shows untested code at a glance.

Read first: `09-network-and-coverage.md`.

## WP4.1 Measure engine, first set

**Build** (`aldente-analysis::measures`)
- Projections (invocation, structure, undirected) at three levels with certainty weights and test and external exclusion.
- Fan-in and fan-out, weighted PageRank (forward and reversed), HITS, SCC with cut suggestions, k-core.
- Lazy computation with results cached in `analysis_result`.

**Accept**
- Each measure is checked against hand-computed values on at least two small graphs, and against a reference implementation's published values for one classic graph (Zachary's karate club; values stored in the test).
- Lowering a set of edges from `certain` to `possible` lowers their targets' PageRank (the handoff's requirement that speculative edges not inflate scores).
- All of the above on a 200,000-node synthetic graph in under ten seconds.

## WP4.2 Measure engine, second set

**Build**
- Betweenness (exact and sampled, labeled approximate), harmonic closeness, eigenvector with convergence reporting, communities (Louvain first; Leiden if time allows), dominators from entry points, articulation points and bridges, instability and abstractness, layering violations, dead code, god-object score.

**Accept**
- Reference checks as in WP4.1.
- Sampled betweenness ranks the true top ten of a test graph within its top twenty.
- Community detection is deterministic for a fixed seed.
- Dead-code result on the fixture equals the hand list in `EXPECTED.md`.

## WP4.3 Churn and co-change

**Build**
- Hardened `git log` reader (numstat with renames), fetching history for shallow remote clones only when this analysis is requested.
- Per-file and per-symbol change counts (symbol attribution by current source ranges for recent commits; file-level beyond a commit limit), co-change pairs with support and confidence, hidden-coupling detection.
- `aldente churn`.

**Accept**
- Tests on a scripted temp repository with known history.
- A repository with no git history returns an empty result with a warning, not an error.

## WP4.4 Static coverage

**Build**
- The four statuses with covering tests listed; namespace and unit roll-ups; `aldente coverage`; `--import` accepting LCOV and Cloverage's formats mapped to symbols by range (running tools is Phase 11).

**Accept**
- Fixture expectations for each status.
- An imported LCOV file for the fixture overrides estimates and the disagreement list is produced.
- Crater checklists now show coverage status.

## WP4.5 Hotspots and risk

**Build**
- Risk score; the findings table from the document with percentile thresholds in `rules/hotspots.toml`; `aldente hotspots`, `cycles`, `communities`, `deadcode`, `metrics`.

**Accept**
- Golden outputs on the fixture and on `mono-mini`.
- Each finding includes its evidence values and a one-line reason a person can act on.
- Thresholds changed in the rules file change the output without recompiling.

## WP4.6 GUI: measures and overlays

**Build**
- Measures table view (virtualized), row selection highlighting nodes.
- Bind node size and color to any measure; coverage overlay; risk overlay as the default "what should worry me" picture; churn overlay.
- Hotspots view; cycles listed with "show this cycle" isolating the component on the canvas.

**Accept**
- Playwright: switch overlay to coverage, see untested nodes styled differently; open a hotspot and land on the node.
- Overlays respect the certainty filter and do not trigger a re-layout.

## WP4.7 Milestone M4 acceptance

**Accept**
- On an acceptance repository, produce the hotspots report. Review the top twenty with Austin: at least fifteen should be judged "yes, that is a real concern or a real core function". Record misses and the rule changes they suggest.
- Namespace-level cycles reported match those found by an independent check (clj-kondo's own namespace-graph tool or a hand check on a small repository).
