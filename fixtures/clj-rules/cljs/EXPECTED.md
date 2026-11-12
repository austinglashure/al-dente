# clj-rules/cljs: expected (R12 and the shadow-cljs part of R13, WP2.7)

IDs in `clj:.//`.

| Expectation | Detail |
|---|---|
| one node per `.cljc` definition | `app.shared/now` and `app.shared/both` each one node, langs clj and cljs |
| per-language edges | `app.shared/now -calls-> java:java.lang.System#currentTimeMillis` with lang clj; `-calls-> js:Date.now` with lang cljs |
| an edge in both languages is stored once | `app.shared/both -calls-> app.shared/now`, no lang (both) |
| npm string require | `lib:npm:react -provides-> ext:clj//react`, certain; `app.main -requires-> ext:clj//react` |
| external namespace | `app.main -requires-> ext:clj//reagent.core` |
| `:require-macros` | `app.main/init -calls-> app.macros/when-dev` (the macro symbol) |
| self-require of macros | `app.ui/render -calls-> app.ui/widget` (macro defined in `ui.clj`, used in `ui.cljs`) |
| the self-require is a real edge | `app.ui -requires-> app.ui` (`:require-macros [app.ui]` in `ui.cljs`) |
| shadow-cljs entry points | `app.main/init` (`:init-fn`) and `app.ui/render` (`:exports`) are flagged `entry-point` |
