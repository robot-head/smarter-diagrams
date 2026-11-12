# Layout constraints

The engine chooses a fresh topology-aware layout, then enforces separation, ownership, pins and relations. Add only the constraints that express your intent. Inconsistent hard constraints return diagnostics.

## Automatic presentation

Unconstrained fans group the independent producers or consumers in one column, with captions beside their horizontal branches. Parallel connections keep a consistent lane order and reserve space for their captions. Long simple pipelines fold into rows to keep the sequence readable. Pins, explicit ports and alignment rules keep control when you specify them; dragging preserves the current arrangement.

### Eight sequential transformations

```diagram guides
read: Read
validate: Validate
normalize: Normalize
enrich: Enrich
filter: Filter
aggregate: Aggregate
encode: Encode
store: Store
read -> validate -> normalize -> enrich -> filter -> aggregate -> encode -> store: data
```

### One event reaches five independent consumers

```diagram guides
bus: Event log [shape: queue]
c0: Consumer 1
c1: Consumer 2
c2: Consumer 3
c3: Consumer 4
c4: Consumer 5
bus -> c0: event
bus -> c1: event
bus -> c2: event
bus -> c3: event
bus -> c4: event
```

### Five producers converge at one sink

```diagram guides
p0: Producer 1
p1: Producer 2
p2: Producer 3
p3: Producer 4
p4: Producer 5
warehouse: Warehouse [shape: database]
p0 -> warehouse: upload
p1 -> warehouse: upload
p2 -> warehouse: upload
p3 -> warehouse: upload
p4 -> warehouse: upload
```

### Three protocols share the same endpoints

```diagram guides
client: Client
server: Server
client -> server: HTTPS
client -> server: WebSocket
client -> server: telemetry
```

## Absolute pins

```diagram guides
api: API @pin(grid: 0, 0)
cache: Cache @pin(grid: 15, 0)
api.E -> cache.W
```

`grid` coordinates are integer grid units. Pins locate centroids; at the default grid the cache is centered at `(240 px, 0 px)`. Absolute pins keep unrelated infrastructure in its world coordinate frame during a drag.

Use `@pin(x: 15g)` or `@pin(y: 6g)` for an axis-only coordinate constraint. `PinExpr::AxisLock` exposes the corresponding typed edit, where `Axis::Row` locks y and `Axis::Col` locks x.

### Single-axis locks

Here the API and worker share `y = 0`, while the database is on `y = 8g`. Their x coordinates remain available to the solver.

```diagram guides
api: API @pin(y: 0g)
worker: Worker @pin(y: 0g)
db: Database @pin(y: 8g)
api -> worker -> db
```

## Relative anchors and size locks

```diagram guides
api: API @pin(grid: 0, 0)
cache: Cache @pin(x: api.E + 6g, y: api.y)
api -> cache
```

Relative expressions use scoped IDs, cardinal edges or center axes, and a grid offset. They remain equalities during compaction. See `PinExpr::Relative` in the Rust API for typed source editing.

An optional size lock uses `lock: [w: 12g, h: 5g]` on the pin. `PinNode::size_lock` takes positive integer grid dimensions. If children, ports or text require more room than an explicit lock permits, compilation fails instead of silently expanding it.

### Locked dimensions

The API box is locked to 192 × 80 pixels at the default grid; the database uses its automatically determined size.

```diagram guides
api: API @pin(grid: 0, 0, lock: [w: 12g, h: 5g])
db: Database @pin(grid: 20, 0)
api.E -> db.W
```

## Alignment and distribution

```diagram guides
api -> worker -> db
align row [api, worker, db]
distribute row [api, worker, db] [gap: 8g]
```

`align row` shares y; `align col` shares x. `distribute row` spaces along x and `distribute col` along y, honoring an optional grid gap. Membership is relational, so these operations do not reparent entities.

### Column alignment

The same services can share a column, with north/south ports keeping the flow vertical.

```diagram guides
api.S -> worker.N
worker.S -> db.N
align col [api, worker, db]
distribute col [api, worker, db] [gap: 4g]
```

## Layout blocks and variants

```diagram guides variants=compact
api -> db
@layout {
  api @pin(grid: 0, 0)
}
@layout(compact) {
  db @pin(grid: 0, 10)
}
```

The unnamed block applies by default. Select a named variant with `LayoutOptions { variant: Some("compact".into()), ..Default::default() }`. Effective source layout rules override inline annotations of the same kind. Keep one canonical graph while retaining several presentation arrangements.

The comparison tabs above use the same source: one with default options, and one with `variant: Some("compact")`. They share a frame and keep the zoom level when switching. Use **Show grid** to inspect coordinates and dimensions.

## Connectors and labels

The `NudgeRouteChannel` edit stores the segment index and lane coordinate in a route annotation. A locked lane must remain clear of obstacles. The `SlideLabel` edit stores an above/below/inline slot and a normalized arc value between 0 and 1.

Prefer those typed operations over composing edge IDs and annotation text manually. The label optimizer uses candidate boxes, knockout pills and nearby callouts when a direct slot is unavailable. Postchecks preserve hard geometry after continuous polish.

### A connector channel

A hand-authored `channel_x` fixes the vertical channel at `10g`; the editor's typed nudge stores an internal segment and its lane instead.

```diagram guides
api: API @pin(grid: 0, 0)
db: Database @pin(grid: 20, 8)
api.E -> db.W: request @route(channel_x: 10g)
```

### Label placement

This label is placed above the connection, halfway along its route.

```diagram guides
api: API @pin(grid: 0, 0)
db: Database @pin(grid: 20, 0)
api.E -> db.W: request @label(slot: above, slide: 0.5)
```

## Constraint failures

Two fixed boxes cannot occupy the same space. Exact ports need perimeter and corner clearance. A pinned child can enlarge an unlocked parent, but a locked parent cannot grow. When a new edit is infeasible, `Document` rejects the transaction and keeps the last good snapshot. See [troubleshooting](/reference/errors).

The configuration below is feasible because the fixed boxes have room between them. Pinning both at `(0, 0)` would be rejected; the compiler does not produce an overlapping “successful” diagram.

```diagram guides
left: Left service @pin(grid: 0, 0)
right: Right service @pin(grid: 20, 0)
left -> right
```

The following source is intentionally infeasible. This diagnostic is generated by the compiler during the documentation build.

```diagram expect-error
left: Left service @pin(grid: 0, 0)
right: Right service @pin(grid: 0, 0)
left -> right
```

### Manual caption positions

The Playground's label drag stores an exact pixel-coordinate center. `ResetLabel` or double-clicking the label removes the manual center so automatic placement can resume.

```diagram
api.E -> db.W: writes @label(x: 120, y: -76)
```
