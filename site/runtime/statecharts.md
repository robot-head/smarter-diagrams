# Harel statecharts & coupled simulations

Smarter Diagrams implements Harel statecharts following [statecharts.dev](https://statecharts.dev/) semantics: hierarchical composite states, orthogonal parallel regions, guards and actions on `"EVENT [guard] / action"` labels, `entry` / `exit` actions, shallow (`H`) and deep (`H*`) history states, and optional live coupling to the fluid queue [`Simulation`](../api/smarter_diagrams/simulation/struct.Simulation.html).

Click event buttons below (starting with **submit**, then **approve**, **decline**, **pack**, **hold**, **resume_shallow** vs **resume_deep**) to see active borders, orthogonal regions, context mutation (`retries`), and history restoration live:

<SimulationEmbed mode="statechart" title="Interactive Order Lifecycle Statechart (parallel regions + shallow/deep history)" />

## Pseudostates & container attributes

| Feature | `.diagram` syntax | Semantics |
| --- | --- | --- |
| Initial pseudostate | `init [shape: initial]` | Solid dark circle; outgoing edge marks initial target of its container |
| Final state | `settled [shape: final]` | Bullseye marker indicating region/state completion |
| Shallow history (`H`) | `h [shape: history]` | Remembers immediate active child when its parent composite was exited |
| Deep history (`H*`) | `dh [shape: deep_history]` | Restores full active atomic descendant tree across nested/parallel states |
| Orthogonal parallel container | `fulfilling [parallel: true] { ... }` | AND-state: activates all direct child `[region: true]` containers concurrently |
| Orthogonal region | `billing [region: true] { ... }` | Dashed container border; evolves independently of sibling regions |
| Context & lifecycle hooks | `[context: "retries=0"]`, `[entry: "..."]`, `[exit: "..."]` | Typed state variables initialized at reset and mutated on state exit/entry |

## Transition syntax: `EVENT [guard] / action`

Edge labels and optional explicit edge attributes support standard statechart notation:

- **`"submit"`** — triggers on event `submit` when source state (or any ancestor) is active.
- **`"decline [retries < max_retries] / retries += 1"`** — fires only if the boolean guard holds; mutates `context` before entering target.
- **`"[workers.saturation >= 0.85] / sever(api -> workers)"`** — eventless guarded transition evaluated during `step_coupled(...)` whenever live queue saturation crosses `85%`.
- Explicit overrides `[event: "..."]`, `[guard: "..."]` (or `[when: "..."]`), and `[action: "..."]` work identically.

```diagram
order [label: "Order Lifecycle", context: "retries=0; max_retries=2"] {
  init [shape: initial]
  draft [label: "Draft"]
  checkout [label: "Checkout"] {
    c_init [shape: initial]
    h [shape: history]
    dh [shape: deep_history]
    review [label: "Review Order"]
    fulfilling [label: "Fulfillment", parallel: true] {
      billing [label: "Billing", region: true] {
        b_init [shape: initial]
        authorizing [label: "Authorizing"]
        paid [label: "Captured"]
        b_init -> authorizing
        authorizing -> paid: "capture"
      }
      shipping [label: "Shipping", region: true] {
        s_init [shape: initial]
        picking [label: "Picking"]
        packed [label: "Packed"]
        s_init -> picking
        picking -> packed: "pack"
      }
    }
    c_init -> review
    review -> fulfilling: "approve"
  }
  paused [label: "On Hold"]
  settled [shape: final]

  init -> draft
  draft -> checkout: "submit"
  checkout -> paused: "hold"
  paused -> checkout.h: "resume_shallow"
  paused -> checkout.dh: "resume_deep"
  checkout.fulfilling -> settled: "complete"
}
```

## Animated APNG / GIF export

![Order Lifecycle Statechart APNG animation](/figures/order-statechart.apng)

Generate the APNG above straight from the CLI:

```sh
cargo run --features export -- examples/order-statechart.diagram \
  -o order-statechart.apng --format apng --fps 2 \
  --events "submit,approve,decline,retry,capture,pack,hold,resume_deep,complete"
```
