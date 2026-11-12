# clj-rules/protocols: expected edges (R6, WP2.3)

All IDs are in `clj:.//`. `area@Circle` means `clj:.//pr.shapes/area@Circle`.
Impl discriminators are the type as written; `reify` bodies use
`reify:L<line of the reify form>`.

## Impls

`area` and `label` each have seven impls: `Circle` (defrecord), `Square`
(deftype), `Tri` (extend-type), `nil`, `Object`, `java.util.Map`
(extend-protocol), `reify:L36` (in `blob`). Each impl `implements`
`pr.shapes/Shape` and is contained by its protocol method.

## `dispatches`

| Caller (`pr.use/…`) | Impls | Certainty | Why |
|---|---|---|---|
| `circle-area` | `area@Circle` | likely | receiver is a local bound to `->Circle` |
| `square-area` | `area@Square` | likely | receiver is a `->Square` call |
| `tri-area` | `area@Tri` | likely | receiver is a local bound to `map->Tri` |
| `any-area` | all seven `area@…` | possible | unknown receiver |
| `blob-label` | all seven `label@…` | possible | local bound to a function call, not a constructor |

Not linked: `circle-area` to any `area@` other than `Circle`. No `dispatches`
edge is `certain`.

## Scope

The `(inc 41)` inside the `reify` belongs to `area@reify:L36`, not to `blob`.
