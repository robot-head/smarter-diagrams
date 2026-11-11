# Runnable examples

Every example below is compiled by the Rust CLI during the site build. Download the source, use it in your project, or open it in the live editor. The generated SVGs use the same engine as the playground.

## A first architecture

[Open in the editor](/demo/?example=hello) · [Download source](/examples/hello.diagram)

<<< ../examples/hello.diagram

![Client, API and database diagram](/generated/hello.svg)

## Ports and record slots

[Open in the editor](/demo/?example=ports) · [Download source](/examples/ports.diagram)

<<< ../examples/ports.diagram

![Named queue port and SQL record diagram](/generated/ports.svg)

## Intentional layout

[Open in the editor](/demo/?example=constraints) · [Download source](/examples/constraints.diagram)

<<< ../examples/constraints.diagram

![Pinned nodes and row alignment diagram](/generated/constraints.svg)

## Shared scope and a protocol trace

[Open in the editor](/demo/?example=architecture) · [Download source](/examples/architecture.diagram)

<<< ../examples/architecture.diagram

![Production architecture with security zone diagram](/generated/architecture.svg)

## Queue overload

[Open in the editor](/demo/?example=simulation) · [Download source](/examples/simulation.diagram)

Press **Run simulation**, then raise ingress to saturate the worker. Shift-click the worker's incoming connector to cut it.

<<< ../examples/simulation.diagram

## Application and hardware strata

[Open the spatial tools](/demo/advanced.html) · [Download source](/examples/strata.diagram)

Use `spatial::strata` or the advanced tool to lay out independent planes. Ordinary SVG compilation renders a canonical 2D graph from the same declarations.

<<< ../examples/strata.diagram
