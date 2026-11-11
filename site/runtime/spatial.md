# Strata and Spatial Git

The [live spatial tools](/demo/advanced.html) demonstrate independent planes and branch comparison. Both features operate on the same scoped IDs used by ordinary compilation.

## Independent strata

```diagram
strata [app, hardware]
api [stratum: app]
worker [stratum: app]
host [stratum: hardware]
api -> worker: RPC
api -> host [relation: runs_on]
worker -> host [relation: runs_on]
```

`spatial::strata(scene, options, theta)` lays out each plane independently and creates cross-layer vias. Tilt is a finite angle in radians from 0 through `π/4`. At zero, typography stays flat. Plane projection changes presentation without changing the underlying source graph.

The browser example draws geometry with WebGPU and places undistorted SVG text above it. It probes shader/queue support and falls back to SVG if the adapter or submissions fail. A usable GPU is optional; the fallback retains the interactive tilt control.

Ordinary `compile` renders the canonical 2D graph. Call `strata` explicitly when you want the independent-plane presentation.

## Compare two source snapshots

```rust
use smarter_diagrams::{compile, model, syntax, spatial, LayoutOptions};
let base = compile("client -> api\napi -> db\n")?;
let proposal = model::lower(&syntax::parse("client -> api\napi -> db\napi -> worker\n"));
let diff = spatial::compare(&base.scene, &proposal, &base.layout, &LayoutOptions::default())?;
let svg = spatial::diff_svg(&diff, 1.0)?;
```

Qualified IDs match entities across snapshots. The union retains removed and added entities, locks unchanged infrastructure to the base layout, and classifies nodes/edges as unchanged, added, removed or modified. Removed entities use amber hatching; additions use green.

`morph(diff, tau, base_metrics, proposal_metrics)` interpolates geometry, opacity and caller-supplied simulation metrics for `tau` in `[0, 1]`. `0` is the base endpoint and `1` the proposal endpoint. `diff_svg` renders an animation frame.

Intermediate morph frames are display interpolation. The canonical source snapshots receive hard geometry validation; interpolated frames are not independently constrained layouts. This is an embeddable comparison API and browser viewer, not a GitHub App that automatically comments on pull requests.
