# WebAssembly

The browser uses the same Rust compiler and structured editing APIs. It needs a static HTTP(S) server that serves `.wasm` as `application/wasm`; opening the HTML through `file://` is unsupported.

## Build both bundles

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked
scripts/build-wasm.sh core
scripts/build-wasm.sh runtime
python3 -m http.server 8765 --bind 127.0.0.1
```

Open `http://127.0.0.1:8765/web/`. The checked-in Cargo lockfile and wasm-bindgen CLI versions must match. Generated packages are build artifacts under `web/pkg` and `web/pkg-runtime`.

## Initialize and edit

```js
import init, { WasmDocument, render_svg } from './pkg/smarter_diagrams.js';
await init();

const document = new WasmDocument('client -> api: HTTPS\n');
document.replace_source(document.revision(), 'client -> api: JSON\n');
document.edit(document.revision(), JSON.stringify([
  { PinNode: { node: 'api', pin: { AbsoluteGrid: { x: 15, y: 0 } }, size_lock: null } }
]));
console.log(document.source(), document.svg());
document.undo();
document.free();
```

`revision()` is a JavaScript **BigInt**, because the Rust revision is `u64`. Pass it directly back to `edit` and `replace_source`; convert it to a string before JSON serialization. Call `free()` when a document is no longer needed.

## Core exports

| Export | Result |
| --- | --- |
| `render_svg(source)` | SVG string |
| `layout_json(source)` | JSON string containing geometry |
| `edit_source(source, operations_json)` | New source after a typed operation batch |
| `WasmDocument` | Stateful source, revision, SVG, edits and undo/redo |

`WasmDocument` also exposes `drag(drags_json)`, `focus_svg(node, hops)`, `flows_json()` and `flow_frame_svg(id, step)`. For continuous dragging, call `const preview = document.begin_drag()`, `preview.update(drags_json)` and display `preview.svg()`. Commit with `document.commit_drag(preview)`, then `preview.free()`. Failed updates retain the last valid preview and prevent commit until a valid update succeeds. Dropping commits the displayed layout; it does not rerun layout or snap coordinates. `preview.settle()` validates without changing the image. `drag(drags_json)` remains the explicit semantic snapping helper. Drag coordinates are pixels; flow steps are zero-based. JSON payloads use the Rust types' Serde representation. Errors cross the boundary as JavaScript exceptions.

## Lazy runtime exports

The separate runtime bundle adds `WasmSimulation`, `check_safety(model_json, invariant_json, horizon)`, `stack_json(source, theta)` and `branch_diff_svg(base, proposal, tau)`. It also enables document verification methods `verify(zone, system_json, horizon)` and `verification_results()`.

Import and initialize the runtime package only when needed. The demo's module worker ticks simulation at 60 Hz, keeping it off the authoring thread. Use [simulation](/runtime/simulation), [verification](/runtime/verification), and [spatial tools](/runtime/spatial) for data models and limits.

## Hosting

All assets use paths relative to their HTML or JavaScript modules, including worker imports. The GitHub Pages build publishes both bundles under this project's subdirectory. No server endpoint, API key or telemetry upload is needed for the playground. Browsers without a usable WebGPU device use SVG for the stratum geometry layer.
