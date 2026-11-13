# test-helpers: what counts as a test for coverage (ground truth)

R14 makes `deftest` vars (and the forms in `rules/clj-tests.toml`) tests.
Coverage starts from everything in a test namespace that runs when the
tests load or run: tests, top-level forms, `def` values, and method bodies
(`reify`, records) written there. Helper functions (`defn`, `defmacro`) in a
test namespace are test support: they run only when called, get no coverage
status, and a test's reach passes through them, so a helper no test calls
covers nothing (decided 2026-11-13 after M4: "test recognition feels wonky").

| Symbol | Status | Why |
|---|---|---|
| `app.core/run` | direct | `run-works` calls it |
| `app.core/build` | indirect (distance 2) | `run-works` → `make` → `build` |
| `app.core/orphan` | untested | only `unused-helper` calls it, and nothing calls that |
| `app.core/load-config` | direct | the value of `(def config …)`, run at load |
| `app.core/register!` | direct | a top-level form, run at load |
| `app.core/send!` | direct | the `reify` method written inside `listener-works` |

Tests: 2 (`run-works`, `listener-works`). Nothing in `app.core-test` gets a status.
