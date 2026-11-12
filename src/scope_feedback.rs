//! Peer ownership scopes keep their reciprocal feedback on distinct channels.
use crate::{layout::*, model::*, topology::SideHints};
use std::collections::BTreeMap;
fn pair(scene: &SceneGraph, boxes: &[Rect], o: &LayoutOptions) -> Option<(usize, usize)> {
    let cross: Vec<_> = scene
        .edges
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            let a = scene.node(&e.source.node).and_then(|n| n.parent.as_ref());
            let b = scene.node(&e.target.node).and_then(|n| n.parent.as_ref());
            a.is_some() && b.is_some() && a != b
        })
        .map(|(i, _)| i)
        .collect();
    if cross.len() != 2 {
        return None;
    }
    let parent = |id: &str| scene.node(id)?.parent.as_deref();
    let a = &scene.edges[cross[0]];
    let b = &scene.edges[cross[1]];
    if parent(&a.source.node) != parent(&b.target.node)
        || parent(&a.target.node) != parent(&b.source.node)
    {
        return None;
    }
    for e in [a, b] {
        if !matches!(e.source.constraint, PortConstraint::Free)
            || !matches!(e.target.constraint, PortConstraint::Free)
            || effective_annotations(scene, &e.id, &e.annotations, o.variant.as_deref())
                .iter()
                .any(|a| a.name == "route")
        {
            return None;
        }
    }
    let from = boxes[scene.index(parent(&a.source.node)?)?];
    let to = boxes[scene.index(parent(&a.target.node)?)?];
    let (inner, outer, top, bottom) = if from.y < to.y {
        (cross[0], cross[1], from, to)
    } else {
        (cross[1], cross[0], to, from)
    };
    (top.bottom() + 2. * o.stub + o.margin < bottom.top()).then_some((inner, outer))
}
pub(crate) fn hints(scene: &SceneGraph, boxes: &[Rect], o: &LayoutOptions) -> Option<SideHints> {
    let (inner, outer) = pair(scene, boxes, o)?;
    let mut hints = vec![None; scene.edges.len()];
    hints[inner] = Some((Side::S, Side::N));
    hints[outer] = Some((Side::E, Side::N));
    Some(hints)
}
pub(crate) fn coordinate(scene: &SceneGraph, o: &LayoutOptions, l: &mut Layout) -> bool {
    let boxes: Vec<_> = scene.nodes.iter().map(|n| l.nodes[&n.id]).collect();
    let Some((inner, outer)) = pair(scene, &boxes, o) else {
        return false;
    };
    let mut routes = BTreeMap::new();
    let top = boxes.iter().map(|r| r.top()).fold(f64::INFINITY, f64::min);
    let right = boxes
        .iter()
        .map(|r| r.right())
        .fold(f64::NEG_INFINITY, f64::max)
        + o.stub
        + o.margin
        + 32.;
    let outer_caption =
        crate::labels::text_height(&crate::labels::connection_text(scene, &scene.edges[outer]));
    for index in [inner, outer] {
        let e = &scene.edges[index];
        let a = &l.ports[&port_key(e, true)];
        let b = &l.ports[&port_key(e, false)];
        let expected = if index == inner {
            (Side::S, Side::N)
        } else {
            (Side::E, Side::N)
        };
        if (a.side, b.side) != expected {
            return false;
        }
        let mut points = vec![a.point, a.stub];
        if index == inner {
            let from = l.nodes[scene.node(&e.source.node).unwrap().parent.as_ref().unwrap()];
            let to = l.nodes[scene.node(&e.target.node).unwrap().parent.as_ref().unwrap()];
            let y = (from.bottom() + to.top()) / 2.;
            points.extend([Point::new(a.stub.x, y), Point::new(b.stub.x, y)]);
        } else {
            let y = top - outer_caption - 32.;
            points.extend([
                Point::new(right, a.stub.y),
                Point::new(right, y),
                Point::new(b.stub.x, y),
            ]);
        }
        points.extend([b.stub, b.point]);
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
    let values: Vec<_> = scene
        .edges
        .iter()
        .map(|e| routes.get(&e.id).unwrap_or(&l.routes[&e.id]))
        .collect();
    for (i, a) in values.iter().enumerate() {
        for b in &values[i + 1..] {
            for p in a.points.windows(2) {
                for q in b.points.windows(2) {
                    if crate::routing::proper_crossing(p[0], p[1], q[0], q[1])
                        || crate::routing::collinear_overlap(p[0], p[1], q[0], q[1]) > 1e-6
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
