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
    let routes: Vec<_> = d
        .layout
        .routes
        .values()
        .map(|r| (r, crate::routing::coalesced(&r.points)))
        .collect();
    for (i, (r, pts)) in routes.iter().enumerate() {
        let edge_span = d.scene.edge(&r.edge).map(|e| e.origin.primary_decl);
        let bends = pts.windows(3).filter(|p| is_bend(p[0], p[1], p[2])).count();
        if bends > 4 {
            out.push(finding(
                "excess-bends",
                format!("{bends} bends; consider an explicit side or route channel"),
                Some(r.edge.clone()),
                edge_span,
            ));
        }
        if pts.len() >= 4 {
            let mut micro_jog_count = 0usize;
            let mut min_jog = f64::INFINITY;
            for k in 1..pts.len() - 2 {
                if is_bend(pts[k - 1], pts[k], pts[k + 1])
                    && is_bend(pts[k], pts[k + 1], pts[k + 2])
                {
                    let len = pts[k].distance(pts[k + 1]);
                    if len < 12.0 - 1e-6 {
                        micro_jog_count += 1;
                        min_jog = min_jog.min(len);
                    }
                }
            }
            if micro_jog_count > 0 {
                out.push(finding(
                    "micro-jog",
                    format!(
                        "{micro_jog_count} interior micro-jog segment(s) under 12 px (shortest {min_jog:.1} px)"
                    ),
                    Some(r.edge.clone()),
                    edge_span,
                ));
            }
        }
        let is_self_loop = d
            .scene
            .edge(&r.edge)
            .is_some_and(|e| e.source.node == e.target.node);
        if !is_self_loop && let (Some(&start), Some(&end)) = (pts.first(), pts.last()) {
            let direct = start.manhattan_distance(end);
            let total_len: f64 = pts.windows(2).map(|p| p[0].distance(p[1])).sum();
            if direct >= 48.0 && total_len > 2.8 * direct + 1e-6 {
                out.push(finding(
                    "excess-detour",
                    format!(
                        "Route length {total_len:.1} px is {:.2}x direct Manhattan distance ({direct:.1} px)",
                        total_len / direct
                    ),
                    Some(r.edge.clone()),
                    edge_span,
                ));
            }
        }
        for (other, other_pts) in &routes[i + 1..] {
            let count = pts
                .windows(2)
                .flat_map(|a| other_pts.windows(2).map(move |b| (a, b)))
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
            let overlap: f64 = pts
                .windows(2)
                .flat_map(|a| {
                    other_pts
                        .windows(2)
                        .map(move |b| crate::routing::collinear_overlap(a[0], a[1], b[0], b[1]))
                })
                .filter(|&len| len > 1e-6)
                .sum();
            if overlap > 1e-6 {
                out.push(finding(
                    "collinear-overlap",
                    format!(
                        "{} overlaps {} collinearly ({overlap:.1} px)",
                        r.edge, other.edge
                    ),
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
fn is_bend(a: Point, b: Point, c: Point) -> bool {
    let l1 = a.distance(b);
    let l2 = b.distance(c);
    if l1 <= 1e-7 || l2 <= 1e-7 {
        return false;
    }
    let cross_norm = cross(a, b, c).abs() / (l1 * l2);
    let dot_norm = ((b.x - a.x) * (c.x - b.x) + (b.y - a.y) * (c.y - b.y)) / (l1 * l2);
    cross_norm > 1e-6 || dot_norm < -1.0 + 1e-6
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
#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile;

    fn codes_for<'a>(findings: &'a [Finding], entity: &str) -> Vec<&'a str> {
        findings
            .iter()
            .filter(|f| f.entity.as_deref() == Some(entity))
            .map(|f| f.code.as_str())
            .collect()
    }

    #[test]
    fn coalesces_collinear_points_and_duplicates_before_counting_bends() {
        let mut d = compile("a -> b\n").unwrap();
        // Straight multi-lattice points with tiny 1e-7 drift should coalesce to 0 bends.
        d.layout.routes.get_mut("a->b").unwrap().points = vec![
            Point::new(0., 0.),
            Point::new(1000., 1e-7),
            Point::new(2000., 0.),
            Point::new(3000., 1e-7),
            Point::new(4000., 0.),
            Point::new(5000., 1e-7),
            Point::new(6000., 0.),
        ];
        assert!(
            !codes_for(&lint(&d), "a->b").contains(&"excess-bends"),
            "collinear multi-lattice vertices must not inflate bend counts"
        );

        // Five true bends where one corner has a duplicate vertex (previously a false negative).
        d.layout.routes.get_mut("a->b").unwrap().points = vec![
            Point::new(0., 0.),
            Point::new(20., 0.),
            Point::new(20., 0.),
            Point::new(20., 20.),
            Point::new(40., 20.),
            Point::new(40., 40.),
            Point::new(60., 40.),
            Point::new(60., 60.),
        ];
        let findings = lint(&d);
        let excess = findings
            .iter()
            .find(|f| f.entity.as_deref() == Some("a->b") && f.code == "excess-bends")
            .expect("coalesced duplicate corner should still report 5 bends");
        assert!(excess.message.starts_with("5 bends"));
    }

    #[test]
    fn detects_collinear_overlap_without_false_positives() {
        let mut d = compile("a -> b\nc -> d\n").unwrap();
        d.layout.routes.get_mut("a->b").unwrap().points = vec![
            Point::new(0., 0.),
            Point::new(40., 0.),
            Point::new(80., 0.),
            Point::new(80., 40.),
        ];
        d.layout.routes.get_mut("c->d").unwrap().points =
            vec![Point::new(20., 0.), Point::new(100., 0.)];
        let findings = lint(&d);
        assert!(
            findings.iter().any(|f| f.code == "collinear-overlap"),
            "shared horizontal shaft must trigger collinear-overlap: {findings:?}"
        );

        // Touching only at an endpoint (zero overlap) or crossing perpendicularly must not trigger.
        d.layout.routes.get_mut("c->d").unwrap().points =
            vec![Point::new(40., -20.), Point::new(40., 20.)];
        let findings = lint(&d);
        assert!(findings.iter().any(|f| f.code == "crossing"));
        assert!(!findings.iter().any(|f| f.code == "collinear-overlap"));
    }

    #[test]
    fn detects_interior_micro_jogs_excluding_stubs_and_collinear_splits() {
        let mut d = compile("a -> b\n").unwrap();
        // Interior 6 px jog between two 90-degree bends.
        d.layout.routes.get_mut("a->b").unwrap().points = vec![
            Point::new(0., 0.),
            Point::new(30., 0.),
            Point::new(30., 6.),
            Point::new(80., 6.),
        ];
        assert!(codes_for(&lint(&d), "a->b").contains(&"micro-jog"));

        // Short 6 px stub-adjacent segment with a 30 px interior segment is NOT a micro-jog.
        d.layout.routes.get_mut("a->b").unwrap().points = vec![
            Point::new(0., 0.),
            Point::new(6., 0.),
            Point::new(6., 30.),
            Point::new(80., 30.),
        ];
        assert!(!codes_for(&lint(&d), "a->b").contains(&"micro-jog"));

        // Collinear split across an 8 + 8 = 16 px interior segment is NOT a micro-jog.
        d.layout.routes.get_mut("a->b").unwrap().points = vec![
            Point::new(0., 0.),
            Point::new(30., 0.),
            Point::new(30., 8.),
            Point::new(30., 16.),
            Point::new(80., 16.),
        ];
        assert!(!codes_for(&lint(&d), "a->b").contains(&"micro-jog"));

        // Four raw points that coalesce to a single bend (len 3) have no interior segment.
        d.layout.routes.get_mut("a->b").unwrap().points = vec![
            Point::new(0., 0.),
            Point::new(50., 0.),
            Point::new(100., 0.),
            Point::new(100., 40.),
        ];
        assert!(!codes_for(&lint(&d), "a->b").contains(&"micro-jog"));
    }

    #[test]
    fn detects_excess_detour_for_non_self_loops_above_48_px() {
        let mut d = compile("a -> b\ns: Self\ns -> s\n").unwrap();
        // Direct Manhattan distance = 100 >= 48; route length = 300 > 280 (2.8x).
        d.layout.routes.get_mut("a->b").unwrap().points = vec![
            Point::new(0., 0.),
            Point::new(0., -100.),
            Point::new(100., -100.),
            Point::new(100., 0.),
        ];
        // Self-loop with identical detour geometry must be exempt.
        d.layout.routes.get_mut("s->s").unwrap().points = vec![
            Point::new(200., 0.),
            Point::new(200., -100.),
            Point::new(300., -100.),
            Point::new(300., 0.),
        ];
        let findings = lint(&d);
        assert!(codes_for(&findings, "a->b").contains(&"excess-detour"));
        assert!(!codes_for(&findings, "s->s").contains(&"excess-detour"));

        // Short edge (Manhattan distance = 40 < 48) with >2.8x detour must not trigger.
        d.layout.routes.get_mut("a->b").unwrap().points = vec![
            Point::new(0., 0.),
            Point::new(0., -100.),
            Point::new(40., -100.),
            Point::new(40., 0.),
        ];
        assert!(!codes_for(&lint(&d), "a->b").contains(&"excess-detour"));

        // Exact 2.8x detour (direct = 70, total = 196) must not trigger (> 2.8x strictly).
        d.layout.routes.get_mut("a->b").unwrap().points = vec![
            Point::new(0., 0.),
            Point::new(0., -63.),
            Point::new(70., -63.),
            Point::new(70., 0.),
        ];
        assert!(!codes_for(&lint(&d), "a->b").contains(&"excess-detour"));
    }
}
