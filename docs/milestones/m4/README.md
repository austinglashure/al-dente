# Milestone M4: measures and coverage

Phase 4 acceptance (WP4.7, `docs/plan/phases/phase-04.md`) on **ring** and **reitit**, the M3 repositories, at the commits pinned in `acceptance/repos.toml`. Produced with the release build on the cached clones:

```
aldente hotspots --json <clone>        > <repo>-hotspots.json
aldente hotspots --top 20 <clone>      > <repo>-hotspots-top20.txt
aldente cycles --json <clone>          > <repo>-cycles-namespace.json
```

`hotspots` took 0.8 s (ring) and 3.8 s (reitit) on a cold measure cache, with built-in rules and no churn.

## Checks

| Criterion | Result |
|---|---|
| Hotspots report produced; top twenty reviewed with Austin, at least fifteen "yes, a real concern or a real core function"; misses and rule changes recorded | **report produced; review pending** (fill the last column below) |
| Namespace cycles match an independent check | **holds**: clj-kondo's own `namespace-usages` (run with an empty `--config-dir`) fed to networkx (`ns_cycles_check.py`) finds 0 cycles in ring (72 namespaces, 92 requires) and 0 in reitit (103, 299); `aldente cycles` finds 0 in both. Because zero against zero is a weak check, the same comparison on a six-namespace repository with a 3-cycle and a 2-cycle gives the same two cycles both ways (also a CLI test, `cycles_finds_require_cycles`). |

## Things I noticed before your review

These are candidates for "misses" and rule changes, not yet acted on:

1. **Example and benchmark code ranks high.** reitit's `examples/*` are separate units, so `example.server/app` appears several times (bottleneck, god object); ring's `ring-bench` `-main` and `bench-env` are "risky untested". Possible rule: leave units under `examples/`, `bench*`, `perf*` and `dev/` out of findings by default (a list in `hotspots.toml`).
2. **Entry points as risky untested.** `-main` functions are rarely unit-tested on purpose. Possible rule: skip entry points in risky-untested.
3. **The same node under two findings** (`resource-request` as bottleneck and must-be-bulletproof; `find-charset-in-content-type` as bulletproof and risky untested). Possible change: merge findings per node, listing the kinds.
4. **reitit's `reitit.dev.pretty/color(s)`** (dev error printing) ranks as must-be-bulletproof because every error formatter calls it. A real core function of that namespace, but arguably not a production concern.
5. **Dead code dominates the count** (50 of 86 in ring, 93 of 207 in reitit) but sits below the top twenty at score 0.5, which seems right.

### ring: top twenty of 86 findings

| # | Kind | Subject | Score | Reason | Real concern? |
|---:|---|---|---:|---|---|
| 1 | must-be-bulletproof | `ring.util.response/find-header` | 1.00 | ring.util.response/find-header: much important code ends up calling it (PageRank in the top 1%); test it directly and harden its error handling. | |
| 2 | risky-untested | `ring.bench.servlet/bench-env` | 1.00 | ring.bench.servlet/bench-env is heavily used (importance 0.28) but no test reaches it; add a test that calls it. | |
| 3 | must-be-bulletproof | `ring.util.response/header` | 0.99 | ring.util.response/header: much important code ends up calling it (PageRank in the top 1%); test it directly and harden its error handling. | |
| 4 | bottleneck | `ring.util.response/url-response` | 0.99 | ring.util.response/url-response: many call paths run through it and removing it disconnects part of the graph; a change here ripples widely, so keep its interface stable. | |
| 5 | risky-untested | `ring.bench.servlet/-main` | 0.99 | ring.bench.servlet/-main is heavily used (importance 0.26) but no test reaches it; add a test that calls it. | |
| 6 | must-be-bulletproof | `ring.util.response/get-header` | 0.99 | ring.util.response/get-header: much important code ends up calling it (PageRank in the top 1%); test it directly and harden its error handling. | |
| 7 | bottleneck | `ring.middleware.resource/resource-request` | 0.99 | ring.middleware.resource/resource-request: many call paths run through it and removing it disconnects part of the graph; a change here ripples widely, so keep its interface stable. | |
| 8 | risky-untested | `ring.middleware.cookies/class-by-name` | 0.99 | ring.middleware.cookies/class-by-name is heavily used (importance 0.22) but no test reaches it; add a test that calls it. | |
| 9 | must-be-bulletproof | `ring.core.protocols/find-charset-in-content-type` | 0.99 | ring.core.protocols/find-charset-in-content-type: much important code ends up calling it (PageRank in the top 2%); test it directly and harden its error handling. | |
| 10 | risky-untested | `ring.middleware.session.cookie/deserialize` | 0.99 | ring.middleware.session.cookie/deserialize is heavily used (importance 0.29) but tests reach it only through uncertain calls; add a test that calls it. | |
| 11 | must-be-bulletproof | `ring.util.time/formatter` | 0.99 | ring.util.time/formatter: much important code ends up calling it (PageRank in the top 2%); test it directly and harden its error handling. | |
| 12 | risky-untested | `ring.core.protocols/response-charset` | 0.99 | ring.core.protocols/response-charset is heavily used (importance 0.29) but tests reach it only through uncertain calls; add a test that calls it. | |
| 13 | risky-untested | `ring.middleware.session.cookie/seal` | 0.98 | ring.middleware.session.cookie/seal is heavily used (importance 0.28) but tests reach it only through uncertain calls; add a test that calls it. | |
| 14 | must-be-bulletproof | `ring.middleware.resource/resource-request` | 0.98 | ring.middleware.resource/resource-request: much important code ends up calling it (PageRank in the top 2%); test it directly and harden its error handling. | |
| 15 | must-be-bulletproof | `ring.middleware.lint/lint` | 0.98 | ring.middleware.lint/lint: much important code ends up calling it (PageRank in the top 2%); test it directly and harden its error handling. | |
| 16 | risky-untested | `ring.core.protocols/find-charset-in-content-type` | 0.98 | ring.core.protocols/find-charset-in-content-type is heavily used (importance 0.27) but tests reach it only through uncertain calls; add a test that calls it. | |
| 17 | god-object | `ring.middleware.file-info/file-info-response` | 0.98 | ring.middleware.file-info/file-info-response does too much: it calls 7 functions and sits in the densest core (k = 3); split it by responsibility. | |
| 18 | risky-untested | `ring.core.protocols/re-charset` | 0.98 | ring.core.protocols/re-charset is heavily used (importance 0.21) but tests reach it only through uncertain calls; add a test that calls it. | |
| 19 | risky-untested | `ring.util.parsing/re-token` | 0.97 | ring.util.parsing/re-token is heavily used (importance 0.19) but tests reach it only through uncertain calls; add a test that calls it. | |
| 20 | risky-untested | `ring.handler.dump/req-pair` | 0.97 | ring.handler.dump/req-pair is heavily used (importance 0.13) but no test reaches it; add a test that calls it. | |

