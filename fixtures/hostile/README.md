# Hostile fixtures

Each case is a small repository that tries to make Al Dente execute code, escape the repository, or corrupt output. The cases come from "Required tests" in `docs/plan/07-safety.md`. Each test asserts that nothing ran and nothing escaped. A case is added by the work package that first needs it; this table records which.

| # | Case | Added by | Status |
|---|---|---|---|
| 1 | `.clj-kondo` directory with a hook that would create a marker file or rewrite a call | WP1.3 | todo |
| 2 | symlink to a file outside the repository | WP1.1 | todo |
| 3 | file name with shell metacharacters, newlines and terminal escapes | WP1.1 | todo |
| 4 | docstring containing `<script>` and an `onerror` attribute (browser test) | WP1.9 | todo |
| 5 | `project.clj` with `#=(...)` read-eval and unquote forms | WP1.1 | todo |
| 6 | `.gitattributes` declaring a filter, and `.gitmodules` with a `file://` URL (clone against a local bare repository) | WP1.2 | todo |
| 6a | local repository whose `.git/config` sets `diff.external`, a textconv driver and `core.pager` | WP1.1 | todo |
| 7 | symbol name crafted as a SQL fragment and as a regex | WP1.6 | todo |
| 8 | 50 MB single-line file and a deeply nested form | WP1.1 / WP1.4 | todo |
| 9 | server requests with a wrong `Host`, a missing token and a foreign `Origin` | WP1.8 | todo |

"Added by" is a best guess from the phase files; move a case if another work package needs it first.

Never run anything from these directories. Tests point Al Dente at them and check the results.
