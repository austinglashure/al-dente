# Al Dente

Al Dente is a static analyzer that turns a git repository into an explorable graph of namespaces, symbols and calls. Every edge carries a certainty and a reason, and anything the analyzer cannot see through is shown as a blind spot. It runs entirely on your machine, has a CLI (every command supports `--json`) and a local web GUI, and never executes the code it analyzes. Clojure and ClojureScript come first; Python and TypeScript follow.

## Status

Pre-alpha. Phase 0 (foundations) is in progress: the graph model and JSON envelope exist, and the `aldente` binary has `analyze`, `find`, `show`, `graph`, `blindspots`, `units`, `cache`, `doctor` and `schema`. Phase 1 (first proof) is in progress. See `PROGRESS.md` for what is done and `docs/plan/roadmap.md` for the plan.

## Run

One-time setup: install Rust with [rustup](https://rustup.rs). The pinned toolchain in `rust-toolchain.toml` is fetched automatically on first build.

```sh
cargo run -- --version
cargo run -- doctor              # checks git, clj-kondo and the cache directory
cargo run -- units PATH|URL      # build units and source roots of a repository (URLs are cloned safely)
cargo run -- analyze PATH|URL    # build (or reuse) the snapshot and print counts
cargo run -- show charge! PATH    # a symbol with its callers and callees
cargo run -- graph --format dot PATH | dot -Tsvg > g.svg
cargo run -- serve PATH|URL      # the GUI on 127.0.0.1 (prints the URL)
cargo run -- cache ls            # clones and snapshots in the cache
cargo run -- schema envelope     # JSON Schema of the output envelope
```

Command reference: `docs/cli.md`.

Analysis needs `git` (2.30+) and `clj-kondo` (2026.10.26+) on `PATH`; `aldente doctor` checks both.

## Test

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check        # needs cargo-deny; CI runs it on Linux, macOS and Windows
```

UI (in `ui/`, needs Node 24+; only for changing the UI, since the built UI in `ui/dist` is committed and embedded in the binary):

```sh
cd ui
npm ci                              # once
npm run check && npm test           # type-check, lint, unit tests
npm run build                       # rebuild ui/dist (commit it with the change)
npx playwright install chromium     # once
npm run e2e                         # builds the binary, then browser tests
```

Golden files live under `fixtures/<name>/golden/`. After an intended change, review and accept them with `cargo insta review` (needs `cargo-insta`) or `INSTA_UPDATE=always cargo test`, then read the diff before committing.

## Structure

| Path | What |
|---|---|
| `crates/aldente-cli` | the `aldente` binary |
| `crates/aldente-model` | nodes, edges, certainty, IDs, JSON envelope |
| `crates/aldente-acquire` | hardened git, file walk, units, snapshot keys |
| `crates/aldente-adapter` | the `Adapter` trait and fact types |
| `crates/aldente-clj` | Clojure and ClojureScript adapter |
| `crates/aldente-graph` | graph build, SQLite snapshot, in-memory graph |
| `crates/aldente-analysis` | crater, measures, coverage, vetting and more |
| `crates/aldente-rules` | loader for detection catalogs in `rules/` |
| `crates/aldente-server` | local HTTP API and embedded UI |
| `ui/` | the GUI (React, Sigma.js); `ui/dist` is the committed build |
| `crates/aldente-testkit` | test support: fixture paths, golden graph harness |
| `crates/acceptance` | runner for real-repository acceptance checks (needs network) |
| `fixtures/` | crafted repositories and golden graphs (`clj-mini/EXPECTED.md` is the ground truth, `mono-mini` is a three-unit monorepo); `hostile/` safety cases |
| `docs/plan/` | the full design and phase plan |

Most crates are empty until their work package lands.

## License

AGPL-3.0-only. See `LICENSE`.

## Acknowledgements

Al Dente is inspired by Leña, whose idea of thin language adapters feeding one graph shaped this design. No code is shared.
