# Graph model

One language-neutral graph. Adapters fill it; every analysis and both front ends read it.

## Node kinds

| Kind | Meaning | Examples |
|---|---|---|
| `repo` | the analyzed repository | |
| `unit` | a build unit; flag `service` when it has an entry point | a `deps.edn` directory |
| `namespace` | a namespace or module | `app.orders` |
| `symbol` | a named definition; `subkind` tells them apart | see below |
| `impl` | an implementation body with no name of its own | a `defmethod` for one dispatch value, a protocol method on one type, a top-level expression |
| `external` | a symbol defined outside the repository | `clojure.core/map`, `reagent.core/atom` |
| `library` | a dependency artifact; carries declared version(s) | `reagent/reagent`, npm `react` |
| `platform` | a host-platform target reached by interop | `java:java.net.Socket`, `js:fetch` |
| `hole` | a place the analyzer cannot see through | a call to `eval`, an unresolved `requiring-resolve` |
| `endpoint` | an ingress route or consumer (Phase 6) | `GET /orders/:id` |
| `datastore` | a database or queue the code talks to (Phase 6) | `postgres`, `kafka:orders` |
| `host` | an external network destination (Phase 6) | `api.stripe.com` |
| `state` | a state in an inferred machine (Phase 8) | `:order/paid` |

`symbol.subkind`: `fn`, `macro`, `var` (plain `def`), `multimethod`, `protocol`, `protocol-method`, `record`, `type`, `interface`, `class`, `method`, `test`.

Flags on symbols: `private`, `test`, `entry-point`, `deprecated`, `stateful` (an atom, ref, agent or volatile), `dynamic`.

Every node from source has a location: file, start and end line and column.

## Edge kinds

| Kind | From → to | Notes |
|---|---|---|
| `contains` | unit → namespace → symbol → impl | the hierarchy; not used in traversals unless asked |
| `requires` | namespace → namespace | from `ns` forms and imports |
| `calls` | symbol or impl → symbol, impl, external, platform, hole | the target is in call position |
| `references` | symbol or impl → symbol or external | the target is used as a value: passed, stored, returned |
| `dispatches` | multimethod or protocol-method → impl | "calling this may run that" |
| `implements` | impl or record/type → protocol or multimethod | structural, for navigation |
| `registers` | symbol or impl → symbol | keyword-keyed registration, for example a re-frame handler |
| `tests` | test symbol → symbol | derived in Phase 4 |
| `provides` | library → external or namespace | which library a namespace belongs to |
| `depends` | unit → library, library → library | Phase 7 adds versions and scopes |
| `routes` | endpoint → symbol | Phase 6 |
| `talks-to` | symbol → host, datastore or endpoint | Phase 6 |
| `transitions` | state → state | Phase 8 |

"Invocation graph" in the handoff means `calls`, `references` and `dispatches` together. Traversals treat `A calls M` followed by `M dispatches I` as reaching `I`.

## Certainty and reason

Every `calls`, `references`, `dispatches`, `registers`, `talks-to`, `provides` and `tests` edge has both fields.

| Certainty | Meaning | Default weight in measures |
|---|---|---|
| `certain` | The source says so directly and the target is unique. | 1.0 |
| `likely` | Sound over-approximation with a small target set, or a value reference that is usually invoked. | 0.6 |
| `possible` | Speculative: large target sets, name matching, flow through parameters. | 0.25 |

| Reason | Typical certainty |
|---|---|
| `direct` (resolved call in call position) | certain |
| `value-ref` (var used as a value) | likely |
| `multimethod-dispatch` | likely when the dispatch value at the call is a literal that selects one impl; otherwise possible for each impl |
| `protocol-dispatch` | likely with a type hint or constructor in scope that selects the impl; otherwise possible for each impl |
| `param-flow` (callee is a parameter; target passed by some caller) | possible |
| `keyword-dispatch` (for example re-frame event to handler) | likely for a literal keyword |
| `quoted-resolve` (`requiring-resolve` on a quoted literal) | likely |
| `name-match` (resolved by name only) | possible |
| `interop` | certain that it leaves the graph; target is a `platform` node |
| `trace-observed` (seen in a supplied stack trace) | certain, overlay only, never stored in the base snapshot |

