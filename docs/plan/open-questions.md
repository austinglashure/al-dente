# Open questions for Austin

None of these block Phase 0 or Phase 1. Each says when an answer is needed.

## Needed before Milestone M1 is judged

1. **Test repositories.** URLs (or paths) and commits for the Clojure and ClojureScript repositories that define success. Ideally one small, one large multi-service, one ClojureScript front end with re-frame or similar. For each, ten to twenty facts known to be true ("`a.b/c` is called from `d.e/f`", "this protocol has four implementations").

   Stand-ins until then, pinned by commit in `acceptance/repos.toml`:

   | Repository | Why |
   |---|---|
   | `ring-clojure/ring` | Small, classic; already among Leña's fixtures. |
   | `metosin/reitit` | Multi-module Clojure and `.cljc`; protocols and records. |
   | `day8/re-frame` (with its example apps) | ClojureScript with keyword dispatch. |
   | `penpot/penpot` | Large monorepo: Clojure back end, ClojureScript front end, several services. |
   | `metabase/metabase` | Large; multimethod-heavy; Clojure plus a TypeScript front end for Phase 10. |

## Needed before Phase 1 starts (only if the answer is "no")

2. **Leña relationship (decision D1).** The plan builds Al Dente as a separate project and does not depend on Leña's core. If working inside or on top of the coworker's project matters for reasons beyond the technical, say so before Phase 1.

## Needed before Phase 6

3. **Security detection review.** Read `10-security-and-deployment.md` and answer the four questions at its end.

## Needed before Phase 11

4. **Editor command.** Confirm `emacsclient -n +LINE:COL FILE` as the default template for "open in editor".
5. **Measured coverage tool.** Cloverage for Clojure is assumed. For ClojureScript, name the test runner in use (shadow-cljs with Karma, Kaocha-cljs, other).
6. **Distribution.** Prebuilt binaries on GitHub Releases plus `cargo install` is assumed. Say if Homebrew, Scoop or Nix matter.

## Decide whenever

7. **Project name on disk.** Binary `aldente`, crates `aldente-*`. Change before the first release if a different spelling is wanted.
8. **Open-sourcing.** Nothing in the plan blocks it. Before publishing: add `CONTRIBUTING.md`, decide on a contributor agreement (AGPL projects often want one if relicensing is ever a possibility), and confirm with Leña's author that the acknowledgement in the README is welcome.
