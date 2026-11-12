# clj-mini: expected graph (ground truth)

Hand-written. Golden snapshots are reviewed against this file, never the other way round. When a golden disagrees with a row, either the analyzer is wrong or this file is; fix this file only with a reason in the commit message.

## Notation

- One unit, the root: `unit:.`. A bare name such as `shop.core/checkout` means `clj:.//shop.core/checkout`. `ext:` names are externals: `ext:clojure.core/map` means `ext:clj//clojure.core/map`. ClojureScript core is `ext:cljs.core/…` (the `clj` prefix covers the whole Clojure family).
- Locations are `file:line:col`, 1-based, relative to `fixtures/clj-mini/`.
- **Phase** is when the row must first hold: `1` = rules R1–R4 (WP1.5), `2` = the Phase 2 work package named in the note.
- Certainty `C` = certain, `L` = likely, `P` = possible. Reasons are the snake-free names from `03-graph-model.md`.
- Rows marked **verify** depend on details of clj-kondo's output that WP1.3's probe test must confirm. If clj-kondo differs, update the row and say so.

## Scope rules for the edge tables

1. **Definition forms are not listed.** Usages of `ns`, `defn`, `defn-`, `def`, `defonce`, `defmacro`, `defmulti`, `defmethod`, `defprotocol`, `defrecord`, `extend-type` and `deftest` that *define* the node are not edges. WP1.5 suppresses exactly these clj-kondo var usages (`aldente-clj::resolve::Suppression`), and the "no usage dropped" test counts them:
   - `definition-form`: the head symbol of a top-level form whose head is one of the above (plus `deftype`, `definterface`, `extend-protocol`);
   - `defmethod-target`: the multimethod named by a `defmethod` (clj-kondo marks it `defmethod: true`);
   - `protocol-in-impl-form`: a protocol symbol in the header of `extend-type`, `extend-protocol`, `defrecord`, `deftype` or `reify` (Phase 2 adds `implements`);
   - `protocol-signature`: parameter names in a `defprotocol` signature, which clj-kondo reports as unresolved;
   - `ns-refer`: names in an `ns` form's `:refer` (the `requires` edge covers them).
   - `macro-template`: symbols clj-kondo cannot resolve inside a macro's syntax-quoted template, i.e. gensyms (`start#`) and special forms (`do`); they are not vars.
   Every other usage becomes an edge site or an `unresolved-symbol` blind spot.
2. **`let` and `fn`** (confirmed with clj-kondo 2026.10.26 in WP1.3). Every usage of `clojure.core/let` and `clojure.core/fn` (`cljs.core/…` in `.cljs`) is a certain `calls` edge from the enclosing scope (clj-kondo reports 5 `let` and 4 `fn` usages). They are not listed row by row. The scopes that use them: `shop.core/checkout`, `shop.core/-main`, `shop.plugins/load-plugin`, `shop.ui.views/cart-view` (`let`); the four re-frame registrations (`fn`).
3. **`@x`** (confirmed in WP1.3). The reader expands `@x` to `(deref x)` and clj-kondo reports it as a `clojure.core/deref` usage (2 in this fixture), so each is a certain `calls` edge to `ext:clojure.core/deref`, not listed row by row; `x` itself is a `references` edge.
4. **Keywords in call position** (`(:sku order)`) are not vars and produce no edge.
4a. **Reader macros are calls.** `@x`, `#(…)` and `#'x` expand to `deref`, `fn*` and `var`; clj-kondo reports those at the reader macro's position, and they are certain `calls` edges like `let` and `fn`. Usages inside `#(…)` are calls when in call position (`#(rf/dispatch …)` calls `dispatch`). `(.method x)` gives a certain `calls` edge to `ext:clojure.core/.` in Phase 1; R9 (WP2.6) replaces it with the `java:` platform edge.
4b. **Phase 1 differences from the final rows.** `.cljc` `requires` edges are per language (two `shop.model -requires-> ext:clojure.string` edges, `lang` clj and cljs) until R12 merges them in Phase 2. `js/console.log` already gives the Phase 2 `js:console.log` platform edge (certain, `interop`), because JS usages have no other target.
5. **Every other certain edge is listed.** Phase 2.10 requires "no extra `certain` edge".

## Nodes

### Namespaces (Phase 1, R2)

