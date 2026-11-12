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
      text: 21 examples
      link: /examples
    - theme: alt
      text: Demonstrations
      link: /guide/demonstrations
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

markdown.E -> formatter.W: "fenced block"
formatter.E -> output.W: "inline SVG"
formatter.S <-> compiler.N: "compile"

align row [markdown, formatter, output]
distribute row [markdown, formatter, output] [gap: 8g]
align col [formatter, compiler]
distribute col [formatter, compiler] [gap: 5.5g]
```

</div>

When topology grows, add scoped containers, exact `sql_table` record ports, and non-owning boundary zones without rewriting earlier declarations:

<div class="sd-split">

```diagram
client: "Client" [shape: actor]
prod: Production {
  gateway: "API gateway"
  orders: "Orders" [shape: sql_table] {
    port id: uuid [side: W, pos: 1]
    port status: state [side: E, pos: 2]
  }
  audit: "Audit log" [shape: queue]
  gateway -> orders#id: "insert"
  orders#status -> audit: "emit"
}
client -> prod.gateway: HTTPS
zone pci "PCI boundary" around [prod.orders, prod.audit] {
  padding: 1.2g
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

</div>

### Outernet Council NMTS fragment

Lowers an [`nmts.v1.Fragment`](https://github.com/outernetcouncil/nmts) entity-relationship graph (`EK_PLATFORM`, `EK_NETWORK_NODE`, `EK_MODULATOR`, `EK_ANTENNA`, `EK_PHYSICAL_MEDIUM_LINK`, `EK_SDN_AGENT`, `EK_PORT`) across `logical` and `physical` strata. Switch to **View: logical_bearer** to collapse the 29.5 GHz RF chain into one summary hop.

[Open in the editor](/demo/?example=nmts-fragment) · [Download source](/examples/nmts-fragment.diagram) · [Fragment lowering & orbital handoff diff →](/guide/demonstrations#_2-outernet-council-nmts-fragment)

<div class="sd-wide">

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

</div>

### Grafana service graph and OTLP trace drift

Renders Grafana Tempo / Alloy `traces_service_graph_request_*` RED span metrics with OpenTelemetry `@bind(otel: ...)` attributes, SLO error-budget burn zones, and an `slo_hotpath` projection.

[Open in the editor](/demo/?example=grafana-service-graph) · [Download source](/examples/grafana-service-graph.diagram) · [OTLP phantom-call detection →](/guide/demonstrations#_3-grafana-service-graph-and-otlp-trace-drift)

<div class="sd-split">

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

</div>

<p class="sd-kicker">After Pikchr &amp; Fossil</p>

## Version-control histories and peer swimlanes

Inspired by Pikchr's commit DAGs and distributed check-in diagrams in the Fossil SCM documentation, row alignments, side ports, and `view` projections keep parallel histories legible without manual pixel coordinates.

### Branch, merge, and rebase commit DAG

Compare the combined ancestry against **View: merge_history** (preserving branch context into `C7`) and **View: rebase_history** (replaying `C3' -> C5'` atop `C4` while leaving original check-ins visible).

[Open in the editor](/demo/?example=fossil-branches) · [Download source](/examples/fossil-branches.diagram)

<div class="sd-wide">

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

</div>

### Distributed peer check-in swimlanes

Alan forks locally while Betty and offline peer Charlie commit independently; Darlene pulls both lineages and records merge check-in `6`.

[Open in the editor](/demo/?example=fossil-swimlanes) · [Download source](/examples/fossil-swimlanes.diagram)

<div class="sd-wide">

```diagram
alan: "Alan (origin)" {
  a1: "check-in 1"
  a3: "fork 3"
  a2: "check-in 2"
  a1 -> a3: fork
  a1 -> a2: trunk
}
betty: "Betty (peer)" {
  b3: "pull 3"
  b4: "check-in 4"
  b3 -> b4: branch
}
charlie: "Charlie (offline)" {
  c2: "clone 2"
  c5: "offline 5"
  c2 -> c5
}
darlene: "Darlene (merge)" {
  d4: "pull 3,4"
  d5: "pull 5"
  d6: "merge 6"
  d4 -> d6
  d5 -> d6: merge
}

alan.a3 -> betty.b3: sync
alan.a2 -> charlie.c2: clone
betty.b4 -> darlene.d4: push
charlie.c5 -> darlene.d5: push

flow converge "Distributed convergence" {
  alan.a1 -> alan.a2 -> charlie.c2 -> charlie.c5 -> darlene.d5 -> darlene.d6
}
```

</div>

<p class="sd-kicker">Architecture &amp; runtime topologies</p>

## Multi-tier architecture layouts

Modeled after multi-tier architecture studies from [D2's TALA showcase](https://d2lang.com/examples/tala) and Pikchr's subsystem diagrams, nested containers, SQL record slots, and automatic ownership packing produce deterministic orthogonal routes.

### Institutional securities and risk data mesh

Master reference tables expose typed record ports (`#limits`, `#party_id`, `#isin`, `#curve`) across trading desks and margin engines while a non-owning regulatory zone groups audited settlement stages.

[Open in the editor](/demo/?example=tala-securities-mesh) · [Download source](/examples/tala-securities-mesh.diagram)

<div class="sd-wide">

```diagram
reference: "Reference Data" {
  clients: "Client Master" [shape: sql_table] {
    port limits: tier [side: E, pos: 1]
    port party_id: uuid [side: E, pos: 2]
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
  credit -> ledger: "margin ok"
  greeks -> ledger: "VaR"
}

reference.clients#limits -> risk.credit.N: limits
reference.clients#party_id -> desks.equities: accounts
reference.securities#isin -> desks.fixed_inc: isin
reference.securities#curve -> risk.greeks: curves
desks.alloc -> risk.credit: trades

zone audit "Regulatory trade surveillance" around [risk.credit, risk.ledger] {
  padding: 1.25g
  tint: oklch(0.79 0.08 185 / 0.16)
}
flow order_lifecycle "Order to settlement" {
  reference.clients#party_id -> desks.equities -> desks.alloc -> risk.credit -> risk.ledger
}
```

</div>

### Producer-consumer ring buffer and worker pool

Three producers batch tasks into a `Queue Runtime` ring buffer fanning out across three workers. Open it in the editor and press **Run simulation** to saturate the dispatcher or cut individual worker links.

[Open in the editor](/demo/?example=tala-ring-queue) · [Download source](/examples/tala-ring-queue.diagram)

<div class="sd-wide">

```diagram
p1: "Producer 01" @sim(rate: 40, service_rate: 120)
p2: "Producer 02" @sim(rate: 40, service_rate: 120)
p3: "Producer 03" @sim(rate: 40, service_rate: 120)
api: "Task Ingress" @sim(service_rate: 150, queue_cap: 180)

queue_lib: "Queue Runtime" {
  writer: "Slot Writer" [shape: rect]
  ring: "Ring Buffer" [shape: queue] @sim(service_rate: 130, queue_cap: 160)
  disp: "Dispatcher" [shape: rect] @sim(service_rate: 135, queue_cap: 100)
  writer -> ring: enqueue
  ring -> disp: claim
}

w1: "Worker 01" @sim(service_rate: 45, queue_cap: 40)
w2: "Worker 02" @sim(service_rate: 45, queue_cap: 40)
w3: "Worker 03" @sim(service_rate: 45, queue_cap: 40)

p1 -> api: submit
p2 -> api: submit
p3 -> api: submit
api -> queue_lib.writer: batch
queue_lib.disp -> w1: dispatch
queue_lib.disp -> w2: dispatch
queue_lib.disp -> w3: dispatch
```

</div>

### GitHub OIDC rollout into an isolated cloud VPC

Short-lived OIDC deployment scope groups `Auth Handler` and `Order Handler` inside `Production Cloud VPC` without reparenting handlers out of their network boundary.

[Open in the editor](/demo/?example=tala-cicd-vpc) · [Download source](/examples/tala-cicd-vpc.diagram)

<div class="sd-wide">

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
  ledger: "State Table" [shape: database]
  apigw -> fn_auth: verify
  apigw -> fn_orders: execute
  fn_auth -> ledger: session
  fn_orders -> ledger: commit
}

github.workflow -> vpc.fn_auth: "deploy"
github.workflow -> vpc.fn_orders: "deploy"

zone oidc "Short-lived OIDC deploy scope" around [vpc.fn_auth, vpc.fn_orders] {
  padding: 1.2g
  tint: oklch(0.80 0.08 165 / 0.16)
}
```

</div>

### Layered embedded SQL engine

Three subsystems (`Core`, `SQL Compiler`, and `Storage Backend`) trace a query path from `C API Interface` through bytecode generation into `B-Tree`, `Pager`, and `OS VFS`.

[Open in the editor](/demo/?example=sqlite-layers) · [Download source](/examples/sqlite-layers.diagram)

<div class="sd-wide">

```diagram
core: Core {
  iface: "C API\nInterface"
  cmd: "Command\nProcessor"
  vm: "Virtual\nMachine"
  iface -> cmd: call
}
compiler: "SQL Compiler" {
  tok: Tokenizer
  parser: Parser
  codegen: "Code\nGenerator"
  tok -> parser: tokens
  parser -> codegen: AST
}
backend: "Storage Backend" {
  btree: "B-Tree"
  pager: "Pager" [shape: cache]
  vfs: "OS VFS" [shape: database]
  btree -> pager: pages
  pager -> vfs: "I/O"
}

core.cmd -> compiler.tok: "SQL text"
compiler.codegen -> core.vm: "bytecode"
core.vm -> backend.btree: "cursors"
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
