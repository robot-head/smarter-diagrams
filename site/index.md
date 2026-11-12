---
layout: home
hero:
  name: Smarter Diagrams
  text: Declarative diagrams that survive edits, diffs, and telemetry.
  tagline: Author architecture in plain text, refine geometry on a canvas without losing comments, inline zero-JS SVGs in Hugo or Markdown, and run queue simulations or trace drift checks in Rust and WebAssembly.
  image:
    light: /generated/markdown-pipeline.svg
    dark: /generated/markdown-pipeline-dark.svg
    alt: Markdown fenced diagram compiled by Hugo or VitePress and Smarter Diagrams into HTML and SVG
  actions:
    - theme: brand
      text: Try the playground
      link: /playground
    - theme: alt
      text: Browse 21 examples
      link: /examples
    - theme: alt
      text: Domain demonstrations
      link: /guide/demonstrations
    - theme: alt
      text: Hugo plugin
      link: /guide/hugo
---

<div class="sd-pillars">
<div>

**Markdown & Hugo fences**

Inline zero-JS build-time SVGs or live Wasm figures from fenced `diagram` blocks in [Hugo](/guide/hugo) and VitePress.

</div>
<div>

**Lossless source edits**

Rowan CST patches write canvas `@pin` constraints back into source without disturbing comments, formatting, or undo history.

</div>
<div>

**Five-tier ports & views**

Combine free attachments, fixed sides, SQL record rows, non-owning security zones, and canonical `view` projections.

</div>
<div>

**Executable topology**

Simulate bounded queues, check finite-state SAT invariants, separate 2.5D strata, and reconcile OpenTelemetry trace drift.

</div>
</div>

<p class="sd-kicker">Progressive notation</p>

## Your first diagram

