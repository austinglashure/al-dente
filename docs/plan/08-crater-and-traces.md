# Impact crater, stack traces, library impact

Covers use cases 3 and 4 and the stack trace normalizer.

## Impact crater

A crater is a **subgraph**, not a list of paths, because recursion and cycles make the set of possible stack traces unbounded.

### Inputs

- one or more seed nodes (symbols, impls, namespaces, libraries);
- direction: `forward` (everything the seed can reach), `reverse` (everything that can reach it), or `both`;
- `depth` (default 6; 0 means unlimited);
- `min_certainty` (default `likely` for craters, because `possible` edges can swallow a large codebase);
- edge kinds followed (default `calls`, `references`, `dispatches`, `registers`);
- stop conditions: `--stop-at <pattern>`, stop at externals (default on), stop at unit boundaries (default off), exclude tests (default off for reverse, since tests are the point of the checklist).

### Algorithm

Breadth-first search over the in-memory graph in the chosen direction. Track for each reached node:

- `distance` from the nearest seed;
- `path_certainty`: the weakest certainty on the strongest path to it. Compute with a three-pass search (certain-only, then adding likely, then adding possible) so a node reachable by a certain path is labeled certain even if a possible path is shorter;
- `via`: one predecessor on that best path, so the UI can explain "why is this here".

Dispatch handling: reaching a multimethod or protocol method continues into its impls through `dispatches`. In reverse, reaching an impl continues to its multimethod or protocol method and then to their callers.

Result: nodes with those three fields, the induced edges, counts by certainty and by unit, the boundary (externals, platform nodes and holes reached, which are the crater's blind spots), and a `truncated` flag when depth or the node cap cut it.

### Keeping craters useful on a large codebase

- Default certainty `likely` and depth 6; the summary line always states how many more nodes `possible` would add, so the user knows what is hidden.
- Hub damping: a node whose degree is above a threshold (default: top 0.1 percent, for example a logging function) is included but not expanded past, unless `--through-hubs`. The result lists the hubs that were not expanded.
- Group the result by unit and namespace in text and in the UI.

### The refactoring checklist

`--checklist` on a reverse crater produces rows: location, symbol, distance, path certainty, coverage status (`tested`, `indirect`, `untested`, from Phase 4), and whether it is an entry point. Sorted by certainty, then distance. This is the discrete list of places to verify after a change.

`--diff <rev>` builds the seed set from git: every symbol or impl whose source range intersects a changed hunk, then runs the reverse crater. That gives "what must I re-verify for this branch" in one command.

## Stack trace normalizer

### Canonical frame

```json
{ "index": 0, "lang": "clj", "namespace": "app.orders", "name": "submit!",
  "anonymous": false, "file": "app/orders.clj", "line": 50, "col": null,
  "raw": "app.orders$submit_BANG_.invokeStatic(orders.clj:50)",
  "noise": false, "node": "clj:.//app.orders/submit!", "match": "exact" }
```

`match` is `exact` (name and line fall inside the node's range), `name` (name resolves, line does not fit, for example stale code), `line` (only file and line resolved, mapped to the enclosing node), or `none`.

A normalized trace is `{ "exception": {...}, "frames": [...], "causes": [ trace... ] }`.

### Parsers

| Source | Handling |
|---|---|
| JVM (`at a.b$c.invoke(file.clj:12)`, `Caused by:`, `... 12 more`) | Demunge class names: `$` separates namespace from function and nested functions; `_BANG_`, `_QMARK_`, `_STAR_`, `_PLUS_`, `_GT_`, `_LT_`, `_EQ_`, `_SLASH_`, `_COLON_`, `_AMPERSAND_`, `_PERCENT_`, `_SHARP_`, `_DOT_`, `_SINGLEQUOTE_` and friends map back; `_` maps to `-`; `fn__1234` and `eval1234` suffixes mark anonymous functions and attach to the enclosing var by line. |
| Clojure data form (`Throwable->map`, `:trace` vectors, `clojure.main` reports, `ex-info` printed maps) | Read as EDN. |
| ClojureScript in a browser or Node | Frames are JavaScript positions. With a source map (`--source-map`, or a `.map` next to a file the user points at) map to original file, line and column, then to the enclosing node. Without one, demunge the JS name (`app.orders.submit_BANG_`) and match by name. |
| Python traceback, Node and V8 traces | Phase 10. The frame format already carries what they need. |

Input is auto-detected; multiple traces in one log are split. Garbage lines are ignored.

### Noise

A frame is noise when its namespace or class matches the catalog in `rules/trace-noise.toml`: `clojure.lang.*`, `clojure.core` internals (`apply`, `map` laziness machinery), `java.*`, `jdk.*`, `sun.*`, thread pool and server internals, nREPL, test runner frames, `cljs.core` internals, bundler runtime. Noise frames are kept in the output with `noise: true` and hidden by default. Frames from the repository are never noise.

### Trace-pinned crater

A trace says which implementations actually ran. Given a normalized trace:

1. The mapped, non-noise frames form a path of nodes, outermost first.
2. Add `trace-observed` overlay edges between consecutive frames. Where the base graph had a `dispatches` fan-out between them, the overlay picks the one impl that ran.
3. The crater is seeded at the innermost frame. In reverse it is restricted to the trace path itself plus, for each frame, its other callers one hop out (shown faint). Forward from the innermost frame it is the ordinary forward crater.
4. Report frames that could not be mapped and consecutive frames with no connecting edge in the base graph. The second list is valuable: each entry is a call the static analysis missed.

## Library impact

"Pick React or Reagent and see everywhere it is invoked."

- `lib ls`: each `library` node with the number of distinct symbols, namespaces and units that use it, and declared versions per unit.
- `lib usage <library>`: every edge from repository code into an `external` provided by that library, grouped by namespace or symbol, with sites. Includes `requires` edges, so a namespace that requires the library but never calls it is listed as such.
- The same data seeds a reverse crater: "what breaks if this library changes" is `aldente crater lib:npm:react --direction reverse`.
- In the GUI, selecting a library highlights every using node at the current aggregation level and lists the call sites in the side panel.
- Attribution certainty (rule R10) is shown, and `lib:unknown` is listed so missing attributions are visible.
