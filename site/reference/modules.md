# Module map

The [complete generated Rust API](https://robot-head.github.io/smarter-diagrams/api/smarter_diagrams/index.html) documents every public symbol and signature with all optional features enabled. This map helps choose the integration level you need.

| Module | Entry points and responsibility |
| --- | --- |
| Crate root | `compile`, `compile_with`, `Diagram`, `Error` |
| `syntax` | `parse`, lossless `Parse`, `Span`, Rowan nodes and typed `ast` views |
| `model` | `lower`, `SceneGraph`, nodes, endpoints, attributes, source origins and diagnostics |
| `document` | `Document`, immutable snapshots, transactions and verification policies |
| `edit` | `DiagramEditOp`, `PinExpr`, `TextPatch`, `apply_ops`, `apply_patches`, `synthesize_drag` |
| `layout` | `LayoutOptions`, `Layout`, `Point`, `Rect`, `layout`, `validate`, stability metrics |
| `topology` | SCC analysis, bounded-width layering and bounded cyclic macro placement |
| `solver` | Weighted linear constraints, projection, forward-mode `Dual` and bounded L-BFGS |
| `routing` | Obstacles, orthogonal routes, shared channels and crossing geometry |
| `labels` | `LabelPlacement`, conservative metrics, candidate selection and safe polish |
| `render` | SVG, focused SVG and flow-frame rendering |
| `export` | Native PNG/PDF export (`export` feature) |
| `views` | Projection, causal cones, attention, lenses, fisheye and zoom frames |
| `simulation` | `Simulation`, service traits, queue state and counterfactual edge cuts |
| `drift` | OTLP/observed edges, static symbols, reconciliation, hydration, promotion and CI policy |
| `formal` | Explicit transition systems, invariants, bounded SAT and counterexample frames |
| `spatial` | Independent strata, vias, branch union and morph frames |
| `lint` | Heuristic `Finding` values and caller-supplied `VisualCritic` |
| `mcp` | Stdio `Server`, JSON-RPC handling and typed edit schemas |

The Wasm-bindgen module is an implementation boundary, not a public Rust module. Its JavaScript exports are covered in the [Wasm reference](/guide/wasm) and generated `.d.ts` files shipped with each package.

## Choose a layer

Use `compile` and `to_svg` for static rendering. Use `Document` for editing. Consume `SceneGraph` and `Layout` for a custom renderer. Use the lower-level solver/routing modules when integrating algorithms directly, and re-run `layout::validate` after changing geometry.

Source-origin spans use byte offsets. Geometry uses pixels, with explicit grid conversions at notation/edit boundaries. The [core concepts](/guide/concepts) explain identity and ownership across all modules.
