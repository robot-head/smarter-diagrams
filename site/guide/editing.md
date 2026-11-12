# Structured editing

`Document` synchronizes immutable syntax, scenegraph and geometry snapshots. A successful edit forms one undo step. Failed transactions preserve the previous snapshot and source bytes.

## Revisions and transactions

```rust
use smarter_diagrams::{Document, edit::{DiagramEditOp, PinExpr, Placement}};

let mut doc = Document::new("a\nb\n")?;
let revision = doc.revision();
doc.edit(revision, &[DiagramEditOp::PinNode {
    node: "b".into(),
    pin: PinExpr::AbsoluteGrid { x: 15, y: 0 },
    size_lock: None,
}], Placement::Auto)?;
assert!(doc.source().contains("@pin"));
assert!(doc.undo());
assert_eq!(doc.source(), "a\nb\n");
```

Pass the current revision to `edit` and `patch`. A stale revision is rejected. Undo and redo restore exact source bytes while revision numbers increase monotonically. `snapshot()` returns an `Arc<Diagram>` you can retain while editing continues.

## Edit algebra

| Operation | Intent |
| --- | --- |
| `PinNode` | Set a grid, relative or single-axis pin, optionally lock size |
| `SnapAlign` | Add members to a row or column alignment |
| `LockPort` | Change a named port's free/side/order/ratio/slot constraint |
| `NudgeRouteChannel` | Constrain a route segment to a lane in grid units |
| `MoveLabel` | Set an exact caption center in pixel coordinates |
| `ResetLabel` | Remove its manual center and restore automatic placement |
| `SlideLabel` | Choose above/below/inline and normalized arc position |
| `UnpinEntities` | Remove pin constraints from selected nodes |
| `BreakAlignment` | Remove a member from a conflicting alignment axis |

`Placement::Auto` chooses an inline annotation or layout block. `Inline` and `Layout` request explicit placement. Multi-selection of three or more nodes uses a trailing layout block under automatic placement.

## Byte patches

```rust
use smarter_diagrams::{Document, edit::TextPatch, syntax::Span};

let mut doc = Document::new("api: Short\n")?;
let patch = TextPatch::replace(doc.source(), Span::new(5, 10), "Public API");
doc.patch(doc.revision(), &[patch])?;
assert_eq!(doc.source(), "api: Public API\n");
```

Ranges use **UTF-8 byte offsets**. A `TextPatch` includes `delete_range`, `insert_text` and optional `expected_text`. Boundary, overlap and expected-text checks run before mutation. Batches are atomic. `apply_ops` also returns patch batches in application order; each batch uses offsets in its preceding source.

The plain leaf-label incremental path verifies declaration identity and shifts source origins. Structural edits, annotations, ports and lexical changes use full lowering. Both paths compile the resulting snapshot before committing.

## Canvas gestures

`edit::synthesize_drag` consumes `Drag { node, position }` values in **pixel coordinates**. It prefers peer snaps within 6 px, container-relative anchors within 6 grid units, then a rounded 16 px grid pin. A 24 px breakaway removes conflicting alignment membership in the same transaction. Apply the resulting operation list with `Document::edit`.

For a continuous canvas gesture, use `Document::begin_drag()`, update its `DragSession` with precise pixel coordinates, and display `session.snapshot()`. `Document::commit_drag(&session)` saves that exact validated snapshot in one undo step. A stale session or invalid placement cannot commit. Returning to the starting position is a no-op.

The [browser playground](/playground) uses this transaction through `WasmDocument.begin_drag()` and `commit_drag(preview)`. Automatic positions are captured in an `@layout` block, preserving existing relative anchors and locks; canvas placement has no hidden grid/peer snap on release. The router builds each candidate from the current geometry and retains the gesture’s original neighboring paths when their endpoints and clearance remain valid. For small movements, turns and adjacent runs follow outward stubs and moving clearance boundaries while preserving the corridor. A genuinely blocked path forces a detour or rejects the placement. Automatic attachment changes compare complete route and caption candidates, including changing either end independently, with a switching cost; explicit sides remain fixed. Clear caption positions are retained for straight lines and simple elbows as well as detours. Clear caption boxes can keep their position with orthogonal leaders that bend around nodes; nearby diagonal positions and a bounded tether allow gradual movement when needed. Rendering and collision checks use the same leader segments. Exact canvas pins constrain body clearance; actual stubs are validated by the router rather than a rectangular reservation along an entire node side. Stationary route preferences use the gesture baseline. Obstacle detours additionally retain the current valid corridor and caption slot until obstructed or more than 25% plus 32 px longer than a fresh route. This deliberate hysteresis prevents equal-cost detours from alternating; complete path validation and the length bound prevent stale or growing detours. Returning to the starting position still restores the exact baseline. Facing ports prefer a balanced corridor while it remains clear, so tiny pointer changes do not switch elbow positions. Protocol flow captions on facing horizontal ports prefer a consistent rail above the endpoint pair rather than alternating inline and callout slots. Explicit route and label annotations retain control. A fresh source compilation may optimize automatic routes and caption positions again; the source does not encode gesture history. Commits and Undo/Redo retain the exact validated preview. The camera remains fixed until **Fit** is chosen. The semantic snapping helper above remains available to hosts that explicitly want those gestures.

## Moving captions directly

Drag a connection label in the Playground to reposition it without moving nodes or rerouting connections. Its center is saved as `@label(x: ..., y: ...)` in pixel coordinates and survives reopening. Double-click the label to restore automatic placement. Each completed move or reset is one Undo step; Escape cancels a drag. Invalid placements retain the last valid preview.

Rust hosts can use `session.update_label(edge_id, position)` and `session.reset_label(edge_id)` before `document.commit_drag(&session)`.
