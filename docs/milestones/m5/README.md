# Milestone M5: GUI depth and scale

Recorded 2026-11-18. Al Dente 0.1.0, release build on Linux, headless Chromium (Playwright).

Reproduce (from `ui/`, after `cargo build --release -p aldente-cli`):

```sh
node scripts/m5-walkthrough.mjs
```

## Repository

The plan asks for the largest acceptance repository. That would be penpot, but its fetch cannot finish on the network here (about 100 KB/s; two attempts hit the 600 s fetch timeout), and penpot is set aside for now. The walkthrough therefore runs on **metosin/reitit** at `1d2fc67` (37 units, 131 files), the largest acceptance repository that can be fetched. The script takes the repository's name as an argument, so another can be added to its table and recorded the same way.

## Walkthrough

Each step is driven through the real GUI against `aldente serve`. Times run from the action until its result is on screen, as recorded in `reitit-walkthrough.json`.

| # | Step | Time | Result | Screenshot |
|---|---|---|---|---|
| 1 | Land on the overview | 0.3 s | 37 units | `reitit-1-overview.png` |
| 2 | Find `resolve-routes` by search (`/`, type, Enter) | 0.05 s | 1 result | `reitit-2-search.png` |
| 3 | See its neighborhood | 0.2 s | 44 nodes | `reitit-3-neighborhood.png` |
| 4 | Prune to one unit (`modules/reitit-core`, unit filter) | 0.1 s | 44 → 6 nodes | `reitit-4-prune.png` |
| 5 | Crater it (forward) | 0.4 s | 108 nodes, 97 boundary, 2 hubs not expanded | `reitit-5-crater.png` |
| 6 | Open source | 0.2 s | `modules/reitit-core/src/reitit/impl.cljc:135:1`, highlighted | `reitit-6-source.png` |
| 7 | Jump to the editor | 0.2 s | editor started with the file, `135`, `1` | `reitit-7-editor.png` |

**Editor:** step 7 uses a stand-in editor (`ui/scripts/record-editor.mjs`, set as `editor` in a throwaway config directory) that records the arguments it was started with. This shows the GUI starts the configured editor with the right file and position. It does not show a particular editor opening, because no default editor has been chosen (open question 4).

## Friction seen while recording

These are for Austin's session, not fixes:

- **Callees listed twice.** `resolve-routes` lives in a `.cljc` file, so `->>`, `cond->>`, `fn*`, `if`, `into`, `keep` and `let` each appear twice, once for `clojure.core` and once for `cljs.core`. Nothing says which entry is which.
- **Pruned nodes have no labels.** After pruning to one unit, the four remaining neighbors are pale dots with no names.
- **Unit filter layout.** The first two unit checkboxes (`.` and `examples/buddy-auth`) share a line.

## Still to do (WP5.6)

- Austin uses Al Dente for thirty minutes on his own repository. His friction list goes in `PROGRESS.md`, and the items he marks blocking are fixed before Phase 6.
