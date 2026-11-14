# Executable diagrams & queue simulation

## 1. What executable diagrams are (and why use them)

Static architecture boxes show *who talks to whom*, not *where queues back up when traffic spikes or an edge breaks*. An **executable diagram** keeps the exact same `.diagram` source as documentation, layout, and model:

- **`@sim(...)` queue traits** turn any service, queue, or database into a 60 Hz fluid capacity model with bounded queues, drop counters, and exponential retry backpressure.
- **Coupled Harel statecharts** observe queue telemetry (such as `workers.saturation >= 0.85`) and trigger state transitions that execute `sever(...)`, `restore(...)`, or `set_rate(...)` in real time.

Try it right here without leaving the page—press **Play**, drag the **Ingress** slider past worker capacity (`36 req/s`), or **Shift-click** an edge to cut traffic:

<SimulationEmbed mode="coupled" title="Interactive coupled queue + circuit-breaker simulation" />

Notice how **Worker Pool** reaches capacity (`2 workers × 18/s = 36/s`), triggers the guarded `[workers.saturation >= 0.85]` transition inside **Circuit Breaker**, moves the active outline to **Open**, and automatically severs `api -> workers` until you click **probe**.

---

## 2. Codelab: From a static diagram to a live self-healing model

Start from a plain static `.diagram` file and layer executable behavior one step at a time.

### Step 1 — Start with a static service diagram

Begin with standard nodes, shapes, and directed edges. Nothing simulates yet—this is already a valid diagram with clean orthogonal layout:

```diagram
client: "Client"
worker: "Worker"
db: "Database" [shape: database]

client -> worker: "requests"
worker -> db: "queries"
align row [client, worker, db]
```

### Step 2 — Add ingress rate, workers, and queue capacity

Attach `@sim(...)` annotations directly to the nodes you want to stress-test. `rate` injects traffic; `workers`, `service_rate`, and `queue_cap` define processing throughput and finite queue bounds:

```diagram
client: "Client" @sim(rate: 80)
worker: "Worker" @sim(workers: 2, service_rate: 30, queue_cap: 100)
db: "Database" [shape: database] @sim(service_rate: 80)

client -> worker: "requests"
worker -> db: "queries"
align row [client, worker, db]
```

At `rate: 80` against `2 × 30 = 60` worker capacity, queue occupancy steadily climbs toward `100` until excess load begins dropping.

### Step 3 — Add delayed retry storms (`exp_backoff`)

Real systems retry failed or timed-out work, amplifying congestion even after upstream spikes subside. Add `timeout` and `retry: exp_backoff(delay, fraction)` to `worker`:

```diagram
client: "Client" @sim(rate: 80, service_rate: 200)
worker: "Worker" @sim(workers: 2, service_rate: 25, queue_cap: 100, timeout: 0.5, retry: exp_backoff(0.5, 0.2))
db: "Database" [shape: database] @sim(service_rate: 80)

client -> worker: "requests"
worker -> db: "queries"
align row [client, worker, db]
```

### Step 4 — Couple a circuit breaker statechart

Declare a compact statechart alongside your services using `[shape: initial]` and a guarded transition `[worker.saturation >= 0.85] / sever(client -> worker)`:

```diagram
client: "Client" @sim(rate: 85, service_rate: 200)
worker: "Worker" @sim(workers: 2, service_rate: 18, queue_cap: 25)
db: "Database" [shape: database] @sim(service_rate: 80)
client -> worker: "requests"
worker -> db: "queries"

breaker [label: "Breaker"] {
  init [shape: initial]
  closed [label: "Closed"]
  open [label: "Open"]
  init -> closed
  closed -> open: "[worker.saturation >= 0.85] / sever(client -> worker)"
  open -> closed: "reset / restore(client -> worker)"
}
```

When `worker.saturation` crosses `85%`, the statechart automatically fires into **Open** and cuts `client -> worker`, draining the backlog cleanly. Read the full [Statecharts guide](/runtime/statecharts) for hierarchy, parallel regions, and shallow/deep history.

---

## 3. Advanced topics & animated export

### Fluid queue model mechanics

Simulation is a deterministic 60 Hz (`tick()` = 1/60 s) conservative fluid approximation—not a discrete-packet simulator:

- `rate`: constant source arrival rate into a node, configurable live via `set_rate`.
- `workers`, `service_rate` (or `lognormal(mu, sigma)`): per-step service capacity equals `workers × service_rate`.
- `queue_cap`: finite buffer before excess arrivals increment cumulative `dropped`.
- `retry: exp_backoff(delay, fraction)`: schedules delayed retry pulses from dropped/timed-out fraction.
- `sever(edge)`: stops traffic propagation without editing source syntax or re-running layout.

```rust,no_run
use smarter_diagrams::{compile, simulation::Simulation};

let d = compile(
    "client @sim(rate: 80)\nworker @sim(service_rate: 50)\nclient -> worker\n"
)?;
let mut sim = Simulation::from_scene(&d.scene)?;
sim.set_rate("client", 120.)?;
sim.advance(2.)?;
sim.sever("client->worker", true)?;
```

### Export simulations to GIF, APNG, and WebM

Render deterministic multi-frame animations offline from the CLI (requiring `--features export`) or capture interactive browser sessions directly to **WebM** via **Export WebM** above or **Record WebM** in the [Playground](/playground):

```sh
# Render coupled queue + circuit-breaker simulation to animated GIF
cargo run --features export -- examples/coupled-circuit-breaker.diagram \
  -o circuit-breaker-sim.gif --format gif --fps 4 --frames 12

# Render scripted Harel statechart event sequence to Animated PNG (APNG)
cargo run --features export -- examples/order-statechart.diagram \
  -o order-statechart.apng --format apng --fps 2 \
  --events "submit,approve,decline,retry,capture,pack,hold,resume_deep,complete"
```

![Coupled queue and circuit-breaker GIF export](/figures/circuit-breaker-sim.gif)

For programmatic embedding, pair [`Simulation`](../api/smarter_diagrams/simulation/struct.Simulation.html) and [`Statechart`](../api/smarter_diagrams/statecharts/struct.Statechart.html) in Rust, or `WasmSimulation` / `WasmStatechart` in WebAssembly (`web/pkg-runtime`).
