//! Fresh topology presentations and ordered routing for compound concepts.
use crate::{layout::*, model::*, topology::SideHints};
use std::collections::{BTreeMap, BTreeSet};

/// Keep the same room for free attachments before and after side selection.
/// This also prevents saved pins from changing body size when reopened.
pub(crate) fn reserve_perimeters(scene: &SceneGraph, rects: &mut [Rect], o: &LayoutOptions) {
    for (i, r) in rects.iter_mut().enumerate() {
        let id = &scene.nodes[i].id;
        let degree = scene
            .edges
            .iter()
            .filter(|e| &e.source.node == id)
            .count()
            .max(scene.edges.iter().filter(|e| &e.target.node == id).count());
        let extent = degree.saturating_sub(1) as f64 * o.port_spacing + 2. * o.corner;
        r.w = r.w.max(extent);
        r.h = r.h.max(extent);
    }
}

pub(crate) fn mesh_layers(scene: &SceneGraph) -> Option<(Vec<usize>, Vec<usize>)> {
    let n = scene.nodes.len();
    if n < 6 || scene.edges.len() < 9 {
        return None;
    }
    let mut incoming = vec![0; n];
    let mut outgoing = vec![0; n];
    let mut pairs = BTreeSet::new();
    for e in &scene.edges {
        let a = scene.index(&e.source.node)?;
        let b = scene.index(&e.target.node)?;
        incoming[b] += 1;
        outgoing[a] += 1;
        pairs.insert((a, b));
    }
    let sources: Vec<_> = (0..n)
        .filter(|&i| incoming[i] == 0 && outgoing[i] > 0)
        .collect();
    let sinks: Vec<_> = (0..n)
        .filter(|&i| outgoing[i] == 0 && incoming[i] > 0)
        .collect();
    if sources.len() + sinks.len() != n
        || sources.len() < 2
        || sinks.len() < 2
        || sources.len() * sinks.len() != scene.edges.len()
        || pairs.len() != scene.edges.len()
    {
        return None;
    }
    sources
        .iter()
        .all(|&a| sinks.iter().all(|&b| pairs.contains(&(a, b))))
        .then_some((sources, sinks))
}

pub(crate) fn mesh(
    scene: &SceneGraph,
    rects: &mut [Rect],
    o: &LayoutOptions,
    place: bool,
) -> Option<SideHints> {
    let (sources, sinks) = mesh_layers(scene)?;
    if sources.len() == 3 && sinks.len() == 3 {
        return mesh_ring(scene, rects, o, place, &sources, &sinks);
    }
    if place {
        let caption_width = scene
            .edges
            .iter()
            .map(|e| crate::labels::text_width(&crate::labels::connection_text(scene, e)))
            .fold(0., f64::max);
        let caption_height = scene
            .edges
            .iter()
            .map(|e| crate::labels::text_height(&crate::labels::connection_text(scene, e)))
            .fold(0., f64::max);
        let lane = (caption_height + 12.).max(o.port_spacing);
        for &i in &sources {
            rects[i].h = rects[i].h.max((sinks.len() + 1) as f64 * lane);
        }
        for &i in &sinks {
            rects[i].h = rects[i].h.max((sources.len() + 1) as f64 * lane);
        }
        let step = rects.iter().map(|r| r.h).fold(0., f64::max) + 48.;
        let gap = rects.iter().map(|r| r.w).fold(0., f64::max)
            + 2. * caption_width
            + scene.edges.len() as f64 * 24.
            + 128.;
        for (row, &i) in sources.iter().enumerate() {
            rects[i].x = 0.;
            rects[i].y = (row as f64 - (sources.len() - 1) as f64 / 2.) * step;
        }
        for (row, &i) in sinks.iter().enumerate() {
            rects[i].x = gap;
            rects[i].y = (row as f64 - (sinks.len() - 1) as f64 / 2.) * step;
        }
    }
    if sources.iter().any(|&a| {
        sinks
            .iter()
            .any(|&b| rects[a].right() + 2. * o.stub >= rects[b].left())
    }) {
        return None;
    }
    Some(vec![Some((Side::E, Side::W)); scene.edges.len()])
}

