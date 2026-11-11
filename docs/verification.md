# Delivery verification

The original documents are preserved in [requirements](requirements/). RFC-001 is the authoritative blueprint. Its illustrative fragments include external references and partial declarations; the executable [architecture fixture](../examples/architecture.diagram) combines a complete scoped graph, explicit row ports, a shared zone, a flow, a view and a story.

## Reproducible checks

```sh
cargo fmt --check
cargo clippy --locked --all-features --all-targets -- -D warnings
cargo test --locked --all-features
cargo build --locked --all-features
cargo bench --bench pipeline
scripts/build-wasm.sh core
scripts/build-wasm.sh runtime
```

The native suite has **61 integration tests and one doctest**. It covers 2,100 generated malformed inputs, exact trivia retention, stale/invalid patches, geometry, cyclic cores, dependent active constraints, mental-map behavior, Unicode, finite-state counterexamples, fluid mass conservation, drift hydration, morphs and visible native-export glyphs. [GitHub CI](../.github/workflows/ci.yml) repeats formatting, Clippy, the all-feature suite, a raw Wasm build, the VS Code JavaScript syntax check and native benchmarks.

Browser checks require Chromium and Playwright (`npm ci`). `CHROMIUM_PATH` overrides `/usr/bin/chromium`. Build both Wasm bundles, serve the repository on loopback port 8765, then run:

```sh
node scripts/browser-test.cjs
node scripts/browser-benchmark.cjs
```

The browser smoke check exercises actual Wasm rendering, pointer-to-source edits, undo/redo, Unicode source replacement, flow playback, simulation load/cuts, stratum tilt and branch morph endpoints. It records screenshots under ignored `artifacts/`. The available software WebGPU driver accepted shader compilation but rejected canvas submissions; the verified SVG fallback renders geometry and flat typography correctly. The WebGPU implementation probes submissions and falls back on driver failure. A physical GPU was not available for hardware execution validation.

Neovim 0.12.6 was run headlessly against the real CLI and loopback preview server; its preview endpoint returned the compiled SVG. The VS Code adapter passed Node syntax validation; a VS Code extension host was not available. Native CLI checks compiled the architecture fixture, emitted SVG/PNG/PDF, and exercised MCP initialization and typed tool schemas.

## Issue evidence

Each issue retains its acceptance criteria and links to the corresponding implementation and checks. Named regression tests live in [core.rs](../tests/core.rs) and [extensions.rs](../tests/extensions.rs). The machine-readable map is [github-plan.json](github-plan.json).

