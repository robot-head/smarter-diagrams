# Queue simulation

Simulation is a conservative fixed-step fluid queue model. It approximates rates, queued work and retry feedback; it is not a packet-level simulator or a prediction of production latency.

```diagram
client @sim(rate: 120, service_rate: 200)
worker @sim(service_rate: 50, queue_cap: 100, workers: 1, timeout: 0.5, retry: exp_backoff(0.5, 0.2))
db @sim(service_rate: 80)
client -> worker -> db
```

## Traits and units

| Trait | Meaning | Default |
| --- | --- | ---: |
| `rate` | External arrivals per second | 0 |
| `workers` | Parallel service capacity multiplier | 1 |
| `service_rate` | Work serviced per second per worker | 100 |
| `queue_cap` | Maximum queued work | 1000 |
| `timeout` | Timeout duration in seconds | 1 |
| `retry_fraction` | Fraction of failed work retried | 0 |
| `retry_backoff` | Retry delay in seconds | 1 |

`service: lognormal(mu, sigma)` derives mean service capacity from the lognormal mean. `retry: exp_backoff(delay, fraction)` sets a delayed feedback approximation. Non-finite or invalid traits return errors. Ordinary ownership containers do not represent service queues; SQL records can.

## Rust API

```rust
let diagram = smarter_diagrams::compile(
    "client @sim(rate: 80)\nworker @sim(service_rate: 50)\nclient -> worker\n"
)?;
let mut sim = smarter_diagrams::simulation::Simulation::from_scene(&diagram.scene)?;
sim.advance(2.0)?;
sim.set_rate("client", 120.0)?;
sim.sever("client->worker", true)?;
sim.tick()?;
println!("Queued: {}", sim.nodes["worker"].queued);
```

`tick()` advances one 1/60-second step. `advance(seconds)` advances a bounded number of fixed steps. Node state exposes queued work, arrivals, throughput, cumulative drops, saturation, retry rate and processed work. A severed edge stops forwarding; it does not remove the source-level declaration.

## Browser controls

Open the [simulation example](/demo/?example=simulation), press **Run simulation**, then increase **Ingress**. Shift-click a connector to cut or restore it. The worker initializes the separate runtime Wasm bundle on demand and serializes commands during startup.

The ingress slider in this example controls the `client` node. When using other sources, include that node for the slider or use `WasmSimulation.set_rate(node, rate)` in an embedding application. Text edits reopen simulation from the new source, resetting its state.
