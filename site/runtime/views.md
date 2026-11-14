# Views, Progressive Steps, Focus & Lenses

One canonical model backs every projection, progressive walkthrough, scenario slice and interactive lens.

## Canonical `view` projections vs. `step` & `scenario`

- **`view <id>`**: selects a subset of nodes and edges (including `*` downstream closure) and re-runs layout for that slice. Use `--view <id>` when you want a compact standalone diagram of one subsystem.
- **`step <id>` (cumulative progressive disclosure)**: accumulates every earlier `step` block up through `<id>`, keeps the full diagram's solved node coordinates and `viewBox`, and reveals new layers without shifting elements you already read. If an intermediate hop (such as a WAF) is hidden in an earlier step, connections cleanly contract between visible endpoints and expand in place when the intermediate node enters.
- **`scenario <id>`**: non-cumulative overlay highlighting one self-contained scenario (such as failover or degraded mode) over the shared canonical graph.

### Progressive disclosure walkthrough

Click **Next ▶** or pick a step tab below to reveal the architecture one layer at a time while every node stays anchored in place:

```diagram steps=core,edge,async,observability
client: "Web / Mobile Client" [shape: actor]
waf: "Edge CDN & WAF"
api: "Orders API"
cache: "Redis Session Cache" [shape: cache]
db: "Orders Primary" [shape: database]
replica: "Read Replica" [shape: database]
bus: "Event Bus" [shape: queue]
billing: "Billing Worker"
telemetry: "OTel Collector" [shape: database]

zone edge_tier "Edge Tier" around [waf] {
  tint: oklch(0.78 0.08 240 / 0.14)
}
zone async_tier "Async & Observability" around [bus, billing, telemetry] {
  tint: oklch(0.82 0.08 155 / 0.14)
}

client -> waf: "HTTPS"
waf -> api: "forwarded"
api -> cache: "session GET"
api -> db: "INSERT order"
db -> replica: "WAL stream"
api -> bus: "OrderPlaced" [style: marching-ants]
bus -> billing: "charge card"
api -> telemetry: "traces" [style: dashed]
billing -> telemetry: "metrics" [style: dashed]

align row [client, waf, api, db]

step core: "1. Core Request Path" {
  include client, api, db
}
step edge: "2. Edge WAF & Cache" {
  include waf, cache
}
step async: "3. Event Bus & Billing" {
  include bus, billing
}
step observability: "4. Metrics & Traces" {
  include telemetry, replica
}

scenario failover: "Read-Replica Failover" {
  include client -> waf -> api -> db -> replica
}
```

### CLI & animated step export

Render an individual step to SVG, PNG, or PDF with `--step <NAME>`, or export all declared `step` blocks sequentially into an animated GIF or APNG:

```sh
# Render step 2 while preserving the full diagram's coordinates
smarter-diagrams examples/progressive-architecture.diagram --step edge -o step2.svg

# Export all progressive steps as a looping animated GIF or APNG
smarter-diagrams examples/progressive-architecture.diagram --format gif --fps 1 -o walkthrough.gif
smarter-diagrams examples/progressive-architecture.diagram --format apng --fps 1 -o walkthrough.apng

# Project a standalone alternative scenario
smarter-diagrams examples/progressive-architecture.diagram --view failover -o failover.svg
```

### Library & Wasm APIs

```rust,no_run
use smarter_diagrams::{compile, views};

let src = std::fs::read_to_string("examples/progressive-architecture.diagram")?;
let d = compile(&src)?;

// Cumulative progressive step with canonical bounds preserved
let step2_svg = views::step_svg(&d, "edge", None)?;

// Causal cone focus and semantic zoom
let cone = views::causal_cone(&d.scene, "api", 1, views::Direction::Both);
let focused = views::zoom(&d.scene, &d.layout, 1.0);
println!(
    "step SVG bytes={}, cone nodes={}, zoom level={}",
    step2_svg.len(),
    cone.len(),
    focused.nodes.len()
);
```

In JavaScript/Wasm, `WasmDocument.steps_json()` lists declared views, steps, and scenarios (`id`, `kind`, `label`), and `WasmDocument.step_svg(name, theme)` renders any step or scenario at the canonical canvas frame.

```rust,no_run
use smarter_diagrams::{compile, LayoutOptions, layout::Point, views};

let d = compile("a -> b -> c -> d\n")?;
let cone = views::causal_cone(&d.scene, "b", 1, views::Direction::Both);
let lens = views::Lens {
    center: d.layout.nodes["b"].center(),
    radius: 260.,
    strength: 0.7,
    kind: views::LensKind::Members(cone),
};
let lensed = views::fisheye(&d.scene, &d.layout, &[lens], &LayoutOptions::default())?;
let _ = (Point::new(0., 0.), &lensed);
```

See the standalone viewer documentation in [Live playground](/demo/) for interactive focus lenses and theme switching.
