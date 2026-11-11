# Architecture

## Pipeline

```text
target (path or URL)
   │
   ▼
[acquire]   safe clone or local open, file walk, unit discovery
   │
   ▼
[adapters]  per language: parse, resolve, emit typed facts
   │
   ▼
[build]     merge facts into one graph, apply dispatch and linking rules
   │
   ▼
[snapshot]  SQLite file in the user cache, keyed by tree hash
   │
   ▼
[load]      in-memory graph
   │
   ├── queries   find, show, neighborhood, crater
   ├── analyses  measures, coverage, vetting, surface, dependencies, state machines
   │
   ▼
[front ends]  CLI (text and JSON)  ·  HTTP API  ·  web GUI
```

Analyses that are expensive (measures, vetting) run lazily on first request and write their results back into the snapshot, so they are computed once per snapshot.

## Repository layout

```text
Cargo.toml                 workspace
LICENSE                    AGPL-3.0-only text
CLAUDE.md  PROGRESS.md
crates/
  aldente-model/           node, edge, certainty, IDs, JSON envelope, schemas
  aldente-acquire/         git hardening, walker, unit discovery, snapshot key
  aldente-adapter/         the Adapter trait and the fact types adapters emit
  aldente-clj/             Clojure and ClojureScript adapter
  aldente-graph/           graph build, SQLite store, in-memory graph, traversals
  aldente-analysis/        measures, coverage, crater, traces, libs, vet, surface, deps, fsm
  aldente-rules/           loader and matcher for the data files in rules/
  aldente-server/          axum HTTP API, embedded UI assets
  aldente-cli/             the `aldente` binary
  aldente-py/  aldente-ts/ Phase 10
  acceptance/              runner for acceptance/repos.toml (needs network; not in default tests)
  bench/                   synthetic corpus generator and benchmarks (Phases 5 and 9)
rules/                     detection catalogs (TOML)
ui/                        Vite + React + TypeScript + Sigma.js
fixtures/                  small crafted repositories and golden outputs
acceptance/                repos.toml and the acceptance runner
emacs/                     Phase 11
docs/plan/                 this plan
```

Split `aldente-analysis` into more crates only when compile times demand it.

## Key dependencies

| Need | Choice |
|---|---|
| CLI parsing | `clap` (derive) |
| Errors | `thiserror` in libraries, `anyhow` in the binary |
| Serialization | `serde`, `serde_json`; `schemars` to emit JSON Schema |
| Store | `rusqlite` with the `bundled` feature |
| Hashing | `blake3` (content hashes, snapshot keys) |
| File walk | `ignore` (gitignore-aware), symlink following off |
| Parallelism | `rayon` for analysis; `tokio` only in the server |
| Parsing | `tree-sitter` plus `tree-sitter-clojure`; later Python and TypeScript grammars |
| EDN | a small in-crate reader for `deps.edn`, `shadow-cljs.edn`, `project.clj` as data |
| Graph algorithms | `petgraph` for SCC and dominators; own implementations for the rest |
| HTTP | `axum`, assets embedded with `rust-embed` |
| Snapshot tests | `insta` |
| License check | `cargo-deny` |
| External tools | `git` (acquisition), `clj-kondo` (Clojure front end) |

## The adapter contract

```rust
pub trait Adapter: Send + Sync {
    fn id(&self) -> &'static str;                       // "clj"
    fn claims(&self, unit: &Unit) -> bool;              // does this unit hold my language?
    fn analyze(&self, unit: &Unit, ctx: &AnalyzeCtx)    // read-only file access through ctx
        -> Result<FactBatch, AdapterError>;
}
```

`FactBatch` carries nodes, edges, literals (strings harvested for later analyses), diagnostics (files that failed to parse, tool warnings) and blind spots. Adapters never write to the store and never touch the network. `AnalyzeCtx` hands out file contents only for paths inside the unit, which is where the no-escape rule is enforced.

Cross-language and cross-unit linking (HTTP calls matched to routes, shared libraries, npm packages) happens in the build step, not in adapters.

## Units

A unit is a directory that owns a build manifest: `deps.edn`, `project.clj`, `shadow-cljs.edn`, `bb.edn`, `package.json`, `pyproject.toml` and similar. Discovery walks the tree and builds a unit tree; a file belongs to the nearest enclosing unit. Source roots come from the manifest (`:paths`, `:source-paths`, aliases' `:extra-paths`), read as data. If nothing declares roots, fall back to `src`, `test`, and the unit root.

Units matter because a monorepo with twenty services will repeat namespace names such as `user` or `config`. Symbol identity includes the unit (see `03-graph-model.md`). A unit with an entry point (`-main`, a `:main` key, a Dockerfile, a shadow-cljs build target) is flagged as a **service**.

## Snapshots and the cache

- **Location.** `<user cache dir>/aldente/` via the `dirs` crate. Never inside the analyzed repository. Layout: `repos/` for remote clones, `snapshots/<key>.db`, `tmp/`.
- **Key.** `blake3` over: analyzer version, schema version, adapter versions, clj-kondo version, effective config, and the sorted list of `(relative path, content hash)` for every analyzed file. A dirty working tree therefore gets its own snapshot without any special handling.
- **Reuse.** Every command resolves its target to a key first. A hit loads; a miss analyzes. `--no-cache` forces analysis.
- **Housekeeping.** `aldente cache ls | gc | clear`. `gc` keeps the newest N snapshots per repository (default 5).
- **Schema.** Tables: `meta`, `unit`, `file`, `node`, `edge`, `site` (source locations for edges), `literal`, `diagnostic`, `blindspot`, `analysis_result` (JSON blobs keyed by analysis name and parameters). Integer primary keys inside a snapshot; string IDs are the public identity.

## In-memory graph

Loaded from the snapshot into compressed sparse rows: forward and reverse adjacency, edge attributes in parallel arrays (kind, certainty, reason, site range). Node attributes stay in SQLite and are fetched by ID on demand, except the few every traversal needs (kind, unit, namespace, flags). Aggregated graphs (namespace level, unit level) are derived once at load.

## Configuration

Precedence: built-in defaults, then `<user config dir>/aldente/config.toml`, then command-line flags. A config file inside the analyzed repository is **not** read by default, because for a stranger's repository it is hostile input. `--repo-config` opts in for the user's own projects; it can only narrow analysis (ignore globs, source roots), never enable execution or change rule catalogs.

## Performance targets

To be validated in Phase 9 against a 3-million-line corpus on an 8-core laptop. Treat them as budgets to measure, not promises.

| Operation | Target |
|---|---|
| Cold analysis, 3M lines | under 10 minutes |
| Any command on an existing snapshot | under 5 seconds to first output |
| `serve` ready on an existing snapshot | under 5 seconds |
| API response, 95th percentile | under 300 ms |
| Peak memory | under 4 GB |
| Any single API payload | at most 2,000 nodes and 10,000 edges |

## Cross-platform notes

- Paths in the model are relative to the repository root, forward slashes, as written on disk (case preserved).
- Spawn tools with argument vectors, never through a shell. Resolve `clj-kondo` and `git` with the `which` crate, honoring `.exe`.
- No reliance on symlinks, file modes or `/dev/null`; use the platform null device through `std::process::Stdio::null()` and empty temp directories.
- CI matrix: `ubuntu-latest`, `macos-latest`, `windows-latest`, with clj-kondo installed in each.
