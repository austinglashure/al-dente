# Milestone M3: crater, traces, libraries

Phase 3 acceptance (WP3.8, `docs/plan/phases/phase-03.md`) on **ring** and **reitit** (Austin's choice, 2026-11-13), pinned in `acceptance/repos.toml`. Produced with:

```
cargo run --release -p acceptance -- --m3 --out docs/milestones/m3 ring reitit
```

Files: `<repo>-acceptance.json` (the M2 statistics, refreshed), `<repo>-m3.json` (seeds, crater timings, the hand-check sample with every step), `<repo>-m3-handcheck.txt` (the same, printable, with each call site's source line).

## Checks

| Criterion | Result |
|---|---|
| Three functions by highest fan-in; forward and reverse craters, each under two seconds | holds: twelve craters, the slowest 1 ms (release build, default settings: depth 6, `likely`); building the crater index takes 4 ms (ring) and 9 ms (reitit) once per snapshot |
| Hand check of twenty reverse-crater entries finds none that cannot reach the seed | holds on both repositories after one fix (below): ring 20/20, reitit 20/20, each step read against its source line |
| One real stack trace normalizes with every repository frame mapped | holds: `fixtures/traces/jvm/ring/wrap-cookies-duration.txt`, 8 of 8 `ring.*` frames map `exact`; the runner checks it on every `--m3` run |

## Seeds and timings

Fan-in = distinct repository symbols and impls that invoke the function at `likely` or stronger (tests excluded as targets). "Possible" = how many more nodes `--min-certainty possible` would add.

| Repository | Seed | Fan-in | Direction | Nodes | +possible | Truncated | Time |
|---|---|---:|---|---:|---:|---|---:|
| ring | `ring.middleware.cookies/wrap-cookies` | 33 | forward | 62 | 4 | no | 0.2 ms |
| ring | `ring.middleware.cookies/wrap-cookies` | 33 | reverse | 34 | 0 | no | 0.1 ms |
| ring | `ring.util.io/string-input-stream` | 29 | forward | 3 | 0 | no | 0.0 ms |
| ring | `ring.util.io/string-input-stream` | 29 | reverse | 37 | 0 | no | 0.1 ms |
| ring | `ring.middleware.session/wrap-session` | 16 | forward | 75 | 41 | yes | 0.2 ms |
| ring | `ring.middleware.session/wrap-session` | 16 | reverse | 17 | 0 | no | 0.1 ms |
| reitit | `reitit.ring/router` | 84 | forward | 320 | 79 | yes | 1.0 ms |
| reitit | `reitit.ring/router` | 84 | reverse | 105 | 35 | no | 0.3 ms |
| reitit | `reitit.ring/ring-handler` | 79 | forward | 48 | 40 | no | 0.1 ms |
| reitit | `reitit.ring/ring-handler` | 79 | reverse | 100 | 35 | no | 0.3 ms |
| reitit | `reitit.coercion.spec/coercion` | 38 | forward | 16 | 1 | no | 0.1 ms |
| reitit | `reitit.coercion.spec/coercion` | 38 | reverse | 59 | 11 | no | 0.2 ms |

The forward craters of `wrap-session` and `reitit.ring/router` stop at depth 6 (`truncated`); that is the default depth, not a limit of the engine.

## The hand check and the fix it led to

The first run found **3 of 20 reitit entries that reach the seed only through code inside `(comment …)` blocks** (REPL-only code in `examples/*/server.clj` and `perf-test/clj/reitit/json_perf.clj`): `(comment (start))` and similar. That code never runs when the program runs. Austin decided (2026-11-13) that calls inside a top-level `comment` form keep their reason but become `possible`. Resolver step `comment_blocks` (`crates/aldente-clj/src/resolve/comments.rs`), fixture `fixtures/clj-rules/comment-blocks/` with its `EXPECTED.md`, adapter VERSION 13. After the fix, reitit's reverse crater of `reitit.ring/router` went from 137 to 105 nodes (35 more at `possible`), and the redrawn sample of twenty entries all reach their seeds. Entries that go through a bare reference (`router` passed to `rfe/start!`, `#'app` passed to `run-jetty`) were checked against the enclosing form.

## The real trace

Produced by running ring-core's own code at the acceptance commit (Austin approved): `fixtures/traces/jvm/ring/gen.clj` reproduces the test `wrap-cookies-throws-exception-when-not-using-intervals-correctly-java` (`ring-core/test/ring/middleware/test/cookies.clj`), which expects an `AssertionError`; `gen.sh <ring clone>` regenerates it (dependencies from `~/.m2` only). The pinned crater (`aldente crater --trace …`) gives the path `wrap-cookies → cookies-response → set-cookies → write-cookies → write-attr-map` with no missing edge; the two unmapped frames are the driver script's own.
