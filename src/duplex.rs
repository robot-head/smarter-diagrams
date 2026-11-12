//! Ordered two-sided protocol fans preserve explicit hub ports.
use crate::{layout::*, model::*, topology::SideHints};
use std::collections::BTreeMap;
fn hub(scene: &SceneGraph) -> Option<(usize, Vec<usize>)> {
    if scene.nodes.len() < 4 {
        return None;
    }
    let center = scene
        .nodes
        .iter()
        .enumerate()
        .find(|(_, n)| !n.ports.is_empty())?
        .0;
    if scene
        .nodes
        .iter()
        .enumerate()
        .any(|(i, n)| n.container || n.parent.is_some() || (i != center && !n.ports.is_empty()))
    {
        return None;
    }
    if !scene.relations.is_empty() {
        return None;
    }
    let mut leaves = Vec::new();
    for i in 0..scene.nodes.len() {
        if i == center {
            continue;
        }
        let id = &scene.nodes[i].id;
        let name = &scene.nodes[center].id;
        let outgoing: Vec<_> = scene
            .edges
            .iter()
            .filter(|e| e.source.node == *id)
            .collect();
        let incoming: Vec<_> = scene
            .edges
            .iter()
            .filter(|e| e.target.node == *id)
            .collect();
        if outgoing.len() != 1
            || incoming.len() != 1
            || outgoing[0].target.node != *name
            || incoming[0].source.node != *name
            || outgoing[0].target.constraint.side() != Some(Side::W)
            || incoming[0].source.constraint.side() != Some(Side::E)
            || !matches!(outgoing[0].source.constraint, PortConstraint::Free)
            || !matches!(incoming[0].target.constraint, PortConstraint::Free)
        {
            return None;
        }
        leaves.push(i);
    }
    if scene.edges.len() != 2 * leaves.len() {
        return None;
    }
    let ratio = |i: usize| {
        scene
            .edges
            .iter()
            .find(|e| e.source.node == scene.nodes[i].id)
            .and_then(|e| match e.target.constraint {
                PortConstraint::FixedRatio { ratio, .. } => Some(ratio),
                PortConstraint::FixedPos { offset_grid, .. } => Some(offset_grid),
                _ => None,
            })
            .unwrap_or(i as f64)
    };
    leaves.sort_by(|&a, &b| ratio(a).total_cmp(&ratio(b)));
    Some((center, leaves))
}
pub(crate) fn present(
    scene: &SceneGraph,
    rects: &mut [Rect],
    o: &LayoutOptions,
    place: bool,
) -> Option<SideHints> {
    let (center, leaves) = hub(scene)?;
    if scene.nodes.iter().any(|n| {
        effective_annotations(scene, &n.id, &n.annotations, o.variant.as_deref())
            .iter()
            .any(|a| a.name == "pin" && (place || a.attrs.contains_key("lock")))
    }) || scene.edges.iter().any(|e| {
        effective_annotations(scene, &e.id, &e.annotations, o.variant.as_deref())
            .iter()
            .any(|a| a.name == "route")
    }) {
        return None;
    }
    let mut required = rects[center].h;
    for side in [Side::W, Side::E] {
        let mut ports: Vec<_> = scene.nodes[center]
            .ports
            .values()
            .filter_map(|p| match p.constraint {
                PortConstraint::FixedRatio { side: s, ratio } if s == side => {
                    Some((ratio, crate::labels::text_height(&p.label) + 8.))
                }
                _ => None,
            })
            .collect();
        ports.sort_by(|a, b| a.0.total_cmp(&b.0));
        for pair in ports.windows(2) {
            required = required.max(pair[0].1.max(pair[1].1) / (pair[1].0 - pair[0].0));
        }
    }
    rects[center].h = required;
    if place {
        let ch = scene
            .edges
            .iter()
            .map(|e| crate::labels::text_height(&crate::labels::connection_text(scene, e)))
            .fold(0., f64::max);
        let h = leaves.iter().map(|&i| rects[i].h).fold(0., f64::max);
        let step = h + ch + 24.;
        for (rank, &i) in leaves.iter().enumerate() {
            rects[i].x = 0.;
            rects[i].y = (leaves.len() - 1 - rank) as f64 * step;
        }
        rects[center].x = 0.;
        rects[center].y = rects[leaves[0]].bottom() + required / 2. + ch + 2. * o.stub;
    }
    if leaves
        .iter()
        .any(|&i| rects[i].bottom() + 2. * o.stub >= rects[center].top())
    {
        return None;
    }
    Some(
        scene
            .edges
            .iter()
            .map(|e| {
                Some(if e.target.node == scene.nodes[center].id {
                    (Side::W, Side::W)
                } else {
                    (Side::E, Side::E)
                })
            })
            .collect(),
    )
}
pub(crate) fn coordinate(scene: &SceneGraph, o: &LayoutOptions, l: &mut Layout) -> bool {
    let Some((center, leaves)) = hub(scene) else {
        return false;
    };
    let mut boxes: Vec<_> = scene.nodes.iter().map(|n| l.nodes[&n.id]).collect();
    let Some(hints) = present(scene, &mut boxes, o, false) else {
        return false;
    };
    let cw = scene
        .edges
        .iter()
        .map(|e| crate::labels::text_width(&crate::labels::connection_text(scene, e)))
        .fold(0., f64::max);
    let left = boxes.iter().map(|r| r.left()).fold(f64::INFINITY, f64::min) - cw - 32.;
    let right = boxes
        .iter()
        .map(|r| r.right())
        .fold(f64::NEG_INFINITY, f64::max)
        + cw
        + 32.;
    let mut routes = BTreeMap::new();
    for (index, e) in scene.edges.iter().enumerate() {
        let incoming = e.target.node == scene.nodes[center].id;
        let leaf = if incoming {
            &e.source.node
        } else {
            &e.target.node
        };
        let rank = leaves
            .iter()
            .position(|&i| scene.nodes[i].id == *leaf)
            .unwrap();
        let lane = if incoming {
            left - rank as f64 * o.lane_spacing.max(16.)
        } else {
            right + rank as f64 * o.lane_spacing.max(16.)
        };
        let a = &l.ports[&port_key(e, true)];
        let b = &l.ports[&port_key(e, false)];
        if hints[index] != Some((a.side, b.side)) {
            return false;
        }
        let mut points = vec![
            a.point,
            a.stub,
            Point::new(lane, a.stub.y),
            Point::new(lane, b.stub.y),
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
    let (center, _) = hub(scene)?;
    let incoming = edge.target.node == scene.nodes[center].id;
    let leaf = if incoming {
        &edge.source.node
    } else {
        &edge.target.node
    };
    let body = l.nodes[leaf];
    for p in l.routes[&edge.id].points.windows(2) {
        if (p[0].y - body.y).abs() > 0.01 || (p[1].y - body.y).abs() > 0.01 {
            continue;
        }
        let left = p[0].x.min(p[1].x) + w / 2. + 8.;
        let right = p[0].x.max(p[1].x) - w / 2. - 8.;
        if left > right {
            continue;
        }
        return Some(Point::new(
            (if incoming {
                body.left() - w / 2. - 16.
            } else {
                body.right() + w / 2. + 16.
            })
            .clamp(left, right),
            body.y,
        ));
    }
    None
}
