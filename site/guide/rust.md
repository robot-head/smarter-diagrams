# Using Rust

## Compile and inspect

```rust
use smarter_diagrams::{compile, layout};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let diagram = compile("client -> api: HTTPS\napi -> db: SQL\n")?;
    println!("{} nodes", diagram.scene.nodes.len());
    let api = diagram.layout.nodes["api"];
    println!("API center: {}, {}", api.x, api.y);
    layout::validate(&diagram.scene, &diagram.layout, &Default::default())?;
    std::fs::write("diagram.svg", diagram.to_svg())?;
    Ok(())
}
```

The `Diagram` exposes `parse`, `scene` and `layout`. Scene nodes and edges retain source origins, making diagnostics and editor navigation possible. The JSON CLI format serializes the `Layout`, not the entire source/CST/IR.

## Configure layout and preserve the mental map

```rust
use smarter_diagrams::{compile_with, LayoutOptions};

let options = LayoutOptions {
    max_layer_width: 4,
    target_aspect_ratio: 1.6,
    ..Default::default()
};
let first = compile_with("a -> b\n", &options, None)?;
let next = compile_with("a -> b -> c\n", &options, Some(&first.layout))?;
println!("Solved nodes: {}", next.layout.solved_nodes);
```

Useful options include `grid`, obstacle `margin`, outward `stub`, `lane_spacing`, `container_padding`, `variant`, `polish_steps` and `shape_budget`. Distances must be positive; `stub` must be at least `margin`. The options participate in geometry cache keys.

`stability` reports translation-adjusted drift, relative-order flips and proximity distortion. `reused_routes` and `solved_nodes` describe work performed. These counters are useful when checking that a local edit stayed local.

## Work with syntax and IR separately

```rust
let parsed = smarter_diagrams::syntax::parse("a -> b\r\n# keep this comment\r\n");
assert_eq!(parsed.syntax().text().to_string(), parsed.source.as_ref());
let scene = smarter_diagrams::model::lower(&parsed);
for diagnostic in &scene.diagnostics {
    println!("{:?}: {}", diagnostic.span, diagnostic.message);
}
```

`Parse::reparse` accepts a UTF-8 byte range and replacement string. Safe local edits preserve unrelated green subtrees; structural or lexical boundary changes fall back to a full lossless parse.

Public IR callers can modify a scenegraph and call `reindex()` after bulk node changes. Re-run layout to obtain geometry for the modified graph. Do not render an old layout against a graph whose IDs or ownership have changed.

## Errors and complete API

`Error` distinguishes source diagnostics, invalid edit requests and infeasible layout. [Troubleshooting](/reference/errors) describes each path. Browse every public module, type and method in the [generated Rust API](https://robot-head.github.io/smarter-diagrams/api/smarter_diagrams/index.html), built with all optional features.