Weights are configurable. Every traversal and measure takes `min_certainty`.

Parallel edges are kept distinct by `(from, to, kind, reason)`; call sites hang off the edge as `site` rows, so "called from three places" is three sites on one edge.

## Blind spots

A blind spot is recorded three ways so it cannot be missed:

1. a `hole` or `platform` node with an edge into it, visible in the graph;
2. a row in the snapshot's `blindspot` table with a category (`dynamic-eval`, `dynamic-resolve`, `interop`, `unresolved-symbol`, `unparsed-file`, `macro-generated`, `reflection`) and a location;
3. a count in the summary every `analyze` prints.

## Identifiers

Public IDs are strings, stable across runs and machines, and readable.

```text
unit:        unit:<repo-relative dir>                 unit:services/billing
namespace:   <lang>:<unit>//<ns>                      clj:services/billing//billing.core
symbol:      <lang>:<unit>//<ns>/<name>               clj:services/billing//billing.core/charge!
impl:        <symbol id>@<discriminator>              ...//billing.core/handle@:order/paid
                                                      ...//billing.core/<top>@L42
external:    ext:<lang>//<ns>/<name>                  ext:clj//clojure.core/map
library:     lib:<ecosystem>:<coordinate>             lib:maven:reagent/reagent   lib:npm:react
platform:    java:<class>[#member]  |  js:<path>      java:java.net.Socket#connect   js:fetch
hole:        hole:<category>@<file>:<line>:<col>
repo:        repo:.
endpoint, datastore, host, state:  <kind>:<text>      opaque until Phases 6 and 8 define them
```

- Top-level prefixes `unit`, `lib`, `hole`, `ext`, `repo`, `java`, `js`, `endpoint`, `datastore`, `host` and `state` are reserved: no language may use them as its `<lang>`. (Externals were once `<lang>:ext//…`, which collided with a unit in a directory named `ext`.)
- In `<name>`, `%` is written `%25` and `@` is written `%40`, so the first bare `@` always starts an impl discriminator. Discriminators are written as-is.
- Hole IDs parse the category up to the first `@` and `line`/`col` from the right, so file names may contain `:` and `@`.

- `<lang>` is `clj` for Clojure-family code. A definition from a `.cljc` file is one node with a `langs` attribute (`["clj","cljs"]`); edges carry the language they were resolved in when it differs. `.cljs`-only code sets `langs` to `["cljs"]`.
- The root unit's directory is written `.`.
- **Short forms.** The CLI accepts any unambiguous suffix (`billing.core/charge!`, even `charge!`). Ambiguity returns the candidates with exit code 3. The JSON output always uses full IDs.
- IDs are never hashed for identity. Integer row IDs exist only inside one snapshot.

## JSON shapes

Rust types in `aldente-model` are the source of truth; `aldente schema` prints the JSON Schema generated from them.

```json
{ "id": "clj:.//app.orders/submit!", "kind": "symbol", "subkind": "fn",
  "name": "submit!", "namespace": "app.orders", "unit": "unit:.",
  "langs": ["clj"], "flags": ["entry-point"],
  "loc": { "file": "src/app/orders.clj", "line": 41, "col": 1, "end_line": 58, "end_col": 30 },
  "doc": "Submits an order.", "signature": "[order] [order opts]" }
```

```json
{ "from": "clj:.//app.orders/submit!", "to": "clj:.//app.pay/charge",
  "kind": "dispatches", "certainty": "possible", "reason": "multimethod-dispatch",
  "sites": [ { "file": "src/app/orders.clj", "line": 50, "col": 5 } ] }
```

A graph payload is `{ "nodes": [...], "edges": [...], "truncated": false, "aggregated": null }`. When a result is cut to fit the payload cap, `truncated` is true and `aggregated` describes how nodes were grouped.

## Aggregation levels

| Level | Nodes | Edge between A and B means |
|---|---|---|
| `unit` | units and libraries | some symbol in A reaches some symbol in B |
| `namespace` | namespaces | `requires`, plus rolled-up invocation edges with a count |
| `symbol` | symbols, impls, externals | the stored edges |

Rolled-up edges carry `count` and the strongest certainty among their members, plus a `by_certainty` breakdown.
