# Phase 1. First proof

Goal (Milestone M1): `aldente serve <a real Clojure or ClojureScript repo>` shows its namespace graph, and clicking through reaches symbol-level invocations. A thin slice through every layer; resolution subtleties wait for Phase 2.

Read first: `02-architecture.md`, `03-graph-model.md`, `04-clojure-analysis.md` (R1 to R4), `05-cli.md` (core), `06-gui.md`, `07-safety.md`.

## WP1.1 Open a local target, walk, discover units

**Build** (`aldente-acquire`)
- Resolve a path to its repository root (nearest `.git`; otherwise the path itself, flagged `no-git`).
- Walker using the `ignore` crate: gitignore-aware, no symlink following, deny-listed directories, size cap, binary sniffing; every skipped source-like file becomes a diagnostic.
- Unit discovery per `02-architecture.md`: manifests, nearest-enclosing ownership, source and test roots read from `deps.edn`, `project.clj`, `shadow-cljs.edn`, `bb.edn` as data with the in-crate EDN reader. Aliases' `:extra-paths` are included and tagged.
- Content hash per file (`blake3`) and the snapshot key.
- `aldente units`.

**Accept**
- On `fixtures/clj-mini`: one unit, correct roots, test root tagged.
- A monorepo fixture (`fixtures/mono-mini/`: three units, two sharing a namespace name, one `:local/root` dependency) yields three units with correct file ownership.
- Hostile cases 2, 3 and 8 pass: the symlink is skipped and reported, odd file names survive round-trip, the huge file is skipped with a diagnostic.
- The key is identical across runs and across operating systems for the same content (line endings are hashed as stored, so pin `.gitattributes` in fixtures).

## WP1.2 Safe remote acquisition

**Build**
- URL validation and the hardened clone from `07-safety.md`; clones under the cache; `--ref`.
- Re-use an existing clone by fetching the requested ref with the same hardening.
- `aldente cache ls|gc|clear`.

**Accept**
- Tests clone from a local bare repository created in a temp directory (no network in tests) using an `https`-shaped code path with the transport allow-list relaxed only inside the test harness.
- Hostile case 6: a repository with a `post-checkout` hook in a template, a `.gitattributes` filter and a `.gitmodules` entry is cloned; a marker file proves no hook or filter ran; the submodule is not fetched and is reported.
- URLs starting with `-`, `file://`, and `ext::` are rejected before any process starts.

## WP1.3 clj-kondo runner

**Build** (`aldente-clj`)
- Start with the probe test described in `04-clojure-analysis.md`: run the pinned clj-kondo on a tiny file and assert every analysis bucket the plan relies on is present; fix option names if any differ and note it in `PROGRESS.md`.
- The runner: argument vector, empty `--config-dir`, cache off, timeout, working directory in temp, one invocation per unit over its roots, JSON parsed with `serde` into typed structs that tolerate unknown fields.
- Paths in the output normalized to repo-relative forward slashes.

**Accept**
- Hostile case 1: a fixture with `.clj-kondo/config.edn` and a hook that would be visible in the output (for example one that rewrites a call) has no effect; assert the analysis is identical to the same fixture without the directory.
- Non-zero "findings" exit codes are treated as success; a missing binary yields exit code 5 with the doctor hint.
- A unit with a syntactically broken file still returns analysis for the other files and a diagnostic for the broken one.

## WP1.4 Form index

**Build**
- Tree-sitter pass per `04-clojure-analysis.md`: top-level form ranges and heads, `defmethod` and protocol-implementation body ranges, call-position lists with argument ranges, literal harvest, reader-conditional branches.
- Runs in parallel across files with `rayon`.

**Accept**
- Snapshot tests on `fixtures/clj-mini` for the index of three representative files.
- A truncated file (unbalanced parentheses) yields the forms before the break plus a diagnostic.
- Hostile case 8's deeply nested form does not overflow the stack (iterative traversal).

## WP1.5 Graph build v1

**Build** (`aldente-adapter`, `aldente-clj`, `aldente-graph`)
- The `Adapter` trait and `FactBatch`.
- Rules R1 (definitions with subkinds and flags), R2 (namespaces and `requires`), R3 (enclosing scope for every usage, including `defmethod` bodies and top-level forms), R4 (calls versus references, both with certainty and reason set).
- Externals as `external` nodes; no library attribution yet.
- `.cljc`: merge by namespace and name with `langs` (the rest of R12 is Phase 2).
- Namespace-level and unit-level rolled-up edges.