| Namespace | File | langs |
|---|---|---|
| `shop.core` | `src/shop/core.clj` | clj |
| `shop.util` | `src/shop/util.clj` | clj |
| `shop.pricing` | `src/shop/pricing.clj` | clj |
| `shop.payment` | `src/shop/payment.clj` | clj |
| `shop.plugins` | `src/shop/plugins.clj` | clj |
| `shop.audit` | `src/shop/audit.clj` | clj |
| `shop.interop` | `src/shop/interop.clj` | clj |
| `shop.inventory` | `src/shop/inventory.clj` | clj |
| `shop.model` | `src/shop/model.cljc` | clj, cljs |
| `shop.ui.events` | `src/shop/ui/events.cljs` | cljs |
| `shop.ui.subs` | `src/shop/ui/subs.cljs` | cljs |
| `shop.ui.views` | `src/shop/ui/views.cljs` | cljs |
| `shop.pricing-test` | `test/shop/pricing_test.clj` | clj |

### Symbols (Phase 1, R1, unless noted)

| Symbol | Subkind | Flags | Note |
|---|---|---|---|
| `shop.core/checkout` | fn | | |
| `shop.core/-main` | fn | entry-point (Phase 2, R13) | |
| `shop.util/clamp` | fn | private | |
| `shop.util/apply-discount` | fn | | calls its parameter |
| `shop.util/log-timing` | fn | | |
| `shop.util/with-timing` | macro | | body is syntax-quoted |
| `shop.pricing/price` | multimethod | | dispatch fn `:kind` |
| `shop.pricing/half-off` | fn | | |
| `shop.payment/Chargeable` | protocol | | |
| `shop.payment/charge` | protocol-method | | |
| `shop.payment/Card` | record | | clj-kondo defines `Card` (confirmed in WP1.3) |
| `shop.payment/Voucher` | record | | |
| `shop.payment/->Card`, `map->Card`, `->Voucher`, `map->Voucher` | fn | | generated constructors; clj-kondo defines them (confirmed in WP1.3) |
| `shop.payment/free-charger` | fn | | contains a `reify` |
| `shop.plugins/audit-hook` | fn | | |
| `shop.plugins/load-plugin` | fn | | |
| `shop.plugins/run-script` | fn | | |
| `shop.audit/record!` | fn | | reached only through `requiring-resolve` |
| `shop.interop/now-ms` | fn | | |
| `shop.interop/new-id` | fn | | |
| `shop.interop/shout` | fn | | |
| `shop.inventory/stock` | var | stateful | `defonce` of an `atom` |
| `shop.inventory/reserve!` | fn | | |
| `shop.inventory/restock!` | fn | | |
| `shop.inventory/available` | fn | | |
| `shop.model/format-money` | fn | | langs clj, cljs (one node) |
| `shop.model/order-summary` | fn | | langs clj, cljs (one node) |
| `shop.ui.views/cart-view` | fn | | |
| `shop.ui.views/submit-button` | fn | | |
| `shop.ui.views/init` | fn | entry-point (Phase 2, R13, `^:export`) | |
| `shop.pricing-test/food-is-taxed` | test | test, entry-point (Phase 2, R13/R14) | |
| `shop.pricing-test/book-is-half-off` | test | test, entry-point (Phase 2, R13/R14) | |

### Impls (Phase 1, R3)

The discriminators for protocol impls and `reify` are not fixed by the plan. This file uses the type name as written and `reify:L<line>`; WP2.3 may change that and must update these rows.

| Impl | Location | Note |
|---|---|---|
| `shop.core/<top>@L10` | `src/shop/core.clj:10:1` | top-level `println` |
| `shop.pricing/price@:book` | `src/shop/pricing.clj:12:1` | |
| `shop.pricing/price@:food` | `src/shop/pricing.clj:15:1` | |
| `shop.pricing/price@:gift` | `src/shop/pricing.clj:18:1` | |
| `shop.pricing/price@:default` | `src/shop/pricing.clj:21:1` | |
| `shop.payment/charge@Card` | `src/shop/payment.clj:9:3` | in `defrecord` |
| `shop.payment/charge@Voucher` | `src/shop/payment.clj:14:3` | in `defrecord` |
| `shop.payment/charge@String` | `src/shop/payment.clj:19:3` | in `extend-type` |
| `shop.payment/charge@reify:L25` | `src/shop/payment.clj:26:5` | Phase 2 (R6). In Phase 1 its body belongs to `free-charger`. |
| `shop.ui.events/<top>@L5` | `src/shop/ui/events.cljs:5:1` | Phase 2 (R11) renames to `shop.ui.events/reg-event-db@:cart/add` |
| `shop.ui.events/<top>@L9` | `src/shop/ui/events.cljs:9:1` | Phase 2 renames to `shop.ui.events/reg-event-fx@:order/submit` |
| `shop.ui.events/<top>@L14` | `src/shop/ui/events.cljs:14:1` | Phase 2 renames to `shop.ui.events/reg-event-db@:order/submitted` |
| `shop.ui.subs/<top>@L5` | `src/shop/ui/subs.cljs:5:1` | Phase 2 renames to `shop.ui.subs/reg-sub@:cart/items` |

