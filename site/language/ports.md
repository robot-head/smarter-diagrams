# Ports and records

Port constraints range from free attachment to an exact named slot. The solver reserves outward stubs, spaces neighboring ports and expands unlocked nodes when more perimeter is needed.

## Five constraint tiers

| Tier | Example endpoint | Constraint |
| --- | --- | --- |
| Free | `api` | Choose a side and position |
| Side | `api.E` | Fix east side; position can move |
| Order | `api.S(order: 2)` | Fix side and clockwise ordering |
| Ratio | `api.W@0.25` | Fix a proportional position on the side |
| Position | `db#id` with a `pos` declaration | Fix an offset in grid units |

```diagram
api.E -> broker.W@0.25: publish
broker.S(order: 1) -> worker.N: consume
```

Sides are `N`, `E`, `S` and `W`. Ordered ports advance clockwise: north left-to-right, east top-to-bottom, south right-to-left and west bottom-to-top. Ratio and exact position are geometric offsets: north/south from the left edge, east/west from the top edge.

Ratios must be strictly between 0 and 1 and still respect corner clearance. Two distinct ports cannot occupy the same fixed ratio. An exact `pos` is nonnegative and must also satisfy corner/neighbor clearance; a geometrically impossible request produces a layout error.

## Explicit named ports

```diagram
broker: Event log [shape: queue] {
  port orders [side: E, ratio: 0.5]
}
worker.W -> broker#orders: subscribe
```

Declare `side`, `order`, `ratio` or `pos` on a `port` to choose its tier. Named ports are shared anchors: all uses refer to the same port definition. Incompatible side constraints on the same port are diagnosed. Stronger compatible endpoint constraints merge with the declaration.

## SQL record slots

```diagram
api.S -> orders#id: insert
orders: Orders [shape: sql_table] {
  port id: uuid [side: W, pos: 1]
  port customer_id: uuid [side: W, pos: 2]
}
```

The slot offsets are `1g` and `2g` from the top on the west side. At the default grid, that is 16 and 32 pixels. Fixed-position SQL ports render their captions inside the record. Other port captions use external placement.

## Virtual ports

`broker.E#orders` introduces a named virtual port by use. A bare `broker#orders` or `db#id` must resolve a declaration or previously inferred side-qualified port. A port lock edit can promote a virtual port to an explicit declaration while preserving unrelated source.

Use [structured `LockPort` edits](/guide/editing) when changing constraints from a canvas or agent. They handle declaration promotion and stale inline hints consistently.
