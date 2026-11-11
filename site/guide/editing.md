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

The [browser playground](/playground) uses the same operation synthesis through `WasmDocument.drag`, so text and canvas share a single model.
