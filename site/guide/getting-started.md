# Getting started

Smarter Diagrams compiles UTF-8 text into a scoped scenegraph, validated geometry and SVG. You can use it as a Rust library, a command-line program, or a browser Wasm module.

## Try it without installing

Open the [playground](/playground). Change the source, drag a node, then press **Undo**. A drag writes a `@pin` constraint into the text. The compiler runs locally in your browser; the demo does not upload diagram source.

## Install the CLI

Use a current stable Rust toolchain. The recorded validation used Rust 1.99.1; the code uses edition 2024 and APIs such as `slice::as_chunks` available in Rust 1.88+.

```sh
git clone https://github.com/robot-head/smarter-diagrams.git
cd smarter-diagrams
cargo install --path . --locked --features export
```

The `export` feature adds PNG/PDF support. Omit it for a smaller SVG-only native installation. Native raster/PDF text rendering requires an installed TrueType/OpenType font, such as DejaVu Sans.

Save this as `hello.diagram`:

```diagram
client: Web client
api: API service
db: Postgres
client -> api: HTTPS
api -> db: SQL
```

```sh
smarter-diagrams hello.diagram -o hello.svg
smarter-diagrams hello.diagram --format check
smarter-diagrams hello.diagram --format png -o hello.png
```

Open `hello.svg` in a browser. `check` exits unsuccessfully for invalid text or infeasible geometry.

## Add the Rust dependency

```toml
[dependencies]
smarter-diagrams = { git = "https://github.com/robot-head/smarter-diagrams", branch = "main" }
```

For a reproducible application release, pin a reviewed Git revision and commit your application's `Cargo.lock`.

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let diagram = smarter_diagrams::compile("client -> api: HTTPS\n")?;
    std::fs::write("hello.svg", diagram.to_svg())?;
    Ok(())
}
```

## What to read next

- [Core concepts](/guide/concepts): syntax, ownership, relations and geometry.
- [Language basics](/language/basics): nodes, chains, labels and containers.
- [Structured editing](/guide/editing): revisions, surgical patches and undo/redo.
- [Examples](/examples): complete source files and generated output.
