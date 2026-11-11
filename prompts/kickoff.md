# Kickoff prompt (first session only)

Paste this into Claude Code in a new, empty git repository that contains this plan folder's contents at its root.

---

You are implementing Al Dente, a static analyzer that turns a git repository into an explorable graph. The complete plan is in this repository.

Before writing any code, read in this order:

1. `CLAUDE.md`
2. `docs/plan/roadmap.md`
3. `docs/plan/decisions.md`
4. `docs/plan/02-architecture.md` and `docs/plan/03-graph-model.md`
5. `docs/plan/07-safety.md`
6. `docs/plan/phases/phase-00.md`

Then give me a short summary, in your own words, of: what the product is, the three rules you consider most likely to be broken by accident, and anything in the plan you think is wrong or contradictory. If you find a contradiction, name the two places and stop for my answer. Otherwise go straight on.

Then do WP0.1 from `docs/plan/phases/phase-00.md`. Follow the "Every session" steps in `CLAUDE.md`. When the acceptance criteria pass, update `PROGRESS.md`, commit, and tell me what you did, what you could not verify locally (for example the macOS and Windows CI jobs), and what the next work package is.
