# Telemetry and code drift

Bindings connect semantic diagram IDs to observed services and static Rust symbols. The library consumes evidence supplied by a host; it does not run a collector or upload traces.

```diagram
api @bind(otel: "service.name=api", rust: "crate::Api")
db @bind(otel: "service.name=db")
```

## Build the observed graph

`SpanRecord` contains `trace_id`, `span_id`, optional `parent_span_id` and `service`. `observed_from_spans` maps cross-service parent/child relationships to `ObservedEdge { source, target, evidence }`. `read_otlp` accepts OTLP/HTTP JSON `resourceSpans` and performs the same reconciliation input step.

```rust
use smarter_diagrams::drift::{SpanRecord, observed_from_spans, reconcile};
let diagram = smarter_diagrams::compile(
    "api @bind(otel: \"service.name=api\")\ndb @bind(otel: \"service.name=db\")\n"
)?;
let observed = observed_from_spans(&diagram.scene, &[
    SpanRecord { trace_id: "t".into(), span_id: "1".into(), parent_span_id: None, service: "api".into() },
    SpanRecord { trace_id: "t".into(), span_id: "2".into(), parent_span_id: Some("1".into()), service: "db".into() },
]);
let report = reconcile(&diagram.scene, &observed, None);
assert_eq!(report.phantom.len(), 1);
```

## Interpret and act

| Field | Meaning |
| --- | --- |
| `phantom` | Observed connections absent from the declared graph |
| `unobserved` | Declared connections absent from the supplied observations |
| `missing_symbols` | Bindings missing from an optionally supplied Rust symbol set |

An unobserved connection may simply be outside the trace sample. The report describes the input evidence; it does not prove a declared connection never occurs.

`hydrate` constructs a differential view, preserving pinned coordinates and marking phantom connections. `promote(source, observed_edge)` returns an explicit source patch. Review and apply that patch to make an observed connection part of the canonical declaration. `enforce_boundary` converts phantom/missing-symbol drift into a CI error for a host that has chosen that policy.

## Static symbols and collection

Enable `code-bindings` for `rust_symbols(source, module)`, backed by Tree-sitter Rust. It recognizes module-qualified items and impl methods. Supply the resulting symbol set to `reconcile` for binding checks.

OTel transport, trace sampling, eBPF adapters, file discovery and authentication belong to the embedding application. This keeps collection policy and credentials out of the compiler.