Like [Pikchr](https://pikchr.org/home/doc/trunk/homepage.md) in Fossil, a diagram starts as plain text inside a fenced Markdown code block so architecture review happens in ordinary diffs. Start with two connections, or align nodes into an explicit pipeline:

[Open in the editor](/demo/?example=markdown-pipeline) · [Download source](/examples/markdown-pipeline.diagram) · [Hugo fenced blocks →](/guide/hugo)

<div class="sd-split">

```diagram
markdown: "Markdown\nsource"
formatter: "Site engine\n(Hugo / VitePress)"
compiler: "Smarter Diagrams\n(CLI or Wasm)"
output: "HTML + SVG\ndocument"
markdown.E -> formatter.W: "fenced diagram"
formatter.E -> output.W: "accessible SVG"
formatter.S <-> compiler.N: "compile / source"
align row [markdown, formatter, output]
align col [formatter, compiler]
```

</div>

When topology grows, add scoped containers, exact `sql_table` record ports, and non-owning boundary zones without rewriting earlier declarations:

<div class="sd-split">

```diagram
client: "Web client" [shape: person]
prod: Production {
  gateway: "API gateway"
  orders: "Orders table" [shape: sql_table] {
    port id: uuid [side: W, pos: 1]
    port status: state [side: E, pos: 2]
  }
  audit: "Audit log" [shape: queue]
  gateway -> orders#id: "insert"
  orders#status -> audit: "emit"
}
client -> prod.gateway: HTTPS
zone pci "PCI boundary" around [prod.orders, prod.audit] {
  padding: 1.5g
  tint: oklch(0.76 0.11 45 / 0.14)
}
```

</div>

Embed the compiler directly in Rust (`cargo install --path .` for the CLI, or depend on the repository from `Cargo.toml`):

```rust
let diagram = smarter_diagrams::compile("client -> gateway: HTTPS\ngateway -> postgres: SQL\n")?;
std::fs::write("architecture.svg", diagram.to_svg())?;
```

See [getting started](/guide/getting-started), the [21-example gallery](/examples), and the [complete Rust API](https://robot-head.github.io/smarter-diagrams/api/smarter_diagrams/index.html).

<p class="sd-kicker">Real-world systems</p>

## Domain demonstrations

Each demonstration pairs a declarative `.diagram` model with a runnable Rust binary in `examples/` that exercises projections, fluid queue simulation, SAT safety checking, temporospatial strata, or OpenTelemetry span reconciliation.

### Kubernetes operator reconciliation loop

Level-triggered controller loop with separate `#spec` and `#status` custom-resource ports, informer/indexer caching, a rate-limiting workqueue with exponential backoff, RBAC scope zone, and projected `control_loop` and `owned_resources` views.

[Open in the editor](/demo/?example=kube-operator) · [Download source](/examples/kube-operator.diagram) · [Simulation & SAT invariant walkthrough →](/guide/demonstrations#_1-kubernetes-operator-reconciliation-loop)

<div class="sd-wide">

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

</div>

### Outernet Council NMTS fragment

Lowers an [`nmts.v1.Fragment`](https://github.com/outernetcouncil/nmts) entity-relationship graph (`EK_PLATFORM`, `EK_NETWORK_NODE`, `EK_INTERFACE`, `EK_MODULATOR`, `EK_ANTENNA`, `EK_PHYSICAL_MEDIUM_LINK`, `EK_SDN_AGENT`) across `logical` and `physical` strata. Switch to **View: logical_bearer** to collapse the 29.5 GHz RF chain into one summary hop.

[Open in the editor](/demo/?example=nmts-fragment) · [Download source](/examples/nmts-fragment.diagram) · [Fragment lowering & orbital handoff diff →](/guide/demonstrations#_2-outernet-council-nmts-fragment)

<div class="sd-wide">

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

</div>

### Grafana service graph and OTLP trace drift

Renders Grafana Tempo / Alloy `traces_service_graph_request_*` RED span metrics with OpenTelemetry `@bind(otel: ...)` attributes, SLO error-budget burn zones, `slo_hotpath` projection, and 1-hop causal focus around `checkout-api`.

[Open in the editor](/demo/?example=grafana-service-graph) · [Download source](/examples/grafana-service-graph.diagram) · [OTLP phantom-call detection →](/guide/demonstrations#_3-grafana-service-graph-and-otlp-trace-drift)

<div class="sd-split">

```diagram views=slo_hotpath focus=checkout
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

</div>

<p class="sd-kicker">After Pikchr &amp; Fossil</p>

## Version-control histories and peer swimlanes

Inspired by Pikchr's commit DAGs and distributed check-in diagrams in the Fossil SCM documentation, row alignments, side ports, and `view` projections keep parallel histories legible without manual pixel coordinates.

### Branch, merge, and rebase commit DAG

Compare the combined ancestry against **View: merge_history** (preserving branch context into `C7`) and **View: rebase_history** (replaying `C3' -> C5'` atop `C6` while leaving original check-ins visible).

[Open in the editor](/demo/?example=fossil-branches) · [Download source](/examples/fossil-branches.diagram)

<div class="sd-wide">

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

</div>

### Distributed peer check-in swimlanes

Alan forks locally while Betty and offline peer Charlie commit independently; Darlene pulls both lineages and records merge check-in `6`.

[Open in the editor](/demo/?example=fossil-swimlanes) · [Download source](/examples/fossil-swimlanes.diagram)

<div class="sd-wide">

```diagram
alan: "Alan (origin)" {
  a1: "check-in 1"
  a2: "check-in 2"
  a3: "fork 3"
  a1 -> a2: trunk
  a1 -> a3: "fork!"
}
betty: "Betty (peer)" {
  b3: "pull 3"
  b4: "check-in 4"
  b3 -> b4: branch
}
charlie: "Charlie (offline)" {
  c2: "clone 2"
  c5: "offline 5"
  c2 -> c5: local
}
darlene: "Darlene (merge)" {
  d4: "pull 3,4"
  d5: "pull 5"
  d6: "merge 6"
  d4 -> d6
  d5 -> d6: merge
}

alan.a3.E -> betty.b3.W: sync
alan.a2.E -> charlie.c2.W: clone
betty.b4.E -> darlene.d4.W: push
charlie.c5.E -> darlene.d5.W: push

flow converge "Distributed convergence" {
  alan.a1 -> alan.a2 -> charlie.c2 -> charlie.c5 -> darlene.d5 -> darlene.d6
}
```

</div>

<p class="sd-kicker">Architecture &amp; runtime topologies</p>

## Multi-tier architecture layouts

Modeled after multi-tier architecture studies from [D2's TALA showcase](https://d2lang.com/examples/tala) and Pikchr's subsystem diagrams, nested containers, SQL record slots, and column alignments produce deterministic orthogonal routes.

### Institutional securities and risk data mesh

Master reference tables expose typed record ports (`#party_id`, `#limits`, `#isin`, `#curve`) across trading desks and margin engines while a non-owning regulatory zone groups audited settlement stages.

[Open in the editor](/demo/?example=tala-securities-mesh) · [Download source](/examples/tala-securities-mesh.diagram)

<div class="sd-wide">

```diagram
reference: "Reference Data" {
  clients: "Client Master" [shape: sql_table] {
    port party_id: uuid [side: E, pos: 1]
    port limits: tier [side: E, pos: 2]
  }
  securities: "Security Master" [shape: sql_table] {
    port isin: text [side: E, pos: 1]
    port curve: rate [side: E, pos: 2]
  }
}
desks: "Trading Desks" {
  equities: "Equities OMS"
  fixed_inc: "Fixed Income"
  alloc: "Allocations"
  equities -> alloc: fills
  fixed_inc -> alloc: blocks
}
risk: "Risk & Margin" {
  credit: "Credit Guard"
  greeks: "Global Greeks"
  ledger: "Settlement Bus" [shape: queue]
  credit -> greeks: approved
  greeks -> ledger: exposure
}

reference.clients#party_id -> desks.equities: accounts
reference.clients#limits -> risk.credit: "limit check"
reference.securities#isin -> desks.fixed_inc: instruments
reference.securities#curve -> risk.greeks: "yield curves"
desks.equities -> risk.credit: orders
desks.alloc -> risk.ledger: trades

zone audit "Regulatory trade surveillance" around [desks.alloc, risk.credit, risk.ledger] {
  padding: 1.5g
  tint: oklch(0.79 0.08 185 / 0.15)
}
flow order_lifecycle "Order to settlement" {
  reference.clients#party_id -> desks.equities -> risk.credit -> risk.greeks -> risk.ledger
}
```

</div>

### Producer-consumer ring buffer and worker pool

Three aligned producers batch tasks into a `Queue Runtime` ring buffer fanning out across four workers. Open it in the editor and press **Run simulation** to saturate the dispatcher or cut individual worker links.

[Open in the editor](/demo/?example=tala-ring-queue) · [Download source](/examples/tala-ring-queue.diagram)

<div class="sd-wide">

```diagram
p1: "Producer 01" @sim(rate: 35, service_rate: 120)
p2: "Producer 02" @sim(rate: 35, service_rate: 120)
p3: "Producer 03" @sim(rate: 35, service_rate: 120)
api: "Task Ingress" @sim(service_rate: 140, queue_cap: 180)

queue_lib: "Queue Runtime" {
  writer: "Slot Writer"
  ring: "Ring Buffer" [shape: queue] @sim(service_rate: 110, queue_cap: 160)
  disp: "Dispatcher" @sim(service_rate: 120, queue_cap: 100)
  writer -> ring: enqueue
  ring -> disp: claim
}

w1: "Worker 01" @sim(service_rate: 35, queue_cap: 40)
w2: "Worker 02" @sim(service_rate: 35, queue_cap: 40)
w3: "Worker 03" @sim(service_rate: 35, queue_cap: 40)
w4: "Worker 04" @sim(service_rate: 35, queue_cap: 40)

p1 -> api: submit
p2 -> api: submit
p3 -> api: submit
api -> queue_lib.writer: batch
queue_lib.disp -> w1: dispatch
queue_lib.disp -> w2: dispatch
queue_lib.disp -> w3: dispatch
queue_lib.disp -> w4: dispatch

align col [p1, p2, p3]
align col [w1, w2, w3, w4]
flow task_path "Ring buffer dispatch" {
  p1 -> api -> queue_lib.writer -> queue_lib.ring -> queue_lib.disp -> w1
}
```

</div>

### GitHub OIDC rollout into an isolated cloud VPC

Short-lived OIDC deployment scope spans across `GitHub` and `Production Cloud VPC` without reparenting handlers out of their network boundary.

[Open in the editor](/demo/?example=tala-cicd-vpc) · [Download source](/examples/tala-cicd-vpc.diagram)

<div class="sd-split">

```diagram
github: "GitHub" {
  repo: "Service Repo"
  workflow: "Actions Runner\n(OIDC deploy)"
  repo -> workflow: "push main"
}
vpc: "Production Cloud VPC" {
  apigw: "API Gateway"
  fn_auth: "Auth Handler"
  fn_orders: "Order Handler"
  fn_events: "Audit Handler"
  ledger: "State Table" [shape: database]
  apigw -> fn_auth: verify
  apigw -> fn_orders: execute
  fn_orders -> ledger: commit
  fn_events -> ledger: append
}

github.workflow -> vpc.fn_auth: "update bundle"
github.workflow -> vpc.fn_orders: "update bundle"
github.workflow -> vpc.fn_events: "update bundle"

zone oidc "Short-lived OIDC deploy scope" around [github.workflow, vpc.fn_auth, vpc.fn_orders, vpc.fn_events] {
  padding: 1.5g
  tint: oklch(0.80 0.08 165 / 0.14)
}
flow rollout "Zero-secret release path" {
  github.repo -> github.workflow -> vpc.fn_orders -> vpc.ledger
}
```

</div>

### Layered embedded SQL engine

Three subsystems (`Core`, `SQL Compiler`, and `Storage Backend`) trace a nine-step query path from `C API Interface` through bytecode generation into the `B-Tree`, `Page Cache`, and `OS VFS`.

[Open in the editor](/demo/?example=sqlite-layers) · [Download source](/examples/sqlite-layers.diagram)

<div class="sd-split">

```diagram
core: Core {
  iface: "C API\nInterface"
  cmd: "SQL Command\nProcessor"
  vm: "Virtual\nMachine"
  iface -> cmd -> vm
}
compiler: "SQL Compiler" {
  tok: Tokenizer
  parser: Parser
  codegen: "Bytecode\nGenerator"
  tok -> parser -> codegen
}
backend: "Storage Backend" {
  btree: "B-Tree\nCursor"
  pager: "Page Cache" [shape: cache]
  vfs: "OS VFS" [shape: database]
  btree -> pager -> vfs
}

core.cmd -> compiler.tok: "SQL text"
compiler.codegen -> core.vm: "prepared VDBE"
core.vm -> backend.btree: "row ops"
flow query "Prepared query execution" {
  core.iface -> core.cmd -> compiler.tok -> compiler.parser -> compiler.codegen -> core.vm -> backend.btree -> backend.pager -> backend.vfs
}
```

</div>

<p class="sd-kicker">Tooling &amp; publishing</p>

## Publish in Hugo, Markdown, CLI, and editors

Mount [`extensions/hugo`](https://github.com/robot-head/smarter-diagrams/tree/main/extensions/hugo) in a Hugo site to render fenced `diagram` and `smarter-diagrams` blocks as zero-JS inline SVGs at build time or live via WebAssembly:

```toml
theme = ["smarter-diagrams"]

[params.smarterDiagrams]
  mode = "auto"
  showSource = "details"
```

Export standalone assets, check geometry in CI, or serve revision-checked [MCP tools](/guide/mcp) from the same binary:

```sh
smarter-diagrams examples/kube-operator.diagram --view control_loop -o reconcile.svg
smarter-diagrams examples/nmts-fragment.diagram --format pdf -o nmts-fragment.pdf
node extensions/hugo/scripts/prerender.mjs --site ./docs-site --bin smarter-diagrams
```

Explore the [Hugo guide](/guide/hugo), [editor previews for VS Code and Neovim](/guide/editors), [architecture notes](/reference/architecture), and [performance measurements](/reference/performance).
