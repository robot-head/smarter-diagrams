//! A dispatch-and-collect stage is one role-aligned parallel region.
use crate::{layout::*, model::*, topology::SideHints};
use std::collections::BTreeMap;
struct Fan {
    hub: usize,
    join: usize,
    leaves: Vec<usize>,
    prefix: Option<usize>,
}
fn fan(scene: &SceneGraph) -> Option<Fan> {
    for (hub, n) in scene.nodes.iter().enumerate() {
        let edges: Vec<_> = scene
            .edges
            .iter()
            .filter(|e| e.source.node == n.id)
            .collect();
        if edges.len() < 3 {
            continue;
        }
        let leaves: Vec<_> = edges
            .iter()
            .map(|e| scene.index(&e.target.node).unwrap())
            .collect();
        let mut join = None;
        let mut valid = true;
        for &i in &leaves {
            let outgoing: Vec<_> = scene
                .edges
                .iter()
                .filter(|e| e.source.node == scene.nodes[i].id)
                .collect();
            if outgoing.len() != 1
                || scene
                    .edges
                    .iter()
                    .filter(|e| e.target.node == scene.nodes[i].id)
                    .count()
                    != 1
            {
                valid = false;
                break;
            }
            let j = scene.index(&outgoing[0].target.node).unwrap();
            if join.is_some_and(|old| old != j) {
                valid = false;
                break;
            }
            join = Some(j);
        }
        if !valid {
            continue;
        }
        let join = join?;
        if join == hub || leaves.contains(&join) {
            continue;
        }
        let incoming: Vec<_> = scene
            .edges
            .iter()
            .filter(|e| e.target.node == n.id)
            .collect();
        let prefix = if incoming.is_empty() {
            None
        } else if incoming.len() == 1 {
            Some(scene.index(&incoming[0].source.node)?)
        } else {
            continue;
        };
        if scene.nodes.len() != leaves.len() + 2 + usize::from(prefix.is_some())
            || scene.edges.len() != 2 * leaves.len() + usize::from(prefix.is_some())
            || scene
                .edges
                .iter()
                .any(|e| e.source.node == scene.nodes[join].id)
        {
            continue;
        }
        return Some(Fan {
            hub,
            join,
            leaves,
            prefix,
        });
    }
    None
}
pub(crate) fn present(
    scene: &SceneGraph,
    rects: &mut [Rect],
    o: &LayoutOptions,
    place: bool,
) -> Option<SideHints> {
    let f = fan(scene)?;
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
        let w = f.leaves.iter().map(|&i| rects[i].w).fold(0., f64::max);
        let h = f.leaves.iter().map(|&i| rects[i].h).fold(0., f64::max);
        rects[f.hub].x = 0.;
        rects[f.hub].y = 0.;
        let x = rects[f.hub].right() + w / 2. + cw + 96.;
        for (rank, &i) in f.leaves.iter().enumerate() {
            rects[i].x = x;
            rects[i].y = (rank as f64 - (f.leaves.len() - 1) as f64 / 2.) * (h + ch + 40.);
        }
        rects[f.join].x = x + w / 2. + rects[f.join].w / 2. + cw + 64.;
        rects[f.join].y = 0.;
        if let Some(i) = f.prefix {
            rects[i].x = rects[f.hub].left() - rects[i].w / 2. - cw - 64.;
            rects[i].y = 0.;
        }
    }
    if f.leaves.iter().any(|&i| {
        rects[f.hub].right() + 2. * o.stub >= rects[i].left()
            || rects[i].right() + 2. * o.stub >= rects[f.join].left()
    }) {
        return None;
    }
    Some(vec![Some((Side::E, Side::W)); scene.edges.len()])
}
pub(crate) fn coordinate(scene: &SceneGraph, o: &LayoutOptions, l: &mut Layout) -> bool {
    let Some(f) = fan(scene) else { return false };
    let mut boxes: Vec<_> = scene.nodes.iter().map(|n| l.nodes[&n.id]).collect();
    if present(scene, &mut boxes, o, false).is_none() {
        return false;
    }
    let mut routes = BTreeMap::new();
    for outgoing in [true, false] {
        let hub = if outgoing { f.hub } else { f.join };
        let y = boxes[hub].y;
        for &i in &f.leaves {
            let e = scene
                .edges
                .iter()
                .find(|e| {
                    if outgoing {
                        e.source.node == scene.nodes[hub].id && e.target.node == scene.nodes[i].id
                    } else {
                        e.source.node == scene.nodes[i].id && e.target.node == scene.nodes[hub].id
                    }
                })
                .unwrap();
            let a = &l.ports[&port_key(e, true)];
            let b = &l.ports[&port_key(e, false)];
            if (a.side, b.side) != (Side::E, Side::W) {
                return false;
            }
            let dy = boxes[i].y - y;
            let rank = f
                .leaves
                .iter()
                .filter(|&&j| {
                    let d = boxes[j].y - y;
                    d.signum() == dy.signum() && d.abs() > dy.abs() + 1e-6
                })
                .count();
            let x = if outgoing {
                a.stub.x + o.lane_spacing * (rank + 1) as f64
            } else {
                b.stub.x - o.lane_spacing * (rank + 1) as f64
            };
            let mut p = vec![
                a.point,
                a.stub,
                Point::new(x, a.stub.y),
                Point::new(x, b.stub.y),
                b.stub,
                b.point,
            ];
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
    }
    l.routes.extend(routes);
    true
}
pub(crate) fn caption_anchor(scene: &SceneGraph, l: &Layout, e: &Edge, w: f64) -> Option<Point> {
    let f = fan(scene)?;
    let (outgoing, i) = if let Some(i) = f
        .leaves
        .iter()
        .copied()
        .find(|&i| e.target.node == scene.nodes[i].id)
    {
        (true, i)
    } else {
        let i = f
            .leaves
            .iter()
            .copied()
            .find(|&i| e.source.node == scene.nodes[i].id)?;
        (false, i)
    };
    let r = l.nodes[&scene.nodes[i].id];
    for p in l.routes[&e.id].points.windows(2) {
        if (p[0].y - r.y).abs() > 0.01 || (p[1].y - r.y).abs() > 0.01 {
            continue;
        }
        let left = p[0].x.min(p[1].x) + w / 2. + 8.;
        let right = p[0].x.max(p[1].x) - w / 2. - 8.;
        if left > right {
            continue;
        }
        return Some(Point::new(
            (if outgoing {
                r.left() - w / 2. - 16.
            } else {
                r.right() + w / 2. + 16.
            })
            .clamp(left, right),
            r.y,
        ));
    }
    None
}
