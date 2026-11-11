# Inferred state machines

Covers use case 2. This is the most speculative part of the product, so the plan is deliberately narrow: a set of concrete detectors, each producing a machine with stated evidence. Nothing is declared by the user; everything is inferred.

## What a machine is

```json
{ "id": "fsm:clj:.//app.orders#:order/status",
  "subject": { "kind": "key", "key": ":order/status", "scope": "clj:.//app.orders" },
  "detector": "keyed-state",
  "certainty": "likely",
  "states": [ { "id": ":pending", "initial": true }, { "id": ":paid" }, { "id": ":shipped", "terminal": true } ],
  "transitions": [
    { "from": ":pending", "to": ":paid", "trigger": "app.orders/pay!",
      "certainty": "likely", "site": { "file": "src/app/orders.clj", "line": 72 } } ] }
```

`from` may be `"*"` when the code sets a state without checking the current one. Every transition points at the code that performs it.

## Detectors

### D1. Keyed state in data (the main one)

**Signal.** A keyword key whose values are a small closed set of keyword or string literals, and code that both branches on it and writes it.

1. **Candidate keys.** A key `k` is a candidate when, across the repository, it appears in at least two of: `(case (k x) …)`, `(condp = (k x) …)`, `(cond (= (k x) :lit) …)`, `(= :lit (k x))`, map destructuring followed by `case`, a multimethod whose dispatch function is `k`; and at least one write: `(assoc x k :lit)`, `(assoc-in x [… k] :lit)`, `(update x k …)` with a literal result, a map literal `{k :lit}` in a return position, `(swap! a assoc k :lit)`. Names such as `:state`, `:status`, `:phase`, `:stage`, `:step`, `:mode` raise priority but are not required.
2. **States.** The union of literals compared against and written.
3. **Transitions.** For each write of `k := s2`: if it sits inside a branch that tested `k = s1` (a `case` clause, a `cond` arm, a `defmethod` for dispatch value `s1`), the transition is `s1 → s2`, `likely`. Otherwise it is `* → s2`, `possible`. The trigger is the enclosing function or impl.
4. **Initial and terminal.** Initial: the literal in constructors and default maps. Terminal: states with no outgoing transition.

Scope the machine to the namespaces where the key is written, so two unrelated uses of `:status` do not merge. Namespaced keywords (`:order/status`) scope themselves.

### D2. Stateful vars

For every symbol flagged `stateful` (an atom, ref, agent or volatile), collect every `swap!`, `reset!`, `compare-and-set!`, `alter`, `ref-set`, `send`, `vswap!` site that names it. Output a mutation map: who writes it, who reads it (`deref`, `@`), from which entry points. If the writes assign from a small literal set (`(reset! mode :editing)`), promote to a machine by the D1 logic with the var as subject. Otherwise the mutation map itself is the result, shown as a star diagram: the var in the middle, writers and readers around it.

### D3. re-frame

Subject: an app-db path. States and transitions come from event handlers.

- For each `reg-event-db` and `reg-event-fx` handler, find literal paths it writes (`assoc-in db [:a :b] v`, `(assoc db :k v)`, `update-in`) and literal values.
- For each path with a small literal value set, build a D1-style machine whose triggers are events.
- Separately, build the **event flow graph**: event → event edges from `:dispatch`, `:dispatch-n`, `:dispatch-later`, `:fx` effects with literal vectors. This is a machine over events and is often what a ClojureScript developer wants to see first.

### D4. Declared machines in data

Libraries where the machine is a literal map: `clj-statecharts`, `tilakone`, `re-statecharts`, `reagi`-style tables, and XState configs in JavaScript (Phase 10). Catalog entries in `rules/fsm-libs.toml` describe where states and transitions sit in the literal. These are `certain`, because the code says so, and they double as test fixtures for the diagram view.

### D5. Function-level control state

A `loop` with a binding that is only ever rebound to literals in `recur` calls, branching on that binding, is a machine local to the function. Same for a `go-loop`. Low priority; implement last and only if D1 to D4 leave time in the phase.

## Certainty

A machine's certainty is the weakest of: how sure the state set is (closed literal set versus "also assigned from a non-literal", which adds an `unknown` state), and how many transitions are `* →`. A machine with an `unknown` state or mostly wildcard transitions is `possible` and hidden by default.

## Limits to state plainly in the UI

- State carried in values that are computed, not literal, is invisible.
- A transition's guard is "the branch it was found in"; conditions beyond the state test are not modeled.
- Machines spread across services (state in a database column, written by several services) are found only when the column's key appears as the same keyword in code; Phase 6's data-shape work can link them later.

## Interface

`aldente fsm ls` lists machines with subject, detector, counts and certainty. `aldente fsm show <id>` prints states and transitions with locations, or DOT and Mermaid `stateDiagram-v2`. The GUI lists machines, draws one as a state diagram (ELK layered), and clicking a transition opens its code. A symbol's details panel links to any machine it triggers.

## Acceptance approach

Fixtures under `fixtures/fsm/` for each detector with a hand-written expected machine. On the acceptance repositories, Austin judges usefulness: the phase's exit criterion is that at least three machines he recognizes as real are found, and the false-positive rate at `likely` is low enough that the list is worth reading.