// K3,3 is nonplanar. Six rim connections and one outside connection leave
// only the unavoidable crossing between the two remaining interior chords.
fn mesh_ring(
    scene: &SceneGraph,
    rects: &mut [Rect],
    o: &LayoutOptions,
    place: bool,
    a: &[usize],
    b: &[usize],
) -> Option<SideHints> {
    // Preserve the perimeter reserve used by the initial three-connection
    // side estimate when reopening saved pins or beginning a drag.
    for r in rects.iter_mut() {
        r.h = r.h.max(2. * o.port_spacing + 2. * o.corner);
    }

    if place {
        let cap_w = scene
            .edges
            .iter()
            .map(|e| crate::labels::text_width(&crate::labels::connection_text(scene, e)))
            .fold(0., f64::max);
        let cap_h = scene
            .edges
            .iter()
            .map(|e| crate::labels::text_height(&crate::labels::connection_text(scene, e)))
            .fold(0., f64::max);
        let width = rects.iter().map(|r| r.w).fold(0., f64::max) + 2. * cap_w + 112.;
        let height = rects.iter().map(|r| r.h).fold(0., f64::max) + cap_h + 112.;
        for (i, x, y) in [
            (a[0], 0., 0.),
            (b[0], width, 0.),
            (a[1], width, height),
            (b[1], width, 2. * height),
            (a[2], 0., 2. * height),
            (b[2], 0., height),
        ] {
            rects[i].x = x;
            rects[i].y = y;
        }
    }
    if rects[a[0]].right() + 2. * o.stub >= rects[b[0]].left()
        || rects[a[0]].bottom() + 2. * o.stub >= rects[b[2]].top()
        || rects[b[2]].bottom() + 2. * o.stub >= rects[a[2]].top()
        || rects[b[0]].bottom() + 2. * o.stub >= rects[a[1]].top()
        || rects[a[1]].bottom() + 2. * o.stub >= rects[b[1]].top()
    {
        return None;
    }
    let mut hints = vec![None; scene.edges.len()];
    for (index, e) in scene.edges.iter().enumerate() {
        let i = a.iter().position(|&i| scene.nodes[i].id == e.source.node)?;
        let j = b.iter().position(|&i| scene.nodes[i].id == e.target.node)?;
        hints[index] = Some(match (i, j) {
            (0, 0) | (2, 1) => (Side::E, Side::W),
            (0, 2) => (Side::S, Side::N),
            (1, 0) | (2, 2) | (2, 0) => (Side::N, Side::S),
            (1, 1) => (Side::S, Side::N),
            (1, 2) => (Side::W, Side::E),
            (0, 1) => (Side::N, Side::S),
            _ => unreachable!(),
        });
    }
    Some(hints)
}

fn coordinate_mesh_ring(
    scene: &SceneGraph,
    o: &LayoutOptions,
    l: &mut Layout,
    a: &[usize],
    b: &[usize],
) -> bool {
    let mut rects: Vec<_> = scene.nodes.iter().map(|n| l.nodes[&n.id]).collect();
    let Some(hints) = mesh_ring(scene, &mut rects, o, false, a, b) else {
        return false;
    };
    if scene.edges.iter().enumerate().any(|(i, e)| {
        hints[i]
            != Some((
                l.ports[&port_key(e, true)].side,
                l.ports[&port_key(e, false)].side,
            ))
    }) {
        return false;
    }
    let top = rects.iter().map(|r| r.top()).fold(f64::INFINITY, f64::min) - 72.;
    let bottom = rects
        .iter()
        .map(|r| r.bottom())
        .fold(f64::NEG_INFINITY, f64::max)
        + 72.;
    let right = rects
        .iter()
        .map(|r| r.right())
        .fold(f64::NEG_INFINITY, f64::max)
        + 72.;
    let mid = (rects[a[0]].right() + rects[b[0]].left()) / 2.;
    let mut routes = BTreeMap::new();
    for e in &scene.edges {
        let i = a
            .iter()
            .position(|&i| scene.nodes[i].id == e.source.node)
            .unwrap();
        let j = b
            .iter()
            .position(|&i| scene.nodes[i].id == e.target.node)
            .unwrap();
        let p = &l.ports[&port_key(e, true)];
        let q = &l.ports[&port_key(e, false)];
        let mut points = vec![p.point, p.stub];
        if (i, j) == (0, 1) {
            points.extend([
                Point::new(p.stub.x, top),
                Point::new(right, top),
                Point::new(right, bottom),
                Point::new(q.stub.x, bottom),
            ])
        } else if (i, j) == (2, 0) {
            points.extend([
                Point::new(p.stub.x, (p.stub.y + rects[b[2]].bottom()) / 2.),
                Point::new(mid, (p.stub.y + rects[b[2]].bottom()) / 2.),
                Point::new(mid, (q.stub.y + rects[a[1]].top()) / 2.),
                Point::new(q.stub.x, (q.stub.y + rects[a[1]].top()) / 2.),
            ])
        } else if matches!(p.side, Side::N | Side::S) {
            let y = (p.stub.y + q.stub.y) / 2.;
            points.extend([Point::new(p.stub.x, y), Point::new(q.stub.x, y)])
        } else {
            let x = (p.stub.x + q.stub.x) / 2.;
            points.extend([Point::new(x, p.stub.y), Point::new(x, q.stub.y)])
        }
        points.extend([q.stub, q.point]);
        points.dedup_by(|a, b| a.distance(*b) < 1e-7);
        routes.insert(
            e.id.clone(),
            Route {
                edge: e.id.clone(),
                points,
            },
        );
    }
    commit(scene, o, l, routes)
}

