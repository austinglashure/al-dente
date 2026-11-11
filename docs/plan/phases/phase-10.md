# Phase 10. Python and TypeScript

Goal (M10): Git repositories built on Python and TypeScript produce the same graph, with the same certainty model, and the analyses built so far work on them.

Read first: decision D11, `03-graph-model.md`, `04-clojure-analysis.md` (as the pattern to mirror).

Principle from the handoff: types make inference easier, and optional types are **evidence, not fact**. A Python type hint raises an edge from `possible` to `likely`, never to `certain`. TypeScript's declared types can make an edge `certain` only when the target is statically unique (a direct import, a non-overridden method on a final or non-exported class); `any`, unions, interfaces with several implementers and structural matches stay `likely` or `possible`.

## WP10.1 Spike and adapter hardening

**Build**
- Two-day spike: compare Tree-sitter plus an own resolver against `oxc` (parser, semantic, resolver) for TypeScript, and Tree-sitter against `ruff`'s parser crates for Python, on parse speed, error tolerance and how much scope and import information comes for free. Write the result into `decisions.md` as D13.
- Review the `Adapter` trait and `FactBatch` against what the two languages need (classes and methods, decorators, module-level code, re-exports); extend the model if required and update `03-graph-model.md`.
- Language tags `py` and `ts` (JavaScript files are analyzed by the `ts` adapter with `langs: ["js"]`).

**Accept**
- D13 recorded. No Clojure golden changes from the model review.

## WP10.2 TypeScript and JavaScript adapter

**Build** (`aldente-ts`)
- Units from `package.json` and `tsconfig.json` (paths, baseUrl, project references), workspaces.
- Definitions: functions, classes, methods, arrow functions bound to names, exported consts, interfaces and types (as symbols for navigation).
- Import and export resolution including re-exports, default exports, barrel files, path aliases, `require`.
- Calls: direct (`certain`), method calls on `this` and on receivers with a declared or constructor-inferred class (`likely` or `certain` per the principle), interface and abstract methods to all implementers (`dispatches`, `possible` or `likely`), callbacks as `references`, parameter flow as for Clojure.
- Dynamic: `eval`, `new Function`, dynamic `import()` with non-literal, computed member calls → holes. Dynamic `import('literal')` resolves.
- Frameworks through a keyword-dispatch-style catalog: React components used in JSX are `calls` edges; Redux action types to reducers; Express and NestJS routes for Phase 6 ingress.
- Fixtures `fixtures/ts-mini/` with `EXPECTED.md`, mirroring the Clojure fixture's coverage.

**Accept**
- `EXPECTED.md` satisfied.
- On a stand-in repository (Leña's NestJS fixture excerpt and one full open-source TypeScript service), parse rate above 98 percent and expected facts hold.
- npm library attribution is `certain` from imports.

## WP10.3 Python adapter

**Build** (`aldente-py`)
- Units from `pyproject.toml`, `setup.cfg`, `setup.py` (read as text for a `packages` or `name` literal, never executed), `requirements*.txt`; source roots including `src/` layouts and namespace packages.
- Definitions: modules, functions, classes, methods, module-level assignments of callables.
- Import resolution: absolute, relative, `from … import …`, `__init__` re-exports, `__all__` literals; conditional and try/except imports take all branches.
- Calls: direct names (`certain`), `self.method` within a class and its bases (`likely`; `dispatches` to overrides in subclasses), attribute calls on receivers with a type hint or a constructor assignment in scope (`likely`), otherwise name-matched methods across the repository (`possible`, reason `name-match`, capped fan-out with the overflow recorded as a blind spot).
- Decorators as `calls` to the decorator plus `references` from decorator to function; framework decorators (Flask, FastAPI, Celery, pytest fixtures, Django URL confs) through the catalog.
- Dynamic: `eval`, `exec`, `getattr` with non-literal, `importlib.import_module` with non-literal, `__import__` → holes.
- Tests: pytest and unittest conventions.
- Fixtures `fixtures/py-mini/` with `EXPECTED.md`.

**Accept**
- `EXPECTED.md` satisfied; the type-hint principle is tested (same code with and without hints differs only in certainty).
- On a stand-in repository (Leña's FastAPI fixture excerpt and one full open-source service), parse rate above 98 percent and expected facts hold.

## WP10.4 Analyses on the new languages

**Build**
- `rules/sinks-py.toml`, `rules/sinks-ts.toml`, ingress catalogs (Flask, FastAPI, Django, Express, NestJS, Next.js routes), datastore catalogs (SQLAlchemy, Django ORM, Prisma, TypeORM, Mongoose), test catalogs.
- Dependency parsers for Python lockfiles (`poetry.lock`, `uv.lock`, `Pipfile.lock`, pinned requirements) with PEP 440 ordering.
- State-machine detector D1 patterns for `match`/`switch` on a status field with enum or literal values; XState configs under D4.
- Cross-language linking in mixed repositories: a TypeScript front end calling a Clojure or Python back end links through Phase 6's internal API matching.

**Accept**
- Vet, surface, deps, coverage, hotspots and crater each have one golden test per new language.
- A mixed fixture (ClojureScript or TypeScript front end, Python back end) shows the cross-language HTTP link.

## WP10.5 Python and Node stack traces

**Build**
- Python traceback parser (including chained exceptions and `During handling…`), Node and V8 parser with source maps for TypeScript; noise catalogs.

**Accept**
- Trace corpora per language; fixture traces map every repository frame.

## WP10.6 External adapter protocol and Leña import

**Build**
- JSON Lines protocol: one fact per line (`node`, `edge`, `literal`, `diagnostic`, `blindspot`), documented in `docs/adapter-protocol.md`, with a validator. `aldente analyze --adapter-cmd <argv>` runs a user-specified adapter (the user's own tool, from user config only).
- `aldente import lena <file|->`: reads Leña's newline-delimited EDN transactions, maps its vocabulary to Al Dente's model, marks every imported call edge `possible` with reason `name-match` unless Leña's `:call/status` says resolved (`likely`).

**Accept**
- A recorded Leña fact stream for a Go fixture imports into a browsable graph.
- Malformed lines are skipped with diagnostics; hostile strings in facts are handled like any repository string.

## WP10.7 Milestone M10 acceptance

**Accept**
- Austin points it at one Python and one TypeScript repository of his choosing; the M1 and M3 checks (graph displayed, crater on three functions hand-verified) pass for each.
