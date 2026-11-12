# Grammar reference

This is a compact reading guide to the supported notation. The lossless Rowan parser and scoped lowering pass are the executable definition. Source diagnostics and semantic validation remain separate from this grammar sketch.

```text
document      = { statement | trivia } ;
statement     = node | edge | port | zone | relation | flow
              | view | layout | strata | story ;
node          = qualified_id, [ ":", label ], [ attributes ],
                { annotation }, [ "{", document, "}" ] ;
edge          = endpoint, operator, endpoint,
                { operator, endpoint }, [ ":", label ],
                [ attributes ], { annotation } ;
endpoint      = qualified_id, [ ".", side ],
                [ "@", ratio | "(", port_options, ")" ],
                [ "#", identifier ] ;
operator      = "->" | "<->" | "--" | "==>" ;
side          = "N" | "E" | "S" | "W" ;
attributes    = "[", key_values, "]" ;
annotation    = "@", identifier, "(", key_values, ")" ;
port          = "port", identifier, [ ":", label ], [ attributes ] ;
relation      = ("align" | "distribute"), ("row" | "col"),
                "[", references, "]", [ attributes ] ;
zone          = "zone", identifier, [ quoted_label ], "around",
                "[", references, "]", [ attributes ],
                [ "{", properties, "}" ] ;
flow          = "flow", identifier, [ quoted_label ], "{", path, "}" ;
view          = "view", identifier, "{", "include", selection, "}" ;
layout        = "@layout", [ "(", identifier, ")" ],
                "{", { target, annotation }, "}" ;
strata        = "strata", "[", identifiers, "]" ;
story         = "@story", "[", path, "]" ;
```

## Lexical conventions

- Newlines and semicolons end ordinary statements; braces delimit blocks.
- Identifiers can be qualified with `.`. Unicode identifier text is preserved.
- Quoted strings use JSON escaping. Unquoted labels extend to the next attribute/annotation or statement boundary.
- Numbers can carry a `g` suffix when a field expects grid units.
- Commas separate key/value pairs and list members. Whitespace and comments remain concrete syntax tokens.
- A whitespace-prefixed `#` begins a comment; an attached `#id` is a port suffix.

```diagram
# Unicode and quoted labels describe the same two-node topology.
α: "Public API"; β: "Ledger λ"
α -> β: "read / write"
```

## Consumed keys

| Construct | Keys consumed by the implementation |
| --- | --- |
| Node | `shape`, `stratum`; runtime and lens attributes described in their guides |
| Port | `side`, `order`, `ratio`, `pos` |
| Pin | `grid`, `x`, `y`, `lock` |
| Route | `segment`, `lane`, `channel_x` |
| Label | `slot`, `slide`, manual pixel center `x`, `y` |
| Distribution | `gap` |
| Zone | `padding`, `tint`; `state:name`, `invariant:name` from body entries |
| Simulation | `rate`, `workers`, `service_rate`, `service`, `queue_cap`, `timeout`, `retry`, `retry_fraction`, `retry_backoff` |
| Binding | `otel`, `rust` |

Unknown metadata can survive lowering without acquiring rendering or runtime semantics. All five port tiers and the [seven typed edit operations](/guide/editing) are represented explicitly in the public IR/API.

### Strata and runtime metadata

This is the canonical 2D graph for an application service and its host. The stratum and simulation/binding metadata remain in the model; [independent-plane rendering](/runtime/spatial), [simulation](/runtime/simulation), and [drift reconciliation](/runtime/drift) consume them explicitly.

```diagram
strata [app, hardware]
api: API [stratum: app] @sim(rate: 80) @bind(otel: "service.name=api")
host: Host [stratum: hardware]
api -> host [relation: runs_on]
```

## Complete language example

This complete source combines scoped ownership, named record ports, typed shapes, a shared zone, a flow, a view and a story. Its rendered output is generated from the same file used by the CLI and example gallery.

<<< ../../examples/architecture.diagram
