//! Direction-state A* on an orthogonal visibility lattice and conservative channel nudging.
use crate::{Error, layout::*, model::*};
use std::{cmp::Ordering, collections::BinaryHeap};
#[derive(Clone, Copy)]
struct Queue {
    f: f64,
    g: f64,
    x: f64,
    y: f64,
    dir: usize,
    state: usize,
}
fn cmp_f_desc(a: f64, b: f64) -> Ordering {
    // Quantize finite f-scores at 1e-6 so near-ties form a transitive total order
    // while preferring larger g on equal-cost plateaus.
    if a.is_finite() && b.is_finite() && a.abs() <= 1e9 && b.abs() <= 1e9 {
        let qa = (a * 1e6).round() as i128;
        let qb = (b * 1e6).round() as i128;
        return qb.cmp(&qa);
    }
    b.total_cmp(&a)
}
impl PartialEq for Queue {
    fn eq(&self, b: &Self) -> bool {
        self.cmp(b) == Ordering::Equal
    }
}
impl Eq for Queue {}
impl PartialOrd for Queue {
    fn partial_cmp(&self, b: &Self) -> Option<Ordering> {
        Some(self.cmp(b))
    }
}
impl Ord for Queue {
    fn cmp(&self, b: &Self) -> Ordering {
        cmp_f_desc(self.f, b.f)
            .then_with(|| self.g.total_cmp(&b.g))
            .then_with(|| b.x.to_bits().cmp(&self.x.to_bits()))
            .then_with(|| b.y.to_bits().cmp(&self.y.to_bits()))
            .then_with(|| b.dir.cmp(&self.dir))
            .then_with(|| b.state.cmp(&self.state))
    }
}
fn stub_pairs(l: &Layout, e: &Edge) -> Vec<(Point, Point)> {
    [port_key(e, true), port_key(e, false)]
        .into_iter()
        .filter_map(|k| l.ports.get(&k).map(|p| (p.point, p.stub)))
        .collect()
}
fn endpoint_obstacles(scene: &SceneGraph, l: &Layout, e: &Edge, margin: f64) -> Vec<Rect> {
    let src_anc = ancestors(scene, &e.source.node);
    let dst_anc = ancestors(scene, &e.target.node);
    let mut out = Vec::new();
    for id in [&e.source.node, &e.target.node] {
        // Ancestor containers (e.g. C in C -> C.child) enclose the other endpoint
        // and must never be treated as solid interior boxes.
        if !src_anc.contains(id) && !dst_anc.contains(id) {
            let r = l.nodes[id].inflate(margin);
            if !out.contains(&r) {
                out.push(r);
            }
        }
    }
    out
}
fn strict_lca(scene: &SceneGraph, a: &str, b: &str) -> Option<String> {
    let b_anc = ancestors(scene, b);
    let mut cur = scene
        .index(a)
        .and_then(|i| scene.nodes[i].parent.as_deref());
    while let Some(parent_id) = cur {
        if b_anc.contains(parent_id) {
            return Some(parent_id.to_string());
        }
        cur = scene
            .index(parent_id)
            .and_then(|i| scene.nodes[i].parent.as_deref());
    }
    None
}
fn carve_rect_for_stubs(rect: Rect, stubs: &[(Point, Point)], margin: f64) -> Vec<Rect> {
    let mut pieces = vec![rect];
    let aperture = margin.max(8.);
    for &(p, s) in stubs {
        let mut next = Vec::new();
        for r in pieces {
            if !r.blocks(p, s) && !r.contains(p) && !r.contains(s) {
                next.push(r);
                continue;
            }
            if (p.x - s.x).abs() < 1e-6 {
                let cut_l = p.x - aperture;
                let cut_r = p.x + aperture;
                if cut_l > r.left() + 1. {
                    let w = cut_l - r.left();
                    next.push(Rect::new(r.left() + w / 2., r.y, w, r.h));
                }
                if r.right() > cut_r + 1. {
                    let w = r.right() - cut_r;
                    next.push(Rect::new(cut_r + w / 2., r.y, w, r.h));
                }
            } else {
                let cut_t = p.y - aperture;
                let cut_b = p.y + aperture;
                if cut_t > r.top() + 1. {
                    let h = cut_t - r.top();
                    next.push(Rect::new(r.x, r.top() + h / 2., r.w, h));
                }
                if r.bottom() > cut_b + 1. {
                    let h = r.bottom() - cut_b;
                    next.push(Rect::new(r.x, cut_b + h / 2., r.w, h));
                }
            }
        }
        pieces = next;
    }
    pieces
        .into_iter()
        .filter(|r| {
            r.w > 1.
                && r.h > 1.
                && !stubs
                    .iter()
                    .any(|&(p, s)| r.blocks(p, s) || r.contains(p) || r.contains(s))
        })
        .collect()
}
pub fn obstacles(scene: &SceneGraph, l: &Layout, e: &Edge, margin: f64) -> Vec<Rect> {
    let mut exclude = ancestors(scene, &e.source.node);
    exclude.extend(ancestors(scene, &e.target.node));
    exclude.insert(e.source.node.clone());
    exclude.insert(e.target.node.clone());
    let mut bodies: Vec<_> = scene
        .nodes
        .iter()
        .filter(|n| !exclude.contains(&n.id))
        .map(|n| l.nodes[&n.id].inflate(margin))
        .collect();
    let stubs = stub_pairs(l, e);
    for n in &scene.nodes {
        let r = l.nodes[&n.id];
        // Reserve container headers and SQL table title bands as solid obstacles
        // with port-side apertures carved where an endpoint stub attaches.
        if let Some(header) = crate::layout::container_header_rect(n, r)
            && exclude.contains(&n.id)
        {
            bodies.extend(carve_rect_for_stubs(
                header.inflate(margin / 2.),
                &stubs,
                margin,
            ));
        }
        if let Some(title) = crate::layout::sql_table_title_rect(n, r)
            && exclude.contains(&n.id)
        {
            bodies.extend(carve_rect_for_stubs(
                title.inflate(margin / 2.),
                &stubs,
                margin,
            ));
        }
    }
    if let Some(lca) = strict_lca(scene, &e.source.node, &e.target.node)
        && let Some(c_rect) = l.nodes.get(&lca).copied()
    {
        let mut inner = c_rect.inflate(-margin);
        if inner.w > 2. && inner.h > 2. {
            let mut min_x = inner.left();
            let mut max_x = inner.right();
            let mut min_y = inner.top();
            let mut max_y = inner.bottom();
            for &(p, s) in &stubs {
                for pt in [p, s] {
                    min_x = min_x.min(pt.x);
                    max_x = max_x.max(pt.x);
                    min_y = min_y.min(pt.y);
                    max_y = max_y.max(pt.y);
                }
            }
            inner = Rect::new(
                (min_x + max_x) / 2.,
                (min_y + max_y) / 2.,
                max_x - min_x,
                max_y - min_y,
            );
            let wall_t = margin.max(8.);
            let outer = inner.inflate(wall_t);
            let walls = [
                Rect::new(
                    outer.x,
                    (outer.top() + inner.top()) / 2.,
                    outer.w,
                    inner.top() - outer.top(),
                ),
                Rect::new(
                    outer.x,
                    (inner.bottom() + outer.bottom()) / 2.,
                    outer.w,
                    outer.bottom() - inner.bottom(),
                ),
                Rect::new(
                    (outer.left() + inner.left()) / 2.,
                    outer.y,
                    inner.left() - outer.left(),
                    outer.h,
                ),
                Rect::new(
                    (inner.right() + outer.right()) / 2.,
                    outer.y,
                    outer.right() - inner.right(),
                    outer.h,
                ),
            ];
            for w in walls {
                if !stubs
                    .iter()
                    .any(|&(p, s)| w.blocks(p, s) || w.contains(p) || w.contains(s))
                {
                    bodies.push(w);
                }
            }
        }
    }
    bodies
}
fn route_crossings_and_overlaps(
    scene: &SceneGraph,
    l: &Layout,
) -> (usize, usize, std::collections::BTreeSet<String>, Vec<Point>) {
    let coalesced_routes: Vec<(&str, Vec<Point>)> = scene
        .edges
        .iter()
        .filter_map(|e| {
            l.routes
                .get(&e.id)
                .map(|r| (e.id.as_str(), coalesced(&r.points)))
        })
        .collect();
    let mut crossings = 0;
    let mut overlaps = 0;
    let mut crossed = std::collections::BTreeSet::new();
    let mut cross_pts = Vec::new();
    for (i, (id_a, a)) in coalesced_routes.iter().enumerate() {
        for (id_b, b) in &coalesced_routes[i + 1..] {
            let mut pair_conflict = false;
            for p in a.windows(2) {
                for q in b.windows(2) {
                    if proper_crossing(p[0], p[1], q[0], q[1]) {
                        crossings += 1;
                        pair_conflict = true;
                        let pt = if (p[0].y - p[1].y).abs() < 1e-6 {
                            Point::new(q[0].x, p[0].y)
                        } else {
                            Point::new(p[0].x, q[0].y)
                        };
                        cross_pts.push(pt);
                    } else if collinear_overlap(p[0], p[1], q[0], q[1]) > 1e-6 {
                        overlaps += 1;
                        pair_conflict = true;
                        let pt = Point::new((p[0].x + p[1].x) / 2., (p[0].y + p[1].y) / 2.);
                        cross_pts.push(pt);
                    }
                }
            }
            if pair_conflict {
                crossed.insert((*id_a).to_string());
                crossed.insert((*id_b).to_string());
            }
        }
    }
    (crossings, overlaps, crossed, cross_pts)
}
fn route_total_length(routes: &std::collections::BTreeMap<String, Route>) -> f64 {
    routes
        .values()
        .map(|r| {
            r.points
                .windows(2)
                .map(|p| p[0].distance(p[1]))
                .sum::<f64>()
        })
        .sum()
}
#[allow(clippy::too_many_arguments)]
fn route_single_edge(
    scene: &SceneGraph,
    o: &LayoutOptions,
    prior: Option<&Layout>,
    l: &Layout,
    e: &Edge,
    used_ports: &std::collections::BTreeSet<String>,
    history: &CongestionHistory,
    allow_prior: bool,
) -> Result<(Route, bool), Error> {
    let a = l.ports[&port_key(e, true)].clone();
    let b = l.ports[&port_key(e, false)].clone();
    let annotations = effective_annotations(scene, &e.id, &e.annotations, o.variant.as_deref());
    if allow_prior
        && let Some(old) = prior
        && old.ports.get(&a.key) == Some(&a)
        && old.ports.get(&b.key) == Some(&b)
        && annotations.is_empty()
        && let Some(r) = old.routes.get(&e.id)
        && route_is_valid(scene, l, e, r, o.margin)
        && route_conflict_counts(l, &e.id, &r.points).1 == 0
    {
        return Ok((r.clone(), true));
    }
    let base_obs = obstacles(scene, l, e, o.margin);
    let mut obs = base_obs.clone();
    // Interior route cannot re-enter non-ancestor endpoint boxes after their mandatory stub.
    for ep_box in endpoint_obstacles(scene, l, e, o.margin) {
        if !obs.contains(&ep_box) {
            obs.push(ep_box);
        }
    }
    // Future mandatory shafts are congestion costs, not hard obstacles:
    // explicit geometry may require a crossing, but must stay feasible.
    let mut congestion = l.clone();
    let mut unrouted_ports = std::collections::BTreeSet::new();
    for other in &scene.edges {
        if other.id != e.id && !l.routes.contains_key(&other.id) {
            unrouted_ports.insert(port_key(other, true));
            unrouted_ports.insert(port_key(other, false));
        }
    }
    for key in used_ports {
        if key == &a.key || key == &b.key || !unrouted_ports.contains(key) {
            continue;
        }
        if let Some(p) = l.ports.get(key) {
            congestion.routes.insert(
                format!("@reserved-port:{key}"),
                Route {
                    edge: key.clone(),
                    points: vec![p.point, p.stub],
                },
            );
        }
    }
    let channel = annotations
        .iter()
        .find(|a| a.name == "route")
        .and_then(|a| a.attrs.get("channel_x"))
        .and_then(|v| number(&v.text))
        .map(|n| n * o.grid);
    if channel.is_some_and(|x| !x.is_finite()) {
        return Err(Error::Layout(
            "route channel exceeds finite coordinate range".into(),
        ));
    }
    let (nx, ny) = a.side.normal();
    let distance = (b.point.x - a.point.x) * nx + (b.point.y - a.point.y) * ny;
    let max_stub_span = a.point.distance(a.stub) + b.point.distance(b.stub);
    let tight_straight = channel.is_none()
        && !annotations.iter().any(|a| a.name == "route")
        && dir(a.side) == opposite(dir(b.side))
        && distance >= 8. - 1e-6
        && ((b.point.x - a.point.x) * ny - (b.point.y - a.point.y) * nx).abs() < 1e-6
        && distance < max_stub_span - 1e-6
        && !base_obs.iter().any(|r| r.blocks(a.point, b.point));
    // A facing pair has a continuous, balanced corridor. Visibility-grid
    // tie breaking must not move its elbow between unrelated lattice lines.
    let balanced = if channel.is_none()
        && !annotations.iter().any(|a| a.name == "route")
        && dir(a.side) == opposite(dir(b.side))
        && distance >= max_stub_span - 1e-6
    {
        let mid = Point::new((a.stub.x + b.stub.x) / 2., (a.stub.y + b.stub.y) / 2.);
        let mut points = if nx.abs() > 0. {
            vec![
                a.point,
                a.stub,
                Point::new(mid.x, a.stub.y),
                Point::new(mid.x, b.stub.y),
                b.stub,
                b.point,
            ]
        } else {
            vec![
                a.point,
                a.stub,
                Point::new(a.stub.x, mid.y),
                Point::new(b.stub.x, mid.y),
                b.stub,
                b.point,
            ]
        };
        dedup_points(&mut points);
        let proposed = Route {
            edge: e.id.clone(),
            points,
        };
        route_is_valid(scene, l, e, &proposed, o.margin)
            .then(|| proposed.points[1..proposed.points.len() - 1].to_vec())
    } else {
        None
    };
    let balanced_segs = build_existing_segments(&congestion, 1.0, history);
    let clear_balanced = balanced.as_ref().filter(|points| {
        points.windows(2).enumerate().all(|(idx, p)| {
            let nd = if (p[0].y - p[1].y).abs() < 1e-6 {
                if p[1].x >= p[0].x { 0 } else { 2 }
            } else if p[1].y >= p[0].y {
                1
            } else {
                3
            };
            let prev = if idx > 0 { Some(points[idx - 1]) } else { None };
            let prev_d = if let Some(pr) = prev {
                if (pr.y - p[0].y).abs() < 1e-6 {
                    if p[0].x >= pr.x { 0 } else { 2 }
                } else if p[0].y >= pr.y {
                    1
                } else {
                    3
                }
            } else {
                dir(a.side)
            };
            let step_ctx = StepCostCtx {
                start: a.stub,
                target: b.stub,
                segs: &balanced_segs,
                hotspots: &history.hotspots,
            };
            step_route_cost(p[0], p[1], prev_d, nd, prev, &step_ctx) < 1e-6
        })
    });
    let route_inner = if tight_straight {
        Vec::new()
    } else if let Some(points) = clear_balanced {
        points.clone()
    } else if let Some(x) = channel {
        let p = Point::new(x, a.stub.y);
        let q = Point::new(x, b.stub.y);
        if obs
            .iter()
            .any(|r| r.contains(p) || r.contains(q) || r.blocks(p, q))
        {
            return Err(Error::Layout(format!(
                "locked route channel for {} is obstructed",
                e.id
            )));
        }
        let channel_direction = if q.y >= p.y { 1 } else { 3 };
        let mut one = search_with_history(
            a.stub,
            p,
            dir(a.side),
            channel_direction,
            &obs,
            &congestion,
            o,
            history,
        )?;
        let two = search_with_history(
            q,
            b.stub,
            channel_direction,
            opposite(dir(b.side)),
            &obs,
            &congestion,
            o,
            history,
        )?;
        one.push(q);
        one.extend(two.into_iter().skip(1));
        one
    } else {
        search_with_history(
            a.stub,
            b.stub,
            dir(a.side),
            opposite(dir(b.side)),
            &obs,
            &congestion,
            o,
            history,
        )
        .or_else(|error| balanced.clone().ok_or(error))?
    };
    let mut points = vec![a.point];
    points.extend(route_inner);
    points.push(b.point);
    dedup_points(&mut points);
    if let Some(annotation) = annotations.iter().find(|a| a.name == "route")
        && let (Some(index), Some(lane)) = (
            value(&annotation.attrs, "segment"),
            value(&annotation.attrs, "lane"),
        )
    {
        let index = index as usize;
        if index == 0 || index + 2 >= points.len() {
            return Err(Error::Layout(
                "only internal route segments can be nudged".into(),
            ));
        }
        let horizontal = (points[index].y - points[index + 1].y).abs() < 1e-6;
        for point in &mut points[index..=index + 1] {
            if horizontal {
                point.y = lane * o.grid;
            } else {
                point.x = lane * o.grid;
            }
        }
        let valid_stub = |port: &PortPlacement, tip: Point| {
            let (nx, ny) = port.side.normal();
            let dx = tip.x - port.point.x;
            let dy = tip.y - port.point.y;
            dx * nx + dy * ny >= o.margin - 1e-6 && (dx * ny - dy * nx).abs() < 1e-6
        };
        if !valid_stub(&a, points[1])
            || !valid_stub(&b, points[points.len() - 2])
            || points.windows(2).enumerate().any(|(i, p)| {
                let diagonal = (p[0].x - p[1].x).abs() > 1e-6 && (p[0].y - p[1].y).abs() > 1e-6;
                diagonal
                    || if i == 0 || i + 2 == points.len() {
                        base_obs.iter().any(|r| r.blocks(p[0], p[1]))
                    } else {
                        obs.iter().any(|r| r.blocks(p[0], p[1]))
                    }
            })
        {
            return Err(Error::Layout(
                "route nudge violates an obstacle or mandatory port stub".into(),
            ));
        }
    }
    // Keep explicit port stub points even when collinear, so geometry consumers can inspect them.
    let mut route = Route {
        edge: e.id.clone(),
        points,
    };
    if !route_is_valid(scene, l, e, &route, o.margin)
        && let Some(points) = balanced
    {
        let mut pts = [vec![a.point], points, vec![b.point]].concat();
        dedup_points(&mut pts);
        if route_conflict_counts(l, &e.id, &pts).1 == 0 {
            route.points = pts;
        }
    }
    if !route_is_valid(scene, l, e, &route, o.margin)
        && !annotations.iter().any(|a| a.name == "route")
    {
        // Congestion costs can favor a direction-state walk that revisits
        // a vertex. Keep the simple geometry contract before aesthetics.
        let mut clear = l.clone();
        clear.routes.clear();
        if let Ok(points) = search(
            a.stub,
            b.stub,
            dir(a.side),
            opposite(dir(b.side)),
            &obs,
            &clear,
            o,
        ) {
            let mut candidate = Route {
                edge: e.id.clone(),
                points: [vec![a.point], points, vec![b.point]].concat(),
            };
            dedup_points(&mut candidate.points);
            if route_is_valid(scene, l, e, &candidate, o.margin) {
                route = candidate;
            }
        }
    }
    if !route_is_valid(scene, l, e, &route, o.margin) {
        return Err(Error::Layout(format!(
            "route {} cannot preserve its attachments and clearance",
            e.id
        )));
    }
    Ok((route, false))
}
fn route_pair_crossings(
    l: &Layout,
    id_a: &str,
    id_b: &str,
    a_pts: &[Point],
    b_pts: &[Point],
    vicinity_centers: &[Point],
    radius: f64,
) -> (usize, usize, usize) {
    let ca = coalesced(a_pts);
    let cb = coalesced(b_pts);
    let mut total_cross = 0;
    let mut stub_cross = 0;
    let mut total_ovlp = 0;
    let count_against = |edge_id: &str,
                         pts: &[Point],
                         total_cross: &mut usize,
                         stub_cross: &mut usize,
                         total_ovlp: &mut usize| {
        for (other_id, other) in &l.routes {
            if other_id == id_a
                || other_id == id_b
                || other_id == edge_id
                || other_id.starts_with("@reserved-")
            {
                continue;
            }
            let co = coalesced(&other.points);
            for p in pts.windows(2) {
                for q in co.windows(2) {
                    if proper_crossing(p[0], p[1], q[0], q[1]) {
                        *total_cross += 1;
                        let pt = if (p[0].y - p[1].y).abs() < 1e-6 {
                            Point::new(q[0].x, p[0].y)
                        } else {
                            Point::new(p[0].x, q[0].y)
                        };
                        if vicinity_centers.iter().any(|c| pt.distance(*c) <= radius) {
                            *stub_cross += 1;
                        }
                    } else if collinear_overlap(p[0], p[1], q[0], q[1]) > 1e-6 {
                        *total_ovlp += 1;
                    }
                }
            }
        }
    };
    for p in ca.windows(2) {
        for q in cb.windows(2) {
            if proper_crossing(p[0], p[1], q[0], q[1]) {
                total_cross += 1;
                let pt = if (p[0].y - p[1].y).abs() < 1e-6 {
                    Point::new(q[0].x, p[0].y)
                } else {
                    Point::new(p[0].x, q[0].y)
                };
                if vicinity_centers.iter().any(|c| pt.distance(*c) <= radius) {
                    stub_cross += 1;
                }
            } else if collinear_overlap(p[0], p[1], q[0], q[1]) > 1e-6 {
                total_ovlp += 1;
            }
        }
    }
    count_against(
        id_a,
        &ca,
        &mut total_cross,
        &mut stub_cross,
        &mut total_ovlp,
    );
    count_against(
        id_b,
        &cb,
        &mut total_cross,
        &mut stub_cross,
        &mut total_ovlp,
    );
    (stub_cross, total_cross, total_ovlp)
}
fn caption_anchors_ok(l: &Layout, edge_id: &str, pts: &[Point]) -> bool {
    let c = coalesced(pts);
    if c.len() < 2 {
        return false;
    }
    for lbl in l.labels.iter().filter(|lbl| lbl.feature == edge_id) {
        let mut best = f64::INFINITY;
        for seg in c.windows(2) {
            let dist = if (seg[0].y - seg[1].y).abs() < 1e-6 {
                let cx = lbl
                    .anchor
                    .x
                    .clamp(seg[0].x.min(seg[1].x), seg[0].x.max(seg[1].x));
                lbl.anchor.distance(Point::new(cx, seg[0].y))
            } else {
                let cy = lbl
                    .anchor
                    .y
                    .clamp(seg[0].y.min(seg[1].y), seg[0].y.max(seg[1].y));
                lbl.anchor.distance(Point::new(seg[0].x, cy))
            };
            best = best.min(dist);
        }
        if best > 160. {
            return false;
        }
    }
    true
}
/// Port-swap trials (each reroutes the pair twice) allowed per `route_all` on
/// graphs with more than 12 edges.
const SWAP_TRIAL_BUDGET: usize = 12;
fn swap_edge_ports(
    scene: &SceneGraph,
    o: &LayoutOptions,
    l: &mut Layout,
    used_ports: &std::collections::BTreeSet<String>,
) -> usize {
    let radius = 4. * o.stub;
    let swappable_endpoint = |ep: &Endpoint| -> bool {
        ep.port.is_none()
            && matches!(
                ep.constraint,
                PortConstraint::Free | PortConstraint::FixedSide { .. }
            )
    };
    let has_fixed_route_or_label = |e: &Edge| -> bool {
        effective_annotations(scene, &e.id, &e.annotations, o.variant.as_deref())
            .iter()
            .any(|a| {
                a.name == "route"
                    || (a.name == "label"
                        && (a.attrs.contains_key("x") || a.attrs.contains_key("y")))
            })
    };
    let mut port_node: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    for n in &scene.nodes {
        for p in n.ports.values() {
            port_node.insert(format!("{}#{}", n.id, p.id), n.id.clone());
        }
    }
    for e in &scene.edges {
        port_node.insert(port_key(e, true), e.source.node.clone());
        port_node.insert(port_key(e, false), e.target.node.clone());
    }
    let mut owners_by_key: std::collections::BTreeMap<String, Vec<(usize, bool)>> =
        std::collections::BTreeMap::new();
    for (ei, e) in scene.edges.iter().enumerate() {
        owners_by_key
            .entry(port_key(e, true))
            .or_default()
            .push((ei, true));
        owners_by_key
            .entry(port_key(e, false))
            .or_default()
            .push((ei, false));
    }
    // At most 4 sweeps; each accepts the first improving swap/slide and restarts.
    let small = scene.edges.len() <= 12;
    let mut trials_left: usize = if small { usize::MAX } else { SWAP_TRIAL_BUDGET };
    let mut tabu: std::collections::BTreeSet<(String, String)> = std::collections::BTreeSet::new();
    let mut total_swaps = 0;
    for _sweep in 0..4 {
        let mut improved = false;
        let (cur_g_cross, cur_g_ovlp, cur_crossed, _) = route_crossings_and_overlaps(scene, l);
        if cur_g_cross == 0 && cur_g_ovlp == 0 {
            break;
        }
        let cur_g_len = route_total_length(&l.routes);
        // Edges worth moving: crossed/overlapping, wrapped (3+ bends) or long detours.
        let involved: std::collections::BTreeSet<usize> = scene
            .edges
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                cur_crossed.contains(&e.id)
                    || l.routes.get(&e.id).is_some_and(|r| {
                        let c = coalesced(&r.points);
                        if c.len() < 2 {
                            return false;
                        }
                        let direct = c[0].manhattan_distance(c[c.len() - 1]);
                        let len: f64 = c.windows(2).map(|p| p[0].distance(p[1])).sum();
                        c.len() >= 5 || (direct >= 48. && len > 2.8 * direct)
                    })
            })
            .map(|(ei, _)| ei)
            .collect();
        if involved.is_empty() {
            break;
        }
        for node in &scene.nodes {
            for side in [Side::E, Side::W, Side::S, Side::N] {
                #[derive(Clone)]
                struct Slot {
                    coord: f64,
                    key: String,
                    owner: Option<(usize, bool)>,
                }
                let mut slots: Vec<Slot> = Vec::new();
                let mut face_involved = false;
                for (key, port) in &l.ports {
                    if port_node.get(key) != Some(&node.id) || port.side != side {
                        continue;
                    }
                    let owners: &[(usize, bool)] =
                        owners_by_key.get(key).map_or(&[], |v| v.as_slice());
                    face_involved |= owners.iter().any(|(ei, _)| involved.contains(ei));
                    let owner = if owners.len() == 1 {
                        let (ei, is_src) = owners[0];
                        let e = &scene.edges[ei];
                        let ep = if is_src { &e.source } else { &e.target };
                        (swappable_endpoint(ep) && !has_fixed_route_or_label(e))
                            .then_some((ei, is_src))
                    } else {
                        None
                    };
                    let coord = match side {
                        Side::N | Side::S => port.point.x,
                        Side::E | Side::W => port.point.y,
                    };
                    slots.push(Slot {
                        coord,
                        key: key.clone(),
                        owner,
                    });
                }
                if !face_involved || slots.len() < 2 {
                    continue;
                }
                slots.sort_by(|a, b| a.coord.total_cmp(&b.coord).then(a.key.cmp(&b.key)));
                for idx in 0..slots.len().saturating_sub(1) {
                    let (Some((ei1, _)), Some((ei2, _))) = (slots[idx].owner, slots[idx + 1].owner)
                    else {
                        continue;
                    };
                    if ei1 == ei2 || (!involved.contains(&ei1) && !involved.contains(&ei2)) {
                        continue;
                    }
                    let e1 = &scene.edges[ei1];
                    let e2 = &scene.edges[ei2];
                    let key1 = &slots[idx].key;
                    let key2 = &slots[idx + 1].key;
                    let p1 = l.ports[key1].clone();
                    let p2 = l.ports[key2].clone();
                    let vicinity = [p1.point, p2.point];
                    let (old_stub_c, old_tot_c, old_ovlp) = route_pair_crossings(
                        l,
                        &e1.id,
                        &e2.id,
                        &l.routes[&e1.id].points,
                        &l.routes[&e2.id].points,
                        &vicinity,
                        radius,
                    );
                    let wrapped_pair = coalesced(&l.routes[&e1.id].points).len() >= 5
                        || coalesced(&l.routes[&e2.id].points).len() >= 5;
                    if old_stub_c == 0 && old_tot_c == 0 && !wrapped_pair {
                        continue;
                    }
                    // Larger graphs: never retry a pair that already failed and
                    // bound the total number of (4-search) swap trials.
                    let pair_id = (key1.clone(), key2.clone());
                    if !small && tabu.contains(&pair_id) {
                        continue;
                    }
                    if trials_left == 0 {
                        return total_swaps;
                    }
                    trials_left -= 1;
                    let old_pair_len = cur_g_len;
                    let mut best_trial: Option<(Layout, usize, usize, usize, usize, f64)> = None;
                    for &(first_e, second_e) in &[(e1, e2), (e2, e1)] {
                        let mut trial = l.clone();
                        if let Some(slot1) = trial.ports.get_mut(key1) {
                            slot1.point = p2.point;
                            slot1.stub = p2.stub;
                        }
                        if let Some(slot2) = trial.ports.get_mut(key2) {
                            slot2.point = p1.point;
                            slot2.stub = p1.stub;
                        }
                        trial.routes.remove(&e1.id);
                        trial.routes.remove(&e2.id);
                        let Ok((r_first, _)) = route_single_edge(
                            scene,
                            o,
                            None,
                            &trial,
                            first_e,
                            used_ports,
                            &CongestionHistory::default(),
                            false,
                        ) else {
                            continue;
                        };
                        trial.routes.insert(first_e.id.clone(), r_first);
                        let Ok((r_second, _)) = route_single_edge(
                            scene,
                            o,
                            None,
                            &trial,
                            second_e,
                            used_ports,
                            &CongestionHistory::default(),
                            false,
                        ) else {
                            continue;
                        };
                        trial.routes.insert(second_e.id.clone(), r_second);
                        let r1 = &trial.routes[&e1.id];
                        let r2 = &trial.routes[&e2.id];
                        if !route_is_valid(scene, &trial, e1, r1, o.margin)
                            || !route_is_valid(scene, &trial, e2, r2, o.margin)
                            || !caption_anchors_ok(&trial, &e1.id, &r1.points)
                            || !caption_anchors_ok(&trial, &e2.id, &r2.points)
                        {
                            continue;
                        }
                        // If swapping freed an earlier wrap or bottleneck, let crossed edges reroute too.
                        let cand_len_0 = route_total_length(&trial.routes);
                        if (cur_g_cross > 0 || cur_g_ovlp > 0)
                            && scene.edges.len() <= 12
                            && cur_crossed.len() <= 10
                            && (wrapped_pair || cand_len_0 < old_pair_len - 16.)
                        {
                            let mut cascade = trial.clone();
                            let follow_edges: Vec<&Edge> = scene
                                .edges
                                .iter()
                                .filter(|ce| {
                                    ce.id != e1.id
                                        && ce.id != e2.id
                                        && cur_crossed.contains(&ce.id)
                                        && !has_fixed_route_or_label(ce)
                                })
                                .collect();
                            if !follow_edges.is_empty() {
                                for ce in &follow_edges {
                                    cascade.routes.remove(&ce.id);
                                }
                                let mut cok = true;
                                for ce in &follow_edges {
                                    if let Ok((cr, _)) = route_single_edge(
                                        scene,
                                        o,
                                        None,
                                        &cascade,
                                        ce,
                                        used_ports,
                                        &CongestionHistory::default(),
                                        false,
                                    ) && route_is_valid(scene, &cascade, ce, &cr, o.margin)
                                        && caption_anchors_ok(&cascade, &ce.id, &cr.points)
                                    {
                                        cascade.routes.insert(ce.id.clone(), cr);
                                    } else {
                                        cok = false;
                                        break;
                                    }
                                }
                                if cok {
                                    trial = cascade;
                                }
                            }
                        }
                        let (new_stub_c, new_tot_c, new_ovlp) = route_pair_crossings(
                            &trial,
                            &e1.id,
                            &e2.id,
                            &trial.routes[&e1.id].points,
                            &trial.routes[&e2.id].points,
                            &vicinity,
                            radius,
                        );
                        let (g_cross, g_ovlp, _, _) = route_crossings_and_overlaps(scene, &trial);
                        let g_len = route_total_length(&trial.routes);
                        let g_better = g_ovlp < cur_g_ovlp
                            || (g_ovlp == cur_g_ovlp
                                && (g_cross < cur_g_cross
                                    || (g_cross == cur_g_cross
                                        && ((new_stub_c < old_stub_c && new_tot_c <= old_tot_c)
                                            || g_len < cur_g_len - 16.))));
                        let stub_better = new_ovlp <= old_ovlp
                            && g_ovlp <= cur_g_ovlp
                            && g_cross <= cur_g_cross
                            && new_stub_c < old_stub_c
                            && new_tot_c < old_tot_c;
                        if (g_better || stub_better)
                            && best_trial
                                .as_ref()
                                .is_none_or(|(_, bgo, bgc, btc, bsc, blen)| {
                                    (g_ovlp, g_cross, new_tot_c, new_stub_c)
                                        < (*bgo, *bgc, *btc, *bsc)
                                        || ((g_ovlp, g_cross, new_tot_c, new_stub_c)
                                            == (*bgo, *bgc, *btc, *bsc)
                                            && g_len < *blen - 1e-6)
                                })
                        {
                            best_trial =
                                Some((trial, g_ovlp, g_cross, new_tot_c, new_stub_c, g_len));
                        }
                    }
                    if let Some((accepted, _, _, _, _, _)) = best_trial {
                        *l = accepted;
                        improved = true;
                        total_swaps += 1;
                        break;
                    }
                    if !small {
                        tabu.insert(pair_id);
                    }
                }
                if improved {
                    break;
                }
                // Slide a crossed/wrapped port across swappable neighbors on the same face if needed.
                // `l` is unchanged since the sweep began, so the cached conflict state applies.
                if (cur_g_cross > 0 || cur_g_ovlp > 0)
                    && cur_crossed.len() <= 10
                    && scene.edges.len() <= 12
                    && slots.len() >= 3
                    && slots.len() <= 8
                {
                    let mut best_slide: Option<(Layout, usize, usize, f64)> = None;
                    for src_idx in 0..slots.len() {
                        let Some((ei_src, _)) = slots[src_idx].owner else {
                            continue;
                        };
                        let e_src = &scene.edges[ei_src];
                        let src_wrapped = coalesced(&l.routes[&e_src.id].points).len() >= 5;
                        if !cur_crossed.contains(&e_src.id) && !src_wrapped {
                            continue;
                        }
                        for dst_idx in 0..slots.len() {
                            if dst_idx == src_idx || src_idx.abs_diff(dst_idx) <= 1 {
                                continue;
                            }
                            let lo = src_idx.min(dst_idx);
                            let hi = src_idx.max(dst_idx);
                            if (lo..=hi).any(|k| slots[k].owner.is_none()) {
                                continue;
                            }
                            let orig_ports: Vec<PortPlacement> =
                                (lo..=hi).map(|k| l.ports[&slots[k].key].clone()).collect();
                            let mut trial = l.clone();
                            // Shift `src_idx` to `dst_idx` while keeping slot coordinates in order.
                            let mut shifted_keys: Vec<String> =
                                (lo..=hi).map(|k| slots[k].key.clone()).collect();
                            let rel_src = src_idx - lo;
                            let rel_dst = dst_idx - lo;
                            let moving = shifted_keys.remove(rel_src);
                            shifted_keys.insert(rel_dst, moving);
                            for (pos, key) in shifted_keys.iter().enumerate() {
                                if let Some(slot) = trial.ports.get_mut(key) {
                                    slot.point = orig_ports[pos].point;
                                    slot.stub = orig_ports[pos].stub;
                                }
                            }
                            let mut rip_eids: Vec<usize> = (lo..=hi)
                                .filter_map(|k| slots[k].owner.map(|(ei, _)| ei))
                                .collect();
                            for (ei, ce) in scene.edges.iter().enumerate() {
                                if (cur_crossed.contains(&ce.id)
                                    || coalesced(&l.routes[&ce.id].points).len() >= 5)
                                    && !has_fixed_route_or_label(ce)
                                    && !rip_eids.contains(&ei)
                                {
                                    rip_eids.push(ei);
                                }
                            }
                            for &ei in &rip_eids {
                                trial.routes.remove(&scene.edges[ei].id);
                            }
                            // Try routing `e_src` first as well as keeping existing first.
                            let mut cand_orders = vec![rip_eids.clone()];
                            let mut rev = rip_eids.clone();
                            rev.reverse();
                            cand_orders.push(rev);
                            for order_list in cand_orders {
                                let mut run = trial.clone();
                                let mut ok = true;
                                for &ei in &order_list {
                                    let ce = &scene.edges[ei];
                                    if let Ok((cr, _)) = route_single_edge(
                                        scene,
                                        o,
                                        None,
                                        &run,
                                        ce,
                                        used_ports,
                                        &CongestionHistory::default(),
                                        false,
                                    ) && route_is_valid(scene, &run, ce, &cr, o.margin)
                                        && caption_anchors_ok(&run, &ce.id, &cr.points)
                                    {
                                        run.routes.insert(ce.id.clone(), cr);
                                    } else {
                                        ok = false;
                                        break;
                                    }
                                }
                                if !ok {
                                    continue;
                                }
                                let (g_cross, g_ovlp, _, _) =
                                    route_crossings_and_overlaps(scene, &run);
                                let g_len = route_total_length(&run.routes);
                                if (g_ovlp < cur_g_ovlp
                                    || (g_ovlp == cur_g_ovlp
                                        && (g_cross < cur_g_cross
                                            || (g_cross == cur_g_cross
                                                && g_len < cur_g_len - 32.))))
                                    && best_slide.as_ref().is_none_or(|(_, bo, bc, bl)| {
                                        (g_ovlp, g_cross) < (*bo, *bc)
                                            || ((g_ovlp, g_cross) == (*bo, *bc)
                                                && g_len < *bl - 1e-6)
                                    })
                                {
                                    best_slide = Some((run, g_ovlp, g_cross, g_len));
                                }
                            }
                        }
                    }
                    if let Some((accepted, _, _, _)) = best_slide {
                        *l = accepted;
                        improved = true;
                        break;
                    }
                }
            }
            if improved {
                break;
            }
        }
        if !improved {
            break;
        }
    }
    total_swaps
}
fn route_conflict_counts(l: &Layout, edge_id: &str, pts: &[Point]) -> (usize, usize) {
    let c = coalesced(pts);
    let mut cross = 0;
    let mut ovlp = 0;
    for (other_id, other) in &l.routes {
        if other_id == edge_id || other_id.starts_with("@reserved-") {
            continue;
        }
        let co = coalesced(&other.points);
        for p in c.windows(2) {
            for q in co.windows(2) {
                if proper_crossing(p[0], p[1], q[0], q[1]) {
                    cross += 1;
                } else if collinear_overlap(p[0], p[1], q[0], q[1]) > 1e-6 {
                    ovlp += 1;
                }
            }
        }
    }
    (cross, ovlp)
}
fn restore_explicit_stubs(c: &[Point], a: &PortPlacement, b: &PortPlacement) -> Vec<Point> {
    let mut out = Vec::with_capacity(c.len() + 2);
    out.push(c[0]);
    let (anx, any) = a.side.normal();
    let req_a = (a.stub.x - a.point.x) * anx + (a.stub.y - a.point.y) * any;
    if c.len() >= 2
        && (c[1].x - c[0].x) * anx + (c[1].y - c[0].y) * any > req_a + 1e-6
        && c[0].distance(a.stub) > 1e-6
    {
        out.push(a.stub);
    }
    out.extend_from_slice(&c[1..c.len() - 1]);
    let (bnx, bny) = b.side.normal();
    let req_b = (b.stub.x - b.point.x) * bnx + (b.stub.y - b.point.y) * bny;
    let last = c.len() - 1;
    if c.len() >= 2
        && (c[last - 1].x - c[last].x) * bnx + (c[last - 1].y - c[last].y) * bny > req_b + 1e-6
        && c[last].distance(b.stub) > 1e-6
    {
        out.push(b.stub);
    }
    out.push(c[last]);
    dedup_points(&mut out);
    out
}
/// Remove interior micro-jogs shorter than one grid unit (the `micro-jog` lint
/// threshold). Handles staircase jogs (both neighbors continue the same way) and
/// narrow U-turns/doglegs (neighbors point in opposite directions). Endpoints never
/// move; a candidate is accepted only if it removes bends, stays valid, and does not
/// add crossings or overlaps for this edge.
fn dejitter(scene: &SceneGraph, o: &LayoutOptions, l: &mut Layout) {
    let threshold = o.grid.max(1.) - 1e-6;
    for e in &scene.edges {
        if effective_annotations(scene, &e.id, &e.annotations, o.variant.as_deref())
            .iter()
            .any(|a| a.name == "route")
        {
            continue;
        }
        let Some(a) = l.ports.get(&port_key(e, true)).cloned() else {
            continue;
        };
        let Some(b) = l.ports.get(&port_key(e, false)).cloned() else {
            continue;
        };
        let Some(route) = l.routes.get(&e.id) else {
            continue;
        };
        let cap = route.points.len() + 2;
        for _ in 0..cap {
            let current = &l.routes[&e.id].points;
            let c = coalesced(current);
            if c.len() < 4 {
                break;
            }
            let short: Vec<usize> = (1..c.len() - 2)
                .filter(|&k| c[k].distance(c[k + 1]) < threshold)
                .collect();
            if short.is_empty() {
                break;
            }
            let (base_cross, base_ovlp) = route_conflict_counts(l, &e.id, current);
            let mut updated = None;
            for k in short {
                let prev_dx = c[k].x - c[k - 1].x;
                let prev_dy = c[k].y - c[k - 1].y;
                let next_dx = c[k + 2].x - c[k + 1].x;
                let next_dy = c[k + 2].y - c[k + 1].y;
                let dot = prev_dx * next_dx + prev_dy * next_dy;
                if dot.abs() <= 1e-6 {
                    continue;
                }
                let jog_vertical = (c[k].x - c[k + 1].x).abs() < 1e-6;
                // Move the listed vertices onto `v` across the jog (staircase
                // alignment) or along it (sliding the jog onto a neighbor bend).
                let shifted = |idxs: [usize; 2], v: f64, across: bool| -> Vec<Point> {
                    let mut cand = c.clone();
                    for i in idxs {
                        if jog_vertical == across {
                            cand[i].y = v;
                        } else {
                            cand[i].x = v;
                        }
                    }
                    cand
                };
                let mut trials = Vec::new();
                let along = |p: Point| if jog_vertical { p.y } else { p.x };
                let perp = |p: Point| if jog_vertical { p.x } else { p.y };
                if dot > 0. {
                    // Staircase: align one neighboring run with the other.
                    if k + 2 < c.len() - 1 {
                        trials.push(shifted([k + 1, k + 2], along(c[k]), true));
                    }
                    if k > 1 {
                        trials.push(shifted([k - 1, k], along(c[k + 1]), true));
                    }
                }
                // Staircase or U-turn: slide the jog itself onto the next or
                // previous bend so two redundant bends collapse. `c[k]` and
                // `c[k + 1]` are always interior; endpoints never move.
                trials.push(shifted([k, k + 1], perp(c[k + 2]), false));
                trials.push(shifted([k, k + 1], perp(c[k - 1]), false));
                for cand in trials {
                    let simple = coalesced(&cand);
                    if simple.len() >= c.len()
                        || simple[0].distance(c[0]) > 1e-6
                        || simple[simple.len() - 1].distance(c[c.len() - 1]) > 1e-6
                    {
                        continue;
                    }
                    let pts = restore_explicit_stubs(&simple, &a, &b);
                    let cand_route = Route {
                        edge: e.id.clone(),
                        points: pts,
                    };
                    if !route_is_valid(scene, l, e, &cand_route, o.margin) {
                        continue;
                    }
                    let (cross, ovlp) = route_conflict_counts(l, &e.id, &cand_route.points);
                    if cross <= base_cross && ovlp <= base_ovlp {
                        updated = Some(cand_route);
                        break;
                    }
                }
                if updated.is_some() {
                    break;
                }
            }
            if let Some(route) = updated {
                l.routes.insert(e.id.clone(), route);
            } else {
                break;
            }
        }
    }
}
pub fn route_all(
    scene: &SceneGraph,
    o: &LayoutOptions,
    prior: Option<&Layout>,
    l: &mut Layout,
) -> Result<(), Error> {
    // Costs include current mandatory ports and routes accepted for this frame,
    // never an edge's old path or paths from a previous pointer position.
    l.routes.clear();
    l.reused_routes = 0;
    let used_ports: std::collections::BTreeSet<_> = scene
        .edges
        .iter()
        .flat_map(|e| [port_key(e, true), port_key(e, false)])
        .collect();
    let mut pair_min_span: std::collections::BTreeMap<(String, String), f64> =
        std::collections::BTreeMap::new();
    let pair_of = |e: &Edge| -> (String, String) {
        if e.source.node <= e.target.node {
            (e.source.node.clone(), e.target.node.clone())
        } else {
            (e.target.node.clone(), e.source.node.clone())
        }
    };
    for e in &scene.edges {
        let a = &l.ports[&port_key(e, true)];
        let b = &l.ports[&port_key(e, false)];
        let span = a.point.manhattan_distance(b.point);
        pair_min_span
            .entry(pair_of(e))
            .and_modify(|m| *m = m.min(span))
            .or_insert(span);
    }
    let mut order: Vec<(u8, i64, i64, i64, usize)> = scene
        .edges
        .iter()
        .enumerate()
        .map(|(idx, e)| {
            let a = &l.ports[&port_key(e, true)];
            let b = &l.ports[&port_key(e, false)];
            let anns = effective_annotations(scene, &e.id, &e.annotations, o.variant.as_deref());
            let has_route = anns.iter().any(|ann| ann.name == "route");
            let can_reuse = prior.is_some_and(|old| {
                old.ports.get(&a.key) == Some(a)
                    && old.ports.get(&b.key) == Some(b)
                    && anns.is_empty()
                    && old
                        .routes
                        .get(&e.id)
                        .is_some_and(|r| route_is_valid(scene, l, e, r, o.margin))
            });
            let priority = if has_route || can_reuse { 0 } else { 1 };
            let min_span = pair_min_span[&pair_of(e)];
            let bend_lb = (bend_lower_bound(a.stub, b.stub, dir(a.side), opposite(dir(b.side)))
                * 10.)
                .round() as i64;
            let span_q = (a.point.manhattan_distance(b.point) * 1000.).round() as i64;
            (
                priority,
                (min_span * 1000.).round() as i64,
                bend_lb,
                span_q,
                idx,
            )
        })
        .collect();
    order.sort();
    let ordered_indices: Vec<usize> = order.into_iter().map(|entry| entry.4).collect();
    let pass1_history = CongestionHistory::default();
    for &idx in &ordered_indices {
        let e = &scene.edges[idx];
        let (route, reused) =
            route_single_edge(scene, o, prior, l, e, &used_ports, &pass1_history, true)?;
        if reused {
            l.reused_routes += 1;
        }
        l.routes.insert(e.id.clone(), route);
    }
    // Up to 2 negotiated-congestion rip-up & reroute passes whenever Pass 1 has crossings or overlaps.
    let (mut best_cross, mut best_ovlp, mut crossed, mut cross_pts) =
        route_crossings_and_overlaps(scene, l);
    if best_cross > 0 || best_ovlp > 0 {
        let mut best_len = route_total_length(&l.routes);
        let mut history = CongestionHistory::default();
        for (pass_idx, mult) in [1.5_f64, 2.5_f64].into_iter().enumerate() {
            if (best_cross == 0 && best_ovlp == 0) || crossed.is_empty() {
                break;
            }
            let penalty = (mult - 1.0) * 0.5 * o.weights.crossing.max(2000.0);
            for eid in &crossed {
                history.edge_mult.insert(eid.clone(), mult);
            }
            for &pt in &cross_pts {
                history.hotspots.push((pt, penalty));
            }
            let mut trial = l.clone();
            let mut rip_indices: Vec<usize> = ordered_indices
                .iter()
                .copied()
                .filter(|&i| {
                    let e = &scene.edges[i];
                    let has_fixed =
                        effective_annotations(scene, &e.id, &e.annotations, o.variant.as_deref())
                            .iter()
                            .any(|a| a.name == "route");
                    if has_fixed {
                        return false;
                    }
                    crossed.contains(&e.id)
                        || (pass_idx == 1 && ordered_indices.len() <= 24)
                        || l.routes
                            .get(&e.id)
                            .is_some_and(|r| coalesced(&r.points).len() >= 5)
                })
                .collect();
            if rip_indices.is_empty() {
                break;
            }
            if pass_idx % 2 == 0 {
                rip_indices.reverse();
            } else {
                // Route formerly-crossed victims first so they can claim direct clear corridors.
                rip_indices.sort_by_key(|&i| !crossed.contains(&scene.edges[i].id));
            }
            for &i in &rip_indices {
                trial.routes.remove(&scene.edges[i].id);
            }
            let mut ok = true;
            for &i in &rip_indices {
                let e = &scene.edges[i];
                match route_single_edge(scene, o, None, &trial, e, &used_ports, &history, false) {
                    Ok((route, _)) => {
                        trial.routes.insert(e.id.clone(), route);
                    }
                    Err(_) => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok && trial.routes.len() == scene.edges.len() {
                let (new_cross, new_ovlp, new_crossed, new_pts) =
                    route_crossings_and_overlaps(scene, &trial);
                let new_len = route_total_length(&trial.routes);
                if (new_ovlp, new_cross) < (best_ovlp, best_cross)
                    || ((new_ovlp, new_cross) == (best_ovlp, best_cross)
                        && new_len < best_len - 1e-6)
                {
                    best_ovlp = new_ovlp;
                    best_cross = new_cross;
                    best_len = new_len;
                    crossed = new_crossed;
                    cross_pts = new_pts;
                    l.routes = trial.routes;
                    l.reused_routes = 0;
                }
            }
        }
    }
    if best_cross > 0 || best_ovlp > 0 {
        swap_edge_ports(scene, o, l, &used_ports);
    }
    dejitter(scene, o, l);
    Ok(())
}
fn is_tight_facing_pair(a: &PortPlacement, b: &PortPlacement) -> bool {
    let (nx, ny) = a.side.normal();
    let dist = (b.point.x - a.point.x) * nx + (b.point.y - a.point.y) * ny;
    let perp = ((b.point.x - a.point.x) * ny - (b.point.y - a.point.y) * nx).abs();
    let max_span = a.point.distance(a.stub) + b.point.distance(b.stub);
    dir(a.side) == opposite(dir(b.side))
        && perp < 1e-6
        && dist >= 8. - 1e-6
        && dist < max_span - 1e-6
}
/// Reused and newly computed paths obey the same geometry contract.
pub(crate) fn route_is_valid(
    scene: &SceneGraph,
    l: &Layout,
    e: &Edge,
    route: &Route,
    margin: f64,
) -> bool {
    route_geometry_valid(scene, l, e, route, margin)
        && route_conflict_counts(l, &e.id, &route.points).1 == 0
}
fn route_geometry_valid(
    scene: &SceneGraph,
    l: &Layout,
    e: &Edge,
    route: &Route,
    margin: f64,
) -> bool {
    let points = &route.points;
    let Some(a) = l.ports.get(&port_key(e, true)) else {
        return false;
    };
    let Some(b) = l.ports.get(&port_key(e, false)) else {
        return false;
    };
    if points.len() < 2
        || points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite())
        || points[0].distance(a.point) > 1e-6
        || points.last().unwrap().distance(b.point) > 1e-6
    {
        return false;
    }
    let simple = coalesced(points);
    if simple.len() < 2 {
        return false;
    }
    let tight_straight = simple.len() == 2 && is_tight_facing_pair(a, b);
    for (port, neighbor) in [(a, simple[1]), (b, simple[simple.len() - 2])] {
        let (nx, ny) = port.side.normal();
        let dx = neighbor.x - port.point.x;
        let dy = neighbor.y - port.point.y;
        let required = if tight_straight {
            8.
        } else {
            (port.stub.x - port.point.x) * nx + (port.stub.y - port.point.y) * ny
        };
        if (dx * ny - dy * nx).abs() > 1e-6 || dx * nx + dy * ny < required - 1e-6 {
            return false;
        }
    }
    let obs = obstacles(scene, l, e, margin);
    let endpoints = endpoint_obstacles(scene, l, e, margin);
    for (i, p) in points.windows(2).enumerate() {
        if p[0].distance(p[1]) < 1e-7
            || (p[0].x - p[1].x).abs() > 1e-6 && (p[0].y - p[1].y).abs() > 1e-6
            || obs.iter().any(|r| r.blocks(p[0], p[1]))
            || i > 0 && i + 2 < points.len() && endpoints.iter().any(|r| r.blocks(p[0], p[1]))
        {
            return false;
        }
    }
    for p in simple.windows(3) {
        if (p[1].x - p[0].x) * (p[2].x - p[1].x) + (p[1].y - p[0].y) * (p[2].y - p[1].y) < -1e-6 {
            return false;
        }
    }
    for (i, p) in simple.windows(2).enumerate() {
        for (j, q) in simple.windows(2).enumerate().skip(i + 2) {
            if i == 0
                && j + 2 == simple.len()
                && e.source.node == e.target.node
                && simple[0].distance(*simple.last().unwrap()) < 1e-6
            {
                continue;
            }
            let horizontal = (p[0].y - p[1].y).abs() < 1e-6;
            let other_horizontal = (q[0].y - q[1].y).abs() < 1e-6;
            if horizontal == other_horizontal {
                if collinear_overlap(p[0], p[1], q[0], q[1]) > 1e-6 {
                    return false;
                }
            } else {
                let (h, v) = if horizontal { (p, q) } else { (q, p) };
                if v[0].x >= h[0].x.min(h[1].x) - 1e-6
                    && v[0].x <= h[0].x.max(h[1].x) + 1e-6
                    && h[0].y >= v[0].y.min(v[1].y) - 1e-6
                    && h[0].y <= v[0].y.max(v[1].y) + 1e-6
                {
                    return false;
                }
            }
        }
    }
    true
}
fn dir(s: Side) -> usize {
    match s {
        Side::E => 0,
        Side::S => 1,
        Side::W => 2,
        Side::N => 3,
    }
}
fn opposite(d: usize) -> usize {
    (d + 2) % 4
}
fn turns(a: usize, b: usize) -> f64 {
    let d = a.abs_diff(b);
    d.min(4 - d) as f64
}
/// Obstacle-free lower bound including the compulsory final stub direction.
pub fn bend_lower_bound(p: Point, t: Point, incoming: usize, arrival: usize) -> f64 {
    let dx = t.x - p.x;
    let dy = t.y - p.y;
    let h = if dx >= 0. { 0 } else { 2 };
    let v = if dy >= 0. { 1 } else { 3 };
    if dx.abs() < 1e-6 && dy.abs() < 1e-6 {
        turns(incoming, arrival)
    } else if dx.abs() < 1e-6 {
        turns(incoming, v) + turns(v, arrival)
    } else if dy.abs() < 1e-6 {
        turns(incoming, h) + turns(h, arrival)
    } else {
        (turns(incoming, h) + 1. + turns(v, arrival))
            .min(turns(incoming, v) + 1. + turns(h, arrival))
    }
}
#[derive(Default)]
struct CongestionHistory {
    edge_mult: std::collections::BTreeMap<String, f64>,
    hotspots: Vec<(Point, f64)>,
}
struct ExistingSegment {
    a: Point,
    b: Point,
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    horiz: bool,
    cross_weight: f64,
}
fn build_existing_segments(
    routed: &Layout,
    base_cross: f64,
    history: &CongestionHistory,
) -> Vec<ExistingSegment> {
    let mut segs = Vec::new();
    for (id, r) in &routed.routes {
        let pts = coalesced(&r.points);
        if pts.len() < 2 {
            continue;
        }
        let mult = if id.starts_with("@reserved-") {
            1.0
        } else {
            history.edge_mult.get(id).copied().unwrap_or(1.0)
        };
        let cross_weight = base_cross * mult;
        for pair in pts.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if a.distance(b) < 1e-7 {
                continue;
            }
            let horiz = (a.y - b.y).abs() < 1e-6;
            segs.push(ExistingSegment {
                a,
                b,
                x_min: a.x.min(b.x),
                x_max: a.x.max(b.x),
                y_min: a.y.min(b.y),
                y_max: a.y.max(b.y),
                horiz,
                cross_weight,
            });
        }
    }
    segs
}
struct StepCostCtx<'a> {
    start: Point,
    target: Point,
    segs: &'a [ExistingSegment],
    hotspots: &'a [(Point, f64)],
}
fn step_route_cost(
    u: Point,
    v: Point,
    d: usize,
    nd: usize,
    prev_pt: Option<Point>,
    ctx: &StepCostCtx<'_>,
) -> f64 {
    step_route_cost_over(u, v, d, nd, prev_pt, ctx, ctx.segs.iter())
}
/// `step_route_cost` over a pre-filtered, order-preserving subset of `ctx.segs`.
fn step_route_cost_over<'s>(
    u: Point,
    v: Point,
    d: usize,
    nd: usize,
    prev_pt: Option<Point>,
    ctx: &StepCostCtx<'_>,
    segs: impl Iterator<Item = &'s ExistingSegment>,
) -> f64 {
    let &StepCostCtx {
        start,
        target,
        hotspots,
        ..
    } = ctx;
    let u_horiz = nd == 0 || nd == 2;
    let vx_min = u.x.min(v.x);
    let vx_max = u.x.max(v.x);
    let vy_min = u.y.min(v.y);
    let vy_max = u.y.max(v.y);
    let mut cost = 0.0;
    for seg in segs {
        if vx_max < seg.x_min - 1e-6
            || vx_min > seg.x_max + 1e-6
            || vy_max < seg.y_min - 1e-6
            || vy_min > seg.y_max + 1e-6
        {
            continue;
        }
        if u_horiz == seg.horiz {
            if collinear_overlap(u, v, seg.a, seg.b) > 1e-6 {
                // Keep prohibitive overlap cost invariant under lattice subdivision.
                let already = prev_pt
                    .is_some_and(|pr| d == nd && collinear_overlap(pr, u, seg.a, seg.b) > 1e-6);
                if !already {
                    cost += 1e6;
                }
            } else if v.distance(target) > 1e-6
                && ((seg.a.distance(start) > 1e-6
                    && seg.a.distance(target) > 1e-6
                    && v.distance(seg.a) < 1e-6)
                    || (seg.b.distance(start) > 1e-6
                        && seg.b.distance(target) > 1e-6
                        && v.distance(seg.b) < 1e-6))
            {
                cost += 1e6;
            }
            continue;
        }
        if proper_crossing(u, v, seg.a, seg.b) {
            cost += seg.cross_weight;
            continue;
        }
        let u_in_open = if seg.horiz {
            (u.y - seg.a.y).abs() < 1e-6 && u.x > seg.x_min + 1e-6 && u.x < seg.x_max - 1e-6
        } else {
            (u.x - seg.a.x).abs() < 1e-6 && u.y > seg.y_min + 1e-6 && u.y < seg.y_max - 1e-6
        };
        if u_in_open && d == nd {
            cost += seg.cross_weight;
            continue;
        }
        for pt in [seg.a, seg.b] {
            if pt.distance(start) <= 1e-6 || pt.distance(target) <= 1e-6 {
                continue;
            }
            let pt_in_uv_open = if u_horiz {
                (pt.y - u.y).abs() < 1e-6 && pt.x > vx_min + 1e-6 && pt.x < vx_max - 1e-6
            } else {
                (pt.x - u.x).abs() < 1e-6 && pt.y > vy_min + 1e-6 && pt.y < vy_max - 1e-6
            };
            if pt_in_uv_open || (v.distance(target) > 1e-6 && v.distance(pt) < 1e-6) {
                cost += 1e6;
                break;
            }
        }
    }
    for &(hp, penalty) in hotspots {
        if hp.x >= vx_min - 1e-6
            && hp.x <= vx_max + 1e-6
            && hp.y >= vy_min - 1e-6
            && hp.y <= vy_max + 1e-6
        {
            cost += penalty;
        }
    }
    cost
}
fn exit_cross_cost(target: Point, d: usize, arrival: usize, segs: &[ExistingSegment]) -> f64 {
    if d != arrival {
        return 0.0;
    }
    let arr_horiz = arrival == 0 || arrival == 2;
    let mut cost = 0.0;
    for seg in segs {
        if seg.horiz == arr_horiz {
            continue;
        }
        let t_in_open = if seg.horiz {
            (target.y - seg.a.y).abs() < 1e-6
                && target.x > seg.x_min + 1e-6
                && target.x < seg.x_max - 1e-6
        } else {
            (target.x - seg.a.x).abs() < 1e-6
                && target.y > seg.y_min + 1e-6
                && target.y < seg.y_max - 1e-6
        };
        if t_in_open {
            cost += seg.cross_weight;
        }
    }
    cost
}
/// Sort lattice tracks and merge values within 1e-3, always keeping the exact
/// `pinned` coordinates (the search's start and target) as representatives.
fn dedup_tracks(values: &mut Vec<f64>, pinned: [f64; 2]) {
    values.extend(pinned);
    values.sort_by(f64::total_cmp);
    let is_pinned = |v: f64| pinned.iter().any(|p| p.to_bits() == v.to_bits());
    let mut out: Vec<f64> = Vec::with_capacity(values.len());
    for &v in values.iter() {
        if let Some(last) = out.last_mut()
            && v - *last < 1e-3
        {
            if v.to_bits() == last.to_bits() {
                continue;
            }
            match (is_pinned(*last), is_pinned(v)) {
                (false, true) => *last = v,
                (true, true) => out.push(v),
                _ => {}
            }
            continue;
        }
        out.push(v);
    }
    *values = out;
}
fn add_midline_tracks(obstacles: &[Rect], tol: f64, xs: &mut Vec<f64>, ys: &mut Vec<f64>) {
    let mut by_left: Vec<usize> = (0..obstacles.len()).collect();
    by_left.sort_by(|&a, &b| obstacles[a].left().total_cmp(&obstacles[b].left()));
    for ia in 0..by_left.len() {
        let a = obstacles[by_left[ia]];
        for ib in (ia + 1)..by_left.len() {
            let b = obstacles[by_left[ib]];
            if b.left() <= a.right() + 1.0 {
                continue;
            }
            let y_lo = a.top().max(b.top());
            let y_hi = a.bottom().min(b.bottom());
            if y_hi > y_lo - tol {
                let band_lo = y_lo.min(y_hi);
                let band_hi = y_lo.max(y_hi);
                let blocked = (0..ib).any(|ic| {
                    let c = obstacles[by_left[ic]];
                    c.right() > a.right() + 1e-6
                        && c.left() < b.left() - 1e-6
                        && c.top() <= band_lo + 1e-6
                        && c.bottom() >= band_hi - 1e-6
                });
                if !blocked {
                    xs.push((a.right() + b.left()) / 2.);
                }
            }
            if b.top() <= a.top() + 1e-6 && b.bottom() >= a.bottom() - 1e-6 {
                break;
            }
        }
    }
    let mut by_top: Vec<usize> = (0..obstacles.len()).collect();
    by_top.sort_by(|&a, &b| obstacles[a].top().total_cmp(&obstacles[b].top()));
    for ia in 0..by_top.len() {
        let a = obstacles[by_top[ia]];
        for ib in (ia + 1)..by_top.len() {
            let b = obstacles[by_top[ib]];
            if b.top() <= a.bottom() + 1.0 {
                continue;
            }
            let x_lo = a.left().max(b.left());
            let x_hi = a.right().min(b.right());
            if x_hi > x_lo - tol {
                let band_lo = x_lo.min(x_hi);
                let band_hi = x_lo.max(x_hi);
                let blocked = (0..ib).any(|ic| {
                    let c = obstacles[by_top[ic]];
                    c.bottom() > a.bottom() + 1e-6
                        && c.top() < b.top() - 1e-6
                        && c.left() <= band_lo + 1e-6
                        && c.right() >= band_hi - 1e-6
                });
                if !blocked {
                    ys.push((a.bottom() + b.top()) / 2.);
                }
            }
            if b.left() <= a.left() + 1e-6 && b.right() >= a.right() - 1e-6 {
                break;
            }
        }
    }
}
fn search(
    start: Point,
    target: Point,
    incoming: usize,
    arrival: usize,
    obstacles: &[Rect],
    routed: &Layout,
    o: &LayoutOptions,
) -> Result<Vec<Point>, Error> {
    search_with_history(
        start,
        target,
        incoming,
        arrival,
        obstacles,
        routed,
        o,
        &CongestionHistory::default(),
    )
}
#[allow(clippy::too_many_arguments)]
fn search_with_history(
    start: Point,
    target: Point,
    incoming: usize,
    arrival: usize,
    obstacles: &[Rect],
    routed: &Layout,
    o: &LayoutOptions,
    history: &CongestionHistory,
) -> Result<Vec<Point>, Error> {
    if obstacles
        .iter()
        .any(|r| r.contains(start) || r.contains(target))
    {
        return Err(Error::Layout(
            "a port stub lies inside an obstacle; increase node separation".into(),
        ));
    }
    let base_cross = o.weights.crossing.max(2000.0);
    let segs = build_existing_segments(routed, base_cross, history);
    // A clear straight segment with no crossing/overlap/T-junction attains the lower bound.
    let straight_dir = if (start.y - target.y).abs() < 1e-7 {
        Some(if target.x >= start.x { 0 } else { 2 })
    } else if (start.x - target.x).abs() < 1e-7 {
        Some(if target.y >= start.y { 1 } else { 3 })
    } else {
        None
    };
    if let Some(sd) = straight_dir
        && sd == incoming
        && sd == arrival
        && !obstacles.iter().any(|r| r.blocks(start, target))
        && step_route_cost(
            start,
            target,
            incoming,
            sd,
            None,
            &StepCostCtx {
                start,
                target,
                segs: &segs,
                hotspots: &history.hotspots,
            },
        ) < 1e-6
        && exit_cross_cost(target, sd, arrival, &segs) < 1e-6
    {
        return Ok(vec![start, target]);
    }
    let mut xs = vec![start.x, target.x];
    let mut ys = vec![start.y, target.y];
    for r in obstacles {
        xs.extend([r.left(), r.right()]);
        ys.extend([r.top(), r.bottom()]);
    }
    add_midline_tracks(obstacles, 2. * o.margin, &mut xs, &mut ys);
    let gap = o.lane_spacing.max(8.);
    let lane_gap = o.lane_spacing.max(12.);
    for p in [start, target] {
        if routed
            .routes
            .iter()
            .any(|(id, r)| !id.starts_with("@reserved-") && r.points.contains(&p))
        {
            xs.extend([p.x - gap, p.x + gap]);
            ys.extend([p.y - gap, p.y + gap]);
        }
    }
    // Bound distant port/route tracks to the relevant search corridor once the
    // lattice would otherwise be large (20+ node DAGs in unoptimized builds).
    let box_pad = (8. * o.stub).max(160.);
    let search_x_min = start.x.min(target.x) - box_pad;
    let search_x_max = start.x.max(target.x) + box_pad;
    let search_y_min = start.y.min(target.y) - box_pad;
    let search_y_max = start.y.max(target.y) + box_pad;
    let dense = obstacles.len() > 14 || routed.routes.len() > 12;
    let outside = |p: &Point| {
        p.x < search_x_min || p.x > search_x_max || p.y < search_y_min || p.y > search_y_max
    };
    for port in routed.ports.values() {
        let p = port.stub;
        if dense && outside(&p) {
            continue;
        }
        xs.extend([p.x - gap, p.x, p.x + gap]);
        ys.extend([p.y - gap, p.y, p.y + gap]);
    }
    for (id, route) in &routed.routes {
        if id.starts_with("@reserved-port:")
            && let Some(p) = route.points.last()
        {
            if dense && outside(p) {
                continue;
            }
            xs.extend([p.x - gap, p.x, p.x + gap]);
            ys.extend([p.y - gap, p.y, p.y + gap]);
        } else if !id.starts_with("@reserved-") {
            let cpts = coalesced(&route.points);
            for pair in cpts.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                if dense
                    && (a.x.max(b.x) < search_x_min
                        || a.x.min(b.x) > search_x_max
                        || a.y.max(b.y) < search_y_min
                        || a.y.min(b.y) > search_y_max)
                {
                    continue;
                }
                if (a.y - b.y).abs() < 1e-6 {
                    ys.extend([a.y - lane_gap, a.y, a.y + lane_gap]);
                } else {
                    xs.extend([a.x - lane_gap, a.x, a.x + lane_gap]);
                }
            }
        }
    }
    if let Some(min) = xs.iter().copied().reduce(f64::min) {
        xs.push(min - o.stub);
    }
    if let Some(max) = xs.iter().copied().reduce(f64::max) {
        xs.push(max + o.stub);
    }
    if let Some(min) = ys.iter().copied().reduce(f64::min) {
        ys.push(min - o.stub);
    }
    if let Some(max) = ys.iter().copied().reduce(f64::max) {
        ys.push(max + o.stub);
    }
    dedup_tracks(&mut xs, [start.x, target.x]);
    dedup_tracks(&mut ys, [start.y, target.y]);
    let nx = xs.len();
    let ny = ys.len();
    // Per-row/column candidate lists. A horizontal step on row `y` can only be
    // blocked by an obstacle whose open band contains `y`, and only meets existing
    // segments whose closed y-range contains `y` (the exact bounding-box rejects
    // used by `Rect::blocks`/`contains` and `step_route_cost`), and vice versa.
    let row_obs: Vec<Vec<usize>> = ys
        .iter()
        .map(|&y| {
            (0..obstacles.len())
                .filter(|&k| y > obstacles[k].top() + 1e-6 && y < obstacles[k].bottom() - 1e-6)
                .collect()
        })
        .collect();
    let col_obs: Vec<Vec<usize>> = xs
        .iter()
        .map(|&x| {
            (0..obstacles.len())
                .filter(|&k| x > obstacles[k].left() + 1e-6 && x < obstacles[k].right() - 1e-6)
                .collect()
        })
        .collect();
    let row_segs: Vec<Vec<usize>> = ys
        .iter()
        .map(|&y| {
            (0..segs.len())
                .filter(|&k| y >= segs[k].y_min - 1e-6 && y <= segs[k].y_max + 1e-6)
                .collect()
        })
        .collect();
    let col_segs: Vec<Vec<usize>> = xs
        .iter()
        .map(|&x| {
            (0..segs.len())
                .filter(|&k| x >= segs[k].x_min - 1e-6 && x <= segs[k].x_max + 1e-6)
                .collect()
        })
        .collect();
    let step_ctx = StepCostCtx {
        start,
        target,
        segs: &segs,
        hotspots: &history.hotspots,
    };
    let points: Vec<Point> = (0..nx * ny)
        .map(|i| Point::new(xs[i % nx], ys[i / nx]))
        .collect();
    let point = |i: usize| points[i];
    // Precompute lattice edge obstacle blockage. Each undirected grid segment
    // (x, y)-(x+1, y) and (x, y)-(x, y+1) is tested against row/column obstacles
    // once, turning the inner A* obstacle test into a single boolean lookup.
    let mut edge_blocked = vec![false; nx * ny * 4];
    for y in 0..ny {
        let py = ys[y];
        for x in 0..nx {
            let px = xs[x];
            let p = Point::new(px, py);
            let i = y * nx + x;
            if x + 1 < nx {
                let v = Point::new(xs[x + 1], py);
                let blocked = row_obs[y]
                    .iter()
                    .any(|&k| obstacles[k].contains(v) || obstacles[k].blocks(p, v));
                edge_blocked[i * 4] = blocked;
                edge_blocked[(i + 1) * 4 + 2] = blocked;
            }
            if y + 1 < ny {
                let v = Point::new(px, ys[y + 1]);
                let blocked = col_obs[x]
                    .iter()
                    .any(|&k| obstacles[k].contains(v) || obstacles[k].blocks(p, v));
                edge_blocked[i * 4 + 1] = blocked;
                edge_blocked[(i + nx) * 4 + 3] = blocked;
            }
        }
    }
    let nearest = |values: &[f64], target: f64| {
        values
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (**a - target).abs().total_cmp(&(**b - target).abs()))
            .map(|(i, _)| i)
            .ok_or_else(|| Error::Layout("empty routing lattice".into()))
    };
    let si = nearest(&ys, start.y)? * nx + nearest(&xs, start.x)?;
    let ti = nearest(&ys, target.y)? * nx + nearest(&xs, target.x)?;
    let mut open = BinaryHeap::new();
    let mut distance = vec![f64::INFINITY; nx * ny * 4];
    let mut parent = vec![usize::MAX; distance.len()];
    let s = si * 4 + incoming;
    let sp = point(si);
    distance[s] = 0.;
    open.push(Queue {
        f: 0.,
        g: 0.,
        x: sp.x,
        y: sp.y,
        dir: incoming,
        state: s,
    });
    let dx_st = target.x - start.x;
    let dy_st = target.y - start.y;
    let backward_dir = [dx_st < -1e-6, dy_st < -1e-6, -dx_st < -1e-6, -dy_st < -1e-6];
    let mut finish = None;
    let mut best = f64::INFINITY;
    while let Some(q) = open.pop() {
        if q.g > distance[q.state] + 1e-8 {
            continue;
        }
        if q.f >= best + 2e-6 {
            break;
        }
        if q.f >= best {
            continue;
        }
        let i = q.state / 4;
        let d = q.state % 4;
        let p = point(i);
        if i == ti {
            if d == opposite(arrival) {
                continue;
            }
            let total =
                q.g + turns(d, arrival) * o.weights.bend + exit_cross_cost(p, d, arrival, &segs);
            if total < best {
                best = total;
                finish = Some(q.state);
            }
            continue;
        }
        let x = i % nx;
        let y = i / nx;
        let was_backward = backward_dir[d];
        let neighbors = [
            (x + 1 < nx, i + 1, 0),
            (y + 1 < ny, i + nx, 1),
            (x > 0, i.saturating_sub(1), 2),
            (y > 0, i.saturating_sub(nx), 3),
        ];
        for (valid, j, nd) in neighbors {
            if !valid || nd == opposite(d) {
                continue;
            }
            if edge_blocked[i * 4 + nd] {
                continue;
            }
            let v = point(j);
            let mut cost =
                p.manhattan_distance(v) / o.grid * o.weights.length + turns(d, nd) * o.weights.bend;
            if !was_backward && backward_dir[nd] {
                cost += o.weights.backward;
            }
            let horizontal_step = nd == 0 || nd == 2;
            let lane_segs = if horizontal_step {
                &row_segs[y]
            } else {
                &col_segs[x]
            };
            if !lane_segs.is_empty() || !history.hotspots.is_empty() {
                let prev_pt = if parent[q.state] != usize::MAX {
                    Some(point(parent[q.state] / 4))
                } else {
                    None
                };
                cost += step_route_cost_over(
                    p,
                    v,
                    d,
                    nd,
                    prev_pt,
                    &step_ctx,
                    lane_segs.iter().map(|&k| &segs[k]),
                );
            }
            let g = q.g + cost;
            let state = j * 4 + nd;
            if g + 1e-8 < distance[state] {
                let heuristic = v.manhattan_distance(target) / o.grid * o.weights.length
                    + bend_lower_bound(v, target, nd, arrival) * o.weights.bend;
                let f = g + heuristic;
                if f >= best {
                    continue;
                }
                distance[state] = g;
                parent[state] = q.state;
                open.push(Queue {
                    f,
                    g,
                    x: v.x,
                    y: v.y,
                    dir: nd,
                    state,
                });
            }
        }
    }
    let Some(mut state) = finish else {
        return Err(Error::Layout(
            "no obstacle-free orthogonal route exists".into(),
        ));
    };
    let mut points = Vec::new();
    loop {
        points.push(point(state / 4));
        if state == s {
            break;
        }
        state = parent[state];
        if state == usize::MAX {
            return Err(Error::Layout("broken A* predecessor chain".into()));
        }
    }
    points.reverse();
    simplify(&mut points);
    Ok(points)
}
pub fn proper_crossing(a: Point, b: Point, c: Point, d: Point) -> bool {
    let h = (a.y - b.y).abs() < 1e-6;
    let v = (c.x - d.x).abs() < 1e-6;
    if h && v {
        c.x > a.x.min(b.x) + 1e-6
            && c.x < a.x.max(b.x) - 1e-6
            && a.y > c.y.min(d.y) + 1e-6
            && a.y < c.y.max(d.y) - 1e-6
    } else if (a.x - b.x).abs() < 1e-6 && (c.y - d.y).abs() < 1e-6 {
        proper_crossing(c, d, a, b)
    } else {
        false
    }
}
pub fn collinear_overlap(a: Point, b: Point, c: Point, d: Point) -> f64 {
    if (a.y - b.y).abs() < 1e-6 && (c.y - d.y).abs() < 1e-6 && (a.y - c.y).abs() < 1e-6 {
        (a.x.max(b.x).min(c.x.max(d.x)) - a.x.min(b.x).max(c.x.min(d.x))).max(0.)
    } else if (a.x - b.x).abs() < 1e-6 && (c.x - d.x).abs() < 1e-6 && (a.x - c.x).abs() < 1e-6 {
        (a.y.max(b.y).min(c.y.max(d.y)) - a.y.min(b.y).max(c.y.min(d.y))).max(0.)
    } else {
        0.
    }
}
fn dedup_points(p: &mut Vec<Point>) {
    p.dedup_by(|a, b| a.distance(*b) < 1e-7);
}
/// A drawing/label view of a route. The public route keeps its mandatory stub
/// vertices; consumers that measure straight runs need their full length.
pub(crate) fn coalesced(points: &[Point]) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::new();
    for &point in points {
        if out.last().is_some_and(|p| p.distance(point) < 1e-7) {
            continue;
        }
        while out.len() >= 2 {
            let a = out[out.len() - 2];
            let b = out[out.len() - 1];
            let straight = ((a.x - b.x).abs() < 1e-6
                && (b.x - point.x).abs() < 1e-6
                && (b.y - a.y) * (point.y - b.y) >= 0.)
                || ((a.y - b.y).abs() < 1e-6
                    && (b.y - point.y).abs() < 1e-6
                    && (b.x - a.x) * (point.x - b.x) >= 0.);
            if !straight {
                break;
            }
            out.pop();
        }
        out.push(point);
    }
    out
}
fn simplify(p: &mut Vec<Point>) {
    dedup_points(p);
    let mut i = 1;
    while i + 1 < p.len() {
        let (a, b, c) = (p[i - 1], p[i], p[i + 1]);
        if ((a.x - b.x).abs() < 1e-6 && (b.x - c.x).abs() < 1e-6)
            || ((a.y - b.y).abs() < 1e-6 && (b.y - c.y).abs() < 1e-6)
        {
            p.remove(i);
        } else {
            i += 1;
        }
    }
}
/// Split shared internal channels with L/straight/R order and centered 1D VPSC lanes.
/// A channel move is committed only when every participating route retains valid geometry.
/// Route an unconstrained fan as one ordered bundle. Outermost branches turn
/// first on fan-out, and last on fan-in, so inner branches cannot cross them.
pub(crate) fn coordinate_fan(scene: &SceneGraph, o: &LayoutOptions, l: &mut Layout) -> bool {
    if scene.nodes.len() < 4 || scene.edges.len() + 1 != scene.nodes.len() {
        return false;
    }
    if !scene
        .edges
        .iter()
        .all(|e| e.source.node == scene.edges[0].source.node)
        && !scene
            .edges
            .iter()
            .all(|e| e.target.node == scene.edges[0].target.node)
    {
        return false;
    }
    let fan_out = scene
        .edges
        .iter()
        .all(|e| e.source.node == scene.edges[0].source.node);
    let hub = if fan_out {
        &scene.edges[0].source.node
    } else {
        &scene.edges[0].target.node
    };
    let y = l.nodes[hub].y;
    let low = scene
        .edges
        .iter()
        .map(|e| l.ports[&port_key(e, true)].stub.x)
        .fold(f64::NEG_INFINITY, f64::max)
        + o.lane_spacing;
    let high = scene
        .edges
        .iter()
        .map(|e| l.ports[&port_key(e, false)].stub.x)
        .fold(f64::INFINITY, f64::min)
        - o.lane_spacing;
    let mut routes = std::collections::BTreeMap::new();
    for e in &scene.edges {
        let leaf = if fan_out {
            &e.target.node
        } else {
            &e.source.node
        };
        let dy = l.nodes[leaf].y - y;
        let rank = scene
            .edges
            .iter()
            .filter(|other| {
                let peer = if fan_out {
                    &other.target.node
                } else {
                    &other.source.node
                };
                let offset = l.nodes[peer].y - y;
                offset.signum() == dy.signum() && offset.abs() > dy.abs() + 1e-6
            })
            .count();
        let lane = if fan_out {
            low + rank as f64 * o.lane_spacing.max(1.)
        } else {
            high - rank as f64 * o.lane_spacing.max(1.)
        };
        if lane < low || lane > high {
            return false;
        }
        let a = &l.ports[&port_key(e, true)];
        let b = &l.ports[&port_key(e, false)];
        let mut route = Route {
            edge: e.id.clone(),
            points: vec![
                a.point,
                a.stub,
                Point::new(lane, a.stub.y),
                Point::new(lane, b.stub.y),
                b.stub,
                b.point,
            ],
        };
        route.points.dedup_by(|a, b| a.distance(*b) < 1e-7);
        if !route_geometry_valid(scene, l, e, &route, o.margin) {
            return false;
        }
        routes.insert(e.id.clone(), route);
    }
    let values: Vec<_> = routes.values().collect();
    for (i, a) in values.iter().enumerate() {
        for b in &values[i + 1..] {
            for p in a.points.windows(2) {
                for q in b.points.windows(2) {
                    if proper_crossing(p[0], p[1], q[0], q[1])
                        || collinear_overlap(p[0], p[1], q[0], q[1]) > 1e-6
                    {
                        return false;
                    }
                }
            }
        }
    }
    l.routes.extend(routes);
    true
}
/// Give a return edge or planar four-node dependency an unmistakable outer rail.
pub(crate) fn coordinate_outer_routes(
    scene: &SceneGraph,
    o: &LayoutOptions,
    l: &mut Layout,
) -> bool {
    let mut candidates = std::collections::BTreeMap::new();
    let top = l
        .nodes
        .values()
        .map(|r| r.top())
        .fold(f64::INFINITY, f64::min)
        - 72.;
    let bottom = l
        .nodes
        .values()
        .map(|r| r.bottom())
        .fold(f64::NEG_INFINITY, f64::max)
        + 72.;
    let left = l
        .nodes
        .values()
        .map(|r| r.left())
        .fold(f64::INFINITY, f64::min)
        - 48.;
    let outer = scene.nodes.len() == 4
        && scene.edges.len() == 4
        && scene.edges.iter().any(|e| {
            l.ports[&port_key(e, true)].side == Side::W
                && l.ports[&port_key(e, false)].side == Side::S
        });
    for (i, e) in scene.edges.iter().enumerate() {
        let a = &l.ports[&port_key(e, true)];
        let b = &l.ports[&port_key(e, false)];
        let points = if a.side == Side::N
            && b.side == Side::N
            && scene.edges[..i].iter().any(|other| {
                other.source.node == e.target.node && other.target.node == e.source.node
            }) {
            vec![
                a.point,
                a.stub,
                Point::new(a.stub.x, top),
                Point::new(b.stub.x, top),
                b.stub,
                b.point,
            ]
        } else if outer && a.side == Side::W && b.side == Side::S {
            vec![
                a.point,
                a.stub,
                Point::new(left, a.stub.y),
                Point::new(left, bottom),
                Point::new(b.stub.x, bottom),
                b.stub,
                b.point,
            ]
        } else if outer && a.side == Side::E && b.side == Side::W {
            let x = (a.stub.x + b.stub.x) / 2.;
            vec![
                a.point,
                a.stub,
                Point::new(x, a.stub.y),
                Point::new(x, b.stub.y),
                b.stub,
                b.point,
            ]
        } else {
            continue;
        };
        let mut route = Route {
            edge: e.id.clone(),
            points,
        };
        route.points.dedup_by(|a, b| a.distance(*b) < 1e-7);
        if !route_geometry_valid(scene, l, e, &route, o.margin) {
            return false;
        }
        candidates.insert(e.id.clone(), route);
    }
    if candidates.is_empty() {
        return false;
    }
    let routes: Vec<_> = scene
        .edges
        .iter()
        .map(|e| candidates.get(&e.id).unwrap_or(&l.routes[&e.id]))
        .collect();
    for (i, a) in routes.iter().enumerate() {
        for b in &routes[i + 1..] {
            for p in a.points.windows(2) {
                for q in b.points.windows(2) {
                    if proper_crossing(p[0], p[1], q[0], q[1])
                        || collinear_overlap(p[0], p[1], q[0], q[1]) > 1e-6
                    {
                        return false;
                    }
                }
            }
        }
    }
    l.routes.extend(candidates);
    true
}
pub fn nudge_channels(scene: &SceneGraph, o: &LayoutOptions, l: &mut Layout) {
    // Sequential WueOrtho (2025) passes: nudge vertical interior tracks first,
    // write updated X coordinates into routes, then recompute horizontal interior
    // tracks from the updated routes before nudging Y.
    nudge_channels_axis(scene, o, l, false);
    nudge_channels_axis(scene, o, l, true);
    // Lane separation shifts interior runs and can leave sub-grid staircase or
    // U-turn jogs behind; straighten them once lanes are final.
    dejitter(scene, o, l);
}
fn nudge_channels_axis(
    scene: &SceneGraph,
    o: &LayoutOptions,
    l: &mut Layout,
    target_horizontal: bool,
) {
    use std::collections::BTreeMap;
    #[derive(Clone)]
    struct Segment {
        edge: String,
        index: usize,
        coord: f64,
        start: f64,
        end: f64,
        legs: [(f64, i8); 2],
        fixed: bool,
        low_stub: f64,
        high_stub: f64,
    }
    let pair_cost = |a: &Segment, b: &Segment| -> i32 {
        let mut cost = 0;
        for (pos, side) in a.legs {
            if side > 0 && pos >= b.start - 1e-6 && pos <= b.end + 1e-6 {
                cost += 2;
            }
        }
        for (pos, side) in b.legs {
            if side < 0 && pos >= a.start - 1e-6 && pos <= a.end + 1e-6 {
                cost += 2;
            }
        }
        let a_bias = a.legs[0].1 as i32 + a.legs[1].1 as i32;
        let b_bias = b.legs[0].1 as i32 + b.legs[1].1 as i32;
        if a_bias > b_bias {
            cost += 1;
        }
        cost
    };
    let mut raw_segments: Vec<Segment> = Vec::new();
    for e in &scene.edges {
        let Some(r) = l.routes.get(&e.id) else {
            continue;
        };
        let locked = effective_annotations(scene, &e.id, &e.annotations, o.variant.as_deref())
            .iter()
            .any(|a| a.name == "route");
        let a_port = l.ports.get(&port_key(e, true));
        let b_port = l.ports.get(&port_key(e, false));
        for i in 1..r.points.len().saturating_sub(2) {
            let (a, b) = (r.points[i], r.points[i + 1]);
            let horizontal = (a.y - b.y).abs() < 1e-6;
            if horizontal != target_horizontal {
                continue;
            }
            let coord = if horizontal { a.y } else { a.x };
            let before = r.points[i - 1];
            let after = r.points[i + 2];
            let leg = |pt: Point, other: Point| -> (f64, i8) {
                let (pos, delta) = if horizontal {
                    (pt.x, other.y - pt.y)
                } else {
                    (pt.y, other.x - pt.x)
                };
                let side = if delta.abs() <= 1e-6 {
                    0
                } else if delta > 0. {
                    1
                } else {
                    -1
                };
                (pos, side)
            };
            let (start, end) = if horizontal {
                (a.x.min(b.x), a.x.max(b.x))
            } else {
                (a.y.min(b.y), a.y.max(b.y))
            };
            if end - start < o.margin.min(8.) {
                continue;
            }
            let legs = [leg(a, before), leg(b, after)];
            let fixed = locked || legs[0].1 == 0 || legs[1].1 == 0;
            let mut low_stub = f64::NEG_INFINITY;
            let mut high_stub = f64::INFINITY;
            if !fixed {
                if i == 1
                    && legs[0].1 != 0
                    && let Some(port) = a_port
                {
                    let (nx, ny) = port.side.normal();
                    let n = if horizontal { ny } else { nx };
                    let s_coord = if horizontal { port.stub.y } else { port.stub.x };
                    if n > 0. {
                        low_stub = low_stub.max(s_coord);
                    } else if n < 0. {
                        high_stub = high_stub.min(s_coord);
                    }
                }
                if i + 2 == r.points.len() - 1
                    && legs[1].1 != 0
                    && let Some(port) = b_port
                {
                    let (nx, ny) = port.side.normal();
                    let n = if horizontal { ny } else { nx };
                    let s_coord = if horizontal { port.stub.y } else { port.stub.x };
                    if n > 0. {
                        low_stub = low_stub.max(s_coord);
                    } else if n < 0. {
                        high_stub = high_stub.min(s_coord);
                    }
                }
            }
            raw_segments.push(Segment {
                edge: e.id.clone(),
                index: i,
                coord,
                start,
                end,
                legs,
                fixed,
                low_stub,
                high_stub,
            });
        }
    }
    raw_segments.sort_by(|a, b| {
        a.coord
            .total_cmp(&b.coord)
            .then_with(|| a.start.total_cmp(&b.start))
            .then_with(|| a.edge.cmp(&b.edge))
    });
    let mut buckets: Vec<(f64, Vec<Segment>)> = Vec::new();
    for seg in raw_segments {
        if let Some((anchor, list)) = buckets.last_mut()
            && seg.coord - *anchor <= 0.5 + 1e-9
        {
            list.push(seg);
        } else {
            buckets.push((seg.coord, vec![seg]));
        }
    }
    let valid_fixed_order = |items: &[Segment]| -> bool {
        let mut last_fixed = f64::NEG_INFINITY;
        for s in items {
            if s.fixed {
                if s.coord < last_fixed - 1e-6 {
                    return false;
                }
                last_fixed = s.coord;
            }
        }
        true
    };
    let edge_conflicts = |routes: &BTreeMap<String, Route>,
                          targets: &BTreeMap<String, Route>|
     -> (usize, usize) {
        let mut cross = 0;
        let mut ovlp = 0;
        let all_coalesced: BTreeMap<&str, Vec<Point>> = scene
            .edges
            .iter()
            .filter_map(|e| {
                targets
                    .get(&e.id)
                    .or_else(|| routes.get(&e.id))
                    .map(|r| (e.id.as_str(), coalesced(&r.points)))
            })
            .collect();
        for id_a in targets.keys() {
            let Some(ca) = all_coalesced.get(id_a.as_str()) else {
                continue;
            };
            for (&id_b, cb) in &all_coalesced {
                if id_a.as_str() == id_b || (targets.contains_key(id_b) && id_b < id_a.as_str()) {
                    continue;
                }
                for p in ca.windows(2) {
                    for q in cb.windows(2) {
                        if proper_crossing(p[0], p[1], q[0], q[1]) {
                            cross += 1;
                        } else if collinear_overlap(p[0], p[1], q[0], q[1]) > 1e-6 {
                            ovlp += 1;
                        }
                    }
                }
            }
        }
        (cross, ovlp)
    };
    for (_, mut segments) in buckets {
        segments.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.edge.cmp(&b.edge)));
        let mut groups: Vec<Vec<Segment>> = Vec::new();
        let mut end = f64::NEG_INFINITY;
        for s in segments {
            if s.start >= end - 1e-6 || groups.is_empty() {
                end = s.end;
                groups.push(vec![s]);
            } else {
                end = end.max(s.end);
                groups.last_mut().unwrap().push(s);
            }
        }
        for mut group in groups {
            if group.len() < 2 || group.iter().all(|s| s.fixed) {
                continue;
            }
            {
                let mut seen = std::collections::BTreeSet::new();
                if !group.iter().all(|s| seen.insert(&s.edge)) {
                    continue;
                }
            }
            group.sort_by(|a, b| {
                let ab = a.legs[0].1 as i32 + a.legs[1].1 as i32;
                let bb = b.legs[0].1 as i32 + b.legs[1].1 as i32;
                ab.cmp(&bb)
                    .then_with(|| a.coord.total_cmp(&b.coord))
                    .then_with(|| a.edge.cmp(&b.edge))
            });
            if !valid_fixed_order(&group) {
                let mut fixed_segs: Vec<Segment> =
                    group.iter().filter(|s| s.fixed).cloned().collect();
                fixed_segs.sort_by(|a, b| a.coord.total_cmp(&b.coord).then(a.edge.cmp(&b.edge)));
                let mut fi = 0;
                for s in &mut group {
                    if s.fixed {
                        *s = fixed_segs[fi].clone();
                        fi += 1;
                    }
                }
            }
            let total_cost = |items: &[Segment]| -> i32 {
                let mut total = 0;
                for p in 0..items.len() {
                    for q in p + 1..items.len() {
                        total += pair_cost(&items[p], &items[q]);
                    }
                }
                total
            };
            if group.len() <= 24 {
                let mut current = total_cost(&group);
                for _ in 0..group.len() * 2 {
                    if current == 0 {
                        break;
                    }
                    let mut best_move = None;
                    for from in 0..group.len() {
                        for to in 0..group.len() {
                            if from == to {
                                continue;
                            }
                            let mut trial = group.clone();
                            let item = trial.remove(from);
                            trial.insert(to, item);
                            if !valid_fixed_order(&trial) {
                                continue;
                            }
                            let score = total_cost(&trial);
                            if score < current {
                                current = score;
                                best_move = Some(trial);
                            }
                        }
                    }
                    if let Some(next) = best_move {
                        group = next;
                    } else {
                        break;
                    }
                }
            }
            let mut base_bounds: Vec<(f64, f64)> = Vec::with_capacity(group.len());
            let mut bounds_ok = true;
            for s in &group {
                if s.fixed {
                    base_bounds.push((s.coord, s.coord));
                    continue;
                }
                let e = scene.edge(&s.edge).unwrap();
                let mut obs = obstacles(scene, l, e, o.margin);
                obs.extend(endpoint_obstacles(scene, l, e, o.margin));
                let mut low = (s.coord - 256.).max(s.low_stub);
                let mut high = (s.coord + 256.).min(s.high_stub);
                for r in obs {
                    let along = if target_horizontal {
                        (r.left(), r.right())
                    } else {
                        (r.top(), r.bottom())
                    };
                    if along.0 >= s.end - 1e-6 || along.1 <= s.start + 1e-6 {
                        continue;
                    }
                    let normal = if target_horizontal {
                        (r.top(), r.bottom())
                    } else {
                        (r.left(), r.right())
                    };
                    if normal.1 <= s.coord + 1e-6 {
                        low = low.max(normal.1);
                    }
                    if normal.0 >= s.coord - 1e-6 {
                        high = high.min(normal.0);
                    }
                }
                if low > high {
                    bounds_ok = false;
                    break;
                }
                base_bounds.push((low, high));
            }
            if !bounds_ok {
                continue;
            }
            let desired: Vec<f64> = group.iter().map(|s| s.coord).collect();
            let weights = vec![1.; group.len()];
            let before_targets: BTreeMap<String, Route> = group
                .iter()
                .map(|s| (s.edge.clone(), l.routes[&s.edge].clone()))
                .collect();
            let (before_cross, before_ovlp) = edge_conflicts(&l.routes, &before_targets);
            let preferred_spacing = o.lane_spacing.max(12.);
            let spacing_trials = if o.lane_spacing < preferred_spacing - 1e-6 {
                vec![preferred_spacing, o.lane_spacing]
            } else {
                vec![preferred_spacing]
            };
            let mut committed = false;
            for spacing in spacing_trials {
                let mut constraints = Vec::new();
                for (i, s) in group.iter().enumerate() {
                    if i > 0 && !(group[i - 1].fixed && s.fixed) {
                        constraints.push(crate::solver::Constraint::separation(i - 1, i, spacing));
                    }
                    if s.fixed {
                        constraints.push(crate::solver::Constraint::fixed(i, s.coord));
                    } else {
                        let (low, high) = base_bounds[i];
                        constraints.push(crate::solver::Constraint {
                            coefficients: vec![(i, 1.)],
                            rhs: low,
                            equality: false,
                        });
                        constraints.push(crate::solver::Constraint {
                            coefficients: vec![(i, -1.)],
                            rhs: -high,
                            equality: false,
                        });
                    }
                }
                let Ok(lanes) = crate::solver::project(&desired, &weights, &constraints) else {
                    continue;
                };
                let mut candidates = BTreeMap::new();
                for (s, lane) in group.iter().zip(lanes) {
                    let route = candidates
                        .entry(s.edge.clone())
                        .or_insert_with(|| l.routes[&s.edge].clone());
                    for p in &mut route.points[s.index..=s.index + 1] {
                        if target_horizontal {
                            p.y = lane;
                        } else {
                            p.x = lane;
                        }
                    }
                }
                let mut valid = true;
                for (id, r) in &candidates {
                    let e = scene.edge(id).unwrap();
                    if !route_geometry_valid(scene, l, e, r, o.margin) {
                        valid = false;
                        break;
                    }
                    let external = obstacles(scene, l, e, o.margin);
                    let mut internal = external.clone();
                    internal.extend(endpoint_obstacles(scene, l, e, o.margin));
                    for (i, p) in r.points.windows(2).enumerate() {
                        if (p[0].x - p[1].x).abs() > 1e-6 && (p[0].y - p[1].y).abs() > 1e-6 {
                            valid = false;
                            break;
                        }
                        let obs = if i == 0 || i + 2 == r.points.len() {
                            &external
                        } else {
                            &internal
                        };
                        if obs.iter().any(|ob| ob.blocks(p[0], p[1])) {
                            valid = false;
                            break;
                        }
                    }
                    if !valid {
                        break;
                    }
                    let a = &l.ports[&port_key(e, true)];
                    let b = &l.ports[&port_key(e, false)];
                    for (port, tip) in [(a, r.points[1]), (b, r.points[r.points.len() - 2])] {
                        let (nx, ny) = port.side.normal();
                        let dx = tip.x - port.point.x;
                        let dy = tip.y - port.point.y;
                        if dx * nx + dy * ny < o.margin - 1e-6 || (dx * ny - dy * nx).abs() > 1e-6 {
                            valid = false;
                        }
                    }
                    if !valid {
                        break;
                    }
                }
                if valid {
                    let (after_cross, after_ovlp) = edge_conflicts(&l.routes, &candidates);
                    if after_cross > before_cross || after_ovlp > before_ovlp {
                        valid = false;
                    }
                }
                if valid {
                    l.routes.extend(candidates);
                    committed = true;
                    break;
                }
            }
            if !committed {
                l.diagnostics.push("A shared channel could not be separated without breaking port/obstacle clearance".into());
            }
        }
    }
}

