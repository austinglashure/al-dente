# Dependency graph

Covers use case 9: a version-aware graph of the dependencies themselves, and graph techniques to root out dependency hell.

## Model

Extends the base graph.

- `library` nodes gain per-version children: `lib:maven:reagent/reagent@1.2.0`. The unversioned node is the parent.
- `depends` edges: unit → library version (declared), library version → library version (transitive). Attributes: `constraint` as written, `scope` (`compile`, `dev`, `test`, `provided`, `peer`, `optional`), `declared_in` (file and line), `via` (alias or profile), `exclusions`, `source` (`manifest`, `lockfile`, `local-cache`, `registry`).
- A resolved view per unit: the version each build tool would select, with the rule that selected it.

## Sources, in order of trust

| Ecosystem | Declared | Transitive without running anything |
|---|---|---|
| Clojure CLI | `deps.edn` (`:deps`, aliases, `:local/root`, git deps) | POM files in the local Maven cache; `:local/root` projects' own `deps.edn`; cached git deps under `~/.gitlibs` |
| Leiningen | `project.clj` read as data (`:dependencies`, profiles, `:managed-dependencies`, `:exclusions`) | same |
| shadow-cljs | `shadow-cljs.edn` `:dependencies`, or delegation to `deps.edn` or Leiningen | same, plus npm below |
| npm, Yarn, pnpm | `package.json` | `package-lock.json`, `yarn.lock`, `pnpm-lock.yaml` give the full resolved tree |
| Python (Phase 10) | `pyproject.toml`, `requirements*.txt`, `setup.cfg` | `poetry.lock`, `uv.lock`, `Pipfile.lock`, pinned requirements |

If transitive metadata is missing locally, the graph stops at the declared level and says so (`incomplete: true`, listing what is missing). `aldente deps resolve --online` then fetches POM and registry metadata over HTTPS from Maven Central, Clojars and the npm registry only, into Al Dente's cache. It downloads metadata, never artifacts, and runs no tool. It is opt-in because it is a network call.

## Resolution semantics

Model each tool's rule so the "selected" version matches what the build would do.

- **Clojure CLI (tools.deps):** top-level declarations win; otherwise the newest version among the candidates, with exclusions applied.
- **Leiningen and Maven:** nearest declaration wins; ties go to first declared; `:managed-dependencies` pin.
- **npm family:** take the lockfile as truth; duplicates at different versions are real and counted.

Version comparison: Maven ordering rules for Maven coordinates (qualifiers, `SNAPSHOT`), semver for npm, PEP 440 later.

## Findings

| Finding | Definition |
|---|---|
| Version conflict | Two or more versions of one library requested in a unit's graph. Report all paths, the selected version, and whether the selection downgrades what some path asked for (the dangerous case). |
| Major-version conflict | A conflict whose versions differ in major number (or pre-1.0 minor). Higher severity. |
| Diamond | A library reached by more than one path. Informational unless versions differ. |
| Duplicate | Several versions of one package installed side by side (npm), or the same classes shipped under different coordinates (known renames listed in `rules/dep-aliases.toml`, for example relocated artifacts). |
| Cross-unit drift | The same library at different selected versions in different units of the monorepo. For twenty services sharing code this is often the most useful list. |
| Risky constraint | `SNAPSHOT`, `LATEST`, `RELEASE`, open ranges, npm `*` or `latest` or git URLs without a commit, git deps without a full SHA, `:local/root` leaving the repository, unpinned Python requirements. |
| Unsatisfiable | Ranges or peer requirements with an empty intersection; an exclusion that removes something still required. |
| Unused declared dependency | Declared in the unit, no namespace from it referenced in the unit's code (uses rule R10; `likely`, since runtime-only and plugin dependencies exist). |
| Undeclared use | Code uses a namespace whose library arrives only transitively. |
| Dependency cycle | A strongly connected component among libraries or between units. |

## Graph techniques applied

- Strongly connected components for cycles.
- Reverse reachability for `deps why`: every path from a unit to a library version.
- Dominators over the dependency graph: which single direct dependency is responsible for pulling in a problem transitive one, and so where an exclusion or pin belongs.
- Centrality on the library graph: the transitive dependencies most of the system rests on.
- Blast radius, joined with the code graph: for a library version, the reverse crater through `provides` into repository code. "If I bump this, what code is affected" combines use cases 4 and 9.

## Output

`aldente deps tree` prints an indented tree with the selected version and markers for omitted or conflicting nodes. `deps conflicts` prints findings with paths and a suggested fix (pin at top level, add an exclusion at the dominator, align units). `--fail-on` enables CI use. The GUI shows the library graph with conflicts marked and a per-library panel listing versions, paths and using code.

## Not in scope

Vulnerability lookup. It needs an online database and is a different product. The model leaves room: a later command could annotate library versions from OSV data, behind the same online opt-in.
