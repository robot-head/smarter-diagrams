# Layout constraints

The engine chooses a fresh topology-aware layout, then enforces separation, ownership, pins and relations. Add only the constraints that express your intent. Inconsistent hard constraints return diagnostics.

## Absolute pins

```diagram
api: API @pin(grid: 0, 0)
cache: Cache @pin(grid: 15, 0)
api.E -> cache.W
```

`grid` coordinates are integer grid units. Pins locate centroids; at the default grid the cache is centered at `(240 px, 0 px)`. Absolute pins keep unrelated infrastructure in its world coordinate frame during a drag.

Use `@pin(x: 15g)` or `@pin(y: 6g)` for an axis-only coordinate constraint. `PinExpr::AxisLock` exposes the corresponding typed edit, where `Axis::Row` locks y and `Axis::Col` locks x.

## Relative anchors and size locks

```diagram
api: API @pin(grid: 0, 0)
cache: Cache @pin(x: api.E + 6g, y: api.y)
api -> cache
```

Relative expressions use scoped IDs, cardinal edges or center axes, and a grid offset. They remain equalities during compaction. See `PinExpr::Relative` in the Rust API for typed source editing.

An optional size lock uses `lock: [w: 12g, h: 5g]` on the pin. `PinNode::size_lock` takes positive integer grid dimensions. If children, ports or text require more room than an explicit lock permits, compilation fails instead of silently expanding it.

## Alignment and distribution

```diagram
api -> worker -> db
align row [api, worker, db]
distribute row [api, worker, db] [gap: 8g]
```

`align row` shares y; `align col` shares x. `distribute row` spaces along x and `distribute col` along y, honoring an optional grid gap. Membership is relational, so these operations do not reparent entities.

## Layout blocks and variants

```diagram
api -> db
@layout {
  api @pin(grid: 0, 0)
}
@layout(compact) {
  db @pin(grid: 15, 0)
}
```

The unnamed block applies by default. Select a named variant with `LayoutOptions { variant: Some("compact".into()), ..Default::default() }`. Effective source layout rules override inline annotations of the same kind. Keep one canonical graph while retaining several presentation arrangements.

## Connectors and labels

The `NudgeRouteChannel` edit stores the segment index and lane coordinate in a route annotation. A locked lane must remain clear of obstacles. The `SlideLabel` edit stores an above/below/inline slot and a normalized arc value between 0 and 1.

Prefer those typed operations over composing edge IDs and annotation text manually. The label optimizer uses candidate boxes, knockout pills and nearby callouts when a direct slot is unavailable. Postchecks preserve hard geometry after continuous polish.

## Constraint failures

Two fixed boxes cannot occupy the same space. Exact ports need perimeter and corner clearance. A pinned child can enlarge an unlocked parent, but a locked parent cannot grow. When a new edit is infeasible, `Document` rejects the transaction and keeps the last good snapshot. See [troubleshooting](/reference/errors).
