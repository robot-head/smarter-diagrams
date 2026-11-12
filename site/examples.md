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

---

## Domain demonstrations

See [Domain demonstrations](/guide/demonstrations) for the accompanying Rust binaries (`kube_operator_reconcile`, `nmts_fragment`, and `grafana_service_graph`).

### Kubernetes operator reconciliation loop

Level-triggered controller reconciling `PostgresCluster CR` `#spec` and `#status` ports through an informer, indexer cache, rate-limiting workqueue, and backoff loop.

[Open in the editor](/demo/?example=kube-operator) · [Download source](/examples/kube-operator.diagram) · [Walkthrough](/guide/demonstrations#_1-kubernetes-operator-reconciliation-loop)

```diagram views=control_loop,owned_resources
kubectl: "kubectl / GitOps" [shape: person]

apiserver: "kube-apiserver" {
  cr: "PostgresCluster CR" [shape: sql_table] {
    port spec: "generation: 7" [side: W, pos: 1]
    port status: "observedGen: 7" [side: E, pos: 2]
  }
  sts: "StatefulSet + PVC\n(ownerRef)"
  etcd: "etcd" [shape: database]
  cr -- etcd: "watch RV"
  sts -> etcd: "persist"
}

operator: "Operator Controller" {
  informer: "ListWatch Informer" @sim(rate: 36, service_rate: 120)
  cache: "Indexer Cache" [shape: cache] @sim(service_rate: 200)
  workqueue: "RateLimitingQueue" [shape: queue] @sim(service_rate: 90, queue_cap: 120)
  reconciler: "Reconciler" @sim(service_rate: 45, queue_cap: 60, retry: exp_backoff(0.4, 0.25))
  converged: "Converged?" [shape: diamond]

  informer -> cache: "ResourceVersion"
  informer -> workqueue: "enqueue ns/name"
  cache -> reconciler: "Get(key)"
  workqueue -> reconciler: "Pop(key)"
  reconciler -> converged: "diff spec"
  converged -> workqueue: "RequeueAfter"
}

kubectl -> apiserver.cr#spec: "apply spec"
apiserver.cr -> operator.informer: "MODIFIED"
operator.converged -> apiserver.sts: "SSA apply"
operator.converged -> apiserver.cr#status: "patch status"

zone rbac "ClusterRole: postgresclusters/finalizers" around [operator.reconciler, apiserver.cr, apiserver.sts] {
  padding: 1.5g
  tint: oklch(0.76 0.11 165 / 0.14)
}
view control_loop {
  include apiserver.cr -> operator.informer -> operator.workqueue -> operator.reconciler -> operator.converged -> apiserver.cr
}
view owned_resources {
  include kubectl -> apiserver.cr -> apiserver.etcd
  include operator.converged -> apiserver.sts -> apiserver.etcd
}
flow reconcile_pass "Level-triggered reconcile" {
  kubectl -> apiserver.cr -> operator.informer -> operator.workqueue -> operator.reconciler -> operator.converged -> apiserver.cr
}
```

### Outernet Council NMTS fragment

Temporospatial `nmts.v1.Fragment` entity-relationship graph spanning `GS Arlington (EK_PLATFORM)`, a 29.5 GHz `EK_PHYSICAL_MEDIUM_LINK`, and `LEO-142 Sat (EK_PLATFORM)` across `logical` and `physical` strata.

[Open in the editor](/demo/?example=nmts-fragment) · [Download source](/examples/nmts-fragment.diagram) · [Walkthrough](/guide/demonstrations#_2-outernet-council-nmts-fragment)

```diagram views=rf_chain,logical_bearer
strata [logical, physical]

gs_arlington: "GS Arlington (EK_PLATFORM)" {
  sdn_gs: "Ground Agent\n(EK_SDN_AGENT)" [stratum: logical]
  gs_node: "Gateway Router\n(EK_NETWORK_NODE)" [stratum: logical]
  intf_gse: "gse0\n(EK_INTERFACE)" [stratum: logical]
  mod_ka: "DVB-S2X Mod\n(EK_MODULATOR)" [stratum: physical]
  ant_gs: "4.5m Ka Dish\n(EK_ANTENNA)" [stratum: physical]

  sdn_gs -> gs_node: "RK_CONTROLS"
  gs_node -> intf_gse: "RK_ORIGINATES"
  intf_gse -> mod_ka: "RK_TRAVERSES"
  mod_ka -> ant_gs: "RK_SIGNAL_TRANSITS"
}

ka_uplink: "Ka Uplink 29.5 GHz\n(EK_PHYSICAL_MEDIUM_LINK)" [shape: queue, stratum: physical]

leo_142: "LEO-142 Sat (EK_PLATFORM)" {
  ant_sat: "Phased Array\n(EK_ANTENNA)" [stratum: physical]
  demod_ka: "Ka Demod\n(EK_DEMODULATOR)" [stratum: physical]
  sat_node: "Onboard Switch\n(EK_NETWORK_NODE)" [stratum: logical]
  isl_port: "Optical ISL0\n(EK_PORT)" [stratum: physical]

  ant_sat -> demod_ka: "RK_SIGNAL_TRANSITS"
  demod_ka -> sat_node: "RK_TRAVERSES"
  sat_node -> isl_port: "RK_TRAVERSES"
}

gs_arlington.ant_gs -> ka_uplink: "RK_SIGNAL_TRANSITS"
ka_uplink -> leo_142.ant_sat: "RK_SIGNAL_TRANSITS"

zone contact_window "Contact window T+120s..540s" around [gs_arlington.ant_gs, ka_uplink, leo_142.ant_sat] {
  padding: 1.5g
  tint: oklch(0.80 0.09 205 / 0.16)
}
view rf_chain {
  include gs_arlington.mod_ka -> gs_arlington.ant_gs -> ka_uplink -> leo_142.ant_sat -> leo_142.demod_ka
}
view logical_bearer {
  include gs_arlington.sdn_gs -> gs_arlington.gs_node -> gs_arlington.intf_gse
  include leo_142.sat_node -> leo_142.isl_port
}
flow bearer "Scheduled Ka-band bearer" {
  gs_arlington.gs_node -> gs_arlington.intf_gse -> gs_arlington.mod_ka -> gs_arlington.ant_gs -> ka_uplink -> leo_142.ant_sat -> leo_142.demod_ka -> leo_142.sat_node -> leo_142.isl_port
}
```

### Grafana service graph and SLO burn

Grafana Tempo `traces_service_graph_request_*` RED span metrics with `@bind(otel: ...)` service bindings, `@sim(...)` queue parameters, and an `slo_hotpath` projection.

[Open in the editor](/demo/?example=grafana-service-graph) · [Download source](/examples/grafana-service-graph.diagram) · [Walkthrough](/guide/demonstrations#_3-grafana-service-graph-and-otlp-trace-drift)

```diagram views=slo_hotpath
ingress: "ingress-nginx" @sim(rate: 240, service_rate: 420) @bind(otel: "service.name=ingress-nginx")
frontend: "shop-frontend" @sim(service_rate: 300, queue_cap: 220) @bind(otel: "service.name=shop-frontend")
catalog: "catalog-api" @sim(service_rate: 240, queue_cap: 160) @bind(otel: "service.name=catalog-api")
checkout: "checkout-api" @sim(service_rate: 95, queue_cap: 120, retry: exp_backoff(0.3, 0.2)) @bind(otel: "service.name=checkout-api")
redis: "redis-cart" [shape: cache] @sim(service_rate: 520) @bind(otel: "service.name=redis-cart")
payments: "payment-gw\n(virtual)" @sim(service_rate: 55, queue_cap: 80) @bind(otel: "service.name=payment-gw")
orders_db: "postgres-orders" [shape: database] @sim(service_rate: 150) @bind(otel: "service.name=postgres-orders")
kafka: "kafka-orders" [shape: queue] @sim(service_rate: 200, queue_cap: 240) @bind(otel: "service.name=kafka-orders")

ingress -> frontend: "240 rps"
frontend -> catalog: "165 rps"
frontend -> checkout: "1.9% err"
catalog -> redis: "1ms"
checkout -> payments: "p95 184ms"
checkout -> orders_db: "14ms"
checkout -> kafka: "60 rps"

zone slo_burn "SLO error budget burn" around [checkout, payments] {
  padding: 1.5g
  tint: oklch(0.76 0.12 35 / 0.16)
}
view slo_hotpath {
  include ingress -> frontend -> checkout -> payments
  include checkout -> orders_db
}
flow tail_latency "p95 checkout trace" {
  ingress -> frontend -> checkout -> payments
}
```

---

## Version-control histories & swimlanes

### Fossil branch, merge, and rebase commit DAG

Modeled after Fossil's branching and *Rebase Considered Harmful* diagrams: trunk check-ins `C0..C7`, concurrent feature branch `C3..C5`, and replayed check-ins `C3'..C5'` projected into `merge_history` and `rebase_history` views.

[Open in the editor](/demo/?example=fossil-branches) · [Download source](/examples/fossil-branches.diagram)

```diagram views=merge_history,rebase_history
c0: C0
c1: C1
c2: C2
c4: C4
c6: C6
c7: C7
c3: C3
c5: C5
c3p: "C3'"
c5p: "C5'"

c0.E -> c1.W
c1.E -> c2.W
c2.E -> c4.W
c4.E -> c6.W
c6.E -> c7.W

c2.N -> c3.W: fork
c3.E -> c5.W
c5.E -> c7.N: merge

c6.S -> c3p.W: rebase
c3p.E -> c5p.W

align row [c0, c1, c2, c4, c6, c7]
align row [c3, c5]
align row [c3p, c5p]

zone trunk_lane "trunk" around [c0, c1, c2, c4, c6, c7] {
  padding: 1.25g
  tint: oklch(0.80 0.07 230 / 0.16)
}
zone feature_lane "feature branch" around [c3, c5] {
  padding: 1.25g
  tint: oklch(0.80 0.09 165 / 0.16)
}
zone rebased_lane "replayed branch" around [c3p, c5p] {
  padding: 1.25g
  tint: oklch(0.82 0.10 75 / 0.16)
}
view merge_history {
  include c0 -> c1 -> c2 -> c4 -> c6 -> c7
  include c2 -> c3 -> c5 -> c7
}
view rebase_history {
  include c0 -> c1 -> c2 -> c4 -> c6
  include c2 -> c3 -> c5
  include c6 -> c3p -> c5p
}
flow ancestry "True merge lineage" {
  c0 -> c1 -> c2 -> c3 -> c5 -> c7
}
```

### Distributed peer check-in swimlanes

Four autonomous repositories sync, branch offline, and merge without a central lock.

[Open in the editor](/demo/?example=fossil-swimlanes) · [Download source](/examples/fossil-swimlanes.diagram)

<<< ../examples/fossil-swimlanes.diagram

---

## Multi-tier & TALA-style domain architectures

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

---

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
