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
        let mut graph = views::filter_exact(scene, &keep);
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

/// A visibility path keeps via ink clear of headings and unrelated node bodies.
#[cfg(feature = "wasm-runtime")]
fn heading_route(a: Point, b: Point, headings: &[Rect]) -> Result<Vec<Point>, Error> {
    let clear = |a, b| {
        !headings
            .iter()
            .any(|r| crate::labels::segment_hits(a, b, *r))
    };
    if clear(a, b) {
        return Ok(vec![a, b]);
    }
    let mut points = vec![a, b];
    for r in headings {
        points.extend([
            Point::new(r.left(), r.top()),
            Point::new(r.right(), r.top()),
            Point::new(r.right(), r.bottom()),
            Point::new(r.left(), r.bottom()),
        ]);
    }
    let mut distance = vec![f64::INFINITY; points.len()];
    let mut previous = vec![None; points.len()];
    let mut visited = vec![false; points.len()];
    distance[0] = 0.;
    for _ in 0..points.len() {
        let Some(i) = (0..points.len())
            .filter(|&i| !visited[i])
            .min_by(|&i, &j| distance[i].total_cmp(&distance[j]))
        else {
            break;
        };
        if !distance[i].is_finite() {
            break;
        }
        if i == 1 {
            break;
        }
        visited[i] = true;
        for j in 0..points.len() {
            if !visited[j] && clear(points[i], points[j]) {
                let next = distance[i] + points[i].distance(points[j]);
                if next < distance[j] {
                    distance[j] = next;
                    previous[j] = Some(i);
                }
            }
        }
    }
    if !distance[1].is_finite() {
        return Err(Error::Layout("no via path outside stratum headings".into()));
    }
    let mut route = vec![b];
    let mut i = 1;
    while let Some(parent) = previous[i] {
        route.push(points[parent]);
        i = parent;
    }
    route.reverse();
    Ok(route)
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
        let mut frame = crate::render::bounds(&views::filter_exact(scene, &keep), &plane.layout);
        let width = crate::labels::text_width(&format!("Stratum: {}", plane.id)) * 12. / 13. + 16.;
        if frame.w < width + 16. {
            frame = Rect::new(
                frame.left() + (width + 16.) / 2.,
                frame.y,
                width + 16.,
                frame.h,
            );
        }
        let heading = Rect::new(
            frame.left() + 8. + width / 2.,
            frame.top() + 12.,
            width,
            24.,
        );
        surfaces.push((plane.id.clone(), frame, heading));
    }
    // Padding also keeps arrowhead bases clear of the text, not just shafts.
    let headings: Vec<_> = surfaces
        .iter()
        .map(|(_, _, h)| Rect::new(h.x, h.y, h.w + 32., h.h + 14.))
        .collect();
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
        let fixed_a = layout.ports.get(&crate::layout::port_key(edge, true));
        let fixed_b = layout.ports.get(&crate::layout::port_key(edge, false));
        let mut a = fixed_a
            .map(|p| p.point)
            .unwrap_or_else(|| crate::shapes::ray_attachment(&source.shape, sr, tr.center()));
        let mut b = fixed_b
            .map(|p| p.point)
            .unwrap_or_else(|| crate::shapes::ray_attachment(&target.shape, tr, sr.center()));
        let mut obstacles = headings.clone();
        obstacles.extend(
            scene
                .nodes
                .iter()
                .filter(|n| !n.container && n.id != source.id && n.id != target.id)
                .map(|n| layout.nodes[&n.id].inflate(8.)),
        );
        let mut points = heading_route(a, b, &obstacles)?;
        for _ in 0..2 {
            if fixed_a.is_none() {
                a = crate::shapes::ray_attachment(&source.shape, sr, points[1]);
            }
            if fixed_b.is_none() {
                b = crate::shapes::ray_attachment(&target.shape, tr, points[points.len() - 2]);
            }
            points = heading_route(a, b, &obstacles)?;
        }
        layout.routes.insert(
            edge.id.clone(),
            Route {
                edge: edge.id.clone(),
                points,
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
    }
    // Every via route is known before assigning any captions.
    for via in &stack.vias {
        let edge = drawing.edge(&via.edge).unwrap();
        let points = &layout.routes[&edge.id].points;
        let w = crate::labels::text_width(&edge.label) + 16.;
        let mut placed = None;
        'positions: for extra in [0., 24., 48., 96., 160.] {
            for t in [0.5_f64, 0.3, 0.7] {
                let anchor = crate::render::at_arc(points, t);
                let before = crate::render::at_arc(points, t - 0.01);
                let after = crate::render::at_arc(points, t + 0.01);
                let length = before.distance(after).max(1.);
                let (nx, ny) = (
                    -(after.y - before.y) / length,
                    (after.x - before.x) / length,
                );
                let distance = nx.abs() * w / 2. + ny.abs() * 12. + 8.;
                for side in [-1., 1.] {
                    let center = anchor.offset(
                        nx * (distance + extra) * side,
                        ny * (distance + extra) * side,
                    );
                    let bounds = Rect::new(center.x, center.y, w, 24.);
                    let padded = bounds.inflate(2.);
                    if scene
                        .nodes
                        .iter()
                        .filter(|n| !n.container)
                        .any(|n| padded.intersects(layout.nodes[&n.id]))
                        || layout
                            .labels
                            .iter()
                            .any(|l| padded.intersects(l.bounds.inflate(2.)))
                        || headings.iter().any(|h| padded.intersects(*h))
                        || surfaces.iter().any(|(_, r, _)| {
                            let corners = [
                                Point::new(r.left(), r.top()),
                                Point::new(r.right(), r.top()),
                                Point::new(r.right(), r.bottom()),
                                Point::new(r.left(), r.bottom()),
                                Point::new(r.left(), r.top()),
                            ];
                            corners
                                .windows(2)
                                .any(|p| crate::labels::segment_hits(p[0], p[1], padded))
                        })
                        || layout.routes.values().any(|r| {
                            r.points
                                .windows(2)
                                .any(|p| crate::labels::segment_hits(p[0], p[1], padded))
                        })
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
                        orthogonal_leader: false,
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
    for (name, r, heading) in &surfaces {
        use std::fmt::Write;
        bounds = bounds.union(r.inflate(4.));
        let _ = write!(
            backdrops,
            "<rect data-plane-background=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"12\" fill=\"var(--sd-plane,#edf3fa)\" fill-opacity=\"0.45\" stroke=\"var(--sd-muted,#73869b)\"/><text data-plane-label=\"{}\" x=\"{}\" y=\"{}\" style=\"font-size:12px;font-weight:600\">Stratum: {}</text>",
            crate::render::escape(name),
            r.left(),
            r.top(),
            r.w,
            r.h,
            crate::render::escape(name),
            heading.left(),
            r.top() + 16.,
            crate::render::escape(name)
        );
    }
    svg = svg.replacen("</style>", &format!("</style>{backdrops}"), 1);
    let accessible = svg
        .split("aria-labelledby=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .to_owned();
    let rest = &svg[svg.find('>').unwrap() + 1..];
    svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" width=\"{}\" height=\"{}\" role=\"img\" aria-labelledby=\"{accessible}\">{rest}",
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
                *n = old.clone();
            }
        }
        for e in &mut scene.edges {
            if let Some(old) = diff.base_scene.edge(&e.id) {
                *e = old.clone();
            }
        }
        // Record titles and named ports can exist in only one snapshot. They
        // must follow the chosen shape instead of leaking out of the union.
        layout.labels.retain(|label| {
            scene.node(&label.feature).is_none()
                || diff.changes.get(&label.feature) == Some(&Change::Added)
        });
        for label in &diff.base_layout.labels {
            if scene.node(&label.feature).is_some() {
                let mut label = label.clone();
                let from = diff.base_layout.nodes[&label.feature];
                let to = layout.nodes[&label.feature];
                label.bounds.x += to.x - from.x;
                label.bounds.y += to.y - from.y;
                layout.labels.push(label);
            }
        }
        layout.ports.retain(|key, _| {
            key.rsplit_once('#')
                .is_none_or(|(node, _)| diff.changes.get(node) == Some(&Change::Added))
        });
        for (key, port) in &diff.base_layout.ports {
            if let Some((node, _)) = key.rsplit_once('#') {
                let mut port = port.clone();
                let from = diff.base_layout.nodes[node];
                let to = layout.nodes[node];
                let horizontal = matches!(port.side, Side::E | Side::W);
                let offset = if horizontal {
                    (port.point.y - from.top()) / from.h * to.h
                } else {
                    (port.point.x - from.left()) / from.w * to.w
                };
                port.point = crate::shapes::attachment(
                    &scene.node(node).unwrap().shape,
                    to,
                    port.side,
                    offset,
                );
                layout.ports.insert(key.clone(), port);
            }
        }
    }
    let interpolate =
        |a: Point, b: Point| Point::new(a.x + (b.x - a.x) * tau, a.y + (b.y - a.y) * tau);
    for (key, p) in &mut layout.ports {
        if tau < 0.5 && key.contains('#') {
            continue;
        }
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
        if tau < 0.5 && scene.node(&label.feature).is_some() {
            continue;
        }
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
    // Interpolated rectangles and discrete shape changes still need exact
    // attachment points. Retarget modified endpoints to the current outline.
    for edge in &scene.edges {
        for (source, endpoint) in [(true, &edge.source), (false, &edge.target)] {
            if diff.changes.get(&endpoint.node) != Some(&Change::Modified) {
                continue;
            }
            let key = crate::layout::port_key(edge, source);
            if let Some(port) = layout.ports.get_mut(&key) {
                let node = scene.node(&endpoint.node).unwrap();
                port.point =
                    crate::shapes::ray_attachment(&node.shape, layout.nodes[&node.id], port.point);
                if let Some(route) = layout.routes.get_mut(&edge.id) {
                    let index = if source { 0 } else { route.points.len() - 1 };
                    route.points[index] = port.point;
                }
            }
        }
    }
    let mut svg = crate::render::svg_with_decorations(
        &scene,
        &layout,
        |id| match diff.edge_changes.get(id) {
            Some(Change::Added) => tau > 0.,
            Some(Change::Removed) => tau < 1.,
            _ => true,
        },
        |node, rect| {
            let Some(change) = diff.changes.get(&node.id) else {
                return String::new();
            };
            let color = match change {
                Change::Unchanged => return String::new(),
                Change::Added => "var(--sd-added,#29966f)",
                Change::Removed => "var(--sd-removed,#c58b22)",
                Change::Modified => "var(--sd-modified,#6276bd)",
            };
            let outline = crate::shapes::outline(&node.shape, rect);
            let dash = if *change == Change::Removed {
                " stroke-dasharray=\"4 3\""
            } else {
                ""
            };
            let hatch = if *change == Change::Removed {
                format!(
                    "<path data-change-hatch=\"true\" d=\"{outline}\" fill=\"url(#removed-hatch)\"/>"
                )
            } else {
                String::new()
            };
            let mark = match change {
                Change::Added => "+",
                Change::Removed => "−",
                Change::Modified => "~",
                _ => "",
            };
            let badge = format!(
                "<g data-change-badge=\"true\"><circle cx=\"{}\" cy=\"{}\" r=\"9\" fill=\"var(--sd-bg,#ffffff)\" stroke=\"{color}\"/><text x=\"{}\" y=\"{}\" text-anchor=\"middle\" dominant-baseline=\"central\" style=\"font-weight:700;font-size:13px;fill:var(--sd-fg,#233347)\">{mark}</text></g>",
                rect.right() + 8.,
                rect.top() - 8.,
                rect.right() + 8.,
                rect.top() - 8.
            );
            format!(
                "<g data-change=\"{}\" data-change-kind=\"{change:?}\"><title>{change:?}: {}</title><path data-change-outline=\"true\" d=\"{outline}\" fill=\"{color}\" fill-opacity=\"0.12\" stroke=\"{color}\" stroke-width=\"2.5\"{dash}/>{hatch}{badge}</g>",
                crate::render::escape(&node.id),
                crate::render::escape(&node.label)
            )
        },
    );
    if diff
        .changes
        .values()
        .any(|change| *change == Change::Removed)
    {
        svg = svg.replacen("</style>", "</style><style>.removed-node text{paint-order:stroke;stroke:var(--sd-bg,#fff7e6);stroke-width:3;stroke-linejoin:round}</style><defs><pattern id=\"removed-hatch\" width=\"8\" height=\"8\" patternUnits=\"userSpaceOnUse\"><path d=\"M 0 8 L 8 0\" stroke=\"var(--sd-removed,#c58b22)\" stroke-width=\"1\"/></pattern></defs>", 1);
    }
    for (id, change) in &diff.changes {
        let opacity = frame.opacity[id];
        if *change != Change::Unchanged {
            for tag in ["data-label", "data-owner-node"] {
                svg = svg.replace(
                    &format!("{tag}=\"{}\"", crate::render::escape(id)),
                    &format!(
                        "opacity=\"{opacity}\" {tag}=\"{}\"",
                        crate::render::escape(id)
                    ),
                );
            }
            let class = if *change == Change::Removed {
                " class=\"removed-node\""
            } else {
                ""
            };
            svg = svg.replace(
                &format!("<g data-node=\"{}\"", crate::render::escape(id)),
                &format!(
                    "<g opacity=\"{opacity}\"{class} data-node=\"{}\"",
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
    if tau == 0. || tau == 1. {
        let keep = frame
            .opacity
            .iter()
            .filter(|(_, opacity)| **opacity > 0.)
            .map(|(id, _)| id.clone())
            .collect();
        let mut visible = views::filter_exact(&scene, &keep);
        visible
            .edges
            .retain(|edge| match diff.edge_changes.get(&edge.id) {
                Some(Change::Added) => tau > 0.,
                Some(Change::Removed) => tau < 1.,
                _ => true,
            });
        if let (Some(start), Some(end)) = (svg.find("<desc id=\""), svg.find("</desc>")) {
            let start = start + svg[start..].find('>').unwrap() + 1;
            svg.replace_range(
                start..end,
                &crate::render::escape(&crate::render::description(&visible)),
            );
        }
    }
    let bounds = crate::render::bounds(&scene, &layout);
    let legend = format!(
        "<g data-diff-legend=\"true\" aria-label=\"Changes: added, removed, modified\"><text x=\"{}\" y=\"{}\" style=\"font-size:13px\">+ Added · − Removed · ~ Modified</text></g>",
        bounds.left() + 12.,
        bounds.bottom() + 20.
    );
    svg = svg.replace("</svg>", &format!("{legend}</svg>"));
    let root_end = svg.find('>').unwrap();
    let rest = &svg[root_end + 1..];
    let accessible = svg
        .split("aria-labelledby=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" width=\"{}\" height=\"{}\" role=\"img\" aria-labelledby=\"{accessible}\">{rest}",
        bounds.left(),
        bounds.top(),
        bounds.w,
        bounds.h + 36.,
        bounds.w,
        bounds.h + 36.
    );
    Ok(crate::render::namespace_svg(svg))
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
