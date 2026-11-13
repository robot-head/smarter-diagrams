---
aside: false
---

# Domain demonstrations

Three end-to-end demonstrations pair declarative `.diagram` models with runnable Rust programs in [`examples/`](https://github.com/robot-head/smarter-diagrams/tree/main/examples):

1. **Kubernetes operator reconciliation loop** — custom resource `#spec`/`#status` record ports, informer/workqueue/reconciler feedback, queue overload simulation, and bounded SAT verification of finalizer cleanup.
2. **Outernet Council NMTS fragment** — lowering [`nmts.v1.Fragment`](https://github.com/outernetcouncil/nmts) entities (`EK_*`) and relationships (`RK_*`) into nested platforms, logical/physical strata, and temporospatial contact-window diffs.
3. **Grafana service graph and OTLP drift** — converting Tempo `traces_service_graph_request_*` RED span metrics into an annotated topology and reconciling live OpenTelemetry spans to surface undeclared calls.

## 1. Kubernetes operator reconciliation loop

[Open in the editor](/demo/?example=kube-operator) · [Download `kube-operator.diagram`](/examples/kube-operator.diagram)

Kubernetes controllers act as level-triggered feedback loops rather than linear request pipelines. `PostgresCluster CR` declares separate `#spec` and `#status` record ports on `kube-apiserver`; `ListWatch Informer` populates `Indexer Cache` and enqueues keys into `RateLimitingQueue`; `Reconciler` reads cached state without hitting `etcd`, patches owned `StatefulSet & Svc` resources via server-side apply, and requeues transient `409 Conflict` errors with exponential backoff.

Switch between **Canonical graph**, **View: control_loop**, and **View: owned_resources** below:

```diagram views=control_loop,owned_resources
kubectl: "kubectl / GitOps" [shape: actor] @sim(rate: 45, service_rate: 160)

apiserver: "kube-apiserver" {
  sts: "StatefulSet & Svc"
  cr: "PostgresCluster CR" [shape: sql_table] {
    port spec: DesiredSpec [side: W, pos: 1]
    port status: ReadyStatus [side: E, pos: 2]
  }
  etcd: "etcd Quorum" [shape: database] @sim(service_rate: 180)
  sts -> etcd: "store"
  cr -> etcd: "RAFT"
}

operator: "Operator Controller" {
  informer: "ListWatch Informer" @sim(service_rate: 150, queue_cap: 160)
  workqueue: "RateLimitingQueue" [shape: queue] @sim(service_rate: 95, queue_cap: 140)
  cache: "Indexer Cache" [shape: cache]
  reconciler: "Reconciler" @sim(service_rate: 65, queue_cap: 90, retry: exp_backoff(0.4, 0.25))

  informer -> workqueue: "key"
  informer -> cache: "sync"
  workqueue -> reconciler: "pop"
  cache -> reconciler: "get"
  reconciler -> workqueue: "requeue"
}

kubectl -> apiserver.cr#spec: "apply"
apiserver.cr -> operator.informer: "watch"
operator.reconciler -> apiserver.cr#status: "status"
operator.reconciler -> apiserver.sts: "SSA patch"

zone rbac "Controller RBAC scope" around [apiserver.sts, apiserver.cr] {
  padding: 1.15g
  tint: oklch(0.79 0.08 175 / 0.16)
}
view control_loop {
  include apiserver.cr -> operator.informer -> operator.workqueue -> operator.reconciler -> apiserver.cr
}
view owned_resources {
  include kubectl -> apiserver.cr -> operator.informer -> operator.workqueue -> operator.reconciler -> apiserver.sts -> apiserver.etcd
}
flow converge "Converged reconciliation" {
  kubectl -> apiserver.cr#spec -> operator.informer -> operator.workqueue -> operator.reconciler -> apiserver.sts -> apiserver.etcd
}
```

### Simulate workqueue bursts and check finalizer invariants

[`examples/kube_operator_reconcile.rs`](https://github.com/robot-head/smarter-diagrams/blob/main/examples/kube_operator_reconcile.rs) compiles `kube-operator.diagram`, projects `control_loop`, drives a watch-storm burst through `Simulation`, and verifies through horizon 12 that a PVC is never orphaned while the finalizer remains attached:

```rust
use std::collections::BTreeMap;
use smarter_diagrams::{
    compile,
    formal::{self, CheckResult, Invariant, State, Transition, TransitionSystem},
    simulation::Simulation,
};

let source = std::fs::read_to_string("examples/kube-operator.diagram")?;
let diagram = compile(&source)?;

let mut sim = Simulation::from_scene(&diagram.scene)?;
sim.set_rate("operator.informer", 220.0)?;
for _ in 0..80 {
    sim.tick()?;
}
assert!(sim.nodes["operator.workqueue"].queued > 0.0);

let model = TransitionSystem {
    states: vec![
        State {
            name: "Ready".into(),
            variables: BTreeMap::from([("volume_state".into(), "bound".into())]),
        },
        State {
            name: "DeletedCleanly".into(),
            variables: BTreeMap::from([("volume_state".into(), "reclaimed".into())]),
        },
    ],
    initial: vec![0],
    transitions: vec![Transition {
        from: 0,
        to: 1,
        event: "detach_pvc_and_clear_finalizer".into(),
        edge: None,
    }],
};
let no_orphan_pvc = Invariant::Never {
    variable: "volume_state".into(),
    value: "orphaned".into(),
};
assert!(matches!(
    formal::check(&model, &no_orphan_pvc, 12)?,
    CheckResult::SafeThrough { horizon: 12 }
));
```

Run the full example (pass an optional output directory to write both SVGs):

```sh
cargo run --locked --all-features --example kube_operator_reconcile
```

## 2. Outernet Council NMTS fragment

[Open in the editor](/demo/?example=nmts-fragment) · [Download `nmts-fragment.diagram`](/examples/nmts-fragment.diagram)

The [Outernet Council Network Modeling for Temporospatial Systems (NMTS)](https://github.com/outernetcouncil/nmts) protobuf schema (`nmts.v1.Fragment`) models terrestrial, submarine, aviation, and orbital networks as typed `Entity` vertices (`EK_PLATFORM`, `EK_NETWORK_NODE`, `EK_MODULATOR`, `EK_ANTENNA`, `EK_PHYSICAL_MEDIUM_LINK`, `EK_SDN_AGENT`, `EK_PORT`) connected by typed `Relationship` edges (`RK_CONTAINS`, `RK_TRAVERSES`, `RK_SIGNAL_TRANSITS`, `RK_CONTROLS`).

Mapping `RK_CONTAINS` into scoped containers and separating `logical` packet entities from `physical` RF/optical hardware produces a multi-stratum model where `rf_chain` isolates the 29.5 GHz Ka-band carrier path and `logical_bearer` collapses the physical RF chain into a single `via` summary hop between `Gateway Router` and `Onboard Switch`:

```diagram views=rf_chain,logical_bearer
strata [logical, physical]

gs_arlington: "GS Arlington (EK_PLATFORM)" {
  sdn_gs: "Spacetime SDN\n(EK_SDN_AGENT)" [stratum: logical]
  gs_node: "Gateway Router\n(EK_NETWORK_NODE)" [stratum: logical]
  mod_ka: "DVB-S2X Mod\n(EK_MODULATOR)" [stratum: physical]
  ant_gs: "4.5m Ka Dish\n(EK_ANTENNA)" [stratum: physical]

  sdn_gs -> mod_ka: "RK_CONTROLS"
  gs_node -> mod_ka: "RK_TRAVERSES"
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
  demod_ka -> isl_port: "RK_SIGNAL_TRANSITS"
}

gs_arlington.ant_gs -> ka_uplink: "RK_SIGNAL_TRANSITS"
ka_uplink -> leo_142.ant_sat: "RK_SIGNAL_TRANSITS"

zone contact_window "Contact window T+120s..540s" around [ka_uplink] {
  padding: 1.4g
  tint: oklch(0.80 0.09 205 / 0.18)
}
view rf_chain {
  include gs_arlington.mod_ka -> gs_arlington.ant_gs -> ka_uplink -> leo_142.ant_sat -> leo_142.demod_ka
}
view logical_bearer {
  include gs_arlington.gs_node
  include leo_142.sat_node
}
```

### Lower `nmts.v1.Fragment` JSON and diff orbital handoffs

[`examples/nmts_fragment.rs`](https://github.com/robot-head/smarter-diagrams/blob/main/examples/nmts_fragment.rs) parses an `nmts.v1.Fragment` payload, emits `.diagram` declarations, computes independent `logical` and `physical` planes with cross-stratum vias through `spatial::strata`, and renders a Spatial Git beam-handoff SVG (`T+120s` vs. `T+545s` LEO handoff) via `spatial::compare` and `spatial::diff_svg`:

```rust
use smarter_diagrams::{LayoutOptions, compile, spatial};

let window_a = compile(
    "strata [logical, physical]\n\
     gs_router: \"Gateway Router\" [stratum: logical]\n\
     ant_gs: \"4.5m Ka Dish\" [stratum: physical]\n\
     leo_142: \"LEO-142 Array\" [stratum: physical]\n\
     gs_router -> ant_gs: \"RK_TRAVERSES\"\n\
     ant_gs -> leo_142: \"RK_SIGNAL_TRANSITS\"\n",
)?;
let stack = spatial::strata(&window_a.scene, &LayoutOptions::default(), 0.35)?;
assert_eq!(stack.planes.len(), 2);

let window_b = compile(
    "strata [logical, physical]\n\
     gs_router: \"Gateway Router\" [stratum: logical]\n\
     ant_gs: \"4.5m Ka Dish\" [stratum: physical]\n\
     leo_143: \"LEO-143 Array\" [stratum: physical]\n\
     gs_router -> ant_gs: \"RK_TRAVERSES\"\n\
     ant_gs -> leo_143: \"RK_SIGNAL_TRANSITS\"\n",
)?;
let handoff = spatial::compare(&window_a.scene, &window_b.scene, &window_a.layout, &LayoutOptions::default())?;
let handoff_svg = spatial::diff_svg(&handoff, 1.0)?;
assert!(handoff_svg.contains("<svg"));
```

Run the NMTS fragment pipeline:

```sh
cargo run --locked --all-features --example nmts_fragment
```

## 3. Grafana service graph and OTLP trace drift

[Open in the editor](/demo/?example=grafana-service-graph) · [Download `grafana-service-graph.diagram`](/examples/grafana-service-graph.diagram)

Grafana Tempo and Grafana Alloy's `servicegraph` connector pair client and server spans into Prometheus series (`traces_service_graph_request_total`, `traces_service_graph_request_failed_total`, `traces_service_graph_request_server_seconds_sum`, and `connection_type="database" | "messaging_system" | "virtual_node"`). Lowering those RED metrics into `.diagram` source attaches `@bind(otel: "service.name=...")` and `@sim(...)` capacities to every node while highlighting error-budget burn zones.

Switch between **Canonical graph** and **View: slo_hotpath** below:

```diagram views=slo_hotpath
ingress: "ingress-nginx" @sim(rate: 240, service_rate: 420) @bind(otel: "service.name=ingress-nginx")
frontend: "shop-frontend" @sim(service_rate: 300, queue_cap: 220) @bind(otel: "service.name=shop-frontend")
catalog: "catalog-api" @sim(service_rate: 240, queue_cap: 160) @bind(otel: "service.name=catalog-api")
checkout: "checkout-api" @sim(service_rate: 95, queue_cap: 120, retry: exp_backoff(0.3, 0.2)) @bind(otel: "service.name=checkout-api")
redis: "redis-cart" [shape: cache] @sim(service_rate: 520) @bind(otel: "service.name=redis-cart")
orders_db: "postgres-orders" [shape: database] @sim(service_rate: 150) @bind(otel: "service.name=postgres-orders")
kafka: "kafka-orders" [shape: queue] @sim(service_rate: 200, queue_cap: 240) @bind(otel: "service.name=kafka-orders")
payments: "payment-gw\n(virtual)" @sim(service_rate: 55, queue_cap: 80) @bind(otel: "service.name=payment-gw")

ingress -> frontend: "240 rps"
frontend -> catalog: "165 rps"
frontend -> checkout: "1.9% err"
catalog -> redis: "1ms"
checkout -> orders_db: "14ms"
checkout -> kafka: "60 rps"
checkout -> payments: "p95 184ms"

zone slo_burn "SLO error budget burn" around [payments] {
  padding: 1.15g
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

### Reconcile live OTLP spans against the declared service graph

[`examples/grafana_service_graph.rs`](https://github.com/robot-head/smarter-diagrams/blob/main/examples/grafana_service_graph.rs) generates the service graph from Tempo RED edge metrics, reconciles live OTLP `SpanRecord` traces via `drift::observed_from_spans` and `drift::reconcile`, renders phantom undeclared calls (`checkout-api -> redis-cart`) with `drift::hydrate`, and synthesizes a lossless `TextPatch` via `drift::promote`:

```rust
use smarter_diagrams::{
    LayoutOptions, compile,
    drift::{self, SpanRecord},
};

let source = std::fs::read_to_string("examples/grafana-service-graph.diagram")?;
let diagram = compile(&source)?;

let spans = [
    SpanRecord {
        trace_id: "4bf92f3577b34da6".into(),
        span_id: "03".into(),
        parent_span_id: None,
        service: "checkout-api".into(),
    },
    SpanRecord {
        trace_id: "4bf92f3577b34da6".into(),
        span_id: "04".into(),
        parent_span_id: Some("03".into()),
        service: "redis-cart".into(),
    },
];
let observed = drift::observed_from_spans(&diagram.scene, &spans);
let report = drift::reconcile(&diagram.scene, &observed, None);
assert_eq!(report.phantom.len(), 1);

let hydrated = drift::hydrate(&source, &observed, &LayoutOptions::default(), &diagram.layout)?;
assert!(hydrated.to_svg().contains("data-drift"));
let patch = drift::promote(&source, &report.phantom[0])?;
assert!(patch.insert_text.contains("checkout -> redis"));
```

Run the Grafana Tempo + OTLP drift demonstration:

```sh
cargo run --locked --all-features --example grafana_service_graph
```
