# Clojure and ClojureScript analysis

The adapter has two front ends and one resolver.

1. **clj-kondo** supplies resolved definitions and usages.
2. **The form index**, Al Dente's own Tree-sitter pass, supplies what clj-kondo leaves out: the extent of every top-level form, literals, argument positions.
3. **The resolver** joins the two and applies the rules below.

## Running clj-kondo safely

Invoke once per unit, as an argument vector, never through a shell:

```text
clj-kondo --lint <source roots...>
          --config-dir <empty directory owned by Al Dente>
          --cache false
          --parallel
          --config <inline EDN, below>
```

```clojure
{:output {:format :json}
 :analysis {:arglists true
            :keywords true
            :locals true
            :protocol-impls true
            :java-class-usages true
            :java-class-definitions true
            :java-member-definitions true
            :instance-invocations true
            :context [:re-frame.core]
            :var-definitions {:meta true}
            :namespace-definitions {:meta true}}}
```

- `--config-dir` pointing at an empty directory is the safety-critical flag. Without it clj-kondo loads the target's `.clj-kondo/config.edn` and runs its hooks. A test must prove a hook planted in a fixture does not run.
- Lint findings are ignored. Exit codes 2 and 3 mean "findings exist", not failure.
- Enforce a timeout and a memory ceiling per unit; on failure record `unparsed-file` blind spots and continue with other units.
- Pin a minimum clj-kondo version in `aldente doctor`. Option names above come from clj-kondo's analysis documentation; WP1.3 starts by writing a probe test that asserts each bucket is present for the pinned version, and adjusts names if any differ.
- `:locals` output is large. If it dominates run time at scale, Phase 9 may restrict it to files that need parameter flow.

## The form index

Parse every `.clj`, `.cljs`, `.cljc`, `.edn` and `.bb` file with tree-sitter-clojure. Produce, per file:

- each top-level form: byte and line range, head symbol (`defn`, `defmethod`, `extend-type`, ...), and for definition forms the defined name;
- for `defmethod`: the multimethod symbol and the dispatch-value text;
- for `extend-type`, `extend-protocol`, `defrecord`, `deftype`, `reify`: the type and protocol names and each method body range;
- every string, keyword, number and regex literal with its location (kept in the `literal` table for Phases 6 and 8);
- for every list form in call position: the head and the argument forms' ranges (needed for parameter flow and dispatch-value matching);
- reader conditionals, with which branch each range belongs to.

The index is purely syntactic and tolerant: a file that fails to parse yields a diagnostic and whatever forms were recovered.

## Resolution rules

Each rule needs a fixture under `fixtures/clj-rules/` and a golden graph.

### R1. Definitions

One `symbol` node per clj-kondo var definition. `subkind` from `defined-by`: `defn`/`defn-`/`fn` → `fn`; `defmacro` → `macro`; `defmulti` → `multimethod`; `defprotocol` → `protocol` and its methods → `protocol-method`; `defrecord` → `record`; `deftype` → `type`; `deftest` or `:test` → `test`; anything else → `var`. Unknown `defined-by` macros (`defstate`, `defroutes`, `s/defn`) default to `var` unless they have arglists, then `fn`. Flags: `private`, `deprecated`, `dynamic` (from meta), `stateful` when the initializer is `(atom ...)`, `(ref ...)`, `(agent ...)`, `(volatile! ...)`.

### R2. Namespaces and requires

One `namespace` node per definition; one `requires` edge per namespace usage. A required namespace outside the repository becomes a namespace-level external and is attached to a library by R10.

### R3. Enclosing scope for every usage

clj-kondo gives `from-var` for usages inside a var. For usages without one, use the form index:

- inside a `defmethod` body → the `impl` node for that multimethod and dispatch value;
- inside a protocol method body in `extend-type`, `extend-protocol`, `defrecord`, `deftype` → the `impl` node for that type and method;
- any other top-level form → an `impl` node `<ns>/<top>@L<line>`, child of the namespace.

Nothing is dropped. This is the main fix over Leña.

### R4. Calls versus references

A usage whose position is the head of a list is a `calls` edge, reason `direct`, certainty `certain`. Any other usage is a `references` edge, reason `value-ref`, certainty `likely`. clj-kondo reports the position for calls as the opening parenthesis; confirm with the form index.

Macros: a usage of a macro is a `calls` edge to the macro symbol. Usages that appear in the macro's expansion are not visible; record one `macro-generated` blind spot per macro defined in the repository whose body syntax-quotes calls, and emit `references` edges from the macro to the vars named inside its syntax-quoted body with certainty `possible`.

### R5. Multimethods

- `defmulti` → `multimethod` symbol.
- each `defmethod` → `impl` node, an `implements` edge to the multimethod, and a `dispatches` edge from the multimethod to the impl.
- certainty of `dispatches`: if the dispatch function is a keyword or a literal `(fn [x] (:k x))` shape and a call site passes a literal map containing that key, the matching impl is `likely` and others are not linked from that site. In every other case each impl is `possible`. The `:default` impl is always included.
- callers keep a single `calls` edge to the multimethod. Traversals follow `dispatches`.

### R6. Protocols, records, types, reify

