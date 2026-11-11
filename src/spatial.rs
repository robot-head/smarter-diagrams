//! Independent stratum planes and stable union layouts for branch comparison.
use crate::{Error, layout::*, model::*, views};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stratum {
    pub id: String,
    pub layout: Layout,
    pub index: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Via {
    pub edge: EdgeId,
    pub source: Point,
    pub target: Point,
    pub source_layer: String,
    pub target_layer: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stack {
    pub planes: Vec<Stratum>,
    pub vias: Vec<Via>,
    pub theta: f64,
}
fn layer(node: &Node, default: &str) -> String {
    node.attrs
        .get("stratum")
        .map(|v| v.text.trim_matches('"').to_string())
        .unwrap_or_else(|| default.into())
}
pub fn strata(scene: &SceneGraph, options: &LayoutOptions, theta: f64) -> Result<Stack, Error> {
    if !theta.is_finite() || !(0. ..=std::f64::consts::FRAC_PI_4).contains(&theta) {
        return Err(Error::Layout("stratum tilt must be in [0, pi/4]".into()));
    }
    let names = if scene.strata.is_empty() {
        vec!["default".into()]
    } else {
        scene.strata.clone()
    };
    let mut ownership = BTreeMap::new();
    for node in &scene.nodes {
        let name = layer(node, &names[0]);
        if !names.contains(&name) {
            return Err(Error::Layout(format!("unknown stratum {name}")));
        }
        ownership.insert(node.id.clone(), name);
    }
    let mut planes = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let keep = ownership
            .iter()
            .filter(|(_, v)| *v == name)
            .map(|(k, _)| k.clone())
            .collect();
        let mut graph = views::filter(scene, &keep);
        for node in &mut graph.nodes {
            if node
                .parent
                .as_ref()
                .is_some_and(|p| graph_ids_missing(scene, &ownership, p, name))
            {
                node.parent = None;
            }
        }
        let layout = crate::layout::layout(&graph, options, None)?;
        planes.push(Stratum {
            id: name.clone(),
            layout,
            index: i,
        });
    }
    let spacing = planes
        .iter()
        .flat_map(|p| p.layout.nodes.values().map(|r| r.h))
        .reduce(f64::max)
        .unwrap_or(80.)
        + 160.;
    for plane in &mut planes {
        let dx = plane.index as f64 * spacing * theta.sin();
        let dy = plane.index as f64 * spacing;
        for r in plane.layout.nodes.values_mut() {
            r.x += dx;
            r.y += dy;
        }
        for p in plane.layout.ports.values_mut() {
            p.point = p.point.offset(dx, dy);
            p.stub = p.stub.offset(dx, dy);
        }
        for route in plane.layout.routes.values_mut() {
            for p in &mut route.points {
                *p = p.offset(dx, dy);
            }
        }
        for label in &mut plane.layout.labels {
            label.bounds.x += dx;
            label.bounds.y += dy;
            label.anchor = label.anchor.offset(dx, dy);
        }
        for points in plane.layout.zones.values_mut() {
            for point in points {
                *point = point.offset(dx, dy);
            }
        }
    }
    let mut vias = Vec::new();
    for e in &scene.edges {
        let a = &ownership[&e.source.node];
        let b = &ownership[&e.target.node];
        if a == b {
            continue;
        }
        let pa = planes.iter().find(|p| p.id == *a).unwrap().layout.nodes[&e.source.node].center();
        let pb = planes.iter().find(|p| p.id == *b).unwrap().layout.nodes[&e.target.node].center();
        vias.push(Via {
            edge: e.id.clone(),
            source: pa,
            target: pb,
            source_layer: a.clone(),
            target_layer: b.clone(),
        });
    }
    Ok(Stack {
        planes,
        vias,
        theta,
    })
}

/// Presentation keeps the core renderer's silhouettes, directions, and labels
/// in both the SVG fallback and the WebGPU fill/text composition.
#[cfg(feature = "wasm-runtime")]
pub(crate) fn presentation(scene: &SceneGraph, stack: &Stack) -> Result<serde_json::Value, Error> {
    let mut layout = Layout::default();
    let mut drawing = scene.clone();
    let mut surfaces = Vec::new();
    for plane in &stack.planes {
        layout.nodes.extend(plane.layout.nodes.clone());
        layout.ports.extend(plane.layout.ports.clone());
        layout.routes.extend(plane.layout.routes.clone());
        layout.labels.extend(plane.layout.labels.clone());
        layout.zones.extend(plane.layout.zones.clone());
        let keep = plane.layout.nodes.keys().cloned().collect();
        surfaces.push((
            plane.id.clone(),
            crate::render::bounds(&views::filter(scene, &keep), &plane.layout),
        ));
    }
    for via in &stack.vias {
        let edge = drawing
            .edges
            .iter_mut()
            .find(|e| e.id == via.edge)
            .ok_or_else(|| Error::Layout("unknown via".into()))?;
        let source = scene.node(&edge.source.node).unwrap();
        let target = scene.node(&edge.target.node).unwrap();
        let sr = layout.nodes[&source.id];
        let tr = layout.nodes[&target.id];
        let a = layout
            .ports
            .get(&crate::layout::port_key(edge, true))
            .map(|p| p.point)
            .unwrap_or_else(|| crate::shapes::ray_attachment(&source.shape, sr, tr.center()));
        let b = layout
            .ports
            .get(&crate::layout::port_key(edge, false))
            .map(|p| p.point)
            .unwrap_or_else(|| crate::shapes::ray_attachment(&target.shape, tr, sr.center()));
        layout.routes.insert(
            edge.id.clone(),
            Route {
                edge: edge.id.clone(),
                points: vec![a, b],
            },
        );
        edge.attrs.insert(
            "style".into(),
            Value {
                text: "dashed".into(),
                ..Default::default()
            },
        );
        if edge.label.is_empty() {
            edge.label = edge
                .attrs
                .get("relation")
                .map(|v| v.text.replace('_', " "))
                .unwrap_or_else(|| "cross-plane".into());
        }
        let w = crate::labels::text_width(&edge.label) + 16.;
        let length = (b.x - a.x).hypot(b.y - a.y).max(1.);
        let (nx, ny) = (-(b.y - a.y) / length, (b.x - a.x) / length);
        let distance = nx.abs() * w / 2. + ny.abs() * 12. + 8.;
        let mut placed = None;
        'positions: for extra in [0., 24., 48., 96., 160.] {
            for t in [0.5, 0.3, 0.7] {
                for side in [-1., 1.] {
                    let anchor = Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
                    let center = anchor.offset(
                        nx * (distance + extra) * side,
                        ny * (distance + extra) * side,
                    );
                    let bounds = Rect::new(center.x, center.y, w, 24.);
                    if scene
                        .nodes
                        .iter()
                        .filter(|n| !n.container)
                        .any(|n| bounds.inflate(2.).intersects(layout.nodes[&n.id]))
                        || layout
                            .labels
                            .iter()
                            .any(|l| bounds.inflate(2.).intersects(l.bounds.inflate(2.)))
                    {
                        continue;
                    }
                    placed = Some(crate::labels::LabelPlacement {
                        feature: edge.id.clone(),
                        text: edge.label.clone(),
                        bounds,
                        anchor,
                        knockout: false,
                        callout: false,
                        horizontal: true,
                        slide: t,
                    });
                    break 'positions;
                }
            }
        }
        if let Some(label) = placed {
            layout.labels.push(label);
        }
    }
    let mut svg = crate::render::svg(&drawing, &layout);
    let mut backdrops = String::new();
    let mut bounds = crate::render::bounds(&drawing, &layout);
    for (name, r) in &surfaces {
        use std::fmt::Write;
        bounds = bounds.union(r.inflate(4.));
        let _ = write!(
            backdrops,
            "<rect data-plane-background=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"12\" fill=\"#edf3fa\" fill-opacity=\"0.45\" stroke=\"#73869b\"/><text x=\"{}\" y=\"{}\" style=\"font-size:12px;font-weight:600\">Stratum: {}</text>",
            crate::render::escape(name),
            r.left(),
            r.top(),
            r.w,
            r.h,
            r.left() + 8.,
            r.top() + 16.,
            crate::render::escape(name)
        );
    }
    svg = svg.replacen("</style>", &format!("</style>{backdrops}"), 1);
    let rest = &svg[svg.find('>').unwrap() + 1..];
    svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" width=\"{}\" height=\"{}\" role=\"img\" aria-labelledby=\"diagram-title diagram-description\">{rest}",
        bounds.left(),
        bounds.top(),
        bounds.w,
        bounds.h,
        bounds.w,
        bounds.h
    );
    let mut result = serde_json::to_value(stack).map_err(|e| Error::Layout(e.to_string()))?;
    result["svg"] = serde_json::json!(svg);
    result["description"] = serde_json::json!(crate::render::description(&drawing));
    Ok(result)
}
fn graph_ids_missing(
    _scene: &SceneGraph,
    ownership: &BTreeMap<String, String>,
    id: &str,
    name: &str,
) -> bool {
    ownership.get(id).is_none_or(|s| s != name)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Change {
    Unchanged,
    Added,
    Removed,
    Modified,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpatialDiff {
    pub base_scene: SceneGraph,
    pub base_layout: Layout,
    pub union: SceneGraph,
    pub layout: Layout,
    pub changes: BTreeMap<NodeId, Change>,
    pub edge_changes: BTreeMap<EdgeId, Change>,
}
pub fn compare(
    base: &SceneGraph,
    proposal: &SceneGraph,
    base_layout: &Layout,
    options: &LayoutOptions,
) -> Result<SpatialDiff, Error> {
    let mut union = proposal.clone();
    let mut changes = BTreeMap::new();
    for n in &base.nodes {
        if let Some(head) = proposal.node(&n.id) {
            let equal = n.label == head.label
                && n.shape == head.shape
                && n.parent == head.parent
                && attrs_equal(&n.attrs, &head.attrs)
                && annotations_equal(&n.annotations, &head.annotations)
                && n.ports.len() == head.ports.len()
                && n.ports.iter().all(|(id, p)| {
                    head.ports
                        .get(id)
                        .is_some_and(|q| p.label == q.label && p.constraint == q.constraint)
                });
            changes.insert(
                n.id.clone(),
                if equal {
                    Change::Unchanged
                } else {
                    Change::Modified
                },
            );
        } else {
            changes.insert(n.id.clone(), Change::Removed);
            union.nodes.push(n.clone());
        }
    }
    for n in &proposal.nodes {
        changes.entry(n.id.clone()).or_insert(Change::Added);
    }
    let mut edge_changes = BTreeMap::new();
    for e in &base.edges {
        if let Some(head) = proposal.edge(&e.id) {
            edge_changes.insert(
                e.id.clone(),
                if e.label == head.label {
                    Change::Unchanged
                } else {
                    Change::Modified
                },
            );
        } else {
            edge_changes.insert(e.id.clone(), Change::Removed);
            union.edges.push(e.clone());
        }
    }
    for e in &proposal.edges {
        edge_changes.entry(e.id.clone()).or_insert(Change::Added);
    }
    // Lock unchanged infrastructure to the base coordinate system in the joint union solve.
    for n in &mut union.nodes {
        if changes[&n.id] == Change::Unchanged
            && let Some(r) = base_layout.nodes.get(&n.id)
        {
            n.annotations.retain(|a| a.name != "pin");
            n.annotations.push(Annotation {
                name: "pin".into(),
                attrs: BTreeMap::from([
                    (
                        "x".into(),
                        Value {
                            text: format!("{}g", r.x / options.grid),
                            ..Default::default()
                        },
                    ),
                    (
                        "y".into(),
                        Value {
                            text: format!("{}g", r.y / options.grid),
                            ..Default::default()
                        },
                    ),
                ]),
                ..Default::default()
            });
        }
    }
    let layout = crate::layout::layout(&union, options, Some(base_layout))?;
    Ok(SpatialDiff {
        base_scene: base.clone(),
        base_layout: base_layout.clone(),
        union,
        layout,
        changes,
        edge_changes,
    })
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MorphFrame {
    pub nodes: BTreeMap<NodeId, Rect>,
    pub opacity: BTreeMap<NodeId, f64>,
    pub metrics: BTreeMap<String, f64>,
}
pub fn morph(
    diff: &SpatialDiff,
    tau: f64,
    base_metrics: &BTreeMap<String, f64>,
    proposal_metrics: &BTreeMap<String, f64>,
) -> Result<MorphFrame, Error> {
    if !tau.is_finite() || !(0. ..=1.).contains(&tau) {
        return Err(Error::Edit("branch morph tau must be in [0,1]".into()));
    }
    let opacity = diff
        .changes
        .iter()
        .map(|(id, c)| {
            (
                id.clone(),
                match c {
                    Change::Added => tau,
                    Change::Removed => 1. - tau,
                    _ => 1.,
                },
            )
        })
        .collect();
    let keys: BTreeSet<_> = base_metrics
        .keys()
        .chain(proposal_metrics.keys())
        .cloned()
        .collect();
    let metrics = keys
        .into_iter()
        .map(|k| {
            let a = base_metrics.get(&k).copied().unwrap_or(0.);
            let b = proposal_metrics.get(&k).copied().unwrap_or(0.);
            (k, a + (b - a) * tau)
        })
        .collect();
    Ok(MorphFrame {
        nodes: diff
            .layout
            .nodes
            .iter()
            .map(|(id, to)| {
                let from = diff.base_layout.nodes.get(id).copied().unwrap_or(*to);
                (
                    id.clone(),
                    Rect::new(
                        from.x + (to.x - from.x) * tau,
                        from.y + (to.y - from.y) * tau,
                        from.w + (to.w - from.w) * tau,
                        from.h + (to.h - from.h) * tau,
                    ),
                )
            })
            .collect(),
        opacity,
        metrics,
    })
}
pub fn diff_svg(diff: &SpatialDiff, tau: f64) -> Result<String, Error> {
    let frame = morph(diff, tau, &BTreeMap::new(), &BTreeMap::new())?;
    let mut scene = diff.union.clone();
    let mut layout = diff.layout.clone();
    layout.nodes = frame.nodes.clone();
    if tau < 0.5 {
        for n in &mut scene.nodes {
            if let Some(old) = diff.base_scene.node(&n.id) {
                n.label = old.label.clone();
                n.shape = old.shape.clone();
            }
        }
    }
    let interpolate =
        |a: Point, b: Point| Point::new(a.x + (b.x - a.x) * tau, a.y + (b.y - a.y) * tau);
    for (key, p) in &mut layout.ports {
        if let Some(old) = diff.base_layout.ports.get(key) {
            p.point = interpolate(old.point, p.point);
            p.stub = interpolate(old.stub, p.stub);
        }
    }
    for (id, route) in &mut layout.routes {
        if let Some(old) = diff.base_layout.routes.get(id) {
            if tau == 0. {
                route.points = old.points.clone();
            } else if tau < 1. {
                let count = old.points.len().max(route.points.len());
                let target = route.points.clone();
                route.points = (0..count)
                    .map(|i| {
                        let t = i as f64 / (count - 1) as f64;
                        interpolate(
                            crate::render::at_arc(&old.points, t),
                            crate::render::at_arc(&target, t),
                        )
                    })
                    .collect();
            }
        }
    }
    for label in &mut layout.labels {
        if let Some(old) = diff
            .base_layout
            .labels
            .iter()
            .find(|p| p.feature == label.feature)
        {
            let a = old.bounds;
            let b = label.bounds;
            label.bounds = Rect::new(
                a.x + (b.x - a.x) * tau,
                a.y + (b.y - a.y) * tau,
                a.w + (b.w - a.w) * tau,
                a.h + (b.h - a.h) * tau,
            );
            if tau < 0.5 {
                label.text = old.text.clone();
            }
        }
    }
    let mut svg = crate::render::svg(&scene, &layout);
    for (id, change) in &diff.changes {
        let opacity = frame.opacity[id];
        if *change != Change::Unchanged {
            svg = svg.replace(
                &format!("<g data-node=\"{}\"", crate::render::escape(id)),
                &format!(
                    "<g opacity=\"{opacity}\" data-node=\"{}\"",
                    crate::render::escape(id)
                ),
            );
        }
    }
    for (id, change) in &diff.edge_changes {
        let opacity = match change {
            Change::Added => tau,
            Change::Removed => 1. - tau,
            _ => 1.,
        };
        for tag in ["data-edge", "data-label"] {
            svg = svg.replace(
                &format!("{tag}=\"{}\"", crate::render::escape(id)),
                &format!(
                    "opacity=\"{opacity}\" {tag}=\"{}\"",
                    crate::render::escape(id)
                ),
            );
        }
    }

    for key in layout.ports.keys() {
        let opacity = if let Some((node, _)) = key.rsplit_once('#') {
            frame.opacity.get(node).copied().unwrap_or(1.)
        } else {
            let edge = key
                .strip_suffix(":source")
                .or_else(|| key.strip_suffix(":target"))
                .unwrap_or(key);
            match diff.edge_changes.get(edge) {
                Some(Change::Added) => tau,
                Some(Change::Removed) => 1. - tau,
                _ => 1.,
            }
        };
        for tag in ["data-port", "data-label"] {
            svg = svg.replace(
                &format!("{tag}=\"{}\"", crate::render::escape(key)),
                &format!(
                    "opacity=\"{opacity}\" {tag}=\"{}\"",
                    crate::render::escape(key)
                ),
            );
        }
    }
    svg=svg.replacen("</defs>","<pattern id=\"removed-hatch\" width=\"8\" height=\"8\" patternUnits=\"userSpaceOnUse\"><path d=\"M 0 8 L 8 0\" stroke=\"#c58b22\" stroke-width=\"1\"/></pattern></defs>",1);
    let mut overlay = String::new();
    use std::fmt::Write;
    for (id, status) in &diff.changes {
        if *status == Change::Unchanged {
            continue;
        }
        let r = frame.nodes[id];
        let color = match status {
            Change::Added => "#29966f",
            Change::Removed => "#c58b22",
            _ => "#6276bd",
        };
        if *status == Change::Removed {
            let _ = write!(
                overlay,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"url(#removed-hatch)\" opacity=\"{}\"/>",
                r.left(),
                r.top(),
                r.w,
                r.h,
                frame.opacity[id]
            );
        }
        let _ = write!(
            overlay,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"8\" fill=\"{color}\" fill-opacity=\"{}\" stroke=\"{color}\" stroke-dasharray=\"{}\"><title>{:?}: {}</title></rect>",
            r.left() - 4.,
            r.top() - 4.,
            r.w + 8.,
            r.h + 8.,
            0.12 * frame.opacity[id],
            if *status == Change::Removed {
                "4 3"
            } else {
                "none"
            },
            status,
            crate::render::escape(id)
        );
    }
    svg = svg.replace("</svg>", &format!("{overlay}</svg>"));
    Ok(svg)
}

fn attrs_equal(a: &Attributes, b: &Attributes) -> bool {
    a.len() == b.len()
        && a.iter()
            .all(|(k, v)| b.get(k).is_some_and(|w| v.text == w.text))
}
fn annotations_equal(a: &[Annotation], b: &[Annotation]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(a, b)| a.name == b.name && attrs_equal(&a.attrs, &b.attrs))
}

#[cfg(all(test, feature = "wasm-runtime"))]
mod presentation_tests {
    #[test]
    fn stack_presentation_preserves_directions_labels_and_plane_names() {
        let d=crate::compile("strata [app, hardware]\napi [stratum: app]\nworker [stratum: app]\nhost [stratum: hardware]\napi -> worker: RPC\napi -> host [relation: runs_on]\nworker -> host [relation: runs_on]\n").unwrap();
        let stack = super::strata(&d.scene, &Default::default(), 0.4).unwrap();
        let result = super::presentation(&d.scene, &stack).unwrap();
        let svg = result["svg"].as_str().unwrap();
        assert_eq!(svg.matches("data-arrowhead=\"end\"").count(), 3);
        assert!(svg.contains("RPC") && svg.contains("runs on"));
        assert!(svg.contains("Stratum: app") && svg.contains("Stratum: hardware"));
        assert!(svg.contains("stroke-dasharray=\"6 4\""));
    }
}
