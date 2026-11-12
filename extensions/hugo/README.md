# Hugo Fenced Diagram Blocks

Render fenced ```` ```diagram ```` and ```` ```smarter-diagrams ```` code blocks in [Hugo](https://gohugo.io/) (`>= 0.112.0`) using Hugo's native [code block render hooks](https://gohugo.io/render-hooks/code-blocks/).

Two rendering paths share the same Markdown syntax and `--sd-*` light/dark theme variables:

1. **Static build-time SVG (`mode = "static"` or `"auto"`, recommended)**: `scripts/prerender.mjs` compiles fenced blocks with `smarter-diagrams` into cached `assets/smarter-diagrams/<sha256>.svg` resources before `hugo` runs. Published pages inline accessible SVG markup directly and require **zero client-side JavaScript**.
2. **Browser WebAssembly (`mode = "wasm"` or `"auto"` fallback)**: When no pre-rendered SVG asset is present, the render hook emits a lightweight module loader once per page that compiles the diagram in the browser with `web/pkg/smarter_diagrams_bg.wasm`.

## Installation

### Option A: Mount as a local theme component

Copy or submodule `extensions/hugo` (or mount it directly from a monorepo) in `hugo.toml`:

```toml
[module]
  [[module.mounts]]
    source = "layouts"
    target = "layouts"
  [[module.mounts]]
    source = "extensions/hugo/layouts"
    target = "layouts"

[params.smarterDiagrams]
  mode = "auto"
  wasmModuleUrl = "/smarter-diagrams/pkg/smarter_diagrams.js"
  showSource = "false"
```

### Option B: Import as a Hugo Module

```toml
[module]
  [[module.imports]]
    path = "github.com/robot-head/smarter-diagrams/extensions/hugo"
```

## Authoring fenced blocks

Write standard Markdown code blocks tagged `diagram` or `smarter-diagrams`:

````markdown
```diagram {caption="Control-plane reconciliation" view="control_loop" showSource="details"}
kubectl: "kubectl / GitOps" [shape: person]
apiserver: "kube-apiserver"
reconciler: "Operator Reconciler"

kubectl -> apiserver: "apply CR"
apiserver -> reconciler: "watch event"
reconciler -> apiserver: "patch status"

view control_loop {
  include apiserver -> reconciler -> apiserver
}
```
````

### Supported fence attributes

| Attribute | Values | Description |
| --- | --- | --- |
| `view` | `<view_id>` | Project a declared `view <view_id> { ... }` (static pre-render mode). |
| `focus` | `<node_id>` | Dim nodes outside `hops` steps from `<node_id>` (`WasmDocument.focus_svg`). |
| `hops` | `1`, `2`, ... | Hop radius when `focus` is set (defaults to `1`). |
| `caption` | Markdown text | Renders a `<figcaption class="smarter-diagram-caption">` under the diagram. |
| `showSource` | `"false"`, `"details"`, `"true"` | Hide source, place it in a collapsible `<details>` block, or show it below the SVG. |
| `mode` | `"auto"`, `"static"`, `"wasm"` | Override `params.smarterDiagrams.mode` for one block (`"static"` fails the Hugo build if pre-render was skipped). |
| `id`, `class` | HTML identifiers | Forwarded to the outer `<figure class="smarter-diagram">`. |

## Static pre-render workflow

Install `smarter-diagrams` (`cargo install --path .`) and run `prerender.mjs` before `hugo`:

```sh
node extensions/hugo/scripts/prerender.mjs --site /path/to/hugo-site --bin smarter-diagrams
hugo --source /path/to/hugo-site --minify
```

Pass `--wasm-src web/pkg` to pre-render `focus="..."` blocks at build time, and `--copy-wasm` if you also want `static/smarter-diagrams/pkg/` populated for live `hugo server` drafting:

```sh
node extensions/hugo/scripts/prerender.mjs \
  --site /path/to/hugo-site \
  --bin smarter-diagrams \
  --wasm-src web/pkg \
  --copy-wasm
```
