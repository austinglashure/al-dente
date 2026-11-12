# clj-rules/multimethods: expected `dispatches` edges (R5, WP2.2)

`dispatches` edges start at the calling function and carry that call's
certainty; each caller also keeps its `calls` (or `references`) edge to the
multimethod. All IDs are in `clj:.//`; `area@:circle` means
`clj:.//mm.shapes/area@:circle`.

| Caller (`mm.use/…`) | Impls | Certainty | Why |
|---|---|---|---|
| `circle-area` | `area@:circle` | likely | keyword dispatch, literal `:shape :circle` |
| `circle-area` | `area@:default` | possible | `:default` stays possible when a literal picked another impl |
| `unknown-area` | `area@:circle`, `area@:square`, `area@:default`, `area@:triangle` | possible | non-literal argument |
| `unmatched-area` | `area@:default` | likely | literal `:hexagon` matches no impl, so `:default` runs |
| `pair` | `describe@[String String]`, `describe@:default` | possible | arbitrary dispatch function |
| `french` | `greet@:fr` | likely | `(fn [m] (:lang m))` is a keyword dispatch |
| `poodle` | `speak@:mm.shapes/dog`, `speak@:default` | possible | `:mm.shapes/poodle` has a `derive` parent, so hierarchy decides |
| `as-value` | `area@:circle`, `area@:square`, `area@:default`, `area@:triangle` | possible | the multimethod is passed as a value |

Not linked: `circle-area` to `area@:square` or `area@:triangle`; `french` to
`greet@:en`. No `dispatches` edge is `certain`, and none starts at a
multimethod.

`area@:triangle` is defined in `mm.more` but belongs to `mm.shapes/area`.
