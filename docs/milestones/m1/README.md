# Milestone M1: first proof

Recorded 2026-11-12. Al Dente 0.1.0, clj-kondo 2026.10.26, release build on Linux.

Reproduce:

```sh
cargo run --release -p acceptance          # clone, analyze, check facts (needs network)
cd ui && node scripts/milestone.mjs        # screenshots and analyze --json (after the line above)
```

| Repository | Kind | Commit | Units | Files | Parse rate | Fetch | Analyze | Facts |
|---|---|---|---|---|---|---|---|---|
| [ring-clojure/ring](https://github.com/ring-clojure/ring) | Clojure, 9 modules | `5f745bb` | 9 | 72 | 100% | 1.4 s | 1.1 s | 7/7 |
| [metosin/reitit](https://github.com/metosin/reitit) | Clojure and `.cljc`, 37 units | `1d2fc67` | 37 | 131 | 100% | 23 s | 3.7 s | 3/3 |
| [day8/re-frame](https://github.com/day8/re-frame) | ClojureScript, with examples | `1a1bf1d` | 5 | 70 | 100% | 40 s | 1.7 s | 5/5 |

Facts are listed in `acceptance/repos.toml`; they were read from each repository's source, not from Al Dente's output.

Files per repository:

- `<name>-analyze.json`: `aldente analyze --json` (counts by node and edge kind, blind spots, diagnostics);
- `<name>-acceptance.json`: the runner's report (timings, parse rate, summary);
- `<name>-1-overview.png`: the GUI's first screen (unit level, since each has several units);
- `<name>-2-drill.png`: after expanding a unit and a namespace and selecting a symbol (details, callers, callees, source).
