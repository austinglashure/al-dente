# Phase 0. Foundations

Goal: an empty but real project. Everything later lands on rails that already enforce the rules.

## WP0.1 Workspace, license, CI

**Build**
- Cargo workspace with the crates in `02-architecture.md` as empty libraries, and `aldente-cli` producing `aldente` with `--version`.
- `LICENSE` with the AGPL-3.0 text; `license = "AGPL-3.0-only"` in workspace metadata; README stating purpose, status and an acknowledgement of Leña as inspiration.
- `rust-toolchain.toml` (stable), `rustfmt.toml`, workspace lints (`unsafe_code = "forbid"`, clippy `unwrap_used` and `expect_used` denied outside tests).
- `deny.toml` allowing licenses compatible with AGPL-3.0 (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode, MPL-2.0); deny unknown.
- Release profile: `lto = "thin"`, `strip = true`, unwinding kept (the server must survive a panic in one request).
- GitHub Actions: fmt, clippy, test, `cargo deny check` on Linux, macOS, Windows. Install clj-kondo in each job.

**Accept**
- CI is green on all three systems.
- `aldente --version` prints the version and exits 0.

## WP0.2 Model crate

**Build**
- In `aldente-model`: `NodeKind`, `SymbolSubkind`, `EdgeKind`, `Certainty` (ordered), `Reason`, `Location`, `Node`, `Edge`, `Site`, `GraphPayload`, ID types with parsing and formatting per `03-graph-model.md`, the JSON `Envelope<T>`, error codes and exit codes.
- `schemars` derives; `aldente schema` prints the envelope schema.
- Property tests: every ID formats and parses back; `Certainty` ordering; envelope round-trip.

**Accept**
- `aldente schema envelope` prints valid JSON Schema.
- ID round-trip tests cover every node kind, including names with `/`, `!`, `?`, `->`, unicode, and Windows-style unit paths normalized to forward slashes.

## WP0.3 Fixture corpus and golden harness

**Build**
- `fixtures/clj-mini/`: a hand-written Clojure project of about fifteen files with `deps.edn`, `src`, `test`, one `.cljc`, one `.cljs`. It must contain at least one of each: private fn, macro, multimethod with three methods and a default, protocol with two implementing records and one `extend-type`, `reify`, higher-order call through a parameter, `requiring-resolve` on a literal and on a variable, `eval`, Java interop, JS interop, top-level side effect, an atom with `swap!` sites, a test namespace, an npm string require, a re-frame event, subscription and dispatch.
- `fixtures/clj-mini/EXPECTED.md`: the edges a correct analyzer must find, written by hand as a table. This is the ground truth later goldens are reviewed against.
- A test helper: `assert_graph_snapshot(fixture, level)` that runs the pipeline and compares with an `insta` snapshot in a stable, sorted, human-readable text form (one edge per line).
- `fixtures/hostile/` directory with a README listing the cases from `07-safety.md`; cases are filled in by the WPs that need them.

**Accept**
- The fixture is valid Clojure: `clj-kondo --lint fixtures/clj-mini` reports no errors (unresolved-symbol warnings for the npm and re-frame parts are fine).
- The helper exists and a placeholder test using an empty pipeline passes.

## WP0.4 Doctor

**Build**
- `aldente doctor`: finds `git` and `clj-kondo` with `which`, prints versions, checks minimum versions (git 2.30; clj-kondo: the release current when this WP runs, recorded as a constant), checks the cache directory is writable, reports platform. `--json` supported.
- Missing tool: clear install hint per platform, exit code 5.

**Accept**
- Tests with a stubbed `PATH` cover found, missing and too-old cases.
- On Windows CI, `doctor` finds `clj-kondo.exe`.
