# Al Dente implementation package

This folder is the full plan for Al Dente, written for an Opus-class model working in Claude Code. It now lives in the Al Dente repository; the root `README.md` describes the project itself.

## What is in here

| File | Purpose |
|---|---|
| `CLAUDE.md` | Standing rules for every session. Claude Code loads it automatically from the repo root. |
| `PROGRESS.md` | The work-package checklist. The implementer updates it at the end of every session. |
| `prompts/kickoff.md` | The prompt for the very first session. |
| `prompts/session.md` | The prompt to reuse for every later session. |
| `docs/plan/roadmap.md` | Phases, milestones, ordering and the use-case trace matrix. Read first. |
| `docs/plan/decisions.md` | Decisions already made, with reasons. Do not reopen without Austin. |
| `docs/plan/open-questions.md` | What Austin still has to supply or approve. |
| `docs/plan/01-lena-assessment.md` | What Leña covers, what it does not, and how Al Dente relates to it. |
| `docs/plan/02-architecture.md` | Stack, crate layout, pipeline, storage, caching, performance budgets. |
| `docs/plan/03-graph-model.md` | Nodes, edges, certainty, identifiers, JSON schema. |
| `docs/plan/04-clojure-analysis.md` | How Clojure and ClojureScript are analyzed and how calls are resolved. |
| `docs/plan/05-cli.md` | The command set and the machine-readable output contract. |
| `docs/plan/06-gui.md` | The local web app: API, views, interactions, scale rules. |
| `docs/plan/07-safety.md` | Safe acquisition, static-only guarantees, the execution gate, server hardening. |
| `docs/plan/08-crater-and-traces.md` | Impact crater, stack trace normalizer, library impact. |
| `docs/plan/09-network-and-coverage.md` | Graph measures, static and measured coverage, risk scoring. |
| `docs/plan/10-security-and-deployment.md` | Security vetting and the deployment view. **Austin reviews this before Phase 6.** |
| `docs/plan/11-dependencies.md` | The version-aware dependency graph. |
| `docs/plan/12-state-machines.md` | Inferred state machines. |
| `docs/plan/phases/phase-NN.md` | Work packages for each phase, each with acceptance criteria. |

## How to run the work

1. One work package (WP) per Claude Code session is the default. Small adjacent WPs can share a session.
2. Phases run in order. Inside a phase, follow the WP order unless the phase file says WPs are independent.
3. A WP is done only when its acceptance criteria pass and `PROGRESS.md` is updated.
4. Milestone M1 (end of Phase 1) is the first proof named in the handoff: a real Clojure or ClojureScript repository becomes a graph and is displayed. Stop there and show Austin before going on.

## Assumptions the plan makes

These were the planner's calls. `docs/plan/decisions.md` has the reasoning.

- Al Dente is a separate Rust project, not a fork of Leña. It borrows Leña's best idea (thin language adapters feeding one graph) and can import Leña's fact stream later.
- The license is AGPL-3.0-only, matching Leña's `Cargo.toml`.
- Clojure resolution uses clj-kondo as an external tool, run in a locked-down way, plus Al Dente's own parser pass.
- Until Austin provides test repositories, acceptance runs against public stand-ins listed in `docs/plan/open-questions.md`.
