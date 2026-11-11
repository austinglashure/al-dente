# Phase 5. GUI depth and scale

Goal (M5): the explorer is pleasant on a multi-service monorepo: it aggregates, loads detail on demand, and lets the user prune down to what matters.

Read first: `06-gui.md`.

## WP5.1 Synthetic large corpus

**Build**
- A generator (`cargo run -p bench -- gen`) that writes a synthetic snapshot directly: configurable units (default 20), namespaces (8,000), symbols (300,000), edges (2,000,000) with realistic degree distribution, certainty mix and a few very high-degree hubs.
- A second generator that writes actual Clojure source for a smaller corpus, for end-to-end timing.

**Accept**
- The synthetic snapshot loads; `aldente serve` on it starts within the budget in `02-architecture.md`, or the gap is recorded for Phase 9.

## WP5.2 Aggregation and level of detail

**Build**
- Server-side: unit-level and namespace-level graphs precomputed at load; `/expand` with paging and "top by degree first"; `/graph` choosing the finest level that fits the cap; edge bundling with counts.
- Client-side: aggregates drawn as sized clusters; expanding keeps positions stable; collapsing restores the aggregate; semantic zoom hides labels and low-weight edges when zoomed out.

**Accept**
- On the synthetic corpus: initial view under two seconds; expand a namespace of 5,000 symbols in pages; pan and zoom stay above 30 frames per second with 5,000 nodes and 20,000 edges on screen (Playwright trace on CI hardware, threshold relaxed there and noted).
- No API response exceeds the payload cap (asserted by a middleware in tests).

## WP5.3 Pruning workspace

**Build**
- Hide, Hide others like this, Focus, Keep only selection, Pin, box select.
- Workspace state: visible set, filters, positions; undo and redo; saved by name; encoded in the URL hash.
- Filter panel: units, kinds, flags, name pattern, measure ranges ("only nodes with fan-in above N").

**Accept**
- Playwright: prune a view through six steps, undo to the start, redo, reload the page from the URL and get the same view.
- A workspace that would exceed URL length limits falls back to browser storage with a short key, and says so.

## WP5.4 Details, source and links

**Build**
- Details panel sections: definition, source with line numbers and client-side highlighting, callers and callees grouped by certainty and reason with call-site snippets, impls and protocols, library, tests covering it, measures, findings. Every item is a link.
- Site navigation: clicking an edge lists its sites; clicking a site shows that line in context.
- `/open` and the editor command from user config; fallback copies `path:line:col`.
- Keyboard navigation per `06-gui.md`.

**Accept**
- `/open` tests: the command template comes only from user config; a node whose file is outside the tree is refused; arguments are passed without a shell (test with a file name containing spaces and quotes).
- Playwright: navigate caller to caller using only the keyboard.

## WP5.5 Service overview

**Build**
- Unit-level view as the landing page for multi-unit repositories: services and shared libraries as boxes, code-level dependencies between them (shared namespaces, `:local/root`), sized by symbol count, colored by any overlay.
- Drill from a unit to its namespace graph with a breadcrumb back.
- Cross-unit edges listed in the details panel of each unit ("uses 14 symbols from `shared/auth`").

**Accept**
- On `mono-mini` and the large stand-in repository the landing view shows every unit and the links between them; drilling works.
- Network-level links between services are explicitly labeled "added in the surface view" until Phase 6.

## WP5.6 Milestone M5 acceptance

**Accept**
- A scripted walkthrough on the largest acceptance repository, recorded in `docs/milestones/m5/`: land on the overview, find a function by search, see its neighborhood, prune to one service, crater it, open source, jump to the editor.
- Austin uses it for thirty minutes on his own repository; his friction list is recorded in `PROGRESS.md` and the items he marks blocking are fixed before Phase 6.
