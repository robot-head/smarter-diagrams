//! Joint attachment and channel planning for bipartite dependencies.
use crate::{layout::*, model::*, topology::SideHints};
use std::collections::{BTreeMap, BTreeSet};
fn layers(scene: &SceneGraph) -> Option<(Vec<usize>, Vec<usize>)> {
    if scene.nodes.len() < 6 {
        return None;
    }
    let mut incoming = vec![0; scene.nodes.len()];
    let mut outgoing = incoming.clone();
    let mut pairs = BTreeSet::new();
    for e in &scene.edges {
        let a = scene.index(&e.source.node)?;
        let b = scene.index(&e.target.node)?;
        incoming[b] += 1;
        outgoing[a] += 1;
        if !pairs.insert((a, b)) {
            return None;
        }
    }
    let a: Vec<_> = (0..scene.nodes.len())
        .filter(|&i| incoming[i] == 0 && outgoing[i] > 0)
        .collect();
    let b: Vec<_> = (0..scene.nodes.len())
        .filter(|&i| outgoing[i] == 0 && incoming[i] > 0)
        .collect();
    if a.len() < 2 || b.len() < 2 || a.len() + b.len() != scene.nodes.len() {
        return None;
    }
    // Retain the established one-crossing perimeter presentation for K3,3.
    if a.len() == 3 && b.len() == 3 && scene.edges.len() == 9 {
        return None;
    }
    Some((a, b))
}
pub(crate) fn present(
    scene: &SceneGraph,
    rects: &mut [Rect],
    o: &LayoutOptions,
    place: bool,
) -> Option<SideHints> {
    let (a, b) = layers(scene)?;
    crate::presentation::reserve_perimeters(scene, rects, o);
    if place {
        let cw = scene
            .edges
            .iter()
            .map(|e| crate::labels::text_width(&crate::labels::connection_text(scene, e)))
            .fold(0., f64::max);
        let ch = scene
            .edges
            .iter()
            .map(|e| crate::labels::text_height(&crate::labels::connection_text(scene, e)))
            .fold(0., f64::max);
        let aw = a.iter().map(|&i| rects[i].w).fold(0., f64::max);
        let ah = a.iter().map(|&i| rects[i].h).fold(0., f64::max);
        let bw = b.iter().map(|&i| rects[i].w).fold(0., f64::max);
        let bh = b.iter().map(|&i| rects[i].h).fold(0., f64::max);
        let x0 = (aw + bw) / 2. + cw + 2. * o.stub + 24.;
        let y0 = (ah + bh) / 2. + ch + 2. * o.stub + 24.;
        let xgap = (aw + 2. * o.margin).max(cw + aw / 2. + o.stub + 8.);
        let ygap = (bh + 2. * o.margin).max(ch + bh / 2. + o.stub + 8.);
        let left = a.len() / 2;
        let top = b.len() / 2;
        for (index, &i) in a.iter().enumerate() {
            rects[i].x = if index < left {
                -x0 - (left - 1 - index) as f64 * xgap
            } else {
                x0 + (index - left) as f64 * xgap
            };
            rects[i].y = 0.;
        }
        for (index, &i) in b.iter().enumerate() {
            rects[i].x = 0.;
            rects[i].y = if index < top {
                -y0 - (top - 1 - index) as f64 * ygap
            } else {
                y0 + (index - top) as f64 * ygap
            };
        }
    }
    let mut hints = Vec::new();
    for e in &scene.edges {
        let p = rects[scene.index(&e.source.node)?];
        let q = rects[scene.index(&e.target.node)?];
        if (p.x - q.x).abs() < (p.w + q.w) / 2. + 2. * o.stub
            || (p.y - q.y).abs() < (p.h + q.h) / 2. + 2. * o.stub
        {
            return None;
        }
        hints.push(Some((
            if q.y < p.y { Side::N } else { Side::S },
            if p.x < q.x { Side::W } else { Side::E },
        )));
    }
    Some(hints)
}
pub(crate) fn order(
    scene: &SceneGraph,
    rects: &[Rect],
    edge: &Edge,
    source: bool,
    side: Side,
) -> Option<f64> {
    layers(scene)?;
    let a = rects[scene.index(&edge.source.node)?];
    let b = rects[scene.index(&edge.target.node)?];
    if source && matches!(side, Side::N | Side::S) {
        Some((b.y - a.y).abs() * if a.x < b.x { -1. } else { 1. })
    } else if !source && matches!(side, Side::W | Side::E) {
        Some((a.x - b.x).abs() * if b.y < a.y { -1. } else { 1. })
    } else {
        None
    }
}
pub(crate) fn coordinate(scene: &SceneGraph, o: &LayoutOptions, l: &mut Layout) -> bool {
    let mut boxes: Vec<_> = scene.nodes.iter().map(|n| l.nodes[&n.id]).collect();
    let Some(hints) = present(scene, &mut boxes, o, false) else {
        return false;
    };
    let mut routes = BTreeMap::new();
    for (index, e) in scene.edges.iter().enumerate() {
        let a = &l.ports[&port_key(e, true)];
        let b = &l.ports[&port_key(e, false)];
        if hints[index] != Some((a.side, b.side)) {
            return false;
        }
        let mut points = vec![
            a.point,
            a.stub,
            Point::new(a.stub.x, b.stub.y),
            b.stub,
            b.point,
        ];
        points.dedup_by(|a, b| a.distance(*b) < 1e-7);
        let r = Route {
            edge: e.id.clone(),
            points,
        };
        if !crate::routing::route_is_valid(scene, l, e, &r, o.margin) {
            return false;
        }
        routes.insert(e.id.clone(), r);
    }
    l.routes.extend(routes);
    true
}
pub(crate) fn caption_anchor(scene: &SceneGraph, l: &Layout, edge: &Edge, w: f64) -> Option<Point> {
    layers(scene)?;
    let a = &l.ports[&port_key(edge, true)];
    let b = &l.ports[&port_key(edge, false)];
    if !matches!(a.side, Side::N | Side::S) || !matches!(b.side, Side::E | Side::W) {
        return None;
    }
    for p in l.routes[&edge.id].points.windows(2) {
        if (p[0].y - b.point.y).abs() > 0.01 || (p[1].y - b.point.y).abs() > 0.01 {
            continue;
        }
        let left = p[0].x.min(p[1].x) + w / 2. + 8.;
        let right = p[0].x.max(p[1].x) - w / 2. - 8.;
        if left > right {
            continue;
        }
        return Some(Point::new(
            (a.stub.x + (b.stub.x - a.stub.x).signum() * (w / 2. + 8.)).clamp(left, right),
            b.point.y,
        ));
    }
    None
}
