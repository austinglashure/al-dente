# comment-blocks: calls inside `(comment …)` (ground truth)

Code in a top-level `(comment …)` form runs only when someone evaluates it
at a REPL. Its edges keep their reason but are `possible` (decided by
Austin, 2026-11-13, during M3). Locations are `file:line:col`.

| From | Kind | To | Certainty | Reason | Site |
|---|---|---|---|---|---|
| `app.core/used` | calls | `app.core/helper` | certain | direct | `src/app/core.clj:6:15` |
| `app.core/also` | calls | `app.core/helper` | certain | direct | `src/app/core.clj:13:15` |
| `app.core/<top>@L8` | calls | `app.core/helper` | possible | direct | `src/app/core.clj:9:3` |
| `app.core/<top>@L8` | calls | `app.core/used` | possible | direct | `src/app/core.clj:10:3` |
| `app.core/<top>@L8` | calls | the `dynamic-eval` hole | possible | dynamic | `src/app/core.clj:11:3` |
| `app.core/<top>@L15` | calls | `app.core/also` | possible | direct | `src/app/core.clj:16:3` |

The `<top>` scopes' calls to `clojure.core/comment` itself and to
`clojure.core/eval` are `possible` too: everything inside the form is.
