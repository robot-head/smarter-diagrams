//! Preserve a declared main progression and separate its bypass/return channels.
use crate::{layout::*, model::*, topology::SideHints};
use std::collections::BTreeMap;
fn endpoint(r: Rect, side: Side) -> Point {
    match side {
        Side::N => Point::new(r.x, r.top()),
        Side::E => Point::new(r.right(), r.y),
        Side::S => Point::new(r.x, r.bottom()),
        Side::W => Point::new(r.left(), r.y),
    }
}
fn exposed(boxes: &[Rect], i: usize, side: Side, o: &LayoutOptions) -> bool {
    let p = endpoint(boxes[i], side);
    let (n, m) = side.normal();
    let end = p.offset(n * 1e7, m * 1e7);
    !boxes
        .iter()
        .enumerate()
        .any(|(j, r)| i != j && r.inflate(o.margin).blocks(p, end))
}
// A bypass between adjacent rows/columns need not travel around the entire
// diagram. Preserve a clear facing channel before considering outside rails.
fn nearby_channel(
    boxes: &[Rect],
    a: usize,
    b: usize,
    caption: f64,
    o: &LayoutOptions,
) -> Option<(Side, Side)> {
    let x = boxes[a];
    let y = boxes[b];
    let sides = if (x.x - y.x).abs() < 1e-6 {
        if x.bottom() + 2. * o.stub < y.top() {
            (Side::S, Side::N)
        } else if y.bottom() + 2. * o.stub < x.top() {
            (Side::N, Side::S)
        } else {
            return None;
        }
    } else if (x.y - y.y).abs() < 1e-6 {
        if x.right() + 2. * o.stub < y.left() {
            (Side::E, Side::W)
        } else if y.right() + 2. * o.stub < x.left() {
            (Side::W, Side::E)
        } else {
            return None;
        }
    } else {
        return None;
    };
    let p = endpoint(x, sides.0);
    let q = endpoint(y, sides.1);
    // Keep enough local room for the caption as well as the two endpoint
    // stubs; a longer caption belongs on a dedicated outside rail.
    if p.distance(q) < caption + 2. * o.stub {
        return None;
    }
    (!boxes
        .iter()
        .enumerate()
        .any(|(i, r)| i != a && i != b && r.inflate(o.margin).blocks(p, q)))
    .then_some(sides)
}
fn ring(a: Point, b: Point, sa: Side, sb: Side, bounds: Rect, clockwise: bool) -> Vec<Point> {
    if !clockwise {
        let mut p = ring(b, a, sb, sa, bounds, true);
        p.reverse();
        return p;
    }
    let w = bounds.w;
    let h = bounds.h;
    let perimeter = 2. * (w + h);
    let at = |p: Point, s| match s {
        Side::N => p.x - bounds.left(),
        Side::E => w + p.y - bounds.top(),
        Side::S => w + h + bounds.right() - p.x,
        Side::W => 2. * w + h + bounds.bottom() - p.y,
    };
    let start = at(a, sa);
    let mut end = at(b, sb);
    if end < start {
        end += perimeter;
    }
    let corners = [
        (w, Point::new(bounds.right(), bounds.top())),
        (w + h, Point::new(bounds.right(), bounds.bottom())),
        (2. * w + h, Point::new(bounds.left(), bounds.bottom())),
        (perimeter, Point::new(bounds.left(), bounds.top())),
    ];
    let mut p = vec![a];
    for lap in [0., perimeter] {
        for &(distance, corner) in &corners {
            let t = distance + lap;
            if t > start + 1e-6 && t < end - 1e-6 {
                p.push(corner);
            }
        }
    }
    p.push(b);
    p
}
fn outside_path(
    a: Point,
    b: Point,
    sa: Side,
    sb: Side,
    bounds: Rect,
    clearance: f64,
) -> Vec<Point> {
    let frame = bounds.inflate(clearance);
    let out = |p: Point, s| match s {
        Side::N => Point::new(p.x, frame.top()),
        Side::E => Point::new(frame.right(), p.y),
        Side::S => Point::new(p.x, frame.bottom()),
        Side::W => Point::new(frame.left(), p.y),
    };
    let x = out(a, sa);
    let y = out(b, sb);
    let cw = ring(x, y, sa, sb, frame, true);
    let ccw = ring(x, y, sa, sb, frame, false);
    let length = |p: &[Point]| p.windows(2).map(|p| p[0].distance(p[1])).sum::<f64>();
    let mut points = vec![a];
    points.extend(if length(&cw) <= length(&ccw) { cw } else { ccw });
    points.push(b);
    points.dedup_by(|a, b| a.distance(*b) < 1e-7);
    points
}
pub(crate) fn hints(scene: &SceneGraph, boxes: &[Rect], o: &LayoutOptions) -> Option<SideHints> {
    let order = crate::presentation::backbone(scene)?;
    let mut rank = vec![0; boxes.len()];
    for (step, &i) in order.iter().enumerate() {
        rank[i] = step;
    }
    let bounds = boxes.iter().copied().reduce(Rect::union)?;
    let mut hints = Vec::new();
    for e in &scene.edges {
        let a = scene.index(&e.source.node)?;
        let b = scene.index(&e.target.node)?;
        if rank[b] == rank[a] + 1 {
            let dx = boxes[b].x - boxes[a].x;
            let dy = boxes[b].y - boxes[a].y;
            hints.push(Some(if dx.abs() > dy.abs() {
                if dx > 0. {
                    (Side::E, Side::W)
                } else {
                    (Side::W, Side::E)
                }
            } else if dy > 0. {
                (Side::S, Side::N)
            } else {
                (Side::N, Side::S)
            }));
            continue;
        }
        if a == b {
            hints.push(None);
            continue;
        }
        if let Some(pair) = nearby_channel(
            boxes,
            a,
            b,
            crate::labels::text_width(&crate::labels::connection_text(scene, e)),
            o,
        ) {
            hints.push(Some(pair));
            continue;
        }
        let mut best = None;
        for sa in [Side::N, Side::E, Side::S, Side::W] {
            if !exposed(boxes, a, sa, o) {
                continue;
            }
            for sb in [Side::N, Side::E, Side::S, Side::W] {
                if !exposed(boxes, b, sb, o) {
                    continue;
                }
                let pa = endpoint(boxes[a], sa);
                let pb = endpoint(boxes[b], sb);
                let (na, ma) = sa.normal();
                let (nb, mb) = sb.normal();
                let p = outside_path(
                    pa.offset(na * o.stub, ma * o.stub),
                    pb.offset(nb * o.stub, mb * o.stub),
                    sa,
                    sb,
                    bounds,
                    o.stub + o.margin,
                );
                let cost =
                    p.windows(2).map(|p| p[0].distance(p[1])).sum::<f64>() + p.len() as f64 * 8.;
                if best.is_none_or(|(_, _, score)| cost < score) {
                    best = Some((sa, sb, cost));
                }
            }
        }
        hints.push(best.map(|(a, b, _)| (a, b)));
    }
    Some(hints)
}
pub(crate) fn coordinate(scene: &SceneGraph, o: &LayoutOptions, l: &mut Layout) -> bool {
    let boxes: Vec<_> = scene.nodes.iter().map(|n| l.nodes[&n.id]).collect();
    let Some(hints) = hints(scene, &boxes, o) else {
        return false;
    };
    let order = crate::presentation::backbone(scene).unwrap();
    let mut rank = vec![0; boxes.len()];
    for (step, &i) in order.iter().enumerate() {
        rank[i] = step;
    }
    let bounds = boxes.iter().copied().reduce(Rect::union).unwrap();
    let mut routes = BTreeMap::new();
    let mut outside = BTreeMap::<(String, String), Vec<(String, f64)>>::new();
    let mut lane = 0;
    for (index, e) in scene.edges.iter().enumerate() {
        let Some((sa, sb)) = hints[index] else {
            continue;
        };
        let a = &l.ports[&port_key(e, true)];
        let b = &l.ports[&port_key(e, false)];
        if (a.side, b.side) != (sa, sb) {
            return false;
        }
        let mut p = vec![a.point, a.stub];
        if rank[scene.index(&e.target.node).unwrap()]
            == rank[scene.index(&e.source.node).unwrap()] + 1
            || nearby_channel(
                &boxes,
                scene.index(&e.source.node).unwrap(),
                scene.index(&e.target.node).unwrap(),
                crate::labels::text_width(&crate::labels::connection_text(scene, e)),
                o,
            ) == Some((sa, sb))
        {
            if matches!(sa, Side::E | Side::W) {
                let x = (a.stub.x + b.stub.x) / 2.;
                p.extend([Point::new(x, a.stub.y), Point::new(x, b.stub.y)]);
            } else {
                let y = (a.stub.y + b.stub.y) / 2.;
                p.extend([Point::new(a.stub.x, y), Point::new(b.stub.x, y)]);
            }
        } else {
            let clearance = o.stub + o.margin + lane as f64 * o.lane_spacing.max(12.);
            p.extend(outside_path(a.stub, b.stub, sa, sb, bounds, clearance));
            outside
                .entry((e.source.node.clone(), e.target.node.clone()))
                .or_default()
                .push((e.id.clone(), clearance));
            lane += 1;
        }
        p.extend([b.stub, b.point]);
        p.dedup_by(|a, b| a.distance(*b) < 1e-7);
        let r = Route {
            edge: e.id.clone(),
            points: p,
        };
        if !crate::routing::route_is_valid(scene, l, e, &r, o.margin) {
            return false;
        }
        routes.insert(e.id.clone(), r);
    }
    // Parallel outside connectors must nest in the same order at both ends.
    // Declaration order alone can cross their endpoint legs twice. Compare
    // both rail orders against the complete proposal before choosing one.
    let conflicts = |routes: &BTreeMap<String, Route>| {
        let all: Vec<_> = scene
            .edges
            .iter()
            .map(|e| routes.get(&e.id).unwrap_or(&l.routes[&e.id]))
            .collect();
        let mut score = (0usize, 0usize);
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                for p in a.points.windows(2) {
                    for q in b.points.windows(2) {
                        score.0 += usize::from(
                            crate::routing::collinear_overlap(p[0], p[1], q[0], q[1]) > 1e-6,
                        );
                        score.1 +=
                            usize::from(crate::routing::proper_crossing(p[0], p[1], q[0], q[1]));
                    }
                }
            }
        }
        score
    };
    for group in outside.values().filter(|g| g.len() > 1) {
        let mut candidate = routes.clone();
        let mut valid = true;
        for ((id, _), (_, clearance)) in group.iter().zip(group.iter().rev()) {
            let e = scene.edge(id).unwrap();
            let a = &l.ports[&port_key(e, true)];
            let b = &l.ports[&port_key(e, false)];
            let mut points = vec![a.point, a.stub];
            points.extend(outside_path(
                a.stub, b.stub, a.side, b.side, bounds, *clearance,
            ));
            points.extend([b.stub, b.point]);
            points.dedup_by(|a, b| a.distance(*b) < 1e-7);
            let route = Route {
                edge: id.clone(),
                points,
            };
            valid &= crate::routing::route_is_valid(scene, l, e, &route, o.margin);
            candidate.insert(id.clone(), route);
        }
        if valid && conflicts(&candidate) < conflicts(&routes) {
            routes = candidate;
        }
    }
    let all: Vec<_> = scene
        .edges
        .iter()
        .map(|e| routes.get(&e.id).unwrap_or(&l.routes[&e.id]))
        .collect();
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            if a.points.windows(2).any(|p| {
                b.points
                    .windows(2)
                    .any(|q| crate::routing::collinear_overlap(p[0], p[1], q[0], q[1]) > 1e-6)
            }) {
                return false;
            }
        }
    }
    l.routes.extend(routes);
    true
}
/// Parallel and antiparallel links sharing attachment sides use consistent
/// non-crossing port order at both ends, including U-turn bundles.
pub(crate) fn parallel_order(
    scene: &SceneGraph,
    e: &Edge,
    source: bool,
    sa: Side,
    sb: Side,
    endpoint_sides: &BTreeMap<String, Side>,
    base: f64,
) -> Option<f64> {
    if e.source.node == e.target.node {
        return None;
    }
    let u_id = e.source.node.as_str().min(e.target.node.as_str());
    let v_id = e.source.node.as_str().max(e.target.node.as_str());
    let (su, sv) = if e.source.node == u_id {
        (sa, sb)
    } else {
        (sb, sa)
    };
    let pairs: Vec<_> = scene
        .edges
        .iter()
        .filter(|other| {
            let ou = other.source.node.as_str().min(other.target.node.as_str());
            let ov = other.source.node.as_str().max(other.target.node.as_str());
            if (ou, ov) != (u_id, v_id) {
                return false;
            }
            let osa = endpoint_sides.get(&port_key(other, true)).copied();
            let osb = endpoint_sides.get(&port_key(other, false)).copied();
            let (osu, osv) = if other.source.node == u_id {
                (osa, osb)
            } else {
                (osb, osa)
            };
            osu == Some(su) && osv == Some(sv)
        })
        .collect();
    if pairs.len() < 2 {
        return None;
    }
    let ep = if source { &e.source } else { &e.target };
    if ep.port.is_some()
        || !matches!(
            ep.constraint,
            PortConstraint::Free | PortConstraint::FixedSide { .. }
        )
    {
        return None;
    }
    let index = pairs.iter().position(|other| other.id == e.id)?;
    let on_u = ep.node == u_id;
    let same_clockwise = matches!(su, Side::N | Side::E) == matches!(sv, Side::N | Side::E);
    Some(base + index as f64 * 1e-4 * if !on_u && same_clockwise { -1. } else { 1. })
}
