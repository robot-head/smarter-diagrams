# Export and Third-Party Licensing Decision

## Summary

`smarter-diagrams` is licensed under **MIT** and distributes cleanly across every target — embedded Rust library, browser WebAssembly bundles, IDE/SSG plugins, native CLI binaries, and Kroki containers — with **zero copyleft or weak-copyleft (`GPL`, `LGPL`, `AGPL`, `MPL-2.0`) dependencies**.

## `resvg` & Rasterizer Audit

Historical releases of `resvg` (`< 0.45`) and `usvg` were distributed under `MPL-2.0`, which raises disclosure/file-boundary review requirements when statically linked into proprietary SDKs or monolithic Wasm binaries. We audited our actual locked Cargo dependency graph (`cargo metadata --format-version 1 --locked --all-features`):

| Rasterizer / Export Crate | Locked Version | Resolved SPDX License | Feature Gate | Compatibility |
| --- | --- | --- | --- | --- |
| `resvg` | `0.45.1` | `Apache-2.0 OR MIT` | `export` | Compatible with MIT, commercial, static, Wasm, and plugin distribution |
| `usvg` | `0.45.1` | `Apache-2.0 OR MIT` | `export` | Compatible |
| `svg2pdf` | `0.13.0` | `MIT OR Apache-2.0` | `export` | Compatible |
| `tiny-skia` / `tiny-skia-path` | `0.11.4` | `BSD-3-Clause` | `export` | Compatible |
| `fontdb` | `0.23.0` | `MIT` | `export` | Compatible |
| `export::gif` & `export::apng` | built-in (`src/export.rs`) | `MIT` (project) | `export` | Pure-Rust in-repo encoders (no GPL/LGPL C libs) |

Because RazrFalcon relicensed `resvg` and `usvg` to **`Apache-2.0 OR MIT`** in `0.45.x`, and `svg2pdf` (`MIT OR Apache-2.0`), `tiny-skia` (`BSD-3-Clause`), and our in-tree GIF89a/APNG encoders are all strictly permissive, native PNG, PDF, GIF, and APNG exports impose no copyleft obligations.

## Feature Isolation Design

Even though `resvg@0.45.1` is permissively licensed, `Cargo.toml` continues to isolate the rasterizer and PDF stack behind an opt-in Cargo feature (`default = []`, `export = ["dep:resvg", "dep:svg2pdf"]`):

| Distribution Artifact | Cargo Feature Profile | Third-Party Crates | Includes `resvg` / `svg2pdf`? | Permissive Only? |
| --- | --- | --- | --- | --- |
| Core Rust library (SVG/JSON/layout/views/statecharts) | `--no-default-features` | 29 | No | Yes |
| Browser Wasm (`web/pkg`, `<smarter-diagram>`) | `--no-default-features --features wasm` | 34 | No | Yes |
| Browser Verification Wasm (`web/pkg-runtime`) | `--no-default-features --features wasm,wasm-runtime` | 34 | No | Yes |
| Native CLI / LSP / MCP / Hugo prerender / Kroki runner | `--features export` / `--all-features` | 104 / 128 | Yes (`Apache-2.0 OR MIT`, `MIT OR Apache-2.0`) | Yes |
| Editor & SSG integrations (VS Code, Neovim, Hugo, Kroki) | JS / Lua / Hugo templates (no bundled copyleft binaries) | 0 native | Invokes user/container CLI | Yes |

Keeping `export` optional keeps browser Wasm bundles small (`web/pkg` avoids pulling 70+ font-shaping/pixel-rasterization crates) and guarantees that browser embeds render SVG natively via the browser compositor (plus HTML Canvas / WebM capture) rather than bundling software font/raster engines.

## Automated CI Enforcement

`npm run test:licenses` ([`scripts/license-check.cjs`](../scripts/license-check.cjs)) runs in CI and fails the build if:

1. Any resolved crate across `--all-features` introduces `GPL`, `LGPL`, `AGPL`, `MPL`, `CDDL`, `EPL`, `EUPL`, or `CC-BY-SA`, or any unreviewed SPDX token.
2. A lockfile update downgrades `resvg` or `usvg` back to an `MPL-2.0` release.
3. The `wasm` / `wasm-runtime` feature graph pulls `resvg`, `usvg`, `svg2pdf`, `tiny-skia`, `fontdb`, or `pdf-writer`.
4. Any resolved crate is missing from [THIRD_PARTY_LICENSES.md](../THIRD_PARTY_LICENSES.md).

See the complete audited inventory in [THIRD_PARTY_LICENSES.md](../THIRD_PARTY_LICENSES.md).
