---
aside: false
---

# Runnable examples

Every example below is compiled by the Rust CLI during the site build. Download the `.diagram` source, inspect its node and connection tables, switch views where declared, or open any figure directly in the live canvas editor.

## Getting started & Markdown pipelines

### A first architecture

[Open in the editor](/demo/?example=hello) · [Download source](/examples/hello.diagram)

<<< ../examples/hello.diagram

### Fenced Markdown and Hugo pipeline

Modeled after Pikchr's self-documenting Markdown pipeline: fenced `diagram` blocks pass from Hugo or VitePress to Smarter Diagrams and return accessible inline SVG.

[Open in the editor](/demo/?example=markdown-pipeline) · [Download source](/examples/markdown-pipeline.diagram) · [Hugo plugin guide](/guide/hugo)

<<< ../examples/markdown-pipeline.diagram

### Ports and record slots

[Open in the editor](/demo/?example=ports) · [Download source](/examples/ports.diagram)

<<< ../examples/ports.diagram

### Intentional layout

[Open in the editor](/demo/?example=constraints) · [Download source](/examples/constraints.diagram)

<<< ../examples/constraints.diagram

### Shared scope and a protocol trace

[Open in the editor](/demo/?example=architecture) · [Download source](/examples/architecture.diagram)

<<< ../examples/architecture.diagram

## Domain demonstrations

Full runnable Rust programs and end-to-end architecture walkthroughs live on [Domain demonstrations](/guide/demonstrations):

