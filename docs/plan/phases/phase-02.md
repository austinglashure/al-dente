# Phase 2. Call resolution

Goal (M2): the graph over-approximates correctly, every uncertain edge says why, and every blind spot is visible.

Read first: `03-graph-model.md` (certainty, reasons, blind spots), `04-clojure-analysis.md` (R5 to R14).

Each WP adds fixtures under `fixtures/clj-rules/<rule>/` with a hand-written expected edge list, and fills the matching rows of `fixtures/clj-mini/EXPECTED.md`.

## WP2.1 Certainty everywhere

**Build**
- `--min-certainty` on `graph`, `show`, `find` neighbors and all API endpoints; rolled-up edges carry `by_certainty`.
- GUI: line styles per certainty, three toggles, hidden levels removed from layout; legend.
- `analyze` summary prints edge counts by kind and certainty.

**Accept**
- Filtering at `certain` on the fixture removes exactly the edges whose golden certainty is lower.
- Playwright: toggling `possible` off removes dotted edges.

## WP2.2 Multimethods (R5)

**Accept**
- Fixture with: keyword dispatch function and literal-map call sites (matching impl `likely`, others absent from that site); arbitrary dispatch function (all impls `possible`); `:default`; a `defmethod` in a different namespace from its `defmulti`; hierarchy-based dispatch with `derive` (treated as `possible` to all).
- A forward traversal from a caller of the multimethod reaches code called inside each method body.

## WP2.3 Protocols, records, types, reify (R6)

**Accept**
- Fixture with: `defrecord` inline impl, `extend-type`, `extend-protocol` over several types, `reify`, `deftype`, a protocol extended to `nil`, `Object` and a Java interface.
- A call whose receiver is bound to a constructor in the same function resolves `likely` to one impl; an unknown receiver resolves `possible` to all.
- `show <protocol-method>` lists its impls.

## WP2.4 Higher-order flow (R7)

**Accept**
- Layer 1: a var passed to `map`, `comp`, `partial`, stored in a map, and returned from a function each produce a `references` edge, `likely`.
- Layer 2: a function that calls its parameter gets `calls` edges, reason `param-flow`, `possible`, to each var passed in that position by its callers; an anonymous-function argument produces nothing extra; arity-overloaded functions match positions per arity.
- A measured note in `PROGRESS.md`: how many `param-flow` edges appear on the acceptance repositories, to judge noise.

## WP2.5 Dynamic code and holes (R8)

**Accept**
- Each row of the R8 table has a fixture case.
- Holes appear as nodes, in `aldente blindspots`, and in the `analyze` summary.
- GUI: holes are drawn distinctly and listed in a Blind spots view that links to the graph.

## WP2.6 Interop exits (R9)

**Accept**
- Java: static method call, constructor, instance method with and without a type hint, `import`ed class, `proxy`. JS: `js/` global, method call, property access, `goog` namespace.
- Each produces an edge to a `platform` node with reason `interop`; toggling "interop exits" in the GUI shows and hides them.
- IDs follow `03-graph-model.md` so a future Java adapter can attach.

## WP2.7 `.cljc`, ClojureScript specifics, npm (R12)

**Accept**
- A `.cljc` function with different callees per reader-conditional branch yields one node and per-language edges.
- A `.cljs` namespace requiring `["react" :as react]` and `[reagent.core :as r]` yields an npm library link and an external namespace.
- Macros defined in `.clj` and used from `.cljs` via `:require-macros` or self-require resolve to the macro symbol.
- shadow-cljs build targets from `shadow-cljs.edn` mark entry points (R13).

## WP2.8 Keyword dispatch (R11)

**Build**
- `rules/clj-keyword-dispatch.toml` and its loader in `aldente-rules`.
- re-frame registrations and dispatches; Integrant keys.

**Accept**
- In the re-frame fixture: a view dispatching `[:order/submit]` reaches the `reg-event-fx` handler; the handler's `:dispatch` effect reaches the next event; `subscribe [:orders/all]` reaches the `reg-sub`.
- Dispatch with a computed keyword produces a hole.
- On the re-frame stand-in repository, the share of handler impls with at least one incoming edge is reported in `PROGRESS.md`.

## WP2.9 Library attribution (R10) and entry points (R13), tests (R14)

**Build**
- Declared dependencies parsed from manifests (names and version strings only; the full dependency model is Phase 7).
- The R10 ladder, including reading jar listings from the local Maven cache when present.
- `rules/clj-namespaces.toml`, `rules/clj-tests.toml`.
- Entry-point and test flags.

**Accept**
- Every external namespace in the fixture has a `provides` edge; `lib:unknown` is empty for the fixture.
- A test with a fake Maven cache directory proves jar listing works without loading classes; with the cache absent, prefix matching still attributes at `likely`.
- `aldente find --kind test` and `--flag entry-point` work.

## WP2.10 Milestone M2 acceptance

**Accept**
- `fixtures/clj-mini/EXPECTED.md` is fully satisfied: no missing edge, no extra `certain` edge.
- On each acceptance repository: blind-spot counts by category, edge counts by certainty and reason, and the share of repository symbols with no incoming edge (excluding entry points and tests) are recorded. Investigate the twenty largest orphaned functions on one repository by hand and record what rule would have connected them.
- Expected facts in `repos.toml`, extended with at least five dispatch facts per repository, hold.
