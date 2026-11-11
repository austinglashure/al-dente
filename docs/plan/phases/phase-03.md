# Phase 3. Crater, traces, libraries

Goal (M3): click a symbol or hand over a stack trace and see everywhere it can go and everything that reaches it; pick a library and see every use.

Read first: `08-crater-and-traces.md`.

## WP3.1 Crater engine

**Build** (`aldente-analysis::crater`)
- The three-pass search with `distance`, `path_certainty`, `via`; forward, reverse, both; dispatch handling in both directions; depth, stop conditions, hub damping; boundary and truncation reporting.

**Accept**
- Unit tests on small synthetic graphs: cycles terminate; a node reachable by a certain long path and a possible short path is labeled certain; reverse through an impl reaches callers of its multimethod; hub damping stops at the hub and lists it.
- Property test: the forward crater of A contains B if and only if the reverse crater of B contains A, for equal settings without depth limits.
- A crater on a 500,000-edge synthetic graph completes in under one second.

## WP3.2 Crater CLI and checklist

**Build**
- `aldente crater` with all flags, text grouped by certainty then unit and namespace, `--checklist`, DOT and Mermaid output, the "N more at `possible`" line.
- `--diff <rev>`: changed hunks from hardened `git diff`, mapped to symbols by source range against the snapshot of the working tree.

**Accept**
- Golden text and JSON for forward and reverse craters on the fixture.
- `--diff` test: a temp repository with two commits; changing a function body seeds exactly that function; changing only a comment above a function seeds nothing; a deleted function is reported as removed (its name comes from the base revision's file content, read with hardened `git show` and the form index); its former callers are listed when a snapshot of the base revision is in the cache, and a warning says so when it is not.

## WP3.3 JVM trace normalizer

**Build** (`aldente-analysis::trace`)
- Canonical frame types, input auto-detection, the JVM text parser with `Caused by` chains, the EDN `Throwable->map` reader, demunging, anonymous-function attribution, `rules/trace-noise.toml`, frame-to-node mapping with match levels.
- `aldente trace normalize`.

**Accept**
- A corpus under `fixtures/traces/jvm/` with at least twelve real-shaped traces: plain exception, nested causes, `ex-info`, lazy-seq realization, `future` and thread-pool frames, anonymous functions, protocol and multimethod frames, a REPL trace, a test-runner trace, a trace with munged special characters in names, a truncated trace, and log lines interleaved.
- Demunge round-trip property test over generated Clojure names.
- For traces generated from `fixtures/clj-mini` (stored as text, with the fixture code that produced them), every repository frame maps `exact`.

## WP3.4 ClojureScript traces and source maps

**Build**
- Browser (Chrome, Firefox, Safari formats) and Node frame parsers.
- Source map v3 reader (the `sourcemap` crate) with `--source-map`; fall back to JS-name demunging.

**Accept**
- Fixture traces with and without source maps (small prebuilt `.js` and `.map` files checked in as test data, generated once and documented).
- With a map, frames resolve to `.cljs` file and line and to the enclosing node; without, name matching finds the symbol at match level `name`.

## WP3.5 Trace-pinned crater

**Build**
- Overlay edges with reason `trace-observed`, never written to the snapshot; pinned crater per the document; the unmapped-frame and missing-edge reports.

**Accept**
- For a fixture trace passing through a multimethod with three impls, the pinned crater contains only the impl in the trace.
- A trace containing a call the static graph lacks (constructed deliberately through `eval` in the fixture) is reported in the missing-edge list.

## WP3.6 Library impact

**Build**
- `aldente lib ls`, `aldente lib usage`; library nodes as crater seeds.
- API endpoints `/libs` and `/lib`.

**Accept**
- On the fixture, `lib usage reagent` lists exactly the expected sites; a namespace that requires but never calls is listed separately.
- `aldente crater lib:npm:react --direction reverse` returns the using code.

## WP3.7 GUI: crater and library views

**Build**
- Crater view with ELK layered layout, direction switch, depth slider, certainty filter, "why is this here" path on hover, boundary markers, checklist tab with export to CSV and Markdown.
- Paste-a-trace dialog: normalize, show frames with noise folded, pin the crater.
- Libraries view with highlight-on-graph.
- "Crater forward" and "Crater reverse" in every node's menu.

**Accept**
- Playwright: open a crater from a node; change direction; paste a fixture trace and see the pinned path highlighted; export the checklist.
- A crater of 2,000 nodes lays out without freezing the page (layout in a worker, progress shown).

## WP3.8 Milestone M3 acceptance

**Accept**
- On an acceptance repository, pick three functions with Austin (or by highest fan-in): forward and reverse craters are produced, each under two seconds, and a hand check of twenty reverse-crater entries finds no entry that cannot reach the seed.
- One real stack trace from that repository (Austin supplies, or produce one from its tests' documented failures) normalizes with every repository frame mapped.
