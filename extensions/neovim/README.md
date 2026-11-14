# Neovim adapter

Live preview (`:DiagramPreview` and `:DiagramPreviewInline`), LSP integration (`smarter-diagrams lsp`), and diagnostics for `.diagram` buffers.

```lua
vim.opt.runtimepath:append('/path/to/smarter-diagrams/extensions/neovim')
require('smarter_diagrams').setup({
  binary = '/path/to/smarter-diagrams', -- omit if on PATH
})
```

- **`:DiagramPreviewInline [theme]`**: Renders PNG bytes via `smarter-diagrams - --format png` into an inline side split using Neovim 0.13+'s `vim.ui.img` (`set`/`del`), automatically updating as you edit. On Neovim 0.11/0.12 (where `vim.ui.img` is absent), it falls back cleanly to `:DiagramPreview`.
- **`:DiagramPreviewStop`**: Clears active `vim.ui.img` handles and closes the inline preview window.
- **`:DiagramPreview`**: Starts the local HTTP + WebSocket preview server (`scripts/preview-server.cjs`) and opens a browser tab.
- **LSP**: Automatically attaches `smarter-diagrams lsp` on `.diagram` files for diagnostics, hover, go-to-definition, references, rename, and formatting (pass `lsp = false` to `setup()` to disable).