#[cfg(test)]
mod ordering_tests {
    use super::*;
    #[test]
    fn equality_and_heap_order_agree_for_every_tie_break() {
        let entries = [
            Queue {
                f: 0.,
                g: 1.,
                x: 0.,
                y: 0.,
                dir: 0,
                state: 1,
            },
            Queue {
                f: 0.,
                g: 2.,
                x: 0.,
                y: 0.,
                dir: 0,
                state: 1,
            },
            Queue {
                f: -0.,
                g: 1.,
                x: 0.,
                y: 0.,
                dir: 0,
                state: 1,
            },
            Queue {
                f: f64::NAN,
                g: 1.,
                x: 4.,
                y: 8.,
                dir: 2,
                state: 2,
            },
        ];
        for a in entries {
            for b in entries {
                assert_eq!(a == b, a.cmp(&b) == Ordering::Equal);
            }
        }
    }
    #[test]
    fn equal_f_ties_prefer_larger_g_deterministically() {
        let mut heap = BinaryHeap::new();
        let shallow = Queue {
            f: 100.,
            g: 20.,
            x: 10.,
            y: 20.,
            dir: 0,
            state: 1,
        };
        let deep = Queue {
            f: 100. + 2e-7,
            g: 80.,
            x: 30.,
            y: 40.,
            dir: 1,
            state: 2,
        };
        heap.push(shallow);
        heap.push(deep);
        assert_eq!(heap.pop().unwrap().state, 2);
        assert_eq!(heap.pop().unwrap().state, 1);
    }
}

