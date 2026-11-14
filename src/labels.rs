//! Spatially indexed label candidates, bounded exact conflict selection and safe SDF polish.
use crate::{
    Error,
    layout::*,
    model::*,
    solver::{self, Dual},
};
use rstar::{AABB, RTree, RTreeObject};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LabelPlacement {
    pub feature: String,
    pub text: String,
    pub bounds: Rect,
    pub anchor: Point,
    pub knockout: bool,
    pub callout: bool,
    /// Use an orthogonal leader for a retained interactive caption.
    #[serde(default)]
    pub orthogonal_leader: bool,
    pub horizontal: bool,
    pub slide: f64,
}
#[derive(Clone)]
struct Candidate {
    label: LabelPlacement,
    cost: f64,
}
#[derive(Clone)]
struct Indexed {
    id: usize,
    bounds: Rect,
}
impl RTreeObject for Indexed {
    type Envelope = AABB<[f64; 2]>;
    fn envelope(&self) -> Self::Envelope {
        envelope(self.bounds)
    }
}
fn envelope(r: Rect) -> AABB<[f64; 2]> {
    AABB::from_corners([r.left(), r.top()], [r.right(), r.bottom()])
}
fn clearance(label: &LabelPlacement) -> Rect {
    label.bounds.inflate(if label.knockout && !label.callout {
        3.
    } else {
        2.
    })
}
fn box_to_point_dist(b: Rect, p: Point) -> f64 {
    let dx = (b.left() - p.x).max(p.x - b.right()).max(0.);
    let dy = (b.top() - p.y).max(p.y - b.bottom()).max(0.);
    dx.hypot(dy)
}
/// Approximate painted title box of a leaf node (centred in its silhouette).
fn node_title_rect(node: &Node, r: Rect) -> Rect {
    let title = node_title(node);
    Rect::new(
        r.x + crate::shapes::title_gutter(&node.shape) / 2.,
        r.y,
        (text_width(&title) * 16. / 15.).min(r.w),
        text_height(&title).min(r.h),
    )
}
fn straight_leader(label: &LabelPlacement) -> Option<(Point, Point)> {
    if !label.callout {
        return None;
    }
    let r = label.bounds.inflate(2.);
    let dx = label.anchor.x - r.x;
    let dy = label.anchor.y - r.y;
    let t = (r.w / 2. / dx.abs().max(1e-9))
        .min(r.h / 2. / dy.abs().max(1e-9))
        .min(1.);
    Some((label.anchor, Point::new(r.x + dx * t, r.y + dy * t)))
}
/// The leader's endpoint span. Use [`leader_path`] for its rendered segments.
pub fn leader(label: &LabelPlacement) -> Option<(Point, Point)> {
    let path = leader_path(label);
    Some((*path.first()?, *path.last()?))
}
/// Actual leader vertices; collision checks and SVG emission share this path.
pub fn leader_path(label: &LabelPlacement) -> Vec<Point> {
    let Some((a, b)) = straight_leader(label) else {
        return Vec::new();
    };
    if !label.orthogonal_leader {
        return vec![a, b];
    }
    let r = label.bounds.inflate(2.);
    let a = label.anchor;
    if r.contains(a) {
        return Vec::new();
    }
    // Strict interior tests: an anchor lying exactly on an inflated face must
    // never route its bend through the caption body.
    let in_x = a.x > r.left() + 1e-3 && a.x < r.right() - 1e-3;
    let in_y = a.y > r.top() + 1e-3 && a.y < r.bottom() - 1e-3;
    let face_y = if a.y < r.y { r.top() } else { r.bottom() };
    let face_x = if a.x < r.x { r.left() } else { r.right() };
    let mut options: Vec<Vec<Point>> = Vec::new();
    if in_x && !in_y {
        // Clean vertical stem onto the facing horizontal face.
        let x = if r.w > 8. {
            a.x.clamp(r.left() + 4., r.right() - 4.)
        } else {
            r.x
        };
        options.push(vec![a, Point::new(x, face_y)]);
    } else if in_y && !in_x {
        // Clean horizontal stem onto the facing vertical face.
        let y = if r.h > 8. {
            a.y.clamp(r.top() + 4., r.bottom() - 4.)
        } else {
            r.y
        };
        options.push(vec![a, Point::new(face_x, y)]);
    } else {
        // Diagonal corner: an L whose bend stays strictly outside the caption
        // and whose end attaches to the face looking back at the anchor.
        let vertical_first = vec![a, Point::new(r.x, a.y), Point::new(r.x, face_y)];
        let horizontal_first = vec![a, Point::new(a.x, r.y), Point::new(face_x, r.y)];
        if label.horizontal {
            options.push(horizontal_first);
            options.push(vertical_first);
        } else {
            options.push(vertical_first);
            options.push(horizontal_first);
        }
    }
    let body = label.bounds.inflate(-1.);
    for mut path in options {
        path.dedup_by(|a, b| a.distance(*b) < 1e-7);
        if path.len() < 2 {
            return path;
        }
        let clips = |p: Point, q: Point| p.distance(q) > 1e-9 && segment_hits(p, q, body);
        let bend_outside = path[1..path.len() - 1].iter().all(|p| !r.contains(*p));
        if bend_outside
            && !path.windows(2).any(|s| clips(s[0], s[1]))
            && !clips(path[0], path[path.len() - 1])
        {
            return path;
        }
    }
    // The straight leader ends on the inflated boundary along the ray to the
    // caption centre, so it never enters the caption body.
    vec![a, b]
}
/// Every segment that may paint or be measured for a callout: the rendered
/// leader path plus its first-to-last chord, which browser checks also test.
pub(crate) fn leader_segments(label: &LabelPlacement) -> Vec<(Point, Point)> {
    let path = leader_path(label);
    let mut out: Vec<(Point, Point)> = path.windows(2).map(|p| (p[0], p[1])).collect();
    if path.len() > 2 {
        out.push((path[0], path[path.len() - 1]));
    }
    out
}
pub(crate) fn segment_hits(a: Point, b: Point, r: Rect) -> bool {
    let mut lo: f64 = 0.;
    let mut hi: f64 = 1.;
    for (start, delta, min, max) in [
        (a.x, b.x - a.x, r.left() + 1e-6, r.right() - 1e-6),
        (a.y, b.y - a.y, r.top() + 1e-6, r.bottom() - 1e-6),
    ] {
        if delta.abs() < 1e-9 {
            if start < min || start > max {
                return false;
            }
        } else {
            let x = (min - start) / delta;
            let y = (max - start) / delta;
            lo = lo.max(x.min(y));
            hi = hi.min(x.max(y));
        }
    }
    lo <= hi
}
fn segments_intersect(a: Point, b: Point, c: Point, d: Point) -> bool {
    let cross =
        |o: Point, u: Point, v: Point| (u.x - o.x) * (v.y - o.y) - (u.y - o.y) * (v.x - o.x);
    if a.x.max(b.x) + 1e-6 < c.x.min(d.x)
        || c.x.max(d.x) + 1e-6 < a.x.min(b.x)
        || a.y.max(b.y) + 1e-6 < c.y.min(d.y)
        || c.y.max(d.y) + 1e-6 < a.y.min(b.y)
    {
        return false;
    }
    let d1 = cross(a, b, c);
    let d2 = cross(a, b, d);
    let d3 = cross(c, d, a);
    let d4 = cross(c, d, b);
    ((d1 <= 1e-6 && d2 >= -1e-6) || (d1 >= -1e-6 && d2 <= 1e-6))
        && ((d3 <= 1e-6 && d4 >= -1e-6) || (d3 >= -1e-6 && d4 <= 1e-6))
}
fn footprint(label: &LabelPlacement) -> Rect {
    let r = clearance(label);
    leader_path(label)
        .into_iter()
        .fold(r, |r, p| r.union(Rect::new(p.x, p.y, 2., 2.)))
}
fn conflict(a: &LabelPlacement, b: &LabelPlacement) -> bool {
    if clearance(a).intersects(clearance(b)) {
        return true;
    }
    if !a.callout && !b.callout {
        return false;
    }
    let sa = leader_segments(a);
    let sb = leader_segments(b);
    if sa.iter().any(|&(p, q)| segment_hits(p, q, clearance(b)))
        || sb.iter().any(|&(p, q)| segment_hits(p, q, clearance(a)))
    {
        return true;
    }
    // Painted leaders must not cross one another; chords are only measured
    // against caption text, so compare the rendered paths here.
    let pa = leader_path(a);
    let pb = leader_path(b);
    a.anchor.distance(b.anchor) > 1e-3
        && pa.windows(2).any(|p| {
            pb.windows(2)
                .any(|q| segments_intersect(p[0], p[1], q[0], q[1]))
        })
}
/// Keep a valid gesture caption instead of searching for a new preferred slot.
pub(crate) fn placements_clear(scene: &SceneGraph, layout: &Layout) -> bool {
    for (i, label) in layout.labels.iter().enumerate() {
        if layout.labels[i + 1..]
            .iter()
            .any(|other| conflict(label, other))
        {
            return false;
        }
        let (ignore, crossed) = feature_scopes(scene, &label.feature);
        let bounds = clearance(label);
        if scene.nodes.iter().any(|node| {
            let rect = layout.nodes[&node.id];
            if ignore.contains(&node.id) {
                node.id != label.feature
                    && crate::layout::container_header_rect_with_ports(node, rect, &layout.ports)
                        .is_some_and(|h| bounds.intersects(h))
            } else if crossed.contains(&node.id) {
                let opaque_flow_pill = !label.callout
                    && scene
                        .edge(&label.feature)
                        .is_some_and(|e| crate::render::flow_caption(scene, e).is_some());
                let clear_border = opaque_flow_pill
                    || !bounds.intersects(rect)
                    || rect.inflate(-6.).encloses(bounds);
                let hits_header =
                    crate::layout::container_header_rect_with_ports(node, rect, &layout.ports)
                        .is_some_and(|h| bounds.intersects(h));
                !clear_border || hits_header
            } else {
                bounds.intersects(rect)
            }
        }) {
            return false;
        }
        if layout.routes.iter().any(|(id, route)| {
            !(label.knockout && !label.callout && id == &label.feature)
                && route
                    .points
                    .windows(2)
                    .any(|p| segment_hits(p[0], p[1], bounds))
        }) {
            return false;
        }
    }
    true
}
fn feature_scopes(scene: &SceneGraph, feature: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    if let Some(edge) = scene.edge(feature) {
        let src_anc = ancestors(scene, &edge.source.node);
        let dst_anc = ancestors(scene, &edge.target.node);
        let mut common = src_anc.clone();
        common.retain(|id| dst_anc.contains(id));
        let mut crossed = src_anc;
        crossed.extend(dst_anc);
        crossed.retain(|id| {
            !common.contains(id) && id != &edge.source.node && id != &edge.target.node
        });
        (common, crossed)
    } else {
        let node = feature
            .rsplit_once('#')
            .map(|(node, _)| node)
            .unwrap_or(feature);
        (ancestors(scene, node), BTreeSet::new())
    }
}
fn feature_width(f: &Feature, scene: &SceneGraph) -> f64 {
    text_width(&f.text)
        * if scene.node(&f.id).is_some() {
            16. / 15.
        } else {
            1.
        }
        + 16.
}
#[derive(Clone)]
struct Feature {
    id: String,
    text: String,
    route: Vec<Point>,
    ignore: BTreeSet<String>,
    crossed: BTreeSet<String>,
    slot: Option<String>,
    slide: Option<f64>,
    position: Option<Point>,
}
pub fn place(
    scene: &SceneGraph,
    o: &LayoutOptions,
    l: &Layout,
) -> Result<Vec<LabelPlacement>, Error> {
    place_inner(scene, o, l, None, false)
}
pub(crate) fn place_with_fallback(
    scene: &SceneGraph,
    o: &LayoutOptions,
    l: &Layout,
) -> Result<Vec<LabelPlacement>, Error> {
    place_inner(scene, o, l, None, true)
}
pub(crate) fn place_with_prior(
    scene: &SceneGraph,
    o: &LayoutOptions,
    l: &Layout,
    prior: Option<&Layout>,
) -> Result<Vec<LabelPlacement>, Error> {
    match place_inner(scene, o, l, prior, false) {
        Ok(labels) => Ok(labels),
        Err(_) if prior.is_some() => place_inner(scene, o, l, prior, true),
        Err(err) => Err(err),
    }
}
fn prune_feature_candidates(valid: Vec<Candidate>) -> Vec<Candidate> {
    const MAX_PRUNED: usize = 28;
    if valid.len() <= MAX_PRUNED {
        return valid;
    }
    let mut non_callouts: Vec<Candidate> = Vec::new();
    let mut callouts: Vec<Candidate> = Vec::new();

    // 1. Up to 16 non-callout Tier 1/Tier 2 candidates across distinct quadrants / segment positions.
    for c in valid.iter().filter(|c| !c.label.callout) {
        let dx = c.label.bounds.center().x - c.label.anchor.x;
        let dy = c.label.bounds.center().y - c.label.anchor.y;
        let quad = (dx >= 0., dy >= 0.);
        let slide_bucket = (c.label.slide * 4.).round() as i32;
        let too_close = non_callouts.iter().any(|k| {
            let k_dx = k.label.bounds.center().x - k.label.anchor.x;
            let k_dy = k.label.bounds.center().y - k.label.anchor.y;
            let k_quad = (k_dx >= 0., k_dy >= 0.);
            let k_slide = (k.label.slide * 4.).round() as i32;
            (k.label.knockout == c.label.knockout && k_quad == quad && k_slide == slide_bucket)
                || k.label.bounds.center().distance(c.label.bounds.center()) < 12.
        });
        if !too_close {
            non_callouts.push(c.clone());
            if non_callouts.len() >= 16 {
                break;
            }
        }
    }
    if non_callouts.len() < 16 {
        for c in valid.iter().filter(|c| !c.label.callout) {
            if non_callouts.len() >= 16 {
                break;
            }
            if !non_callouts
                .iter()
                .any(|k| k.label.bounds == c.label.bounds)
            {
                non_callouts.push(c.clone());
            }
        }
    }

    // 2. Up to 12 callout candidates across distinct directions.
    let mut seen_sectors = BTreeSet::new();
    for c in valid.iter().filter(|c| c.label.callout) {
        let dx = c.label.bounds.center().x - c.label.anchor.x;
        let dy = c.label.bounds.center().y - c.label.anchor.y;
        let angle = dy.atan2(dx);
        let sector =
            (((angle / std::f64::consts::TAU).rem_euclid(1.0) * 12.0).round() as usize) % 12;
        if seen_sectors.insert(sector) {
            callouts.push(c.clone());
            if callouts.len() >= 12 {
                break;
            }
        }
    }
    if callouts.len() < 12 {
        for c in valid.iter().filter(|c| c.label.callout) {
            if callouts.len() >= 12 {
                break;
            }
            if !callouts.iter().any(|k| k.label.bounds == c.label.bounds) {
                callouts.push(c.clone());
            }
        }
    }

    let mut result = non_callouts;
    result.extend(callouts);
    if result.len() < MAX_PRUNED {
        for c in valid {
            if result.len() >= MAX_PRUNED {
                break;
            }
            if !result.iter().any(|k| k.label.bounds == c.label.bounds) {
                result.push(c);
            }
        }
    }
    result.sort_by(|a, b| a.cost.total_cmp(&b.cost));
    result
}
fn place_inner(
    scene: &SceneGraph,
    o: &LayoutOptions,
    l: &Layout,
    prior: Option<&Layout>,
    allow_remote_fallback: bool,
) -> Result<Vec<LabelPlacement>, Error> {
    let travel = prior
        .map(|p| {
            l.nodes
                .iter()
                .filter_map(|(id, r)| p.nodes.get(id).map(|old| r.center().distance(old.center())))
                .fold(0., f64::max)
        })
        .unwrap_or(0.);
    let mut features = Vec::new();
    for e in &scene.edges {
        let text = connection_text(scene, e);
        if text.is_empty() {
            continue;
        }
        let (ignore, crossed) = feature_scopes(scene, &e.id);
        let anns = effective_annotations(scene, &e.id, &e.annotations, o.variant.as_deref());
        let ann = anns.iter().find(|a| a.name == "label");
        features.push(Feature {
            id: e.id.clone(),
            text,
            route: crate::routing::coalesced(&l.routes[&e.id].points),
            ignore,
            crossed,
            slot: ann
                .and_then(|a| a.attrs.get("slot"))
                .map(|v| v.text.clone()),
            slide: ann.and_then(|a| value(&a.attrs, "slide")),
            position: if let Some(a) =
                ann.filter(|a| a.attrs.contains_key("x") || a.attrs.contains_key("y"))
            {
                Some(Point::new(
                    value(&a.attrs, "x").ok_or_else(|| {
                        Error::Layout("manual labels require finite x and y".into())
                    })?,
                    value(&a.attrs, "y").ok_or_else(|| {
                        Error::Layout("manual labels require finite x and y".into())
                    })?,
                ))
            } else {
                None
            },
        });
    }
    for node in &scene.nodes {
        if node.shape == "sql_table" {
            let r = l.nodes[&node.id];
            let mut ignore = ancestors(scene, &node.id);
            ignore.remove(&node.id);
            features.push(Feature {
                id: node.id.clone(),
                text: node_title(node),
                route: vec![
                    Point::new(r.left(), r.top()),
                    Point::new(r.right(), r.top()),
                ],
                ignore,
                crossed: BTreeSet::new(),
                slot: Some("above".into()),
                slide: None,
                position: None,
            });
        }
        for port in node.ports.values() {
            if node.shape == "sql_table"
                && matches!(port.constraint, PortConstraint::FixedPos { .. })
            {
                continue;
            }
            let uses: Vec<_> = scene
                .edges
                .iter()
                .filter(|e| {
                    (e.source.node == node.id && e.source.port.as_ref() == Some(&port.id))
                        || (e.target.node == node.id && e.target.port.as_ref() == Some(&port.id))
                })
                .collect();
            if !uses.is_empty() && uses.iter().all(|e| e.label.trim() == port.label.trim()) {
                continue;
            }
            let key = format!("{}#{}", node.id, port.id);
            if let Some(p) = l.ports.get(&key) {
                let mut ignore = ancestors(scene, &node.id);
                ignore.remove(&node.id);
                features.push(Feature {
                    id: key,
                    text: port.label.clone(),
                    route: vec![p.point, p.stub],
                    ignore,
                    crossed: BTreeSet::new(),
                    slot: None,
                    slide: None,
                    position: None,
                });
            }
        }
    }
    for f in &features {
        if f.slide
            .is_some_and(|t| !t.is_finite() || !(0. ..=1.).contains(&t))
            || f.slot
                .as_ref()
                .is_some_and(|s| !matches!(s.as_str(), "above" | "below" | "inline"))
        {
            return Err(Error::Layout("invalid label slot or arc slide".into()));
        }
    }
    let mut all = Vec::new();
    let mut by_feature = Vec::new();
    let bottom = l
        .nodes
        .values()
        .map(|r| r.bottom())
        .chain(l.routes.values().flat_map(|r| r.points.iter().map(|p| p.y)))
        .reduce(f64::max)
        .unwrap_or(0.)
        + 64.;
    let extent = l
        .nodes
        .values()
        .copied()
        .chain(
            l.routes
                .values()
                .flat_map(|r| r.points.iter().map(|p| Rect::new(p.x, p.y, 0., 0.))),
        )
        .reduce(Rect::union)
        .unwrap_or_default();
    let node_index = RTree::bulk_load(
        scene
            .nodes
            .iter()
            .enumerate()
            .map(|(id, n)| Indexed {
                id,
                bounds: l.nodes[&n.id],
            })
            .collect(),
    );
    // Most pointer frames only move anchors. Keep the complete previous
    // caption solution when it is still valid instead of searching again.
    if let Some(prior) =
        prior.filter(|_| travel < 32. && features.iter().all(|f| scene.edge(&f.id).is_some()))
    {
        let mut kept = Vec::new();
        for f in &features {
            if f.slot.is_some() || f.slide.is_some() {
                break;
            }
            let Some(old) = prior
                .labels
                .iter()
                .find(|l| l.feature == f.id && l.text == f.text)
            else {
                break;
            };
            let mut label = old.clone();
            if let Some(position) = f.position {
                label.bounds.x = position.x;
                label.bounds.y = position.y;
            }
            let mut found = false;
            let target = if f.position.is_some() {
                label.bounds.center()
            } else {
                old.anchor
            };
            let mut anchors = f
                .route
                .windows(2)
                .filter_map(|segment| {
                    let dx = segment[1].x - segment[0].x;
                    let dy = segment[1].y - segment[0].y;
                    if dx * dx + dy * dy < 1e-12 {
                        return None;
                    }
                    let t = (((target.x - segment[0].x) * dx + (target.y - segment[0].y) * dy)
                        / (dx * dx + dy * dy))
                        .clamp(0., 1.);
                    Some(Point::new(segment[0].x + t * dx, segment[0].y + t * dy))
                })
                .collect::<Vec<_>>();
            anchors.sort_by(|a, b| a.distance(target).total_cmp(&b.distance(target)));
            for anchor in anchors {
                let mut next = label.clone();
                next.anchor = anchor;
                let edge_gap = box_to_point_dist(next.bounds, anchor);
                if f.position.is_some() && !old.callout && edge_gap < 32. {
                    let overlaps_route = next.bounds.inflate(2.).contains(anchor);
                    next.callout = false;
                    next.orthogonal_leader = false;
                    next.knockout = overlaps_route && old.knockout;
                } else if !old.callout && edge_gap < 32. {
                    // Preserve clean flush/knockout mode while label remains < 32px from route.
                } else {
                    next.callout = true;
                    next.knockout = false;
                    next.orthogonal_leader = true;
                }
                if f.position.is_none() && next.anchor.distance(next.bounds.center()) > 256. {
                    continue;
                }
                for horizontal in [next.horizontal, !next.horizontal] {
                    next.horizontal = horizontal;
                    if valid(&next, f, scene, l, &node_index)
                        && kept.iter().all(|other| !conflict(&next, other))
                    {
                        kept.push(next.clone());
                        found = true;
                        break;
                    }
                }
                if !found && !next.callout {
                    next.callout = true;
                    next.knockout = false;
                    next.orthogonal_leader = true;
                    for horizontal in [next.horizontal, !next.horizontal] {
                        next.horizontal = horizontal;
                        if valid(&next, f, scene, l, &node_index)
                            && kept.iter().all(|other| !conflict(&next, other))
                        {
                            kept.push(next.clone());
                            found = true;
                            break;
                        }
                    }
                }
                if found {
                    break;
                }
            }
            if !found {
                break;
            }
        }
        if kept.len() == features.len() {
            kept.sort_by(|a, b| a.feature.cmp(&b.feature));
            return Ok(kept);
        }
    }
    let mut fallback_y = bottom;
    for f in &features {
        let mut candidates = generate(f, o, l, scene);
        if prior.is_none()
            && f.position.is_none()
            && f.slot.is_none()
            && f.slide.is_none()
            && let Some(candidate) = fan_caption(f, scene, l)
        {
            candidates.push(candidate);
        }
        // Facing horizontal ports with a vertical Z-elbow use one stable caption
        // rail above the pair when the leader path is unobstructed. Straight
        // horizontal connectors keep their tight on-segment placement instead.
        if f.position.is_none()
            && f.slot.is_none()
            && f.slide.is_none()
            && let Some(edge) = scene.edge(&f.id)
            && crate::render::flow_caption(scene, edge).is_some()
        {
            let a = &l.ports[&port_key(edge, true)];
            let b = &l.ports[&port_key(edge, false)];
            if matches!((a.side, b.side), (Side::E, Side::W) | (Side::W, Side::E))
                && (a.point.y - b.point.y).abs() > 4.
            {
                let points = &f.route;
                let total: f64 = points.windows(2).map(|p| p[0].distance(p[1])).sum();
                let mut remaining = total / 2.;
                let mut anchor = points[0];
                for pair in points.windows(2) {
                    let length = pair[0].distance(pair[1]);
                    if remaining <= length && length > 1e-9 {
                        let t = remaining / length;
                        anchor = Point::new(
                            pair[0].x + t * (pair[1].x - pair[0].x),
                            pair[0].y + t * (pair[1].y - pair[0].y),
                        );
                        break;
                    }
                    remaining -= length;
                }
                let h = text_height(&f.text);
                let rail_label = LabelPlacement {
                    feature: f.id.clone(),
                    text: f.text.clone(),
                    bounds: Rect::new(
                        (a.stub.x + b.stub.x) / 2.,
                        l.nodes[&edge.source.node]
                            .top()
                            .min(l.nodes[&edge.target.node].top())
                            - h / 2.
                            - 24.,
                        feature_width(f, scene),
                        h,
                    ),
                    anchor,
                    knockout: false,
                    callout: true,
                    orthogonal_leader: false,
                    horizontal: true,
                    slide: 0.5,
                };
                let rail_leader = leader_path(&rail_label);
                let crosses_foreign_route = rail_leader.windows(2).any(|lp| {
                    l.routes.iter().any(|(oid, oroute)| {
                        oid != &f.id
                            && oroute
                                .points
                                .windows(2)
                                .any(|rp| segments_intersect(lp[0], lp[1], rp[0], rp[1]))
                    })
                });
                let punches_container = rail_leader.windows(2).any(|lp| {
                    scene.nodes.iter().any(|n| {
                        n.container && {
                            let r = l.nodes[&n.id];
                            r.contains(lp[0]) != r.contains(lp[1])
                        }
                    })
                });
                let dist = rail_label.anchor.distance(rail_label.bounds.center());
                if dist <= 160. && !crosses_foreign_route && !punches_container {
                    candidates.push(Candidate {
                        label: rail_label,
                        cost: 180. + 4. * dist + if dist > 96. { 40. } else { 0. },
                    });
                }
            }
        }

        if f.position.is_none()
            && f.slot.is_none()
            && f.slide.is_none()
            && let Some(old) = prior.and_then(|p| {
                p.labels
                    .iter()
                    .find(|label| label.feature == f.id && label.text == f.text)
            })
            && scene.edge(&f.id).is_some()
        {
            let minimum = candidates
                .iter()
                .map(|c| c.cost)
                .reduce(f64::min)
                .unwrap_or(0.);
            for c in &mut candidates {
                c.cost = (c.cost - minimum).max(0.)
                    + 4. * c.label.bounds.center().distance(old.bounds.center())
                    + if (c.label.callout, c.label.knockout, c.label.horizontal)
                        != (old.callout, old.knockout, old.horizontal)
                    {
                        128.
                    } else {
                        0.
                    };
            }
            if let Some(anchor) = f
                .route
                .windows(2)
                .filter(|p| p[0].distance(p[1]) > 1e-7)
                .map(|p| {
                    let dx = p[1].x - p[0].x;
                    let dy = p[1].y - p[0].y;
                    let t = (((old.anchor.x - p[0].x) * dx + (old.anchor.y - p[0].y) * dy)
                        / (dx * dx + dy * dy))
                        .clamp(0., 1.);
                    Point::new(p[0].x + t * dx, p[0].y + t * dy)
                })
                .min_by(|a, b| a.distance(old.anchor).total_cmp(&b.distance(old.anchor)))
            {
                // A still-clear caption need not move just because its nearest
                // route segment or attachment changes. Try anchors on every
                // segment; the normal leader/body checks decide feasibility.
                for segment in f.route.windows(2) {
                    let dx = segment[1].x - segment[0].x;
                    let dy = segment[1].y - segment[0].y;
                    if dx * dx + dy * dy < 1e-12 {
                        continue;
                    }
                    let t = (((old.bounds.x - segment[0].x) * dx
                        + (old.bounds.y - segment[0].y) * dy)
                        / (dx * dx + dy * dy))
                        .clamp(0., 1.);
                    let mut fixed = old.clone();
                    fixed.anchor = Point::new(segment[0].x + t * dx, segment[0].y + t * dy);
                    fixed.callout =
                        old.callout || box_to_point_dist(fixed.bounds, fixed.anchor) >= 32.;
                    fixed.orthogonal_leader = fixed.callout;
                    fixed.knockout = old.knockout && !fixed.callout;
                    let distance = fixed.anchor.distance(fixed.bounds.center());
                    if distance < 512. {
                        for step in [0., (distance - 184.).clamp(0., 16.)] {
                            let mut tethered = fixed.clone();
                            let t = step / distance.max(1.);
                            tethered.bounds.x += (fixed.anchor.x - fixed.bounds.x) * t;
                            tethered.bounds.y += (fixed.anchor.y - fixed.bounds.y) * t;
                            let excess =
                                (tethered.anchor.distance(tethered.bounds.center()) - 120.).max(0.);
                            let mut alternate = tethered.clone();
                            alternate.horizontal = !alternate.horizontal;
                            candidates.push(Candidate {
                                cost: 4. * step + excess * excess / 32. + 1.,
                                label: alternate,
                            });
                            candidates.push(Candidate {
                                cost: 4. * step + excess * excess / 32.,
                                label: tethered,
                            });
                        }
                    }
                }
                let mut label = old.clone();
                label.bounds.x += anchor.x - old.anchor.x;
                label.bounds.y += anchor.y - old.anchor.y;
                label.anchor = anchor;
                let mut offsets = Vec::new();
                for x in [-32., -16., -8., 0., 8., 16., 32.] {
                    for y in [-32., -16., -8., 0., 8., 16., 32.] {
                        offsets.push(Point::new(x, y));
                    }
                }
                for offset in offsets {
                    let mut nearby = label.clone();
                    nearby.bounds.x += offset.x;
                    nearby.bounds.y += offset.y;
                    let edge_gap = box_to_point_dist(nearby.bounds, anchor);
                    if offset.distance(Point::default()) > 0. && (old.callout || edge_gap >= 32.) {
                        nearby.callout = true;
                        nearby.orthogonal_leader = true;
                        nearby.knockout = false;
                    }
                    let dist_cost = 4. * nearby.bounds.center().distance(old.bounds.center());
                    if nearby.orthogonal_leader {
                        let mut alternate = nearby.clone();
                        alternate.horizontal = !alternate.horizontal;
                        candidates.push(Candidate {
                            cost: dist_cost + 1.,
                            label: alternate,
                        });
                    }
                    candidates.push(Candidate {
                        cost: dist_cost,
                        label: nearby,
                    });
                }
            }
        }
        if let Some(position) = f.position {
            if !position.x.is_finite() || !position.y.is_finite() {
                return Err(Error::Layout(
                    "manual label coordinates must be finite".into(),
                ));
            }
            candidates.clear();
            for segment in f.route.windows(2) {
                let dx = segment[1].x - segment[0].x;
                let dy = segment[1].y - segment[0].y;
                if dx * dx + dy * dy < 1e-12 {
                    continue;
                }
                let t = (((position.x - segment[0].x) * dx + (position.y - segment[0].y) * dy)
                    / (dx * dx + dy * dy))
                    .clamp(0., 1.);
                let bounds = Rect::new(
                    position.x,
                    position.y,
                    feature_width(f, scene),
                    text_height(&f.text),
                );
                let anchor = Point::new(segment[0].x + t * dx, segment[0].y + t * dy);
                let edge_gap = box_to_point_dist(bounds, anchor);
                if bounds.inflate(2.).contains(anchor) {
                    candidates.push(Candidate {
                        cost: 0.,
                        label: LabelPlacement {
                            feature: f.id.clone(),
                            text: f.text.clone(),
                            bounds,
                            anchor,
                            knockout: true,
                            callout: false,
                            orthogonal_leader: false,
                            horizontal: dx.abs() > dy.abs(),
                            slide: 0.5,
                        },
                    });
                } else if edge_gap < 32. {
                    // Manual drags / @label positions staying < 32px from the route remain clean non-callouts.
                    candidates.push(Candidate {
                        cost: 1. + edge_gap,
                        label: LabelPlacement {
                            feature: f.id.clone(),
                            text: f.text.clone(),
                            bounds,
                            anchor,
                            knockout: false,
                            callout: false,
                            orthogonal_leader: false,
                            horizontal: dx.abs() > dy.abs(),
                            slide: 0.5,
                        },
                    });
                }
                for horizontal in [true, false] {
                    candidates.push(Candidate {
                        cost: 40. + edge_gap,
                        label: LabelPlacement {
                            feature: f.id.clone(),
                            text: f.text.clone(),
                            bounds,
                            anchor,
                            knockout: false,
                            callout: true,
                            orthogonal_leader: true,
                            horizontal,
                            slide: 0.5,
                        },
                    });
                }
            }
        }
        let crossed_penalty = |c: &Candidate| candidate_penalty(c, f, scene, l, prior.is_none());
        let is_valid_candidate =
            |c: &Candidate| candidate_valid(c, f, scene, l, &node_index, prior.is_none(), (8., 4.));
        let mut valid_cands = Vec::new();
        for mut c in candidates {
            if is_valid_candidate(&c) {
                c.cost += crossed_penalty(&c);
                valid_cands.push(c);
            }
        }
        let shares_corridor = scene.edge(&f.id).is_some_and(|edge| {
            scene.edges.iter().any(|other| {
                other.id != edge.id
                    && ((other.source.node == edge.source.node
                        && other.target.node == edge.target.node)
                        || (other.source.node == edge.target.node
                            && other.target.node == edge.source.node))
            }) || l.routes.iter().any(|(oid, oroute)| {
                oid != &f.id
                    && f.route.windows(2).any(|a| {
                        let a_horiz = (a[0].y - a[1].y).abs() < 1e-6;
                        oroute.points.windows(2).any(|b| {
                            let b_horiz = (b[0].y - b[1].y).abs() < 1e-6;
                            if a_horiz && b_horiz {
                                (a[0].y - b[0].y).abs() <= 28.
                                    && a[0].x.min(a[1].x) < b[0].x.max(b[1].x) - 16.
                                    && b[0].x.min(b[1].x) < a[0].x.max(a[1].x) - 16.
                            } else if !a_horiz && !b_horiz {
                                (a[0].x - b[0].x).abs() <= 28.
                                    && a[0].y.min(a[1].y) < b[0].y.max(b[1].y) - 16.
                                    && b[0].y.min(b[1].y) < a[0].y.max(a[1].y) - 16.
                            } else {
                                false
                            }
                        })
                    })
            })
        });
        let is_port = f.id.contains('#') && scene.edge(&f.id).is_none();
        let core_left = extent.left();
        let core_right = extent.right();
        if !is_port
            && (valid_cands.len() < 6 || shares_corridor)
            && f.position.is_none()
            && f.slot.is_none()
            && f.slide.is_none()
        {
            for mut c in nearby_callouts(f, scene) {
                if is_valid_candidate(&c) {
                    c.cost += crossed_penalty(&c)
                        + horizontal_overhang_penalty(c.label.bounds, core_left, core_right);
                    valid_cands.push(c);
                }
            }
        }
        if f.position.is_none()
            && f.slot.as_deref() != Some("inline")
            && (valid_cands.is_empty()
                || (valid_cands.len() < 2 && f.slot.is_none() && f.slide.is_none()))
        {
            for (radii, port_box, leader_node_pad) in [
                (&[28., 44., 64., 88., 112., 136., 152., 159.][..], 8., 4.),
                (&[28., 44., 64., 88., 112., 136., 152., 159.][..], 2., 0.),
            ] {
                for mut c in dense_callout_ring(f, scene, radii) {
                    if candidate_valid(
                        &c,
                        f,
                        scene,
                        l,
                        &node_index,
                        prior.is_none(),
                        (port_box, leader_node_pad),
                    ) {
                        c.cost += crossed_penalty(&c)
                            + horizontal_overhang_penalty(c.label.bounds, core_left, core_right);
                        valid_cands.push(c);
                    }
                }
                if !valid_cands.is_empty() {
                    break;
                }
            }
        }
        // Snug pockets: captions that exactly fill the free band between
        // neighbouring routes and node faces near any segment of the route.
        if f.position.is_none() && f.slot.as_deref() != Some("inline") && valid_cands.len() < 2 {
            for mut c in pocket_candidates(f, scene, l) {
                if is_valid_candidate(&c) {
                    c.cost += crossed_penalty(&c)
                        + horizontal_overhang_penalty(c.label.bounds, core_left, core_right);
                    valid_cands.push(c);
                }
            }
        }
        if allow_remote_fallback && f.position.is_none() && valid_cands.len() < 2 {
            // Deterministic outer-perimeter margin slots on all four sides,
            // each tethered by a straight leader to a segment midpoint.
            let w = feature_width(f, scene);
            let h = text_height(&f.text);
            let anchors: Vec<Point> = f
                .route
                .windows(2)
                .filter(|p| p[0].distance(p[1]) > 1e-6)
                .map(|p| Point::new((p[0].x + p[1].x) / 2., (p[0].y + p[1].y) / 2.))
                .collect();
            let mut slots = Vec::new();
            for k in 0..4 {
                let step = k as f64 * (h + 20.);
                let top = extent.top() - 48. - h / 2. - step;
                let bottom = fallback_y.max(extent.bottom() + 48.) + h / 2. + step;
                let left = extent.left() - 64. - w / 2. - k as f64 * (w + 24.);
                let right = extent.right() + 64. + w / 2. + k as f64 * (w + 24.);
                for &anchor in &anchors {
                    slots.push(Rect::new(anchor.x, top, w, h));
                    slots.push(Rect::new(anchor.x, bottom, w, h));
                    slots.push(Rect::new(left, anchor.y, w, h));
                    slots.push(Rect::new(right, anchor.y, w, h));
                }
                for i in 0..=8 {
                    let x = extent.left() + extent.w * i as f64 / 8.;
                    let y = extent.top() + extent.h * i as f64 / 8.;
                    slots.extend([
                        Rect::new(x, top, w, h),
                        Rect::new(x, bottom, w, h),
                        Rect::new(left, y, w, h),
                        Rect::new(right, y, w, h),
                    ]);
                }
            }
            let mut best: Option<Candidate> = None;
            for bounds in slots {
                for &anchor in &anchors {
                    let dist = bounds.center().distance(anchor);
                    let c = Candidate {
                        label: LabelPlacement {
                            feature: f.id.clone(),
                            text: f.text.clone(),
                            bounds,
                            anchor,
                            knockout: false,
                            callout: true,
                            orthogonal_leader: false,
                            horizontal: true,
                            slide: 0.5,
                        },
                        cost: 10000.
                            + o.weights.pill
                            + 4. * dist
                            + horizontal_overhang_penalty(bounds, core_left, core_right),
                    };
                    if best.as_ref().is_some_and(|b| b.cost <= c.cost) {
                        continue;
                    }
                    if is_valid_candidate(&c) {
                        best = Some(c);
                    }
                }
            }
            if best.is_none() {
                // If all outer leaders cross a node, try a clean close-proximity or
                // inline pill fallback on the edge's route segments.
                for seg in f.route.windows(2) {
                    let (a, b) = (seg[0], seg[1]);
                    let horizontal = (a.y - b.y).abs() < 1e-6;
                    for t in [0.5, 0.35, 0.65, 0.2, 0.8, 0.1, 0.9] {
                        let anchor = Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
                        for (knockout, side) in [(true, 0.), (false, -1.), (false, 1.)] {
                            let offset = if side == 0. {
                                0.
                            } else if horizontal {
                                h / 2. + 4.
                            } else {
                                w / 2. + 4.
                            };
                            let center = if horizontal {
                                anchor.offset(0., side * offset)
                            } else {
                                anchor.offset(side * offset, 0.)
                            };
                            let bounds = Rect::new(center.x, center.y, w, h);
                            let c = Candidate {
                                label: LabelPlacement {
                                    feature: f.id.clone(),
                                    text: f.text.clone(),
                                    bounds,
                                    anchor,
                                    knockout,
                                    callout: false,
                                    orthogonal_leader: false,
                                    horizontal,
                                    slide: t,
                                },
                                cost: 12000.
                                    + if knockout {
                                        o.weights.pill
                                    } else {
                                        horizontal_overhang_penalty(bounds, core_left, core_right)
                                    },
                            };
                            if is_valid_candidate(&c) {
                                best = Some(c);
                                break;
                            }
                        }
                        if best.is_some() {
                            break;
                        }
                    }
                    if best.is_some() {
                        break;
                    }
                }
            }
            fallback_y += h + 20.;
            if let Some(mut c) = best {
                c.cost += crossed_penalty(&c);
                valid_cands.push(c);
            }
        }
        if valid_cands.is_empty() {
            return Err(Error::Layout(format!(
                "no feasible label position for {}",
                f.id
            )));
        }
        if f.position.is_none()
            && f.slot.is_none()
            && f.slide.is_none()
            && let Some(prior) = prior
            && let Some(old) = prior.labels.iter().find(|label| label.feature == f.id)
        {
            let travel = l
                .nodes
                .iter()
                .filter_map(|(id, r)| {
                    prior
                        .nodes
                        .get(id)
                        .map(|old| r.center().distance(old.center()))
                })
                .fold(0., f64::max);
            let radius = (2. * travel + 8.).max(24.);
            if valid_cands
                .iter()
                .any(|c| c.label.bounds.center().distance(old.bounds.center()) <= radius)
            {
                for c in &mut valid_cands {
                    let distance = c.label.bounds.center().distance(old.bounds.center());
                    if distance > radius {
                        c.cost += 1_000_000. + distance * distance;
                    }
                }
            }
        }
        // Normalize candidate costs so every cost >= 0.0 and B&B pruning is admissible.
        let min_cost = valid_cands
            .iter()
            .map(|c| c.cost)
            .reduce(f64::min)
            .unwrap_or(0.);
        if min_cost < 0. {
            for c in &mut valid_cands {
                c.cost -= min_cost;
            }
        }
        for c in &mut valid_cands {
            c.cost = c.cost.max(0.);
        }
        valid_cands.sort_by(|a, b| a.cost.total_cmp(&b.cost));
        // Prune to at most 28 diverse candidates per feature to keep conflict graph O(C^2) bounded.
        let pruned = prune_feature_candidates(valid_cands);
        let mut ids = Vec::with_capacity(pruned.len());
        for c in pruned {
            ids.push(all.len());
            all.push(c);
        }
        by_feature.push(ids);
    }
    let index = RTree::bulk_load(
        all.iter()
            .enumerate()
            .map(|(id, c)| Indexed {
                id,
                bounds: footprint(&c.label),
            })
            .collect(),
    );
    let mut conflicts = vec![BTreeSet::new(); all.len()];
    let mut owner = vec![0; all.len()];
    for (i, ids) in by_feature.iter().enumerate() {
        for &id in ids {
            owner[id] = i;
        }
    }
    let mut neighbors = vec![BTreeSet::new(); features.len()];
    for (i, c) in all.iter().enumerate() {
        for hit in index.locate_in_envelope_intersecting(&envelope(footprint(&c.label))) {
            if owner[i] != owner[hit.id] && conflict(&c.label, &all[hit.id].label) {
                conflicts[i].insert(hit.id);
                neighbors[owner[i]].insert(owner[hit.id]);
            }
        }
    }
    let mut seen = vec![false; features.len()];
    let mut selected = Vec::new();
    for root in 0..features.len() {
        if seen[root] {
            continue;
        }
        let mut component = Vec::new();
        let mut stack = vec![root];
        seen[root] = true;
        while let Some(i) = stack.pop() {
            component.push(i);
            for &j in &neighbors[i] {
                if !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        component.sort_by_key(|i| by_feature[*i].len());
        let mut solution = select_component(&component, &by_feature, &all, &conflicts, 3_500);
        if solution.len() != component.len() {
            solution = retry_component(
                &component,
                &features,
                scene,
                l,
                &node_index,
                prior.is_none(),
                &selected,
                &mut by_feature,
                &mut all,
            );
        }
        if solution.len() != component.len() {
            return Err(Error::Layout(
                "no feasible label position for complete caption assignment".into(),
            ));
        }
        selected.extend(solution);
    }
    let mut labels: Vec<_> = selected.into_iter().map(|i| all[i].label.clone()).collect();
    labels.sort_by(|a, b| a.feature.cmp(&b.feature));
    polish(&mut labels, scene, l, o);
    for (i, a) in labels.iter().enumerate() {
        if labels[i + 1..]
            .iter()
            .any(|b| a.bounds.intersects(b.bounds))
        {
            return Err(Error::Layout("label optimizer returned an overlap".into()));
        }
    }
    Ok(labels)
}
// In a simple fan, a caption belongs to the independent leaf's horizontal
// branch. Prefer the same offset from that leaf instead of a random elbow.
fn fan_caption(f: &Feature, scene: &SceneGraph, l: &Layout) -> Option<Candidate> {
    let edge = scene.edge(&f.id)?;
    let fan = scene.nodes.len() >= 4 && scene.edges.len() + 1 == scene.nodes.len();
    let fan_out = fan
        && scene
            .edges
            .iter()
            .all(|e| e.source.node == edge.source.node);
    let fan_in = fan
        && scene
            .edges
            .iter()
            .all(|e| e.target.node == edge.target.node);
    let record_bundle = edge.target.port.is_some()
        && scene.edges.len() > 1
        && scene
            .edges
            .iter()
            .all(|e| e.source.node == edge.source.node && e.target.node == edge.target.node);
    if !fan_out && !fan_in && !record_bundle {
        return None;
    }
    let leaf = if fan_out {
        &edge.target.node
    } else {
        &edge.source.node
    };
    let body = l.nodes[leaf];
    let branch_y = if record_bundle {
        l.ports[&port_key(edge, true)].point.y
    } else {
        body.y
    };
    let w = feature_width(f, scene);
    let h = text_height(&f.text);
    for (segment, pair) in f.route.windows(2).enumerate() {
        if (pair[0].y - branch_y).abs() > 0.01 || (pair[1].y - branch_y).abs() > 0.01 {
            continue;
        }
        let left = pair[0].x.min(pair[1].x) + w / 2. + 8.;
        let right = pair[0].x.max(pair[1].x) - w / 2. - 8.;
        if left > right {
            continue;
        }
        let x = if fan_out {
            body.left() - w / 2. - 20.
        } else {
            body.right() + w / 2. + 20.
        };
        let anchor = Point::new(x.clamp(left, right), branch_y);
        return Some(Candidate {
            cost: 0.,
            label: LabelPlacement {
                feature: f.id.clone(),
                text: f.text.clone(),
                bounds: Rect::new(anchor.x, anchor.y - h / 2. - 6., w, h),
                anchor,
                knockout: false,
                callout: false,
                orthogonal_leader: false,
                horizontal: true,
                slide: (f
                    .route
                    .windows(2)
                    .take(segment)
                    .map(|p| p[0].distance(p[1]))
                    .sum::<f64>()
                    + pair[0].distance(anchor))
                    / f.route
                        .windows(2)
                        .map(|p| p[0].distance(p[1]))
                        .sum::<f64>()
                        .max(1.),
            },
        });
    }
    None
}
fn core_x_span(l: &Layout) -> (f64, f64) {
    let mut left = f64::INFINITY;
    let mut right = f64::NEG_INFINITY;
    for r in l.nodes.values() {
        left = left.min(r.left());
        right = right.max(r.right());
    }
    for route in l.routes.values() {
        for p in &route.points {
            left = left.min(p.x);
            right = right.max(p.x);
        }
    }
    if left.is_finite() {
        (left, right)
    } else {
        (0., 0.)
    }
}
fn horizontal_overhang_penalty(bounds: Rect, core_left: f64, core_right: f64) -> f64 {
    let left_over = (core_left - 16. - bounds.left()).max(0.);
    let right_over = (bounds.right() - (core_right + 16.)).max(0.);
    0.8 * (left_over + right_over)
}
fn generate(f: &Feature, o: &LayoutOptions, l: &Layout, scene: &SceneGraph) -> Vec<Candidate> {
    let w = feature_width(f, scene);
    let h = text_height(&f.text);
    let is_port = f.id.contains('#') && scene.edge(&f.id).is_none();
    let (core_left, core_right) = core_x_span(l);
    let lengths: Vec<_> = f.route.windows(2).map(|p| p[0].distance(p[1])).collect();
    let total = lengths.iter().sum::<f64>().max(1.);
    let mut arc = 0.;
    let mut out = Vec::new();
    let edge_kind = scene.edge(&f.id).map(|e| e.kind);
    let seg_count = f.route.len().saturating_sub(1);
    if !is_port {
        for (i, pair) in f.route.windows(2).enumerate() {
            let (a, b) = (pair[0], pair[1]);
            let len = lengths[i];
            let horizontal = (a.y - b.y).abs() < 1e-6;
            let along = if horizontal { w } else { h };
            if len < 1e-6 {
                arc += len;
                continue;
            }
            let low = ((along / 2. + 4.) / len).min(0.5);
            let high = 1. - low;
            let clamped_samples = if let Some(t) = f.slide {
                vec![((t * total - arc) / len).clamp(low, high)]
            } else {
                let mut s = Vec::new();
                for cand in [
                    0.5,
                    0.35_f64.clamp(low, high),
                    0.65_f64.clamp(low, high),
                    0.25_f64.clamp(low, high),
                    0.75_f64.clamp(low, high),
                    0.15_f64.clamp(low, high),
                    0.85_f64.clamp(low, high),
                    low,
                    high,
                ] {
                    if s.iter().all(|prev: &f64| (*prev - cand).abs() >= 1e-4) {
                        s.push(cand);
                    }
                }
                s
            };
            let tier1_samples = if f.slide.is_none() && len >= 48. {
                let mut s = Vec::new();
                for cand in [0.5, 0.35, 0.65, 0.25, 0.75]
                    .into_iter()
                    .chain(clamped_samples.iter().copied())
                {
                    if s.iter().all(|prev: &f64| (*prev - cand).abs() >= 1e-4) {
                        s.push(cand);
                    }
                }
                s
            } else {
                clamped_samples.clone()
            };
            let has_start_arrow = i == 0 && edge_kind == Some(EdgeKind::Bidirectional);
            let has_end_arrow =
                i + 1 == seg_count && edge_kind.is_some_and(|k| k != EdgeKind::Undirected);
            let pill_start_margin = if has_start_arrow { 16. } else { 12. };
            let pill_end_margin = if has_end_arrow { 16. } else { 12. };
            let explicit_inline = f.slot.as_deref() == Some("inline");
            for (side, extra) in [-1., 1., 0.].into_iter().flat_map(|side| {
                [0., 4., 8., 16., 26.]
                    .into_iter()
                    .filter(move |extra| side != 0. || *extra == 0.)
                    .map(move |extra| (side, extra))
            }) {
                if let Some(slot) = &f.slot
                    && ((slot == "above" && side != -1.)
                        || (slot == "below" && side != 1.)
                        || (slot == "inline" && side != 0.))
                {
                    continue;
                }
                let samples = if side == 0. {
                    &clamped_samples
                } else {
                    &tier1_samples
                };
                for &t in samples {
                    let anchor = Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
                    let mut tight_pill = false;
                    if side == 0. && !explicit_inline {
                        // RFC-001 Tier 2 centered inline pill: require enough segment length and
                        // clearance from arrowheads, stubs, and bends.
                        let roomy = len >= along + 24.
                            && t * len >= along / 2. + pill_start_margin - 1e-6
                            && (1. - t) * len >= along / 2. + pill_end_margin - 1e-6;
                        if !roomy {
                            // Short segments may still carry a compact pill (6px margins,
                            // arrowheads keep 12px) at a small premium over roomy pills; it
                            // only wins where adjacent Tier 1 slots are blocked, and still
                            // outranks every Tier 3 callout.
                            let pill_margin = 6.;
                            let start = if has_start_arrow { 12. } else { pill_margin };
                            let end = if has_end_arrow { 12. } else { pill_margin };
                            if len < along + 2. * pill_margin
                                || t * len < along / 2. + start - 1e-6
                                || (1. - t) * len < along / 2. + end - 1e-6
                            {
                                continue;
                            }
                            tight_pill = true;
                        }
                    }
                    let offset = if horizontal {
                        h / 2. + 6. + extra
                    } else {
                        w / 2. + 6. + extra
                    };
                    let center = if horizontal {
                        anchor.offset(0., side * offset)
                    } else {
                        anchor.offset(side * offset, 0.)
                    };
                    let bounds = Rect::new(center.x, center.y, w, h);
                    let centered = (2. * (arc + len * t) / total - 1.).abs();
                    let slide_cost = f
                        .slide
                        .map(|target| 1000. * ((arc + len * t) / total - target).abs());
                    let cost = if side == 0. {
                        // Tier 2 (w_pill = 60..110): inline knockout pill outranks flagpoles.
                        slide_cost.unwrap_or(0.)
                            + o.weights.pill
                            + o.weights.centeredness * centered
                            + if tight_pill { 10. } else { 0. }
                    } else {
                        // Tier 1 (0..45): flush external adjacent slot along segment.
                        slide_cost.unwrap_or(4.)
                            + extra * 1.25
                            + o.weights.centeredness * centered
                            + if side > 0. { 1. } else { 0. }
                            + if f.position.is_none() {
                                horizontal_overhang_penalty(bounds, core_left, core_right)
                            } else {
                                0.
                            }
                    };
                    out.push(Candidate {
                        label: LabelPlacement {
                            feature: f.id.clone(),
                            text: f.text.clone(),
                            bounds,
                            anchor,
                            knockout: side == 0.,
                            callout: false,
                            orthogonal_leader: false,
                            horizontal,
                            slide: (arc + len * t) / total,
                        },
                        cost,
                    });
                }
            }
            arc += len;
        }
    }
    // Restrict port captions primarily to 4 tight Imhof corner quadrants (4..14px
    // outside port along face normal, inside owning container), adding outward
    // normal stagger and short Tier 3 callouts when tightly stacked port routes
    // block every adjacent offset.
    if is_port && f.route.len() == 2 {
        let wall = f.route[0];
        let anchor = f.route[1];
        let dx_axis = anchor.x - wall.x;
        let dy_axis = anchor.y - wall.y;
        let nx = if dx_axis.abs() > 1e-6 {
            dx_axis.signum()
        } else {
            0.
        };
        let ny = if dy_axis.abs() > 1e-6 {
            dy_axis.signum()
        } else {
            0.
        };
        let owner_pen = |b: Rect| {
            let escapes = f
                .ignore
                .iter()
                .filter(|id| {
                    scene.node(id).is_some_and(|n| n.container)
                        && !l.nodes[*id].inflate(-2.).encloses(b)
                })
                .count();
            escapes as f64 * 500.
        };
        for (origin, origin_cost, normal_extras) in [
            (wall, 0., &[0., 20., 40., 68., 100., 136.][..]),
            (anchor, 14., &[0., 24., 52., 84.][..]),
        ] {
            for &n_extra in normal_extras {
                let along_steps: &[f64] = if origin_cost == 0. && n_extra == 0. {
                    &[0., 16., 32., 48., 72., 96., 120.]
                } else if n_extra <= 40. {
                    &[0., 16., 32., 48.]
                } else {
                    &[0., 24.]
                };
                for &along in along_steps {
                    for gap in [6., 8., 10., 12., 14.] {
                        for (dx, dy, qcost) in
                            [(1., -1., 0.), (-1., -1., 1.), (1., 1., 2.), (-1., 1., 3.)]
                        {
                            let inward = (nx != 0. && dx * nx < 0.) || (ny != 0. && dy * ny < 0.);
                            if inward && (origin_cost > 0. || n_extra > 0.) {
                                continue;
                            }
                            let x_gap = if nx != 0. { gap + n_extra } else { 6. + along };
                            let y_gap = if ny != 0. { gap + n_extra } else { 6. + along };
                            let bounds = Rect::new(
                                origin.x + dx * (w / 2. + x_gap),
                                origin.y + dy * (h / 2. + y_gap),
                                w,
                                h,
                            );
                            if (n_extra > 0. || along > 16.)
                                && bounds.center().distance(anchor) > 158.
                            {
                                continue;
                            }
                            let inward_penalty = if inward { 24. } else { 0. };
                            out.push(Candidate {
                                label: LabelPlacement {
                                    feature: f.id.clone(),
                                    text: f.text.clone(),
                                    bounds,
                                    anchor,
                                    knockout: false,
                                    callout: false,
                                    orthogonal_leader: false,
                                    horizontal: true,
                                    slide: 0.5,
                                },
                                cost: origin_cost
                                    + inward_penalty
                                    + qcost
                                    + (gap - 6.).abs() * 0.5
                                    + n_extra * 0.45
                                    + along * 0.6
                                    + owner_pen(bounds),
                            });
                        }
                    }
                }
            }
        }
        // Fallback Tier 3 short callout candidates only if all non-callout slots collide.
        for radius in [36., 56., 80., 104., 128., 156.] {
            for k in 0..12 {
                let angle = (k as f64) * std::f64::consts::TAU / 12.;
                let dx = angle.cos();
                let dy = angle.sin();
                let bounds = Rect::new(anchor.x + dx * radius, anchor.y + dy * radius, w, h);
                let dist = bounds.center().distance(anchor);
                if dist > 160. {
                    continue;
                }
                for (orthogonal, extra_cost) in [(false, 0.), (true, 2.)] {
                    out.push(Candidate {
                        label: LabelPlacement {
                            feature: f.id.clone(),
                            text: f.text.clone(),
                            bounds,
                            anchor,
                            knockout: false,
                            callout: true,
                            orthogonal_leader: orthogonal,
                            horizontal: dx.abs() >= dy.abs(),
                            slide: 0.5,
                        },
                        cost: 240.
                            + extra_cost
                            + 4. * dist
                            + if dist > 96. { 40. } else { 0. }
                            + owner_pen(bounds),
                    });
                }
            }
        }
        return out;
    }
    // Tier 3 (180 + 4.0 * dist): short callout leader <= 160px, preferring <= 96px.
    let tier3_start = out.len();
    if f.slot.as_deref() != Some("inline") {
        for arc in [0.5, 0.35, 0.65] {
            let anchor = crate::render::at_arc(&f.route, arc);
            if !is_port && is_vertical_at_arc(&f.route, arc) {
                push_vertical_edge_callouts(f, anchor, (w, h), arc, 180., false, &mut out);
            }
            for radius in [28., 36., 44., 56., 80., 96., 128., 160.] {
                for (dx, dy) in [(0., -1.), (0., 1.), (-1., 0.), (1., 0.)] {
                    if let Some(slot) = f.slot.as_deref()
                        && ((slot == "above" && dy >= 0.) || (slot == "below" && dy <= 0.))
                    {
                        continue;
                    }
                    let bounds = Rect::new(anchor.x + dx * radius, anchor.y + dy * radius, w, h);
                    let dist = bounds.center().distance(anchor);
                    if dist > 160. + 1e-6 {
                        continue;
                    }
                    out.push(Candidate {
                        label: LabelPlacement {
                            feature: f.id.clone(),
                            text: f.text.clone(),
                            bounds,
                            anchor,
                            knockout: false,
                            callout: true,
                            orthogonal_leader: false,
                            horizontal: true,
                            slide: arc,
                        },
                        cost: 180.
                            + 4. * dist
                            + if dist > 96. { 40. } else { 0. }
                            + (arc - 0.5).abs() * 12.,
                    });
                }
            }
        }
    }
    if f.position.is_none() {
        for c in &mut out[tier3_start..] {
            c.cost += horizontal_overhang_penalty(c.label.bounds, core_left, core_right);
        }
    }
    out
}
fn is_vertical_at_arc(route: &[Point], arc: f64) -> bool {
    if route.len() < 2 {
        return false;
    }
    let total: f64 = route.windows(2).map(|p| p[0].distance(p[1])).sum();
    if total < 1e-6 {
        return false;
    }
    let target = arc.clamp(0., 1.) * total;
    let mut seen = 0.;
    for pair in route.windows(2) {
        let len = pair[0].distance(pair[1]);
        if seen + len + 1e-6 >= target {
            return (pair[0].y - pair[1].y).abs() > (pair[0].x - pair[1].x).abs();
        }
        seen += len;
    }
    let last = &route[route.len() - 2..];
    (last[0].y - last[1].y).abs() > (last[0].x - last[1].x).abs()
}
fn push_vertical_edge_callouts(
    f: &Feature,
    anchor: Point,
    (w, h): (f64, f64),
    arc: f64,
    base_cost: f64,
    with_ortho_variants: bool,
    out: &mut Vec<Candidate>,
) {
    let variants: &[(bool, f64)] = if with_ortho_variants {
        &[(false, 0.), (true, 2.)]
    } else {
        &[(false, 0.)]
    };
    for side in [-1., 1.] {
        for gap in [8., 14., 22., 32.] {
            for dy_off in [0., -16., 16.] {
                if let Some(slot) = f.slot.as_deref()
                    && ((slot == "above" && dy_off >= 0.) || (slot == "below" && dy_off <= 0.))
                {
                    continue;
                }
                let bounds = Rect::new(anchor.x + side * (w * 0.5 + gap), anchor.y + dy_off, w, h);
                let dist = bounds.center().distance(anchor);
                if dist > 160. + 1e-6 {
                    continue;
                }
                for &(orthogonal_leader, extra_cost) in variants {
                    out.push(Candidate {
                        label: LabelPlacement {
                            feature: f.id.clone(),
                            text: f.text.clone(),
                            bounds,
                            anchor,
                            knockout: false,
                            callout: true,
                            orthogonal_leader,
                            horizontal: true,
                            slide: arc,
                        },
                        cost: base_cost
                            + extra_cost
                            + 4. * dist
                            + if dist > 96. { 40. } else { 0. }
                            + (arc - 0.5).abs() * 12.,
                    });
                }
            }
        }
    }
}
// A tightly packed parallel bundle may need a diagonal, local leader. Only
// search this larger neighborhood when ordinary automatic slots are all blocked.
fn nearby_callouts(f: &Feature, scene: &SceneGraph) -> Vec<Candidate> {
    let w = feature_width(f, scene);
    let h = text_height(&f.text);
    let mut out = Vec::new();
    for arc in [0.15, 0.35, 0.5, 0.65, 0.85] {
        let anchor = crate::render::at_arc(&f.route, arc);
        // Offsets are measured from the caption's half extent, so `gap` is the
        // clear distance between the anchor and the nearest caption face.
        for gap in [
            6., 14., 20., 24., 28., 34., 40., 44., 64., 68., 84., 96., 116., 128., 148.,
        ] {
            for (dx, dy) in [
                (0.0_f64, -1.),
                (1., -1.),
                (1., 0.),
                (1., 1.),
                (0., 1.),
                (-1., 1.),
                (-1., 0.),
                (-1., -1.),
            ] {
                let bounds = Rect::new(
                    anchor.x + dx * (w * 0.5 + gap),
                    anchor.y + dy * (h * 0.5 + gap),
                    w,
                    h,
                );
                let dist = bounds.center().distance(anchor);
                if dist > 160. + 1e-6 {
                    continue;
                }
                let edge_gap = box_to_point_dist(bounds, anchor);
                let on_segment = f.route.windows(2).any(|seg| {
                    let (a, b) = (seg[0], seg[1]);
                    let horiz = (a.y - b.y).abs() < 1e-6;
                    let vert = (a.x - b.x).abs() < 1e-6;
                    if horiz {
                        (anchor.y - a.y).abs() < 1e-3
                            && anchor.x >= a.x.min(b.x) - 1e-3
                            && anchor.x <= a.x.max(b.x) + 1e-3
                            && bounds.left() >= a.x.min(b.x) - 8.
                            && bounds.right() <= a.x.max(b.x) + 8.
                    } else if vert {
                        (anchor.x - a.x).abs() < 1e-3
                            && anchor.y >= a.y.min(b.y) - 1e-3
                            && anchor.y <= a.y.max(b.y) + 1e-3
                            && bounds.top() >= a.y.min(b.y) - 8.
                            && bounds.bottom() <= a.y.max(b.y) + 8.
                    } else {
                        false
                    }
                });
                if on_segment && edge_gap <= 14. {
                    // Tier 1 adjacent caption: close enough to read without a leader and aligned on segment.
                    out.push(Candidate {
                        label: LabelPlacement {
                            feature: f.id.clone(),
                            text: f.text.clone(),
                            bounds,
                            anchor,
                            knockout: false,
                            callout: false,
                            orthogonal_leader: false,
                            horizontal: dy == 0.,
                            slide: arc,
                        },
                        cost: 40. + 2. * edge_gap + (arc - 0.5).abs() * 12.,
                    });
                    continue;
                }
                if gap < 16. {
                    continue;
                }
                for horizontal in [false, true] {
                    out.push(Candidate {
                        label: LabelPlacement {
                            feature: f.id.clone(),
                            text: f.text.clone(),
                            bounds,
                            anchor,
                            knockout: false,
                            callout: true,
                            orthogonal_leader: true,
                            horizontal,
                            slide: arc,
                        },
                        cost: 180.
                            + 4. * dist
                            + if dist > 96. { 40. } else { 0. }
                            + (arc - 0.5).abs() * 12.,
                    });
                }
            }
        }
    }
    out
}
fn candidate_penalty(
    c: &Candidate,
    f: &Feature,
    scene: &SceneGraph,
    l: &Layout,
    prior_none: bool,
) -> f64 {
    let mut pen = if f
        .crossed
        .iter()
        .any(|id| l.nodes[id].intersects(c.label.bounds))
    {
        48.
    } else {
        0.
    };
    if f.position.is_none() {
        let inset = if scene.edge(&f.id).is_some() {
            -4.
        } else {
            -2.
        };
        let owner_escapes = f
            .ignore
            .iter()
            .filter(|id| {
                scene.node(id).is_some_and(|n| n.container)
                    && !l.nodes[*id].inflate(inset).encloses(c.label.bounds)
            })
            .count();
        pen += owner_escapes as f64 * 500.;
    }
    if f.position.is_none()
        && (c.label.callout || (!c.label.knockout && scene.edge(&f.id).is_some()))
    {
        let tether = [c.label.anchor, c.label.bounds.center()];
        let path_storage;
        let path: &[Point] = if c.label.callout {
            path_storage = leader_path(&c.label);
            &path_storage
        } else {
            &tether
        };
        let foreign_crossings = l
            .routes
            .iter()
            .filter(|(oid, oroute)| {
                *oid != &f.id
                    && path.windows(2).any(|lp| {
                        oroute
                            .points
                            .windows(2)
                            .any(|rp| segments_intersect(lp[0], lp[1], rp[0], rp[1]))
                    })
            })
            .count();
        pen += foreign_crossings as f64 * 600.;
        if c.label.callout {
            let container_punches = scene
                .nodes
                .iter()
                .filter(|n| {
                    n.container && {
                        let r = l.nodes[&n.id];
                        path.windows(2)
                            .any(|lp| r.contains(lp[0]) != r.contains(lp[1]))
                    }
                })
                .count();
            pen += container_punches as f64 * 240.;
            if prior_none {
                let leader_len: f64 = path.windows(2).map(|p| p[0].distance(p[1])).sum();
                pen += 0.8 * leader_len;
            }
        }
    }
    pen
}
fn candidate_valid(
    c: &Candidate,
    f: &Feature,
    scene: &SceneGraph,
    l: &Layout,
    index: &RTree<Indexed>,
    prior_none: bool,
    (port_box, leader_node_pad): (f64, f64),
) -> bool {
    let stable_leader = prior_none
        || !leader_segments(&c.label).iter().any(|&(p, q)| {
            scene.nodes.iter().any(|n| {
                !f.ignore.contains(&n.id)
                    && !f.crossed.contains(&n.id)
                    && segment_hits(p, q, l.nodes[&n.id].inflate(leader_node_pad))
            })
        });
    stable_leader && valid_with_port_box(&c.label, f, scene, l, index, port_box)
}
fn dense_callout_ring(f: &Feature, scene: &SceneGraph, radii: &[f64]) -> Vec<Candidate> {
    let w = feature_width(f, scene);
    let h = text_height(&f.text);
    let is_port = f.id.contains('#') && scene.edge(&f.id).is_none();
    let mut out = Vec::new();
    let anchors: Vec<(f64, Point)> = if is_port && f.route.len() == 2 {
        vec![(0.5, f.route[1])]
    } else {
        [0.5, 0.35, 0.65, 0.2, 0.8]
            .into_iter()
            .map(|arc| (arc, crate::render::at_arc(&f.route, arc)))
            .collect()
    };
    for (arc, anchor) in anchors {
        if !is_port && is_vertical_at_arc(&f.route, arc) {
            push_vertical_edge_callouts(f, anchor, (w, h), arc, 200., true, &mut out);
        }
        for &radius in radii {
            for k in 0..12 {
                let angle = (k as f64) * std::f64::consts::TAU / 12.;
                let dx = angle.cos();
                let dy = angle.sin();
                let bounds = Rect::new(anchor.x + dx * radius, anchor.y + dy * radius, w, h);
                let dist = bounds.center().distance(anchor);
                if dist > 160. {
                    continue;
                }
                for (orthogonal, extra_cost) in [(false, 0.), (true, 2.)] {
                    out.push(Candidate {
                        label: LabelPlacement {
                            feature: f.id.clone(),
                            text: f.text.clone(),
                            bounds,
                            anchor,
                            knockout: false,
                            callout: true,
                            orthogonal_leader: orthogonal,
                            horizontal: dx.abs() >= dy.abs(),
                            slide: arc,
                        },
                        cost: 200.
                            + extra_cost
                            + 4. * dist
                            + if dist > 96. { 40. } else { 0. }
                            + (arc - 0.5).abs() * 12.,
                    });
                }
            }
        }
    }
    out
}
/// Snug local pockets around every route segment: caption centres snapped so
/// the caption clearance just touches nearby route segments, node faces and
/// port boxes. Dense cyclic layouts often leave exactly one caption-high band
/// between two parallel routes, which fixed-radius rings step over.
fn pocket_candidates(f: &Feature, scene: &SceneGraph, l: &Layout) -> Vec<Candidate> {
    const REACH: f64 = 156.;
    const KEEP: usize = 24;
    let w = feature_width(f, scene);
    let h = text_height(&f.text);
    let lengths: Vec<f64> = f.route.windows(2).map(|p| p[0].distance(p[1])).collect();
    let total = lengths.iter().sum::<f64>().max(1e-9);
    let mut anchors: Vec<(f64, Point)> = Vec::new();
    let mut arc = 0.;
    for (i, p) in f.route.windows(2).enumerate() {
        let len = lengths[i];
        if len > 1e-6 {
            for t in [0.5, 0.25, 0.75] {
                anchors.push((
                    (arc + len * t) / total,
                    Point::new(
                        p[0].x + (p[1].x - p[0].x) * t,
                        p[0].y + (p[1].y - p[0].y) * t,
                    ),
                ));
            }
        }
        arc += len;
    }
    let solid: Vec<Rect> = scene
        .nodes
        .iter()
        .filter(|n| !n.container)
        .map(|n| l.nodes[&n.id])
        .collect();
    let segments: Vec<(Point, Point)> = l
        .routes
        .values()
        .flat_map(|r| r.points.windows(2).map(|p| (p[0], p[1])))
        .collect();
    let mut out = Vec::new();
    for (slide, anchor) in anchors {
        let region = Rect::new(anchor.x, anchor.y, 2. * REACH + 2. * w, 2. * REACH + 2. * h);
        let near_solid: Vec<Rect> = solid
            .iter()
            .copied()
            .filter(|r| region.intersects(*r))
            .collect();
        let near_segments: Vec<(Point, Point)> = segments
            .iter()
            .copied()
            .filter(|&(a, b)| segment_hits(a, b, region))
            .collect();
        let mut xs = vec![anchor.x, anchor.x - w / 2. - 8., anchor.x + w / 2. + 8.];
        let mut ys = vec![anchor.y, anchor.y - h / 2. - 8., anchor.y + h / 2. + 8.];
        let mut raw_xs = vec![anchor.x];
        let mut raw_ys = vec![anchor.y];
        let add_x = |x: f64, xs: &mut Vec<f64>| {
            xs.push(x - w / 2. - 2.);
            xs.push(x + w / 2. + 2.);
        };
        let add_y = |y: f64, ys: &mut Vec<f64>| {
            ys.push(y - h / 2. - 2.);
            ys.push(y + h / 2. + 2.);
        };
        for r in &near_solid {
            add_x(r.left(), &mut xs);
            add_x(r.right(), &mut xs);
            add_y(r.top(), &mut ys);
            add_y(r.bottom(), &mut ys);
            raw_xs.push(r.left());
            raw_xs.push(r.right());
            raw_ys.push(r.top());
            raw_ys.push(r.bottom());
        }
        for &(a, b) in &near_segments {
            if (a.x - b.x).abs() < 1e-6 {
                add_x(a.x, &mut xs);
                raw_xs.push(a.x);
            }
            if (a.y - b.y).abs() < 1e-6 {
                add_y(a.y, &mut ys);
                raw_ys.push(a.y);
            }
        }
        for p in l.ports.values() {
            if region.contains(p.point) {
                add_x(p.point.x - 4., &mut xs);
                add_x(p.point.x + 4., &mut xs);
                add_y(p.point.y - 4., &mut ys);
                add_y(p.point.y + 4., &mut ys);
                raw_xs.push(p.point.x);
                raw_ys.push(p.point.y);
            }
        }
        raw_xs.sort_by(|a, b| a.total_cmp(b));
        raw_xs.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
        for pair in raw_xs.windows(2) {
            let mid = (pair[0] + pair[1]) / 2.;
            if (mid - anchor.x).abs() <= REACH {
                xs.push(mid);
            }
        }
        for &ox in &raw_xs {
            let mid = (anchor.x + ox) / 2.;
            if (mid - anchor.x).abs() <= REACH {
                xs.push(mid);
            }
        }
        raw_ys.sort_by(|a, b| a.total_cmp(b));
        raw_ys.dedup_by(|a, b| (*a - *b).abs() < 1e-3);
        for pair in raw_ys.windows(2) {
            let mid = (pair[0] + pair[1]) / 2.;
            if (mid - anchor.y).abs() <= REACH {
                ys.push(mid);
            }
        }
        for &oy in &raw_ys {
            let mid = (anchor.y + oy) / 2.;
            if (mid - anchor.y).abs() <= REACH {
                ys.push(mid);
            }
        }
        let nearest = |values: &mut Vec<f64>, centre: f64| {
            values.retain(|v| (v - centre).abs() <= REACH);
            values.sort_by(|a, b| (a - centre).abs().total_cmp(&(b - centre).abs()));
            let mut kept: Vec<f64> = Vec::new();
            for &v in values.iter() {
                if kept.iter().all(|k| (k - v).abs() > 0.25) {
                    kept.push(v);
                }
                if kept.len() == KEEP {
                    break;
                }
            }
            *values = kept;
        };
        nearest(&mut xs, anchor.x);
        nearest(&mut ys, anchor.y);
        for &y in &ys {
            if let Some(slot) = f.slot.as_deref()
                && ((slot == "above" && y >= anchor.y) || (slot == "below" && y <= anchor.y))
            {
                continue;
            }
            for &x in &xs {
                let bounds = Rect::new(x, y, w, h);
                let dist = bounds.center().distance(anchor);
                let r = bounds.inflate(2.);
                if dist > REACH
                    || r.contains(anchor)
                    || near_solid.iter().any(|n| r.intersects(*n))
                    || near_segments.iter().any(|&(a, b)| r.blocks(a, b))
                {
                    continue;
                }
                let gap = box_to_point_dist(bounds, anchor);
                let label = LabelPlacement {
                    feature: f.id.clone(),
                    text: f.text.clone(),
                    bounds,
                    anchor,
                    knockout: false,
                    callout: false,
                    orthogonal_leader: false,
                    horizontal: (anchor.y - y).abs() < 1e-6,
                    slide,
                };
                if gap <= 14. {
                    out.push(Candidate {
                        label,
                        cost: 60. + 2. * gap + (slide - 0.5).abs() * 12.,
                    });
                    continue;
                }
                for (orthogonal_leader, extra) in [(true, 0.), (false, 2.)] {
                    let mut label = label.clone();
                    label.callout = true;
                    label.orthogonal_leader = orthogonal_leader;
                    out.push(Candidate {
                        label,
                        cost: 200.
                            + extra
                            + 4. * dist
                            + if dist > 96. { 40. } else { 0. }
                            + (slide - 0.5).abs() * 12.,
                    });
                }
            }
        }
    }
    out
}
const RETRY_PER_FEATURE: usize = 32;
const RETRY_BUDGET: usize = 2_500;
#[allow(clippy::too_many_arguments)]
fn retry_component(
    component: &[usize],
    features: &[Feature],
    scene: &SceneGraph,
    l: &Layout,
    node_index: &RTree<Indexed>,
    prior_none: bool,
    already_selected: &[usize],
    by_feature: &mut [Vec<usize>],
    all: &mut Vec<Candidate>,
) -> Vec<usize> {
    let (core_left, core_right) = core_x_span(l);
    let attempts: [(&[f64], f64, f64); 2] = [
        (&[36., 56., 80., 96., 120., 144., 158.], 8., 4.),
        (&[28., 44., 64., 88., 112., 136., 152., 159.], 2., 0.),
    ];
    for (radii, port_box, leader_node_pad) in attempts {
        for &fi in component {
            let f = &features[fi];
            if f.position.is_some() {
                continue;
            }
            // Validate ring candidates cheapest-first and keep at most
            // RETRY_PER_FEATURE obstacle-free additions per feature.
            let mut ring = dense_callout_ring(f, scene, radii);
            ring.sort_by(|a, b| a.cost.total_cmp(&b.cost));
            let mut added = 0usize;
            for mut c in ring {
                if added >= RETRY_PER_FEATURE {
                    break;
                }
                if candidate_valid(
                    &c,
                    f,
                    scene,
                    l,
                    node_index,
                    prior_none,
                    (port_box, leader_node_pad),
                ) && already_selected
                    .iter()
                    .all(|&sel| !conflict(&all[sel].label, &c.label))
                {
                    c.cost += candidate_penalty(&c, f, scene, l, prior_none)
                        + horizontal_overhang_penalty(c.label.bounds, core_left, core_right);
                    by_feature[fi].push(all.len());
                    all.push(c);
                    added += 1;
                }
            }
            let min_cost = by_feature[fi]
                .iter()
                .map(|id| all[*id].cost)
                .reduce(f64::min)
                .unwrap_or(0.);
            if min_cost < 0. {
                for &id in &by_feature[fi] {
                    all[id].cost -= min_cost;
                }
            }
            for &id in &by_feature[fi] {
                if all[id].cost < 0. {
                    all[id].cost = 0.;
                }
            }
            by_feature[fi].sort_by(|a, b| all[*a].cost.total_cmp(&all[*b].cost));
        }
        let mut comp_entries = Vec::new();
        for &fi in component {
            for &id in &by_feature[fi] {
                comp_entries.push(Indexed {
                    id,
                    bounds: footprint(&all[id].label),
                });
            }
        }
        let mut owner = std::collections::BTreeMap::new();
        for &fi in component {
            for &id in &by_feature[fi] {
                owner.insert(id, fi);
            }
        }
        let comp_index = RTree::bulk_load(comp_entries);
        let mut local_conflicts = vec![BTreeSet::new(); all.len()];
        for &fi in component {
            for &id in &by_feature[fi] {
                let fp = footprint(&all[id].label);
                for hit in comp_index.locate_in_envelope_intersecting(&envelope(fp)) {
                    if owner.get(&hit.id) != Some(&fi)
                        && conflict(&all[id].label, &all[hit.id].label)
                    {
                        local_conflicts[id].insert(hit.id);
                    }
                }
            }
        }
        let mut sorted = component.to_vec();
        sorted.sort_by_key(|fi| by_feature[*fi].len());
        let sol = select_component(&sorted, by_feature, all, &local_conflicts, RETRY_BUDGET);
        if sol.len() == component.len() {
            return sol;
        }
    }
    Vec::new()
}
fn valid(
    label: &LabelPlacement,
    f: &Feature,
    scene: &SceneGraph,
    l: &Layout,
    index: &RTree<Indexed>,
) -> bool {
    valid_with_port_box(label, f, scene, l, index, 8.)
}
fn valid_with_port_box(
    label: &LabelPlacement,
    f: &Feature,
    scene: &SceneGraph,
    l: &Layout,
    index: &RTree<Indexed>,
    port_box: f64,
) -> bool {
    let r = clearance(label);
    let path = leader_path(label);
    let mut segments: Vec<(Point, Point)> = path.windows(2).map(|p| (p[0], p[1])).collect();
    let path_len = segments.len();
    if path.len() > 2 {
        segments.push((path[0], path[path.len() - 1]));
    }
    // The rendered path and its measured chord never enter the caption body.
    let body = label.bounds.inflate(-0.5);
    if segments
        .iter()
        .any(|&(p, q)| p.distance(q) > 1e-9 && segment_hits(p, q, body))
    {
        return false;
    }
    let any_hits = |rect: Rect| segments.iter().any(|&(p, q)| segment_hits(p, q, rect));
    let path_hits = |rect: Rect| {
        segments[..path_len.min(segments.len())]
            .iter()
            .any(|&(p, q)| segment_hits(p, q, rect))
    };
    for hit in index.locate_in_envelope_intersecting(&envelope(footprint(label))) {
        let n = &scene.nodes[hit.id];
        if f.ignore.contains(&n.id) {
            if n.id != f.id
                && let Some(header) =
                    crate::layout::container_header_rect_with_ports(n, hit.bounds, &l.ports)
                && (r.intersects(header) || any_hits(header))
            {
                return false;
            }
        } else if f.crossed.contains(&n.id) {
            let opaque_flow_pill = !label.callout
                && scene
                    .edge(&f.id)
                    .is_some_and(|e| crate::render::flow_caption(scene, e).is_some());
            if !opaque_flow_pill && r.intersects(hit.bounds) && !hit.bounds.inflate(-6.).encloses(r)
            {
                return false;
            }
            if let Some(header) =
                crate::layout::container_header_rect_with_ports(n, hit.bounds, &l.ports)
                && (r.intersects(header) || any_hits(header))
            {
                return false;
            }
        } else if r.intersects(hit.bounds)
            || path_hits(hit.bounds)
            || (!n.container && any_hits(node_title_rect(n, hit.bounds)))
        {
            return false;
        }
    }
    if port_box > 0.
        && l.ports
            .values()
            .any(|p| r.intersects(Rect::new(p.point.x, p.point.y, port_box, port_box)))
    {
        return false;
    }
    for route in l.routes.values() {
        if label.knockout && !label.callout && route.edge == f.id {
            continue;
        }
        if route.points.windows(2).any(|s| r.blocks(s[0], s[1])) {
            return false;
        }
    }
    true
}
fn select_component(
    component: &[usize],
    by_feature: &[Vec<usize>],
    all: &[Candidate],
    conflicts: &[BTreeSet<usize>],
    budget: usize,
) -> Vec<usize> {
    let mut in_comp = vec![false; all.len()];
    for &f in component {
        for &id in &by_feature[f] {
            in_comp[id] = true;
        }
    }
    let mut adj = vec![Vec::new(); all.len()];
    for &f in component {
        for &id in &by_feature[f] {
            adj[id] = conflicts[id]
                .iter()
                .copied()
                .filter(|&other| in_comp[other])
                .collect();
        }
    }
    let sum_min_cost: f64 = component
        .iter()
        .filter_map(|&f| by_feature[f].first().map(|&id| all[id].cost))
        .sum();

    // Initial MRV greedy solution.
    let mut greedy = Vec::new();
    let mut greedy_blocked = vec![0u32; all.len()];
    let mut remaining: Vec<usize> = component.to_vec();
    while !remaining.is_empty() {
        let mut min_count = usize::MAX;
        let mut best_idx = 0;
        for (idx, &f) in remaining.iter().enumerate() {
            let count = by_feature[f]
                .iter()
                .filter(|&&id| greedy_blocked[id] == 0)
                .count();
            if count < min_count {
                min_count = count;
                best_idx = idx;
            }
        }
        if min_count == 0 {
            break;
        }
        let f = remaining.remove(best_idx);
        if let Some(&id) = by_feature[f].iter().find(|&&id| greedy_blocked[id] == 0) {
            greedy.push(id);
            for &other in &adj[id] {
                greedy_blocked[other] += 1;
            }
        } else {
            break;
        }
    }
    if greedy.len() == component.len() {
        let greedy_cost: f64 = greedy.iter().map(|&i| all[i].cost).sum();
        if greedy_cost <= sum_min_cost + 1e-6 {
            return greedy;
        }
    }
    struct Search<'a> {
        by_feature: &'a [Vec<usize>],
        all: &'a [Candidate],
        adj: &'a [Vec<usize>],
        blocked: Vec<u32>,
        best: Vec<usize>,
        cost: f64,
        budget: usize,
        root_lower_bound: f64,
    }
    impl Search<'_> {
        fn visit(&mut self, remaining: &[usize], cost: f64, chosen: &mut Vec<usize>) {
            if self.budget == 0 || self.cost <= self.root_lower_bound + 1e-6 {
                return;
            }
            self.budget -= 1;
            if remaining.is_empty() {
                if cost < self.cost {
                    self.best = chosen.clone();
                    self.cost = cost;
                }
                return;
            }
            // Recompute the most constrained feature after every choice.
            // A cheap early caption must not strand a later feature or leave
            // a partial greedy assignment as the search's final result.
            let mut selected = 0;
            let mut min_count = usize::MAX;
            let mut lower = 0.;
            for (at, &feature) in remaining.iter().enumerate() {
                let mut count = 0usize;
                let mut cheapest = f64::INFINITY;
                for &id in &self.by_feature[feature] {
                    if self.blocked[id] == 0 {
                        if count == 0 {
                            cheapest = self.all[id].cost;
                        }
                        count += 1;
                    }
                }
                if count == 0 {
                    return;
                }
                lower += cheapest;
                if count < min_count {
                    min_count = count;
                    selected = at;
                }
            }
            if cost + lower >= self.cost {
                return;
            }
            let target_feature = remaining[selected];
            let choices: Vec<usize> = self.by_feature[target_feature]
                .iter()
                .copied()
                .filter(|&id| self.blocked[id] == 0)
                .collect();
            let mut next = Vec::with_capacity(remaining.len() - 1);
            for (at, &f) in remaining.iter().enumerate() {
                if at != selected {
                    next.push(f);
                }
            }
            for id in choices {
                if cost + self.all[id].cost >= self.cost {
                    break;
                }
                chosen.push(id);
                for &other in &self.adj[id] {
                    self.blocked[other] += 1;
                }
                self.visit(&next, cost + self.all[id].cost, chosen);
                for &other in &self.adj[id] {
                    self.blocked[other] -= 1;
                }
                chosen.pop();
            }
        }
    }
    let cost = if greedy.len() == component.len() {
        greedy.iter().map(|&i| all[i].cost).sum()
    } else {
        f64::INFINITY
    };
    let mut search = Search {
        by_feature,
        all,
        adj: &adj,
        blocked: vec![0; all.len()],
        best: if greedy.len() == component.len() {
            greedy
        } else {
            Vec::new()
        },
        cost,
        budget,
        root_lower_bound: sum_min_cost,
    };
    search.visit(component, 0., &mut Vec::new());
    search.best
}
/// Wrap at word boundaries using the same glyph estimates as geometry. Explicit
/// newlines remain hard breaks; source labels and accessibility text stay intact.
pub(crate) fn wrap_text(text: &str, width: f64) -> String {
    text.split('\n')
        .map(|line| {
            if text_width(line) <= width {
                return line.to_owned();
            }
            let mut lines = Vec::new();
            let mut current = String::new();
            for word in line.split_whitespace() {
                let next = if current.is_empty() {
                    word.to_owned()
                } else {
                    format!("{current} {word}")
                };
                if !current.is_empty() && text_width(&next) > width {
                    lines.push(current);
                    current = word.to_owned();
                } else {
                    current = next;
                }
            }
            lines.push(current);
            lines.join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n")
}
pub(crate) fn node_title(node: &Node) -> String {
    if node.container {
        node.label.clone()
    } else {
        wrap_text(&node.label, 180.)
    }
}
pub(crate) fn connection_text(scene: &SceneGraph, edge: &Edge) -> String {
    wrap_text(
        &crate::render::flow_caption(scene, edge).unwrap_or_else(|| edge.label.clone()),
        180.,
    )
}
pub fn text_height(text: &str) -> f64 {
    text.split('\n').count() as f64 * 18. + 6.
}
pub fn text_width(text: &str) -> f64 {
    text.split('\n')
        .map(|line| {
            line.chars()
                .map(|c| match c {
                    'W' | '@' | '%' => 13.,
                    'M' => 12.5,
                    'm' => 12.,
                    'w' => 11.,
                    'i' | 'l' | 'j' | 'f' | 't' | 'r' => 5.5,
                    'I' => 6.,
                    '.' | ',' | ':' | ';' | '!' | '\'' | ' ' => 4.5,
                    '=' | '+' | '^' | '~' | '<' | '>' => 11.5,
                    c if c.is_ascii() => 10.,
                    _ => 16.,
                })
                .sum::<f64>()
                * 15.
                / 13.
        })
        .fold(0., f64::max)
}
/// Exact signed distance between axis-aligned rectangles (negative under overlap).
pub fn rectangle_sdf(a: Rect, b: Rect) -> f64 {
    let dx = (a.x - b.x).abs() - (a.w + b.w) / 2.;
    let dy = (a.y - b.y).abs() - (a.h + b.h) / 2.;
    dx.max(0.).hypot(dy.max(0.)) + dx.max(dy).min(0.)
}
fn smooth_sdf_dual(ax: &Dual, ay: &Dual, a: Rect, bx: &Dual, by: &Dual, b: Rect) -> Dual {
    let n = ax.gradient.len();
    let c = |v| Dual::constant(v, n);
    let abs = |z: Dual| z.square().add(&c(1e-6)).sqrt();
    let dx = abs(ax.sub(bx)).sub(&c((a.w + b.w) / 2.));
    let dy = abs(ay.sub(by)).sub(&c((a.h + b.h) / 2.));
    let px = dx.softplus(2.);
    let py = dy.softplus(2.);
    let outside = px.square().add(&py.square()).add(&c(1e-8)).sqrt();
    let max = dx.add(&dy).add(&abs(dx.sub(&dy))).scale(0.5);
    outside.add(&max.sub(&max.softplus(2.)))
}
fn smooth_sdf(x: &Dual, y: &Dual, a: Rect, b: Rect) -> Dual {
    let n = x.gradient.len();
    smooth_sdf_dual(x, y, a, &Dual::constant(b.x, n), &Dual::constant(b.y, n), b)
}
fn segment_rect(a: Point, b: Point) -> Option<Rect> {
    if a.distance(b) < 1e-6 {
        return None;
    }
    Some(Rect::new(
        (a.x + b.x) / 2.,
        (a.y + b.y) / 2.,
        (a.x - b.x).abs().max(2.),
        (a.y - b.y).abs().max(2.),
    ))
}
fn push_static_obstacles(
    out: &mut Vec<Rect>,
    region: Rect,
    scene: &SceneGraph,
    l: &Layout,
    self_feature: &str,
) {
    for node in &scene.nodes {
        let nr = l.nodes[&node.id];
        if !node.container {
            if region.intersects(nr) {
                out.push(nr);
            }
            if node.id != self_feature
                && let Some(tr) = sql_table_title_rect(node, nr)
                && region.intersects(tr)
            {
                out.push(tr);
            }
        } else if node.id != self_feature
            && let Some(hr) = container_header_rect_with_ports(node, nr, &l.ports)
            && region.intersects(hr)
        {
            out.push(hr);
        }
    }
    for (pkey, port) in &l.ports {
        if pkey == self_feature {
            continue;
        }
        let pr = Rect::new(port.point.x, port.point.y, 8., 8.);
        if region.intersects(pr) {
            out.push(pr);
        }
    }
}
fn polish(labels: &mut [LabelPlacement], scene: &SceneGraph, l: &Layout, o: &LayoutOptions) {
    if labels.is_empty() || o.polish_steps == 0 {
        return;
    }
    let original = labels.to_vec();
    let n = labels.len();
    let manual: Vec<bool> = labels
        .iter()
        .map(|label| {
            scene.edge(&label.feature).is_some_and(|edge| {
                effective_annotations(scene, &edge.id, &edge.annotations, o.variant.as_deref())
                    .iter()
                    .any(|a| {
                        a.name == "label"
                            && (a.attrs.contains_key("x") || a.attrs.contains_key("y"))
                    })
            })
        })
        .collect();
    let bounds: Vec<_> = labels
        .iter()
        .enumerate()
        .map(|(i, a)| {
            if a.callout || manual[i] {
                (0., 0.)
            } else {
                (-6., 6.)
            }
        })
        .collect();
    let mut nearby_obstacles: Vec<Vec<Rect>> = Vec::with_capacity(n);
    for label in &original {
        let region = label.bounds.inflate(28.);
        let mut obs = Vec::new();
        push_static_obstacles(&mut obs, region, scene, l, &label.feature);
        for (eid, route) in &l.routes {
            // Explicitly exclude self-edge repulsion between an edge and its own attached label.
            if *eid == label.feature {
                continue;
            }
            for seg in crate::routing::coalesced(&route.points).windows(2) {
                if let Some(sr) = segment_rect(seg[0], seg[1])
                    && region.intersects(sr)
                {
                    obs.push(sr);
                }
            }
        }
        nearby_obstacles.push(obs);
    }
    let result = solver::lbfgs(&vec![0.; n], &bounds, o.polish_steps, |vars| {
        let mut energy = Dual::constant(0., n);
        let mut xs = Vec::with_capacity(n);
        let mut ys = Vec::with_capacity(n);
        for (i, label) in original.iter().enumerate() {
            let x = if label.horizontal {
                vars[i].add(&Dual::constant(label.bounds.x, n))
            } else {
                Dual::constant(label.bounds.x, n)
            };
            let y = if label.horizontal {
                Dual::constant(label.bounds.y, n)
            } else {
                vars[i].add(&Dual::constant(label.bounds.y, n))
            };
            energy = energy.add(&vars[i].square().scale(0.1));
            xs.push(x);
            ys.push(y);
        }
        for (i, label) in original.iter().enumerate() {
            for &obs in &nearby_obstacles[i] {
                let sdf = smooth_sdf(&xs[i], &ys[i], label.bounds, obs);
                let penalty = Dual::constant(6., n).sub(&sdf).softplus(2.).square();
                energy = energy.add(&penalty);
            }
            for j in (i + 1)..n {
                let other = &original[j];
                if !label.bounds.inflate(28.).intersects(other.bounds) {
                    continue;
                }
                let sdf =
                    smooth_sdf_dual(&xs[i], &ys[i], label.bounds, &xs[j], &ys[j], other.bounds);
                energy = energy.add(&Dual::constant(6., n).sub(&sdf).softplus(2.).square());
            }
        }
        energy
    });
    let index = RTree::bulk_load(
        scene
            .nodes
            .iter()
            .enumerate()
            .map(|(id, n)| Indexed {
                id,
                bounds: l.nodes[&n.id],
            })
            .collect(),
    );
    for i in 0..n {
        if manual[i] {
            continue;
        }
        let mut candidate = labels[i].clone();
        if candidate.horizontal {
            candidate.bounds.x += result[i];
        } else {
            candidate.bounds.y += result[i];
        }
        let (ignore, crossed) = feature_scopes(scene, &candidate.feature);
        let f = Feature {
            id: candidate.feature.clone(),
            text: candidate.text.clone(),
            route: Vec::new(),
            ignore,
            crossed,
            slot: None,
            slide: None,
            position: None,
        };
        if valid(&candidate, &f, scene, l, &index)
            && labels
                .iter()
                .enumerate()
                .all(|(j, b)| i == j || !conflict(&candidate, b))
        {
            labels[i] = candidate;
        }
    }
}

pub(crate) fn interior_micro_jog_count(pts: &[Point], threshold: f64) -> usize {
    let c = crate::routing::coalesced(pts);
    (1..c.len().saturating_sub(2))
        .filter(|&k| c[k].distance(c[k + 1]) < threshold)
        .count()
}

/// Polish safe floating route lanes after discrete label selection. The line search is followed
/// by exact obstacle/label checks, so a smooth approximation never weakens hard geometry.
pub fn polish_channels(scene: &SceneGraph, o: &LayoutOptions, l: &mut Layout) {
    if l.labels.is_empty() || o.polish_steps == 0 {
        return;
    }
    struct Lane {
        edge: String,
        index: usize,
        horizontal: bool,
        rect: Rect,
        bound: f64,
    }
    let mut lanes = Vec::new();
    for e in &scene.edges {
        if l.labels
            .iter()
            .any(|label| label.feature == e.id && label.callout)
        {
            continue;
        }
        if effective_annotations(scene, &e.id, &e.annotations, o.variant.as_deref())
            .iter()
            .any(|a| a.name == "route")
        {
            continue;
        }
        let route = &l.routes[&e.id];
        for i in 2..route.points.len().saturating_sub(3) {
            let (a, b) = (route.points[i], route.points[i + 1]);
            let horizontal = (a.y - b.y).abs() < 1e-6;
            let prev = route.points[i - 1];
            let next = route.points[i + 2];
            if horizontal && ((prev.x - a.x).abs() > 1e-6 || (next.x - b.x).abs() > 1e-6)
                || !horizontal && ((prev.y - a.y).abs() > 1e-6 || (next.y - b.y).abs() > 1e-6)
            {
                continue;
            }
            let rect = Rect::new(
                (a.x + b.x) / 2.,
                (a.y + b.y) / 2.,
                (a.x - b.x).abs().max(1.),
                (a.y - b.y).abs().max(1.),
            );
            let mut bound = 8.0f64;
            for (other_id, other) in &l.routes {
                if other_id == &e.id {
                    continue;
                }
                for p in other.points.windows(2) {
                    let h = (p[0].y - p[1].y).abs() < 1e-6;
                    if h != horizontal {
                        continue;
                    }
                    let overlap = if horizontal {
                        a.x.max(b.x) > p[0].x.min(p[1].x) && a.x.min(b.x) < p[0].x.max(p[1].x)
                    } else {
                        a.y.max(b.y) > p[0].y.min(p[1].y) && a.y.min(b.y) < p[0].y.max(p[1].y)
                    };
                    if overlap {
                        let distance = if horizontal {
                            (a.y - p[0].y).abs()
                        } else {
                            (a.x - p[0].x).abs()
                        };
                        bound = bound.min(((distance - o.lane_spacing) / 2.).max(0.));
                    }
                }
            }
            lanes.push(Lane {
                edge: e.id.clone(),
                index: i,
                horizontal,
                rect,
                bound,
            });
        }
    }
    if lanes.is_empty() {
        return;
    }
    let n = lanes.len();
    let lane_keys: BTreeSet<(&str, usize)> = lanes
        .iter()
        .map(|lane| (lane.edge.as_str(), lane.index))
        .collect();
    let mut lane_obstacles: Vec<Vec<Rect>> = Vec::with_capacity(n);
    for lane in &lanes {
        let region = lane.rect.inflate(lane.bound + 28.);
        let mut obs = Vec::new();
        // Exclude self-edge label repulsion between `lane.edge` and its own attached label.
        for label in &l.labels {
            if label.feature == lane.edge {
                continue;
            }
            if region.intersects(label.bounds) {
                obs.push(label.bounds);
            }
        }
        push_static_obstacles(&mut obs, region, scene, l, &lane.edge);
        for (other_id, other_route) in &l.routes {
            if *other_id == lane.edge {
                continue;
            }
            for (seg_idx, pair) in other_route.points.windows(2).enumerate() {
                if lane_keys.contains(&(other_id.as_str(), seg_idx)) {
                    continue;
                }
                if let Some(sr) = segment_rect(pair[0], pair[1])
                    && region.intersects(sr)
                {
                    obs.push(sr);
                }
            }
        }
        lane_obstacles.push(obs);
    }
    let bounds: Vec<_> = lanes.iter().map(|s| (-s.bound, s.bound)).collect();
    let result = solver::lbfgs(&vec![0.; n], &bounds, o.polish_steps, |vars| {
        let mut energy = Dual::constant(0., n);
        let mut xs = Vec::with_capacity(n);
        let mut ys = Vec::with_capacity(n);
        for (i, lane) in lanes.iter().enumerate() {
            let x = if lane.horizontal {
                Dual::constant(lane.rect.x, n)
            } else {
                vars[i].add(&Dual::constant(lane.rect.x, n))
            };
            let y = if lane.horizontal {
                vars[i].add(&Dual::constant(lane.rect.y, n))
            } else {
                Dual::constant(lane.rect.y, n)
            };
            energy = energy.add(&vars[i].square().scale(o.weights.symmetry));
            xs.push(x);
            ys.push(y);
        }
        for (i, lane) in lanes.iter().enumerate() {
            for &obs in &lane_obstacles[i] {
                let sdf = smooth_sdf(&xs[i], &ys[i], lane.rect, obs);
                energy = energy.add(&Dual::constant(6., n).sub(&sdf).softplus(2.).square());
            }
            for j in (i + 1)..n {
                let other = &lanes[j];
                if other.edge == lane.edge || !lane.rect.inflate(28.).intersects(other.rect) {
                    continue;
                }
                let sdf = smooth_sdf_dual(&xs[i], &ys[i], lane.rect, &xs[j], &ys[j], other.rect);
                energy = energy.add(&Dual::constant(6., n).sub(&sdf).softplus(2.).square());
            }
        }
        energy
    });
    let threshold = o.grid.max(1.) - 1e-6;
    let mut edge_lanes: std::collections::BTreeMap<&str, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (li, lane) in lanes.iter().enumerate() {
        edge_lanes.entry(lane.edge.as_str()).or_default().push(li);
    }
    let mut candidate = l.clone();
    for (edge_id, idxs) in edge_lanes {
        let Some(orig_route) = l.routes.get(edge_id) else {
            continue;
        };
        let Some(edge) = scene.edges.iter().find(|e| e.id == edge_id) else {
            continue;
        };
        let before_count = interior_micro_jog_count(&orig_route.points, threshold);
        let mut accepted = None;
        for scale in [1.0, 0.5, 0.25] {
            let mut trial = orig_route.clone();
            for &li in &idxs {
                let lane = &lanes[li];
                let delta = result[li] * scale;
                for point in &mut trial.points[lane.index..=lane.index + 1] {
                    if lane.horizontal {
                        point.y += delta;
                    } else {
                        point.x += delta;
                    }
                }
            }
            if interior_micro_jog_count(&trial.points, threshold) <= before_count
                && crate::routing::route_is_valid(scene, &candidate, edge, &trial, o.margin)
                && crate::routing::route_conflict_counts(&candidate, edge_id, &trial.points).1 == 0
            {
                accepted = Some(trial);
                break;
            }
        }
        if let Some(trial) = accepted {
            candidate.routes.insert(edge_id.to_string(), trial);
        }
    }
    if crate::layout::validate(scene, &candidate, o).is_err() {
        return;
    }
    for label in &candidate.labels {
        for route in candidate.routes.values() {
            if label.knockout && !label.callout && label.feature == route.edge {
                continue;
            }
            if route
                .points
                .windows(2)
                .any(|p| clearance(label).blocks(p[0], p[1]))
            {
                return;
            }
        }
    }
    *l = candidate;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc001_tiers_keep_spacious_external_slots_and_use_inline_pills_in_dense_meshes() {
        // Spacious edge must choose flush external Tier 1 slot (`!knockout && !callout`).
        let spacious =
            crate::compile("a @pin(grid: 0, 0)\nb @pin(grid: 18, 0)\na.E -> b.W: payload\n")
                .unwrap();
        let lbl = spacious
            .layout
            .labels
            .iter()
            .find(|l| l.feature == "a->b")
            .unwrap();
        assert!(!lbl.knockout && !lbl.callout, "spacious edge got {lbl:?}");

        // Long horizontal segment where external top/bottom adjacent strips are blocked by
        // adjacent parallel lanes must pick Tier 2 centered inline pill (`knockout && !callout`),
        // outranking Tier 3 callout flagpoles (`>= 180`).
        let blocked = "\
a @pin(grid: 0, 0) {
 port p1 [side: E, pos: 1]
 port p2 [side: E, pos: 2.5]
 port p3 [side: E, pos: 4]
}
b @pin(grid: 24, 0) {
 port q1 [side: W, pos: 1]
 port q2 [side: W, pos: 2.5]
 port q3 [side: W, pos: 4]
}
a#p1 -> b#q1
a#p2 -> b#q2: crowded
a#p3 -> b#q3
";
        let dense = crate::compile(blocked).unwrap();
        let dlbl = dense
            .layout
            .labels
            .iter()
            .find(|l| l.feature == "a#p2->b#q2")
            .unwrap();
        assert!(
            dlbl.knockout && !dlbl.callout,
            "blocked corridor should prefer Tier 2 inline knockout pill over Tier 3 flagpole: got {dlbl:?}"
        );
    }

    #[test]
    fn manual_drag_within_32px_stays_non_callout_and_ports_stay_tight() {
        let d = crate::compile(
            "a @pin(grid: 0, 0)\nb @pin(grid: 20, 0)\na.E -> b.W: close\n@layout { (a.E -> b.W) @label(x: 160, y: 28) }\n",
        )
        .unwrap();
        let lbl = d
            .layout
            .labels
            .iter()
            .find(|l| l.feature == "a->b")
            .unwrap();
        assert!(
            !lbl.callout,
            "< 32px manual @label should not emit callout: {lbl:?}"
        );

        // Port caption distance must remain within 4..14px of port wall.
        let pd = crate::compile(
            "producer @pin(grid: 0, 0)\nbroker @pin(grid: 24, 0) {\n port ingress [side: W, ratio: 0.25]\n}\nproducer.E -> broker#ingress: publish\n",
        )
        .unwrap();
        let port = &pd.layout.ports["broker#ingress"];
        let plbl = pd
            .layout
            .labels
            .iter()
            .find(|l| l.feature == "broker#ingress")
            .unwrap();
        assert!(!plbl.callout);
        let gap_x = (port.point.x - plbl.bounds.right()).abs();
        assert!(
            (4. ..=14.1).contains(&gap_x),
            "port caption must stay 4..14px outside port along face normal, got {gap_x}"
        );
    }

    #[test]
    fn polish_excludes_self_edge_repulsion_while_keeping_valid_layout() {
        let diag = crate::compile(
            "box {\n a @pin(grid: 0, 0)\n b @pin(grid: 16, 0)\n a.E -> b.W: RPC\n}\n",
        )
        .unwrap();
        assert!(
            crate::layout::validate(&diag.scene, &diag.layout, &LayoutOptions::default()).is_ok()
        );
        assert!(diag.layout.labels.iter().all(|l| !l.callout));
    }

    #[test]
    fn stacked_ports_and_16px_record_lanes_assign_completely() {
        let records = crate::compile(include_str!(
            "../tests/fixtures/quality/09-many-record-slots.diagram"
        ))
        .unwrap();
        assert!(
            crate::layout::validate(&records.scene, &records.layout, &LayoutOptions::default())
                .is_ok()
        );
        assert!(
            records.layout.labels.iter().all(|l| !l.callout),
            "16px-pitched record slots must avoid callouts: {:?}",
            records.layout.labels
        );

        let duplex = crate::compile(include_str!(
            "../tests/fixtures/quality/10-many-named-ports.diagram"
        ))
        .unwrap();
        assert!(
            crate::layout::validate(&duplex.scene, &duplex.layout, &LayoutOptions::default())
                .is_ok()
        );
    }

    #[test]
    fn tall_callout_never_enters_bounds_or_chord() {
        let label = LabelPlacement {
            feature: "e".into(),
            text: "one\ntwo\nthree".into(),
            bounds: Rect::new(100., 100., 80., 60.),
            anchor: Point::new(100., 100. - 30. - 2.), // Anchor sitting right at the inflated top edge
            knockout: false,
            callout: true,
            orthogonal_leader: true,
            horizontal: false,
            slide: 0.5,
        };
        let path = leader_path(&label);
        assert!(!path.is_empty(), "leader path should be generated");
        let body = label.bounds.inflate(-0.5);
        for pair in path.windows(2) {
            assert!(
                !segment_hits(pair[0], pair[1], body),
                "leader segment {:?} hits body {:?}",
                pair,
                body
            );
        }
        let chord = (path[0], *path.last().unwrap());
        assert!(
            !segment_hits(chord.0, chord.1, body),
            "leader chord {:?} hits body {:?}",
            chord,
            body
        );

        // Diagonal corner placement
        let diag_label = LabelPlacement {
            feature: "e2".into(),
            text: "one\ntwo\nthree".into(),
            bounds: Rect::new(100., 100., 80., 60.),
            anchor: Point::new(100. + 40. + 16., 100. + 30. + 16.),
            knockout: false,
            callout: true,
            orthogonal_leader: true,
            horizontal: true,
            slide: 0.5,
        };
        let dpath = leader_path(&diag_label);
        for pair in dpath.windows(2) {
            assert!(!segment_hits(pair[0], pair[1], body));
        }
        let dchord = (dpath[0], *dpath.last().unwrap());
        assert!(!segment_hits(dchord.0, dchord.1, body));
    }

    #[test]
    fn test_10293847_cycle_feasible() {
        let src = r#"n0: "Worker 0" [shape: queue]
n1: "Event journal 1" [shape: queue]
n2: "Worker 2" [shape: diamond]
n3: "Regional gateway 3" [shape: cache]
n4: "Organization membership service 4" [shape: rect]
n5: "Organization membership service 5" [shape: rect]
n6: "Regional gateway 6" [shape: rect]
n7: "東京 API 7" [shape: queue]
n8: "東京 API 8" [shape: queue]
n0 -> n1: ack
n1 -> n2: read through
n2 -> n3: validate organization access
n3 -> n4: read through
n4 -> n5: send
n5 -> n6: retry
n6 -> n7: read through
n7 -> n8: ack
n8 -> n0: 
n5 -> n7: validate organization access
n4 -> n7: ack
n3 -> n5: send
n0 -> n6: ack
"#;
        let res = crate::compile(src);
        assert!(res.is_ok(), "compile failed: {:?}", res.err());
    }

    #[test]
    fn test_28_labeled_edge_dag_speed() {
        use std::time::Instant;
        let mut src = String::new();
        for i in 0..29 {
            src.push_str(&format!("n{i}: \"Node {i}\"\n"));
        }
        for i in 0..28 {
            src.push_str(&format!("n{i} -> n{}: \"label {i}\"\n", i + 1));
        }
        let diag = crate::compile(&src).unwrap();
        let o = LayoutOptions::default();

        let t0 = Instant::now();
        let labels = place(&diag.scene, &o, &diag.layout).unwrap();
        let elapsed = t0.elapsed();
        assert_eq!(labels.len(), 28);
        assert!(
            elapsed.as_millis() < 200,
            "place on 28-edge DAG took {:?}, expected < 200ms",
            elapsed
        );
    }

    #[test]
    fn polish_channels_preserves_micro_jog_counts_and_fuzz_dag_4_has_zero_lint_jogs() {
        let o = LayoutOptions::default();
        let threshold = o.grid.max(1.) - 1e-6;
        let no_polish = LayoutOptions {
            polish_steps: 0,
            ..o.clone()
        };
        let fixtures = [
            (
                "examples/ports.diagram",
                include_str!("../examples/ports.diagram"),
            ),
            (
                "examples/multi-slot-records.diagram",
                include_str!("../examples/multi-slot-records.diagram"),
            ),
            (
                "tests/fixtures/quality/09-many-record-slots.diagram",
                include_str!("../tests/fixtures/quality/09-many-record-slots.diagram"),
            ),
            (
                "tests/fixtures/quality/5eed1203-sparse-mesh.diagram",
                include_str!("../tests/fixtures/quality/5eed1203-sparse-mesh.diagram"),
            ),
        ];
        for (name, src) in fixtures {
            let pre = crate::compile_with(src, &no_polish, None).unwrap();
            let post = crate::compile(src).unwrap();
            let mut polished = pre.layout.clone();
            let mut lbls = pre.layout.labels.clone();
            polish(&mut lbls, &pre.scene, &pre.layout, &o);
            polished.labels = lbls;
            polish_channels(&pre.scene, &o, &mut polished);
            for (eid, post_route) in &post.layout.routes {
                let pre_count = pre
                    .layout
                    .routes
                    .get(eid)
                    .map_or(0, |r| interior_micro_jog_count(&r.points, threshold));
                let post_count = interior_micro_jog_count(&post_route.points, threshold);
                assert!(
                    post_count <= pre_count,
                    "{name} edge {eid}: post_polish ({post_count}) > pre_polish ({pre_count})"
                );
                let direct_count = polished
                    .routes
                    .get(eid)
                    .map_or(0, |r| interior_micro_jog_count(&r.points, threshold));
                assert!(
                    direct_count <= pre_count,
                    "{name} edge {eid}: direct polish_channels ({direct_count}) > pre_polish ({pre_count})"
                );
            }
        }

        // Exact `fuzz-dag-4` generated by `scripts/layout-audit.cjs:51-62` (seed `0x5eedc0de`).
        let fuzz_dag_4 = "\
n0: \"API 0\" [shape: cache]
n1: \"Worker 1\" [shape: rect]
n2: \"Event log 2\" [shape: queue]
n3: \"地域ゲートウェイ 3\" [shape: diamond]
n4: \"Cache 4\" [shape: cache]
n5: \"Validate request 5\" [shape: diamond]
n6: \"地域ゲートウェイ 6\" [shape: diamond]
n7: \"地域ゲートウェイ 7\" [shape: rect]
n0 -> n1: send
n1 -> n2: retry
n2 -> n3: retry
n3 -> n4: read through
n4 -> n5: read through
n5 -> n6: ack
n6 -> n7: read through
n1 -> n4: validate organization access
n1 -> n6: validate organization access
n1 -> n7: 
n4 -> n7: read through
n5 -> n7: validate organization access
";
        let d_fuzz = crate::compile(fuzz_dag_4).unwrap();
        let jog_findings: Vec<_> = crate::lint::lint(&d_fuzz)
            .into_iter()
            .filter(|f| f.code == "micro-jog")
            .collect();
        assert!(
            jog_findings.is_empty(),
            "expected 0 <12px micro-jog findings on fuzz-dag-4, got {jog_findings:?}"
        );
    }

    #[test]
    fn polish_channels_synthetic_guard_blocks_new_micro_jogs() {
        let src = "\
a @pin(grid: 0, 0)
b @pin(grid: 24, 0)
c @pin(grid: 0, 12)
d @pin(grid: 24, 12)
e @pin(grid: 0, 24)
f @pin(grid: 24, 24)
a.E -> b.W
c.E -> d.W
e.E -> f.W: push
";
        let mut diag = crate::compile(src).unwrap();
        let o = LayoutOptions::default();
        let threshold = o.grid.max(1.) - 1e-6;
        let ab_pts = &diag.layout.routes["a->b"].points;
        let a_port = ab_pts[0];
        let b_port = *ab_pts.last().unwrap();
        let x0 = a_port.x;
        let x1 = b_port.x;
        let xm = ((x0 + 16.) + (x1 - 16.)) * 0.5;
        let y0 = a_port.y;
        // Guarded route: `16.0px` vertical and return steps (`0` micro-jogs); any downward shift
        // on `y0 - 32.` shrinks `|(y0 - 32.) - (y0 - 16.)| = 16.0` below `threshold`.
        diag.layout.routes.insert(
            "a->b".into(),
            Route {
                edge: "a->b".into(),
                points: vec![
                    Point::new(x0, y0),
                    Point::new(x0 + 16., y0),
                    Point::new(x0 + 16., y0 - 32.),
                    Point::new(xm, y0 - 32.),
                    Point::new(xm, y0 - 16.),
                    Point::new(x1 - 16., y0 - 16.),
                    Point::new(x1 - 16., y0),
                    Point::new(x1, y0),
                ],
            },
        );
        // Safe peer route: `32.0px` vertical step (`0` micro-jogs) that can move freely without
        // creating a `< 16px` segment.
        let cd_pts = &diag.layout.routes["c->d"].points;
        let c_port = cd_pts[0];
        let d_port = *cd_pts.last().unwrap();
        let cxm = ((c_port.x + 16.) + (d_port.x - 16.)) * 0.5;
        let cy0 = c_port.y;
        diag.layout.routes.insert(
            "c->d".into(),
            Route {
                edge: "c->d".into(),
                points: vec![
                    Point::new(c_port.x, cy0),
                    Point::new(c_port.x + 16., cy0),
                    Point::new(c_port.x + 16., cy0 - 32.),
                    Point::new(cxm, cy0 - 32.),
                    Point::new(cxm, cy0),
                    Point::new(d_port.x - 16., cy0),
                    Point::new(d_port.x, cy0),
                ],
            },
        );
        // Add two foreign labels derived from `e->f` (`6.5px` above each route's `i=2` lane) so
        // `polish_channels` runs L-BFGS actively and pushes both lanes downward while passing hard
        // `6.0px` label clearance.
        let push_label = diag
            .layout
            .labels
            .iter()
            .find(|l| l.feature == "e->f")
            .cloned()
            .unwrap();
        let mut lbl_a = push_label.clone();
        lbl_a.bounds = Rect::new((x0 + 16. + xm) * 0.5, y0 - 48.5, 48., 20.);
        let mut lbl_c = push_label;
        lbl_c.bounds = Rect::new((c_port.x + 16. + cxm) * 0.5, cy0 - 48.5, 48., 20.);
        diag.layout.labels.retain(|l| l.feature != "e->f");
        diag.layout.labels.push(lbl_a);
        diag.layout.labels.push(lbl_c);
        assert!(crate::layout::validate(&diag.scene, &diag.layout, &o).is_ok());
        assert_eq!(
            interior_micro_jog_count(&diag.layout.routes["a->b"].points, threshold),
            0
        );
        polish_channels(&diag.scene, &o, &mut diag.layout);
        let pts_ab = &diag.layout.routes["a->b"].points;
        assert_eq!(
            interior_micro_jog_count(pts_ab, threshold),
            0,
            "polish_channels introduced a <16px interior micro-jog: {pts_ab:?}"
        );
        assert!(
            (pts_ab[2].y - (y0 - 32.)).abs() < 1e-9,
            "expected guarded segment to hold exact 16px step instead of shrinking: {pts_ab:?}"
        );
        let pts_cd = &diag.layout.routes["c->d"].points;
        assert!(
            pts_cd[2].y > cy0 - 32. + 1e-3,
            "expected unguarded 32px peer lane to polish downward cleanly: {pts_cd:?}"
        );
        assert_eq!(interior_micro_jog_count(pts_cd, threshold), 0);
    }
}
