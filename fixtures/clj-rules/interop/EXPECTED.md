# clj-rules/interop: expected edges (R9, WP2.6)

Every edge is certain, reason `interop`, to a `platform` node. Functions are
in `clj:.//`.

| From | Edge | To | Case |
|---|---|---|---|
| `io.jvm` (namespace) | references | `java:java.util.ArrayList`, `java:java.util.UUID`, `java:java.io.File` | imported classes |
| `io.jvm/static-call` | calls | `java:java.lang.Math#abs` | static method |
| `io.jvm/ctor` | calls | `java:java.util.ArrayList` | constructor `Class.` |
| `io.jvm/ctor-new` | calls | `java:java.io.File` | constructor `new` |
| `io.jvm/hinted` | calls | `java:java.lang.String#trim` | instance method, hinted parameter |
| `io.jvm/hinted-inline` | calls | `java:java.lang.String#length` | instance method, hinted argument |
| `io.jvm/unhinted` | calls | `java:?#close` | instance method, class unknown |
| `io.jvm/uuid` | calls | `java:java.util.UUID#randomUUID` | static method on an imported class |
| `io.jvm/cmp` | references | `java:java.util.Comparator` | `proxy` |
| `io.web/log` | calls | `js:console.log` | `js/` global |
| `io.web/method` | calls | `js:?.focus` | method call |
| `io.web/prop` | references | `js:?.value` | property access |
| `io.web/fmt` | calls | `js:goog.string.format` | `goog` namespace |
| `io.web` (namespace) | requires | `js:goog.string` | `goog` require |

No edges to `clojure.core/.`, `cljs.core/.` or `clojure.core/new` (the
interop forms themselves). Type hints (`^String`) produce no edge of their
own. Each distinct platform target is one `interop` blind spot.
