//! Direction-state A* on an orthogonal visibility lattice and conservative channel nudging.
use crate::{Error, layout::*, model::*};
use std::{cmp::Ordering, collections::BinaryHeap};
#[derive(Clone, Copy)]
struct Queue {
    f: f64,
    g: f64,
    state: usize,
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
        b.f.total_cmp(&self.f)
            .then_with(|| b.g.total_cmp(&self.g))
            .then_with(|| b.state.cmp(&self.state))
    }
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
    // Routes may cross an owning frame, but its title still needs a reserved
    // channel. Excluding the whole ancestor used to let peer loops strike it.
    for n in scene.nodes.iter().filter(|n| {
        n.container
            && n.shape != "sql_table"
            && exclude.contains(&n.id)
            && n.id != e.source.node
            && n.id != e.target.node
    }) {
        let r = l.nodes[&n.id];
        let width = crate::labels::text_width(&n.label) * 16. / 15. + 8.;
        bodies.push(
            Rect::new(
                r.left() + 12. + width / 2.,
                r.top() + 19. + (header_height(n) - 24.) / 2.,
                width,
                28. + header_height(n) - 24.,
            )
            .inflate(margin / 2.),
        );
    }
    bodies
}
pub fn route_all(
    scene: &SceneGraph,
    o: &LayoutOptions,
    prior: Option<&Layout>,
    l: &mut Layout,
) -> Result<(), Error> {
    // Costs include only routes accepted for this frame, never an edge's old
    // path or paths left over from a previous pointer position.
    l.routes.clear();
    l.reused_routes = 0;
    for e in &scene.edges {
        let a = l.ports[&port_key(e, true)].clone();
        let b = l.ports[&port_key(e, false)].clone();
        let mut obs = obstacles(scene, l, e, o.margin);
        let mut reused = None;
        if let Some(old) = prior
            && old.ports.get(&a.key) == Some(&a)
            && old.ports.get(&b.key) == Some(&b)
            && effective_annotations(scene, &e.id, &e.annotations, o.variant.as_deref()).is_empty()
            && let Some(r) = old.routes.get(&e.id)
            && route_is_valid(scene, l, e, r, o.margin)
        {
            reused = Some(r.clone());
        }
        if let Some(r) = reused {
            l.routes.insert(e.id.clone(), r);
            l.reused_routes += 1;
            continue;
        }
        // Interior route cannot re-enter either endpoint box after its mandatory stub.
        for id in [&e.source.node, &e.target.node] {
            let r = l.nodes[id].inflate(o.margin);
            if !obs.contains(&r) {
                obs.push(r);
            }
        }
        let annotations = effective_annotations(scene, &e.id, &e.annotations, o.variant.as_deref());
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
        let tight_straight = channel.is_none()
            && !annotations.iter().any(|a| a.name == "route")
            && dir(a.side) == opposite(dir(b.side))
            && distance > 0.
            && ((b.point.x - a.point.x) * ny - (b.point.y - a.point.y) * nx).abs() < 1e-6
            && distance <= a.point.distance(a.stub) + b.point.distance(b.stub) + 1e-6;
        let route = if tight_straight {
            // Facing stubs can overlap on a short straight connection. Order
            // their vertices along the shaft instead of drawing a hairpin.
            let mut points = vec![a.stub, b.stub];
            points.sort_by(|p, q| {
                ((p.x - a.point.x) * nx + (p.y - a.point.y) * ny)
                    .total_cmp(&((q.x - a.point.x) * nx + (q.y - a.point.y) * ny))
            });
            points
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
            let mut one = search(a.stub, p, dir(a.side), channel_direction, &obs, l, o)?;
            let two = search(
                q,
                b.stub,
                channel_direction,
                opposite(dir(b.side)),
                &obs,
                l,
                o,
            )?;
            one.push(q);
            one.extend(two.into_iter().skip(1));
            one
        } else {
            search(
                a.stub,
                b.stub,
                dir(a.side),
                opposite(dir(b.side)),
                &obs,
                l,
                o,
            )?
        };
        let mut points = vec![a.point];
        points.extend(route);
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
                            obstacles(scene, l, e, o.margin)
                                .iter()
                                .any(|r| r.blocks(p[0], p[1]))
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
        let route = Route {
            edge: e.id.clone(),
            points,
        };
        if !route_is_valid(scene, l, e, &route, o.margin) {
            return Err(Error::Layout(format!(
                "route {} cannot preserve its attachments and clearance",
                e.id
            )));
        }
        l.routes.insert(e.id.clone(), route);
    }
    Ok(())
}
/// Reused and newly computed paths obey the same geometry contract.
pub(crate) fn route_is_valid(
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
    for (port, neighbor) in [(a, simple[1]), (b, simple[simple.len() - 2])] {
        let (nx, ny) = port.side.normal();
        let dx = neighbor.x - port.point.x;
        let dy = neighbor.y - port.point.y;
        let required = (port.stub.x - port.point.x) * nx + (port.stub.y - port.point.y) * ny;
        if (dx * ny - dy * nx).abs() > 1e-6 || dx * nx + dy * ny < required - 1e-6 {
            return false;
        }
    }
    let obs = obstacles(scene, l, e, margin);
    let endpoints = [
        l.nodes[&e.source.node].inflate(margin),
        l.nodes[&e.target.node].inflate(margin),
    ];
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
fn search(
    start: Point,
    target: Point,
    incoming: usize,
    arrival: usize,
    obstacles: &[Rect],
    routed: &Layout,
    o: &LayoutOptions,
) -> Result<Vec<Point>, Error> {
    if obstacles
        .iter()
        .any(|r| r.contains(start) || r.contains(target))
    {
        return Err(Error::Layout(
            "a port stub lies inside an obstacle; increase node separation".into(),
        ));
    }
    // A clear straight segment with no crossing/overlap attains the obstacle-free lower bound.
    let straight_dir = if (start.y - target.y).abs() < 1e-7 {
        Some(if target.x >= start.x { 0 } else { 2 })
    } else if (start.x - target.x).abs() < 1e-7 {
        Some(if target.y >= start.y { 1 } else { 3 })
    } else {
        None
    };
    if straight_dir == Some(incoming)
        && straight_dir == Some(arrival)
        && !obstacles.iter().any(|r| r.blocks(start, target))
        && routed.routes.values().all(|r| {
            r.points.windows(2).all(|p| {
                !proper_crossing(start, target, p[0], p[1])
                    && collinear_overlap(start, target, p[0], p[1]) < 1e-7
            })
        })
    {
        return Ok(vec![start, target]);
    }
    let mut xs = vec![start.x, target.x];
    let mut ys = vec![start.y, target.y];
    for r in obstacles {
        xs.extend([r.left(), r.right()]);
        ys.extend([r.top(), r.bottom()]);
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
    xs.sort_by(f64::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 1e-7);
    ys.sort_by(f64::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 1e-7);
    let nx = xs.len();
    let ny = ys.len();
    let point = |i: usize| Point::new(xs[i % nx], ys[i / nx]);
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
    distance[s] = 0.;
    open.push(Queue {
        f: 0.,
        g: 0.,
        state: s,
    });
    let mut finish = None;
    let mut best = f64::INFINITY;
    while let Some(q) = open.pop() {
        if q.g > distance[q.state] + 1e-8 {
            continue;
        }
        if q.f >= best {
            break;
        }
        let i = q.state / 4;
        let d = q.state % 4;
        let p = point(i);
        if i == ti {
            if d == opposite(arrival) {
                continue;
            }
            let total = q.g + turns(d, arrival) * o.weights.bend;
            if total < best {
                best = total;
                finish = Some(q.state);
            }
            continue;
        }
        let x = i % nx;
        let y = i / nx;
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
            let v = point(j);
            if obstacles.iter().any(|r| r.contains(v) || r.blocks(p, v)) {
                continue;
            }
            let mut cost =
                p.manhattan_distance(v) / o.grid * o.weights.length + turns(d, nd) * o.weights.bend;
            if (v.x - p.x) * (target.x - start.x) + (v.y - p.y) * (target.y - start.y) < -1e-6 {
                cost += o.weights.backward;
            }
            for r in routed.routes.values() {
                for pair in r.points.windows(2) {
                    if proper_crossing(p, v, pair[0], pair[1]) {
                        cost += o.weights.crossing;
                    } else if collinear_overlap(p, v, pair[0], pair[1]) > 1e-6 {
                        cost += collinear_overlap(p, v, pair[0], pair[1]) / o.grid * o.weights.bend;
                    }
                }
            }
            let g = q.g + cost;
            let state = j * 4 + nd;
            if g + 1e-8 < distance[state] {
                distance[state] = g;
                parent[state] = q.state;
                let heuristic = v.manhattan_distance(target) / o.grid * o.weights.length
                    + bend_lower_bound(v, target, nd, arrival) * o.weights.bend;
                open.push(Queue {
                    f: g + heuristic,
                    g,
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
pub fn nudge_channels(scene: &SceneGraph, o: &LayoutOptions, l: &mut Layout) {
    use std::collections::BTreeMap;
    #[derive(Clone)]
    struct Segment {
        edge: String,
        index: usize,
        start: f64,
        end: f64,
        entry: i8,
        locked: bool,
    }
    let mut channels: BTreeMap<(bool, u64), Vec<Segment>> = BTreeMap::new();
    for e in &scene.edges {
        let r = &l.routes[&e.id];
        let locked = effective_annotations(scene, &e.id, &e.annotations, o.variant.as_deref())
            .iter()
            .any(|a| a.name == "route");
        for i in 1..r.points.len().saturating_sub(2) {
            let (a, b) = (r.points[i], r.points[i + 1]);
            let horizontal = (a.y - b.y).abs() < 1e-6;
            let coordinate = if horizontal { a.y } else { a.x };
            let before = r.points[i - 1];
            let entry = if horizontal {
                (before.y - a.y).signum()
            } else {
                (a.x - before.x).signum()
            };
            let (start, end) = if horizontal {
                (a.x.min(b.x), a.x.max(b.x))
            } else {
                (a.y.min(b.y), a.y.max(b.y))
            };
            if end - start < o.stub {
                continue;
            }
            channels
                .entry((
                    horizontal,
                    if coordinate == 0. {
                        0.0f64.to_bits()
                    } else {
                        coordinate.to_bits()
                    },
                ))
                .or_default()
                .push(Segment {
                    edge: e.id.clone(),
                    index: i,
                    start,
                    end,
                    entry: entry as i8,
                    locked,
                });
        }
    }
    for ((horizontal, bits), mut segments) in channels {
        let coordinate = f64::from_bits(bits);
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
            if group.len() < 2 {
                continue;
            }
            group.sort_by(|a, b| a.entry.cmp(&b.entry).then(a.edge.cmp(&b.edge)));
            if group.windows(2).any(|p| p[0].edge == p[1].edge) {
                continue;
            }
            let mut constraints = Vec::new();
            let mut valid = true;
            for (i, s) in group.iter().enumerate() {
                if i > 0 {
                    constraints.push(crate::solver::Constraint::separation(
                        i - 1,
                        i,
                        o.lane_spacing,
                    ));
                }
                if s.locked {
                    constraints.push(crate::solver::Constraint::fixed(i, coordinate));
                }
                let e = scene.edge(&s.edge).unwrap();
                let mut obs = obstacles(scene, l, e, o.margin);
                obs.push(l.nodes[&e.source.node].inflate(o.margin));
                obs.push(l.nodes[&e.target.node].inflate(o.margin));
                let mut low = coordinate - 256.;
                let mut high = coordinate + 256.;
                for r in obs {
                    let along = if horizontal {
                        (r.left(), r.right())
                    } else {
                        (r.top(), r.bottom())
                    };
                    if along.0 >= s.end - 1e-6 || along.1 <= s.start + 1e-6 {
                        continue;
                    }
                    let normal = if horizontal {
                        (r.top(), r.bottom())
                    } else {
                        (r.left(), r.right())
                    };
                    if normal.1 <= coordinate + 1e-6 {
                        low = low.max(normal.1);
                    }
                    if normal.0 >= coordinate - 1e-6 {
                        high = high.min(normal.0);
                    }
                }
                if low > high {
                    valid = false;
                    break;
                }
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
            if !valid {
                continue;
            }
            let Ok(lanes) = crate::solver::project(
                &vec![coordinate; group.len()],
                &vec![1.; group.len()],
                &constraints,
            ) else {
                continue;
            };
            let mut candidates = BTreeMap::new();
            for (s, lane) in group.iter().zip(lanes) {
                let route = candidates
                    .entry(s.edge.clone())
                    .or_insert_with(|| l.routes[&s.edge].clone());
                for p in &mut route.points[s.index..=s.index + 1] {
                    if horizontal {
                        p.y = lane;
                    } else {
                        p.x = lane;
                    }
                }
            }
            for (id, r) in &candidates {
                let e = scene.edge(id).unwrap();
                if !route_is_valid(scene, l, e, r, o.margin) {
                    valid = false;
                    break;
                }
                let external = obstacles(scene, l, e, o.margin);
                let mut internal = external.clone();
                internal.push(l.nodes[&e.source.node].inflate(o.margin));
                internal.push(l.nodes[&e.target.node].inflate(o.margin));
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
            }
            if valid {
                l.routes.extend(candidates);
            } else {
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
                state: 1,
            },
            Queue {
                f: 0.,
                g: 2.,
                state: 1,
            },
            Queue {
                f: -0.,
                g: 1.,
                state: 1,
            },
            Queue {
                f: f64::NAN,
                g: 1.,
                state: 2,
            },
        ];
        for a in entries {
            for b in entries {
                assert_eq!(a == b, a.cmp(&b) == Ordering::Equal);
            }
        }
    }
}
