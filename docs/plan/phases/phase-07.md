# Phase 7. Dependency graph

Goal (M7): a version-aware graph of dependencies and a list of the problems in it.

Read first: `11-dependencies.md`, `07-safety.md` (tools and network sections).

## WP7.1 Manifest and lockfile parsers

**Build** (`aldente-analysis::deps`)
- `deps.edn` (deps, aliases, `:local/root`, git deps, `:mvn/repos`, exclusions), `project.clj` read as data (dependencies, profiles, managed dependencies, exclusions, repositories, plugins), `shadow-cljs.edn`, `bb.edn`, `package.json`, `package-lock.json` (v2 and v3), `yarn.lock` (classic and Berry), `pnpm-lock.yaml`.
- Every declaration keeps file and line.
- Version types: Maven ordering, semver with ranges.

**Accept**
- Fixture manifests for each format, including awkward real-world shapes (Leiningen profiles with `^:replace`, `deps.edn` aliases overriding versions, workspaces in `package.json`).
- Version comparison is tested against published ordering examples for Maven and semver.
- `project.clj` containing code forms is parsed for its data parts and the code forms are reported, never evaluated.

## WP7.2 Transitive graph without running tools

**Build**
- POM reader (dependencies, dependency management, parent POMs, properties, scopes, optional, exclusions) over the local Maven cache; `~/.gitlibs` and `:local/root` project manifests; npm trees from lockfiles.
- `incomplete` reporting when metadata is missing.
- `aldente deps resolve --online`: HTTPS GET of POM files and npm registry metadata from a fixed host list into the Al Dente cache, with a size cap and no redirects to other hosts.

**Accept**
- Tests use a fake Maven cache directory and a local HTTP server standing in for registries; no real network in tests.
- Without metadata the graph stops at declared dependencies and says exactly which coordinates are missing.
- `--online` contacts only allow-listed hosts (asserted) and never downloads a jar.

## WP7.3 Resolution and findings

**Build**
- Selected-version computation per tool rule (Clojure CLI, Leiningen/Maven, lockfile).
- Every finding in the document's table, with paths, the selected version and a suggested fix; dominator-based "pin or exclude here" advice; cross-unit drift; unused-declared and undeclared-use by joining with library attribution.
- `rules/dep-aliases.toml` for known relocations.

**Accept**
- `fixtures/deps/hell/`: a constructed set of manifests and POMs producing one of each finding. Golden output.
- On a real project with a known `clojure -Stree` output stored as a text file in the fixture (generated once by hand and documented), the selected versions match.

## WP7.4 Commands and view

**Build**
- `aldente deps tree|conflicts|why|resolve`; `--fail-on`.
- Blast radius: `aldente crater lib:…@version --direction reverse` through `provides`.
- GUI Dependencies view: library graph with conflicts marked, per-library panel (versions, paths, using code, units), drift table across units.

**Accept**
- Golden text and JSON on the fixture.
- Playwright: open a conflict, see both paths, jump to the code using the library.

## WP7.5 Milestone M7 acceptance

**Accept**
- On Austin's multi-service repository: the drift table and conflict list are reviewed with him; at least the conflicts he already knows about appear.
- Selected versions for one service are compared with the output of his real build tool, which he runs himself and pastes in.
