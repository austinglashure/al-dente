# Decisions

These are settled. Each has the alternatives that were considered. If one proves wrong during implementation, stop and raise it with Austin through `PROGRESS.md`.

## D1. Al Dente is a separate project, not a fork of Leña

**Decision.** New repository, new Rust workspace. No dependency on `lena-core` or MesoDB. A Leña fact importer arrives in Phase 10.

**Why.** Leña covers a small part of the need (see `01-lena-assessment.md`). The parts Al Dente needs most (certainty on edges, dispatch, interop, units, caching, a server) all cut against Leña's schema, ID scheme and ingest path. Graph measures and craters need an in-memory graph, not a Datalog store. Building on a young database from a single author would put an unknown on the critical path.

**Alternatives.** Fork Leña and extend it (inherits the schema and storage limits; diverges from the coworker's goals of docs and LLM context). Contribute everything upstream (couples Al Dente's pace to someone else's project).

**Austin can override** this one for non-technical reasons. The cost of switching is lowest before Phase 1 starts.

## D2. Rust for the engine, TypeScript for the GUI

**Why.** One self-contained binary on Linux, macOS and Windows; fast enough for millions of lines; the Clojure front end is an external tool regardless of host language, so nothing pulls toward the JVM. The GUI must be a browser app, so TypeScript.

**Alternatives.** Clojure on the JVM (natural for the domain, but slow start for a CLI used from Emacs, heavier distribution). Go (fine, fewer graph and parsing libraries).

## D3. clj-kondo is the Clojure front end, run as a locked-down subprocess

**Decision.** Require `clj-kondo` on `PATH` (minimum version pinned in Phase 1). Invoke it with an Al Dente-owned config directory, cache off, lint findings ignored, analysis buckets on. Add Al Dente's own Tree-sitter pass for what clj-kondo does not report.

**Why.** Correct symbol resolution in Clojure (aliases, refers, `.cljc` reader conditionals, macros) is years of work that clj-kondo has done. It is static. Its license (EPL) is incompatible with linking into AGPL code, which a subprocess avoids.

**Alternatives.** Write a resolver from scratch on tree-sitter-clojure (large, worse results). Use clojure-lsp (heavier, wraps the same analysis). Use tools.analyzer (evaluates code; violates the static rule).

## D4. In-process adapters behind a trait; an out-of-process protocol later

**Decision.** Language adapters are Rust crates implementing one `Adapter` trait and returning typed facts. A JSON Lines protocol for external adapters is defined in Phase 10.

**Why.** Typed structs are simpler and faster than Leña's stringly EDN, and the first three languages are all written here.

## D5. SQLite snapshot plus in-memory graph

**Decision.** Each analysis produces a snapshot: one SQLite file (`rusqlite`, bundled) in the user cache directory. Commands load it into an in-memory compressed adjacency structure for traversal and measures.

**Why.** Boring, portable, inspectable with standard tools, and trivially handles a few million edges. Measures and craters need memory-resident adjacency.

**Alternatives.** MesoDB (see D1). A graph database server (violates "no required runtime"). Plain JSON files (too slow to load partially at target scale).

## D6. Recompute whole snapshots first; incremental in Phase 9

**Decision.** Phases 1 to 8 key the cache on the whole analyzed tree: unchanged tree, instant reuse; any change, full re-analysis. Phase 9 adds a per-file fact cache.

**Why.** It matches the handoff's preference for recomputing, keeps correctness simple while resolution rules are still changing, and the cache key already makes repeated commands instant.

## D7. Three certainty levels and a reason on every edge

**Decision.** `certain`, `likely`, `possible`, each edge also carrying a machine-readable reason. Defined in `03-graph-model.md`.

**Why.** The handoff needs uncertain calls shown differently, filtered on demand and weighted lower in measures. Three levels are enough to act on and few enough to style.

## D8. Hardened `git` CLI for acquisition

**Decision.** Shell out to the user's `git` with a fixed set of protective flags and environment (see `07-safety.md`). Git is a required tool.

**Why.** Auditable, well understood, handles every authentication setup the user already has. **Alternative.** The pure-Rust `gix` crate removes the dependency and cannot run hooks or filters at all; revisit if Windows setup proves painful.

## D9. License: AGPL-3.0-only

Matches Leña's manifest. Add the full license text as `LICENSE`, SPDX headers are not required, `cargo deny` enforces compatible dependencies.

## D10. Sigma.js for graph rendering

**Decision.** Sigma.js v3 on graphology, WebGL. Layered layouts (craters, state machines) use ELK in a web worker.

**Why.** It stays interactive with tens of thousands of nodes; graphology gives a clean in-browser graph model. **Alternative.** Cytoscape.js has built-in compound nodes but slows down an order of magnitude sooner.

## D11. Python and TypeScript adapters are native Rust, no Node or Python runtime

**Decision.** Parse with Tree-sitter (or `oxc` for TypeScript if the Phase 10 spike prefers it), resolve imports and scopes in Rust, treat type annotations as evidence that raises certainty.

**Why.** Keeps the single-binary promise and the static rule (the TypeScript compiler and Python tools want the project's dependencies installed). It fits the over-approximate-with-certainty model. Phase 10 opens with a time-boxed spike that can revise the parser choice but not the no-runtime constraint.