Below, `events@L5`, `events@L9`, `events@L14` and `subs@L5` stand for these impls under whichever name the phase uses.

### Holes, platforms and blind spots (Phase 2)

| Node | Category | WP | Note |
|---|---|---|---|
| `hole:dynamic-resolve@src/shop/plugins.clj:8:11` | dynamic-resolve | 2.5 | `requiring-resolve` on a variable |
| `hole:dynamic-eval@src/shop/plugins.clj:12:3` | dynamic-eval | 2.5 | `eval` of `read-string`; one hole, at the `eval` |
| `java:java.lang.System#currentTimeMillis` | interop | 2.6 | |
| `java:java.util.UUID#randomUUID` | interop | 2.6 | |
| `java:java.lang.String#toUpperCase` | interop | 2.6 | via the `^String` hint; if hints are not used, `java:?#toUpperCase` |
| `js:console.log` | interop | 2.6 | |
| blind spot for `shop.util/with-timing` | macro-generated | Phase 1 (R4) | one per repository macro whose body syntax-quotes calls |

## Edges

### `contains` (Phase 1)

`unit:.` contains every namespace. Each namespace contains its symbols and its `<top>` impls. `shop.pricing/price` contains the four `price@…` impls; `shop.payment/charge` contains the four `charge@…` impls. Not listed row by row.

### `requires` (Phase 1, R2)

| From | To |
|---|---|
| `shop.core` | `shop.inventory`, `shop.model`, `shop.payment`, `shop.plugins`, `shop.pricing`, `shop.util` |
| `shop.util` | `shop.interop` |
| `shop.pricing` | `shop.util` |
| `shop.model` | `ext:clojure.string` |
| `shop.ui.events` | `ext:re-frame.core` |
| `shop.ui.subs` | `ext:re-frame.core` |
| `shop.ui.views` | `ext:react`, `ext:re-frame.core`, `shop.model`, `shop.ui.events`, `shop.ui.subs` |
| `shop.pricing-test` | `ext:clojure.test`, `shop.pricing` |

`shop.interop`'s `(:import (java.util UUID))` produces no `requires` edge; the class appears through R9.

### `calls` and `references`, Phase 1 (R3, R4)

`calls` rows are certain, reason `direct`. `references` rows are likely, reason `value-ref`.

