# Session prompt (every session after the first)

## Standard

---

Continue Al Dente. Read `PROGRESS.md`, find the next unticked work package, read its phase file and every plan document that phase file lists under "Read first".

Do that one work package. Write the fixtures and tests the acceptance criteria call for before or alongside the code, not after. If an acceptance criterion cannot be met, do not weaken it: stop and tell me which one and why.

Finish with the "Every session" steps in `CLAUDE.md`. In your final message give me: what now works (with the command I can run to see it), what you could not verify, any decision you made where the plan was silent, and the next work package.

---

## Variants

**A specific work package**

> Continue Al Dente with WP3.3. Same rules as always.

**A milestone work package**

> We are at a milestone work package. Run the acceptance repositories, fix what breaks, and write the milestone record under `docs/milestones/`. Do not start the next phase. End with a list of what I should look at myself.

**A review pass** (worth running at the end of each phase, in a fresh session)

> Review everything committed in Phase N against its phase file and `CLAUDE.md`. You did not write this code; be skeptical. Check each acceptance criterion by running it. Look specifically for: uncertain edges presented as certain, repository strings reaching a shell, a query or the DOM unescaped, commands missing `--json`, tests that assert nothing, and golden files that encode wrong behavior (compare against the hand-written `EXPECTED.md`). Report findings ranked by severity. Fix nothing yet.

**When Austin supplies test repositories**

> Add these repositories to `acceptance/repos.toml` at these commits: … For each, here are facts I know to be true: … Encode them as expectations, run the acceptance runner, and report which fail and why.

**When something in the plan looks wrong**

> Do not work around it. Write the problem under "Questions for Austin" in `PROGRESS.md` with the options you see and the one you recommend, then continue with any work that does not depend on the answer.