#[cfg(test)]
mod stage2_routing_tests {
    use super::*;
    #[test]
    fn split_step_crossing_detects_crossings_over_subdivided_routes() {
        let o = LayoutOptions::default();
        let mut routed = Layout::default();
        routed.routes.insert(
            "existing".into(),
            Route {
                edge: "existing".into(),
                points: vec![
                    Point::new(-100., 0.),
                    Point::new(-20., 0.),
                    Point::new(0., 0.),
                    Point::new(20., 0.),
                    Point::new(100., 0.),
                ],
            },
        );
        let segs = build_existing_segments(
            &routed,
            o.weights.crossing.max(2000.0),
            &CongestionHistory::default(),
        );
        // Existing route coalesces into a single horizontal span [-100, 100].
        assert_eq!(segs.len(), 1);
        // Step arrives at u=(0, 0) from North (d=1) and exits South (nd=1): must charge >= 2000.0!
        let cross_cost = step_route_cost(
            Point::new(0., 0.),
            Point::new(0., 40.),
            1,
            1,
            Some(Point::new(0., -40.)),
            &StepCostCtx {
                start: Point::new(0., -40.),
                target: Point::new(0., 40.),
                segs: &segs,
                hotspots: &[],
            },
        );
        assert!(cross_cost >= 2000.0);
        // Same step turning onto existing segment overlaps: must charge 1e6 once (subdivision invariant).
        let overlap_ctx = StepCostCtx {
            start: Point::new(0., -40.),
            target: Point::new(100., 0.),
            segs: &segs,
            hotspots: &[],
        };
        let first_ovlp = step_route_cost(
            Point::new(0., 0.),
            Point::new(20., 0.),
            1,
            0,
            Some(Point::new(0., -40.)),
            &overlap_ctx,
        );
        let cont_ovlp = step_route_cost(
            Point::new(20., 0.),
            Point::new(40., 0.),
            0,
            0,
            Some(Point::new(0., 0.)),
            &overlap_ctx,
        );
        assert!((first_ovlp - 1e6).abs() < 1e-6);
        assert!(cont_ovlp < 1e-6);
    }
    #[test]
    fn compound_container_parent_to_child_and_intra_container_barriers() {
        let d = crate::compile(
            "c {\n  label: \"Container\"\n  a: Child A\n  b: Child B\n  a -> b: internal\n}\nc -> c.a: parent_to_child\n",
        )
        .unwrap();
        let c_rect = d.layout.nodes["c"];
        let internal = coalesced(&d.layout.routes["c.a->c.b"].points);
        for p in &internal[1..internal.len() - 1] {
            assert!(
                p.x >= c_rect.left() - 1e-6
                    && p.x <= c_rect.right() + 1e-6
                    && p.y >= c_rect.top() - 1e-6
                    && p.y <= c_rect.bottom() + 1e-6,
                "intra-container point escaped LCA container: {p:?} vs {c_rect:?}"
            );
        }
        assert!(d.layout.routes.contains_key("c->c.a"));
        crate::layout::validate(&d.scene, &d.layout, &LayoutOptions::default()).unwrap();
    }
    #[test]
    fn tight_facing_ports_at_16px_gap_connect_directly() {
        let o = LayoutOptions::default();
        let scene = crate::model::lower(&crate::syntax::parse("a.E -> b.W\n"));
        let mut l = Layout::default();
        // Node centers separated so gap = 16px (a.right = -8, b.left = +8).
        l.nodes.insert("a".into(), Rect::new(-64., 0., 112., 48.));
        l.nodes.insert("b".into(), Rect::new(64., 0., 112., 48.));
        let e = &scene.edges[0];
        l.ports.insert(
            port_key(e, true),
            PortPlacement {
                key: port_key(e, true),
                side: Side::E,
                point: Point::new(-8., 0.),
                stub: Point::new(8., 0.),
            },
        );
        l.ports.insert(
            port_key(e, false),
            PortPlacement {
                key: port_key(e, false),
                side: Side::W,
                point: Point::new(8., 0.),
                stub: Point::new(-8., 0.),
            },
        );
        route_all(&scene, &o, None, &mut l).unwrap();
        let pts = &l.routes[&e.id].points;
        assert_eq!(pts.len(), 2);
        assert_eq!(pts[0], Point::new(-8., 0.));
        assert_eq!(pts[1], Point::new(8., 0.));
        crate::layout::validate(&scene, &l, &o).unwrap();
    }
    #[test]
    fn sequential_epsilon_bucketed_nudge_locks_side_zero_stub_channels() {
        let o = LayoutOptions::default();
        let scene = crate::model::lower(&crate::syntax::parse("a.E -> b.W\nc.E -> d.W\n"));
        let mut l = Layout::default();
        l.nodes.insert("a".into(), Rect::new(-200., 0., 112., 48.));
        l.nodes.insert("b".into(), Rect::new(200., 0., 112., 48.));
        l.nodes
            .insert("c".into(), Rect::new(-200., 120., 112., 48.));
        l.nodes.insert("d".into(), Rect::new(200., 120., 112., 48.));
        let e0 = &scene.edges[0];
        let e1 = &scene.edges[1];
        // e0 runs straight collinear out of a.E at y=0 (side == 0).
        // e1 enters the same y=0.25 channel (within 0.5px epsilon bucket) via perpendicular legs.
        l.ports.insert(
            port_key(e0, true),
            PortPlacement {
                key: port_key(e0, true),
                side: Side::E,
                point: Point::new(-144., 0.),
                stub: Point::new(-128., 0.),
            },
        );
        l.ports.insert(
            port_key(e0, false),
            PortPlacement {
                key: port_key(e0, false),
                side: Side::W,
                point: Point::new(144., 0.),
                stub: Point::new(128., 0.),
            },
        );
        l.ports.insert(
            port_key(e1, true),
            PortPlacement {
                key: port_key(e1, true),
                side: Side::E,
                point: Point::new(-144., 120.),
                stub: Point::new(-128., 120.),
            },
        );
        l.ports.insert(
            port_key(e1, false),
            PortPlacement {
                key: port_key(e1, false),
                side: Side::W,
                point: Point::new(144., 120.),
                stub: Point::new(128., 120.),
            },
        );
        l.routes.insert(
            e0.id.clone(),
            Route {
                edge: e0.id.clone(),
                points: vec![
                    Point::new(-144., 0.),
                    Point::new(-128., 0.),
                    Point::new(128., 0.),
                    Point::new(144., 0.),
                ],
            },
        );
        l.routes.insert(
            e1.id.clone(),
            Route {
                edge: e1.id.clone(),
                points: vec![
                    Point::new(-144., 120.),
                    Point::new(-128., 120.),
                    Point::new(-80., 120.),
                    Point::new(-80., 0.25),
                    Point::new(80., 0.25),
                    Point::new(80., 120.),
                    Point::new(128., 120.),
                    Point::new(144., 120.),
                ],
            },
        );
        nudge_channels(&scene, &o, &mut l);
        assert!(l.diagnostics.is_empty(), "{:?}", l.diagnostics);
        assert!((l.routes[&e0.id].points[1].y - 0.).abs() < 1e-6);
        assert!((l.routes[&e1.id].points[3].y - 0.).abs() >= 12. - 1e-6);
        crate::layout::validate(&scene, &l, &o).unwrap();
    }
    #[test]
    fn dejitter_straightens_sub_12px_z_jog_when_clear() {
        let o = LayoutOptions::default();
        let scene = crate::model::lower(&crate::syntax::parse("a.E -> b.W\n"));
        let mut l = Layout::default();
        l.nodes.insert("a".into(), Rect::new(-120., 0., 80., 48.));
        l.nodes.insert("b".into(), Rect::new(120., 40., 80., 48.));
        let e = &scene.edges[0];
        l.ports.insert(
            port_key(e, true),
            PortPlacement {
                key: port_key(e, true),
                side: Side::E,
                point: Point::new(-80., 0.),
                stub: Point::new(-64., 0.),
            },
        );
        l.ports.insert(
            port_key(e, false),
            PortPlacement {
                key: port_key(e, false),
                side: Side::W,
                point: Point::new(80., 40.),
                stub: Point::new(64., 40.),
            },
        );
        // Interior 6px Z-jog from (-40, 0) to (-40, 6) before running to (20, 6) -> (20, 40).
        l.routes.insert(
            e.id.clone(),
            Route {
                edge: e.id.clone(),
                points: vec![
                    Point::new(-80., 0.),
                    Point::new(-64., 0.),
                    Point::new(-40., 0.),
                    Point::new(-40., 6.),
                    Point::new(20., 6.),
                    Point::new(20., 40.),
                    Point::new(64., 40.),
                    Point::new(80., 40.),
                ],
            },
        );
        dejitter(&scene, &o, &mut l);
        let c = coalesced(&l.routes[&e.id].points);
        assert!(
            c.windows(2)
                .enumerate()
                .all(|(k, p)| k == 0 || k + 2 == c.len() || p[0].distance(p[1]) >= o.grid - 1e-6),
            "Z-jog should be removed: {c:?}"
        );
        crate::layout::validate(&scene, &l, &o).unwrap();
    }

