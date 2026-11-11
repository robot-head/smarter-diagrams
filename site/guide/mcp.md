# MCP tools

`smarter-diagrams mcp` starts a stdio JSON-RPC server. Each process maintains one open `Document`. Read/write one JSON object per line; stdout is reserved for protocol responses.

## Configure a client

A typical MCP client configuration is:

```json
{
  "mcpServers": {
    "smarter-diagrams": {
      "command": "smarter-diagrams",
      "args": ["mcp"]
    }
  }
}
```

Use the absolute executable path if the client's PATH differs from your terminal's. The server advertises protocol version `2024-11-05`, tools capability, and its crate version during `initialize`.

## Tool reference

| Tool | Arguments | Result |
| --- | --- | --- |
| `diagram_open` | `source: string` | Revision and SVG; replaces the current document |
| `diagram_render` | Empty object | Current revision, SVG and layout |
| `diagram_edit` | `revision: integer`, `operations: array` | New revision, source and SVG |
| `diagram_lint` | Empty object | Aesthetic findings |

`tools/list` returns schemas covering all seven edit variants, pin variants and port constraints. Use those schemas to constrain agent output; do not generate arbitrary source patches for a typed operation.

## Example exchange

Send these lines in order:

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"diagram_open","arguments":{"source":"a\nb\n"}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"diagram_edit","arguments":{"revision":0,"operations":[{"PinNode":{"node":"b","pin":{"AbsoluteGrid":{"x":15,"y":0}},"size_lock":null}}]}}}
```

Successful `tools/call` responses contain MCP text content whose `text` field is a JSON-encoded result object. Parse that object to obtain the new revision. A stale revision or invalid edit returns an error; retain the current source and refresh before retrying.

## Aesthetic criticism

`lint::lint` returns structured findings with codes, severity, messages, optional entity IDs and source spans. Use a finding to choose a typed edit; findings themselves do not execute edits. The `VisualCritic` trait supports an embedding application's external visual-model transport. `lint::with_critic` combines those findings with heuristic checks. The library does not call an external model or send source over the network by itself.
