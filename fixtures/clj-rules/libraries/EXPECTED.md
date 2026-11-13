# libraries: expected library usage (ground truth, WP3.6)

Hand-written from the source. Locations are `file:line:col` relative to this
directory, at the opening parenthesis of the call (clj-kondo's position) or
the required namespace symbol.

## `lib usage reagent` (`lib:maven:reagent/reagent`)

Attribution: `reagent.core` and `reagent.dom` by the declared coordinate
`reagent/reagent` (R10, `likely`, `name-match`).

| From | Kind | To | Site |
|---|---|---|---|
| `lib.views/counter` | calls | `reagent.core/atom` | `src/lib/views.cljs:7:18` |
| `lib.views/mount!` | calls | `reagent.dom/render` | `src/lib/views.cljs:13:3` |
| `lib.views/wrapped` | calls | `reagent.core/as-element` | `src/lib/views.cljs:16:3` |
| `lib.views` | requires | `reagent.core` | `src/lib/views.cljs:3:14` |
| `lib.views` | requires | `reagent.dom` | `src/lib/views.cljs:4:14` |

Requires without calling: `lib.state` requires `reagent.core` at
`src/lib/state.cljs:3:14` and calls nothing from it.

`lib.server` does not use reagent.

## `lib usage react` (`lib:npm:react`)

| From | Kind | To | Site |
|---|---|---|---|
| `lib.views/wrapped` | calls | `react/createElement` | `src/lib/views.cljs:16:23` |
| `lib.views` | requires | `react` | `src/lib/views.cljs:5:14` |

`aldente crater lib:npm:react --direction reverse` reaches `lib.views/wrapped`.
