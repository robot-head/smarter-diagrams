---
layout: home
hero:
  name: Smarter Diagrams
  text: Text in. Diagrams out.
  tagline: A Rust library for diagrams you can author in text, refine on a canvas, and keep in version control.
  image:
    src: /generated/hello.svg
    alt: A web client connected to an API and Postgres
  actions:
    - theme: brand
      text: Try the playground
      link: /playground
    - theme: alt
      text: Start with Rust
      link: /guide/getting-started
features:
  - title: Start with a connection
    details: Write client → API → database. Add containers, ports and constraints as the diagram grows.
    link: /language/basics
  - title: Keep your source
    details: Immutable lossless syntax and atomic edits preserve comments, whitespace, Unicode and your undo history.
    link: /guide/editing
  - title: Geometry with constraints
    details: Five-tier ports, stable orthogonal routes and label placement share one validated scenegraph.
    link: /language/layout
  - title: Rust and the browser
    details: Embed the compiler, use the CLI, or load the same engine through WebAssembly.
    link: /guide/wasm
  - title: Explore behavior
    details: Simulate queues, reconcile observed calls, check finite-state invariants and compare branches.
    link: /runtime/simulation
  - title: Ship the result
    details: Export accessible SVG, PNG or PDF. Work through editor previews and revision-checked MCP tools.
    link: /guide/cli
---

## Your first diagram

```diagram
client -> gateway: HTTPS
gateway -> postgres: SQL
```

```rust
let diagram = smarter_diagrams::compile("client -> gateway: HTTPS\n")?;
std::fs::write("architecture.svg", diagram.to_svg())?;
```

The library is available from [GitHub](https://github.com/robot-head/smarter-diagrams); version 0.1 is not published to crates.io. See [installation](/guide/getting-started), [runnable examples](/examples), and the [complete Rust API](https://robot-head.github.io/smarter-diagrams/api/smarter_diagrams/index.html).

## A small language with room to grow

Start with topology. Declare exact record ports when they matter, use zones for shared boundaries, and keep presentation changes as constraints. Canvas drags write those constraints back into the text.

The [architecture notes](/reference/architecture) explain the bounded algorithms and [performance measurements](/reference/performance) distinguish initial compilation from warm local updates. External telemetry, protocol semantics and visual-critic transport come from the embedding application.
