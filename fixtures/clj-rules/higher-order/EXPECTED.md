# clj-rules/higher-order: expected edges (R7, WP2.4)

All IDs are in `clj:.//ho.core/`.

## Layer 1: `references`, likely (R4, already in Phase 1)

| From | To |
|---|---|
| `mapped` | `inc2` (passed to `map`) |
| `composed` | `inc2`, `dbl` (passed to `comp`) |
| `partial-dbl` | `dbl` (passed to `partial`) |
| `table` | `inc2` (stored in a map) |
| `returns-fn` | `dbl` (returned) |
| `use-apply` | `inc2` (passed to `apply-it`) |

## Layer 2: `calls`, possible, reason `param-flow`

| From | To | Why |
|---|---|---|
| `apply-it` | `inc2` | `use-apply` passes it as `f`, and `apply-it` calls `f` |
| `apply-it` | `dbl` | `use-apply-2` passes it as `f` |
| `twice` | `dbl` | `use-twice` calls the two-argument arity, which calls `f` |

Not present: `twice → inc2` (`use-twice-1` calls the one-argument arity,
which does not call `f`; it only passes it on, and R7 is one level only);
nothing for `use-anon`'s anonymous function. These are the only `param-flow`
edges.
