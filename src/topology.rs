//! Bounded weighted-SAT macro placement for cyclic cores.
//!
//! Boolean variables `x[v,cell]` use exactly-one row clauses and at-most-one column
//! clauses. Pairwise soft costs estimate port-aware route turns, length and blocked
//! channels; crossing soft clauses couple completed edge shapes. The search branches
//! on one-hot groups and propagates occupied cells directly. It always retains a
//! feasible incumbent and stops after a deterministic number of search states.
use crate::{
    layout::{LayoutOptions, Point, Rect},
    model::{Endpoint, PortConstraint, SceneGraph, Side},
    routing::{collinear_overlap, proper_crossing},
};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Default)]
pub struct ShapeStats {
    pub searched_states: usize,
    pub optimized_components: usize,
    pub before: f64,
    pub after: f64,
}
pub fn hard_clauses(nodes: usize) -> Vec<Vec<i32>> {
    let var = |v: usize, c: usize| (v * nodes + c + 1) as i32;
    let mut clauses = Vec::new();
    for v in 0..nodes {
        clauses.push((0..nodes).map(|c| var(v, c)).collect());
        for a in 0..nodes {
            for b in a + 1..nodes {
                clauses.push(vec![-var(v, a), -var(v, b)]);
                clauses.push(vec![-var(a, v), -var(b, v)]);
            }
        }
    }
    clauses
}
pub fn components(scene: &SceneGraph) -> Vec<Vec<usize>> {
    let n = scene.nodes.len();
    let mut adj = vec![Vec::new(); n];
    let mut reverse = vec![Vec::new(); n];
    for e in &scene.edges {
        let (a, b) = (
            scene.index(&e.source.node).unwrap(),
            scene.index(&e.target.node).unwrap(),
        );
        adj[a].push(b);
        reverse[b].push(a);
    }
    fn visit(i: usize, adj: &[Vec<usize>], seen: &mut [bool], order: &mut Vec<usize>) {
        if seen[i] {
            return;
        }
        seen[i] = true;
        for &j in &adj[i] {
            visit(j, adj, seen, order);
        }
        order.push(i);
    }
    let mut seen = vec![false; n];
    let mut order = Vec::new();
    for i in 0..n {
        visit(i, &adj, &mut seen, &mut order);
    }
    let mut assigned = vec![false; n];
    let mut groups = Vec::new();
    for &i in order.iter().rev() {
        if assigned[i] {
            continue;
        }
        let mut group = Vec::new();
        let mut stack = vec![i];
        assigned[i] = true;
        while let Some(j) = stack.pop() {
            group.push(j);
            for &k in &reverse[j] {
                if !assigned[k] {
                    assigned[k] = true;
                    stack.push(k);
                }
            }
        }
        group.sort_unstable();
        groups.push(group);
    }
    groups
}
#[derive(Clone)]
struct Path {
    cost: f64,
    points: Vec<Point>,
}
struct Search<'a> {
    scene: &'a SceneGraph,
    options: &'a LayoutOptions,
    members: &'a [usize],
    sizes: Vec<Rect>,
    cells: Vec<Point>,
    edges: Vec<usize>,
    endpoints: Vec<(usize, usize)>,
    order: Vec<usize>,
    assignment: Vec<usize>,
    used: Vec<bool>,
    best: Vec<usize>,
    best_cost: f64,
    cache: BTreeMap<(usize, usize, usize), Path>,
    states: usize,
    budget: usize,
}
impl Search<'_> {
    fn route(&mut self, e: usize, a: usize, b: usize) -> Path {
        if let Some(path) = self.cache.get(&(e, a, b)) {
            return path.clone();
        }
        let edge = &self.scene.edges[self.edges[e]];
        let (va, vb) = self.endpoints[e];
        let mut ra = self.sizes[va];
        let mut rb = self.sizes[vb];
        ra.x = self.cells[a].x;
        ra.y = self.cells[a].y;
        rb.x = self.cells[b].x;
        rb.y = self.cells[b].y;
        let (pa, sa) = port(&edge.source, ra, rb, self.options);
        let (pb, sb) = port(&edge.target, rb, ra, self.options);
        let an = sa.normal();
        let bn = sb.normal();
        let qs = pa.offset(an.0 * self.options.stub, an.1 * self.options.stub);
        let qt = pb.offset(bn.0 * self.options.stub, bn.1 * self.options.stub);
        let maxw = self.sizes.iter().map(|r| r.w).reduce(f64::max).unwrap();
        let maxh = self.sizes.iter().map(|r| r.h).reduce(f64::max).unwrap();
        let mut obstacles: Vec<_> = self
            .cells
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != a && *i != b)
            .map(|(_, p)| Rect::new(p.x, p.y, maxw, maxh).inflate(self.options.margin))
            .collect();
        obstacles.extend([
            ra.inflate(self.options.margin),
            rb.inflate(self.options.margin),
        ]);
        let middles = [
            vec![Point::new(qt.x, qs.y)],
            vec![Point::new(qs.x, qt.y)],
            vec![
                Point::new((qs.x + qt.x) / 2., qs.y),
                Point::new((qs.x + qt.x) / 2., qt.y),
            ],
            vec![
                Point::new(qs.x, (qs.y + qt.y) / 2.),
                Point::new(qt.x, (qs.y + qt.y) / 2.),
            ],
        ];
        let mut best = Path {
            cost: f64::INFINITY,
            points: Vec::new(),
        };
        for middle in middles {
            let mut points = vec![pa, qs];
            points.extend(middle);
            points.extend([qt, pb]);
            points.dedup_by(|a, b| a.distance(*b) < 1e-6);
            let mut cost = 0.;
            for (i, p) in points.windows(2).enumerate() {
                cost += p[0].distance(p[1]) / self.options.grid * self.options.weights.length;
                let relevant = if i == 0 || i + 2 == points.len() {
                    &obstacles[..obstacles.len() - 2]
                } else {
                    &obstacles
                };
                cost += relevant.iter().filter(|r| r.blocks(p[0], p[1])).count() as f64 * 1e6;
            }
            for p in points.windows(3) {
                let ax = p[1].x - p[0].x;
                let ay = p[1].y - p[0].y;
                let bx = p[2].x - p[1].x;
                let by = p[2].y - p[1].y;
                if (ax * by - ay * bx).abs() > 1e-6 {
                    cost += self.options.weights.bend;
                } else if ax * bx + ay * by < 0. {
                    cost += 2. * self.options.weights.bend;
                }
            }
            if cost < best.cost {
                best = Path { cost, points };
            }
        }
        self.cache.insert((e, a, b), best.clone());
        best
    }
    fn pair_cost(&self, a: usize, pa: &Path, b: usize, pb: &Path) -> f64 {
        let ea = &self.scene.edges[self.edges[a]];
        let eb = &self.scene.edges[self.edges[b]];
        if ea.source.node == eb.source.node
            && ea.source.port.is_some()
            && ea.source.port == eb.source.port
        {
            return 0.;
        }
        let mut cost = 0.;
        for a in pa.points.windows(2) {
            for b in pb.points.windows(2) {
                if proper_crossing(a[0], a[1], b[0], b[1]) {
                    cost += self.options.weights.crossing;
                }
                cost += collinear_overlap(a[0], a[1], b[0], b[1]) / self.options.grid
                    * self.options.weights.bend;
            }
        }
        cost
    }
    fn complete_cost(&mut self, assignment: &[usize]) -> f64 {
        let mut cost = 0.;
        let mut paths = Vec::new();
        for e in 0..self.edges.len() {
            let (a, b) = self.endpoints[e];
            let path = self.route(e, assignment[a], assignment[b]);
            cost += path.cost;
            for (j, old) in paths.iter().enumerate() {
                cost += self.pair_cost(e, &path, j, old);
            }
            paths.push(path);
        }
        cost + assignment
            .iter()
            .enumerate()
            .map(|(i, c)| i.abs_diff(*c) as f64 * self.options.weights.source_order)
            .sum::<f64>()
    }
    fn branch(&mut self, depth: usize, cost: f64, completed: &mut Vec<(usize, Path)>) {
        if self.states >= self.budget || cost >= self.best_cost - 1e-8 {
            return;
        }
        self.states += 1;
        if depth == self.order.len() {
            self.best = self.assignment.clone();
            self.best_cost = cost;
            return;
        }
        let v = self.order[depth];
        let mut cells: Vec<_> = (0..self.cells.len()).filter(|c| !self.used[*c]).collect();
        cells.sort_by(|a, b| {
            let estimate = |c: usize| {
                self.endpoints
                    .iter()
                    .filter_map(|&(a, b)| {
                        let peer = if a == v {
                            b
                        } else if b == v {
                            a
                        } else {
                            return None;
                        };
                        (self.assignment[peer] != usize::MAX)
                            .then(|| self.cells[c].distance(self.cells[self.assignment[peer]]))
                    })
                    .sum::<f64>()
            };
            estimate(*a)
                .total_cmp(&estimate(*b))
                .then(v.abs_diff(*a).cmp(&v.abs_diff(*b)))
                .then(a.cmp(b))
        });
        for cell in cells {
            self.assignment[v] = cell;
            self.used[cell] = true;
            let before = completed.len();
            let mut extra = v.abs_diff(cell) as f64 * self.options.weights.source_order;
            for e in 0..self.edges.len() {
                let (a, b) = self.endpoints[e];
                if a != v && b != v
                    || self.assignment[a] == usize::MAX
                    || self.assignment[b] == usize::MAX
                {
                    continue;
                }
                let path = self.route(e, self.assignment[a], self.assignment[b]);
                extra += path.cost;
                for (other, old) in completed.iter() {
                    extra += self.pair_cost(e, &path, *other, old);
                }
                completed.push((e, path));
            }
            self.branch(depth + 1, cost + extra, completed);
            completed.truncate(before);
            self.used[cell] = false;
            self.assignment[v] = usize::MAX;
            if self.states >= self.budget {
                break;
            }
        }
    }
}
fn port(ep: &Endpoint, a: Rect, b: Rect, o: &LayoutOptions) -> (Point, Side) {
    let side = ep.constraint.side().unwrap_or_else(|| {
        if (b.x - a.x).abs() >= (b.y - a.y).abs() {
            if b.x >= a.x { Side::E } else { Side::W }
        } else if b.y >= a.y {
            Side::S
        } else {
            Side::N
        }
    });
    let len = if matches!(side, Side::N | Side::S) {
        a.w
    } else {
        a.h
    };
    let t = match ep.constraint {
        PortConstraint::FixedRatio { ratio, .. } => ratio * len,
        PortConstraint::FixedPos { offset_grid, .. } => offset_grid * o.grid,
        _ => len / 2.,
    };
    let p = match side {
        Side::N => Point::new(a.left() + t, a.top()),
        Side::E => Point::new(a.right(), a.top() + t),
        Side::S => Point::new(a.left() + t, a.bottom()),
        Side::W => Point::new(a.left(), a.top() + t),
    };
    (p, side)
}
pub fn optimize(scene: &SceneGraph, rects: &mut [Rect], o: &LayoutOptions) -> ShapeStats {
    let mut stats = ShapeStats::default();
    if o.shape_budget == 0 {
        return stats;
    }
    for members in components(scene) {
        let n = members.len();
        if !(3..=30).contains(&n)
            || members.iter().any(|i| {
                scene.nodes[*i].container
                    || scene.nodes[*i].parent != scene.nodes[members[0]].parent
                    || crate::layout::effective_annotations(
                        scene,
                        &scene.nodes[*i].id,
                        &scene.nodes[*i].annotations,
                        o.variant.as_deref(),
                    )
                    .iter()
                    .any(|a| matches!(a.name.as_str(), "pin" | "route"))
            })
            || scene.relations.iter().any(|r| {
                r.members
                    .iter()
                    .any(|id| members.iter().any(|i| scene.nodes[*i].id == *id))
            })
        {
            continue;
        }
        let edges: Vec<_> = scene
            .edges
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                members.contains(&scene.index(&e.source.node).unwrap())
                    && members.contains(&scene.index(&e.target.node).unwrap())
                    && e.source.node != e.target.node
            })
            .map(|(i, _)| i)
            .collect();
        let endpoints: Vec<_> = edges
            .iter()
            .map(|e| {
                let e = &scene.edges[*e];
                (
                    members
                        .iter()
                        .position(|i| scene.nodes[*i].id == e.source.node)
                        .unwrap(),
                    members
                        .iter()
                        .position(|i| scene.nodes[*i].id == e.target.node)
                        .unwrap(),
                )
            })
            .collect();
        let sizes: Vec<_> = members.iter().map(|i| rects[*i]).collect();
        let original: Vec<_> = sizes.iter().map(|r| r.center()).collect();
        let center = Point::new(
            original.iter().map(|p| p.x).sum::<f64>() / n as f64,
            original.iter().map(|p| p.y).sum::<f64>() / n as f64,
        );
        let width =
            sizes.iter().map(|r| r.w).reduce(f64::max).unwrap() + 2. * o.stub + 4. * o.margin;
        let height =
            sizes.iter().map(|r| r.h).reduce(f64::max).unwrap() + 2. * o.stub + 4. * o.margin;
        let columns = (n as f64).sqrt().ceil() as usize;
        let rows = n.div_ceil(columns);
        let cells: Vec<_> = (0..n)
            .map(|i| {
                Point::new(
                    center.x + (i % columns) as f64 * width - (columns - 1) as f64 * width / 2.,
                    center.y + (i / columns) as f64 * height - (rows - 1) as f64 * height / 2.,
                )
            })
            .collect();
        let mut order: Vec<_> = (0..n).collect();
        order.sort_by_key(|i| {
            (
                std::cmp::Reverse(endpoints.iter().filter(|(a, b)| a == i || b == i).count()),
                *i,
            )
        });
        let mut search = Search {
            scene,
            options: o,
            members: &members,
            sizes,
            cells: original,
            edges,
            endpoints,
            order,
            assignment: vec![usize::MAX; n],
            used: vec![false; n],
            best: (0..n).collect(),
            best_cost: f64::INFINITY,
            cache: BTreeMap::new(),
            states: 0,
            budget: o.shape_budget.saturating_sub(stats.searched_states),
        };
        if n >= 6
            && (0..n).any(|v| {
                search.endpoints.iter().filter(|&&(a, _)| a == v).count() >= 4
                    || search.endpoints.iter().filter(|&&(_, b)| b == v).count() >= 4
            })
        {
            continue;
        }
        let identity: Vec<_> = (0..n).collect();
        let original_cost = search.complete_cost(&identity);
        search.cells = cells;
        search.cache.clear();
        for rotation in 0..n.min(4) {
            let seed: Vec<_> = (0..n).map(|i| (i + rotation) % n).collect();
            let cost = search.complete_cost(&seed);
            if cost < search.best_cost {
                search.best_cost = cost;
                search.best = seed;
            }
        }
        search.branch(0, 0., &mut Vec::new());
        stats.searched_states += search.states;
        stats.before += original_cost;
        stats.after += original_cost.min(search.best_cost);
        if search.best_cost + 1e-8 < original_cost {
            for (v, &i) in search.members.iter().enumerate() {
                let p = search.cells[search.best[v]];
                rects[i].x = p.x;
                rects[i].y = p.y;
            }
            stats.optimized_components += 1;
        }
        if stats.searched_states >= o.shape_budget {
            break;
        }
    }
    stats
}

