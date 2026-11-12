# Runnable examples

Every example below is compiled by the Rust CLI during the site build. Download the source, use it in your project, or open it in the live editor. The generated SVGs use the same engine as the playground.

## A first architecture

[Open in the editor](/demo/?example=hello) · [Download source](/examples/hello.diagram)

<<< ../examples/hello.diagram


## Ports and record slots

[Open in the editor](/demo/?example=ports) · [Download source](/examples/ports.diagram)

<<< ../examples/ports.diagram


## Intentional layout

[Open in the editor](/demo/?example=constraints) · [Download source](/examples/constraints.diagram)

<<< ../examples/constraints.diagram


## Shared scope and a protocol trace

[Open in the editor](/demo/?example=architecture) · [Download source](/examples/architecture.diagram)

<<< ../examples/architecture.diagram


## Queue overload

[Open in the editor](/demo/?example=simulation) · [Download source](/examples/simulation.diagram)

Press **Run simulation**, then raise ingress to saturate the worker. Shift-click the worker's incoming connector to cut it.

<<< ../examples/simulation.diagram

## Application and hardware strata

[Open the spatial tools](/demo/advanced.html) · [Download source](/examples/strata.diagram)

Use `spatial::strata` or the advanced tool to lay out independent planes. Ordinary SVG compilation renders a canonical 2D graph from the same declarations.

<<< ../examples/strata.diagram

## Dispatch and collect

Five workers occupy one role-aligned column. Separate ordered lanes carry dispatches and results without crossings.

[Open in the editor](/demo/?example=dispatch-collect) · [Download source](/examples/dispatch-collect.diagram)

<<< ../examples/dispatch-collect.diagram

## Two services and four stores

Both services access every store. This graph is planar: automatic attachment choices and ordered channels keep all eight connections distinct without crossings.

[Open in the editor](/demo/?example=service-store-mesh) · [Download source](/examples/service-store-mesh.diagram)

<<< ../examples/service-store-mesh.diagram

## Repeated conditional stages

Each decision, its two branches and its merge stay together when the sequence folds. Follow the arrows from Request through the three checks to Response.

[Open in the editor](/demo/?example=conditional-stages) · [Download source](/examples/conditional-stages.diagram)

<<< ../examples/conditional-stages.diagram

## Several services and fixed record rows

Checkout, auditing and reporting target six exact table slots. The reporting node sits inside the two audit paths so every operation can reach its row without a crossing. Fixed slot offsets remain unchanged.

[Open in the editor](/demo/?example=multi-slot-records) · [Download source](/examples/multi-slot-records.diagram)

<<< ../examples/multi-slot-records.diagram

## Feedback between peer regions

West and East remain separate owners. Their feedback uses an inner channel and an outer return channel with different attachment sides, keeping the paths distinct.

[Open in the editor](/demo/?example=peer-feedback) · [Download source](/examples/peer-feedback.diagram)

<<< ../examples/peer-feedback.diagram