fn tree_order(scene: &SceneGraph) -> Option<(usize, Vec<Vec<usize>>)> {
    let n = scene.nodes.len();
    if n < 4 || scene.edges.len() + 1 != n {
        return None;
    }
    let mut incoming = vec![0; n];
    let mut children = vec![Vec::new(); n];
    for e in &scene.edges {
        let a = scene.index(&e.source.node)?;
        let b = scene.index(&e.target.node)?;
        incoming[b] += 1;
        children[a].push(b);
    }
    if incoming.iter().any(|&i| i > 1) || !children.iter().any(|v| v.len() > 1) {
        return None;
    }
    let roots: Vec<_> = (0..n).filter(|&i| incoming[i] == 0).collect();
    if roots.len() != 1 {
        return None;
    }
    for v in &mut children {
        v.sort_unstable();
    }
    let mut stack = vec![roots[0]];
    let mut seen = BTreeSet::new();
    while let Some(i) = stack.pop() {
        if !seen.insert(i) {
            return None;
        }
        stack.extend(&children[i]);
    }
    (seen.len() == n).then_some((roots[0], children))
}

pub(crate) fn tree(
    scene: &SceneGraph,
    rects: &mut [Rect],
    o: &LayoutOptions,
    place: bool,
) -> Option<SideHints> {
    let (root, children) = tree_order(scene)?;
    reserve_perimeters(scene, rects, o);
    let mut depths = vec![0; rects.len()];
    let mut order = vec![root];
    let mut at = 0;
    while at < order.len() {
        let i = order[at];
        for &j in &children[i] {
            depths[j] = depths[i] + 1;
            order.push(j);
        }
        at += 1;
    }
    let cap_w = scene
        .edges
        .iter()
        .map(|e| crate::labels::text_width(&crate::labels::connection_text(scene, e)))
        .fold(0., f64::max)
        + 32.;
    let cap_h = scene
        .edges
        .iter()
        .map(|e| crate::labels::text_height(&crate::labels::connection_text(scene, e)))
        .fold(0., f64::max);
    let make = |vertical: bool| {
        let mut boxes = rects.to_vec();
        let mut spans = vec![0.; boxes.len()];
        let levels = depths.iter().copied().max().unwrap() + 1;
        let mut sizes = vec![0.0f64; levels];
        for &i in order.iter().rev() {
            let own = if vertical { boxes[i].w } else { boxes[i].h };
            spans[i] = own.max(
                children[i].iter().map(|&c| spans[c]).sum::<f64>()
                    + children[i].len().saturating_sub(1) as f64 * 40.,
            );
            sizes[depths[i]] = sizes[depths[i]].max(if vertical { boxes[i].h } else { boxes[i].w });
        }
        let gap = if vertical {
            cap_h + 2. * o.stub + 32.
        } else {
            cap_w + 2. * o.stub + 32.
        };
        let mut levels_at = vec![0.; levels];
        for d in 1..levels {
            levels_at[d] = levels_at[d - 1] + (sizes[d - 1] + sizes[d]) / 2. + gap;
        }
        let mut centers = vec![0.; boxes.len()];
        for &i in &order {
            let total = children[i].iter().map(|&c| spans[c]).sum::<f64>()
                + children[i].len().saturating_sub(1) as f64 * 40.;
            let mut start = centers[i] - total / 2.;
            for &c in &children[i] {
                centers[c] = start + spans[c] / 2.;
                start += spans[c] + 40.;
            }
        }
        for i in 0..boxes.len() {
            if vertical {
                boxes[i].x = centers[i];
                boxes[i].y = levels_at[depths[i]];
            } else {
                boxes[i].x = levels_at[depths[i]];
                boxes[i].y = centers[i];
            }
        }
        boxes
    };
    let vertical = if place {
        let h = make(false);
        let v = make(true);
        let score = |boxes: &[Rect]| {
            let b = boxes.iter().copied().reduce(Rect::union).unwrap();
            (450. * o.target_aspect_ratio / (b.w + 48.))
                .min(450. / (b.h + 48.))
                .min(1.)
        };
        let vertical = score(&v) > score(&h);
        rects.copy_from_slice(if vertical { &v } else { &h });
        vertical
    } else {
        let vertical = scene.edges.iter().all(|e| {
            let a = rects[scene.index(&e.source.node).unwrap()];
            let b = rects[scene.index(&e.target.node).unwrap()];
            a.bottom() + 2. * o.stub < b.top()
        });
        if !vertical
            && !scene.edges.iter().all(|e| {
                let a = rects[scene.index(&e.source.node).unwrap()];
                let b = rects[scene.index(&e.target.node).unwrap()];
                a.right() + 2. * o.stub < b.left()
            })
        {
            return None;
        }
        vertical
    };
    Some(vec![
        Some(if vertical {
            (Side::S, Side::N)
        } else {
            (Side::E, Side::W)
        });
        scene.edges.len()
    ])
}

