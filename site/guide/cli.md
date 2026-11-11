# CLI and export

```sh
smarter-diagrams [INPUT|-] [-o OUTPUT] [--view NAME] [--format FORMAT]
smarter-diagrams edit --operations JSON
smarter-diagrams mcp
```

Omit `INPUT` or use `-` to read stdin. Output defaults to stdout. Unknown flags, malformed source and infeasible geometry produce an error on stderr and a nonzero exit status.

## Formats

| Format | Output | Feature |
| --- | --- | --- |
| `svg` (default) | Escaped, accessible SVG with source entity attributes | Core |
| `json` | Pretty-printed layout geometry | Core |
| `check` | Node/edge counts and geometry success | Core |
| `lint` | Structured aesthetic findings | Core |
| `png` | Raster image at CLI scale 1 | `export` |
| `pdf` | Vector PDF | `export` |

```sh
smarter-diagrams examples/architecture.diagram -o architecture.svg
smarter-diagrams examples/architecture.diagram --view request_only --format check
smarter-diagrams examples/architecture.diagram --format json -o layout.json
smarter-diagrams examples/architecture.diagram --format lint
smarter-diagrams examples/architecture.diagram --format pdf -o architecture.pdf
```

`--view` projects the canonical graph before laying out the selected entities. It is separate from a named `@layout` variant, which is selected through the Rust `LayoutOptions` API.

## Source edits from a process

```sh
printf 'a\nb\n' | smarter-diagrams edit --operations \
  '[{"PinNode":{"node":"b","pin":{"AbsoluteGrid":{"x":15,"y":0}},"size_lock":null}}]'
```

The command reads source from stdin, applies a typed operation array, validates the resulting diagram, and writes the new source. The operation's spelling and enum casing follow Serde's Rust representation. See the [MCP guide](/guide/mcp) for the same structured request over JSON-RPC.

## Native rendering API

With `export`, call `export::png(&svg, scale)` or `export::pdf(&svg)`. PNG scale must be finite and in `(0, 8]`; output is capped at 100 megapixels. SVG remains useful when a large diagram would exceed the raster cap.

Native text export loads installed fonts and chooses an available sans-serif family. Install a TrueType/OpenType font if the machine has none. Exact glyph metrics can differ between machines; the layout uses conservative text-width estimates. Keep a consistent font environment for reproducible raster/PDF output.

Browser SVG colors use CSS custom properties such as `--sd-fg`, `--sd-edge`, `--sd-bg`, and `--sd-database`, with light-mode color fallbacks. Inline SVG inherits these properties from its host; an SVG used as an image needs the palette embedded in the file. Native PNG/PDF exports resolve the emitted fallbacks so browser theming does not remove fills or text.
