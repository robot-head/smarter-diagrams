# Language Server (LSP)

Smarter Diagrams ships a built-in Language Server Protocol (LSP) server in the main binary (`smarter-diagrams lsp`) implementing JSON-RPC 2.0 over `Content-Length`-framed stdio.

## Starting the server

```sh
smarter-diagrams lsp
```

No external runtime, Node process, or daemon is required. Every `.diagram` file edit reuses the incremental Rowan syntax parser, semantic lowering pass, layout engine, and static diagram linter.

## Capabilities

| LSP feature | Behavior |
| --- | --- |
| **Diagnostics** (`publishDiagnostics`) | Reports syntax errors, unknown node/port/zone references, infeasible layout constraints, and [`lint`](/reference/architecture) warnings (such as external label callouts or crossed routes) on open and edit. |
| **Go to definition** (`textDocument/definition`) | Jumps from node references (`vpc.api`, `zone [...]`, `align [...]`, `view ... { include ... }`, `@pin(x: a.E + 4g)`) and `#port` anchors to their primary declaration (or first implicit edge introduction). |
| **Find references** (`textDocument/references`) | Finds all occurrences of a node, scoped container segment, port, zone, flow, or view/step/scenario across the document, honoring `includeDeclaration`. |
| **Hover** (`textDocument/hover`) | Displays node shape, label, container hierarchy, hyperlink URL, declared ports, incoming/outgoing connections, and computed layout center/dimensions in pixels. |
| **Rename** (`prepareRename`, `textDocument/rename`) | Validates identifier syntax cleanly and renames the exact segment across declarations, qualified paths, edge anchors, ports, zones, flows, views, and steps. |
| **Document formatting** (`textDocument/formatting`) | Normalizes 2-space brace indentation, strips trailing horizontal spaces, and collapses extra blank lines while leaving multi-line `"""..."""` strings and fenced code blocks verbatim. |

## Editor configuration

- **Neovim (`extensions/neovim`)**: Calling `require('smarter_diagrams').setup({ binary = 'smarter-diagrams' })` registers the `.diagram` filetype and attaches `smarter-diagrams lsp` automatically (pass `lsp = false` to opt out). See [Editor previews](/guide/editors).
- **Generic LSP clients**: Configure `cmd = { "smarter-diagrams", "lsp" }` for filetype `diagram` / extension `.diagram`.
