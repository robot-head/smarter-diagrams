//! Code/runtime binding hydration and declared-versus-observed graph reconciliation.
use crate::{Error, edit::TextPatch, model::*, syntax::Span};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ObservedEdge {
    pub source: NodeId,
    pub target: NodeId,
    pub evidence: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DriftReport {
    pub phantom: Vec<ObservedEdge>,
    pub unobserved: Vec<EdgeId>,
    pub missing_symbols: Vec<NodeId>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanRecord {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub service: String,
}
fn binding(node: &Node, key: &str) -> Option<String> {
    node.annotations
        .iter()
        .find(|a| a.name == "bind")
        .and_then(|a| a.attrs.get(key))
        .map(|v| serde_json::from_str(&v.text).unwrap_or_else(|_| v.text.clone()))
}
pub fn observed_from_spans(scene: &SceneGraph, spans: &[SpanRecord]) -> Vec<ObservedEdge> {
    let services: BTreeMap<_, _> = scene
        .nodes
        .iter()
        .filter_map(|n| {
            binding(n, "otel").map(|b| {
                (
                    b.strip_prefix("service.name=").unwrap_or(&b).to_owned(),
                    n.id.clone(),
                )
            })
        })
        .collect();
    let lookup: BTreeMap<_, _> = spans
        .iter()
        .map(|s| ((s.trace_id.as_str(), s.span_id.as_str()), s))
        .collect();
    let mut out = BTreeSet::new();
    for s in spans {
        if let Some(parent) = s
            .parent_span_id
            .as_deref()
            .and_then(|p| lookup.get(&(s.trace_id.as_str(), p)))
            && parent.service != s.service
        {
            let a = services
                .get(&parent.service)
                .cloned()
                .unwrap_or_else(|| parent.service.clone());
            let b = services
                .get(&s.service)
                .cloned()
                .unwrap_or_else(|| s.service.clone());
            out.insert(ObservedEdge {
                source: a,
                target: b,
                evidence: format!("otel:{}:{}", s.trace_id, s.span_id),
            });
        }
    }
    out.into_iter().collect()
}
/// Parse OTLP/HTTP JSON resourceSpans and retain cross-service parent-child relationships.
pub fn read_otlp(scene: &SceneGraph, json: &str) -> Result<Vec<ObservedEdge>, Error> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| Error::Edit(e.to_string()))?;
    let resources = v
        .get("resourceSpans")
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::Edit("OTLP JSON requires resourceSpans".into()))?;
    let mut spans = Vec::new();
    for resource in resources {
        let service = resource
            .pointer("/resource/attributes")
            .and_then(|v| v.as_array())
            .and_then(|a| a.iter().find(|v| v["key"] == "service.name"))
            .and_then(|v| v.pointer("/value/stringValue"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        if let Some(scopes) = resource["scopeSpans"].as_array() {
            for scope in scopes {
                if let Some(ss) = scope["spans"].as_array() {
                    for s in ss {
                        spans.push(SpanRecord {
                            trace_id: s["traceId"].as_str().unwrap_or("").into(),
                            span_id: s["spanId"].as_str().unwrap_or("").into(),
                            parent_span_id: s["parentSpanId"]
                                .as_str()
                                .filter(|v| !v.is_empty())
                                .map(str::to_owned),
                            service: service.into(),
                        });
                    }
                }
            }
        }
    }
    Ok(observed_from_spans(scene, &spans))
}
pub fn reconcile(
    scene: &SceneGraph,
    observed: &[ObservedEdge],
    symbols: Option<&BTreeSet<String>>,
) -> DriftReport {
    let mut report = DriftReport::default();
    let mut seen = BTreeSet::new();
    for edge in observed {
        if !scene
            .edges
            .iter()
            .any(|e| e.source.node == edge.source && e.target.node == edge.target)
            && seen.insert((edge.source.clone(), edge.target.clone()))
        {
            report.phantom.push(edge.clone());
        }
    }
    for edge in &scene.edges {
        if !observed
            .iter()
            .any(|e| e.source == edge.source.node && e.target == edge.target.node)
        {
            report.unobserved.push(edge.id.clone());
        }
    }
    if let Some(symbols) = symbols {
        for node in &scene.nodes {
            if let Some(symbol) = binding(node, "rust")
                && !symbols.contains(&symbol)
            {
                report.missing_symbols.push(node.id.clone());
            }
        }
    }
    report
}
/// A source patch for deliberate promotion of observed calls; never silently mutates source.
pub fn promote(source: &str, edge: &ObservedEdge) -> Result<TextPatch, Error> {
    fn valid(id: &str) -> bool {
        id.split('.').all(|part| {
            let mut c = part.chars();
            c.next().is_some_and(|c| c.is_alphabetic() || c == '_')
                && c.all(|c| c.is_alphanumeric() || c == '_')
        })
    }
    if !valid(&edge.source) || !valid(&edge.target) {
        return Err(Error::Edit(
            "observed service IDs need explicit valid DSL bindings before promotion".into(),
        ));
    }
    let prefix = if source.is_empty() || source.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    Ok(TextPatch {
        delete_range: Span::new(source.len(), source.len()),
        insert_text: format!("{prefix}{} -> {}\n", edge.source, edge.target),
        expected_text: Some(String::new()),
    })
}
#[cfg(feature = "code-bindings")]
pub fn rust_symbols(source: &str, module: &str) -> Result<BTreeSet<String>, Error> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .map_err(|e| Error::Edit(e.to_string()))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| Error::Edit("Rust parser cancelled".into()))?;
    if tree.root_node().has_error() {
        return Err(Error::Edit("Rust source contains syntax errors".into()));
    }
    let mut out = BTreeSet::new();
    fn visit(node: tree_sitter::Node, source: &str, path: &str, out: &mut BTreeSet<String>) {
        let named = matches!(
            node.kind(),
            "struct_item"
                | "enum_item"
                | "trait_item"
                | "function_item"
                | "type_item"
                | "mod_item"
                | "const_item"
                | "static_item"
        );
        let mut next = path.to_string();
        if named && let Some(name) = node.child_by_field_name("name") {
            let name = &source[name.byte_range()];
            next = if path.is_empty() {
                name.into()
            } else {
                format!("{path}::{name}")
            };
            out.insert(next.clone());
        }
        if node.kind() == "impl_item"
            && let Some(ty) = node.child_by_field_name("type")
        {
            let name = source[ty.byte_range()].split('<').next().unwrap();
            next = format!("{path}::{name}");
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            visit(
                child,
                source,
                if matches!(node.kind(), "mod_item" | "impl_item") {
                    &next
                } else {
                    path
                },
                out,
            );
        }
    }
    visit(tree.root_node(), source, module, &mut out);
    Ok(out)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftView {
    pub scene: SceneGraph,
    pub layout: crate::layout::Layout,
    pub report: DriftReport,
}
impl DriftView {
    /// Reconciliation status lives at node corners; evidence remains in titles,
    /// leaving protocol captions and routed edges available for the diagram.
    pub fn to_svg(&self) -> String {
        crate::render::svg_with_decorations(
            &self.scene,
            &self.layout,
            |_| true,
            |node, rect| {
                let observed = self
                    .report
                    .phantom
                    .iter()
                    .filter(|e| e.source == node.id)
                    .count();
                let absent = self
                    .report
                    .unobserved
                    .iter()
                    .filter(|id| {
                        self.scene
                            .edge(id)
                            .is_some_and(|e| e.source.node == node.id)
                    })
                    .count();
                let missing = usize::from(self.report.missing_symbols.contains(&node.id));
                let count = observed + absent + missing;
                if count == 0 {
                    return String::new();
                }
                let title = crate::render::escape(&format!(
                    "{}: {observed} observed-only calls, {absent} unobserved calls, {missing} missing symbols",
                    node.label
                ));
                format!(
                    "<g data-drift-badge=\"{}\" aria-label=\"{title}\"><title>{title}</title><rect x=\"{}\" y=\"{}\" width=\"72\" height=\"20\" rx=\"8\" fill=\"var(--sd-bg,#ffffff)\" stroke=\"var(--sd-modified,#6276bd)\"/><text x=\"{}\" y=\"{}\" text-anchor=\"middle\" dominant-baseline=\"central\" style=\"font-size:12px\">drift: {count}</text></g>",
                    crate::render::escape(&node.id),
                    rect.right() - 72.,
                    rect.top() - 24.,
                    rect.right() - 36.,
                    rect.top() - 14.
                )
            },
        )
    }
}
pub fn hydrate(
    source: &str,
    observed: &[ObservedEdge],
    options: &crate::layout::LayoutOptions,
    prior: &crate::layout::Layout,
) -> Result<DriftView, Error> {
    let scene = crate::model::lower(&crate::syntax::parse(source));
    let report = reconcile(&scene, observed, None);
    let patches: Vec<_> = report
        .phantom
        .iter()
        .map(|e| promote(source, e))
        .collect::<Result<_, _>>()?;
    let mut text = source.to_owned();
    for patch in patches {
        text.push_str(&patch.insert_text);
    }
    let mut graph = crate::model::lower(&crate::syntax::parse(&text));
    for edge in &mut graph.edges {
        if report
            .phantom
            .iter()
            .any(|p| p.source == edge.source.node && p.target == edge.target.node)
        {
            edge.attrs.insert(
                "phantom".into(),
                Value {
                    text: "true".into(),
                    ..Default::default()
                },
            );
        }
    }
    if graph
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error)
    {
        return Err(Error::Diagnostics(graph.diagnostics));
    }
    let layout = crate::layout::layout(&graph, options, Some(prior))?;
    Ok(DriftView {
        scene: graph,
        layout,
        report,
    })
}
pub fn enforce_boundary(report: &DriftReport) -> Result<(), Error> {
    if report.phantom.is_empty() && report.missing_symbols.is_empty() {
        Ok(())
    } else {
        Err(Error::Layout(format!(
            "architecture boundary violation: {} undocumented calls; {} missing symbols",
            report.phantom.len(),
            report.missing_symbols.len()
        )))
    }
}
