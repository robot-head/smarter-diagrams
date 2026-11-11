# Performance measurements

Measured on 2026-11-11 in shared x86-64 Linux (AMD EPYC 9V74), Rust 1.99.1, Node 24.22.1 and Chromium 156.0.8078.79. These are observed fixture results, not universal latency guarantees. Compilation uses locked dependencies; the Wasm profile uses optimization level 3, LTO, one codegen unit and aborting panics.

## Native compiler

`cargo bench --bench pipeline` measures one initial compile with no prior geometry and the median of nine incremental operations. Initial timings exclude process startup. The chain update adds a comment and reuses semantic geometry. Reparse timings replace one identifier in the lossless syntax tree.

| Case | Nodes | Initial compile (ms) | Incremental median (ms) | CST reparse median (ms) |
| --- | ---: | ---: | ---: | ---: |
| chain | 10 | 0.221 | 0.030 | 0.001 |
| chain | 30 | 0.482 | 0.102 | 0.002 |
| chain | 100 | 2.435 | 0.308 | 0.014 |
| chain | 500 | 67.437 | 1.797 | 0.061 |
| container-local | 550 | 70.472 | 2.821 | n/a |
| document-local | 550 | 69.984 | 2.085 | n/a |

The 550-node fixture contains a compound container with nine leaf nodes and an unrelated 540-node chain. Changing `Short` to `A longer service` expands a child while keeping the reserved container bounds feasible. `container-local` passes the full new text to `compile_with`; `document-local` applies a versioned source patch to `Document`, reusing both the lowered topology and independent layout query. The benchmark asserts that no more than ten nodes are solved and unrelated world coordinates remain unchanged. Undo occurs outside the timed document patch.

Raw native measurements: [benchmarks.csv](benchmarks.csv).

## Browser Wasm

Build both bundles, serve the repository on port 8765, install the locked browser tooling with `npm ci`, then run `node scripts/browser-benchmark.cjs`. The browser calls `WasmDocument.replace_source` on the same 550-node fixture. Thirty warmup edits permit Wasm tiering; the reported median uses nine subsequent edits. Undo occurs outside the timer. Wasm download, module instantiation and SVG serialization are excluded.

The initial 550-node compile took **287.2 ms**; the warm incremental median was **2.6 ms**, with samples from 2.4 to 3.3 ms. Raw observations are in [browser-benchmark.json](browser-benchmark.json).

## Bundle budget

`scripts/build-wasm.sh core` and `scripts/build-wasm.sh runtime` produce separate wasm-bindgen bundles. `node scripts/measure-wasm.cjs` compresses the generated Wasm with Node's Brotli quality 11 and checks the core against a 450,000-byte limit. JavaScript glue is not included in the Wasm limit.

| Bundle | Raw bytes | Brotli bytes | Brotli KiB |
| --- | ---: | ---: | ---: |
| core | 1,222,155 | 299,037 | 292.0 |
| runtime | 1,418,294 | 334,876 | 327.0 |

The runtime extension is loaded on demand for simulation and advanced tools. Native raster/PDF export and Tree-sitter dependencies are optional and absent from the browser core. Raw measurements: [wasm-size.json](wasm-size.json).

## Reading the targets

The measured warm 550-node document edit meets the blueprint's 4 ms local-update target on this machine. Smaller fresh native layouts fit within 16 ms, and syntax reparsing is below 1 ms in these fixtures. Large initial layouts take substantially longer than a frame; initial browser compilation is also above 16 ms. There is no claim that every graph, font, device, edit or simulation fits these budgets.

Local reuse requires unchanged container bounds and independence from crossing-boundary edges/constraints. Container expansion, ownership changes, mixed-scope relations and other structural changes can trigger a global solve. The plain leaf-label lowering fast path rejects lexical/structural changes and checks against full-lowering regressions. Cyclic placement and label optimization retain feasible incumbents at explicit search budgets. General quadratic projection uses dense active linear systems, so these timings do not imply asymptotically optimal VPSC or exact global layout optimality after bounded search.
