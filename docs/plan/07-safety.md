# Safety

The promise: pointing Al Dente at a stranger's repository cannot run that repository's code, cannot write outside Al Dente's cache, and cannot send anything off the machine. This document is the checklist that makes the promise true. Each item has a test.

## Threat model

The analyzed repository is controlled by an attacker. They control file names, file contents, symlinks, git metadata inside the repository, build files and tool configuration. They do not control the user's machine, the user's Al Dente config, or the Al Dente binary.

## Acquisition

Remote targets are cloned into `<cache>/aldente/repos/<host>/<owner>/<name>/`, never into or near the user's projects. Run `git` as an argument vector with:

| Setting | Purpose |
|---|---|
| `GIT_CONFIG_GLOBAL` and `GIT_CONFIG_SYSTEM` pointing at an empty file; `GIT_CONFIG_NOSYSTEM=1` | The user's aliases, filters and credential helpers are not applied to hostile content. Authentication for private repositories: allow the user to opt back in with `--use-git-config`. |
| `--template=<empty dir>` and `-c core.hooksPath=<empty dir>` | No hooks exist or run. |
| `--no-recurse-submodules`, `-c submodule.recurse=false` | Submodules are not fetched. They are listed as blind spots. |
| `GIT_LFS_SKIP_SMUDGE=1`, `-c filter.lfs.smudge=`, `-c filter.lfs.process=`, `-c filter.lfs.required=false` | No filter programs run. |
| `-c core.symlinks=false` | Symlinks are checked out as small text files, so they cannot point out of the tree. |
| `-c protocol.allow=never -c protocol.https.allow=always -c protocol.ssh.allow=always` | No `file://`, `ext::` or other transports. |
| `-c core.fsmonitor=false`, `-c core.autocrlf=false`, `-c core.longpaths=true` | No monitor program; stable content hashes; Windows path length. |
| `GIT_TERMINAL_PROMPT=0`, `GIT_ASKPASS` unset | Never hangs waiting for input. |
| `--depth 1 --single-branch` by default | Small and fast. `churn` fetches history on demand with the same hardening. |

URL validation before any process starts: accept `https://`, `ssh://` and `git@host:path` forms only; reject anything beginning with `-`; reject embedded credentials in output and logs.

After clone, never run another git command with the clone as the working directory without the same environment, because a repository's own `.git/config` is only safe when written by a hardened clone. For **local** targets, read files directly and call git only for `rev-parse`, `ls-files`, `diff` and `log`, each with `-c core.fsmonitor=false -c core.hooksPath=<empty dir>` and never any command that runs filters, hooks or aliases.

## Reading files

- Walk with symlink following off. A symlink is recorded and skipped.
- Canonicalize every path before opening and verify it is inside the root.
- Skip files over a size cap (default 2 MB), binary files, and directories on a deny list (`.git`, `node_modules`, `target`, `.cpcache`, `.shadow-cljs`, `dist`, `out`). Skipped source-like files are diagnostics.
- Cap total files and total bytes per run, with a clear message when hit.
- Archives are never unpacked.

## Tools

- **clj-kondo:** always with `--config-dir <empty dir>` and `--cache false`, so the target's `.clj-kondo` configuration and hooks are not loaded. Argument vector, timeout, working directory set to a temp directory, not the repository.
- **Build files are data.** `deps.edn`, `shadow-cljs.edn` and `bb.edn` are parsed with the EDN reader. `project.clj` is read as forms and never evaluated; unquote, `eval` or `read-eval` in it is recorded as a vet finding. `package.json` is parsed as JSON; its scripts are reported, never run.
- **No resolution tools.** `clojure -Stree`, `lein deps`, `npm install`, `pip` are never invoked in static mode. Phase 7 reads lockfiles and local caches instead.
- The local Maven cache is read-only input: list jar entries and read POM files. Jars are never put on a classpath.

## The execution gate

Anything that runs repository code (Phase 11: measured coverage) goes through one function, `ExecutionGate::authorize`, and nothing else spawns repository-defined commands.

- Requires `--allow-exec` on the command line. No config file can set it.
- If the snapshot's origin is a remote URL, or the path is under the Al Dente cache, also requires `--i-trust-this-repo` and prints the exact command, working directory and the count of unresolved vet findings before running. With no terminal attached and no `--yes`, it refuses.
- The command run is shown in full. It is spawned as an argument vector with a scrubbed environment (no tokens, no cloud credentials; an allow list of variables).
- Recommend, and document, running inside a container or VM. Al Dente does not claim to sandbox.
- Refusal exits with code 6.

## The local server

- Bind `127.0.0.1` only (and `::1`). No flag exposes it on other interfaces in the first release.
- A random token is generated at start and embedded in the URL the browser opens; the UI sends it as a header. Requests without it get `401`.
- Check the `Host` header against `127.0.0.1:<port>` and `localhost:<port>` to block DNS rebinding. Reject requests with an `Origin` that is not the server's own. No CORS headers.
- `Content-Security-Policy: default-src 'self'; connect-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'`. The UI loads nothing from any other origin: fonts and libraries are bundled.
- Repository strings reach the DOM only as text. No `dangerouslySetInnerHTML`, no Markdown rendering of docstrings in the first release.
- `/source` serves only files recorded in the snapshot, by node or site ID, not by path.
- `/open` runs only the user-configured editor command (see `06-gui.md`).
- The server is read-only with respect to the repository.

## Output handling

- Terminal output strips control characters and escape sequences from repository strings.
- DOT, Mermaid and GraphML writers escape labels.
- SQL uses bound parameters only.

## Network

- The only outbound connections are the clone or fetch the user asked for and, in Phase 7, `deps resolve --online` to fixed registry hosts over HTTPS, fetching metadata files only.
- No telemetry, no update checks.

## Required tests

Under `fixtures/hostile/`, each asserting nothing was executed or escaped:

1. a `.clj-kondo` directory with a hook that would create a marker file;
2. a symlink to a file outside the repository;
3. a file name containing shell metacharacters, newlines and terminal escapes;
4. a docstring containing `<script>` and an `onerror` attribute, checked in a browser test;
5. a `project.clj` with `#=(...)` read-eval and unquote forms;
6. a `.gitattributes` declaring a filter, and a `.gitmodules` with a `file://` URL (clone test against a local bare repository);
7. a symbol name crafted as a SQL fragment and as a regex;
8. a 50 MB single-line file and a deeply nested form;
9. a request to the server with a wrong `Host` header, a missing token, and a foreign `Origin`.
