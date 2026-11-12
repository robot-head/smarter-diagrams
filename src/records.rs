//! Fixed record slots retain their row order across multi-source bundles.
use crate::{layout::*, model::*, topology::SideHints};
use std::collections::{BTreeMap, BTreeSet};
struct Group {
    node: usize,
    low: f64,
    high: f64,
    parent: Option<usize>,
}
fn groups(scene: &SceneGraph) -> Option<(usize, Vec<Group>)> {
    if scene.nodes.len() < 3
        || scene
            .nodes
            .iter()
            .any(|n| n.parent.is_some() || n.container)
        || !scene.relations.is_empty()
    {
        return None;
    }
    let table = scene.nodes.iter().position(|n| n.shape == "sql_table")?;
    if scene.edges.iter().any(|e| {
        e.target.node != scene.nodes[table].id
            || e.source.node == e.target.node
            || !matches!(
                e.source.constraint,
                PortConstraint::Free | PortConstraint::FixedSide { side: Side::E }
            )
            || !matches!(
                e.target.constraint,
                PortConstraint::FixedPos { side: Side::W, .. }
            )
    }) {
        return None;
    }
    let mut out = Vec::new();
    let mut used = BTreeSet::new();
    for (node, n) in scene.nodes.iter().enumerate() {
        if node == table {
            continue;
        }
        let rows: Vec<_> = scene
            .edges
            .iter()
            .filter(|e| e.source.node == n.id)
            .filter_map(|e| {
                if let PortConstraint::FixedPos { offset_grid, .. } = e.target.constraint {
                    Some(offset_grid)
                } else {
                    None
                }
            })
            .collect();
        if rows.is_empty() {
            return None;
        }
        out.push(Group {
            node,
            low: rows.iter().copied().fold(f64::INFINITY, f64::min),
            high: rows.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            parent: None,
        });
    }
    for e in &scene.edges {
        if !used.insert(e.target.port.as_ref()?) {
            return None;
        }
    }
    for i in 0..out.len() {
        for j in 0..out.len() {
            if i == j {
                continue;
            }
            let (a, b) = (&out[i], &out[j]);
            if a.low < b.low && b.low < a.high && a.high < b.high {
                return None;
            }
        }
    }
    for i in 0..out.len() {
        out[i].parent = (0..out.len())
            .filter(|&j| j != i && out[j].low < out[i].low && out[j].high > out[i].high)
            .min_by(|&a, &b| (out[a].high - out[a].low).total_cmp(&(out[b].high - out[b].low)));
    }
    Some((table, out))
}
pub(crate) fn present(
    scene: &SceneGraph,
    rects: &mut [Rect],
    o: &LayoutOptions,
    place: bool,
) -> Option<SideHints> {
    let (table, groups) = groups(scene)?;
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
    for g in &groups {
        let degree = scene
            .edges
            .iter()
            .filter(|e| e.source.node == scene.nodes[g.node].id)
            .count();
        if degree > 1 {
            rects[g.node].h = rects[g.node]
                .h
                .max((degree + 1) as f64 * (ch + 12.).max(o.port_spacing));
        }
    }
    if place {
        rects[table].x = 0.;
        rects[table].y = 0.;
        let mut roots: Vec<_> = (0..groups.len())
            .filter(|&i| groups[i].parent.is_none())
            .collect();
        roots.sort_by(|&a, &b| groups[a].low.total_cmp(&groups[b].low));
        let mut cursor = 0.;
        for &root in &roots {
            let r = rects[groups[root].node];
            rects[groups[root].node].y = cursor + r.h / 2.;
            rects[groups[root].node].x = -r.w / 2.;
            cursor += r.h + ch + 32.;
        }
        let last = *roots.last()?;
        let shift = rects[table].top() + (groups[last].low + groups[last].high) * o.grid / 2.
            - rects[groups[last].node].y;
        for &root in &roots {
            rects[groups[root].node].y += shift;
        }
        for _ in 0..groups.len() {
            for g in &groups {
                if let Some(parent) = g.parent {
                    let p = &groups[parent];
                    rects[g.node].x = rects[p.node].right() + cw + 48. + rects[g.node].w / 2.;
                    rects[g.node].y =
                        rects[p.node].y + (g.low + g.high - p.low - p.high) * o.grid / 2.;
                }
            }
        }
        let right = groups
            .iter()
            .map(|g| rects[g.node].right())
            .fold(f64::NEG_INFINITY, f64::max);
        rects[table].x = right
            + cw
            + 2. * o.stub
            + scene.edges.len() as f64 * o.lane_spacing
            + 48.
            + rects[table].w / 2.;
    }
    if groups
        .iter()
        .any(|g| rects[g.node].right() + 2. * o.stub >= rects[table].left())
    {
        return None;
    }
    Some(vec![Some((Side::E, Side::W)); scene.edges.len()])
}
fn conflict(a: &Route, b: &Route) -> bool {
    a.points.windows(2).any(|p| {
        b.points.windows(2).any(|q| {
            crate::routing::proper_crossing(p[0], p[1], q[0], q[1])
                || crate::routing::collinear_overlap(p[0], p[1], q[0], q[1]) > 1e-6
        })
    })
}
pub(crate) fn coordinate(scene: &SceneGraph, o: &LayoutOptions, l: &mut Layout) -> bool {
    let Some((table, _)) = groups(scene) else {
        return false;
    };
    let mut rects: Vec<_> = scene.nodes.iter().map(|n| l.nodes[&n.id]).collect();
    if present(scene, &mut rects, o, false).is_none() {
        return false;
    }
    let gap = crate::labels::text_height("Ag") + o.margin + 8.;
    let mut candidates = Vec::new();
    let mut candidate_budget = o.shape_budget;
    for e in &scene.edges {
        let a = &l.ports[&port_key(e, true)];
        let b = &l.ports[&port_key(e, false)];
        if (a.side, b.side) != (Side::E, Side::W) {
            return false;
        }
        let mut ys = vec![a.stub.y, b.stub.y];
        for (id, r) in &l.nodes {
            if id != &e.source.node && id != &e.target.node {
                ys.extend([r.top() - gap, r.bottom() + gap]);
            }
        }
        let mut choices = Vec::new();
        let mut seen = BTreeSet::new();
        for y in ys {
            for rank in 0..scene.edges.len() + 2 {
                if candidate_budget == 0 {
                    return false;
                }
                candidate_budget -= 1;
                let x = l.nodes[&scene.nodes[table].id].left()
                    - o.stub
                    - o.margin
                    - (rank + 1) as f64 * o.lane_spacing.max(12.);
                if x < a.stub.x + o.stub {
                    continue;
                }
                let mut points = vec![
                    a.point,
                    a.stub,
                    Point::new(a.stub.x, y),
                    Point::new(x, y),
                    Point::new(x, b.stub.y),
                    b.stub,
                    b.point,
                ];
                points.dedup_by(|a, b| a.distance(*b) < 1e-7);
                let points = crate::routing::coalesced(&points);
                let key: Vec<_> = points
                    .iter()
                    .map(|p| ((p.x * 1000.).round() as i64, (p.y * 1000.).round() as i64))
                    .collect();
                if !seen.insert(key) {
                    continue;
                }
                let route = Route {
                    edge: e.id.clone(),
                    points,
                };
                if !crate::routing::route_is_valid(scene, l, e, &route, o.margin) {
                    continue;
                }
                let cost = route
                    .points
                    .windows(2)
                    .map(|p| p[0].distance(p[1]))
                    .sum::<f64>()
                    + route.points.len() as f64 * 16.;
                choices.push((cost, route));
            }
        }
        choices.sort_by(|a, b| a.0.total_cmp(&b.0));
        if choices.is_empty() {
            return false;
        }
        candidates.push(choices);
    }
    fn search(
        choices: &[Vec<(f64, Route)>],
        picked: &mut BTreeMap<usize, usize>,
        budget: &mut usize,
    ) -> bool {
        if picked.len() == choices.len() {
            return true;
        }
        if *budget == 0 {
            return false;
        }
        *budget -= 1;
        let mut next = None;
        let mut feasible = Vec::new();
        for (i, options) in choices.iter().enumerate() {
            if picked.contains_key(&i) {
                continue;
            }
            let compatible: Vec<_> = options
                .iter()
                .enumerate()
                .filter(|(_, (_, r))| picked.iter().all(|(&j, &k)| !conflict(r, &choices[j][k].1)))
                .map(|(k, _)| k)
                .collect();
            if compatible.is_empty() {
                return false;
            }
            if next.is_none() || compatible.len() < feasible.len() {
                next = Some(i);
                feasible = compatible;
            }
        }
        let i = next.unwrap();
        for k in feasible {
            picked.insert(i, k);
            if search(choices, picked, budget) {
                return true;
            }
            picked.remove(&i);
        }
        false
    }
    let mut picked = BTreeMap::new();
    if !search(
        &candidates,
        &mut picked,
        &mut o.shape_budget.saturating_mul(50),
    ) {
        return false;
    }
    for (i, k) in picked {
        l.routes
            .insert(scene.edges[i].id.clone(), candidates[i][k].1.clone());
    }
    true
}
pub(crate) fn caption_anchor(scene: &SceneGraph, l: &Layout, e: &Edge, w: f64) -> Option<Point> {
    groups(scene)?;
    let body = l.nodes[&e.source.node];
    let r = &l.routes[&e.id];
    r.points
        .windows(2)
        .filter(|p| {
            (p[0].y - p[1].y).abs() < 1e-7 && p[0].x.max(p[1].x) - p[0].x.min(p[1].x) >= w + 24.
        })
        .min_by(|a, b| ((a[0].y - body.y).abs()).total_cmp(&(b[0].y - body.y).abs()))
        .map(|p| {
            Point::new(
                (body.right() + w / 2. + 16.).clamp(
                    p[0].x.min(p[1].x) + w / 2. + 8.,
                    p[0].x.max(p[1].x) - w / 2. - 8.,
                ),
                p[0].y,
            )
        })
}
