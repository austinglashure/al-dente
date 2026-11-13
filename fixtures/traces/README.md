# Stack trace fixtures

Test data for the trace normalizer (WP3.3) and trace-pinned craters (WP3.5).
Nothing here is ever run by Al Dente; the generator scripts are for people
regenerating the data.

## `jvm/`

Real JVM traces from Clojure 1.12.0 on OpenJDK 17, produced by running the
small program in `jvm/gen/src/corpus/` with `jvm/gen/gen.sh`:

| File | What it shows |
|---|---|
| `01-plain.txt` | One exception, `printStackTrace` |
| `02-nested-causes.txt` | Two `Caused by:` levels with `... N more` |
| `03-ex-info.txt` | `ex-info` wrapping `ex-info`; data printed in the message |
| `04-lazy-seq.txt` | Failure while a lazy sequence is realized |
| `05-future.txt` | `future` and thread-pool frames, `ExecutionException` cause |
| `06-anonymous.txt` | A named local fn (`outer$inner__159`) |
| `07-anonymous-literal.txt` | `fn` and `#()` literals nested in a defn |
| `08-protocol-record.txt` | A `defrecord` method (`corpus.core.Card.charge`) |
| `09-protocol-extend.txt` | An `extend-type` method and the protocol dispatch fn |
| `10-multimethod.txt` | A `defmethod` body through `MultiFn.invoke` |
| `11-munged-names.txt` | Names with `?`, `!`, `->`, `*`, `<=>`, `+`, `%`, `&`, `'` |
| `12-repl.txt` | A REPL session: "Execution error" line and `clojure.repl/pst` |
| `13-test-runner.txt` | `clojure.test` ERROR report (`clojure.stacktrace` format) |
| `14-throwable-map.edn` | `Throwable->map` |
| `15-printed-error.edn` | A printed `#error {…}` |
| `16-main-report.edn` | `clojure.main`'s error report (`-Dclojure.main.report=stderr`) |
| `17-truncated.txt` | **Hand-cut** from `02`: stops mid-cause, last frame cut mid-line |
| `18-log-interleaved.txt` | **Hand-made** from `01` and `02`: log lines between frames, two traces in one log |

`jvm/gen/deps.edn` lets tests analyze the corpus source, so its frames can
be checked against real definitions.

## `jvm/clj-mini/`

Traces produced by `fixtures/clj-mini`'s own code (`jvm/clj-mini/gen.clj`,
run with `jvm/clj-mini/gen.sh`). Every `shop.*` frame must map `exact`.
`run-script-eval.*` reaches `half-off` through `eval`, a call the static
graph cannot see (WP3.5's missing-edge report).
