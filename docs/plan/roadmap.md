# Roadmap

Twelve phases. Each ends in something Austin can run. Work packages (WPs) and their acceptance criteria are in `phases/phase-NN.md`.

## Phases

| Phase | Name | Delivers | Milestone |
|---|---|---|---|
| 0 | Foundations | Workspace, license, CI on three systems, model crate, fixture harness, `doctor` | |
| 1 | First proof | Safe acquisition, Clojure adapter v1, snapshot, core CLI, server, GUI showing namespace and symbol graphs | **M1: a real Clojure/ClojureScript repo is graphed and displayed** |
| 2 | Call resolution | Certainty on every edge, multimethods, protocols, higher-order flow, dynamic holes, interop exits, `.cljc`, re-frame, library attribution, blind-spot report | M2: the graph is trustworthy |
| 3 | Crater, traces, libraries | Impact crater both directions, checklist, diff seeding, stack trace normalizer, trace-pinned crater, library impact | M3: use cases 3 and 4 |
| 4 | Measures and coverage | All graph measures, hotspots, cycles, dead code, churn, static coverage, risk score, overlays | M4: use cases 5 and 8 (static) |
| 5 | GUI depth and scale | Aggregation and expand at scale, pruning workspace, source panel, editor links, service overview, large-graph performance | M5: usable on a large monorepo |
| 6 | Outside world | Rule catalogs, vet report, ingress, egress, internal API links, datastores and data shape, surface view | M6: use cases 6 and 7 |
| 7 | Dependency graph | Manifest and lockfile parsing, transitive resolution, conflict findings, `deps` commands and view | M7: use case 9 |
| 8 | State machines | Detectors D1 to D4, `fsm` commands, diagram view | M8: use case 2 |
| 9 | Scale and incremental | Benchmark corpus, per-file incremental analysis, memory and store tuning, performance budgets met | M9: target scale proven |
| 10 | Python and TypeScript | Adapter protocol, TypeScript adapter, Python adapter, catalogs and traces for both, Leña fact import | M10: next-target languages |
| 11 | Execution, Emacs, release | Execution gate, measured coverage, Emacs package, release builds, open-source readiness | M11: shippable |

## Why this order

- **M1 first and alone.** The handoff names it as the first proof. Phase 1 is a thin vertical slice through every layer (acquire, analyze, store, CLI, server, GUI), so every later phase adds to working software.
- **Resolution before analyses.** Craters, measures, coverage and vetting are all only as good as the edges. Phase 2 makes the edges right and labeled before anything is built on them.
- **Crater before measures.** It is the most interactive use case and exercises the traversal engine that coverage and vetting reuse.
- **GUI depth after there is something to show.** Phases 1 to 4 each add a simple view. Phase 5 then makes the explorer good with real data and real complaints in hand.
- **Outside-world analysis waits for Austin's review** of `10-security-and-deployment.md`, and benefits from the literal harvest and library attribution already in place.
- **Scale is designed in from Phase 1 and tuned in Phase 9.** Units, payload caps, aggregated APIs and the snapshot store exist from the start. Incremental analysis comes late because it must be proven equal to full analysis, which is easier once the rules have stopped changing.
- **New languages after the model has been stressed** by eight phases of Clojure. The adapter trait exists from Phase 1 so nothing Clojure-specific leaks into the core.
- **Execution last.** It is the only feature that can hurt the user, so it lands after the vetting it depends on.

**Later languages.** Java and C# come after Phase 10 (decision D12). They have no phase yet.

Phases 6, 7 and 8 are independent of each other and can be reordered to taste after Phase 5. Phase 9 can move earlier if the acceptance repositories turn out to be slow.

## Use-case trace

| Use case | Phases | Plan document |
|---|---|---|
| 1. Repository graph, two levels | 1, 2, 5 | `03`, `04`, `06` |
| 2. Inferred state machines | 8 | `12` |
| 3. Impact crater | 3 (checklist coverage in 4) | `08` |
| 4. Dependency impact | 2 (attribution), 3 | `04` R10, `08` |
| 5. Network analysis | 4 | `09` |
| 6. Security vetting | 6 | `10` |
| 7. Deployment view | 6 | `10` |
| 8. Test coverage | 4 (static), 11 (measured) | `09` |
| 9. Dependency hell | 7 | `11` |
| Call resolution rules | 2 | `03`, `04` |
| Stack trace normalizer | 3, 10 | `08` |
| Input and safety | 1, 11 | `07` |
| CLI, JSON output | 1 onward | `05` |
| Local web GUI | 1, 5 | `06` |
| Emacs layer | 11 | phase 11 |
| Three operating systems | 0 onward | `02` |
| Scale, monorepos | 1 (units), 5, 9 | `02`, `06` |
| Python, TypeScript | 10 | phase 10, decision D11 |
| Java, C# | after 10 (no phase yet) | decision D12 |

## Acceptance repositories

`acceptance/repos.toml` lists repositories by URL and pinned commit, with expected facts (symbols that must exist, edges that must exist, edges that must not). `cargo run -p acceptance` clones them safely into the cache and checks. It is not part of the default test run because it needs the network.

Until Austin supplies his repositories, use the stand-ins in `open-questions.md`. When his arrive, add them and write expectations with him: ten to twenty facts per repository that he knows to be true.

## Definition of done, for any WP

1. Acceptance criteria in the phase file pass.
2. New behavior has fixture-backed tests; golden files reviewed, not blindly accepted.
3. `fmt`, `clippy -D warnings`, tests and `cargo deny` pass on all three systems in CI.
4. Every new command has `--json`, a schema, and a line in `docs/cli.md` (user documentation, started in Phase 1).
5. No rule in `CLAUDE.md` is bent.
6. `PROGRESS.md` updated.