/// A declared main sequence may have shortcuts, return edges and parallel sends.
pub(crate) fn backbone(scene: &SceneGraph) -> Option<Vec<usize>> {
    let n = scene.nodes.len();
    if n < 5 {
        return None;
    }
    let pairs: BTreeSet<_> = scene
        .edges
        .iter()
        .map(|e| {
            (
                scene.index(&e.source.node).unwrap(),
                scene.index(&e.target.node).unwrap(),
            )
        })
        .collect();
    // In a DAG the main sequence is topological, independent of the order in
    // which the author declares its nodes. Parallel sends count once here.
    let mut indegree = vec![0; n];
    for &(_, b) in &pairs {
        indegree[b] += 1;
    }
    let mut ready: BTreeSet<_> = (0..n).filter(|&i| indegree[i] == 0).collect();
    let mut order = Vec::new();
    while let Some(i) = ready.pop_first() {
        order.push(i);
        for &(a, b) in &pairs {
            if a == i {
                indegree[b] -= 1;
                if indegree[b] == 0 {
                    ready.insert(b);
                }
            }
        }
    }
    if order.len() == n && order.windows(2).all(|p| pairs.contains(&(p[0], p[1]))) {
        return Some(order);
    }
    // A cyclic main sequence has no topological order; retain the declared
    // progression and let the presentation score decide whether to fold it.
    if (0..n - 1).all(|i| pairs.contains(&(i, i + 1))) {
        Some((0..n).collect())
    } else {
        None
    }
}

fn shortcut_sides(scene: &SceneGraph, rects: &[Rect], order: &[usize]) -> Option<SideHints> {
    // Too many exterior rails enlarge the frame and make fitted text smaller.
    // Dense bypass sets retain the normal interior channel planner.
    if scene.edges.len().saturating_sub(scene.nodes.len() - 1) > 3 {
        return None;
    }

    let mut rank = vec![0; rects.len()];
    for (step, &i) in order.iter().enumerate() {
        rank[i] = step;
    }
    let left = rects.iter().map(|r| r.x).fold(f64::INFINITY, f64::min);
    let right = rects.iter().map(|r| r.x).fold(f64::NEG_INFINITY, f64::max);
    let mut hints = vec![None; scene.edges.len()];
    for (index, e) in scene.edges.iter().enumerate() {
        let a = scene.index(&e.source.node)?;
        let b = scene.index(&e.target.node)?;
        if rank[a] >= rank[b] {
            return None;
        }
        if rank[b] == rank[a] + 1 {
            continue;
        }
        let p = rects[a];
        let q = rects[b];
        // Preserve lateral choices through small drags. Intermediate columns
        // retain normal port selection because an outside exit may be blocked.
        let side = |r: Rect| {
            if (r.x - left).abs() < 32. {
                Some(Side::W)
            } else if (r.x - right).abs() < 32. {
                Some(Side::E)
            } else {
                None
            }
        };
        if let (Some(sa), Some(sb)) = (side(p), side(q)) {
            hints[index] = Some((sa, sb));
        }
    }
    hints.iter().any(Option::is_some).then_some(hints)
}

