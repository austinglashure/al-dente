# Security vetting and the deployment view

Covers use cases 6 and 7. **Austin asked to review how external calls, data exfiltration and paid API usage are detected. This document is that proposal. Phase 6 should not start until he has read it.**

## What static vetting can and cannot say

It can list every place the code is able to reach the network, the file system, a subprocess or dynamic code loading, show where those places are reachable from, and flag patterns that deserve a human look. It cannot prove a repository is safe: a determined author can hide behavior behind dynamic code, native libraries or dependencies whose source is not analyzed. The report therefore always ends with the blind spots that limit it, and words findings as "can", not "does".

## Detection layers

Findings come from five layers. Each finding records which layer and what evidence produced it.

### Layer 1. Capability sinks (catalog match)

A catalog of known functions, classes and globals, by category. Matching is on graph nodes (externals and platform targets), so aliases and refers are already resolved.

| Category | Examples in the Clojure catalog |
|---|---|
| `net.http` | `clj-http.client/*`, `org.httpkit.client/*`, `hato.client/*`, `java.net.http.HttpClient`, `java.net.URL#openConnection`, `clojure.java.io/reader` or `slurp` on a URL, `js:fetch`, `js:XMLHttpRequest`, `cljs-http.client/*`, `ajax.core/*`, npm `axios`, `node-fetch`, `got` |
| `net.socket` | `java.net.Socket`, `java.nio.channels.*`, `aleph.tcp/*`, `js:WebSocket`, npm `ws`, `net` |
| `net.mail-dns` | `postal.core/*`, `javax.mail.*`, `java.net.InetAddress` |
| `proc.exec` | `clojure.java.shell/sh`, `babashka.process/*`, `java.lang.Runtime#exec`, `java.lang.ProcessBuilder`, npm `child_process` |
| `code.dynamic` | `eval`, `load-string`, `load-file`, `read-string` with read-eval, `clojure.lang.Compiler`, `Class/forName`, `java.net.URLClassLoader`, `js:eval`, `js:Function`, `cljs.js/*` |
| `fs.read`, `fs.write` | `slurp`, `spit`, `clojure.java.io/*`, `java.nio.file.Files`, npm `fs` |
| `env.read` | `System/getenv`, `System/getProperty`, `environ.core/env`, `js:process.env` |
| `crypto.encode` | `java.util.Base64`, `javax.crypto.*`, `buddy.*`, `js:btoa`, `js:atob` |
| `sys.sensitive` | clipboard, keychain and browser-storage APIs, `java.awt.Robot`, `js:document.cookie`, `js:localStorage` |
| `serialize.unsafe` | `java.io.ObjectInputStream`, `nippy/thaw` on untrusted input, `clojure.edn/read` versus `clojure.core/read` |

Catalogs live in `rules/sinks-<lang>.toml`. Each entry: pattern, category, optional argument role (which argument is the URL, command or path). Adding a library is a data change.

### Layer 2. Literal evidence

From the literal table built during parsing:

- URLs, host names, IP addresses, ports, e-mail addresses;
- file paths that point at sensitive places (`~/.ssh`, `~/.aws`, `.env`, browser profile directories, `/etc/passwd`, keychains, wallet files);
- strings that look like credentials (high-entropy tokens, key prefixes such as `sk-`, `AKIA`, `ghp_`), reported as "secret in source";
- long high-entropy or base64-like strings and large numeric arrays, as obfuscation signals;
- shell fragments (`curl … | sh`, `bash -c`, `powershell -enc`).

A literal is attached to a sink when it is an argument at the call site, or is defined in a `def` that the sink's enclosing function references. That turns "this code can call HTTP" into "this code can call `https://example.com/collect`".

### Layer 3. Paid and third-party service catalog

`rules/services.toml` maps hosts, URL patterns, client libraries and environment variable names to a service and a flag for whether usage is typically billed.