| From | Kind | To | Site |
|---|---|---|---|
| `shop.core/<top>@L10` | calls | `ext:clojure.core/println` | `src/shop/core.clj:10:1` |
| `shop.core/checkout` | calls | `shop.util/with-timing` | `src/shop/core.clj:15:3` |
| `shop.core/checkout` | calls | `shop.pricing/price` | `src/shop/core.clj:16:17` |
| `shop.core/checkout` | calls | `shop.inventory/reserve!` | |
| `shop.core/checkout` | calls | `shop.payment/charge` | `src/shop/core.clj:18:7` |
| `shop.core/checkout` | calls | `shop.model/order-summary` | |
| `shop.core/-main` | calls | `shop.plugins/audit-hook` | `src/shop/core.clj:22:3` |
| `shop.core/-main` | calls | `shop.payment/->Card` | `src/shop/core.clj:23:14` |
| `shop.core/-main` | calls | `shop.payment/charge` | `src/shop/core.clj:24:5` |
| `shop.core/-main` | calls | `shop.core/checkout` | |
| `shop.core/-main` | calls | `ext:clojure.core/println` | |
| `shop.util/clamp` | calls | `ext:clojure.core/max`, `ext:clojure.core/min` | |
| `shop.util/apply-discount` | calls | `shop.util/clamp` | |
| `shop.util/log-timing` | calls | `ext:clojure.core/println` | |
| `shop.pricing/half-off` | calls | `ext:clojure.core//` | |
| `shop.pricing/price@:book` | calls | `shop.util/apply-discount` | |
| `shop.pricing/price@:book` | references | `shop.pricing/half-off` | `src/shop/pricing.clj:13:24` |
| `shop.pricing/price@:food` | calls | `ext:clojure.core/*` | |
| `shop.payment/charge@Voucher` | calls | `ext:clojure.core/max`, `ext:clojure.core/-` | |
| `shop.payment/free-charger` | calls | `ext:clojure.core/reify` | `src/shop/payment.clj:25:3` |
| `shop.plugins/audit-hook` | calls | `ext:clojure.core/requiring-resolve` | `src/shop/plugins.clj:5:4` |
| `shop.plugins/load-plugin` | calls | `ext:clojure.core/requiring-resolve`, `ext:clojure.core/symbol` | |
| `shop.plugins/run-script` | calls | `ext:clojure.core/eval`, `ext:clojure.core/read-string` | |
| `shop.audit/record!` | calls | `ext:clojure.core/println` | |
| `shop.interop/new-id` | calls | `ext:clojure.core/str` | |
| `shop.inventory/stock` | calls | `ext:clojure.core/atom` | |
| `shop.inventory/reserve!` | calls | `ext:clojure.core/swap!` | `src/shop/inventory.clj:7:3` |
| `shop.inventory/reserve!` | references | `shop.inventory/stock`, `ext:clojure.core/update`, `ext:clojure.core/-` | |
| `shop.inventory/restock!` | calls | `ext:clojure.core/swap!`, `ext:clojure.core/fnil` | `src/shop/inventory.clj:10:3` |
| `shop.inventory/restock!` | references | `shop.inventory/stock`, `ext:clojure.core/update`, `ext:clojure.core/+` | |
| `shop.inventory/available` | calls | `ext:clojure.core/get` | |
| `shop.inventory/available` | references | `shop.inventory/stock` | `src/shop/inventory.clj:13:8` |
| `shop.model/format-money` | calls | `ext:clojure.core/format`, `ext:clojure.core/double` | edge `lang` = clj |
| `shop.model/format-money` | calls | `ext:cljs.core/str` | edge `lang` = cljs |
| `shop.model/order-summary` | calls | `ext:clojure.string/join`, `shop.model/format-money` | both languages; one edge each |
| `events@L5` | calls | `ext:re-frame.core/reg-event-db`, `ext:cljs.core/update-in`, `ext:cljs.core/fnil` | |
| `events@L5` | references | `ext:cljs.core/inc` | |
| `events@L9` | calls | `ext:re-frame.core/reg-event-fx`, `ext:cljs.core/assoc` | |
| `events@L14` | calls | `ext:re-frame.core/reg-event-db`, `ext:cljs.core/assoc` | |
| `subs@L5` | calls | `ext:re-frame.core/reg-sub` | |
| `shop.ui.views/cart-view` | calls | `ext:re-frame.core/subscribe`, `ext:react/createElement`, `ext:cljs.core/count` | |
| `shop.ui.views/submit-button` | calls | `ext:react/createElement`, `ext:re-frame.core/dispatch` | the `#(…)` belongs to `submit-button` |
| `shop.ui.views/init` | calls | `shop.model/format-money` (lang cljs), `ext:re-frame.core/dispatch` | |
| `shop.pricing-test/food-is-taxed` | calls | `ext:clojure.test/is`, `ext:clojure.core/<`, `shop.pricing/price` | `test/shop/pricing_test.clj:6:15` |
| `shop.pricing-test/book-is-half-off` | calls | `ext:clojure.test/is`, `ext:clojure.core/=`, `shop.pricing/price` | `test/shop/pricing_test.clj:9:12` |

#### Macro bodies (Phase 1, R4)

`shop.util/with-timing` has `references`, certainty possible, reason `macro-body`, to the vars named in its syntax-quoted body: `shop.interop/now-ms`, `shop.util/log-timing`, `ext:clojure.core/let`, `ext:clojure.core/-`. There is also one `macro-generated` blind spot.

### Phase 2 edges

