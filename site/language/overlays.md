# Zones, flows and views

These constructs reference one canonical graph. They add meaning and presentation without changing node ownership.

## Shared zones

```diagram
prod { api -> db }
kms: Key service
zone pci "PCI scope" around [prod.db, kms] {
  padding: 1.5g
  tint: oklch(0.78 0.08 170 / 0.16)
}
```

Zones render a smooth oriented enclosing ellipse around their members, crossing container boundaries and overlapping other zones without changing node ownership. `padding` is in grid units. OKLCH tints are converted to sRGB with alpha for consistent native exports.

Zone `state` and `invariant` entries also provide metadata for [bounded verification](/runtime/verification). The caller supplies the finite transition system.

## Numbered flows

```diagram
client -> api: HTTPS
api -> db: SQL
flow request "Request path" {
  client -> api -> db
}
```

A flow resolves an ordered sequence of existing nodes and connections. It does not create missing connections. Rendering adds numbered step badges and a highlighted path. `WasmDocument.flows_json()` lists flows; `flow_frame_svg(id, step)` exposes zero-based playback frames.

## Canonical views

```diagram views=request_only,downstream
client -> api -> db
api -> metrics
view request_only {
  include client -> api -> db
}
view downstream {
  include api -> *
}
```

`*` includes outgoing reachability from the preceding seed. Named members and required ownership ancestors are retained; edges and relations are filtered to the projected graph. A named sequence selects those members; it is not a new protocol definition.

Use `views::project(&scene, name)` or `--view name` in the CLI. A view chooses graph membership; an `@layout(name)` variant chooses constraints. They solve different authoring needs.

Use the comparison tabs above to inspect the full graph, the request path without `metrics`, and the downstream view without `client`.

## Story and causal focus

```diagram focus=api
client -> api -> db
api -> metrics
metrics -> archive
@story [client -> api -> db]
```

The story marks a static path for saliency. `views::causal_cone` and `views::attention` calculate k-hop focus from incoming, outgoing or both directions. The browser uses `focus_svg(node, hops)` for hover emphasis while retaining context.

The **Full context** tab shows the whole story. **Focus: api** applies one-hop focus to `api`: `archive` remains in place with faded shape ink because it is two hops away. Its label keeps enough contrast to remain readable.