| Issue | Implementation | Verification |
| --- | --- | --- |
| [#1](https://github.com/robot-head/smarter-diagrams/issues/1) Bootstrap Rust crate, CI and specification fixtures | [`Cargo.toml`](../Cargo.toml), [`Cargo.lock`](../Cargo.lock), [`.github/workflows/ci.yml`](../.github/workflows/ci.yml), [`README.md`](../README.md), [`docs/requirements/`](../docs/requirements/) | `cargo fmt --check`; `cargo clippy --locked --all-features --all-targets -- -D warnings`; `cargo test --locked --all-features` |
| [#2](https://github.com/robot-head/smarter-diagrams/issues/2) Implement progressive DSL and lossless red-green CST | [`src/syntax.rs`](../src/syntax.rs), [`src/model.rs`](../src/model.rs) | `lossless_unicode_crlf_and_recovery`; `incremental_reparse_preserves_unaffected_green`; `typed_cst_endpoint_and_key_value_views`; `malformed_input_never_panics_and_roundtrips`; `incremental_leaf_lowering_matches_fresh_origins_and_structural_fallback` |
| [#3](https://github.com/robot-head/smarter-diagrams/issues/3) Lower scoped relational scenegraph with source origins | [`src/model.rs`](../src/model.rs) | `scoped_nodes_ports_and_overlays`; `scoped_relative_pin_and_anchored_layout_edge`; `virtual_named_ports_are_inferred_and_promoted_for_edits`; `incremental_leaf_lowering_matches_fresh_origins_and_structural_fallback` |
| [#4](https://github.com/robot-head/smarter-diagrams/issues/4) Implement atomic source-preserving structured edits | [`src/edit.rs`](../src/edit.rs) | `pin_patches_preserve_every_unrelated_byte`; `endpoint_port_lock_is_surgical`; `invalid_and_stale_patches_are_rejected`; `source_injection_is_rejected_by_typed_coordinate_edits` |
| [#5](https://github.com/robot-head/smarter-diagrams/issues/5) Synthesize semantic gestures and relax conflicting alignments | [`src/edit.rs`](../src/edit.rs) | `semantic_gesture_promotion_and_relaxation`; `breakaway_and_undo_are_atomic`; `multiselect_and_existing_layout_upsert` |
| [#6](https://github.com/robot-head/smarter-diagrams/issues/6) Add immutable document snapshots and undo/redo | [`src/document.rs`](../src/document.rs) | `breakaway_and_undo_are_atomic`; `verification_reruns_on_source_edits_and_undo`; `mcp_revision_checked_edits`; `incremental_leaf_lowering_matches_fresh_origins_and_structural_fallback` |
| [#7](https://github.com/robot-head/smarter-diagrams/issues/7) Assign five-tier ports and deterministic topology | [`src/topology.rs`](../src/topology.rs), [`src/layout.rs`](../src/layout.rs) | `all_port_tiers`; `port_order_is_clockwise_on_all_sides`; `bounded_cyclic_shape_search_is_feasible_and_deterministic`; `coffman_graham_bounds_dag_layer_width`; `dense_cyclic_graphs_remain_geometrically_valid` |
| [#8](https://github.com/robot-head/smarter-diagrams/issues/8) Implement separable VPSC and recursive containers | [`src/solver.rs`](../src/solver.rs), [`src/layout.rs`](../src/layout.rs) | `projection_and_forward_autodiff`; `transitive_active_constraints_can_be_released`; `hard_pins_and_relations`; `relative_pins_remain_equalities_after_compaction`; `nested_containment` |
| [#9](https://github.com/robot-head/smarter-diagrams/issues/9) Preserve mental map across incremental layouts | [`src/solver.rs`](../src/solver.rs), [`src/layout.rs`](../src/layout.rs) | `incremental_stability`; `rigid_translation_is_discounted_from_mental_map`; `elastic_order_can_relax_when_pins_require_it`; `dragging_one_node_keeps_unrelated_world_positions` |
| [#10](https://github.com/robot-head/smarter-diagrams/issues/10) Route obstacle-avoiding orthogonal connectors | [`src/routing.rs`](../src/routing.rs) | `ports_routes_loops_cycles_and_collision_free_labels`; `dense_cyclic_graphs_remain_geometrically_valid` |
| [#11](https://github.com/robot-head/smarter-diagrams/issues/11) Separate shared-channel connector tracks | [`src/routing.rs`](../src/routing.rs) | `channel_vpsc_separates_shared_tracks`; `incremental_stability` |
| [#12](https://github.com/robot-head/smarter-diagrams/issues/12) Place collision-free labels with discrete optimization | [`src/labels.rs`](../src/labels.rs) | `ports_routes_loops_cycles_and_collision_free_labels`; `complete_architecture_fixture_has_readable_local_callouts` |
| [#13](https://github.com/robot-head/smarter-diagrams/issues/13) Polish label slides and channels with smooth distances | [`src/solver.rs`](../src/solver.rs), [`src/labels.rs`](../src/labels.rs) | `projection_and_forward_autodiff`; `ports_routes_loops_cycles_and_collision_free_labels` |
| [#14](https://github.com/robot-head/smarter-diagrams/issues/14) Render SVG with zones, ports, semantic flows and safe labels | [`src/render.rs`](../src/render.rs), [`src/main.rs`](../src/main.rs), [`examples/architecture.diagram`](../examples/architecture.diagram) | `escaped_svg`; `complete_architecture_fixture_has_readable_local_callouts`; `optical_tints_and_trace_playback_frames` |
| [#15](https://github.com/robot-head/smarter-diagrams/issues/15) Deliver Wasm API, benchmarks and end-to-end validation | [`src/wasm.rs`](../src/wasm.rs), [`scripts/build-wasm.sh`](../scripts/build-wasm.sh), [`scripts/browser-test.cjs`](../scripts/browser-test.cjs), [`benches/pipeline.rs`](../benches/pipeline.rs), [`docs/performance.md`](../docs/performance.md) | `native all-feature suite`; `core and runtime wasm32 builds`; `browser interactions and exported Wasm API`; `native and browser benchmarks` |
| [#16](https://github.com/robot-head/smarter-diagrams/issues/16) Implement canonical views, semantic zoom and composable lenses | [`src/views.rs`](../src/views.rs) | `views_focus_story_and_zoom`; `lenses_intersect_and_fisheye_reprojects_collisions` |
| [#17](https://github.com/robot-head/smarter-diagrams/issues/17) Add cognitive notation defaults, attention cones and story saliency | [`src/render.rs`](../src/render.rs), [`src/views.rs`](../src/views.rs) | `views_focus_story_and_zoom`; `optical_tints_and_trace_playback_frames`; `complete_architecture_fixture_has_readable_local_callouts` |
| [#18](https://github.com/robot-head/smarter-diagrams/issues/18) Ship SVG/PDF/PNG exporter and editor previews | [`src/export.rs`](../src/export.rs), [`src/main.rs`](../src/main.rs), [`extensions/vscode/`](../extensions/vscode/), [`extensions/neovim/`](../extensions/neovim/), [`scripts/preview-server.cjs`](../scripts/preview-server.cjs) | `vector_and_raster_exports (including visible glyph pixels)`; `CLI SVG/PNG/PDF smoke checks`; `Neovim live preview against the real compiler`; `VS Code adapter syntax check` |
| [#19](https://github.com/robot-head/smarter-diagrams/issues/19) Expose aesthetic linter and grammar-constrained MCP server | [`src/lint.rs`](../src/lint.rs), [`src/mcp.rs`](../src/mcp.rs) | `mcp_revision_checked_edits`; `CLI MCP initialize/tools/list smoke checks` |
| [#20](https://github.com/robot-head/smarter-diagrams/issues/20) Integrate fluid queueing simulation and counterfactual controls | [`src/simulation.rs`](../src/simulation.rs), [`web/simulation-worker.js`](../web/simulation-worker.js), [`web/index.html`](../web/index.html) | `fluid_mass_is_conserved`; `queue_overload_retry_and_sever`; `browser worker load and edge-cut controls` |
| [#21](https://github.com/robot-head/smarter-diagrams/issues/21) Reconcile telemetry and code bindings as Phantom Drift | [`src/drift.rs`](../src/drift.rs) | `drift_differential_and_promotion`; `phantom_hydration_preserves_pins_and_marks_observed_edges`; `tree_sitter_symbols` |
| [#22](https://github.com/robot-head/smarter-diagrams/issues/22) Verify bounded safety invariants and choreograph counterexamples | [`src/formal.rs`](../src/formal.rs), [`src/document.rs`](../src/document.rs), [`src/wasm.rs`](../src/wasm.rs) | `bounded_sat_finds_exact_trace`; `zone_invariants_use_only_zone_members`; `verification_reruns_on_source_edits_and_undo` |
| [#23](https://github.com/robot-head/smarter-diagrams/issues/23) Project independently laid out strata into oblique 2.5D | [`src/spatial.rs`](../src/spatial.rs), [`web/stack-renderer.js`](../web/stack-renderer.js), [`web/advanced.html`](../web/advanced.html) | `stratigraphic_vias`; `browser tilt, flat labels and SVG driver fallback` |
| [#24](https://github.com/robot-head/smarter-diagrams/issues/24) Implement bitemporal Spatial Git union layout and branch morph | [`src/spatial.rs`](../src/spatial.rs), [`web/advanced.html`](../web/advanced.html) | `spatial_union_locks_infrastructure`; `branch_morph_interpolates_changed_geometry`; `browser branch morph endpoints` |
| [#25](https://github.com/robot-head/smarter-diagrams/issues/25) Meet Wasm size and hierarchical incremental scaling budgets | [`src/model.rs`](../src/model.rs), [`src/document.rs`](../src/document.rs), [`src/layout.rs`](../src/layout.rs), [`Cargo.toml`](../Cargo.toml), [`scripts/browser-benchmark.cjs`](../scripts/browser-benchmark.cjs), [`benches/pipeline.rs`](../benches/pipeline.rs), [`docs/performance.md`](../docs/performance.md) | `local_container_query_reuses_unrelated_geometry`; `cached_equal_size_labels_refresh_text`; `cache_is_sensitive_to_layout_options`; `550-node native and Wasm incremental benchmarks`; `Brotli core/runtime bundle measurements`; `incremental_leaf_lowering_matches_fresh_origins_and_structural_fallback` |

## Scope and algorithm limits

[Architecture notes](architecture.md) identify the exact algorithms: bounded one-hot macro placement, active-set/dual coordinate projection, A*, bounded label conflict search, forward autodiff and explicit container query memoization. These implement the blueprint's capabilities without depending on domus, ELK, Rose or Salsa. A bounded search preserves a feasible incumbent rather than promising global optimality after its budget expires. Geometry is checked separately; infeasible pins and size locks return diagnostics.

Formal verification requires a caller-supplied finite transition system and supports horizons through 12; it does not infer protocol semantics from topology. Telemetry/eBPF collection and VLM transport are embedding-application responsibilities. Phantom Drift consumes observations and code symbols and exposes promotion/boundary enforcement APIs. Simulation is a conservative fluid approximation. Intermediate Spatial Git frames are display interpolation; compiled snapshots are the validated geometry.

[Performance measurements](performance.md) report cold and warm timings separately. They describe specific fixtures on one shared machine; they are not universal frame-time guarantees. This release is source-installable and is not published on crates.io or editor marketplaces.