| From | Kind | To | C | Reason | WP | Note |
|---|---|---|---|---|---|---|
| each `shop.pricing/price@…` | implements | `shop.pricing/price` | | | 2.2 | four edges |
| `shop.pricing-test/food-is-taxed` | dispatches | `price@:food` L, `price@:default` P; `:book`, `:gift` not linked | | multimethod-dispatch | 2.2 | literal `:kind :food`; site `test/shop/pricing_test.clj:6:15` |
| `shop.pricing-test/book-is-half-off` | dispatches | `price@:book` L, `price@:default` P; others not linked | | multimethod-dispatch | 2.2 | site `test/shop/pricing_test.clj:9:12` |
| `shop.core/checkout` | dispatches | all four `price@…` impls P | | multimethod-dispatch | 2.2 | non-literal argument; site `src/shop/core.clj:16:17` |
| each `shop.payment/charge@…` (4) | implements | `shop.payment/Chargeable` | | | 2.3 | records and the `reify` too |
| `shop.core/-main` | dispatches | `charge@Card` L; others not linked | | protocol-dispatch | 2.3 | local bound to `->Card`; site `src/shop/core.clj:24:5` |
| `shop.core/checkout` | dispatches | all four `charge@…` impls P | | protocol-dispatch | 2.3 | site `src/shop/core.clj:18:7` |
| `shop.util/apply-discount` | calls | `shop.pricing/half-off` | P | param-flow | 2.4 | the argument at `price@:book` |
| `shop.plugins/audit-hook` | calls | `shop.audit/record!` | L | quoted-resolve | 2.5 | |
| `shop.plugins/load-plugin` | calls | `hole:dynamic-resolve@src/shop/plugins.clj:8:11` | C | dynamic | 2.5 | |
| `shop.plugins/run-script` | calls | `hole:dynamic-eval@src/shop/plugins.clj:12:3` | C | dynamic | 2.5 | |
| `shop.interop/now-ms` | calls | `java:java.lang.System#currentTimeMillis` | C | interop | 2.6 | |
| `shop.interop/new-id` | calls | `java:java.util.UUID#randomUUID` | C | interop | 2.6 | |
| `shop.interop/shout` | calls | `java:java.lang.String#toUpperCase` | C | interop | 2.6 | |
| `shop.ui.views/init` | calls | `js:console.log` | C | interop | 2.6 | |
| `shop.ui.views/submit-button` | calls | `shop.ui.events/reg-event-fx@:order/submit` | L | keyword-dispatch | 2.8 | |
| `shop.ui.views/init` | calls | `shop.ui.events/reg-event-db@:cart/add` | L | keyword-dispatch | 2.8 | |
| `shop.ui.events/reg-event-fx@:order/submit` | calls | `shop.ui.events/reg-event-db@:order/submitted` | L | keyword-dispatch | 2.8 | `:dispatch` effect at `src/shop/ui/events.cljs:12:6` |
| `shop.ui.views/cart-view` | calls | `shop.ui.subs/reg-sub@:cart/items` | L | keyword-dispatch | 2.8 | |
| `lib:maven:org.clojure/clojure` | provides | `ext:clojure.core`, `ext:clojure.string`, `ext:clojure.test` | C | | 2.9 | R10 rule 4 |
| `lib:maven:org.clojure/clojurescript` | provides | `ext:cljs.core` | C | | 2.9 | |
| `lib:maven:re-frame/re-frame` | provides | `ext:re-frame.core` | L | | 2.9 | prefix match from `deps.edn` |
| `lib:npm:react` | provides | `ext:react` | C | | 2.9 | npm string require |

`lib:unknown` provides nothing.

## Edges that must not exist

| Not this | Why |
|---|---|
| any `certain` edge from a caller to a `price@…` or `charge@…` impl | dispatch is never certain |
| any edge from `shop.core/checkout` to `shop.interop/now-ms` or `shop.util/log-timing` | macro expansion is invisible; only `with-timing`'s possible references exist |
| a `certain` edge to `shop.audit/record!` | reached only by `quoted-resolve` |
| an edge from `shop.pricing-test/food-is-taxed` to `price@:book` or `price@:gift` | literal dispatch value selects one impl |
| any `dispatches` edge from `shop.pricing/price` or `shop.payment/charge` | dispatch edges start at the caller |
| a second `shop.model/format-money` node | `.cljc` definitions merge |
| any edge from `shop.util/apply-discount` to a local (`rule`) | locals are not nodes |
