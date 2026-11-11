# Phase 8. State machines

Goal (M8): machines inferred from the code, each with its evidence, shown as diagrams.

Read first: `12-state-machines.md`.

## WP8.1 Stateful vars (detector D2)

**Build**
- Mutation and read sites for every `stateful` symbol, with entry points that reach each writer; the mutation-map result type; promotion to a machine when writes assign from a small literal set.

**Accept**
- Fixture with an atom written from three functions and read from two; a mode atom reset to literals (promoted); an atom updated with computed values (mutation map only).

## WP8.2 Keyed state (detector D1)

**Build**
- Candidate-key discovery from the literal table and form index; state sets; transition extraction with branch context (`case`, `cond`, `condp`, `defmethod` dispatch value, `if (= …)`); initial and terminal states; scoping; the `unknown` state and machine certainty.

**Accept**
- `fixtures/fsm/order/`: an order lifecycle with five states written across three namespaces, including one wildcard transition and one computed assignment. The expected machine is hand-written and matched exactly.
- Two unrelated `:status` keys in different namespaces do not merge.
- A key that is only read, or only written, yields no machine.

## WP8.3 re-frame (detector D3)

**Build**
- app-db path writes per event handler; per-path machines with events as triggers; the event flow graph.

**Accept**
- Fixture app with a login flow: `:auth/status` moves through `:anonymous`, `:pending`, `:authenticated`, `:failed` with the right events on each transition; the event flow graph shows `:auth/login` → `:auth/login-success` → `:nav/home`.

## WP8.4 Declared machines (detector D4) and, if time allows, D5

**Build**
- `rules/fsm-libs.toml` with extractors for `clj-statecharts` and `tilakone` literals.
- D5 loop-local machines only if the phase has slack.

**Accept**
- A fixture per library whose extracted machine equals the literal.

## WP8.5 Commands and view

**Build**
- `aldente fsm ls|show` with text, JSON, DOT and Mermaid.
- GUI: machine list with filters by detector and certainty; state diagram (ELK); clicking a transition opens its code; wildcard transitions drawn from a `*` pseudo-state; links from a symbol's details to machines it triggers.

**Accept**
- Golden outputs; Playwright opens the order machine and follows a transition to source.

## WP8.6 Milestone M8 acceptance

**Accept**
- Per `12-state-machines.md`: on the acceptance repositories, at least three machines Austin recognizes as real, and a `likely` list he judges worth reading. Record precision on the top twenty machines.
- If precision is poor, tighten D1's candidate rule (require more branch sites) before closing the phase and record the change.
