# Milestone M2: dispatch, interop and libraries

Phase 2 acceptance (WP2.10, `docs/plan/phases/phase-02.md`). Produced on 2026-11-12 with `cargo run --release -p acceptance` (reports in this directory, one `<repo>-acceptance.json` each; repositories pinned in `acceptance/repos.toml`). Library attribution read the local Maven cache (`~/.m2/repository`) where present.

## Checks

| Criterion | Result |
|---|---|
| `fixtures/clj-mini/EXPECTED.md` fully satisfied: no missing edge, no extra `certain` edge | holds: `m2_clj_mini_expected_is_fully_satisfied` in `crates/aldente-testkit/tests/rules.rs` checks all 130 listed rows and allows only the unlisted `let`/`fn`/`fn*`/`deref`/`var` calls its scope rules 2, 3 and 4a describe |
| Blind spots by category, edges by certainty and reason, orphan share recorded per repository | below, and in each report (`summary.blindspots`, `edges_by_kind_certainty_reason`, `orphans`) |
| Twenty largest orphaned functions investigated on one repository | reitit, below |
| Facts in `repos.toml`, with at least five dispatch facts per repository, hold | all hold: ring 5, reitit 7, re-frame 5 new dispatch facts (each with the source line it was read from), plus the M1 facts |

## Blind spots by category

| Category | ring | reitit | re-frame |
|---|---:|---:|---:|
| dynamic-eval | 2 | 2 | 4 |
| dynamic-resolve | 0 | 5 | 9 |
| interop | 352 | 143 | 69 |
| macro-generated | 10 | 2 | 4 |
| unresolved-symbol | 71 | 89 | 99 |

## Edges by reason and certainty

Summed over edge kinds; `-` is an edge without that field (`contains`, `requires`, `implements`). Per-kind counts are in the reports.

| Reason | Certainty | ring | reitit | re-frame |
|---|---|---:|---:|---:|
| - | - | 1229 | 2556 | 1498 |
| direct | certain | 3163 | 7694 | 6075 |
| dynamic | certain | 2 | 7 | 13 |
| interop | certain | 644 | 270 | 94 |
| keyword-dispatch | likely | 0 | 5 | 172 |
| keyword-dispatch | possible | 0 | 1 | 0 |
| macro-body | possible | 80 | 10 | 52 |
| multimethod-dispatch | possible | 7 | 13 | 21 |
| name-match | likely | 10 | 72 | 5 |
| name-match | possible | 4 | 59 | 4 |
| param-flow | possible | 10 | 14 | 7 |
| protocol-dispatch | likely | 0 | 1 | 0 |
| protocol-dispatch | possible | 76 | 267 | 36 |
| value-ref | likely | 348 | 1695 | 1056 |

## Orphaned symbols

A repository symbol (a var with a location) is eligible unless it is an entry point, a test, or in a test namespace. It is orphaned when no edge other than `contains` points at it.

| Repository | Eligible | Orphaned | Share |
|---|---:|---:|---:|
| ring | 305 | 2 | 0.7% |
| reitit | 615 | 63 | 10.2% |
| re-frame | 496 | 63 | 12.7% |

## The twenty largest orphaned functions in reitit