- [Kubernetes operator reconciliation loop](/guide/demonstrations#_1-kubernetes-operator-reconciliation-loop) — custom resource `#spec`/`#status` ports, queue burst simulation, and bounded finalizer verification ([`kube-operator.diagram`](/examples/kube-operator.diagram)).
- [Outernet Council NMTS fragment & Graphviz benchmark](/guide/demonstrations#_2-outernet-council-nmts-fragment) — `nmts.v1.Fragment` entities, logical/physical strata, and `nmtscli` + `dot` benchmarks ([`nmts-fragment.diagram`](/examples/nmts-fragment.diagram)).
- [Grafana service graph & OTLP drift](/guide/demonstrations#_3-grafana-service-graph-and-otlp-trace-drift) — Tempo RED span metrics, error-budget zones, and live OTLP drift reconciliation ([`grafana-service-graph.diagram`](/examples/grafana-service-graph.diagram)).

### Clickable nodes and edges (hyperlinks)

Attach `[link: "..."]` or `[href: "..."]` to any node, container, or connection to wrap the silhouette, path, and edge caption in an accessible `<a class="sd-link">` SVG hyperlink (`https://`, `http://`, `mailto:`, `#`, or site-relative paths; `javascript:` and `data:` schemes are rejected at compile time).

[Open in the editor](/demo/?example=hyperlinks) · [Download source](/examples/hyperlinks.diagram)

<<< ../examples/hyperlinks.diagram

## Version-control histories & swimlanes

### Release cherry-pick and hotfix branch DAG

Branch, cherry-pick (`[style: marching-ants]`), and merge history lanes alongside Pikchr-style horizontal commit tracks and `hotfix_backport` view projection.

[Open in the editor](/demo/?example=git-rebase-cherrypick) · [Download source](/examples/git-rebase-cherrypick.diagram)

<<< ../examples/git-rebase-cherrypick.diagram

### Fossil branch, merge, and rebase commit DAG

Modeled after Fossil's branching and *Rebase Considered Harmful* diagrams: trunk check-ins `C1..C7`, concurrent feature branch `C3..C5`, and replayed check-ins `C3'..C5'` projected into `merge_history` and `rebase_history` views.

[Open in the editor](/demo/?example=fossil-branches) · [Download source](/examples/fossil-branches.diagram)

```diagram views=merge_history,rebase_history
c1: C1
c2: C2
c4: C4
c6: C6
c7: C7
c3: C3
c5: C5
c3p: "C3'"
c5p: "C5'"

c1.E -> c2.W
c2.E -> c4.W
c4.E -> c6.W
c6.E -> c7.W

c2.N -> c3.W: fork
c3.E -> c5.W: branch
c5.E -> c7.N: merge

c4.S -> c3p.W: rebase
c3p.E -> c5p.W

align row [c3, c5]
align row [c1, c2, c4, c6, c7]
align row [c3p, c5p]
align col [c3, c4]
distribute col [c3, c4] [gap: 4g]
align col [c5, c6, c3p]
distribute col [c5, c6, c3p] [gap: 4g]
align col [c7, c5p]
distribute col [c7, c5p] [gap: 4g]

zone feature_lane "feature branch" around [c3, c5] {
  padding: 1.1g
  tint: oklch(0.80 0.09 165 / 0.16)
}
zone trunk_lane "trunk" around [c1, c2, c4, c6, c7] {
  padding: 1.1g
  tint: oklch(0.80 0.07 230 / 0.16)
}
zone rebased_lane "replayed branch" around [c3p, c5p] {
  padding: 1.1g
  tint: oklch(0.82 0.10 75 / 0.16)
}

view merge_history {
  include c1 -> c2 -> c4 -> c6 -> c7
  include c2 -> c3 -> c5 -> c7
}
view rebase_history {
  include c1 -> c2 -> c4 -> c6
  include c2 -> c3 -> c5
  include c4 -> c3p -> c5p
}
flow ancestry "True merge lineage" {
  c2 -> c3 -> c5 -> c7
}
```

### Distributed peer check-in swimlanes

Four autonomous repositories sync, branch offline, and merge without a central lock.

[Open in the editor](/demo/?example=fossil-swimlanes) · [Download source](/examples/fossil-swimlanes.diagram)

<<< ../examples/fossil-swimlanes.diagram

## Multi-tier & TALA-style domain architectures

### Multi-tier event-driven streaming platform

Nested edge validation, Kafka event & DLQ queues, an Avro schema UML `class` node, Redis deduplication cache, and lakehouse storage.

[Open in the editor](/demo/?example=tala-event-platform) · [Download source](/examples/tala-event-platform.diagram)

<<< ../examples/tala-event-platform.diagram

### Institutional securities and risk data mesh

[Open in the editor](/demo/?example=tala-securities-mesh) · [Download source](/examples/tala-securities-mesh.diagram)

<<< ../examples/tala-securities-mesh.diagram

### Producer-consumer ring buffer and worker pool

[Open in the editor](/demo/?example=tala-ring-queue) · [Download source](/examples/tala-ring-queue.diagram)

<<< ../examples/tala-ring-queue.diagram

### GitHub OIDC rollout into a cloud VPC

[Open in the editor](/demo/?example=tala-cicd-vpc) · [Download source](/examples/tala-cicd-vpc.diagram)

<<< ../examples/tala-cicd-vpc.diagram

### Layered embedded SQL engine

[Open in the editor](/demo/?example=sqlite-layers) · [Download source](/examples/sqlite-layers.diagram)

<<< ../examples/sqlite-layers.diagram

## Simulation, strata & planar routing studies

### Queue overload

[Open in the editor](/demo/?example=simulation) · [Download source](/examples/simulation.diagram)

Press **Run simulation**, then raise ingress to saturate the worker. Shift-click the worker's incoming connector to cut it.

<<< ../examples/simulation.diagram

### Application and hardware strata

[Open the spatial tools](/demo/advanced.html) · [Download source](/examples/strata.diagram)

Use `spatial::strata` or the advanced tool to lay out independent planes. Ordinary SVG compilation renders a canonical 2D graph from the same declarations.

<<< ../examples/strata.diagram

### Dispatch and collect

Five workers occupy one role-aligned column. Separate ordered lanes carry dispatches and results without crossings.

[Open in the editor](/demo/?example=dispatch-collect) · [Download source](/examples/dispatch-collect.diagram)

<<< ../examples/dispatch-collect.diagram

### Two services and four stores

Both services access every store. This graph is planar: automatic attachment choices and ordered channels keep all eight connections distinct without crossings.

[Open in the editor](/demo/?example=service-store-mesh) · [Download source](/examples/service-store-mesh.diagram)

<<< ../examples/service-store-mesh.diagram

### Repeated conditional stages

Each decision, its two branches and its merge stay together when the sequence folds. Follow the arrows from Request through the three checks to Response.

[Open in the editor](/demo/?example=conditional-stages) · [Download source](/examples/conditional-stages.diagram)

<<< ../examples/conditional-stages.diagram

### Several services and fixed record rows

Checkout, auditing and reporting target six exact table slots. The reporting node sits inside the two audit paths so every operation can reach its row without a crossing. Fixed slot offsets remain unchanged.

[Open in the editor](/demo/?example=multi-slot-records) · [Download source](/examples/multi-slot-records.diagram)

<<< ../examples/multi-slot-records.diagram

### Feedback between peer regions

West and East remain separate owners. Their feedback uses an inner channel and an outer return channel with different attachment sides, keeping the paths distinct.

[Open in the editor](/demo/?example=peer-feedback) · [Download source](/examples/peer-feedback.diagram)

<<< ../examples/peer-feedback.diagram

### Syntax-highlighted code blocks, marching ants, and themes

Nodes can embed fenced `sql`, `rust`, `go`, `ts`, `python`, `json`, or `yaml` blocks inside triple-quoted labels, edges can animate with `[style: marching-ants]`, and any example can be viewed in Catppuccin, Solarized, or Base16 palettes from the toolbar above.

[Open in the editor](/demo/?example=themes-and-code) · [Download source](/examples/themes-and-code.diagram)

<<< ../examples/themes-and-code.diagram
