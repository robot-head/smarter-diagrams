# Core concepts

## One source, three representations

| Representation | Public API | Purpose |
| --- | --- | --- |
| Lossless concrete syntax tree | `syntax::Parse` | Every source byte, trivia token and source range |
| Relational scenegraph | `model::SceneGraph` | Resolved IDs, ports, ownership and shared relations |
| Geometry | `layout::Layout` | Node boxes, port points, connector polylines, labels and zones |

`compile` produces a `Diagram` containing all three. `Diagram::to_svg` renders the existing geometry. It does not run another layout pass.

The parser can retain malformed source for an editor. The compiler refuses error diagnostics before producing a successful diagram. Geometry constraints can also fail after syntactically valid text is lowered.

## Identity and ownership

IDs identify entities; labels provide human-readable text. `api: Public API` keeps `api` stable when the label changes. Qualified IDs such as `prod.api` include their containing scope.

A container owns its children. A zone references members and may overlap another zone. Alignments, distributions, flows and views also reference existing entities. They do not change ownership.

```diagram
prod: Production {
  api: Public API
  db: Ledger
  api -> db
}
kms: Key service
zone sensitive "Sensitive services" around [prod.db, kms] {
  padding: 1.5g
}
```

Bare edge endpoints infer nodes. Explicit declarations are useful for stable labels, record ports and attributes. Named record slots must be declared; a side-qualified virtual port such as `api.E#requests` can be inferred.

## Coordinates and units

A grid unit (`g`) is 16 pixels by default. Pins locate **node centroids**. `Rect::x` and `Rect::y` are center coordinates; use `left()`, `top()` and related methods for edges. Grid and pixel coordinates are distinct in the editing API.

The coordinate frame is unconstrained on a new graph. An absolute pin fixes it. Negative coordinates are valid; the SVG viewBox covers the resulting geometry.

## Determinism and incremental work

For the same source, options and prior layout, compilation is deterministic. A prior layout introduces stability constraints, so a fresh layout and an incremental layout can differ while both remain valid.

Keep a `Document` for a live editor. For a stateless compiler integration, retain a `Layout` and pass it to `compile_with`. The engine reuses semantic geometry and independent container queries when their constraints permit it. See [architecture](/reference/architecture) for exact cache rules and fallback conditions.
