# Al Dente

Al Dente is a static analyzer that turns a git repository into an explorable graph of namespaces, symbols and calls. Every edge carries a certainty and a reason, and anything the analyzer cannot see through is shown as a blind spot. It runs entirely on your machine, has a CLI (every command supports `--json`) and a local web GUI, and never executes the code it analyzes. Clojure and ClojureScript come first; Python and TypeScript follow.

## Status

Pre-alpha. Phase 0 (foundations) is in progress: the graph model and JSON envelope exist, and the `aldente` binary answers `--version` and `schema`. See `PROGRESS.md` for what is done and `docs/plan/roadmap.md` for the plan.

## Run

One-time setup: install Rust with [rustup](https://rustup.rs). The pinned toolchain in `rust-toolchain.toml` is fetched automatically on first build.

```sh
cargo run -- --version
cargo run -- schema envelope     # JSON Schema of the output envelope
```

Command reference: `docs/cli.md`.

Later phases also need `git` and `clj-kondo` on `PATH`.

## Test

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check        # needs cargo-deny; CI runs it on Linux, macOS and Windows
```

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
| `crates/acceptance` | runner for real-repository acceptance checks (needs network) |
| `docs/plan/` | the full design and phase plan |

Most crates are empty until their work package lands.

## License

AGPL-3.0-only. See `LICENSE`.

## Acknowledgements

Al Dente is inspired by Leña, whose idea of thin language adapters feeding one graph shaped this design. No code is shared.
