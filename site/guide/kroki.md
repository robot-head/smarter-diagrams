# Kroki Companion Service

[Kroki](https://kroki.io/) (`yuzutech/kroki`) provides a unified HTTP API across diagram engines for Confluence, MkDocs, Sphinx, AsciiDoc, and Notion-style wikis. `extensions/kroki` provides a drop-in companion service speaking Kroki's exact REST, JSON, and compressed GET protocol on top of `smarter-diagrams`.

## Endpoints

| Endpoint | Method | Payload | Output |
| --- | --- | --- | --- |
| `/health` | `GET` | — | `{"status":"ok","engine":"smarter-diagrams"}` |
| `/smarterdiagrams/:format` | `POST` | Raw `.diagram` source (`text/plain`) | `svg`, `png`, `pdf`, `gif`, `apng` |
| `/` | `POST` | JSON `{ diagram_source, diagram_type, output_format, diagram_options }` | `svg`, `png`, `pdf`, `gif`, `apng` |
| `/smarterdiagrams/:format/:encoded` | `GET` | Base64url zlib/deflate payload | `svg`, `png`, `pdf`, `gif`, `apng` |

Optional diagram modifiers (`theme`, `view`, `step`) are accepted via `Kroki-Diagram-Options-Theme`, `Kroki-Diagram-Options-View`, and `Kroki-Diagram-Options-Step` HTTP headers, query parameters (`?theme=catppuccin-mocha&step=s2`), or JSON `diagram_options`.

## Running locally or with Docker

```sh
# Build CLI with PNG/PDF/GIF/APNG export support
cargo build --release --locked --features export

# Start companion HTTP gateway (default :8008)
node extensions/kroki/server.cjs
```

```sh
# Render SVG via POST
curl -X POST http://127.0.0.1:8008/smarterdiagrams/svg \
  -H 'Content-Type: text/plain' \
  --data-binary 'client -> api: HTTPS'
```

Docker and Docker Compose definitions live in `extensions/kroki/Dockerfile` and `extensions/kroki/docker-compose.yml`, and `extensions/kroki/UPSTREAM_PR.md` documents the `yuzutech/kroki` upstream Vert.x `DiagramService` integration.