**Accept**
- Golden graph for `fixtures/clj-mini` at symbol and namespace levels, reviewed against `EXPECTED.md` for the rows marked "Phase 1".
- No usage in clj-kondo's output is dropped: a test counts usages in and edge sites out and requires equality after documented de-duplication.
- Usages inside each `defmethod` body are attributed to that method's impl node.
- In `fixtures/mono-mini`, the two same-named namespaces are distinct nodes.

## WP1.6 Snapshot store

**Build**
- SQLite schema per `02-architecture.md`, written in one transaction, with schema version in `meta`.
- Loader into the in-memory forward and reverse adjacency.
- Cache hit and miss logic; `--no-cache`.

**Accept**
- Write then load round-trips the `clj-mini` graph exactly.
- A second `analyze` on an unchanged tree does no analysis work (assert through a counter) and returns in well under a second.
- Changing one file changes the key.
- A snapshot with an older schema version is ignored and rebuilt, not crashed on.

## WP1.7 Core CLI

**Build**
- `analyze`, `find`, `show`, `graph` (JSON, DOT, Mermaid, GraphML; `--level`, `--focus`, `--depth`, `--direction`), `blindspots` (lists diagnostics for now), `schema` for each data shape.
- Symbol short-form resolution with ambiguity handling (exit code 3 and candidates).
- Text and JSON renderers; terminal sanitization of repository strings.
- Start `docs/cli.md`.

**Accept**
- For each command: a snapshot test of text output and a test that JSON output validates against its schema.
- Byte-identical JSON on repeated runs.
- `aldente graph --level namespace --format dot | dot -Tsvg` works on the fixture (run in CI on Linux only).
- `aldente show charge!` in `mono-mini`, where two units define it, exits 3 and lists both.

## WP1.8 Server and API v1

**Build** (`aldente-server`)
- `aldente serve`: loopback bind, free port by default, token, `Host` and `Origin` checks, security headers, embedded assets, opens the browser unless `--no-open`.
- Endpoints: `/snapshot`, `/graph`, `/neighborhood`, `/expand`, `/node`, `/source`, `/search`. Payload cap enforced with aggregation and the `truncated` flag.

**Accept**
- Hostile case 9: wrong `Host`, missing token and foreign `Origin` are each rejected.
- `/source` cannot be made to return a file that is not in the snapshot (test with an ID crafted to look like a path).
- A handler panic returns 500 and the server keeps serving.
- API responses validate against the same schemas as the CLI.

## WP1.9 GUI v1

**Build** (`ui/`)
- Project setup: Vite, React, TypeScript strict, ESLint, Vitest, Playwright; build output embedded by `aldente-server`; `cargo build` works without Node when a prebuilt `ui/dist` is present, and CI builds it.
- Graph view: namespace-level graph on load (unit level when there is more than one unit), Sigma.js rendering, force layout in a worker.
- Click to select with a details panel (kind, location, doc, signature, callers, callees as links); double-click a namespace to expand its symbols; double-click a symbol to load its neighborhood; search box; hide tests and externals toggles; truncated notice.
- Source panel showing the definition text.

**Accept**
- Playwright test against the fixture: load, expand a namespace, select a symbol, follow a callee link, see its source.
- Hostile case 4: a docstring with `<script>` and an `onerror` attribute renders as text; the test asserts no script ran.
- No request leaves `127.0.0.1` (asserted by Playwright request interception).

## WP1.10 Milestone M1 acceptance

**Build**
- `acceptance/repos.toml` and the runner; stand-in repositories pinned by commit (see `open-questions.md`), or Austin's if supplied.
- Run `analyze` and `serve` on each. Fix what breaks. Record timings and counts in `PROGRESS.md`.

**Accept**
- On at least one Clojure and one ClojureScript repository: analysis completes, at least 95 percent of source files parse, expected facts in `repos.toml` hold, and the GUI shows the namespace graph and drills to symbols.
- A screenshot of each and the `analyze --json` summary are saved under `docs/milestones/m1/`.
- **Stop. Show Austin before starting Phase 2.**
