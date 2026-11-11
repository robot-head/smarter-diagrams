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

## Consumed keys

| Construct | Keys consumed by the implementation |
| --- | --- |
| Node | `shape`, `stratum`; runtime and lens attributes described in their guides |
| Port | `side`, `order`, `ratio`, `pos` |
| Pin | `grid`, `x`, `y`, `lock` |
| Distribution | `gap` |
| Zone | `padding`, `tint`; `state:name`, `invariant:name` from body entries |
| Simulation | `rate`, `workers`, `service_rate`, `service`, `queue_cap`, `timeout`, `retry`, `retry_fraction`, `retry_backoff` |
| Binding | `otel`, `rust` |

Unknown metadata can survive lowering without acquiring rendering or runtime semantics. All five port tiers and the [seven typed edit operations](/guide/editing) are represented explicitly in the public IR/API.
