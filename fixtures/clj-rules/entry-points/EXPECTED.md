# clj-rules/entry-points: expected (R13 and R14, WP2.9)

IDs in `clj:.//`.

## Entry points (`entry-point` flag)

| Symbol | Why |
|---|---|
| `app.core/start` | `:exec-fn` in the `:run` alias of `deps.edn` |
| `app.core/app` | ring handler: `#'app` is the first argument of `run-jetty` (`rules/clj-entry-points.toml`) |
| `app.core/other-handler` | ring handler passed by name |
| `app.core/-main` | named `-main` |
| `app.core/exported` | `^:export` |
| `app.thing-test/helper-is-two` | a test var (also `test`) |
| `app.thing-test/newest-is-stable` | `defspec` (`rules/clj-tests.toml`), subkind `test` |
| `app.midje-facts/<top>@L5` | a Midje `fact` (also `test`) |

Not entry points: `app.core/helper`, `app.core/serve-other`, `app.latest/newest`, `app.thing-test/fixture-helper`, `app.helpers/make-req`.

## Tests (`test` flag)

| Node | Why |
|---|---|
| namespace `app.thing-test` | name ends in `-test` |
| namespace `app.helpers` | under the test root `test` |
| namespace `app.midje-facts` | under the test root |
| `app.thing-test/helper-is-two`, `app.thing-test/newest-is-stable` | subkind `test` |

Namespace `app.latest` is not a test namespace. `app.thing-test/fixture-helper` keeps subkind `fn` and has no `test` flag (a helper in a test namespace is not a test).
