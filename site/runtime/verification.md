# Bounded verification

The checker unrolls an explicit finite transition system into one-hot SAT and searches for a shortest bounded counterexample. It checks horizons 0 through 12 and at most 512 states. It does not infer protocol behavior from arrows or simulate arbitrary Rust code.

## Zone invariants

```diagram
a -> b
b -> a
zone quorum around [a, b] {
  invariant split_brain_free: count(role == Leader) <= 1
}
```

The textual invariant parser supports `count(variable == value) <= integer`. The Rust `Invariant` enum additionally supports `Equals` and `Never`. Zone checks restrict qualified state variables to member nodes, so an unrelated service's role does not affect the quorum predicate.

## Supply states and transitions

```rust
use std::collections::BTreeMap;
use smarter_diagrams::formal::{State, Transition, TransitionSystem, parse_invariant, check};

let system = TransitionSystem {
    states: vec![
        State { name: "one leader".into(), variables: BTreeMap::from([
            ("a.role".into(), "Leader".into()), ("b.role".into(), "Follower".into())
        ]) },
        State { name: "split brain".into(), variables: BTreeMap::from([
            ("a.role".into(), "Leader".into()), ("b.role".into(), "Leader".into())
        ]) },
    ],
    initial: vec![0],
    transitions: vec![Transition {
        from: 0, to: 1, event: "drop heartbeat; elect b".into(), edge: Some("b->a".into())
    }],
};
let result = check(&system, &parse_invariant("count(role == Leader) <= 1")?, 12)?;
```

`SafeThrough { horizon }` is bounded safety, not an unbounded proof. `Violation` includes exact states, events, optional edge IDs and the failing horizon. `trace_frame` produces playback frames for a UI. Budget exhaustion returns an explicit error/unknown outcome; it is never reported as safe.

## Recheck on edits

With the `verification` feature, register a `VerificationPolicy { zone, system, horizon }` using `Document::verify_continuously`. Successful edits and undo/redo recompute registered checks. Read `verification_results()` for named results; a violation is evidence to display rather than an implicit source rewrite.

The runtime Wasm bundle exposes `check_safety` for JSON inputs and document-level `verify`/`verification_results`. The host supplies state/transition semantics, and can choreograph counterexample edges using the returned frames.
