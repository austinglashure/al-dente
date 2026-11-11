# Leña assessment

This answers the two open questions in the handoff: how much of the analysis Leña already covers, and what its export format is. It is based on reading the cloned copy (workspace version 0.1.6).

## What Leña is

- **Language mix.** A Rust workspace (edition 2024) with seven crates: `lena-protocol`, `lena-core`, `lena-cli`, `lena-bench`, and adapters for Rust, Clojure and Tree-sitter languages. A Go adapter is written in Go. Shell scripts handle builds.
- **License.** `AGPL-3.0-only`, declared in the workspace `Cargo.toml`. The clone has no `LICENSE` file, so the declaration is the source of truth.
- **Purpose.** Its README describes a tool for generated documentation, call-graph skeletons and LLM context bundles. It is not built as an analysis engine for a graph explorer.
- **Shape.** Each adapter is a standalone binary that prints facts to stdout. `lena index` runs the adapters as subprocesses and loads the facts into MesoDB, an embedded database by the same author, at `.lena/db`.

## Export format: not a Datalog export

Austin expected Datalog. The accurate picture:

- **Between adapters and the core:** newline-delimited EDN transactions in Datomic style, one vector of `[:db/add <entity-id> <attribute> <value>]` facts per line.
- **In the database:** entity-attribute-value facts under a fixed schema (the "UIR" vocabulary in `lena-protocol/src/schema.rs`).
- **Queries:** Datomic-style Datalog, with four recursive rules built in (`calls_rule`, `calls_transitive`, `parent_rule`, `parent_transitive`). Results come back as JSON.
- **Outputs:** HTML documentation, mdBook, XML or Markdown prompt bundles, and call skeletons as ASCII, Mermaid or DOT.

So Datalog is the query language, not an export. There is no command that writes the graph out as Datalog facts or rules.

## What the Clojure adapter extracts

`lena-adapter-clojure` shells out to `clj-kondo --lint <dir>` with analysis on, and reads three of the buckets clj-kondo offers:

| clj-kondo bucket | Leña turns it into |
|---|---|
| `namespace-definitions` | a module entity per namespace |
| `var-definitions` | an entity per var, always typed `function`, kind `fn` or `macro` |
| `var-usages` | a call-site entity with caller and callee, plus a stub entity for callees outside the repository |

## Coverage against the handoff

| Need | Leña today | Verdict |
|---|---|---|
| Namespace graph | Namespaces exist as entities. Require relationships (`namespace-usages`) are not read, so namespace edges exist only by implication from symbol edges. | Partial |
| Symbol invocation graph | Direct var-to-var usage edges for Clojure. | Partial, see gaps |
| Uncertain calls marked | The schema has `:call/status` (`resolved`, `ambiguous`); the Clojure and Tree-sitter adapters never set it. | Missing |
| Multimethods, protocols, higher-order | No dispatch modelling. Code inside a `defmethod` body has no enclosing var in clj-kondo's output, and Leña drops every usage without one, so those calls vanish. | Missing |
| Java and JavaScript interop | Not read. | Missing |
| ClojureScript and `.cljc` | Everything is prefixed `clj:`; the language field is ignored, so `.cljc` definitions collapse or duplicate. | Missing |
| Top-level code (route tables, `def` initializers) | Dropped for the same no-enclosing-var reason. | Missing |
| State machines | Nothing. | Missing |
| Impact crater | `callers` and `callees` return direct neighbours only; `skeleton` walks to a depth. No certainty, no reverse checklist, no stack traces. | Seed only |
| Library impact | External callees are stubbed by namespace; no mapping to a library or version. | Seed only |
| Network analysis, coverage, security, deployment, dependency graph | Nothing. | Missing |
| Python and TypeScript | The Tree-sitter adapter matches calls by bare name across the whole repository, guesses when a name is ambiguous, and identifies a call site by caller and callee only, so repeated calls merge. No imports, no types. | Not usable as-is |
| Local web GUI, JSON-first CLI | Static HTML docs only. Only `query` prints JSON. | Missing |
| Incremental or cached analysis | Re-indexing adds facts; nothing retracts facts for deleted or changed code. | Missing |
| Safe analysis of untrusted repositories | clj-kondo runs against the target with default settings, so the target's own `.clj-kondo` configuration and hooks are honored. No clone handling. | Missing |

Rough size of the overlap: Leña supplies the first step of use case 1 for Clojure (definitions and direct usage edges). Everything else in the handoff must be built.

## Things in Leña not to carry over

- Entity IDs come from Rust's `DefaultHasher` truncated to 63 bits. The hash is not specified to be stable across Rust versions and collisions go undetected.
- Queries are built by interpolating symbol names into Datalog strings.
- Every var is a "function"; `def`, `defmulti`, `defprotocol` and `defrecord` are not told apart.
- The release profile sets `panic = "abort"`, which would take a long-running server down on any bug.

## Relationship chosen

Al Dente is a separate project that keeps Leña's good idea and stays friendly to it. See decision D1 in `decisions.md` for the alternatives.

- **Kept:** the architecture of thin per-language adapters feeding one language-neutral graph; `clj-kondo` as the Clojure front end; the `lang:namespace/name` naming style.
- **Not used:** `lena-core`, MesoDB, the Leña CLI, Leña's adapters.
- **Interop:** Phase 10 adds an importer for Leña's EDN fact stream, so any Leña adapter (Go, Rust, Java, PHP and so on) can feed Al Dente at low fidelity. Improvements to the Clojure extraction can be offered back to Leña's author as findings; both projects share a license.