    #[test]
    fn dejitter_collapses_narrow_uturn_dogleg() {
        let o = LayoutOptions::default();
        let scene = crate::model::lower(&crate::syntax::parse("a.E -> b.W\n"));
        let mut l = Layout::default();
        l.nodes.insert("a".into(), Rect::new(-120., 0., 80., 48.));
        l.nodes.insert("b".into(), Rect::new(120., 40., 80., 48.));
        let e = &scene.edges[0];
        l.ports.insert(
            port_key(e, true),
            PortPlacement {
                key: port_key(e, true),
                side: Side::E,
                point: Point::new(-80., 0.),
                stub: Point::new(-64., 0.),
            },
        );
        l.ports.insert(
            port_key(e, false),
            PortPlacement {
                key: port_key(e, false),
                side: Side::W,
                point: Point::new(80., 40.),
                stub: Point::new(64., 40.),
            },
        );
        // Narrow 8px U-turn: East to (40, 0), North by 8px to (40, 8), West to (20, 8),
        // South/East to (20, 40) -> (64, 40) -> (80, 40).
        l.routes.insert(
            e.id.clone(),
            Route {
                edge: e.id.clone(),
                points: vec![
                    Point::new(-80., 0.),
                    Point::new(-64., 0.),
                    Point::new(-20., 0.),
                    Point::new(40., 0.),
                    Point::new(40., 8.),
                    Point::new(20., 8.),
                    Point::new(20., 40.),
                    Point::new(64., 40.),
                    Point::new(80., 40.),
                ],
            },
        );
        dejitter(&scene, &o, &mut l);
        let c = coalesced(&l.routes[&e.id].points);
        assert!(
            c.windows(2)
                .enumerate()
                .all(|(k, p)| k == 0 || k + 2 == c.len() || p[0].distance(p[1]) >= o.grid - 1e-6),
            "U-turn jog should be removed: {c:?}"
        );
        assert!(
            c.len() <= 6,
            "Narrow U-turn should collapse redundant bends: {c:?}"
        );
        crate::layout::validate(&scene, &l, &o).unwrap();
    }

