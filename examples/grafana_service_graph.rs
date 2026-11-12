//! Demonstrate rendering a Grafana Tempo service graph and reconciling live OTLP trace drift.
//!
//! Converts `traces_service_graph_request_*` RED metrics into Smarter Diagrams source, compiles
//! an orthogonal service-graph SVG, and reconciles OTLP spans to detect undeclared phantom calls.
//!
//! Run with:
//!   cargo run --example grafana_service_graph
use smarter_diagrams::{
    LayoutOptions, compile,
    drift::{self, SpanRecord},
};
use std::collections::BTreeMap;

struct TempoEdgeMetric<'a> {
    client: &'a str,
    server: &'a str,
    connection_type: &'a str,
    rps: f64,
    failed_rps: f64,
    p95_ms: u32,
}

fn id_of(service: &str) -> String {
    service.replace('-', "_")
}

fn shape_for(connection_type: &str) -> Option<&'static str> {
    match connection_type {
        "database" => Some("database"),
        "messaging_system" => Some("queue"),
        "cache" => Some("cache"),
        _ => None,
    }
}

fn build_diagram_from_tempo(edges: &[TempoEdgeMetric<'_>]) -> String {
    let mut node_shapes: BTreeMap<&str, Option<&str>> = BTreeMap::new();
    let mut inbound_rps: BTreeMap<&str, f64> = BTreeMap::new();
    for edge in edges {
        node_shapes.entry(edge.client).or_insert(None);
        let inferred = shape_for(edge.connection_type);
        let entry = node_shapes.entry(edge.server).or_insert(inferred);
        if entry.is_none() && inferred.is_some() {
            *entry = inferred;
        }
        *inbound_rps.entry(edge.server).or_default() += edge.rps;
    }

    let mut out = String::new();
    for (service, shape) in &node_shapes {
        let id = id_of(service);
        let attrs = shape.map(|s| format!(" [shape: {s}]")).unwrap_or_default();
        let rate = inbound_rps.get(service).copied().unwrap_or(240.0);
        let service_rate = (rate * 1.35).ceil() as u32;
        out.push_str(&format!(
            "{id}: \"{service}\"{attrs} @sim(service_rate: {service_rate}) @bind(otel: \"service.name={service}\")\n"
        ));
    }
    out.push('\n');

    let mut slo_breached = Vec::new();
    for edge in edges {
        let err_pct = if edge.rps > 0.0 {
            (edge.failed_rps / edge.rps) * 100.0
        } else {
            0.0
        };
        let caption = if err_pct >= 1.0 {
            slo_breached.push(id_of(edge.client));
            slo_breached.push(id_of(edge.server));
            format!("{:.0} rps · {:.1}% err", edge.rps, err_pct)
        } else {
            format!("{:.0} rps · {}ms", edge.rps, edge.p95_ms)
        };
        out.push_str(&format!(
            "{} -> {}: \"{caption}\"\n",
            id_of(edge.client),
            id_of(edge.server)
        ));
    }
    slo_breached.sort();
    slo_breached.dedup();
    if !slo_breached.is_empty() {
        out.push_str(&format!(
            "\nzone slo_burn \"SLO error budget burn\" around [{}] {{\n  padding: 1.5g\n  tint: oklch(0.76 0.12 35 / 0.16)\n}}\n",
            slo_breached.join(", ")
        ));
    }
    out
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tempo_metrics = [
        TempoEdgeMetric {
            client: "ingress-nginx",
            server: "shop-frontend",
            connection_type: "http",
            rps: 240.0,
            failed_rps: 0.2,
            p95_ms: 11,
        },
        TempoEdgeMetric {
            client: "shop-frontend",
            server: "catalog-api",
            connection_type: "grpc",
            rps: 165.0,
            failed_rps: 0.1,
            p95_ms: 8,
        },
        TempoEdgeMetric {
            client: "shop-frontend",
            server: "checkout-api",
            connection_type: "grpc",
            rps: 75.0,
            failed_rps: 1.4,
            p95_ms: 92,
        },
        TempoEdgeMetric {
            client: "catalog-api",
            server: "redis-cart",
            connection_type: "cache",
            rps: 150.0,
            failed_rps: 0.0,
            p95_ms: 1,
        },
        TempoEdgeMetric {
            client: "checkout-api",
            server: "payment-gw",
            connection_type: "virtual_node",
            rps: 62.0,
            failed_rps: 1.2,
            p95_ms: 184,
        },
        TempoEdgeMetric {
            client: "checkout-api",
            server: "postgres-orders",
            connection_type: "database",
            rps: 62.0,
            failed_rps: 0.0,
            p95_ms: 14,
        },
    ];

    let generated = build_diagram_from_tempo(&tempo_metrics);
    let diagram = compile(&generated)?;
    let service_svg = diagram.to_svg();

    // Live OTLP span sample includes an undeclared bypass from checkout-api -> redis-cart.
    let spans = [
        SpanRecord {
            trace_id: "4bf92f3577b34da6".into(),
            span_id: "01".into(),
            parent_span_id: None,
            service: "ingress-nginx".into(),
        },
        SpanRecord {
            trace_id: "4bf92f3577b34da6".into(),
            span_id: "02".into(),
            parent_span_id: Some("01".into()),
            service: "shop-frontend".into(),
        },
        SpanRecord {
            trace_id: "4bf92f3577b34da6".into(),
            span_id: "03".into(),
            parent_span_id: Some("02".into()),
            service: "checkout-api".into(),
        },
        SpanRecord {
            trace_id: "4bf92f3577b34da6".into(),
            span_id: "04".into(),
            parent_span_id: Some("03".into()),
            service: "redis-cart".into(),
        },
    ];
    let observed = drift::observed_from_spans(&diagram.scene, &spans);
    let report = drift::reconcile(&diagram.scene, &observed, None);
    assert_eq!(report.phantom.len(), 1);
    assert_eq!(report.phantom[0].source, "checkout_api");
    assert_eq!(report.phantom[0].target, "redis_cart");

    let drift_view = drift::hydrate(
        &generated,
        &observed,
        &LayoutOptions::default(),
        &diagram.layout,
    )?;
    let drift_svg = drift_view.to_svg();
    let patch = drift::promote(&generated, &report.phantom[0])?;
    assert!(patch.insert_text.contains("checkout_api -> redis_cart"));

    if let Some(dir) = std::env::args().nth(1) {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(format!("{dir}/grafana-service-graph.svg"), &service_svg)?;
        std::fs::write(format!("{dir}/grafana-drift.svg"), &drift_svg)?;
    }

    println!(
        "Grafana service graph: {} services, {} RED edges | OTLP drift detected {} phantom call ({} -> {})",
        diagram.scene.nodes.len(),
        diagram.scene.edges.len(),
        report.phantom.len(),
        report.phantom[0].source,
        report.phantom[0].target,
    );
    Ok(())
}