/// Coffman–Graham labeling and bounded-width level packing of the SCC condensation DAG.
/// Cyclic cores are compound vertices here and receive their own shape search afterward.
pub fn coffman_graham(scene: &SceneGraph, width: usize) -> Vec<usize> {
    let groups = components(scene);
    let count = groups.len();
    if count == 0 {
        return Vec::new();
    }
    let mut owner = vec![0; scene.nodes.len()];
    for (i, g) in groups.iter().enumerate() {
        for &v in g {
            owner[v] = i;
        }
    }
    let mut pred = vec![Vec::new(); count];
    let mut succ = vec![Vec::new(); count];
    for e in &scene.edges {
        let (a, b) = (
            owner[scene.index(&e.source.node).unwrap()],
            owner[scene.index(&e.target.node).unwrap()],
        );
        if a != b && !succ[a].contains(&b) {
            succ[a].push(b);
            pred[b].push(a);
        }
    }
    let mut labels = vec![0; count];
    for label in 1..=count {
        let chosen = (0..count)
            .filter(|i| labels[*i] == 0 && pred[*i].iter().all(|p| labels[*p] > 0))
            .min_by(|a, b| {
                let key = |i: usize| {
                    let mut values: Vec<_> = pred[i].iter().map(|p| labels[*p]).collect();
                    values.sort_by(|a, b| b.cmp(a));
                    values
                };
                key(*a)
                    .cmp(&key(*b))
                    .then(groups[*a][0].cmp(&groups[*b][0]))
            })
            .expect("SCC condensation is acyclic");
        labels[chosen] = label;
    }
    let mut order: Vec<_> = (0..count).collect();
    order.sort_by_key(|i| std::cmp::Reverse(labels[*i]));
    let mut levels = vec![0; count];
    let mut occupied = Vec::<usize>::new();
    for i in order {
        let mut level = succ[i].iter().map(|j| levels[*j] + 1).max().unwrap_or(0);
        while occupied.get(level).copied().unwrap_or(0) >= width.max(1) {
            level += 1;
        }
        if occupied.len() <= level {
            occupied.resize(level + 1, 0);
        }
        occupied[level] += 1;
        levels[i] = level;
    }
    let last = levels.iter().copied().max().unwrap_or(0);
    let mut inner_offsets: Vec<Vec<usize>> = Vec::with_capacity(count);
    let mut group_spans = vec![1usize; count];
    for (gi, g) in groups.iter().enumerate() {
        if g.len() <= 1 {
            inner_offsets.push(vec![0; g.len()]);
            continue;
        }
        let m = g.len();
        let mut entries: Vec<usize> = (0..m)
            .filter(|&k| {
                let id = &scene.nodes[g[k]].id;
                scene.edges.iter().any(|e| {
                    &e.target.node == id && owner[scene.index(&e.source.node).unwrap()] != gi
                })
            })
            .collect();
        if entries.is_empty() {
            entries.push(0);
        }
        let mut dist = vec![usize::MAX; m];
        let mut queue = std::collections::VecDeque::new();
        for &e_idx in &entries {
            dist[e_idx] = 0;
            queue.push_back(e_idx);
        }
        while let Some(u) = queue.pop_front() {
            let uid = &scene.nodes[g[u]].id;
            for e in &scene.edges {
                if &e.source.node == uid
                    && let Some(v) = g
                        .iter()
                        .position(|&idx| scene.nodes[idx].id == e.target.node)
                    && dist[v] == usize::MAX
                {
                    dist[v] = dist[u] + 1;
                    queue.push_back(v);
                }
            }
        }
        for (k, d) in dist.iter_mut().enumerate() {
            if *d == usize::MAX {
                *d = k;
            }
        }
        let mut order_in: Vec<usize> = (0..m).collect();
        order_in.sort_by_key(|&k| (dist[k], k));
        let mut inner = vec![0usize; m];
        for &u in &order_in {
            let uid = &scene.nodes[g[u]].id;
            for e in &scene.edges {
                if &e.source.node == uid
                    && let Some(v) = g
                        .iter()
                        .position(|&idx| scene.nodes[idx].id == e.target.node)
                    && dist[v] > dist[u]
                {
                    inner[v] = inner[v].max(inner[u] + 1);
                }
            }
        }
        group_spans[gi] = inner.iter().copied().max().unwrap_or(0) + 1;
        inner_offsets.push(inner);
    }
    let mut step_span = vec![1usize; last + 1];
    for i in 0..count {
        let step = last - levels[i];
        step_span[step] = step_span[step].max(group_spans[i]);
    }
    let mut step_start = vec![0usize; last + 1];
    for s in 1..=last {
        step_start[s] = step_start[s - 1] + step_span[s - 1];
    }
    let mut ranks = vec![0; scene.nodes.len()];
    for (i, g) in groups.iter().enumerate() {
        let base = step_start[last - levels[i]];
        for (k, &v) in g.iter().enumerate() {
            ranks[v] = base + inner_offsets[i][k];
        }
    }
    let cap = width.max(1);
    let max_rank = ranks.iter().copied().max().unwrap_or(0);
    let mut counts = vec![0usize; max_rank + 1];
    for &r in &ranks {
        counts[r] += 1;
    }
    for (i, g) in groups.iter().enumerate() {
        if g.len() == 1 && succ[i].is_empty() && !pred[i].is_empty() {
            let v = g[0];
            let min_rank = pred[i]
                .iter()
                .flat_map(|&p| &groups[p])
                .map(|&u| ranks[u] + 1)
                .max()
                .unwrap_or(0);
            if min_rank + 1 < ranks[v]
                && let Some(target) = (min_rank..ranks[v]).find(|&r| counts[r] < cap)
            {
                counts[ranks[v]] -= 1;
                ranks[v] = target;
                counts[target] += 1;
            }
        }
    }
    ranks
}