    #[test]
    fn dense_24_node_30_edge_dag_routes_under_budget() {
        let mut d = String::new();
        // 4 ranks of 6 nodes each
        for r in 0..4 {
            for c in 0..6 {
                let id = format!("n_{r}_{c}");
                d.push_str(&format!("{id}: \"Node {r}-{c}\" [shape: rect]\n"));
            }
        }
        // 30 forward edges across ranks
        let mut edge_count = 0;
        'outer: for r in 0..3 {
            for c in 0..6 {
                let u = format!("n_{r}_{c}");
                let v = format!("n_{}_{c}", r + 1);
                d.push_str(&format!("{u} -> {v}\n"));
                edge_count += 1;
                if edge_count == 30 {
                    break 'outer;
                }
                if c + 1 < 6 {
                    let v2 = format!("n_{}_{}", r + 1, c + 1);
                    d.push_str(&format!("{u} -> {v2}\n"));
                    edge_count += 1;
                    if edge_count == 30 {
                        break 'outer;
                    }
                }
            }
        }
        assert_eq!(edge_count, 30);
        let scene = crate::model::lower(&crate::syntax::parse(&d));
        let o = LayoutOptions::default();
        let mut l = Layout::default();
        for (i, node) in scene.nodes.iter().enumerate() {
            let r = (i / 6) as f64;
            let c = (i % 6) as f64;
            l.nodes
                .insert(node.id.clone(), Rect::new(c * 160., r * 120., 100., 50.));
        }
        let mut edges_by_node: std::collections::BTreeMap<(String, bool), Vec<String>> =
            std::collections::BTreeMap::new();
        for e in &scene.edges {
            edges_by_node
                .entry((e.source.node.clone(), true))
                .or_default()
                .push(e.id.clone());
            edges_by_node
                .entry((e.target.node.clone(), false))
                .or_default()
                .push(e.id.clone());
        }
        for e in &scene.edges {
            let src_rect = &l.nodes[&e.source.node];
            let tgt_rect = &l.nodes[&e.target.node];
            let src_group = &edges_by_node[&(e.source.node.clone(), true)];
            let tgt_group = &edges_by_node[&(e.target.node.clone(), false)];
            let src_idx = src_group.iter().position(|id| id == &e.id).unwrap();
            let tgt_idx = tgt_group.iter().position(|id| id == &e.id).unwrap();
            let src_x = src_rect.left()
                + (src_idx as f64 + 1.) / (src_group.len() as f64 + 1.) * src_rect.w;
            let tgt_x = tgt_rect.left()
                + (tgt_idx as f64 + 1.) / (tgt_group.len() as f64 + 1.) * tgt_rect.w;
            l.ports.insert(
                port_key(e, true),
                PortPlacement {
                    key: port_key(e, true),
                    side: Side::S,
                    point: Point::new(src_x, src_rect.bottom()),
                    stub: Point::new(src_x, src_rect.bottom() + o.stub),
                },
            );
            l.ports.insert(
                port_key(e, false),
                PortPlacement {
                    key: port_key(e, false),
                    side: Side::N,
                    point: Point::new(tgt_x, tgt_rect.top()),
                    stub: Point::new(tgt_x, tgt_rect.top() - o.stub),
                },
            );
        }
        let start = std::time::Instant::now();
        route_all(&scene, &o, None, &mut l).unwrap();
        let elapsed = start.elapsed();
        assert!(
            elapsed < std::time::Duration::from_millis(500),
            "Dense 24-node / 30-edge DAG routing took {:?}, expected < 500ms",
            elapsed
        );
        assert_eq!(l.routes.len(), 30);
    }
}

