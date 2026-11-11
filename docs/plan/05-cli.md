# CLI

Binary name: `aldente`. The CLI is the primary interface; the GUI and the Emacs layer use the same engine and the same JSON.

## Conventions

- **Target.** Commands take `[TARGET]`: a local path or a git URL. Default: the current directory's repository root. `--ref <rev>` selects a commit, tag or branch for URLs.
- **Formats.** `--format text|json|dot|mermaid|graphml|csv` where sensible; `--json` is shorthand for `--format json`. Text is the default on a terminal.
- **JSON envelope.** Identical for every command:

```json
{
  "schema": "aldente/crater/v1",
  "ok": true,
  "snapshot": { "key": "b3:9f2c…", "repo": "…", "rev": "a1b2c3d", "dirty": false, "analyzer": "0.3.0" },
  "data": { },
  "warnings": [ { "code": "truncated", "message": "…" } ],
  "error": null
}
```

- **Determinism.** `data` is sorted and contains no timestamps or absolute paths. Two runs on the same snapshot are byte-identical.
- **Streams.** Results on stdout; progress, logs and prompts on stderr. `--quiet` silences stderr progress. Honors `NO_COLOR`.
- **Locations** print as `path:line:col`, which Emacs compilation mode and most editors recognize.
- **Exit codes.** `0` success. `1` internal error. `2` usage error. `3` ambiguous or unknown symbol. `4` findings at or above `--fail-on` (for `vet`, `deps`, `cycles`). `5` required tool missing. `6` execution gate refused.
- **Common filters.** `--min-certainty certain|likely|possible` (default `possible`), `--unit <id>` (repeatable), `--include-tests/--no-tests`, `--include-external/--no-external`, `--limit N`.
- **Symbols** may be given as full IDs or unambiguous suffixes (see `03-graph-model.md`).

## Commands

### Core (Phase 1)

| Command | Does |
|---|---|
| `aldente analyze [TARGET] [--no-cache] [--lang clj,…]` | Builds or reuses the snapshot. Prints counts by node and edge kind, units found, blind-spot counts, diagnostics. |
| `aldente serve [TARGET] [--port N] [--no-open]` | Starts the local server and opens the GUI. |
| `aldente find <pattern> [--kind …] [--regex]` | Searches symbols and namespaces. Substring by default. |
| `aldente show <symbol>` | Details: kind, location, signature, doc, direct callers and callees grouped by certainty, measures when computed. |
| `aldente graph [--level unit\|namespace\|symbol] [--focus <id> --depth N] [--direction out\|in\|both]` | Emits a graph or a neighborhood. Refuses a whole-repository symbol graph above the payload cap unless `--limit 0`. |
| `aldente units` | Lists units, their languages, source roots, service flag. |
| `aldente blindspots [--category …]` | Lists everything the analyzer could not see through. |
| `aldente doctor` | Checks `git`, `clj-kondo` (and versions), cache directory, platform quirks. |
| `aldente cache ls\|gc\|clear` | Manages snapshots and remote clones. |
| `aldente schema [name]` | Prints JSON Schemas for the envelope and every `data` shape. |

### Crater, traces, libraries (Phase 3)

| Command | Does |
|---|---|
| `aldente crater <symbol>… [--direction forward\|reverse\|both] [--depth N] [--stop-at <pattern>]` | The impact subgraph. `--checklist` prints the reverse crater as a flat list of places to verify, one location per line, with coverage status once Phase 4 lands. |
| `aldente crater --trace <file\|->` | Pins the crater to the path in a stack trace. |
| `aldente crater --diff <rev>[..<rev>]` | Seeds the crater from every symbol whose source range changed between revisions. |
| `aldente trace normalize <file\|-> [--source-map <path>…]` | Converts a raw trace to canonical frames, with each frame mapped to a node when possible. |
| `aldente lib ls` | Libraries in use, with usage counts. |
| `aldente lib usage <library> [--by namespace\|symbol]` | Every place the library is invoked. |

### Measures and coverage (Phase 4)

| Command | Does |
|---|---|
| `aldente metrics [--level …] [--metric pagerank,betweenness,…] [--top N]` | Graph measures as a table. |
| `aldente hotspots [--top N]` | Ranked findings: god objects, hubs, bottlenecks, heavily used and untested code, each with the reason. |
| `aldente cycles [--level namespace\|symbol\|unit]` | Strongly connected components larger than one, with a suggested edge to cut. |
| `aldente communities [--level …]` | Detected communities and how they differ from the namespace layout. |
| `aldente deadcode` | Symbols unreachable from any entry point or test. |
| `aldente coverage [--import <report>]` | Static coverage estimate per symbol and namespace; merges a measured report when given. |
| `aldente churn [--since <date>]` | Change frequency and co-change coupling from git history. |

### Outside world (Phase 6)

| Command | Does |
|---|---|
| `aldente vet [TARGET] [--fail-on low\|medium\|high]` | The security report. Works on a URL before anything is run. |
| `aldente surface [--kind ingress\|egress\|internal\|datastore]` | The deployment view as data. |

### Dependencies (Phase 7)

| Command | Does |
|---|---|
| `aldente deps tree [--unit …]` | The version-aware dependency graph. |
| `aldente deps conflicts [--fail-on …]` | Conflicts, diamonds, duplicates, risky constraints, drift across units. |
| `aldente deps why <library>` | Every path by which a library and version arrives. |
| `aldente deps resolve --online` | Opt-in metadata fetch for transitive resolution. |

### State machines (Phase 8)

| Command | Does |
|---|---|
| `aldente fsm ls` | Inferred machines with their subject and certainty. |
| `aldente fsm show <id>` | States, transitions, and the code behind each transition. |

### Opt-in execution (Phase 11)

| Command | Does |
|---|---|
| `aldente coverage measure --allow-exec [--cmd …]` | Runs the repository's tests under a coverage tool, behind the execution gate in `07-safety.md`. |

### Supporting commands (added in the phases shown)

| Command | Phase | Does |
|---|---|---|
| `aldente rules ls\|check <file>` | 6 | Lists loaded rule catalogs; validates a user catalog. |
| `aldente import lena <file\|->` | 10 | Imports a Leña EDN fact stream as a low-certainty graph. |
| `aldente analyze --adapter-cmd <argv>` | 10 | Runs a user-configured external adapter speaking the JSON Lines protocol. |
| `aldente serve --focus <id>` | 11 | Opens the GUI centered on one node (used by the Emacs layer). |

## Text output

Terse and aligned, built for a terminal panel. Example:

```text
$ aldente crater app.orders/submit! --direction reverse --checklist
reverse crater of clj:.//app.orders/submit!   depth 6   27 symbols   9 untested
certain
  src/app/http/routes.clj:88:3     app.http.routes/post-order        tested
  src/app/jobs/retry.clj:41:5      app.jobs.retry/retry-order        UNTESTED
likely
  src/app/events.cljs:120:7        app.events/<reg-event-fx>@:order/submit   UNTESTED
possible
  …
```

## Driving it from Claude Code and Emacs

- Every command's `--json` output validates against `aldente schema <name>`.
- Errors are JSON too when `--json` is set: `ok: false`, `error: { code, message, candidates? }`.
- `aldente show` and `aldente crater --checklist` are the two commands an agent needs most; keep their `data` shapes small and stable.
