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

Use `views::project(&scene, name)` or `--view name` in the CLI. For cumulative progressive disclosure (`step <name> { ... }`) that keeps canonical node coordinates anchored across steps, plus non-cumulative `scenario` overlays, see [Views, progressive steps, focus & lenses](/runtime/views). A view chooses graph membership; an `@layout(name)` variant chooses constraints. They solve different authoring needs.

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

## Animated dashed edges (marching ants)

```diagram
producer: Event Producer
stream: Kafka Partition [shape: queue]
consumer: Stream Processor
sink: Analytics Warehouse [shape: database]

producer -> stream: "publish" [style: marching-ants]
stream -> consumer: "consume" [style: marching-ants]
consumer -> sink: "flush" [style: dashed]

align row [producer, stream, consumer, sink]
```

Set `[style: marching-ants]` (or `[style: animated]` / `[animate: true]`) on any edge or flow segment to animate dashed flow along the connection direction. `[style: dotted]` draws dotted strokes, and `[style: dashed]` draws static dashes. Animations automatically honor `@media (prefers-reduced-motion: reduce)`.

## Syntax-highlighted code inside nodes

````diagram
api: """
Worker Handler
```ts
export async function handle(id: string): Promise<number> {
  const count = await claimRows(id, 64);
  return count;
}
```
"""

db: """
Queue Store
```sql
SELECT id, payload
FROM outbox
WHERE sent_at IS NULL -- pending rows
LIMIT 64;
```
""" [shape: database]

api -> db: "claim batch" [style: marching-ants]
````

Triple-quoted `"""..."""` node labels preserve line breaks and common-indent dedenting cleanly. Fenced blocks (`sql`, `rust`, `go`, `ts`/`js`, `python`, `json`/`yaml`, `sh`) render inside a recessed code container sized during layout so connectors never cross text.

## Color themes (Catppuccin, Solarized, Base16)

```diagram
theme: catppuccin-mocha

ingress: "Edge Router" [shape: actor]
worker: "Worker Pool"
cache: "Hot Cache" [shape: cache]
store: "Primary DB" [shape: database]

ingress -> worker: "dispatch" [style: marching-ants]
worker -> cache: "lookup"
worker -> store: "commit"
align row [ingress, worker, cache]
```

Every diagram viewer includes a live **Theme** switcher, and authors or CI exports can pin any palette directly:

- **Built-in palettes**: `light`, `dark`, Catppuccin (`catppuccin-latte`, `catppuccin-frappe`, `catppuccin-macchiato`, `catppuccin-mocha`), Solarized (`solarized-light`, `solarized-dark`), and Base16 presets (`base16-nord`, `base16-ocean`, `base16-eighties`, `base16-gruvbox-dark`, `base16-onedark`, `base16-dracula`, `base16-tokyo-night`, `base16-github`).
- **Source directive**: add `theme: <name>` or `@theme(<name>)` at top level so SVGs and PNG/PDF exports bake in that theme.
- **Custom Base16 scheme families**: load any standard `base00`–`base0F` YAML or JSON scheme with `--theme-file path/to/scheme.yaml`, `ThemePalette::from_base16_str(...)`, `ThemePalette::from_base16_dir(...)`, or **Import Base16** in the browser playground. All derived shape fills, borders, and syntax token colors are checked to satisfy WCAG contrast (`>= 4.5:1` text, `>= 3:1` outlines).

## Hyperlinks on nodes and edges

```diagram
portal: "Developer Portal" [shape: actor, link: "https://example.com/portal"]
service: "Checkout API" [href: "/guide/getting-started"]
db: "Orders DB" [shape: database, link: "https://example.com/db/orders"]

portal -> service: "REST" [link: "https://example.com/openapi"]
service -> db: "SQL" [link: "https://example.com/db/schema"]
```

Attach `[link: "..."]` or `[href: "..."]` to any leaf node, container, or connection. External `http://` and `https://` URLs open in a new tab with `rel="noopener noreferrer"`; internal `/`, `./`, `../`, `#`, and `mailto:` targets remain same-page links, and dangerous schemes (`javascript:`, `data:`, `vbscript:`) are rejected with a compile diagnostic.