| Signal | Example |
|---|---|
| Host or URL literal | `api.openai.com`, `api.anthropic.com`, `*.amazonaws.com`, `api.stripe.com`, `api.twilio.com`, `maps.googleapis.com`, `api.sendgrid.com` |
| Client library in the dependency list or required | AWS SDK, Google Cloud clients, Stripe, Twilio, OpenAI and Anthropic clients, `cognitect.aws.client.api` |
| Environment variable read | `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `AWS_SECRET_ACCESS_KEY`, `STRIPE_SECRET_KEY` |

A paid-usage finding requires at least one signal and is stronger with more: library plus key read plus a call site reachable from an entry point is high confidence. Calls inside loops, retry wrappers, scheduled jobs or request handlers are marked as amplifiers, since they are how a bill runs up.

### Layer 4. Reachability and flow

Uses the call graph.

- **Reachability.** Each sink is tagged with the entry points that can reach it and at what certainty: at load time (top-level impl nodes: runs on require), at startup (`-main`, init functions), on request (route handlers), in tests only, or unreachable. A network call that runs when a namespace is merely loaded is ranked far above one behind a user action.
- **Coarse flow, source to sink.** A function is a *source* if it calls a `fs.read`, `env.read` or `sys.sensitive` sink (or reads a sensitive path literal). A flow finding is raised when a source and a `net.*` or `proc.exec` sink are connected: same function, or source reachable from a function that also reaches the sink within a small depth, at `likely` or better. This is graph proximity, not data-flow proof, and the report says so. It is the detector for "could send out the user's data".
- **Obfuscated flow.** A `crypto.encode` call or obfuscation literal on the same path as a `net.*` or `code.dynamic` sink raises severity. Decode-then-eval (`base64 decode` near `load-string`) is a high finding on its own.

### Layer 5. Outside the source code

Things that run before or around the code, parsed as data:

| Where | What is flagged |
|---|---|
| `package.json` | `preinstall`, `install`, `postinstall`, `prepare` scripts; dependencies from git URLs or tarballs |
| `deps.edn`, `project.clj` | `:prep-lib`, git dependencies without a full SHA, `:local/root` outside the repo, Leiningen plugins and `:injections`, read-eval or unquote in `project.clj`, non-standard `:mvn/repos` or `:repositories` hosts, `:jvm-opts` with agents |
| `shadow-cljs.edn`, `bb.edn` | build hooks, tasks that shell out |
| `data_readers.clj(c)`, `user.clj`, `.clj-kondo/hooks` | code that tools load automatically |
| Dockerfiles, compose files, CI workflows, Makefiles, shell scripts | network fetches piped to a shell, `curl`/`wget` of executables, privileged containers, secrets echoed |
| `.gitmodules`, `.gitattributes` | submodules, filters |
| Binary and minified files in source directories | listed; cannot be analyzed |

## Severity

| Level | Typical |
|---|---|
| `high` | decode-then-eval; network or exec at load time; sensitive-path read connected to a network sink; install scripts that fetch and run; credentials in source |
| `medium` | paid service reachable from an entry point; dynamic code loading; subprocess execution; unpinned git dependency; network call to a literal host that is not in the service catalog |
| `low` | network capability to known services behind user actions; environment reads; non-standard package repository |
| `info` | inventory: every sink, host and service, with reachability |

Severity is a pure function of the evidence fields, implemented as rules in `rules/severity.toml`, so Austin can retune without code changes.

## The vet report

`aldente vet <url>` clones safely, analyzes, and prints:

1. a verdict line: counts by severity, and whether anything runs at load or install time;
2. findings, highest first, each with category, location, the evidence (sink, literals, service), reachability, and the shortest path from an entry point;
3. the full outbound inventory: every host and service the code can contact;
4. what was not analyzed: dependencies (count and whether versions are pinned), interop exits, holes, unparsed and binary files.

`--fail-on <level>` sets the exit code for scripting. The GUI shows the same findings and can highlight each one's path on the graph.

## Deployment view

Built from the same catalogs, with the service flag on units.

| Aspect | Detection |
|---|---|
| **Ingress** | Route definitions: Compojure (`GET`, `POST`, `defroutes`, `context`), Reitit route vectors, Pedestal route tables, bidi, Ring handlers passed to `run-jetty`, `http-kit/run-server`, Aleph; GraphQL schemas (Lacinia); message consumers (Kafka, RabbitMQ, SQS); scheduled jobs. Each becomes an `endpoint` node with method, path pattern and a `routes` edge to the handler. Listening ports from literals and config. |
| **External calls out** | Layer 1 `net.*` sinks with their literals → `talks-to` edges to `host` nodes. |
| **Internal API calls** | An egress whose host or path matches another unit's ingress: by service name in the URL (compose service names, Kubernetes service DNS, config keys), or by path pattern when unique. Emits `talks-to` from the caller to the other unit's `endpoint`, certainty `likely` on a host match plus path match, `possible` on path alone. Shared message topics link producers to consumers the same way. |
| **Datastores** | Client libraries and JDBC URL literals: `next.jdbc`, `clojure.java.jdbc`, HoneySQL, HugSQL, Datomic, XTDB, Monger (MongoDB), Carmine (Redis), Elasticsearch clients, Cassandra, DynamoDB, S3, Kafka. Each becomes a `datastore` node with a kind. |
| **Shape of the data** | Best effort, labeled with its source: SQL migration files (`CREATE TABLE`, parsed for tables and columns); HugSQL and inline SQL strings (tables referenced); HoneySQL table and column keywords; Datomic and XTDB schema maps (attribute idents and types); Malli and spec definitions that sit next to persistence calls; document-store collection names. |

`aldente surface` emits this as data. The GUI draws services as boxes with ingress on one side, datastores and external hosts on the other, and internal calls between them.

Configuration files (`config.edn`, `.env.example`, compose and Kubernetes manifests) are read as data to resolve hosts and ports, and every value taken from them is marked with its file.

## What Austin should check

1. Are the categories and severity defaults right for how he vets a repository?
2. Is graph-proximity flow acceptable for "could exfiltrate", given it will raise some false alarms and miss flows through data structures?
3. Should the paid-service catalog ship with a broad list, or only the services he uses?
4. Is reading the local Maven cache and `.env.example` files acceptable, or should vetting touch nothing outside the clone?
