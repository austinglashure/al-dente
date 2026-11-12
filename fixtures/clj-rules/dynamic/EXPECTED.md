# clj-rules/dynamic: expected edges and holes (R8, WP2.5)

One row per line of the R8 table. IDs in `clj:.//`; holes are
`hole:<category>@src/dyn/core.clj:<line>:<col>` at the call's `(`.

| Function (`dyn.core/…`) | Edge | Target | Certainty, reason |
|---|---|---|---|
| `by-resolve` | calls | `dyn.target/handler` | likely, quoted-resolve |
| `by-requiring-resolve` | calls | `dyn.target/other` | likely, quoted-resolve |
| `by-ns-resolve` | calls | `dyn.target/handler` | likely, quoted-resolve |
| `by-find-var` | calls | `dyn.target/other` | likely, quoted-resolve |
| `computed` | calls | `hole:dynamic-resolve@src/dyn/core.clj:12:23` | certain, dynamic |
| `computed-ns` | calls | `hole:dynamic-resolve@src/dyn/core.clj:14:28` | certain, dynamic |
| `evaluate` | calls | `hole:dynamic-eval@src/dyn/core.clj:16:20` | certain, dynamic (one hole, at `eval`) |
| `load-it` | calls | `hole:dynamic-eval@src/dyn/core.clj:18:19` | certain, dynamic |
| `load-a-file` | calls | `hole:dynamic-eval@src/dyn/core.clj:20:26` | certain, dynamic |
| `load-reader-it` | calls | `hole:dynamic-eval@src/dyn/core.clj:22:26` | certain, dynamic |
| `load-lib` | calls | `hole:dynamic-eval@src/dyn/core.clj:24:19` | certain, dynamic |

`spread`'s `(apply f args)` adds nothing beyond R4/R7. `bump`, `make-var`
and `stubbed` add no hole; they flag `dyn.target/counter`, `dyn.target/made`
and `dyn.target/handler` as `dynamic` (`made` has no definition, so it is a
bare node). Every hole is also a blind spot of its category. There are
exactly seven holes.
