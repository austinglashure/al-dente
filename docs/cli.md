# aldente CLI

User documentation. Every command accepts `--json`, which wraps the result in the envelope described by `aldente schema envelope`. Exit codes: `0` success, `1` internal error, `2` usage error, `3` ambiguous or unknown symbol, `4` findings at or above `--fail-on`, `5` required tool missing, `6` execution refused.

| Command | Does |
|---|---|
| `aldente schema [name]` | Prints the JSON Schema called `name` (currently `envelope`), or lists the names when `name` is omitted. With `--json`, `data` holds the schema or the list. |
