# Phase 9. Scale and incremental analysis

Goal (M9): the performance budgets in `02-architecture.md` are met on a few-million-line corpus, and re-analysis after a small change is fast.

## WP9.1 Benchmarks and profiling

**Build**
- A benchmark suite (`criterion` for micro, a timed end-to-end runner for macro) over: the largest acceptance repositories; the synthetic source corpus scaled to about 3 million lines across 20 units.
- A report command printing time and peak memory per pipeline stage; results appended to `docs/perf/history.md`.

**Accept**
- Baseline numbers recorded for every row of the budget table, with the gap to target.

## WP9.2 Pipeline tuning

**Build**, guided by the profile. Likely candidates:
- clj-kondo: units in parallel with a bounded pool; `:locals` only for files containing functions that call a parameter (two-pass); streaming JSON parse.
- Form index: reuse parsers per thread; avoid re-reading files (single read shared with hashing).
- Graph build: interned strings, integer IDs early, parallel per-unit build then a merge.
- Store: bulk inserts with prepared statements in one transaction, indexes created after load, `WITHOUT ROWID` where it helps, memory-mapped reads.
- Load: read only traversal columns into memory; lazy node details.

**Accept**
- Cold analysis and snapshot load meet budget on the 3-million-line corpus, or the remaining gap and its cause are written up for Austin.
- No golden graph changes.

## WP9.3 Incremental analysis

**Build**
- Per-file fact cache keyed by content hash, adapter version and the file's resolution context (its unit's source roots and the set of namespace names in the unit).
- On re-analysis: re-run the front ends only for changed files and files whose resolution could change (those requiring a namespace that was added, removed or renamed; `.cljc` and macro-defining namespaces invalidate their users).
- Rebuild cross-file rules (dispatch, param-flow, keyword dispatch, attribution) from cached facts; they are cheap relative to parsing.
- `--no-incremental` to force a full run.

**Accept**
- **Equivalence test:** for a scripted series of edits on the fixtures and on one acceptance repository (edit a body, add a function, rename a namespace, change a macro, delete a file, add a `defmethod` elsewhere), the incremental snapshot equals a full re-analysis exactly.
- Editing one file in the 3-million-line corpus re-analyzes in under 15 seconds.

## WP9.4 Server and GUI under load

**Build**
- API profiling on the large snapshot; caching of aggregate graphs and measure results; cancellation of superseded requests; background computation with progress for first-time measures.

**Accept**
- API 95th percentile under budget across a scripted session; the GUI walkthrough from M5 runs on the large corpus without a stall over one second.

## WP9.5 Optional: watch mode

**Build**
- `aldente serve --watch`: file watcher (`notify`) triggering incremental re-analysis; the GUI is told a new snapshot exists and offers to switch, keeping the workspace.

**Accept**
- Editing a file in a served local repository updates the graph within the incremental budget.

## WP9.6 Milestone M9 acceptance

**Accept**
- The budget table in `02-architecture.md` is replaced with measured numbers.
- Austin's largest repository: cold, warm and one-file-changed timings recorded.