/// Preserve a detour's corridor while moving only its terminal legs. Revalidate
/// the entire path, and bound its excess length so detours cannot grow forever.
pub(crate) fn retain_detour(
    scene: &SceneGraph,
    l: &Layout,
    edge: &Edge,
    prior: &Layout,
    margin: f64,
) -> Option<Route> {
    let old = prior.routes.get(&edge.id)?;
    let fresh = l.routes.get(&edge.id)?;
    let mut points = coalesced(&old.points);
    if points.len() <= 4 && coalesced(&fresh.points).len() <= 4 {
        return None;
    }
    if points.len() < 4 {
        return None;
    }
    let a = l.ports.get(&port_key(edge, true))?;
    let b = l.ports.get(&port_key(edge, false))?;
    if prior.ports.get(&a.key)?.side != a.side || prior.ports.get(&b.key)?.side != b.side {
        return None;
    }
    let last = points.len() - 1;
    if (points[0].y - points[1].y).abs() < 1e-6 {
        points[1].y = a.point.y;
    } else {
        points[1].x = a.point.x;
    }
    if (points[last].y - points[last - 1].y).abs() < 1e-6 {
        points[last - 1].y = b.point.y;
    } else {
        points[last - 1].x = b.point.x;
    }
    // A turn at the old stub must follow an outward-moving port. Move its
    // whole perpendicular run, not just one vertex (which would create a
    // diagonal or a tiny U-turn and discard an otherwise valid corridor).
    for (port, turn, next) in [(a, 1, 2), (b, last - 1, last - 2)] {
        let (nx, ny) = port.side.normal();
        if nx != 0. && (points[turn].x - port.stub.x) * nx < 0. {
            points[turn].x = port.stub.x;
            points[next].x = port.stub.x;
        } else if ny != 0. && (points[turn].y - port.stub.y) * ny < 0. {
            points[turn].y = port.stub.y;
            points[next].y = port.stub.y;
        }
    }
    // Tight corridors next to moving bodies must follow their clearance
    // boundary, rather than become invalid on alternating subpixel frames.
    for node in &scene.nodes {
        let (Some(old), Some(new)) = (prior.nodes.get(&node.id), l.nodes.get(&node.id)) else {
            continue;
        };
        if old == new || old.center().distance(new.center()) > 16. {
            continue;
        }
        let old = old.inflate(margin);
        let new = new.inflate(margin);
        for i in 1..last - 1 {
            let (p, q) = (points[i], points[i + 1]);
            if (p.y - q.y).abs() < 1e-6 && p.x.max(q.x) > new.left() && p.x.min(q.x) < new.right() {
                let y = if p.y <= old.top() + 1e-6 && p.y > new.top() {
                    Some(new.top())
                } else if p.y >= old.bottom() - 1e-6 && p.y < new.bottom() {
                    Some(new.bottom())
                } else {
                    None
                };
                if let Some(y) = y {
                    points[i].y = y;
                    points[i + 1].y = y;
                }
            } else if (p.x - q.x).abs() < 1e-6
                && p.y.max(q.y) > new.top()
                && p.y.min(q.y) < new.bottom()
            {
                let x = if p.x <= old.left() + 1e-6 && p.x > new.left() {
                    Some(new.left())
                } else if p.x >= old.right() - 1e-6 && p.x < new.right() {
                    Some(new.right())
                } else {
                    None
                };
                if let Some(x) = x {
                    points[i].x = x;
                    points[i + 1].x = x;
                }
            }
        }
    }
    points[0] = a.point;
    points[last] = b.point;
    points.insert(1, a.stub);
    points.insert(points.len() - 1, b.stub);
    dedup_points(&mut points);
    let route = Route {
        edge: edge.id.clone(),
        points,
    };
    let length = |r: &Route| {
        r.points
            .windows(2)
            .map(|p| p[0].distance(p[1]))
            .sum::<f64>()
    };
    let (ret_cross, ret_ovlp) = route_conflict_counts(l, &edge.id, &route.points);
    let (fresh_cross, fresh_ovlp) = route_conflict_counts(l, &edge.id, &fresh.points);
    (length(&route) <= length(fresh) * 1.25 + 32.
        && route_is_valid(scene, l, edge, &route, margin)
        && ret_ovlp <= fresh_ovlp
        && ret_cross <= fresh_cross)
        .then_some(route)
}

