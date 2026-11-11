# Neovim preview

Neovim 0.11+, Node, and the Rust CLI are required. Keep this directory inside the repository so the preview server can be found.

```lua
vim.opt.runtimepath:append('/absolute/path/to/smarter-diagrams/extensions/neovim')
require('smarter_diagrams').setup({
  binary = '/absolute/path/to/smarter-diagrams',
})
```

Open a `.diagram` buffer and run `:DiagramPreview`. A loopback HTTP server opens a browser preview and refreshes it as the buffer changes. Errors become Neovim diagnostics. The child server exits when Neovim closes its input stream.

For custom installations, `node` and `server` override executable/server paths. `on_open = function(url) ... end` overrides browser launch, which also supports headless integration checks. Structured source edits are available through `smarter-diagrams edit --operations JSON`; the browser Wasm demo supplies interactive canvas gestures.
