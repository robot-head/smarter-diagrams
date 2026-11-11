# Compiler and authoring architecture

`parse` retains all UTF-8 bytes, including whitespace, comments, CRLF, Unicode and malformed input, in immutable Rowan green nodes. Red nodes provide absolute byte spans and typed node/edge/endpoint/anchor/annotation/key-value views. A delimiter-safe local edit replaces one top-level green subtree; structural changes use a full lossless reparse. `lower` resolves explicit declarations before edges and relational references, infers topology nodes, validates named ports and retains source origins.

`SceneGraph` owns a container tree and separate reference relations. Zones are hyperedges; align/distribute, flows and named views retain shared membership. Public IR callers can use `reindex` after bulk mutation; lookup also tolerates stale cached indexes. Layout validates IDs, ownership, ports and numeric configuration.

## Layout stages

1. SCC condensation and Coffman–Graham labeling place a bounded number of compound vertices in each layer. The width can be explicit or derived from a target aspect ratio. Cyclic sibling cores of 3–30 unpinned nodes receive a bounded weighted-SAT placement search. Its one-hot node/cell variables enforce unique cells; soft costs estimate port-aware length, quarter turns, blocked channels, crossings and shared ink. Branching propagates occupied cells directly and retains a feasible incumbent. It is a finite macro-placement formulation rather than a port of domus's face-rotation solver.
2. Port assignment chooses free sides, sorts tangential attachments, honors clockwise order and fixed ratios/grid offsets, and expands perimeters for clearance. Discrete sides stay fixed during compaction. Separate weighted quadratic projections solve x/y equality and separation constraints, including pins, relative anchors, align/distribute, recursive containment and outward-stub envelopes. A KKT active set handles the common case; dual coordinate projection releases dependent active inequalities. The implementation uses general sparse constraints with dense active linear systems, rather than an asymptotically optimal red-black-tree VPSC implementation.
3. Direction-state A* searches the lattice of padded obstacle boundaries and port-stub coordinates. Its lower bound includes mandatory turns and normalized Manhattan length. Costs include bends, backward motion, crossings and shared segments. All segments are orthogonal. Shared channels use left/straight/right entry order and centered one-dimensional separation constraints. Locked routes and all node/port clearance constraints remain hard.
4. Label candidates occupy straight segments or external port quadrants. An R-tree identifies candidate conflicts. Connected components use bounded branch-and-bound with a feasible incumbent. External callouts and knockout pills handle dense cases. Conservative glyph-width estimates determine boxes; the generated SVG specifies the reference font stack.
5. A small forward-mode autodiff implementation supplies gradients to bounded L-BFGS. Smoothed signed rectangle/segment clearance, label anchors and channel symmetry drive micro-polish. Exact node, route and label checks accept or reject the resulting geometry. SVG rendering adds semantic silhouettes, zones, flow badges, crossings and knockout halos. OKLCH zone tints are converted to sRGB and alpha for consistent native export.

The default weight hierarchy is pin 100000, crossing 2000, order flip 450, bend 120, backward flow 80, pill 60, length 1.5/grid, label centeredness 4, source order 0.25, and channel symmetry 0.1. Hard geometry is validated independently of those weights.

## Stability and incremental queries

Surviving nodes anchor to previous coordinates with degree-based weights. A shared translation variable discounts rigid motion. Absolute user pins fix the coordinate frame, so dragging one node does not pan every unrelated node. Prior adjacent-axis order uses nonnegative elastic slacks with a linear penalty; Delaunay neighbors supply proximity anchors. Source order resolves fresh-layout ties. Stability reports translation-adjusted drift, relative-order flips and proximity distortion.

Exact semantic geometry keys reuse layouts after trivia-only edits or equal-metric text changes, refreshing rendered text. Entity keys identify independent container changes. If there are no crossing-boundary edges/constraints and the old container bounds remain feasible, only that container is solved; outside nodes and routes are reused. Changed ownership, container expansion or cross-boundary constraints safely fall back to global layout. `Layout::solved_nodes` and `reused_routes` expose the work performed. This is explicit query memoization and does not use Salsa.

Single document patches can also reuse lowered topology for plain label changes in an unadorned leaf declaration. The compiler parses the old/new declarations to verify identity, shifts all source origins, and updates the label. Attributes, ports, annotations, comments, separators and structural edits take the full lowering path. A regression compares the complete resulting IR and origins against fresh compilation, including Unicode, CRLF and semicolon/comment boundaries.

## Source editing

`TextPatch` ranges use UTF-8 byte offsets. Patch batches are validated for boundaries, overlap and expected old text before mutation. Existing annotation values are spliced in place. Changing coordinate forms removes only the relevant attributes. New single-node annotations precede trailing comments; a three-node selection uses a trailing layout block. Endpoint port edits preserve unrelated spacing. Route channel edits retain the segment index; label edits store normalized arc position.

Canvas drags prefer adjacent-peer snaps within 6 px, container-relative east offsets within 6 grid units, then rounded 16 px grid pins. A 24 px breakaway prunes conflicting alignment membership in the same transaction. `Document` compiles the whole transaction before committing a new immutable snapshot. Undo/redo restores exact source bytes and synchronized geometry while revisions increase monotonically.

## Runtime and ecosystem

Simulation integrates conservative fixed-step fluid queues at 60 Hz, using expected service rates, finite queue capacity, workers, drops and delayed retry feedback. It is a fluid model rather than a discrete packet simulator. `@sim` accepts rates and `lognormal(mu,sigma)` service/`exp_backoff(delay,fraction)` retry traits.

`@bind` connects semantic IDs to Rust symbols and OTel services. OTLP/HTTP JSON parent-child spans yield observed calls; differential union produces marked phantom edges. Promotion is an explicit source patch and boundary enforcement returns a CI error. The embedding application supplies collection/transport, including any eBPF adapter.

Formal checking consumes an explicit finite transition system and zone-local `count(role == Leader) <= 1` predicates. One-hot state variables and transition clauses unroll horizons 0–12. Unit-propagating DPLL returns an exact shortest bounded counterexample or a bounded-safe result; budget exhaustion is an explicit unknown/error. With `verification`, a document reruns registered policies after edits and undo/redo. Counterexample frames expose states, events and active edge IDs.

Strata solve independent 2D planes and project cross-layer vias with tilt through pi/4. The WebGPU example renders geometry separately from its flat SVG text layer. Lenses intersect semantic memberships and reproject fisheye displacement through hard layout constraints. Cubic smoothstep and homothetic boxes provide C1 zoom frames.

Spatial Git matches qualified IDs, solves a union with unchanged infrastructure pinned to the base layout, marks removals/additions, and interpolates geometry and caller-supplied simulation metrics. Morph frames are display animation; canonical source snapshots remain the validated states. Native SVG/PDF/PNG and all editor/MCP integrations use the same compiler/edit APIs.