#[cfg(test)]
mod detour_tests {
    use super::*;
    fn path(l: &Layout, e: &Edge, y: f64) -> Route {
        let a = &l.ports[&port_key(e, true)];
        let b = &l.ports[&port_key(e, false)];
        Route {
            edge: e.id.clone(),
            points: vec![
                a.point,
                a.stub,
                Point::new(a.stub.x, y),
                Point::new(b.stub.x, y),
                b.stub,
                b.point,
            ],
        }
    }
    #[test]
    fn equal_cost_obstacle_detours_do_not_oscillate_but_obstructions_and_excess_do_release() {
        let d = crate::compile(
            "a @pin(grid: 0,0)\nb @pin(grid: 20,0)\nblock @pin(grid: 10,0)\na.E -> b.W\n",
        )
        .unwrap();
        let e = &d.scene.edges[0];
        let mut prior = d.layout.clone();
        prior.routes.insert(e.id.clone(), path(&prior, e, -64.));
        assert!(route_is_valid(
            &d.scene,
            &prior,
            e,
            &prior.routes[&e.id],
            8.
        ));
        for i in 0..100 {
            let mut next = d.layout.clone();
            let delta = if i % 2 == 0 { 0.1 } else { -0.1 };
            next.nodes.get_mut("a").unwrap().y += delta;
            next.nodes.get_mut("a").unwrap().x += delta;
            next.nodes.get_mut("b").unwrap().x -= delta;
            let port = next.ports.get_mut(&port_key(e, true)).unwrap();
            port.point.y += delta;
            port.stub.y += delta;
            port.point.x += delta;
            port.stub.x += delta;
            let port = next.ports.get_mut(&port_key(e, false)).unwrap();
            port.point.x -= delta;
            port.stub.x -= delta;
            next.routes.insert(
                e.id.clone(),
                path(&next, e, if i % 2 == 0 { 64. } else { -64. }),
            );
            let kept = retain_detour(&d.scene, &next, e, &prior, 8.).unwrap();
            assert!(kept.points.iter().any(|p| p.y == -64.));
            assert!(!kept.points.iter().any(|p| p.y == 64.));
            assert!(
                coalesced(&kept.points).len() <= 6,
                "retention must not accumulate bends"
            );
            next.routes.insert(e.id.clone(), kept);
            prior = next;
        }
        let mut blocked = d.layout.clone();
        blocked.nodes.get_mut("block").unwrap().y = -64.;
        blocked.routes.insert(e.id.clone(), path(&blocked, e, 64.));
        assert!(retain_detour(&d.scene, &blocked, e, &prior, 8.).is_none());
        prior.routes.insert(e.id.clone(), path(&prior, e, -1000.));
        assert!(retain_detour(&d.scene, &d.layout, e, &prior, 8.).is_none());
    }
}