### reitit: top twenty of 207 findings

| # | Kind | Subject | Score | Reason | Real concern? |
|---:|---|---|---:|---|---|
| 1 | must-be-bulletproof | `reitit.dev.pretty/color` | 1.00 | reitit.dev.pretty/color: much important code ends up calling it (PageRank in the top 1%); test it directly and harden its error handling. | |
| 2 | bottleneck | `reitit.core/router` | 1.00 | reitit.core/router: many call paths run through it and removing it disconnects part of the graph; a change here ripples widely, so keep its interface stable. | |
| 3 | risky-untested | `reitit.frontend.easy/href` | 1.00 | reitit.frontend.easy/href is heavily used (importance 0.46) but no test reaches it; add a test that calls it. | |
| 4 | must-be-bulletproof | `reitit.exception/fail!` | 1.00 | reitit.exception/fail!: much important code ends up calling it (PageRank in the top 1%); test it directly and harden its error handling. | |
| 5 | must-be-bulletproof | `reitit.impl/fast-assoc` | 1.00 | reitit.impl/fast-assoc: much important code ends up calling it (PageRank in the top 1%); test it directly and harden its error handling. | |
| 6 | must-be-bulletproof | `reitit.frontend.easy/href` | 1.00 | reitit.frontend.easy/href: much important code ends up calling it (PageRank in the top 1%); test it directly and harden its error handling. | |
| 7 | risky-untested | `reitit.frontend/router` | 1.00 | reitit.frontend/router is heavily used (importance 0.22) but no test reaches it; add a test that calls it. | |
| 8 | must-be-bulletproof | `reitit.frontend.easy/history` | 0.99 | reitit.frontend.easy/history: much important code ends up calling it (PageRank in the top 1%); test it directly and harden its error handling. | |
| 9 | risky-untested | `reitit.dev.pretty/colors` | 0.99 | reitit.dev.pretty/colors is heavily used (importance 0.25) but tests reach it only through uncertain calls; add a test that calls it. | |
| 10 | must-be-bulletproof | `reitit.coercion/coerce-request` | 0.99 | reitit.coercion/coerce-request: much important code ends up calling it (PageRank in the top 1%); test it directly and harden its error handling. | |
| 11 | bottleneck | `reitit.trie/compile` | 0.99 | reitit.trie/compile: many call paths run through it and removing it disconnects part of the graph; a change here ripples widely, so keep its interface stable. | |
| 12 | must-be-bulletproof | `reitit.coercion/encode-error` | 0.99 | reitit.coercion/encode-error: much important code ends up calling it (PageRank in the top 1%); test it directly and harden its error handling. | |
| 13 | risky-untested | `reitit.dev.pretty/-start` | 0.99 | reitit.dev.pretty/-start is heavily used (importance 0.23) but tests reach it only through uncertain calls; add a test that calls it. | |
| 14 | must-be-bulletproof | `reitit.dev.pretty/colors` | 0.99 | reitit.dev.pretty/colors: much important code ends up calling it (PageRank in the top 1%); test it directly and harden its error handling. | |
| 15 | bottleneck | `example.server/app` | 0.99 | example.server/app: many call paths run through it and removing it disconnects part of the graph; a change here ripples widely, so keep its interface stable. | |
| 16 | risky-untested | `frontend.core/state` | 0.99 | frontend.core/state is heavily used (importance 0.16) but no test reaches it; add a test that calls it. | |
| 17 | must-be-bulletproof | `reitit.dev.pretty/-start` | 0.99 | reitit.dev.pretty/-start: much important code ends up calling it (PageRank in the top 1%); test it directly and harden its error handling. | |
| 18 | god-object | `example.server/app` | 0.99 | example.server/app does too much: it calls 21 functions and sits in the densest core (k = 5); split it by responsibility. | |
| 19 | risky-untested | `reitit.coercion/map->CoercionError` | 0.99 | reitit.coercion/map->CoercionError is heavily used (importance 0.15) but no test reaches it; add a test that calls it. | |
| 20 | god-object | `example.server/app` | 0.99 | example.server/app does too much: it calls 23 functions and sits in the densest core (k = 5); split it by responsibility. | |
