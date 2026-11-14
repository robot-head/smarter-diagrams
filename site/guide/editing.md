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

### 1. Transactional drag lifecycle & `@layout` pins

The [browser playground](/playground) drives interactive placement through `WasmDocument.begin_drag()`, `preview.update(...)`, and `commit_drag(preview)`:

- **Source capture without hidden post-drop jumps**: Dropping a node writes its committed canvas position into an `@layout` block (preserving existing relative anchors and locks) without applying an unrequested grid or peer snap on mouse release.
- **Exact baseline & undo**: A fresh compilation from scratch may pick new automatic routes (because `.diagram` text does not record gesture history), whereas commits and Undo/Redo replay the exact validated preview snapshot.
- **Camera stability**: The viewport keeps its pan/zoom frame across drag previews and commits until you press **Fit**.
- **Explicit host snapping**: Hosts that want semantic peer/grid snapping can still invoke `edit::synthesize_drag`.

![Browser playground showing live @layout pin capture alongside preserved orthogonal routes after dragging API Gateway](/figures/gesture-node-corridor.png)

### 2. Route hysteresis & corridor preservation

Instead of re-solving all routes from scratch on every pointer frame, the router preserves existing geometry whenever endpoints and body clearance remain valid:

- **Small motions & stubs**: Outward port stubs follow the moving body clearance box rather than reserving an entire node side, keeping bends and adjacent corridor lanes stationary until a path is genuinely blocked.
- **Detour hysteresis**: Active detours and clear caption slots keep their corridor until obstructed or more than `25% + 32 px` longer than a fresh route, preventing equal-cost elbows from flickering back and forth as you drag.
- **Facing-port rails**: Facing horizontal ports retain a balanced elbow corridor and prefer a shared rail above the endpoint pair for protocol flow labels. Returning to the starting coordinates restores the exact baseline route.
- **Independent attachment search**: Only when an automatic port attachment is blocked does the router evaluate complete route+caption candidates on each end (with a switching cost); explicit `.N`/`.E`/`.S`/`.W` sides always remain fixed.

### 3. Direct caption tethering & orthogonal leaders

Edge captions track straight lines, simple elbows, and obstacle detours without jumping away on neighboring node moves:

- When you drag a caption (`preview.update_label(...)`), the canvas attaches an orthogonal dashed leader bent cleanly around nearby node bodies, using the exact same leader segments for collision checks and rendering.
- Nearby diagonal positions and a bounded tether let captions slide smoothly as attached nodes move. Double-clicking a dragged caption resets it to automatic placement (`preview.reset_label`).

![Dragging the publish caption below its bus produces an obstacle-aware dashed orthogonal leader](/figures/gesture-label-leader.png)

### 4. Grid overlay & alignment guides

Open the **Grid & snap** toolbar menu in the playground to toggle visual authoring aids (**Show grid**, **Snap to grid**, and **Align & equal spacing**). Hold `Alt` / `Option` during a drag to bypass magnetic guides temporarily without losing your settings.

![Mid-drag preview with the grid overlay and vertical alignment guide active between Worker A and Worker B](/figures/gesture-snap-guides.png)

## Moving captions directly

Drag a connection label in the Playground to reposition it without moving nodes or rerouting connections. Its center is saved as `@label(x: ..., y: ...)` in pixel coordinates and survives reopening. Double-click the label to restore automatic placement. Each completed move or reset is one Undo step; Escape cancels a drag. Invalid placements retain the last valid preview.

Rust hosts can use `session.update_label(edge_id, position)` and `session.reset_label(edge_id)` before `document.commit_drag(&session)`.

## Grid, alignment and spacing

Open **Grid & snap** in the Playground canvas toolbar. **Show grid** displays a dot grid; choose 8, 16, 24 or 32 px spacing. **Snap to grid** rounds dragged node and caption centers to that grid. These options are independent and start off, so existing diagrams retain precise free movement.

Enable **Align & equal spacing** for guides that snap node edges and centers to neighboring nodes, or caption centers to other captions. Gap guides show equal edge-to-edge spacing between nodes in the same row or column. Nearby guides retain their target until you move clearly away, keeping small pointer movements stable. Alignment takes priority over grid snapping when both are enabled.

Hold **Alt/Option** to bypass snapping during a drag. The preview shows the position that will be saved on release; each move remains one Undo step, and Escape cancels it. Grid dots and guides are editing aids and are excluded from downloaded SVGs. These canvas options do not add persistent alignment constraints to the diagram language.
