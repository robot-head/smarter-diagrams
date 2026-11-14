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
    let vertical = if a.y >= r.top() && a.y <= r.bottom() {
        false
    } else if a.x >= r.left() && a.x <= r.right() {
        true
    } else {
        label.horizontal
    };
    let (bend, end) = if vertical {
        (
            Point::new(r.x, a.y),
            Point::new(r.x, if a.y < r.y { r.top() } else { r.bottom() }),
        )
    } else {
        (
            Point::new(a.x, r.y),
            Point::new(if a.x < r.x { r.left() } else { r.right() }, r.y),
        )
    };
    let mut path = vec![a, bend, end];
    path.dedup_by(|a, b| a.distance(*b) < 1e-7);
    path
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
    clearance(a).intersects(clearance(b))
        || leader_path(a)
            .windows(2)
            .any(|p| segment_hits(p[0], p[1], clearance(b)))
        || leader_path(b)
            .windows(2)
            .any(|p| segment_hits(p[0], p[1], clearance(a)))
        || (a.anchor.distance(b.anchor) > 1e-3
            && leader_path(a).windows(2).any(|pa| {
                leader_path(b)
                    .windows(2)
                    .any(|pb| segments_intersect(pa[0], pa[1], pb[0], pb[1]))
            }))
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
                    fixed.callout = old.callout || box_to_point_dist(fixed.bounds, fixed.anchor) >= 32.;
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
        let crossed_penalty =
            |c: &Candidate| candidate_penalty(c, f, scene, l, prior.is_none());
        let is_valid_candidate = |c: &Candidate| {
            candidate_valid(c, f, scene, l, &node_index, prior.is_none(), (8., 4.))
        };
        let mut ids = Vec::new();
        for mut c in candidates {
            if is_valid_candidate(&c) {
                c.cost += crossed_penalty(&c);
                ids.push(all.len());
                all.push(c);
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
        if !is_port
            && (ids.len() < 6 || shares_corridor)
            && f.position.is_none()
            && f.slot.is_none()
            && f.slide.is_none()
        {
            for mut c in nearby_callouts(f, scene) {
                if is_valid_candidate(&c) {
                    c.cost += crossed_penalty(&c);
                    ids.push(all.len());
                    all.push(c);
                }
            }
        }
        if f.position.is_none()
            && f.slot.as_deref() != Some("inline")
            && (ids.is_empty() || (ids.len() < 2 && f.slot.is_none() && f.slide.is_none()))
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
                        c.cost += crossed_penalty(&c);
                        ids.push(all.len());
                        all.push(c);
                    }
                }
                if !ids.is_empty() {
                    break;
                }
            }
        }
        if allow_remote_fallback && f.position.is_none() && ids.len() < 2 {
            let w = feature_width(f, scene);
            let h = text_height(&f.text);
            let anchor = f.route.get(f.route.len() / 2).copied().unwrap_or_default();
            let fallback = Candidate {
                label: LabelPlacement {
                    feature: f.id.clone(),
                    text: f.text.clone(),
                    bounds: Rect::new(anchor.x, fallback_y + h / 2., w, h),
                    anchor,
                    knockout: true,
                    callout: true,
                    orthogonal_leader: false,
                    horizontal: true,
                    slide: 0.5,
                },
                cost: 10000. + o.weights.pill,
            };
            fallback_y += h + 20.;
            if is_valid_candidate(&fallback) {
                ids.push(all.len());
                all.push(fallback);
            }
        }
        if ids.is_empty() {
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
            if ids
                .iter()
                .any(|id| all[*id].label.bounds.center().distance(old.bounds.center()) <= radius)
            {
                for id in &ids {
                    let distance = all[*id].label.bounds.center().distance(old.bounds.center());
                    if distance > radius {
                        all[*id].cost += 1_000_000. + distance * distance;
                    }
                }
            }
        }
        // Normalize candidate costs so every cost >= 0.0 and B&B pruning is admissible.
        let min_cost = ids
            .iter()
            .map(|id| all[*id].cost)
            .reduce(f64::min)
            .unwrap_or(0.);
        if min_cost < 0. {
            for &id in &ids {
                all[id].cost -= min_cost;
            }
        }
        for &id in &ids {
            all[id].cost = all[id].cost.max(0.);
        }
        ids.sort_by(|a, b| all[*a].cost.total_cmp(&all[*b].cost));
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
        let mut solution = select_component(&component, &by_feature, &all, &conflicts);
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
fn generate(f: &Feature, o: &LayoutOptions, l: &Layout, scene: &SceneGraph) -> Vec<Candidate> {
    let w = feature_width(f, scene);
    let h = text_height(&f.text);
    let is_port = f.id.contains('#') && scene.edge(&f.id).is_none();
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
            let samples = if let Some(t) = f.slide {
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
            let has_start_arrow = i == 0 && edge_kind == Some(EdgeKind::Bidirectional);
            let has_end_arrow =
                i + 1 == seg_count && edge_kind.is_some_and(|k| k != EdgeKind::Undirected);
            let pill_start_margin = if has_start_arrow { 16. } else { 12. };
            let pill_end_margin = if has_end_arrow { 16. } else { 12. };
            let explicit_inline = f.slot.as_deref() == Some("inline");
            for t in samples {
                let anchor = Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
                for (side, extra) in [-1., 1., 0.].into_iter().flat_map(|side| {
                    [0., 4., 8., 14.]
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
                    if side == 0. && !explicit_inline {
                        // RFC-001 Tier 2 centered inline pill: require enough segment length and
                        // clearance from arrowheads, stubs, and bends.
                        let req_len = w + 24.;
                        if len < req_len.min(along + 24.).max(along + 24.)
                            || t * len < along / 2. + pill_start_margin - 1e-6
                            || (1. - t) * len < along / 2. + pill_end_margin - 1e-6
                        {
                            continue;
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
                        slide_cost.unwrap_or(0.) + o.weights.pill + o.weights.centeredness * centered
                    } else {
                        // Tier 1 (0..45): flush external adjacent slot along segment.
                        slide_cost.unwrap_or(4.)
                            + extra * 1.25
                            + o.weights.centeredness * centered
                            + if side > 0. { 1. } else { 0. }
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
        let nx = if dx_axis.abs() > 1e-6 { dx_axis.signum() } else { 0. };
        let ny = if dy_axis.abs() > 1e-6 { dy_axis.signum() } else { 0. };
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
                            let x_gap = if nx != 0. {
                                gap + n_extra
                            } else {
                                6. + along
                            };
                            let y_gap = if ny != 0. {
                                gap + n_extra
                            } else {
                                6. + along
                            };
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
    let _ = l;
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
        for radius in [32., 40., 46., 56., 80., 96., 128., 160.] {
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
                let length: f64 = (dx * dx + dy * dy).sqrt();
                let bounds = Rect::new(
                    anchor.x + dx * radius / length,
                    anchor.y + dy * radius / length,
                    w,
                    h,
                );
                let dist = bounds.center().distance(anchor);
                if dist > 160. + 1e-6 {
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
        let inset = if scene.edge(&f.id).is_some() { -4. } else { -2. };
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
    if f.position.is_none() && c.label.callout {
        let path = leader_path(&c.label);
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
        || !leader_path(&c.label).windows(2).any(|p| {
            scene.nodes.iter().any(|n| {
                !f.ignore.contains(&n.id)
                    && !f.crossed.contains(&n.id)
                    && segment_hits(p[0], p[1], l.nodes[&n.id].inflate(leader_node_pad))
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
            for mut c in dense_callout_ring(f, scene, radii) {
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
                    c.cost += candidate_penalty(&c, f, scene, l, prior_none);
                    by_feature[fi].push(all.len());
                    all.push(c);
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
        let sol = select_component(&sorted, by_feature, all, &local_conflicts);
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
    for hit in index.locate_in_envelope_intersecting(&envelope(footprint(label))) {
        let n = &scene.nodes[hit.id];
        if f.ignore.contains(&n.id) {
            if n.id != f.id
                && let Some(header) =
                    crate::layout::container_header_rect_with_ports(n, hit.bounds, &l.ports)
                && (r.intersects(header)
                    || leader_path(label)
                        .windows(2)
                        .any(|p| segment_hits(p[0], p[1], header)))
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
                && (r.intersects(header)
                    || leader_path(label)
                        .windows(2)
                        .any(|p| segment_hits(p[0], p[1], header)))
            {
                return false;
            }
        } else if r.intersects(hit.bounds)
            || leader_path(label)
                .windows(2)
                .any(|p| segment_hits(p[0], p[1], hit.bounds))
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
    struct Search<'a> {
        by_feature: &'a [Vec<usize>],
        all: &'a [Candidate],
        adj: &'a [Vec<usize>],
        blocked: Vec<u32>,
        best: Vec<usize>,
        cost: f64,
        budget: usize,
    }
    impl Search<'_> {
        fn visit(&mut self, remaining: &[usize], cost: f64, chosen: &mut Vec<usize>) {
            if self.budget == 0 {
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
    let mut greedy = Vec::new();
    let mut greedy_blocked = vec![0u32; all.len()];
    for &f in component {
        if let Some(&id) = by_feature[f].iter().find(|&&id| greedy_blocked[id] == 0) {
            greedy.push(id);
            for &other in &adj[id] {
                greedy_blocked[other] += 1;
            }
        }
    }
    let cost = if greedy.len() == component.len() {
        greedy.iter().map(|i| all[*i].cost).sum()
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
        budget: 150000,
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
        .map(|(i, a)| if a.callout || manual[i] { (0., 0.) } else { (-6., 6.) })
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
                let sdf = smooth_sdf_dual(&xs[i], &ys[i], label.bounds, &xs[j], &ys[j], other.bounds);
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
    let mut candidate = l.clone();
    for (lane, delta) in lanes.iter().zip(result) {
        let route = candidate.routes.get_mut(&lane.edge).unwrap();
        for point in &mut route.points[lane.index..=lane.index + 1] {
            if lane.horizontal {
                point.y += delta;
            } else {
                point.x += delta;
            }
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
        let spacious = crate::compile("a @pin(grid: 0, 0)\nb @pin(grid: 18, 0)\na.E -> b.W: payload\n").unwrap();
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
        let lbl = d.layout.labels.iter().find(|l| l.feature == "a->b").unwrap();
        assert!(!lbl.callout, "< 32px manual @label should not emit callout: {lbl:?}");

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
        assert!(crate::layout::validate(&diag.scene, &diag.layout, &LayoutOptions::default()).is_ok());
        assert!(diag.layout.labels.iter().all(|l| !l.callout));
    }

    #[test]
    fn stacked_ports_and_16px_record_lanes_assign_completely() {
        let records = crate::compile(include_str!(
            "../tests/fixtures/quality/09-many-record-slots.diagram"
        ))
        .unwrap();
        assert!(crate::layout::validate(&records.scene, &records.layout, &LayoutOptions::default()).is_ok());
        assert!(
            records.layout.labels.iter().all(|l| !l.callout),
            "16px-pitched record slots must avoid callouts: {:?}",
            records.layout.labels
        );

        let duplex = crate::compile(include_str!(
            "../tests/fixtures/quality/10-many-named-ports.diagram"
        ))
        .unwrap();
        assert!(crate::layout::validate(&duplex.scene, &duplex.layout, &LayoutOptions::default()).is_ok());
    }
}
