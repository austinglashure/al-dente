# clj-rules/keyword-dispatch: expected (R11, WP2.8)

IDs in `clj:.//`. All edges below have reason `keyword-dispatch`. Catalog: `rules/clj-keyword-dispatch.toml`.

## Registrations become impls

| Impl | Registered at |
|---|---|
| `app.events/reg-cofx@:now` | `src/app/events.cljs:4:1` |
| `app.events/reg-event-fx@:order/submit` | `src/app/events.cljs:7:1` |
| `app.events/reg-event-db@:order/submitted` | `src/app/events.cljs:14:1` |
| `app.events/reg-event-db@:order/log` | `src/app/events.cljs:17:1` |
| `app.events/reg-event-fx@:order/batch` | `src/app/events.cljs:20:1` |
| `app.events/reg-event-db@:app.events/local` | `src/app/events.cljs:23:1` (`::local` resolved) |
| `app.events/reg-fx@:app/notify` | `src/app/events.cljs:35:1` |
| `app.events/reg-event-fx@:order/notify` | `src/app/events.cljs:38:1` |
| `app.events/reg-event-fx@:order/forward` | `src/app/events.cljs:43:1` |
| `app.subs/reg-sub@:orders/all` | `src/app/subs.cljs:4:1` |
| `app.subs/reg-sub@:orders/count` | `src/app/subs.cljs:7:1` |

There are no `<top>@L…` impls for these forms any more; code inside a registration is scoped to its impl.

## Edges

| From | Kind | To | Certainty | Why |
|---|---|---|---|---|
| `app.views/order-list` | calls | `app.subs/reg-sub@:orders/all` | likely | `subscribe` |
| `app.views/submit-button` | calls | `app.events/reg-event-fx@:order/submit` | likely | `dispatch` inside an inner `fn` |
| `app.views/local-by-alias` | calls | `app.events/reg-event-db@:app.events/local` | likely | `dispatch-sync [::ev/local]` |
| `app.views/local-by-name` | calls | `app.events/reg-event-db@:app.events/local` | likely | fully qualified keyword |
| `app.events/reg-event-fx@:order/submit` | calls | `app.events/reg-cofx@:now` | likely | `inject-cofx :now` |
| `app.events/reg-event-fx@:order/submit` | calls | `app.events/reg-event-db@:order/submitted` | likely | `:dispatch` effect |
| `app.events/reg-event-fx@:order/submit` | calls | `app.events/reg-event-db@:order/log` | likely | `:fx [[:dispatch …]]` |
| `app.events/reg-event-fx@:order/batch` | calls | `app.events/reg-event-db@:order/submitted` | likely | `:dispatch-n` |
| `app.events/reg-event-fx@:order/batch` | calls | `app.events/reg-event-db@:order/log` | likely | `:dispatch-n` |
| `app.subs/reg-sub@:orders/count` | calls | `app.subs/reg-sub@:orders/all` | likely | `:<-` signal |
| `app.views/alpha-log` | calls | `app.events/reg-event-db@:order/log` | likely | `re-frame.alpha/dispatch` |
| `app.events/reg-event-fx@:order/notify` | calls | `app.events/reg-fx@:app/notify` | likely | effect-map key `:app/notify` (40:6) and `:fx` tuple (41:6), one edge, two sites |
| `app.system/config` | references | `integrant.core/init-key@:app/db` | possible | `ig/ref :app/db` |
| `app.system/config` | references | `integrant.core/halt-key!@:app/db` | possible | `ig/ref :app/db` |

## Holes and blind spots

| Where | Result |
|---|---|
| `app.events/dispatch-computed` `(rf/dispatch evt)` | `hole:dynamic-resolve@src/app/events.cljs:27:3`, certain `dynamic` call, blind spot |
| `app.events/dispatch-computed-key` `(rf/dispatch [k])` | `hole:dynamic-resolve@src/app/events.cljs:30:3`, same |
| `app.events/dispatch-unregistered` `(rf/dispatch [:no/handler])` | no edge; an `unresolved-symbol` blind spot naming `:no/handler` |
| `app.events/reg-event-fx@:order/forward` `{:dispatch evt}` | `hole:dynamic-resolve@src/app/events.cljs:45:6` (at the key) |
| `[:db nil]` in `:fx`, `:db` keys in effect maps | nothing: an unregistered effect key is data, not a blind spot |

## Must not appear

- Any `keyword-dispatch` edge to `app.system/…@:app/handler` (nothing refs it).
- A `certain` `keyword-dispatch` edge.
