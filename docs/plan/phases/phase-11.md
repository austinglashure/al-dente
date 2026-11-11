# Phase 11. Execution, Emacs, release

Goal (M11): the one feature that runs code is safely gated; Emacs has a thin front end; the project can be installed and, if Austin chooses, published.

## WP11.1 Execution gate

**Build**
- `ExecutionGate::authorize` exactly as specified in `07-safety.md`: `--allow-exec` required, extra `--i-trust-this-repo` and a printed summary for remote-origin snapshots, refusal without a terminal unless `--yes`, scrubbed environment allow list, exit code 6 on refusal.
- A workspace-level lint or test that fails if any crate other than the gate module spawns a process with a command that is not `git`, `clj-kondo` or the user-configured editor.

**Accept**
- Tests for every refusal path.
- The spawn-audit test passes and fails when a deliberate violation is introduced.
- The summary shows unresolved vet findings for the snapshot.

## WP11.2 Measured coverage

**Build**
- `aldente coverage measure --allow-exec`: runs a coverage command (default for Clojure CLI projects: Cloverage through an injected alias written to a temp `deps.edn` overlay, not into the repository; `--cmd` to override), collects the report, imports it through the Phase 4 importer.
- Documentation recommending a container, with a ready `docker run` example that mounts the repository read-only and the output directory read-write.
- ClojureScript and later-language runners per Austin's answer in `open-questions.md`.

**Accept**
- End-to-end test on `fixtures/clj-mini` in CI on Linux only (needs a JVM and Clojure CLI): measured coverage appears per symbol, and the estimate-versus-measured disagreement list is produced.
- Nothing is written inside the analyzed repository.

## WP11.3 Emacs package

**Build** (`emacs/aldente.el`)
- A thin layer over `aldente --json`: `aldente-show-at-point`, `aldente-callers`, `aldente-callees`, `aldente-crater-forward`, `aldente-crater-reverse` (results in a `compilation-mode`-style buffer so `next-error` walks the checklist), `aldente-crater-from-trace` (region or buffer), `aldente-lib-usage`, `aldente-serve` (opens the GUI focused on the symbol at point), `aldente-hotspots`.
- Symbol at point resolved for Clojure buffers via namespace form plus symbol; falls back to prompting with `completing-read` over `aldente find`.
- No dependency beyond built-in Emacs libraries and an optional `transient` menu.

**Accept**
- ERT tests with a stubbed `aldente` binary returning recorded JSON.
- Byte-compiles without warnings on Emacs 29 and later.
- `aldente serve --focus <id>` exists and opens the GUI on that node.

## WP11.4 Release engineering

**Build**
- Release workflow: binaries for Linux (x86_64 and aarch64, musl), macOS (both architectures), Windows (x86_64), with the UI embedded; checksums; `cargo install aldente-cli` works.
- `aldente doctor` covers first-run problems on each system.
- Shell completions and a man page generated from `clap`.
- Versioning: semantic versions for the binary; independent `schema` versions for JSON shapes with a compatibility note in `docs/cli.md`.

**Accept**
- A tagged pre-release produces all artifacts; each is smoke-tested in CI (`aldente analyze fixtures/clj-mini`).
- Third-party license notices generated (`cargo about`) and shipped.

## WP11.5 Documentation and open-source readiness

**Build**
- User docs: quick start, the CLI reference, the certainty model explained with pictures, the safety model, how to write rule catalogs, limitations.
- `CONTRIBUTING.md`, issue templates, `SECURITY.md`.
- A pass over the repository for anything Austin-specific (paths, private repository names in acceptance files moved to an untracked local file).
- README acknowledgement of Leña and clj-kondo.

**Accept**
- A person who has never seen the project can install it and reach a graph of a public repository by following the quick start, tested by a fresh clone in CI.
- `git grep` for private names returns nothing.
