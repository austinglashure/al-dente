# aldente CLI

User documentation. Every command accepts `--json`, which wraps the result in the envelope described by `aldente schema envelope`. Exit codes: `0` success, `1` internal error, `2` usage error, `3` ambiguous or unknown symbol, `4` findings at or above `--fail-on`, `5` required tool missing, `6` execution refused.

| Command | Does |
|---|---|
| `aldente doctor` | Checks that `git` (2.30.0 or newer) and `clj-kondo` (2026.10.26 or newer) are on `PATH`, that the cache directory (`<user cache dir>/aldente`) is writable, and reports the platform. Prints an install hint for anything missing or too old and exits `5` if any check fails. With `--json`, `data` is the full report (schema `doctor`), also when `ok` is false. |
| `aldente schema [name]` | Prints the JSON Schema called `name` (`doctor`, `envelope`), or lists the names when `name` is omitted. `doctor` describes `data` only. With `--json`, `data` holds the schema or the list. |
