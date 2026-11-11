# Nodes and connections

The language is progressive: topology first, then labels, ports and constraints as needed. Source is UTF-8; comments, whitespace and CRLF are retained in the syntax tree.

Each source example below is followed by its actual compiler output. Open an SVG to inspect a larger version.

## Nodes and labels

```diagram
client: Web client
api: Public API
postgres: Ledger
client -> api: HTTPS
api -> postgres: SQL
```

The stable IDs are `client`, `api` and `postgres`. Text after `:` is the display label. Quoted labels use JSON string escaping. Edges infer undeclared endpoint nodes, so `client -> api` is already a complete diagram.

IDs may be qualified with dots. Within a container, lexical lookup tries the current scope and its ancestors. Use a qualified path to make a cross-scope reference explicit. Bare cardinal IDs such as `N` are nodes; `.N` on an endpoint selects a port side.

## Edge operators and chains

| Operator | Meaning |
| --- | --- |
| `->` | Directed connection |
| `<->` | Bidirectional connection |
| `--` | Undirected connection |
| `==>` | Bold directed connection |

```diagram
client -> gateway -> worker: request
worker <-> cache: read through
worker ==> db: commit
cache -- observer
```

A chain creates one edge per adjacent endpoint pair. A chain's trailing label/attributes apply to its generated edges. Parallel connections receive distinct canonical edge IDs. Read those IDs from `SceneGraph::edges` before issuing an edge edit; avoid constructing IDs from display labels.

## Containers

```diagram
prod: Production {
  api: API
  db: Database
  api -> db
}
client -> prod.api
```

The children are `prod.api` and `prod.db`. A block with owned nodes/edges is a container. A record block containing only `port` declarations defines ports on a node; it does not turn that node into an ownership container.

## Shapes and attributes

```diagram
client [shape: actor]
decision [shape: diamond]
cache [shape: cache]
events [shape: queue]
store [shape: database]
client -> decision
decision -> cache
decision -> events
events -> store
```

Supported silhouettes include `rect`, `actor`, `cache`, `queue`, `database`, `sql_table`, `diamond` and `decision`. A missing shape is inferred from the ID: database-like names become stores, broker/queue names become queues, and client/user names become actors. An unknown explicit shape falls back to the rectangle renderer.

Attributes use `[key: value, ...]`; annotations use `@name(key: value, ...)`. Values and source ranges are retained in the IR. Retaining an arbitrary key does not imply renderer or runtime support for it. The relevant guides list the keys consumed by each subsystem.

## Comments and statement boundaries

```diagram
# A full-line comment
api: Public API # A trailing comment
api -> db; db -> archive
```

Newlines and semicolons separate statements. `#` starts a comment at the beginning of the file or after whitespace. A directly attached `#name` selects a named port. Quoting a label is useful when it contains punctuation that otherwise has syntax meaning.
