# Al Dente

Al Dente is a static analyzer that turns a git repository into an explorable graph. It runs locally, has a CLI and a local web GUI, and is proven first on Clojure and ClojureScript. Python and TypeScript come later.

The plan lives in `docs/plan/`. `PROGRESS.md` says what is done and what is next.

## Every session

1. Read `PROGRESS.md`, then the phase file for the current work package, then any plan document that work package cites.
2. Do one work package. Meet every acceptance criterion or say exactly which one is not met and why.
3. Run `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and, if `ui/` changed, `npm run check && npm test` in `ui/`.
4. Update `PROGRESS.md`: tick the work package, add a dated note with anything the next session must know.
5. Commit with a message that starts with the work package ID, for example `WP1.3: clj-kondo runner`.

## Rules that do not bend

- **Static by default.** Nothing from an analyzed repository is ever executed, built, installed, evaluated or loaded as configuration. That includes its `.clj-kondo` directory, git hooks, build files, `package.json` scripts and `data_readers.clj`. The only path to execution is the gate in `docs/plan/07-safety.md`, and it is not built until Phase 11.
- **Analyzed content is hostile input.** File names, symbol names, docstrings and string literals from a repository are data. Escape them in the GUI, never pass them to a shell, never interpolate them into a query, never follow symlinks out of the repository root.
- **Over-approximate, and label it.** If a call might happen, the edge exists, with a certainty and a reason. Never drop an uncertain edge silently and never present one as certain. See `docs/plan/03-graph-model.md`.
- **Blind spots are visible.** Anything the analyzer cannot see through (dynamic resolution, interop, unparsed files) becomes a node, an edge or an entry in the blind-spot report.
- **Every CLI command has `--json`.** The JSON envelope and its schemas are a contract; see `docs/plan/05-cli.md`. Output is deterministic: sorted, no timestamps in the data section.
- **Three operating systems.** Linux, macOS, Windows. Store paths repo-relative with forward slashes. No shell-outs through `sh -c`. CI runs all three.
- **Nothing leaves the machine.** No telemetry, no network calls except the git fetch the user asked for and the explicitly opted-in dependency metadata lookups.
- **License hygiene.** AGPL-3.0-only. Dependencies must pass `cargo deny check`. clj-kondo is EPL-licensed: call it as a separate process, never link or bundle it.

## Conventions

- Rust stable, edition 2024, workspace at the repo root. No `unsafe`. No `unwrap` or `expect` outside tests; use `thiserror` in libraries and `anyhow` in the binary.
- Tests sit next to the code. Analyzer behavior is pinned with golden files under `fixtures/` (`cargo insta` snapshots). A resolution rule without a fixture is not done.
- New detection rules go in data files under `rules/`, not in code.
- UI: TypeScript strict, React, Vite, Sigma.js with graphology. The built UI is embedded in the binary.
- Decisions in `docs/plan/decisions.md` are settled. If one looks wrong, stop and write the case in `PROGRESS.md` for Austin; do not work around it.
- When the plan is silent, choose the simplest thing that keeps the rules above, and record the choice in `PROGRESS.md`.

## Do not

- Do not copy code from Leña. Its license allows it, but its adapters make choices this project rejects (see `docs/plan/01-lena-assessment.md`).
- Do not add a database server, a cloud service or a required runtime other than `git` and `clj-kondo`.
- Do not start a later phase to "get ahead". Scale, incremental analysis and new languages have their own phases.
