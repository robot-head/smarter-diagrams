# Hugo fenced blocks

[Hugo](https://gohugo.io/) (`>= 0.112.0`) renders fenced `diagram` and `smarter-diagrams` code blocks through native [code block render hooks](https://gohugo.io/render-hooks/code-blocks/). The plugin lives in [`extensions/hugo`](https://github.com/robot-head/smarter-diagrams/tree/main/extensions/hugo) and supports both static build-time SVG inlining and live browser WebAssembly rendering.

## Two rendering modes

- **Static build-time SVG (`mode = "auto"` or `"static"`, recommended)**: `extensions/hugo/scripts/prerender.mjs` scans `content/**/*.md`, compiles fenced blocks with `smarter-diagrams`, and caches deterministic SVGs in `assets/smarter-diagrams/<sha256>.svg`. Hugo inlines those SVGs directly into the generated HTML with **zero client-side JavaScript**, following the same self-contained publishing model as Pikchr in Fossil.
- **Browser WebAssembly (`mode = "wasm"` or `"auto"` fallback)**: When no pre-rendered SVG asset exists for a block, the render hook emits one `<script type="module">` loader per page that loads `smarter_diagrams_bg.wasm` and renders every pending block in the browser.

Both modes emit the same `--sd-*` light and dark CSS custom properties once per page, adapting automatically to `prefers-color-scheme: dark` as well as `.dark`, `[data-theme="dark"]`, `[data-bs-theme="dark"]`, and `[data-color-mode="dark"]` theme switches.

## Install the plugin

### Mount from a local checkout or theme folder

Copy or submodule `extensions/hugo` into `themes/smarter-diagrams` (or mount `extensions/hugo/layouts` directly) in `hugo.toml`:

```toml
baseURL = "https://example.org/docs/"
locale = "en-us"
title = "Engineering Handbook"
theme = ["smarter-diagrams"]

[params.smarterDiagrams]
  mode = "auto"
  wasmModuleUrl = "/smarter-diagrams/pkg/smarter_diagrams.js"
  showSource = "false"
```

### Import as a Hugo Module

```toml
[module]
  [[module.imports]]
    path = "github.com/robot-head/smarter-diagrams/extensions/hugo"

[params.smarterDiagrams]
  mode = "static"
  showSource = "details"
```

If your site defines a custom `<head>` partial and you want the palette `<style>` emitted in `<head>` rather than before the first diagram in `<body>`, include <code v-pre>{{ partial "smarter-diagrams/head.html" . }}</code> inside `<head>`.

## Author fenced diagram blocks

Tag any fenced Markdown code block with `diagram` or `smarter-diagrams` and pass optional Goldmark attributes inside `{ ... }`:

```markdown
~~~diagram {view="control_loop" caption="Operator watch and status loop" showSource="details"}
kubectl: "kubectl / GitOps" [shape: person]
apiserver: "kube-apiserver"
reconciler: "Operator Reconciler"

kubectl -> apiserver: "apply CR"
apiserver -> reconciler: "watch event"
reconciler -> apiserver: "patch status"

view control_loop {
  include apiserver -> reconciler -> apiserver
}
~~~
```

The block above compiles to the same canonical graph and `control_loop` projection shown below:

```diagram views=control_loop
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

### Fence attributes

| Attribute | Example | Effect |
| --- | --- | --- |
| `view` | `view="control_loop"` | Projects `view control_loop { ... }` via `smarter-diagrams --view` during pre-rendering. |
| `focus` | `focus="checkout"` | Highlights nodes within `hops` steps of `checkout` and dims distant context (`0.35` opacity). |
| `hops` | `hops="2"` | Sets the hop radius for `focus` (defaults to `1`). |
| `caption` | `caption="Figure 1: Ka-band bearer"` | Renders a `<figcaption class="smarter-diagram-caption">` beneath the canvas. |
| `showSource` | `showSource="details"` | `"false"` hides source, `"details"` adds a collapsible `<details>` block, `"true"` shows source inline. |
| `mode` | `mode="static"` | Overrides `params.smarterDiagrams.mode` (`"auto"`, `"static"`, or `"wasm"`). |
| `id`, `class` | `id="fig-reconcile"` | Adds HTML `id` or custom classes to the outer `<figure class="smarter-diagram">`. |

## Static pre-rendering and CI

Install the CLI (`cargo install --path . --locked`) and run `prerender.mjs` before building with `hugo`:

```sh
node extensions/hugo/scripts/prerender.mjs --site . --bin smarter-diagrams
hugo --minify
```

To pre-render `focus="..."` lenses at build time or copy the WebAssembly bundle into `static/smarter-diagrams/pkg/` for interactive `hugo server` drafting, pass `--wasm-src`:

```sh
node extensions/hugo/scripts/prerender.mjs \
  --site . \
  --bin smarter-diagrams \
  --wasm-src /path/to/smarter-diagrams/web/pkg \
  --copy-wasm
```

Setting `mode = "static"` in `hugo.toml` makes Hugo fail the build immediately with the file and line position if any fenced diagram block was added without running `prerender.mjs`.