Read by hand at the pinned commit. "Lines" is the span of the defining form (a record's generated factory spans the whole `defrecord`).

| Lines | Function | Kind | How it is really reached |
|---:|---|---|---|
| 73 | `reitit.dev.pretty/->EdnPrinter` (modules/reitit-dev/src/reitit/dev/pretty.cljc:73) | record factory | code uses `map->EdnPrinter` (pretty.cljc:151) |
| 42 | `reitit.frontend.history/->Html5History` (modules/reitit-frontend/src/reitit/frontend/history.cljs:100) | record factory | code uses `map->Html5History` (history.cljs:176) |
| 36 | `reitit.frontend.history/->FragmentHistory` (modules/reitit-frontend/src/reitit/frontend/history.cljs:18) | record factory | code uses `map->FragmentHistory` (history.cljs:175) |
| 14 | `server/start` (examples/pedestal-malli-swagger/src/server.clj:149) | example REPL entry | called only from a discarded form `#_(start server config)` (server.clj:164) |
| 12 | `example.naive/app` (examples/just-coercion-with-ring/src/example/naive.clj:15) | example REPL entry | used from the example README: `(server/restart naive/app)` |
| 10 | `example.server/restart` (examples/just-coercion-with-ring/src/example/server.clj:8) | example REPL entry | used from the example README |
| 10 | `reitit.ring.schema/map->Upload` (modules/reitit-schema/src/reitit/ring/schema.cljc:6) | record factory | code uses `->Upload` (schema.cljc:20) |
| 9 | `reitit.http.interceptors.dev/print-context-diffs` (modules/reitit-interceptors/src/reitit/http/interceptors/dev.clj:45) | library API | users pass it as the `::reitit.interceptor/transform` router option (doc/http/transforming_interceptor_chain.md) |
| 8 | `reitit.ring.middleware.dev/print-request-diffs` (modules/reitit-middleware/src/reitit/ring/middleware/dev.clj:46) | library API | users pass it as a router option (doc/ring/default_middleware.md) |
| 6 | `reitit.http.spec/validate` (modules/reitit-http/src/reitit/http/spec.cljc:21) | library API | users pass it as `{:validate spec/validate}` (CHANGELOG) |
| 5 | `reitit.ring.middleware.exception/wrap-exception` (modules/reitit-middleware/src/reitit/ring/middleware/exception.clj:111) | library API | public middleware; nothing in the repository uses it |
| 4 | `reitit.http.interceptors.exception/wrap-log-to-console` (modules/reitit-interceptors/src/reitit/http/interceptors/exception.clj:69) | library API | public handler for exception middleware; nothing in the repository uses it |
| 4 | `reitit.ring.middleware.exception/wrap-log-to-console` (modules/reitit-middleware/src/reitit/ring/middleware/exception.clj:94) | library API | public handler for exception middleware; nothing in the repository uses it |
| 2 | `reitit.impl/wild-path?` (modules/reitit-core/src/reitit/impl.cljc:78) | unused | no reference anywhere in the repository |
| 2 | `reitit.dev.pretty/indent` (modules/reitit-dev/src/reitit/dev/pretty.cljc:249) | unused | no reference anywhere in the repository |
| 1 | `reitit.coercion/->CoercionError` (modules/reitit-core/src/reitit/coercion.cljc:30) | record factory | code uses `map->CoercionError` (malli.cljc:95 and others) |
| 1 | `reitit.coercion/map->ParameterCoercion` (modules/reitit-core/src/reitit/coercion.cljc:39) | record factory | code uses `->ParameterCoercion` (coercion.cljc:42) |
| 1 | `reitit.interceptor/->Context` (modules/reitit-core/src/reitit/interceptor.cljc:12) | record factory | code uses `map->Context` (interceptor.cljc:27) |
| 1 | `reitit.interceptor/->Endpoint` (modules/reitit-core/src/reitit/interceptor.cljc:11) | record factory | code uses `map->Endpoint` (interceptor.cljc:123 and others) |
| 1 | `reitit.interceptor/->Interceptor` (modules/reitit-core/src/reitit/interceptor.cljc:10) | record factory | code uses `map->Interceptor` (interceptor.cljc:73) |

### What would have connected them

- **Record factory family (9).** `defrecord X` generates `->X` and `map->X`; code usually calls only one. A rule that treats `X`, `->X`, `map->X` and `X.` as one family (a use of any reaches the record) would connect all of these. This is the largest single cause and the cheapest to fix (R6 extension).
- **Public API of library units (6).** These are meant to be called by users of the library, typically by passing them as options. A rule that marks public vars of units without a service as consumer entry points (or a `library-api` flag that the orphan count excludes) would account for them. Whether that belongs in R13 is a decision for Austin.
- **Example REPL entry points (3).** Reached from READMEs or `#_`/`(comment …)` forms. A `possible` edge from forms inside `comment` blocks and `#_` would catch `start`; README usage is outside the analyzer's scope.
- **Genuinely unused (2).** `reitit.impl/wild-path?` and `reitit.dev.pretty/indent` have no reference anywhere; these are the dead code the orphan count should find.

## re-frame handler coverage (WP2.8)

142 of 167 re-frame handler impls (85.0%) have an incoming edge; all 28 outside test namespaces do. The rest are registered in re-frame's own tests and reached only through internals (`re-frame.router/dispatch-and-settle`, `registrar/get-handler`).

