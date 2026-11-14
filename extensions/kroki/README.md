# Kroki Companion Service for Smarter Diagrams

[Kroki](https://kroki.io/) (`yuzutech/kroki`) is the standard HTTP gateway unifying text-to-diagram tools across wikis, Sphinx, AsciiDoc, MkDocs, and Markdown processors. Because Kroki accepts pluggable companion containers for specialized compilers (just like Mermaid, BPMN, and Excalidraw), `extensions/kroki/server.cjs` implements Kroki's standard REST, JSON, and zlib/deflate GET endpoints directly on top of the `smarter-diagrams` CLI.

## Endpoints

| Method | Path | Body / Encoding | Formats |
| --- | --- | --- | --- |
| `GET` | `/health` | — | JSON `{"status":"ok","engine":"smarter-diagrams"}` |
| `POST` | `/smarterdiagrams/:format` (or `/:format`) | Raw `.diagram` source | `svg`, `png`, `pdf`, `gif`, `apng` |
| `POST` | `/` | JSON `{ diagram_source, diagram_type, output_format, diagram_options }` | `svg`, `png`, `pdf`, `gif`, `apng` |
| `GET` | `/smarterdiagrams/:format/:encoded` (or `/:format/:encoded`) | Base64url (`deflateRaw` / `deflate`) | `svg`, `png`, `pdf`, `gif`, `apng` |

Options `theme`, `view`, and `step` can be sent via:
- `Kroki-Diagram-Options-Theme`, `Kroki-Diagram-Options-View`, `Kroki-Diagram-Options-Step` HTTP headers
- Query parameters (`?theme=catppuccin-mocha&step=s2`)
- JSON `diagram_options` (`{ "theme": "solarized-dark", "view": "overview" }`)

## Local usage

```sh
node extensions/kroki/server.cjs
# Render SVG via POST
curl -X POST http://127.0.0.1:8008/smarterdiagrams/svg \
  -H 'Content-Type: text/plain' \
  --data-binary 'client -> api: HTTPS'
```

See [UPSTREAM_PR.md](./UPSTREAM_PR.md) for the `yuzutech/kroki` upstream integration specification.
