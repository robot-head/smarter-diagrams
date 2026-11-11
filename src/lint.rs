//! Deterministic aesthetic diagnostics with an optional caller-supplied visual critic.
use crate::{Diagram, layout::Point, syntax::Span};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub code: String,
    pub message: String,
    pub entity: Option<String>,
    pub span: Option<Span>,
    pub severity: String,
}
fn finding(code: &str, message: String, entity: Option<String>, span: Option<Span>) -> Finding {
    Finding {
        code: code.into(),
        message,
        entity,
        span,
        severity: "warning".into(),
    }
}
pub fn lint(d: &Diagram) -> Vec<Finding> {
    let mut out = Vec::new();
    let routes: Vec<_> = d.layout.routes.values().collect();
    for (i, r) in routes.iter().enumerate() {
        let bends = r
            .points
            .windows(3)
            .filter(|p| cross(p[0], p[1], p[2]).abs() > 1e-6)
            .count();
        if bends > 4 {
            out.push(finding(
                "excess-bends",
                format!("{bends} bends; consider an explicit side or route channel"),
                Some(r.edge.clone()),
                d.scene.edge(&r.edge).map(|e| e.origin.primary_decl),
            ));
        }
        for other in &routes[i + 1..] {
            let count = r
                .points
                .windows(2)
                .flat_map(|a| other.points.windows(2).map(move |b| (a, b)))
                .filter(|(a, b)| crate::routing::proper_crossing(a[0], a[1], b[0], b[1]))
                .count();
            if count > 0 {
                out.push(finding(
                    "crossing",
                    format!("{} crosses {} {count} time(s)", r.edge, other.edge),
                    Some(r.edge.clone()),
                    None,
                ));
            }
        }
    }
    for label in &d.layout.labels {
        if label.callout {
            out.push(finding(
                "label-callout",
                "Label needed an external leader callout; increase space near this edge".into(),
                Some(label.feature.clone()),
                None,
            ));
        }
    }
    for n in &d.scene.nodes {
        if n.label.chars().count() > 48 {
            out.push(finding(
                "long-label",
                "Shorten this label or use a semantic abbreviation".into(),
                Some(n.id.clone()),
                Some(n.origin.primary_decl),
            ));
        }
    }
    out
}
fn cross(a: Point, b: Point, c: Point) -> f64 {
    (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x)
}
/// Integrators provide their own authenticated VLM transport. No network calls occur in the library.
pub trait VisualCritic {
    fn critique(
        &self,
        svg: &str,
        scene: &crate::model::SceneGraph,
    ) -> Result<Vec<Finding>, crate::Error>;
}
pub fn with_critic(d: &Diagram, critic: &impl VisualCritic) -> Result<Vec<Finding>, crate::Error> {
    let mut findings = lint(d);
    findings.extend(critic.critique(&d.to_svg(), &d.scene)?);
    Ok(findings)
}