- protocol methods are `protocol-method` symbols.
- each implementation (from clj-kondo's protocol-impls bucket, joined with the form index for body ranges) → `impl` node, `implements` edge, `dispatches` edge from the protocol method.
- certainty: `likely` when the call's first argument is, in the same function, a local bound directly to a constructor call (`->Rec`, `map->Rec`, `Rec.`, `reify`) or carries a type hint naming the type; otherwise `possible` for every impl.
- `proxy` and `gen-class` bodies are scoped as impls; their targets are `platform` nodes.

### R7. Higher-order flow

Two layers.

- **Layer 1 (Phase 2, required).** R4's `references` edges already say "f hands g to something". For traversals, a `references` edge is followed like a call at its certainty.
- **Layer 2 (Phase 2, WP2.4).** When a function calls one of its own parameters (a local in call position whose binding is a parameter, from the locals bucket), find every call site of that function in the repository and the argument in the matching position (form index). If the argument is a var, emit `calls` from the function to that var, reason `param-flow`, certainty `possible`. One level only; no fixpoint. If no var can be found, record nothing extra: the caller's `references` edge already covers it.

Core functions that invoke their argument (`map`, `filter`, `reduce`, `apply`, `comp`, `partial`, `update`, `swap!`, `mapv`, `run!`, `keep`, `some`, `juxt`, `fnil`, `memoize`, `future-call`, `send`, `add-watch`, ...) need no special handling because the reference edge from the enclosing function already exists.

### R8. Dynamic code

| Form | Result |
|---|---|
| `(resolve 'a.b/c)`, `(requiring-resolve 'a.b/c)`, `(ns-resolve 'a.b 'c)`, `(find-var 'a.b/c)` with quoted literals | `calls` to the named var, reason `quoted-resolve`, `likely` |
| the same with non-literal arguments | `hole` node, category `dynamic-resolve` |
| `eval`, `load-string`, `load-file`, `load-reader`, `read-string` followed by eval, `clojure.core/load` | `hole`, category `dynamic-eval` |
| `(apply f args)` | covered by R4/R7 |
| `intern`, `alter-var-root`, `with-redefs` | flag the target symbol `dynamic`; no hole |

### R9. Interop

- Java: from java-class-usages and instance-invocations, emit `calls` to `platform` nodes `java:<class>` or `java:<class>#<member>` when the class is known, `java:?#<member>` when only the method name is known. Reason `interop`.
- JavaScript in ClojureScript: `js/foo.bar`, `(.method obj)`, `(.-prop obj)`, `goog.*` → `platform` nodes `js:<path>`.
- Each distinct platform target is also an `interop` blind spot, counted once.
- These nodes are leaves until Java analysis is added; the edge kinds and IDs are already what a Java adapter would attach to.

### R10. Library attribution

Every external namespace needs a `provides` edge from a `library` node, so "everywhere Reagent is used" works.

1. npm string requires (`["react" :as r]`) map to `lib:npm:<name>` directly, `certain`.
2. If the unit's declared dependencies have jars in the user's local Maven cache, read each jar's file list (read-only, no code loaded) and map namespaces to artifacts, `certain`.
3. Otherwise match by coordinate prefix (`reagent.core` → `reagent/reagent`; `clojure.core.async` → `org.clojure/core.async`) using the declared dependency list and a small built-in table in `rules/clj-namespaces.toml`, `likely`.
4. `clojure.*` and `cljs.*` core namespaces map to `lib:maven:org.clojure/clojure` and `.../clojurescript`.
5. Anything left attaches to `lib:unknown`, listed as `unresolved-symbol` blind spots.

### R11. Keyword-keyed dispatch

Without this a re-frame application's graph falls apart into islands.

- **re-frame.** `reg-event-db`, `reg-event-fx`, `reg-sub`, `reg-fx`, `reg-cofx` register a keyword (clj-kondo marks these keywords with `:reg`). Create an `impl` node per registration (`<ns>/<reg-kind>@<keyword>`), scoped per R3. Every `dispatch`, `dispatch-sync`, `subscribe`, and `:dispatch`/`:dispatch-n`/`:fx` effect with a literal keyword vector gives `calls` from the enclosing scope to the registered impl, reason `keyword-dispatch`, `likely`. Non-literal dispatch → `hole`.
- **Integrant, multimethod-based systems.** `ig/init-key` and friends are multimethods and fall under R5. Additionally, keys in an `ig/ref` or system config map with a matching `defmethod` dispatch value give `references` edges, `possible`.
- **Catalog-driven.** The list of registration and dispatch functions lives in `rules/clj-keyword-dispatch.toml` so Mount, Component, Fulcro mutations and others can be added as data.

### R12. `.cljc` and reader conditionals

clj-kondo analyzes `.cljc` once per language and tags each record with `lang`. Merge definitions with the same namespace and name into one node with both languages. Keep edges per language; if the same edge exists in both, store it once with `langs` set to both.

### R13. Entry points

Flag `entry-point` on: `-main`; functions named in `:main`, `:exec-fn`, `:init-fn`, shadow-cljs `:init-fn` and `:exports`; `^:export` vars; ring handler vars passed to a server start call; test vars. Entry points seed dead-code detection and the unit `service` flag.

### R14. Tests

A namespace is a test namespace when it lives under a declared test path or its name ends in `-test`/`test`. `deftest` vars, and `defspec`, `deftest-async`, Midje `fact(s)` and Kaocha-style forms listed in `rules/clj-tests.toml`, get subkind `test`.

## Things explicitly out of scope for the adapter

- Evaluating macros or any code.
- Type inference beyond the local constructor and type-hint evidence in R6.
- Resolving into dependency sources. External symbols are leaves; Phase 7 knows their versions, not their bodies.
