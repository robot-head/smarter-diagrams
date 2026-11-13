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
        5.
    } else {
        2.
    })
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
                    && crate::layout::container_header_rect(node, rect)
                        .is_some_and(|h| bounds.intersects(h))
            } else if crossed.contains(&node.id) {
                let opaque_flow_pill = !label.callout
                    && scene
                        .edge(&label.feature)
                        .is_some_and(|e| crate::render::flow_caption(scene, e).is_some());
                let clear_border = opaque_flow_pill
                    || !bounds.intersects(rect)
                    || rect.inflate(-6.).encloses(bounds);
                let hits_header = crate::layout::container_header_rect(node, rect)
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
                next.callout = true;
                next.knockout = false;
                next.orthogonal_leader = true;
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
                if !crosses_foreign_route && !punches_container {
                    candidates.push(Candidate {
                        label: rail_label,
                        cost: -10000.,
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
                c.cost = c.cost - minimum
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
                    fixed.callout = true;
                    fixed.orthogonal_leader = true;
                    fixed.knockout = false;
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
                                cost: 4. * step + excess * excess / 32. - 1.,
                                label: alternate,
                            });
                            candidates.push(Candidate {
                                cost: 4. * step + excess * excess / 32. - 2.,
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
                    if offset.distance(Point::default()) > 0. {
                        nearby.callout = true;
                        nearby.orthogonal_leader = true;
                        nearby.knockout = false;
                    }
                    if nearby.orthogonal_leader {
                        let mut alternate = nearby.clone();
                        alternate.horizontal = !alternate.horizontal;
                        candidates.push(Candidate {
                            cost: 4. * alternate.bounds.center().distance(old.bounds.center()),
                            label: alternate,
                        });
                    }
                    candidates.push(Candidate {
                        cost: 4. * nearby.bounds.center().distance(old.bounds.center()) - 1.,
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
                if bounds.inflate(2.).contains(anchor) {
                    candidates.push(Candidate {
                        cost: -1.,
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
                }
                for horizontal in [true, false] {
                    candidates.push(Candidate {
                        cost: 0.,
                        label: LabelPlacement {
                            feature: f.id.clone(),
                            text: f.text.clone(),
                            bounds: Rect::new(
                                position.x,
                                position.y,
                                feature_width(f, scene),
                                text_height(&f.text),
                            ),
                            anchor: Point::new(segment[0].x + t * dx, segment[0].y + t * dy),
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
        let crossed_penalty = |c: &Candidate| {
            let mut pen = if f
                .crossed
                .iter()
                .any(|id| l.nodes[id].intersects(c.label.bounds))
            {
                48.
            } else {
                0.
            };
            if f.position.is_none() && scene.edge(&f.id).is_some() {
                let owner_escapes = f
                    .ignore
                    .iter()
                    .filter(|id| {
                        scene.node(id).is_some_and(|n| n.container)
                            && !l.nodes[*id].inflate(-4.).encloses(c.label.bounds)
                    })
                    .count();
                pen += owner_escapes as f64 * 360.;
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
                if prior.is_none() && c.cost > -5000. {
                    let leader_len: f64 = path.windows(2).map(|p| p[0].distance(p[1])).sum();
                    pen += 0.8 * leader_len;
                }
            }
            pen
        };
        let is_valid_candidate = |c: &Candidate| {
            let stable_leader = prior.is_none()
                || !leader_path(&c.label).windows(2).any(|p| {
                    scene.nodes.iter().any(|n| {
                        !f.ignore.contains(&n.id)
                            && !f.crossed.contains(&n.id)
                            && segment_hits(p[0], p[1], l.nodes[&n.id].inflate(4.))
                    })
                });
            stable_leader && valid(&c.label, f, scene, l, &node_index)
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
        if (ids.len() < 6 || shares_corridor)
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
        let solution = select_component(&component, &by_feature, &all, &conflicts);
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
    let w = feature_width(f, scene);
    let h = text_height(&f.text);
    if let Some(anchor) = crate::bundles::caption_anchor(scene, l, edge, w)
        .or_else(|| crate::duplex::caption_anchor(scene, l, edge, w))
        .or_else(|| crate::gather::caption_anchor(scene, l, edge, w))
        .or_else(|| crate::records::caption_anchor(scene, l, edge, w))
    {
        return Some(Candidate {
            cost: -1000.,
            label: LabelPlacement {
                feature: f.id.clone(),
                text: f.text.clone(),
                bounds: Rect::new(anchor.x, anchor.y - h / 2. - 6., w, h),
                anchor,
                knockout: false,
                callout: false,
                orthogonal_leader: false,
                horizontal: true,
                slide: 0.5,
            },
        });
    }
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
    let mesh = crate::presentation::mesh_layers(scene).is_some()
        && scene.edges.iter().all(|e| {
            l.ports[&port_key(e, true)].side == Side::E
                && l.ports[&port_key(e, false)].side == Side::W
        });
    if !fan_out && !fan_in && !record_bundle && !mesh {
        return None;
    }
    let leaf = if fan_out {
        &edge.target.node
    } else {
        &edge.source.node
    };
    let body = l.nodes[leaf];
    let branch_y = if record_bundle || mesh {
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
            cost: -1000.,
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
    let lengths: Vec<_> = f.route.windows(2).map(|p| p[0].distance(p[1])).collect();
    let total = lengths.iter().sum::<f64>().max(1.);
    let mut arc = 0.;
    let mut out = Vec::new();
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
        let mut samples = if let Some(t) = f.slide {
            vec![((t * total - arc) / len).clamp(low, high)]
        } else {
            vec![
                0.5,
                0.35_f64.clamp(low, high),
                0.65_f64.clamp(low, high),
                0.2_f64.clamp(low, high),
                0.8_f64.clamp(low, high),
                low,
                high,
            ]
        };
        samples.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
        for t in samples {
            let anchor = Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
            for (side, extra) in [-1., 1., 0.].into_iter().flat_map(|side| {
                [0., 8., 18., 30.]
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
                let cost = f
                    .slide
                    .map(|target| 1000. * ((arc + len * t) / total - target).abs())
                    .unwrap_or(0.)
                    + extra / 2.
                    + o.weights.centeredness * centered
                    + if side > 0. { 1. } else { 0. }
                    // Automatic cutouts are a last resort after nearby external
                    // callouts. An explicitly requested inline slot keeps its cost.
                    + if side == 0. {
                        o.weights.pill
                            + if f.slot.as_deref() == Some("inline") {
                                0.
                            } else {
                                1000.
                            }
                    } else {
                        0.
                    };
                out.push(Candidate {
                    label: LabelPlacement {
                        feature: f.id.clone(),
                        text: f.text.clone(),
                        bounds,
                        anchor,
                        knockout: side == 0.,
                        callout: extra + 6. > h,
                        orthogonal_leader: extra + 6. > h,
                        horizontal,
                        slide: (arc + len * t) / total,
                    },
                    cost,
                });
            }
        }
        arc += len;
    }
    // External quadrants at several distances let port captions yield space
    // to an edge label instead of forcing a cutout through the connection.
    if f.id.contains('#') && scene.edge(&f.id).is_none() && f.route.len() == 2 {
        let wall = f.route[0];
        let anchor = f.route[1];
        let nx = (anchor.x - wall.x).signum();
        let ny = (anchor.y - wall.y).signum();
        for (origin, penalty) in [(wall, 0.), (anchor, 2.)] {
            for extra in [0., 12., 28., 44.] {
                for (dx, dy, cost) in [(1., -1., 0.), (-1., -1., 1.), (1., 1., 2.), (-1., 1., 3.)] {
                    let inward = if (nx != 0. && dx * nx < 0.) || (ny != 0. && dy * ny < 0.) {
                        1.5
                    } else {
                        0.
                    };
                    let bounds = Rect::new(
                        origin.x + dx * (w / 2. + 6. + if ny != 0. { extra } else { 0. }),
                        origin.y + dy * (h / 2. + 6. + if nx != 0. { extra } else { 0. }),
                        w,
                        h,
                    );
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
                        cost: penalty + cost + inward + extra / 3.,
                    });
                }
            }
        }
    }
    // Keep exceptional callouts close to the connection before using the
    // guaranteed global rail. This matters at compound boundaries, where an
    // otherwise short label may have no legal slot directly above its route.
    if f.slot.as_deref() != Some("inline") {
        let anchor = crate::render::at_arc(&f.route, 0.5);
        for radius in [64., 112., 160.] {
            for (dx, dy) in [(0., -1.), (0., 1.), (-1., 0.), (1., 0.)] {
                if let Some(slot) = f.slot.as_deref()
                    && ((slot == "above" && dy >= 0.) || (slot == "below" && dy <= 0.))
                {
                    continue;
                }
                out.push(Candidate {
                    label: LabelPlacement {
                        feature: f.id.clone(),
                        text: f.text.clone(),
                        bounds: Rect::new(anchor.x + dx * radius, anchor.y + dy * radius, w, h),
                        anchor,
                        knockout: true,
                        callout: true,
                        orthogonal_leader: false,
                        horizontal: true,
                        slide: 0.5,
                    },
                    cost: 100. + radius + o.weights.pill,
                });
            }
        }
    }
    let _ = l;
    out
}
// A tightly packed parallel bundle may need a diagonal, local leader. Only
// search this larger neighborhood when ordinary automatic slots are all blocked.
fn nearby_callouts(f: &Feature, scene: &SceneGraph) -> Vec<Candidate> {
    let w = feature_width(f, scene);
    let h = text_height(&f.text);
    let mut out = Vec::new();
    for arc in [0.15, 0.35, 0.5, 0.65, 0.85] {
        let anchor = crate::render::at_arc(&f.route, arc);
        for radius in [48., 80., 112., 160.] {
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
                for horizontal in [false, true] {
                    out.push(Candidate {
                        label: LabelPlacement {
                            feature: f.id.clone(),
                            text: f.text.clone(),
                            bounds: Rect::new(
                                anchor.x + dx * radius / length,
                                anchor.y + dy * radius / length,
                                w,
                                h,
                            ),
                            anchor,
                            knockout: false,
                            callout: true,
                            orthogonal_leader: true,
                            horizontal,
                            slide: arc,
                        },
                        cost: 300. + radius,
                    });
                }
            }
        }
    }
    out
}
fn valid(
    label: &LabelPlacement,
    f: &Feature,
    scene: &SceneGraph,
    l: &Layout,
    index: &RTree<Indexed>,
) -> bool {
    let r = clearance(label);
    for hit in index.locate_in_envelope_intersecting(&envelope(footprint(label))) {
        let n = &scene.nodes[hit.id];
        if f.ignore.contains(&n.id) {
            if n.id != f.id
                && let Some(header) = crate::layout::container_header_rect(n, hit.bounds)
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
            if let Some(header) = crate::layout::container_header_rect(n, hit.bounds)
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
    if l.ports
        .values()
        .any(|p| r.intersects(Rect::new(p.point.x, p.point.y, 8., 8.)))
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
        budget: 40000,
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
fn smooth_sdf(x: &Dual, y: &Dual, a: Rect, b: Rect) -> Dual {
    let n = x.gradient.len();
    let c = |v| Dual::constant(v, n);
    let abs = |z: Dual| z.square().add(&c(1e-6)).sqrt();
    let dx = abs(x.sub(&c(b.x))).sub(&c((a.w + b.w) / 2.));
    let dy = abs(y.sub(&c(b.y))).sub(&c((a.h + b.h) / 2.));
    let px = dx.softplus(2.);
    let py = dy.softplus(2.);
    let outside = px.square().add(&py.square()).add(&c(1e-8)).sqrt();
    let max = dx.add(&dy).add(&abs(dx.sub(&dy))).scale(0.5);
    outside.add(&max.sub(&max.softplus(2.)))
}
fn polish(labels: &mut [LabelPlacement], scene: &SceneGraph, l: &Layout, o: &LayoutOptions) {
    if labels.is_empty() || o.polish_steps == 0 {
        return;
    }
    let original = labels.to_vec();
    let n = labels.len();
    let bounds: Vec<_> = labels
        .iter()
        .map(|a| if a.callout { (0., 0.) } else { (-6., 6.) })
        .collect();
    let result = solver::lbfgs(&vec![0.; n], &bounds, o.polish_steps, |vars| {
        let mut energy = Dual::constant(0., n);
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
            for node in scene.nodes.iter().filter(|n| !n.container) {
                let sdf = smooth_sdf(&x, &y, label.bounds, l.nodes[&node.id]);
                let penalty = Dual::constant(6., n).sub(&sdf).softplus(2.).square();
                energy = energy.add(&penalty);
            }
            for other in original.iter().skip(i + 1) {
                energy = energy.add(
                    &Dual::constant(6., n)
                        .sub(&smooth_sdf(&x, &y, label.bounds, other.bounds))
                        .softplus(2.)
                        .square(),
                );
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
        if let Some(edge) = scene.edge(&labels[i].feature)
            && effective_annotations(scene, &edge.id, &edge.annotations, o.variant.as_deref())
                .iter()
                .any(|a| {
                    a.name == "label" && (a.attrs.contains_key("x") || a.attrs.contains_key("y"))
                })
        {
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
    let bounds: Vec<_> = lanes.iter().map(|s| (-s.bound, s.bound)).collect();
    let result = solver::lbfgs(&vec![0.; n], &bounds, o.polish_steps, |vars| {
        let mut energy = Dual::constant(0., n);
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
            for label in &l.labels {
                if label.knockout && label.feature == lane.edge {
                    continue;
                }
                let sdf = smooth_sdf(&x, &y, lane.rect, label.bounds);
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
