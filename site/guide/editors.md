# Editor previews

Both adapters invoke the installed Rust CLI through a child process, using argument arrays rather than a shell. Install it first with `cargo install --path . --locked --features export`.

## VS Code

The source adapter lives in [`extensions/vscode`](https://github.com/robot-head/smarter-diagrams/tree/main/extensions/vscode). Start a development host from the repository root:

```sh
code --extensionDevelopmentPath="$PWD/extensions/vscode" examples/architecture.diagram
```

Set `smarterDiagrams.binary` if the compiler is not on PATH. The `.diagram` language activates the adapter.

- **Smarter Diagrams: Open Live Preview** displays an SVG webview and refreshes on text changes.
- **Smarter Diagrams: Apply Structured Edit** accepts a `DiagramEditOp` JSON array and applies the compiler result in one editor undo step.

The adapter refuses an edit result if the buffer version changed during compilation. Errors appear in the Problems panel. The preview disables scripts and has a restrictive content security policy. This adapter is source-installable and is not published to the Marketplace.

## Neovim

Neovim 0.11+, Node and the CLI are required. Keep the adapter inside the repository so it can locate `scripts/preview-server.cjs`.

```lua
vim.opt.runtimepath:append('/absolute/path/to/smarter-diagrams/extensions/neovim')
require('smarter_diagrams').setup({
  binary = '/absolute/path/to/smarter-diagrams',
})
```

Run `:DiagramPreview` in a `.diagram` buffer. A loopback-only server opens the browser and refreshes the SVG after buffer changes. Compiler errors become Neovim diagnostics. The server exits when Neovim closes its input stream.

Options `node` and `server` override transport paths. `on_open = function(url) ... end` overrides browser launch and supports headless integration tests. For scripted source edits, invoke `smarter-diagrams edit --operations JSON`; interactive canvas editing is provided by the [Wasm playground](/playground).

## Hugo and static site publishing

For Markdown documentation sites built with [Hugo](https://gohugo.io/), [`extensions/hugo`](https://github.com/robot-head/smarter-diagrams/tree/main/extensions/hugo) provides native `render-codeblock-diagram.html` and `render-codeblock-smarter-diagrams.html` hooks with build-time CLI pre-rendering and a browser Wasm fallback. See [Hugo fenced blocks](/guide/hugo).

## Validation status

The Neovim preview and Hugo fenced codeblock plugin were exercised headlessly against the real compiler. The VS Code adapter passed JavaScript syntax checks; a VS Code extension host was not available in the recorded environment. See [validation evidence](/reference/verification).
