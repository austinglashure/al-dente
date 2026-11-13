# no-ns

A file without an `ns` form (a script). Clojure evaluates it in the namespace `user`, so the analyzer does too.

| Expectation | Why |
|---|---|
| Node `clj:.//user` (namespace) exists and contains `user/shout` and the two top-level forms `user/<top>@L2`, `user/<top>@L6` | Code in a script has a home; edges never point at a missing node, so the snapshot saves. |
| `user/<top>@L6 -calls-> user/shout`, certain | Ordinary call resolution inside the implicit namespace. |
