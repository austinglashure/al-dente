# Progress

Update at the end of every session: tick finished work packages, add a dated note. Notes are for the next session: what was decided, what surprised you, what is half done.

**Current:** Phase 0, WP0.2 (not started). WP0.1 is done locally; its "CI green on all three systems" criterion is unverified until the repo is pushed to GitHub.

## Phase 0. Foundations
- [x] WP0.1 Workspace, license, CI
- [ ] WP0.2 Model crate
- [ ] WP0.3 Fixture corpus and golden harness
- [ ] WP0.4 Doctor

## Phase 1. First proof
- [ ] WP1.1 Local target, walk, units
- [ ] WP1.2 Safe remote acquisition
- [ ] WP1.3 clj-kondo runner
- [ ] WP1.4 Form index
- [ ] WP1.5 Graph build v1
- [ ] WP1.6 Snapshot store
- [ ] WP1.7 Core CLI
- [ ] WP1.8 Server and API v1
- [ ] WP1.9 GUI v1
- [ ] WP1.10 **Milestone M1** (stop and show Austin)

## Phase 2. Call resolution
- [ ] WP2.1 Certainty everywhere
- [ ] WP2.2 Multimethods
- [ ] WP2.3 Protocols, records, types, reify
- [ ] WP2.4 Higher-order flow
- [ ] WP2.5 Dynamic code and holes
- [ ] WP2.6 Interop exits
- [ ] WP2.7 `.cljc`, ClojureScript, npm
- [ ] WP2.8 Keyword dispatch
- [ ] WP2.9 Library attribution, entry points, tests
- [ ] WP2.10 Milestone M2

## Phase 3. Crater, traces, libraries
- [ ] WP3.1 Crater engine
- [ ] WP3.2 Crater CLI and checklist
- [ ] WP3.3 JVM trace normalizer
- [ ] WP3.4 ClojureScript traces and source maps
- [ ] WP3.5 Trace-pinned crater
- [ ] WP3.6 Library impact
- [ ] WP3.7 GUI: crater and library views
- [ ] WP3.8 Milestone M3

## Phase 4. Measures and coverage
- [ ] WP4.1 Measure engine, first set
- [ ] WP4.2 Measure engine, second set
- [ ] WP4.3 Churn and co-change
- [ ] WP4.4 Static coverage
- [ ] WP4.5 Hotspots and risk
- [ ] WP4.6 GUI: measures and overlays
- [ ] WP4.7 Milestone M4

## Phase 5. GUI depth and scale
- [ ] WP5.1 Synthetic large corpus
- [ ] WP5.2 Aggregation and level of detail
- [ ] WP5.3 Pruning workspace
- [ ] WP5.4 Details, source and links
- [ ] WP5.5 Service overview
- [ ] WP5.6 Milestone M5

## Phase 6. Outside world (gate: Austin's review of plan document 10)
- [ ] WP6.1 Rules engine
- [ ] WP6.2 Capability sinks and literal evidence
- [ ] WP6.3 Services and paid usage
- [ ] WP6.4 Reachability and flow
- [ ] WP6.5 Outside the source
- [ ] WP6.6 Vet report
- [ ] WP6.7 Ingress
- [ ] WP6.8 Egress, internal links, datastores, data shape
- [ ] WP6.9 Surface view
- [ ] WP6.10 Milestone M6

## Phase 7. Dependency graph
- [ ] WP7.1 Manifest and lockfile parsers
- [ ] WP7.2 Transitive graph without running tools
- [ ] WP7.3 Resolution and findings
- [ ] WP7.4 Commands and view
- [ ] WP7.5 Milestone M7

## Phase 8. State machines
- [ ] WP8.1 Stateful vars
- [ ] WP8.2 Keyed state
- [ ] WP8.3 re-frame
- [ ] WP8.4 Declared machines
- [ ] WP8.5 Commands and view
- [ ] WP8.6 Milestone M8

## Phase 9. Scale and incremental
- [ ] WP9.1 Benchmarks and profiling
- [ ] WP9.2 Pipeline tuning
- [ ] WP9.3 Incremental analysis
- [ ] WP9.4 Server and GUI under load
- [ ] WP9.5 Optional: watch mode
- [ ] WP9.6 Milestone M9

## Phase 10. Python and TypeScript
- [ ] WP10.1 Spike and adapter hardening
- [ ] WP10.2 TypeScript and JavaScript adapter
- [ ] WP10.3 Python adapter
- [ ] WP10.4 Analyses on the new languages
- [ ] WP10.5 Python and Node stack traces
- [ ] WP10.6 External adapter protocol and Leña import
- [ ] WP10.7 Milestone M10

## Phase 11. Execution, Emacs, release
- [ ] WP11.1 Execution gate
- [ ] WP11.2 Measured coverage
- [ ] WP11.3 Emacs package
- [ ] WP11.4 Release engineering
- [ ] WP11.5 Documentation and open-source readiness

## Questions for Austin

(Add here when a decision in `docs/plan/decisions.md` looks wrong or the plan is silent on something that matters. Do not work around it.)

- **Local-target git hardening gap (`07-safety.md`, Acquisition).** For local targets the plan adds only `core.fsmonitor=false` and `core.hooksPath`. `git diff`/`git log` still honor a hostile repo's `.git/config`: `diff.external`, `diff.<driver>.textconv`, `core.pager`. Proposal: always pass `--no-ext-diff --no-textconv`, set `GIT_PAGER=cat` / `--no-pager`, and add a hostile fixture for it. Needs Austin's OK before WP1.1.

## Notes

(Newest first. `YYYY-MM-DD WPx.y: note`.)

- 2026-11-11 WP0.1: Repo root is `al-dente/` (plan moved up from `al-dente-plan/`; the plan package README is now `docs/plan/README.md`). Toolchain is stable 1.99.1 via rustup; `rust-version = 1.85` (edition 2024 minimum).
- 2026-11-11 WP0.1: Crates created: the nine in `02-architecture.md` plus `crates/acceptance` (empty lib; becomes a binary when acceptance lands). `aldente-py`, `aldente-ts`, `bench` deliberately not created yet (Phases 5 and 10). `acceptance/` at the root holds `repos.toml` later.
- 2026-11-11 WP0.1: "unwrap/expect denied outside tests" = workspace clippy lints `deny` + `clippy.toml` `allow-unwrap-in-tests`/`allow-expect-in-tests`. Verified on a scratch copy that `unwrap()` and `unsafe {}` in library code fail clippy.
- 2026-11-11 WP0.1: `deny.toml` also allows `AGPL-3.0-only` (our own crates) and both `Unicode-3.0` and `Unicode-DFS-2016` (the plan says "Unicode"). cargo-deny is not installed locally; licenses were checked by hand from `cargo metadata` (all MIT/Apache/Unicode-3.0). `cargo deny check` runs only in CI so far.
- 2026-11-11 WP0.1: CI pins clj-kondo 2026.10.26 (latest release today) via release zips; macOS uses the aarch64 build (`macos-latest` is Apple Silicon). Local machine has clj-kondo 2023.09.07, which is older than what WP0.4 will set as the minimum.
- 2026-11-11 WP0.1: Added `.gitattributes` (`* text=auto eol=lf`) so Windows checkouts don't break fmt checks or future golden files. Plan was silent; simplest choice.