pub(crate) fn backbone_sides(scene: &SceneGraph, rects: &[Rect]) -> Option<SideHints> {
    shortcut_sides(scene, rects, &backbone(scene)?)
}

pub(crate) fn coordinate_backbone(scene: &SceneGraph, o: &LayoutOptions, l: &mut Layout) -> bool {
    let rects: Vec<_> = scene.nodes.iter().map(|n| l.nodes[&n.id]).collect();
    let Some(hints) = backbone_sides(scene, &rects) else {
        return false;
    };
    let bounds = rects.iter().copied().reduce(Rect::union).unwrap();
    let mut routes = BTreeMap::new();
    let mut lane = 0;
    for (index, e) in scene.edges.iter().enumerate() {
        let Some((sa, sb)) = hints[index] else {
            continue;
        };
        let a = &l.ports[&port_key(e, true)];
        let b = &l.ports[&port_key(e, false)];
        if a.side != sa || b.side != sb {
            return false;
        }
        let clearance = o.stub + o.margin + lane as f64 * o.lane_spacing.max(12.);
        lane += 1;
        let outside_x = |side| {
            if side == Side::W {
                bounds.left() - clearance
            } else {
                bounds.right() + clearance
            }
        };
        let x = outside_x(sa);
        let mut points = vec![a.point, a.stub, Point::new(x, a.stub.y)];
        if sa == sb {
            points.push(Point::new(x, b.stub.y));
        } else {
            let y = if (a.stub.y + b.stub.y) / 2. < bounds.y {
                bounds.top() - clearance
            } else {
                bounds.bottom() + clearance
            };
            let target_x = outside_x(sb);
            points.extend([
                Point::new(x, y),
                Point::new(target_x, y),
                Point::new(target_x, b.stub.y),
            ]);
        }
        points.extend([b.stub, b.point]);
        points.dedup_by(|a, b| a.distance(*b) < 1e-7);
        routes.insert(
            e.id.clone(),
            Route {
                edge: e.id.clone(),
                points,
            },
        );
    }
    commit(scene, o, l, routes)
}

fn commit(
    scene: &SceneGraph,
    o: &LayoutOptions,
    l: &mut Layout,
    routes: BTreeMap<String, Route>,
) -> bool {
    if routes.iter().any(|(id, r)| {
        !crate::routing::route_is_valid(scene, l, scene.edge(id).unwrap(), r, o.margin)
    }) {
        return false;
    }
    l.routes.extend(routes);
    true
}