pub(crate) type SideHints = Vec<Option<(Side, Side)>>;
/// Recognize elementary concepts. Fresh placement may move nodes; route-only
/// planning reuses existing pinned or dragged geometry without changing it.
pub(crate) fn readable_concept(
    scene: &SceneGraph,
    rects: &mut [Rect],
    o: &LayoutOptions,
    place_nodes: bool,
) -> Option<SideHints> {
    let n = scene.nodes.len();
    if o.shape_budget == 0
        || n < 2
        || scene.nodes.iter().any(|node| {
            node.container
                || node.parent.is_some()
                || !node.ports.is_empty()
                || crate::layout::effective_annotations(
                    scene,
                    &node.id,
                    &node.annotations,
                    o.variant.as_deref(),
                )
                .iter()
                .any(|a| a.name == "pin" && (place_nodes || a.attrs.contains_key("lock")))
        })
        || place_nodes && !scene.relations.is_empty()
        || scene.edges.iter().any(|e| {
            !matches!(e.source.constraint, PortConstraint::Free)
                || !matches!(e.target.constraint, PortConstraint::Free)
                || {
                    let annotations = crate::layout::effective_annotations(
                        scene,
                        &e.id,
                        &e.annotations,
                        o.variant.as_deref(),
                    );
                    if place_nodes {
                        !annotations.is_empty()
                    } else {
                        annotations.iter().any(|a| a.name == "route")
                    }
                }
        })
    {
        return None;
    }
    let pairs: Vec<_> = scene
        .edges
        .iter()
        .map(|e| {
            (
                scene.index(&e.source.node).unwrap(),
                scene.index(&e.target.node).unwrap(),
            )
        })
        .collect();
    let mut incoming = vec![0; n];
    let mut outgoing = vec![0; n];
    for &(a, b) in &pairs {
        outgoing[a] += 1;
        incoming[b] += 1;
    }
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
    if n == 2 && pairs.len() >= 2 && pairs.iter().all(|p| *p == pairs[0]) {
        let (a, b) = pairs[0];
        if a == b {
            return None;
        }
        if place_nodes {
            let spacing = o.port_spacing.max(caption_height + 12.);
            let height = (pairs.len() + 1) as f64 * spacing;
            rects[a].h = rects[a].h.max(height);
            rects[b].h = rects[b].h.max(height);
            rects[a].x = 0.;
            rects[a].y = 0.;
            rects[b].y = 0.;
            rects[b].x = (rects[a].w + rects[b].w) / 2. + caption_width + 96.;
        }
        if rects[a].right() + 2. * o.stub >= rects[b].left() {
            return None;
        }
        return Some(vec![Some((Side::E, Side::W)); pairs.len()]);
    }
    let sources: Vec<_> = (0..n)
        .filter(|&i| incoming[i] == 0 && outgoing[i] == 2)
        .collect();
    let sinks: Vec<_> = (0..n)
        .filter(|&i| outgoing[i] == 0 && incoming[i] == 2)
        .collect();
    if n == 4 && pairs.len() == 4 && sources.len() == 2 && sinks.len() == 2 {
        if place_nodes {
            let step = rects.iter().map(|r| r.h).fold(0., f64::max) + caption_height + 96.;
            let gap = rects.iter().map(|r| r.w).fold(0., f64::max) + 2. * caption_width + 160.;
            for (row, &i) in sources.iter().enumerate() {
                rects[i].x = 0.;
                rects[i].y = row as f64 * step;
            }
            for (row, &i) in sinks.iter().enumerate() {
                rects[i].x = gap;
                rects[i].y = row as f64 * step;
            }
        }
        if sources.iter().any(|&a| {
            sinks
                .iter()
                .any(|&b| rects[a].right() + 2. * o.stub >= rects[b].left())
        }) || rects[sources[0]].y >= rects[sources[1]].y
            || rects[sinks[0]].y >= rects[sinks[1]].y
        {
            return None;
        }
        return Some(
            pairs
                .iter()
                .map(|&(a, b)| {
                    Some(if a == sources[0] && b == sinks[1] {
                        (Side::W, Side::S)
                    } else {
                        (Side::E, Side::W)
                    })
                })
                .collect(),
        );
    }
    if n >= 3 && pairs.len() == n {
        let returns: Vec<_> = pairs
            .iter()
            .enumerate()
            .filter(|(i, (a, b))| a != b && pairs[..*i].contains(&(*b, *a)))
            .collect();
        if returns.len() == 1 {
            let (index, &(a, b)) = returns[0];
            let forward = pairs.iter().position(|p| *p == (b, a)).unwrap();
            if !place_nodes {
                if (rects[a].x - rects[b].x).abs() < (rects[a].y - rects[b].y).abs() {
                    return None;
                }
                let top = rects.iter().map(|r| r.top()).fold(f64::INFINITY, f64::min) - 72.;
                for node in [a, b] {
                    let corridor = Rect::new(
                        rects[node].x,
                        (top + rects[node].top()) / 2.,
                        2. * o.margin,
                        rects[node].top() - top,
                    );
                    if rects
                        .iter()
                        .enumerate()
                        .any(|(i, r)| i != a && i != b && r.intersects(corridor))
                    {
                        return None;
                    }
                }
            }
            let mut hints = vec![None; pairs.len()];
            hints[index] = Some((Side::N, Side::N));
            hints[forward] = Some(if rects[b].x < rects[a].x {
                (Side::E, Side::W)
            } else {
                (Side::W, Side::E)
            });
            return Some(hints);
        }
    }
    if n >= 4 && pairs.len() == n - 1 {
        let hub = (0..n).find(|&i| {
            (outgoing[i] == n - 1 && incoming[i] == 0) || (incoming[i] == n - 1 && outgoing[i] == 0)
        });
        if let Some(hub) = hub {
            let fan_out = outgoing[hub] == n - 1;
            if (0..n).filter(|&i| i != hub).all(|i| {
                if fan_out {
                    incoming[i] == 1 && outgoing[i] == 0
                } else {
                    outgoing[i] == 1 && incoming[i] == 0
                }
            }) {
                let leaves: Vec<_> = (0..n).filter(|&i| i != hub).collect();
                if place_nodes {
                    let leaf_width = leaves.iter().map(|&i| rects[i].w).fold(0., f64::max);
                    let step = leaves.iter().map(|&i| rects[i].h).fold(0., f64::max)
                        + caption_height.max(24.)
                        + 48.;
                    let gap = (rects[hub].w + leaf_width) / 2. + 2. * caption_width + 160.;
                    rects[hub].x = if fan_out { 0. } else { gap };
                    rects[hub].y = 0.;
                    for (row, &i) in leaves.iter().enumerate() {
                        rects[i].x = if fan_out { gap } else { 0. };
                        rects[i].y = (row as f64 - (leaves.len() - 1) as f64 / 2.) * step;
                    }
                }
                if pairs
                    .iter()
                    .any(|&(a, b)| rects[a].right() + 2. * o.stub >= rects[b].left())
                {
                    return None;
                }
                return Some(vec![Some((Side::E, Side::W)); pairs.len()]);
            }
        }
    }
    None
}
