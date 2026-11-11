# Phase 6. Outside world

Goal (M6): before running a new repository, see everything it can reach and anything that looks like exfiltration, obfuscation or paid usage; and see how a codebase touches the outside world.

**Gate:** Austin has reviewed `10-security-and-deployment.md` and answered its four questions. Record his answers in `decisions.md` as D12 before starting.

## WP6.1 Rules engine

**Build** (`aldente-rules`)
- TOML catalog format with a version field: sink patterns (namespace or class globs, name globs, argument roles), services, noise, severity rules.
- Matcher over graph nodes and literals; catalogs embedded in the binary; user catalogs in the user config directory merge on top. Catalogs inside an analyzed repository are never loaded.
- `aldente rules ls|check <file>` for catalog authors.

**Accept**
- Schema validation with useful errors for a malformed catalog.
- A user catalog adds a sink and it is detected without recompiling.

## WP6.2 Capability sinks and literal evidence (layers 1 and 2)

**Build**
- `rules/sinks-clj.toml` covering every category in the document, for Clojure, Java interop and ClojureScript/JS.
- Literal classifiers: URL, host, IP, port, e-mail, sensitive path, credential-shaped, high-entropy, shell fragment.
- Attach literals to sinks by call-site argument and by referenced `def`.
- Each sink occurrence stored as a `capability` record: category, node, site, attached literals.

**Accept**
- Fixture `fixtures/vet/caps/` with one positive and one near-miss per category (for example `slurp` on a file path versus on a URL).
- Entropy and credential detectors are tested against a list of real-shaped fakes and a list of ordinary strings (UUIDs, hashes in tests, base64 images) with a stated false-positive rate.

## WP6.3 Services and paid usage (layer 3)

**Build**
- `rules/services.toml`: hosts, URL patterns, libraries, environment variable names, billed flag.
- Paid-usage findings combining signals; amplifier detection (inside `loop`, `doseq`, `map`-family calls, retry wrappers, scheduled jobs, request handlers).

**Accept**
- Fixture with: a billed client library used behind an entry point (finding), the same library declared but unused (inventory only), a bare host literal in a comment-like docstring (inventory, low).

## WP6.4 Reachability and flow (layer 4)

**Build**
- Reachability classes: load time, startup, on request, tests only, unreachable; with certainty.
- Source and sink proximity flow; obfuscated-flow escalation; decode-then-eval.
- Shortest explanatory path from an entry point to each finding.

**Accept**
- `fixtures/vet/malicious/`: a small fake library with (a) a top-level form that reads `~/.ssh` and posts it, (b) a base64 string decoded and passed to `load-string`, (c) an innocuous-looking function three calls away from a socket write. All three are `high`, each with a path.
- `fixtures/vet/benign/`: an ordinary web service with an HTTP client and a database. No `high` findings.

## WP6.5 Outside the source (layer 5)

**Build**
- Parsers and checks for every row of the layer 5 table. Dockerfile, compose, GitHub Actions and Makefile readers are line-oriented and tolerant.

**Accept**
- One fixture per row; `project.clj` with read-eval (hostile case 5) is flagged and never evaluated.

## WP6.6 Vet report

**Build**
- Severity rules in `rules/severity.toml`; `aldente vet` text and JSON with the four sections; `--fail-on`.
- GUI Vet view: findings grouped and filterable; "show path" draws the path on the canvas; outbound inventory table.

**Accept**
- Golden reports for the malicious and benign fixtures.
- `aldente vet <url>` on a stand-in public repository completes without executing anything (assert no child processes other than `git` and `clj-kondo`, through a spawn hook in tests).
- The report's last section always lists blind spots and unanalyzed dependencies.

## WP6.7 Ingress

**Build**
- Route extraction for Compojure, Reitit, Pedestal, bidi and server start calls, catalog-driven where the shape allows (`rules/ingress-clj.toml`) with small per-library extractors for route data structures.
- Consumers and scheduled jobs.
- `endpoint` nodes with method, path pattern, handler edge; ports.

**Accept**
- A fixture service per routing library with the expected endpoint list.
- Nested contexts and route data built with `concat` or `into` of literals resolve; routes built dynamically produce a hole.

## WP6.8 Egress, internal links, datastores, data shape

**Build**
- `host` nodes and `talks-to` edges from capabilities; config file reading for hosts and ports.
- Internal API linking by service name and path; topic linking for queues.
- Datastore detection and kinds; data-shape extraction from SQL migrations, SQL strings, HoneySQL keywords, Datomic and XTDB schemas, Malli and spec next to persistence calls.
- `aldente surface`.

**Accept**
- `fixtures/surface/shop/`: three services (a gateway calling two back ends over HTTP, one publishing to a topic the other consumes, two databases of different kinds). Expected: every internal link found with the stated certainty, both datastores with their tables or attributes, all external hosts.
- Each shape element records its source ("migration `003_orders.sql`", "HoneySQL keyword at `orders.clj:40`").

## WP6.9 Surface view

**Build**
- GUI Surface view per `06-gui.md`; clicking a link shows the calling code and the receiving handler; datastore panel shows the inferred shape.

**Accept**
- Playwright on the shop fixture: follow a request from gateway route to back-end handler to database table.

## WP6.10 Milestone M6 acceptance

**Accept**
- Vet three public repositories Austin has not run, and one deliberately suspicious one from the fixtures; he reads the reports and confirms they would have changed, or justified, his decision to run them.
- Surface view on his multi-service repository: he checks ingress, datastores and at least five internal links against what he knows.
