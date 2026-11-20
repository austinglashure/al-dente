# GUI

A local web app. `aldente serve` starts an HTTP server on the loopback interface and opens the browser. The UI is static assets embedded in the binary; all data comes from the JSON API. Server hardening rules are in `07-safety.md`.

## Stack

React, TypeScript (strict), Vite. Sigma.js v3 on graphology for the main canvas. ELK (in a web worker) for layered layouts in the crater and state-machine views. ForceAtlas2 (graphology worker) for exploratory layouts. Zustand for state. No UI framework beyond a small set of local components. Light and dark themes.

## API (`/api/v1`)

Every response uses the CLI's JSON envelope and the same `data` shapes, so the server handlers are thin wrappers over the functions the CLI calls.

| Endpoint | Returns |
|---|---|
| `GET /snapshot` | repository, revision, counts, units, blind-spot summary |
| `GET /graph?level=&unit=&dir=&min_certainty=&kinds=` | an aggregated graph that fits the payload cap; `level=auto` is the finest level that fits; `dir` (a repository path) keeps only nodes whose files lie in that directory and the edges between them |
| `GET /neighborhood?id=&depth=&direction=&min_certainty=` | nodes and edges around one node |
| `GET /expand?id=` | the children of an aggregate (unit → namespaces, namespace → symbols) and edges to what is already on screen (`&visible=` list); one page at a time (`offset`, `limit` ≤ 200), the aggregate itself standing for children not loaded yet; `POST` with the visible IDs as a JSON array for screens too big for a URL |
| `GET /node?id=` | full details for the side panel |
| `GET /source?id=&context=` | the source text for a node or site, as plain text |
| `GET /search?q=&kind=&limit=` | ranked matches |
| `POST /crater` | the crater for seeds, direction, depth, filters, optional trace |
| `GET /metrics?level=&metric=` | measures for nodes on screen or top-N |
| `GET /overlay/coverage`, `/overlay/churn`, `/overlay/risk` | per-node scalar overlays |
| `GET /libs`, `GET /lib?id=` | libraries and their usage sites |
| `GET /vet`, `GET /surface`, `GET /deps`, `GET /fsm`, `GET /fsm?id=` | later-phase views |
| `POST /open` | asks the server to open a location in the user's editor (see below) |

Long computations return `202` with a job ID; `GET /job?id=` reports progress. The UI shows progress and never blocks.

## Scale rules

- The server never returns more than the payload cap. It aggregates instead and says so (`truncated`, `aggregated`).
- The canvas starts at the unit level for multi-unit repositories and the namespace level otherwise. Symbols appear only through expand, neighborhood, crater or search.
- Levels of detail: a namespace with many symbols expands into a paged cluster (top symbols by degree first, "show more" for the rest).
- Edge bundling at aggregate levels: one edge per pair with a count and a certainty breakdown.
- Layout runs in a worker; positions are kept when nodes are added so the picture does not jump.

## Layout of the screen

```text
┌──────────────────────────────────────────────────────────────────────────┐
│ repo @ rev   [search ⌘K]         view: Graph | Crater | Libraries | …    │
├────────────┬─────────────────────────────────────────────┬───────────────┤
│ filters    │                                             │ details       │
│ certainty  │                 canvas                      │ source        │
│ edge kinds │                                             │ callers       │
│ units      │                                             │ callees       │
│ overlays   │                                             │ measures      │
│ workspace  ├─────────────────────────────────────────────┤ actions       │
│ history    │ breadcrumb · counts · truncated notice      │               │
└────────────┴─────────────────────────────────────────────┴───────────────┘
```

## Interactions

| Action | Result |
|---|---|
| Click a node | Select it; details panel fills; its edges highlight. |
| Double-click an aggregate | Expand it in place. Double-click again to collapse. |
| Double-click a symbol | Load its one-hop neighborhood into the canvas. |
| Right-click or `…` menu | Focus (keep only this node's neighborhood), Hide, Hide others like this, Pin, Crater forward, Crater reverse, Show library usage, Open in editor, Copy ID. |
| Shift-drag | Box select, then Hide or Keep only. |
| Certainty control | Three toggles. `certain` solid, `likely` dashed, `possible` dotted and faint. Hidden levels are removed from the layout, not just dimmed. |
| Edge-kind toggles | calls, references, dispatches, requires, interop exits. |
| Quick filters | Hide tests, hide externals, hide `clojure.core`, only this unit. |
| Search (`⌘K`) | Jump to a symbol, namespace, library or view. Selecting a result adds it to the canvas and centers it. |
| Details panel links | Every caller, callee, impl, protocol, library and file is a link that selects or adds that node. This is the "follow links to related places" requirement. |
| Undo and redo | Every prune or expand is a step in a workspace history. |
| Workspace | The set of visible nodes, filters and positions. It lives in the URL hash (compressed) so a view can be bookmarked, and can be saved by name in browser storage. |

## Views

| View | Phase | Content |
|---|---|---|
| Graph | 1, deepened in 5 | The main explorer at unit, namespace and symbol levels. |
| Crater | 3 | Layered left-to-right layout from the seed. Direction switch, depth slider, certainty filter, trace-pinned path highlighted. A checklist tab lists the same nodes as rows with location and coverage, exportable. |
| Libraries | 3 | List of libraries with usage counts; selecting one highlights every use on the graph and lists call sites. |
| Measures | 4 | Sortable table of nodes with all measures; selecting rows highlights them. Node size and color on the canvas can be bound to any measure or overlay. |
| Hotspots | 4 | The ranked findings from `aldente hotspots`, each linking to the node. |
| Coverage overlay | 4 | Untested code visible at a glance: fill for tested, outline for untested, a marker for high-risk untested. |
| Vet | 6 | Findings grouped by category and severity, each with evidence and the path from an entry point. |
| Surface | 6 | Services as boxes; ingress on the left, egress and datastores on the right, internal calls between services. |
| Dependencies | 7 | The version graph with conflicts marked. |
| State machines | 8 | List of machines; each drawn as a state diagram; clicking a transition shows the code. |
| Blind spots | 2 | Table of holes and interop exits, linking to the graph. |

## Source and editor links

- The source panel shows text from `/source`, rendered as text nodes only. No HTML from the repository is ever interpreted. Syntax highlighting is done client-side on escaped text.
- "Open in editor" posts a node or site ID to `/open`. The server looks up the location itself and runs the editor command from the user's config, for example `editor = ["emacsclient", "-n", "+{line}:{col}", "{file}"]`. The command template comes only from the user's config file, placeholders are filled as separate arguments, and the file must be inside the analyzed tree. If no editor is configured the button copies `path:line:col`.

## Accessibility and feel

- Everything reachable by keyboard: arrow keys move selection along edges, `e` expands, `h` hides, `f` focuses, `c` craters, `/` searches.
- Certainty is encoded by line style as well as opacity, never by color alone.
- A persistent notice whenever the canvas shows a truncated or aggregated result.
