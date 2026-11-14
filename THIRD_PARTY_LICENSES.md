# Third-Party Licenses

`smarter-diagrams` itself is dual/single-licensed under **MIT** (`Cargo.toml`).  
All **128** locked third-party crates across `--all-features` use permissive open-source licenses (`0` GPL, LGPL, AGPL, or MPL crates). Verified with `cargo metadata --format-version 1 --locked --all-features` and `scripts/license-check.cjs`.

- **`core` / `wasm` / `wasm-runtime`**: `34` third-party crates (zero rasterizer, font discovery, or PDF dependencies).
- **`export` (`--features export`)**: `104` third-party crates including `resvg` (`0.45.1`, `Apache-2.0 OR MIT`), `usvg` (`0.45.1`, `Apache-2.0 OR MIT`), `svg2pdf` (`0.13.0`, `MIT OR Apache-2.0`), `tiny-skia` (`0.11.4`, `BSD-3-Clause`), and `fontdb` (`0.23.0`, `MIT`).
- **Pure-Rust encoders (`export::gif`, `export::apng`)**: implemented directly in `src/export.rs` using Rust stdlib only (`MIT`, matching the workspace).

| Crate | Version | SPDX License | Feature Profiles |
| --- | --- | --- | --- |
| `adler2` | `2.0.1` | `0BSD OR MIT OR Apache-2.0` | `export` |
| `aho-corasick` | `1.1.5` | `Unlicense OR MIT` | `code-bindings` |
| `arrayref` | `0.3.9` | `BSD-2-Clause` | `export` |
| `arrayvec` | `0.7.8` | `MIT OR Apache-2.0` | `export` |
| `autocfg` | `1.5.1` | `Apache-2.0 OR MIT` | `dev / bench` |
| `base64` | `0.22.1` | `MIT OR Apache-2.0` | `export` |
| `bitflags` | `1.3.2` | `MIT/Apache-2.0` | `export` |
| `bitflags` | `2.13.2` | `MIT OR Apache-2.0` | `export` |
| `bumpalo` | `3.20.3` | `MIT OR Apache-2.0` | `wasm` |
| `bytemuck` | `1.25.2` | `Zlib OR Apache-2.0 OR MIT` | `export` |
| `bytemuck_derive` | `1.12.1` | `Zlib OR Apache-2.0 OR MIT` | `export` |
| `byteorder` | `1.5.0` | `Unlicense OR MIT` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `byteorder-lite` | `0.1.0` | `Unlicense OR MIT` | `export` |
| `cc` | `1.11.1` | `MIT OR Apache-2.0` | `dev / bench` |
| `cfg-if` | `1.0.5` | `MIT OR Apache-2.0` | `wasm`, `export` |
| `color_quant` | `1.1.0` | `MIT` | `export` |
| `core_maths` | `0.1.1` | `MIT` | `export` |
| `countme` | `3.0.1` | `MIT OR Apache-2.0` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `crc32fast` | `1.5.5` | `MIT OR Apache-2.0` | `export` |
| `data-url` | `0.3.2` | `MIT OR Apache-2.0` | `export` |
| `delaunator` | `1.1.0` | `ISC` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `equivalent` | `1.0.3` | `Apache-2.0 OR MIT` | `export` |
| `euclid` | `0.22.14` | `MIT OR Apache-2.0` | `dev / bench` |
| `fdeflate` | `0.3.7` | `MIT OR Apache-2.0` | `export` |
| `find-msvc-tools` | `0.1.15` | `MIT OR Apache-2.0` | `dev / bench` |
| `flate2` | `1.1.11` | `MIT OR Apache-2.0` | `export` |
| `float-cmp` | `0.9.0` | `MIT` | `export` |
| `foldhash` | `0.2.0` | `Zlib` | `dev / bench` |
| `font-types` | `0.11.3` | `MIT OR Apache-2.0` | `export` |
| `fontconfig-parser` | `0.5.8` | `MIT` | `dev / bench` |
| `fontdb` | `0.23.0` | `MIT` | `export` |
| `gif` | `0.13.3` | `MIT OR Apache-2.0` | `export` |
| `gif` | `0.14.2` | `MIT OR Apache-2.0` | `export` |
| `hash32` | `0.3.1` | `MIT OR Apache-2.0` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `hashbrown` | `0.14.5` | `MIT OR Apache-2.0` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `hashbrown` | `0.17.1` | `MIT OR Apache-2.0` | `export` |
| `heapless` | `0.8.0` | `MIT OR Apache-2.0` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `image` | `0.25.10` | `MIT OR Apache-2.0` | `export` |
| `image-webp` | `0.2.4` | `MIT OR Apache-2.0` | `export` |
| `imagesize` | `0.13.0` | `MIT` | `export` |
| `indexmap` | `2.14.2` | `Apache-2.0 OR MIT` | `export` |
| `itoa` | `1.0.19` | `MIT OR Apache-2.0` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `kurbo` | `0.11.3` | `Apache-2.0 OR MIT` | `export` |
| `kurbo` | `0.13.1` | `Apache-2.0 OR MIT` | `export` |
| `libc` | `0.2.191` | `MIT OR Apache-2.0` | `dev / bench` |
| `libm` | `0.2.16` | `MIT` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `log` | `0.4.35` | `MIT OR Apache-2.0` | `export` |
| `memchr` | `2.8.3` | `Unlicense OR MIT` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `memmap2` | `0.9.12` | `MIT OR Apache-2.0` | `export` |
| `miniz_oxide` | `0.8.9` | `MIT OR Zlib OR Apache-2.0` | `export` |
| `miniz_oxide` | `0.9.1` | `MIT OR Zlib OR Apache-2.0` | `export` |
| `moxcms` | `0.8.1` | `BSD-3-Clause OR Apache-2.0` | `export` |
| `num-traits` | `0.2.19` | `MIT OR Apache-2.0` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `once_cell` | `1.21.4` | `MIT OR Apache-2.0` | `export` |
| `pdf-writer` | `0.12.1` | `MIT OR Apache-2.0` | `export` |
| `pico-args` | `0.5.0` | `MIT` | `export` |
| `png` | `0.17.16` | `MIT OR Apache-2.0` | `export` |
| `png` | `0.18.1` | `MIT OR Apache-2.0` | `export` |
| `polycool` | `0.4.0` | `MIT OR Apache-2.0` | `export` |
| `proc-macro2` | `1.0.107` | `MIT OR Apache-2.0` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `pxfm` | `0.1.30` | `BSD-3-Clause OR Apache-2.0` | `export` |
| `quick-error` | `2.0.1` | `MIT/Apache-2.0` | `export` |
| `quote` | `1.0.47` | `MIT OR Apache-2.0` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `read-fonts` | `0.39.2` | `MIT OR Apache-2.0` | `export` |
| `regex` | `1.13.1` | `MIT OR Apache-2.0` | `code-bindings` |
| `regex-automata` | `0.4.18` | `MIT OR Apache-2.0` | `code-bindings` |
| `regex-syntax` | `0.8.11` | `MIT OR Apache-2.0` | `code-bindings` |
| `resvg` | `0.45.1` | `Apache-2.0 OR MIT` | `export` |
| `rgb` | `0.8.53` | `MIT` | `export` |
| `robust` | `1.2.0` | `MIT OR Apache-2.0` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `rowan` | `0.16.1` | `MIT OR Apache-2.0` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `roxmltree` | `0.20.0` | `MIT OR Apache-2.0` | `export` |
| `rstar` | `0.12.2` | `MIT OR Apache-2.0` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `rustc-hash` | `1.1.0` | `Apache-2.0/MIT` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `rustc-hash` | `2.1.3` | `Apache-2.0 OR MIT` | `export` |
| `rustversion` | `1.0.23` | `MIT OR Apache-2.0` | `dev / bench` |
| `rustybuzz` | `0.20.1` | `MIT` | `export` |
| `ryu` | `1.0.23` | `Apache-2.0 OR BSL-1.0` | `export` |
| `serde` | `1.0.229` | `MIT OR Apache-2.0` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `serde_core` | `1.0.229` | `MIT OR Apache-2.0` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `serde_derive` | `1.0.229` | `MIT OR Apache-2.0` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `serde_json` | `1.0.154` | `MIT OR Apache-2.0` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `shlex` | `2.0.1` | `MIT OR Apache-2.0` | `dev / bench` |
| `simd-adler32` | `0.3.10` | `MIT` | `export` |
| `simplecss` | `0.2.2` | `Apache-2.0 OR MIT` | `export` |
| `siphasher` | `1.0.4` | `MIT OR Apache-2.0` | `export` |
| `skrifa` | `0.42.1` | `MIT OR Apache-2.0` | `export` |
| `slotmap` | `1.1.1` | `Zlib` | `export` |
| `smallvec` | `1.16.5` | `MIT OR Apache-2.0` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `stable_deref_trait` | `1.2.1` | `MIT OR Apache-2.0` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `streaming-iterator` | `0.1.9` | `MIT OR Apache-2.0` | `code-bindings` |
| `strict-num` | `0.1.1` | `MIT` | `export` |
| `subsetter` | `0.2.6` | `MIT OR Apache-2.0` | `export` |
| `svg2pdf` | `0.13.0` | `MIT OR Apache-2.0` | `export` |
| `svgtypes` | `0.15.3` | `Apache-2.0 OR MIT` | `export` |
| `syn` | `3.0.10` | `MIT OR Apache-2.0` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `text-size` | `1.1.1` | `MIT OR Apache-2.0` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `thiserror` | `2.0.21` | `MIT OR Apache-2.0` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `thiserror-impl` | `2.0.21` | `MIT OR Apache-2.0` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `tiny-skia` | `0.11.4` | `BSD-3-Clause` | `export` |
| `tiny-skia-path` | `0.11.4` | `BSD-3-Clause` | `export` |
| `tinyvec` | `1.13.3` | `Zlib OR Apache-2.0 OR MIT` | `export` |
| `tree-sitter` | `0.25.10` | `MIT` | `code-bindings` |
| `tree-sitter-language` | `0.1.9` | `MIT` | `code-bindings` |
| `tree-sitter-rust` | `0.24.2` | `MIT` | `code-bindings` |
| `ttf-parser` | `0.25.1` | `MIT OR Apache-2.0` | `export` |
| `unicode-bidi` | `0.3.18` | `MIT OR Apache-2.0` | `export` |
| `unicode-bidi-mirroring` | `0.4.0` | `MIT/Apache-2.0` | `export` |
| `unicode-ccc` | `0.4.0` | `MIT/Apache-2.0` | `export` |
| `unicode-ident` | `1.0.27` | `(MIT OR Apache-2.0) AND Unicode-3.0` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `unicode-properties` | `0.1.5` | `(MIT OR Apache-2.0) AND Unicode-3.0` | `export` |
| `unicode-script` | `0.5.9` | `MIT OR Apache-2.0` | `export` |
| `unicode-vo` | `0.1.0` | `MIT/Apache-2.0` | `export` |
| `usvg` | `0.45.1` | `Apache-2.0 OR MIT` | `export` |
| `version_check` | `0.9.5` | `MIT/Apache-2.0` | `dev / bench` |
| `wasm-bindgen` | `0.2.130` | `MIT OR Apache-2.0` | `wasm` |
| `wasm-bindgen-macro` | `0.2.130` | `MIT OR Apache-2.0` | `wasm` |
| `wasm-bindgen-macro-support` | `0.2.130` | `MIT OR Apache-2.0` | `wasm` |
| `wasm-bindgen-shared` | `0.2.130` | `MIT OR Apache-2.0` | `wasm` |
| `weezl` | `0.1.12` | `MIT OR Apache-2.0` | `export` |
| `write-fonts` | `0.48.1` | `MIT OR Apache-2.0` | `export` |
| `xmlwriter` | `0.1.0` | `MIT` | `export` |
| `zlib-rs` | `0.6.8` | `Zlib` | `dev / bench` |
| `zmij` | `1.0.23` | `MIT` | `core`, `wasm`, `export`, `verification`, `code-bindings` |
| `zune-core` | `0.4.12` | `MIT OR Apache-2.0 OR Zlib` | `export` |
| `zune-core` | `0.5.3` | `MIT OR Apache-2.0 OR Zlib` | `export` |
| `zune-jpeg` | `0.4.21` | `MIT OR Apache-2.0 OR Zlib` | `export` |
| `zune-jpeg` | `0.5.15` | `MIT OR Apache-2.0 OR Zlib` | `export` |