pub(crate) fn coordinate_tree(scene: &SceneGraph, o: &LayoutOptions, l: &mut Layout) -> bool {
    let Some((_root, children)) = tree_order(scene) else {
        return false;
    };
    let vertical = scene.edges.iter().all(|e| {
        l.ports[&port_key(e, true)].side == Side::S && l.ports[&port_key(e, false)].side == Side::N
    });
    if !vertical
        && !scene.edges.iter().all(|e| {
            l.ports[&port_key(e, true)].side == Side::E
                && l.ports[&port_key(e, false)].side == Side::W
        })
    {
        return false;
    }
    let mut routes = BTreeMap::new();
    for e in &scene.edges {
        let a = &l.ports[&port_key(e, true)];
        let b = &l.ports[&port_key(e, false)];
        let parent = scene.index(&e.source.node).unwrap();
        let center = l.nodes[&e.source.node];
        let offset = if vertical {
            b.point.x - center.x
        } else {
            b.point.y - center.y
        };
        let rank = children[parent]
            .iter()
            .filter(|&&i| {
                let p = l.nodes[&scene.nodes[i].id];
                let v = if vertical {
                    p.x - center.x
                } else {
                    p.y - center.y
                };
                v.signum() == offset.signum() && v.abs() > offset.abs() + 1e-6
            })
            .count();
        let lane = if vertical {
            a.stub.y + o.lane_spacing * (rank + 1) as f64
        } else {
            a.stub.x + o.lane_spacing * (rank + 1) as f64
        };
        let mut points = if vertical {
            vec![
                a.point,
                a.stub,
                Point::new(a.stub.x, lane),
                Point::new(b.stub.x, lane),
                b.stub,
                b.point,
            ]
        } else {
            vec![
                a.point,
                a.stub,
                Point::new(lane, a.stub.y),
                Point::new(lane, b.stub.y),
                b.stub,
                b.point,
            ]
        };
        points.dedup_by(|a, b| a.distance(*b) < 1e-7);
        routes.insert(
            e.id.clone(),
            Route {
                edge: e.id.clone(),
                points,
            },
        );
    }
    let values: Vec<_> = routes.values().collect();
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
    commit(scene, o, l, routes)
}

pub(crate) fn coordinate_mesh(scene: &SceneGraph, o: &LayoutOptions, l: &mut Layout) -> bool {
    let Some((sources, sinks)) = mesh_layers(scene) else {
        return false;
    };
    if sources.len() == 3 && sinks.len() == 3 {
        return coordinate_mesh_ring(scene, o, l, &sources, &sinks);
    }
    if !scene.edges.iter().all(|e| {
        l.ports[&port_key(e, true)].side == Side::E && l.ports[&port_key(e, false)].side == Side::W
    }) {
        return false;
    }
    let caption = scene
        .edges
        .iter()
        .map(|e| crate::labels::text_width(&crate::labels::connection_text(scene, e)) + 32.)
        .fold(0., f64::max);
    let low = scene
        .edges
        .iter()
        .map(|e| l.ports[&port_key(e, true)].stub.x)
        .fold(f64::NEG_INFINITY, f64::max)
        + caption
        + 24.;
    let high = scene
        .edges
        .iter()
        .map(|e| l.ports[&port_key(e, false)].stub.x)
        .fold(f64::INFINITY, f64::min)
        - 16.;
    let spacing = o.lane_spacing.max(24.);
    if low + (scene.edges.len() - 1) as f64 * spacing > high {
        return false;
    }
    let make = |order: &[usize]| {
        let mut routes = BTreeMap::new();
        for (lane, &index) in order.iter().enumerate() {
            let e = &scene.edges[index];
            let a = &l.ports[&port_key(e, true)];
            let b = &l.ports[&port_key(e, false)];
            let x = low + lane as f64 * spacing;
            let mut points = vec![
                a.point,
                a.stub,
                Point::new(x, a.stub.y),
                Point::new(x, b.stub.y),
                b.stub,
                b.point,
            ];
            points.dedup_by(|a, b| a.distance(*b) < 1e-7);
            routes.insert(
                e.id.clone(),
                Route {
                    edge: e.id.clone(),
                    points,
                },
            );
        }
        routes
    };
    let cost = |routes: &BTreeMap<String, Route>| {
        let v: Vec<_> = routes.values().collect();
        let mut count = 0;
        for (i, a) in v.iter().enumerate() {
            for b in &v[i + 1..] {
                for p in a.points.windows(2) {
                    for q in b.points.windows(2) {
                        count +=
                            usize::from(crate::routing::proper_crossing(p[0], p[1], q[0], q[1]));
                    }
                }
            }
        }
        count
    };
    let mut order: Vec<_> = (0..scene.edges.len()).collect();
    let mut score = cost(&make(&order));
    let mut budget = o.shape_budget.min(256);
    loop {
        let mut improved = false;
        for i in 0..order.len() {
            for j in i + 1..order.len() {
                if budget == 0 {
                    break;
                }
                budget -= 1;
                order.swap(i, j);
                let next = cost(&make(&order));
                if next < score {
                    score = next;
                    improved = true;
                } else {
                    order.swap(i, j);
                }
            }
        }
        if !improved || budget == 0 {
            break;
        }
    }
    commit(scene, o, l, make(&order))
}
