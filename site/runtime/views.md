# Views, zoom and lenses

## Project a named view

```rust
let diagram = smarter_diagrams::compile(
    "client -> api -> db\nview request { include client -> api -> db }\n"
)?;
let scene = smarter_diagrams::views::project(&diagram.scene, "request")?;
let layout = smarter_diagrams::layout::layout(&scene, &Default::default(), None)?;
let svg = smarter_diagrams::render::svg(&scene, &layout);
```

Projection filters canonical entities and preserves required ancestors. It keeps labels and origins intact. Use a new layout for a projected graph rather than indexing the canonical layout with missing/filtered entities.

## Causal focus

`causal_cone(scene, start, hops, direction)` returns graph membership for bounded incoming, outgoing or bidirectional reachability. `attention(scene, focus, hops)` maps node IDs to opacity/stroke/fill tones. `render::focused_svg` renders those tones without removing context.

`@story` provides a static saliency path when there is no transient hover focus. The [playground](/playground) demonstrates causal emphasis on pointer hover.

## Semantic lenses

A `Lens` has a pixel `center`, positive `radius`, finite `strength` and `LensKind`. Kinds are explicit `Members`, `SqlSchema`, `IamThreat` and `ByteLayout`. Membership combines semantic selection with spatial radius.

`lens_intersection` intersects memberships for multiple lenses. With no lenses it returns an empty set. `fisheye(scene, layout, lenses, options)` displaces selected geometry and reprojects it through the hard layout constraints, avoiding a purely visual distortion that would make nodes overlap.

The built-in semantic categories use shapes/IDs/attributes: SQL tables and databases; auth/KMS/security entities; and byte/memory metadata. They are lightweight selectors rather than a domain inference engine.

## Continuous zoom

`zoom(scene, layout, depth)` returns a `ZoomFrame` with interpolated geometry and opacity. `smoothstep` supplies a cubic C1 transition; boxes scale homothetically. These are animation frames for an embedding UI. The current playground exposes focus, while applications can compose the lower-level lens and zoom APIs into their own controls.

When a named view hides intermediate services, projection contracts their causal paths into dashed summary connections labeled `via …`. Traversal stops at the next visible node and handles cycles; the canonical source graph stays intact.
