# aldente CLI

User documentation. Every command accepts `--json`, which wraps the result in the envelope described by `aldente schema envelope`. Exit codes: `0` success, `1` internal error, `2` usage error, `3` ambiguous or unknown symbol, `4` findings at or above `--fail-on`, `5` required tool missing, `6` execution refused.

| Command | Does |
|---|---|
| `aldente doctor` | Checks that `git` (2.30.0 or newer) and `clj-kondo` (2026.10.26 or newer) are on `PATH`, that the cache directory (`<user cache dir>/aldente`) is writable, and reports the platform. Prints an install hint for anything missing or too old and exits `5` if any check fails. With `--json`, `data` is the full report (schema `doctor`), also when `ok` is false. |
| `aldente units [TARGET]` | Lists build units (directories with `deps.edn`, `project.clj`, `shadow-cljs.edn` or `bb.edn`), their languages, source and test roots (with the alias or profile that adds them), the service flag and the number of source files each owns, then everything the file walk skipped and why. `TARGET` defaults to `.`; a path inside a git repository means the whole repository. Build files are read as data, never run. With `--json`, `data` follows schema `units`. |
| `aldente schema [name]` | Prints the JSON Schema called `name` (`doctor`, `envelope`, `units`), or lists the names when `name` is omitted. `doctor` describes `data` only. With `--json`, `data` holds the schema or the list. |
