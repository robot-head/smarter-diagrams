# Errors and troubleshooting

## Error categories

| Error | Typical cause | Next action |
| --- | --- | --- |
| `Diagnostics` | Invalid source, unknown named port, conflicting port hints or unresolved relation | Read the diagnostic's byte span; fix the declaration/reference |
| `Edit` | Stale revision, invalid typed payload, overlapping patch or invalid UTF-8 range | Refresh the snapshot and use current byte ranges/revision |
| `Layout` | Inconsistent pins, insufficient locked perimeter or blocked locked channel | Relax the relevant constraint or increase available space |

`syntax::parse` retains malformed input, including error ranges, for an editor. `compile` only succeeds after source and geometry validation. A failed `Document` edit keeps the last good snapshot; do not replace the editor's source with a failed result.

## Common authoring problems

**A named port is unknown.** Declare `port id` in the node block, or use a side-qualified virtual port such as `broker.E#orders`. A bare `db#id` is an exact named reference, not an inferred row declaration.

**A drag cannot move a node.** Absolute pins, relative anchors, alignment and size locks can conflict. Use `BreakAlignment` or unpin an entity to relax the appropriate constraint. Pins locate centroids, not upper-left corners.

**The whole graph moved after a change.** Fresh layout has no fixed coordinate frame. Keep a `Document` or pass prior geometry to `compile_with`; add absolute pins when world positions matter. Structural changes can still require a global solve.

**An edge label has a leader line.** Direct slots were infeasible under node/port/route/other-label clearance. Increase spacing or use a typed label/route edit. The linter reports callout usage as a readability finding.

## Browser and export problems

**The playground stays at “Loading”.** Check the module/Wasm requests in browser developer tools. Build both packages, serve over HTTP(S), and preserve the relative package/worker paths. `.wasm` needs the `application/wasm` MIME type. The Rust/Wasm-bindgen CLI versions must match the Cargo lockfile.

**The browser has no WebGPU.** Stratum geometry uses SVG automatically. WebGPU is optional and independent of the ordinary SVG authoring renderer.

**PNG/PDF is unsupported.** Reinstall with `--features export`. If native text export reports no fonts, install a TrueType/OpenType sans-serif font. Font choice can change final glyph appearance.

**Raster export is too large.** The PNG renderer caps allocation at 100 megapixels and scale at 8. Use SVG for a very large diagram or render a named view.

**Simulation has no load.** Supply an ingress `@sim(rate: ...)`. The example slider targets `client`; embedding code can call `set_rate` for any service node. A fresh source update restarts the simulation.

**A verification result is unknown.** The finite search budget or model/horizon limit was exceeded. Reduce the model, split checks by zone or choose a shorter horizon. Unknown is not a safety proof.

## Reproduce a report

Include the minimal `.diagram` source, feature flags, current Rust/Wasm/browser version, layout options and whether prior geometry was supplied. For edit failures, include the operation and expected revision without unrelated confidential source. The [verification page](/reference/verification) lists known validation limits and the [performance page](/reference/performance) records fixture methodology.
