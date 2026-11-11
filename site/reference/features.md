# Feature flags

The default feature set is empty. The compiler, SVG renderer, source editing and runtime data-model modules are part of the Rust library without optional native raster/code parser dependencies.

| Feature | Enables |
| --- | --- |
| `export` | `resvg` PNG rendering and `svg2pdf` PDF conversion |
| `code-bindings` | Tree-sitter and its Rust grammar for static symbol extraction |
| `verification` | Continuous document verification policies and results |
| `wasm` | wasm-bindgen exports for authoring, rendering and documents |
| `wasm-runtime` | `wasm` + `verification`, and extra browser simulation/safety/spatial exports |

```toml
[dependencies]
smarter-diagrams = {
  git = "https://github.com/robot-head/smarter-diagrams",
  branch = "main",
  features = ["export", "code-bindings", "verification"]
}
```

Use the `scripts/build-wasm.sh` presets for browser builds. They intentionally exclude native font/raster/PDF and Tree-sitter dependencies. Do not use `--all-features` as a browser bundle-size preset; all-feature native builds are useful for API documentation and validation.

The `formal` finite-state API exists in the core Rust library. The `verification` flag adds continuous checking to `Document`. Similarly, the fluid simulation Rust API is available independently of the separate browser worker/runtime package.

The crate version is `0.1.0`, MIT licensed and source-installable from GitHub. Neither crates.io publication nor editor Marketplace publication is part of the current distribution.
