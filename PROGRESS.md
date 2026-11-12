# Progress

Update at the end of every session: tick finished work packages, add a dated note. Notes are for the next session: what was decided, what surprised you, what is half done.

**Current:** Phase 2, WP2.4 (not started). Phase 0, Phase 1 and WP2.1–WP2.3 are done locally. "CI green on all three systems" is unverified until the repo is pushed to GitHub.

## Phase 0. Foundations
- [x] WP0.1 Workspace, license, CI
- [x] WP0.2 Model crate
- [x] WP0.3 Fixture corpus and golden harness
- [x] WP0.4 Doctor

## Phase 1. First proof
- [x] WP1.1 Local target, walk, units
- [x] WP1.2 Safe remote acquisition
- [x] WP1.3 clj-kondo runner
- [x] WP1.4 Form index
- [x] WP1.5 Graph build v1
- [x] WP1.6 Snapshot store
- [x] WP1.7 Core CLI
- [x] WP1.8 Server and API v1
- [x] WP1.9 GUI v1
- [x] WP1.10 **Milestone M1** (stop and show Austin)

## Phase 2. Call resolution
- [x] WP2.1 Certainty everywhere
- [x] WP2.2 Multimethods
- [x] WP2.3 Protocols, records, types, reify
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

- **Libraries to confirm** (added while you were away; each named in the plan by purpose, not by package): `tree-sitter-clojure-orchard` for the Clojure grammar (WP1.4, needs `CC0-1.0` in deny.toml); `toml` for `acceptance/repos.toml` (WP1.10; rule catalogs will need it too). Not added: `regex` for `find --regex` (WP1.7), waiting for a yes.
- **Acceptance repositories:** M1 used the plan's stand-ins ring, reitit and re-frame. Swap in your own repositories (and their facts) in `acceptance/repos.toml` whenever you like; penpot and metabase are not run yet (large).

## Notes

(Newest first. `YYYY-MM-DD WPx.y: note`.)

- 2026-11-12 WP2.3: R6 per-caller protocol `dispatches` (`resolve::protocol_dispatch`), `reify` bodies as impls (`<method>@reify:L<line>`), `implements` edges from every protocol impl to its protocol and, missed in WP2.2, from every `defmethod` impl to its multimethod (externals when defined elsewhere). Fixture `fixtures/clj-rules/protocols/` (defrecord, deftype, extend-type, extend-protocol over `nil`, `Object`, `java.util.Map`, reify) with `EXPECTED.md`, checked exactly; `show` on a protocol method lists its seven impls (via `children`). Receiver rule: a constructor call (`->Rec`, `map->Rec`, `Rec.`) as the first argument, or a local bound to one by the innermost enclosing `let`-style form, selects that type's impl `likely`; otherwise every impl `possible`. Not done: type hints (`^Card x`) as receiver evidence (R6 mentions them); a Java-class constructor never matches an interface impl (`(HashMap.)` vs `java.util.Map`). Found while testing, fixed test first: `(extend-protocol P nil …)` lost the `nil` type (tree-sitter parses it as a nil literal). clj-mini has its WP2.3 rows. Adapter VERSION 6.
- 2026-11-12 WP2.2: R5 per-caller `dispatches` edges (`resolve::multimethod_dispatch`); the form index now records each call argument's kind, text and literal-map entries (`CallSite::arg_info`). Fixture `fixtures/clj-rules/multimethods/` with a hand-written `EXPECTED.md` table, checked exactly by `testkit/tests/rules.rs` (keyword dispatch with literal maps, `(fn [m] (:lang m))`, arbitrary dispatch, `:default`, a `defmethod` in another namespace, `derive`, value usage); a forward walk from a caller reaches calls inside the method bodies. clj-mini now has its WP2.2 rows (8 `dispatches` edges). Choices where the plan was silent: a literal that matches no impl links `:default` as `likely` (it is what runs), or every impl `possible` without a default; a literal with a `derive` parent anywhere in the unit links every impl `possible`; matching compares dispatch values as written (`:a/b` vs `::b` do not match; the resolver does not expand `::`); defmethods in other units are not linked (cross-unit dispatch waits for a real case). Adapter VERSION 5.
- 2026-11-12 WP2.1: `--min-certainty` on `graph` and `show` (CLI) and `min_certainty` on `/graph`, `/neighborhood`, `/expand`, `/node`; `analyze` reports `edge_certainty` (edges by kind and certainty). GUI: certain/likely/possible toggles with counts in a legend; hidden levels are left out of the force layout. Accept: `--min-certainty certain` on clj-mini removes exactly the likely and possible edges (CLI test); Playwright unchecks `possible` and the drawn-edge count drops by exactly the possible count. **Deviation:** Sigma 3 has no dashed or dotted edge program, so certainty is shown by color and width (dark thick, blue medium, light thin) instead of line style; a custom WebGL edge program would add dotted lines later. `find` has no `--min-certainty`: it returns nodes, not edges. Bug found by the e2e test and fixed: toggling a level changed a callback's identity and re-ran the initial-load effect, reloading the repository view; levels now live in a ref.
- 2026-11-12 GUI quality of life (Austin's request; Phase 1 accepted): deselect with Escape, the × in the details header, or a click on empty canvas; hide either side panel with the toggles at the right of the breadcrumb bar; resize them by dragging the border (arrow keys when it is focused, Shift for bigger steps), 180–720 px; sizes and visibility are kept in localStorage (wrapped in try/catch, defaults if unavailable). The canvas follows its container's size (ResizeObserver). Tests first: e2e for deselect and for hide/resize/reload; a unit test for parsing saved sizes.
- 2026-11-12 Phase 1 follow-up (Austin's review of the GUI): clicking a unit or namespace showed empty callers/callees, and "unit" was unexplained. Now: `show` and `/node` give namespaces and units rolled-up "used by"/"depends on" with counts and a `children` list (`ShowResult.children`, `Neighbor.count`); a unit's panel shows its directory, build files, source files, languages, roots and service flag; unit labels are their last directory (`(root)` for `.`). The GUI opens a unit or namespace on its own canvas (double-click, the list's "open", or the Open tool), with a breadcrumb back (`Repository › reitit-core › reitit.impl`); opening a namespace keeps the namespaces it talks to as context. Tools per selection: Open, Expand here (aggregates); Show neighbors, Focus (symbols); Copy ID. A hint explains units when nothing is selected. Callers and callees differing only by language are merged into one entry (the full R12 merge is still Phase 2). Tests first: query tests for children, rolled neighbors and language merging; e2e tests for opening a namespace and a unit (mono-mini) and the hint; `show` snapshots for a namespace and a unit. Milestone screenshots regenerated.
- 2026-11-12 WP1.10: `acceptance/repos.toml` (ring, reitit, re-frame, pinned commits, 15 facts read from source), the runner `cargo run -p acceptance`, and `ui/scripts/milestone.mjs` for screenshots. All facts hold; parse rate 100% on all three; release-build analysis 1.1 s (ring, 72 files), 3.7 s (reitit, 131 files), 1.7 s (re-frame, 70 files). Evidence in `docs/milestones/m1/` (summary README, `analyze --json`, runner reports, overview and drill-down screenshots). The screenshots exposed a UI bug, fixed test first: the "showing the unit level to stay within the size limit" notice appeared whenever the payload was aggregated, including the normal multi-unit default; `query::fit` now reports a real fallback, `/graph` sends a `coarser` warning, and the UI shows the notice only then (e2e asserts no notice on clj-mini). Seen but not fixed (Phase 2, R12): a `.cljc` call appears twice in the callee list (clj and cljs edges), without saying which language. Debt: the runner duplicates the CLI's clj-kondo and cache setup (`commands/loaded.rs`); a shared library function would remove that.
- 2026-11-12 WP1.9: `ui/` (Vite 8, React 19, TypeScript 5.9 strict, ESLint 10 + typescript-eslint, Vitest 5, Playwright 1.65, Sigma.js 3.0.3, graphology 0.26). Versions: TypeScript 5.9 rather than 7 because typescript-eslint supports up to 6.0; Sigma 3 rather than the new 4 to stay on a known API. Accept criteria met: Playwright on clj-mini loads, expands `shop.core`, selects `checkout`, follows the callee `shop.pricing/price` and sees `(defmulti price` in the source panel; hostile case 4 (`fixtures/hostile/xss/`): the docstring shows as text, `window.__pwned` stays unset, no `<img>`/`<script>` in the panel, no request for `/x`; every request is checked to go to 127.0.0.1 (others are aborted and would fail the test). Choices: `ui/dist` is committed so `cargo build` needs no Node; CI (Linux) runs `npm run check`, `npm test`, `npm run build` and fails if `dist` differs, then the e2e tests. The force layout runs in our own worker file (graphology's blob worker is blocked by `default-src 'self'`). A node list beside the canvas (select, expand/collapse, neighbors) gives keyboard access and stable test targets; WebGL canvas clicks are not tested. The token is moved from the URL to sessionStorage on load. Externals are hidden by default. ESLint forbids `dangerouslySetInnerHTML` and `innerHTML`. Playwright uses the full Chromium (`channel: 'chromium'`) since only that build downloaded here. Not built: paged clusters ("show more"), undo/redo, the context menu, keyboard navigation along edges, possible-edge styling beyond color; these are later GUI work. The unit tests in `src/model.test.ts` were written with the code, not before.
- 2026-11-11 WP1.8: `aldente-server` (axum, tokio, rust-embed; all in the plan) and `aldente serve`. Endpoints `/snapshot` (new schema `snapshot`), `/graph` (fits the payload cap by falling back to a coarser level, else truncating by degree), `/neighborhood`, `/expand` (children best-connected first, at most 200; edges to visible nodes, rolled up to a visible namespace or unit), `/node`, `/source`, `/search`; responses use the CLI envelope and schemas (tested). Accept criteria met: hostile case 9 (wrong Host 403, missing or wrong token 401, foreign Origin 403, over a real socket); `/source` serves only files recorded in the snapshot, by node ID, and only if their hash is unchanged (crafted path-like IDs give 404); a handler panic is a 500 and the server keeps serving (handlers run in their own task). Choices: the token check covers `/api/*` only, since the browser must load the UI before it can send a header (Host and Origin are checked everywhere); bind is 127.0.0.1 only, not ::1; the token is 32 hex digits from std's OS-seeded `RandomState` keys hashed with blake3 (no `rand` crate); the browser opener is `xdg-open`/`open`/`rundll32 url.dll,FileProtocolHandler` as argument vectors; snapshots now store their analyzed files (schema version 2, so older snapshots rebuild); `ui/dist/index.html` is a placeholder until WP1.9. Not built: long-running jobs (`202` + `/job`) and the later-phase endpoints.
- 2026-11-11 WP1.7: `find`, `show`, `graph` (text, JSON, DOT, Mermaid, GraphML; level, focus, depth, direction, payload cap with `--limit 0`), `blindspots`; schemas `find`, `show`, `graph`, `blindspots`; shared lookups in `aldente-graph::query` (short-form `resolve`, `search`, `neighbors`, `neighborhood`) for reuse by the server. Accept criteria: every command has a text snapshot (`crates/aldente-cli/tests/snapshots/`, each reviewed) and a JSON schema check; repeated JSON is byte-identical (find, graph, analyze); `graph --format dot | dot -Tsvg` works locally and is a Linux CI step; `show charge!` in mono-mini exits 3 and lists both (`charge!` added to `orders.core` and `billing.core`, goldens updated: one more call each). Choices: `TARGET` is the last positional (`find <pattern> [TARGET]`); a short form must start at a whole ID component (`billing.core/charge!` yes, `core/charge!` no); no match is also exit 3 (`unknown-symbol`); `graph` defaults to namespace level, depth 1, both directions; a neighborhood walks every edge kind except `contains`; `blindspots` lists diagnostics too. **Deferred:** `find --regex` needs the `regex` crate, which the plan does not name; asking before adding it. Found while reviewing snapshots, fixed test first: gensyms and special forms inside a macro's syntax-quoted template were unresolved-symbol blind spots; now the `macro-template` suppression (adapter VERSION 4).
- 2026-11-11 WP1.6: `aldente-graph::{snapshot, adjacency}`, `pipeline::analyze_cached`, `acquire::local_git::head_rev`, `aldente analyze`, `cache gc` prunes snapshots (newest 5 per repository). Accept criteria all met: clj-mini round trip (`testkit/tests/snapshot.rs`), cache hit runs no adapter (counting adapter) and takes 0.09 s on reitit, one changed file changes the key, an older schema version is ignored and rebuilt. Also closed WP1.3's leftover: no usable clj-kondo exits 5 with the doctor hint. **Decision: workspace `rust-version` raised from 1.85 to 1.89** so `rusqlite` 0.42 can bundle SQLite 3.54, which fixes RUSTSEC-2026-0459 (`cargo deny` failed on 0.40); the bump also enabled clippy's let-chain and `as_chunks` lints, applied with `clippy --fix`. Choices: nodes are stored as JSON blobs with indexed `node_id`/`kind`, edges as columns with integer node references (traversal-friendly for Phase 9); `analysis_result` and `file` tables exist but are unused; a corrupt snapshot counts as a miss; `dirty` is always false: `git status`/`diff` can run clean filters from a hostile `.git/config`, and the key already covers the working tree. Found on reitit and fixed test first: `%` in `#(…)` gives local usages without `id`/`name`; `cond->>` gives var usages without position (now edges without sites, from `from-var`, counted as `positionless`); clj-kondo records are now parsed one by one, so an odd record is skipped and reported instead of failing the unit. reitit: 37 units, 130 namespaces, about 7,900 calls, 10 s cold (debug build). **Debt:** the key does not change when Al Dente's code changes; `aldente-clj::adapter::VERSION` must be bumped whenever resolver output changes (bumped to 3 today). Consider a build ID in the key for dev builds.
- 2026-11-11 WP1.5: `aldente-adapter` (trait, `FactBatch`, `AnalyzeCtx`), `aldente-clj::{resolve, adapter}` (R1–R4), `aldente-graph::{build, rollup, pipeline}`; testkit's `run_pipeline` now runs the real pipeline (needs clj-kondo on PATH). Goldens: clj-mini symbol and namespace, mono-mini namespace and unit, reviewed row by row against EXPECTED.md; the review found three bugs (refers, reader macros, `#(…)` call sites), fixed test first. Model additions: `Edge::{count, by_certainty}` for rolled-up edges, `BlindSpot`, `SymbolFlag::Service` (unit nodes), `Ord` for `NodeId` (by string form). Choices where the plan was silent:
  - All Clojure-family IDs use the `clj` prefix (`clj:.//shop.ui.views/init` for a cljs namespace), as EXPECTED.md already assumed.
  - Cross-unit linking happens in the build: adapters point at `ext:clj//ns/name` for namespaces their unit does not define; the build re-targets to the unit that defines it. Several candidate units: an edge to each, `possible`, `name-match`.
  - A usage naming a var with no definition (macro-made) gets a bare `var` node without location plus an `unresolved-symbol` blind spot, so the edge is kept.
  - Scope order (R3): defmethod body, protocol impl body (not reify in Phase 1), clj-kondo `from-var`, innermost var definition range, else `<top>@L<line>`. Impl discriminators: dispatch value as written, the type name as written, `L<line>` for top-level forms.
  - Symbol level = every stored node and edge. Namespace level = namespaces, namespace-level externals, platforms; `requires` kept, invocation edges rolled up per kind, self-loops dropped. Unit level leaves externals out until Phase 2 attaches libraries. A rolled edge's `reason` is its members' most frequent one, so it still validates.
  - Adapter failures other than a missing tool give an `analyzer-failed` diagnostic and `unparsed-file` blind spots for the unit's files; a missing tool stops the run.
  Debt: `index_files` reads with `read_inside` directly rather than through `AnalyzeCtx` (the files come from `ctx.files`, so the no-escape rule holds, but the contract says ctx hands out contents); the testkit duplicates the CLI's clj-kondo setup.
- 2026-11-11 WP1.4: `aldente-clj::forms` (`index_source`, `index_files` with rayon). **Library choice for Austin to check:** the crate literally named `tree-sitter-clojure` (0.1.0) is a vendored copy kept for Harper; I used `tree-sitter-clojure-orchard` 0.2.9, the maintained fork of the same sogaiu grammar, which needs `CC0-1.0` in `deny.toml` (public domain, AGPL-compatible). New deps: `tree-sitter` 0.26, `tree-sitter-clojure-orchard`, `rayon`, all named in the plan except the orchard package name. Choices: columns count characters (matches clj-kondo); discarded forms (`#_`), comments and metadata are skipped by the walk; every list with a symbol or keyword head is a call candidate (method bodies and `ns` clauses included; the resolver decides); impl bodies are collected anywhere in the file, so nested `reify` works; literals are capped at 1,000 characters. Snapshots for `pricing.clj`, `payment.clj`, `model.cljc` reviewed line by line against the source. `.edn` files are not indexed yet (the walker does not keep them; Phase 6/8 need them for literals).
- 2026-11-11 WP1.3: `aldente-clj::kondo` (`Kondo::analyze`, typed buckets, findings to diagnostics). Probe: every bucket the plan names is present in clj-kondo 2026.10.26 with the plan's option names; nothing renamed. Choices: (a) clj-kondo gets the walker's file list, chunked to about 8,000 characters per invocation, never whole roots (roots could reach symlinks, ignored or oversized files); (b) `XDG_CONFIG_HOME` points at the empty dir too, so the user's home clj-kondo config cannot change results; (c) every bucket is sorted after parsing, since `--parallel` order varies; (d) syntax and file errors become one `parse-error`/`unreadable` diagnostic per file; other findings are ignored. Found: JS interop usages (`js/console.log`) have no `to`, so `VarUsage::to` is optional. The EXPECTED.md "verify" rows that depend on clj-kondo are now confirmed (records and constructors are definitions; `let`, `fn`, `deref` usages reported). Not met yet: "missing binary yields exit 5 with the doctor hint" is tested at library level (`KondoError::Missing`); the CLI mapping lands with `analyze` in WP1.5. Not done: a memory ceiling per unit (no portable way to cap a native child's memory from std); timeout is 300 s per unit. Debt: the unit test helpers here duplicate testkit's, because testkit will depend on this crate.
- 2026-11-11 WP1.2: `acquire::remote` (URL validation, `Git` hardened command builder, `acquire`), `acquire::cache::Layout` (`ls`/`gc`/`clear`), `aldente cache`, URL targets and `--ref` via shared `commands/target.rs`. Choices: (a) one code path for first clone, re-use and `--ref`: `init --template=<empty>` once, then `fetch --depth 1 --no-tags -- <url> <ref|HEAD>` and `checkout --detach --force FETCH_HEAD`; a commit id works as a ref. (b) Every inherited `GIT_*` variable is removed before the hardening env is set. (c) `$ALDENTE_CACHE_DIR` overrides the cache location (tests use it; plan's config file comes later). (d) Snapshot files will be `snapshots/<hex>.db`: Windows forbids `:` in names, so the `b3:` prefix is not in the file name. (e) URLs with any userinfo on https (tokens too) are refused; ssh users are allowed. A host with a port becomes `<host>_<port>` in the cache path. (f) `gc` only empties `tmp/` until snapshots exist (WP1.6 adds pruning). (g) Fetch and clone failures exit 1 with git's stderr; no exit code fits network errors. Hostile case 6 lives in `remote.rs` unit tests because the `file` transport relaxation is a private, test-only field. The clone tests were written right after the clone code, not before; the URL and CLI tests were red first. Tried: `aldente units https://github.com/metosin/reitit` (31 s, mostly network) and `--ref 0.7.0`; the clone has no `.git/hooks`. Debt: Windows reserved names (`con`, `nul`) as repo names would break the cache path.
- 2026-11-11 WP1.1 follow-up: tried on metosin/reitit (37 units, 0.06 s; all 170 Clojure files accounted for: 131 analyzed, 37 `project.clj`, 2 `scripts/` outside roots). Fixed, test first: lein-cljsbuild `:cljsbuild :builds … :source-paths` are now roots (alias `cljsbuild`); shadow-cljs test targets (`:node-test`, `:browser-test`, `:karma`) no longer mark a service; a root is `test` when any path component is `test`/`tests`. Leiningen `checkouts/` symlinks are reported and skipped, as intended. Not handled yet: figwheel-main `*.cljs.edn` build files.
- 2026-11-11 WP1.1: `aldente units [TARGET]`. In `aldente-acquire`: `target` (nearest `.git`, else the directory, `git: false`), `walk` (ignore crate), `edn` (in-crate reader; `#=` kept as `ReadEval`, metadata dropped, nesting capped at 256), `manifest`, `units`, `key`. Model: `Diagnostic`/`DiagnosticCode`, `UnitsReport` (schema `units`), `RepoPath::{join_relative, starts_with, parent, components}`. New deps `ignore` 0.4 and `blake3` 1, both in `02-architecture.md`. New fixtures `mono-mini/` and `hostile/read-eval/`. Choices where the plan was silent:
  - Only the four Clojure manifests make units; `package.json`/`pyproject.toml` wait for Phase 10.
  - An implicit root unit `unit:.` (no manifest) exists only when some source file has no enclosing unit, so `mono-mini` stays at three.
  - Fallback roots: `src` and `test` if present, else the unit directory. Tool defaults (deps.edn `src`, Leiningen `src`/`test`) are dropped silently when absent; explicitly declared but absent roots give `missing-root`.
  - A root is `test` when its alias/profile name contains "test", it comes from Leiningen `:test-paths`, or its last directory is `test`/`tests`.
  - Service flag: Leiningen `:main`, a shadow-cljs build with `:target`, or a `Dockerfile` in the unit directory. deps.edn never sets it (`-m` aliases are mostly test runners and tools); `-main` detection (R13) adds it in WP1.5+.
  - Source files inside a unit but under none of its roots (e.g. `build.clj`) get `outside-roots` and are not analyzed.
  - The walk honors only the repository's own `.gitignore` files (no global gitignore, no `.git/info/exclude`, no `.ignore`), for machine-independent results. Run caps: 200,000 files, 1 GiB.
  - Snapshot key: inputs plus `(path, hash)` of analyzed sources and manifests, length-prefixed, `b3:` + hex. Pinned in tests (`key.rs`, and over clj-mini in `tests/units.rs`). Not printed anywhere until snapshots exist.
  - A target inside a git repository analyzes the whole repository.
  - Hostile cases 2, 3, 8 are generated at test time; 2 is Unix-only, 3 uses fewer names on Windows. 6a moves to the first WP that runs local git.
  Not verified: macOS and Windows (the pinned clj-mini key is the cross-OS check). Debt: (a) a tracked file that matches `.gitignore` (force-added) is skipped without a diagnostic; fix once local `git ls-files` exists; (b) unit ownership is files × units, fine now, revisit in Phase 9; (c) parsing project.clj ignores `#=`/unquote, and vet should report them (Phase 5).
- 2026-11-11 WP0.4: `aldente doctor` in `aldente-cli`; logic in `aldente-acquire` (`process::run_captured` with timeout and temp-dir cwd, `tools`, `cache`); report types in `aldente-model::doctor`, schema `doctor`. New deps `which` 8 and `dirs` 7, both named in `02-architecture.md`. Choices where the plan was silent: (a) a too-old tool, an unrecognized `--version` output and an unwritable cache also exit 5, since none of the other codes fit; (b) with `--json`, a failing doctor still carries the full report in `data` next to the `tool-missing` error; (c) doctor's `data` holds absolute paths (tool and cache locations), an exception to the no-absolute-paths rule, which is about snapshot data; (d) tools are looked up only in `PATH`'s directories, never the current directory, and run with the temp dir as cwd, so a `git.exe` inside an analyzed repo is never picked up on Windows. `clj-kondo` minimum is `tools::CLJ_KONDO` (2026.10.26, latest release today); bump it with the CI pin. Not verified locally: the Windows `.cmd` stubs in `tests/doctor.rs` and the new CI `doctor` step (accept criterion "on Windows CI, doctor finds clj-kondo.exe"). The process timeout test is Unix-only.
- 2026-11-11 (Austin's answers, between WP0.3 and WP0.4): (1) `dispatches` edges start at the calling scope with per-caller certainty; no global multimethod/protocol-method → impl edge. A value usage of a multimethod gives `possible` dispatches to every impl; `:default` is `possible` when a literal selected another impl (my choice, the plan was silent). (2) New reasons `macro-body` (possible) and `dynamic` (certain, edges into holes) in `aldente-model`. Updated `03-graph-model.md`, `04-clojure-analysis.md` R4/R5/R6/R8 and `fixtures/clj-mini/EXPECTED.md`.
- 2026-11-11 WP0.3: `fixtures/clj-mini/` has 13 source files plus `deps.edn` and `EXPECTED.md`, covering every item WP0.3 lists. `clj-kondo --lint` gives 0 errors and 0 warnings with the local 2023.09.07; CI lints it with 2026.10.26 and fails only on errors (exit ≥ 3).
- 2026-11-11 WP0.3: `EXPECTED.md` conventions WP1.5 must honor or amend: definition-form usages (`defn`, `defmethod`, …) are not edges; `let`/`fn` usages are certain calls if clj-kondo reports them; protocol impl discriminators are the type as written (`charge@Card`, `charge@String`) and `reify:L<line>`; re-frame registrations are `<top>@L<n>` in Phase 1 and get renamed by WP2.8. Rows marked **verify** need WP1.3's clj-kondo probe (record constructor vars, `@x` deref usage, type-hinted interop).
- 2026-11-11 WP0.3: Added `crates/aldente-testkit` (not in the architecture list): `fixture_dir`, `render_graph` (sorted, one node/edge per line, control characters and `\\` escaped) and `assert_graph_snapshot(fixture, level)`. Goldens live in `fixtures/<name>/golden/<name>__<level>.snap`. `run_pipeline` is a placeholder returning an empty graph; **WP1.5 replaces its body**, and the two placeholder goldens will then change (review them against `EXPECTED.md`). A missing golden fails the test; `*.snap.new` is gitignored.
- 2026-11-11 WP0.3: Model follow-up (commit e4d0151): namespace-level externals `ext:<lang>//<ns>` for R2, which WP0.2 had missed. `03-graph-model.md` updated.
- 2026-11-11 WP0.3: cargo-deny 0.20.2 now installed (in `~/.cargo/bin`); `cargo deny check` passes locally. Removed the `MIT-0` allowance: `borrow-or-share` is in `Cargo.lock` but not in the resolved graph. Set `unused-allowed-license = "allow"` because the list is the plan's, allowed ahead of need.
- 2026-11-11 WP0.3: `fixtures/hostile/README.md` lists the safety cases with the WP expected to add each (best guess).

- 2026-11-11 WP0.2: **External IDs changed to `ext:<lang>//<ns>/<name>`** (Austin's call): the old `<lang>:ext//…` collided with a unit in a directory named `ext`. `03-graph-model.md` updated, including the reserved-prefix rule: no language may be called `unit`, `lib`, `hole`, `ext`, `repo`, `java`, `js`, `endpoint`, `datastore`, `host`, `state`. Phase 10 must not name a lang `js`.
- 2026-11-11 WP0.2: Plan was silent, chose: `repo:.` for the repo node; `endpoint:`/`datastore:`/`host:`/`state:` + opaque text until Phases 6/8; `%`→`%25` and `@`→`%40` in symbol names so the first bare `@` starts an impl discriminator; hole line/col parsed from the right; leading zeros rejected so every ID has one written form. A symbol may be named `/c` (`clj:a//b//c` is valid).
- 2026-11-11 WP0.2: `RepoPath` is strict when parsing IDs/JSON (rejects `./`, `//`, `..`, absolute, trailing `/`). OS paths go through `RepoPath::from_native(_str)`: on Windows `\` is a separator and drive letters are rejected; on Unix `\` is a literal filename character. WP1.1's walker should use `from_native`.
- 2026-11-11 WP0.2: Envelope keys are always present (nullable but `required` in the schema, via `schema::all_keys_required`; field-level `#[schemars(required)]` strips `null`, don't use it). Node/Edge optional fields are omitted when absent to keep payloads small. Flags and langs are `BTreeSet`s, so they serialize sorted.
- 2026-11-11 WP0.2: `--json` is a global flag on the CLI. `aldente schema` with no name lists names. A broken stdout pipe (`| head`) exits 0 silently. Started `docs/cli.md` (Definition of done item 4).
- 2026-11-11 WP0.2: New deps: `serde`, `serde_json`, `schemars` 1.x, `thiserror`, `anyhow` (all in the plan); dev-only `proptest` (property tests) and `jsonschema` (validates output against the JSON Schema meta-schema). `deny.toml` now allows `MIT-0` (from `borrow-or-share`, dev-only via `jsonschema`).
- 2026-11-11 WP0.2: Debt to watch: each enum keeps a hand-written `ALL` list (and `Lang`/`BlindSpotCategory` an `as_str` match); tests catch serde/`as_str` drift but not a missing `ALL` entry. A `strum`-style derive could replace them if more enums appear.

- 2026-11-11 plan: Austin approved the local-target git hardening fix. `07-safety.md` now requires `--no-ext-diff --no-textconv` on `diff`/`log`, `--no-pager` + `GIT_PAGER=cat` on every local git call, and hostile test 6a. Implement in WP1.1.

- 2026-11-11 WP0.1: Repo root is `al-dente/` (plan moved up from `al-dente-plan/`; the plan package README is now `docs/plan/README.md`). Toolchain is stable 1.99.1 via rustup; `rust-version = 1.85` (edition 2024 minimum).
- 2026-11-11 WP0.1: Crates created: the nine in `02-architecture.md` plus `crates/acceptance` (empty lib; becomes a binary when acceptance lands). `aldente-py`, `aldente-ts`, `bench` deliberately not created yet (Phases 5 and 10). `acceptance/` at the root holds `repos.toml` later.
- 2026-11-11 WP0.1: "unwrap/expect denied outside tests" = workspace clippy lints `deny` + `clippy.toml` `allow-unwrap-in-tests`/`allow-expect-in-tests`. Verified on a scratch copy that `unwrap()` and `unsafe {}` in library code fail clippy.
- 2026-11-11 WP0.1: `deny.toml` also allows `AGPL-3.0-only` (our own crates) and both `Unicode-3.0` and `Unicode-DFS-2016` (the plan says "Unicode"). cargo-deny is not installed locally; licenses were checked by hand from `cargo metadata` (all MIT/Apache/Unicode-3.0). `cargo deny check` runs only in CI so far.
- 2026-11-11 WP0.1: CI pins clj-kondo 2026.10.26 (latest release today) via release zips; macOS uses the aarch64 build (`macos-latest` is Apple Silicon). Local machine has clj-kondo 2023.09.07, which is older than what WP0.4 will set as the minimum.
- 2026-11-11 WP0.1: Added `.gitattributes` (`* text=auto eol=lf`) so Windows checkouts don't break fmt checks or future golden files. Plan was silent; simplest choice.
