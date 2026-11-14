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
    optimize_scaled(scene, rects, o, 1.0)
}
/// Shape optimization whose grid cells grow with the label-retry `spacing`
/// factor, so whitespace retries also widen optimized components.
pub fn optimize_scaled(
    scene: &SceneGraph,
    rects: &mut [Rect],
    o: &LayoutOptions,
    spacing: f64,
) -> ShapeStats {
    let spacing = if spacing.is_finite() {
        spacing.max(1.0)
    } else {
        1.0
    };
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
            (sizes.iter().map(|r| r.w).reduce(f64::max).unwrap() + 2. * o.stub + 4. * o.margin)
                * spacing;
        let height =
            (sizes.iter().map(|r| r.h).reduce(f64::max).unwrap() + 2. * o.stub + 4. * o.margin)
                * spacing;
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
        let mut local = std::collections::HashMap::with_capacity(n);
        for (li, &gi) in members.iter().enumerate() {
            local.insert(gi, li);
        }
        let mset: std::collections::HashSet<usize> = members.iter().copied().collect();
        let mut ext_dirs = Vec::new();
        for e in &scene.edges {
            if let (Some(su), Some(sv)) = (scene.index(&e.source.node), scene.index(&e.target.node))
            {
                if !mset.contains(&su) && local.contains_key(&sv) {
                    let dx = (rects[su].x - center.x).round() as i32;
                    let dy = (rects[su].y - center.y).round() as i32;
                    ext_dirs.push((
                        local[&sv],
                        if dx == 0 { -1 } else { dx.signum() },
                        dy.signum(),
                    ));
                } else if local.contains_key(&su) && !mset.contains(&sv) {
                    let dx = (rects[sv].x - center.x).round() as i32;
                    let dy = (rects[sv].y - center.y).round() as i32;
                    ext_dirs.push((
                        local[&su],
                        if dx == 0 { 1 } else { dx.signum() },
                        dy.signum(),
                    ));
                }
            }
        }
        let rem_budget = o.shape_budget.saturating_sub(stats.searched_states);
        let (domus_opt, sat_used) =
            stage1a::domus_solve_component(scene, &members, rem_budget, &ext_dirs);
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
            budget: rem_budget.saturating_sub(sat_used),
        };
        let identity: Vec<_> = (0..n).collect();
        let original_cost = search.complete_cost(&identity);

        let mut domus_best_cells: Option<(Vec<Point>, f64)> = None;
        if let Some(ref domus) = domus_opt {
            let dcx = domus.gx.iter().map(|&x| x as f64).sum::<f64>() / n as f64;
            let dcy = domus.gy.iter().map(|&y| y as f64).sum::<f64>() / n as f64;
            let d_cells: Vec<Point> = (0..n)
                .map(|i| {
                    Point::new(
                        center.x + (domus.gx[i] as f64 - dcx) * width,
                        center.y + (domus.gy[i] as f64 - dcy) * height,
                    )
                })
                .collect();
            let mut probe = rects.to_vec();
            for (li, &gi) in members.iter().enumerate() {
                probe[gi].x = d_cells[li].x;
                probe[gi].y = d_cells[li].y;
            }
            if stage1a::rects_non_overlapping(&members, &probe, scene, o.margin.max(4.0)) {
                search.cells = d_cells.clone();
                search.cache.clear();
                let cost = search.complete_cost(&identity);
                search.best_cost = cost;
                search.best = identity.clone();
                domus_best_cells = Some((d_cells, cost));
            }
        }

        search.cells = cells.clone();
        search.cache.clear();
        let mut chosen_cells = cells;
        let mut grid_improved = false;
        let pre_grid_best = search.best_cost;
        for rotation in 0..n.min(4) {
            let seed: Vec<_> = (0..n).map(|i| (i + rotation) % n).collect();
            let cost = search.complete_cost(&seed);
            if cost < search.best_cost {
                let mut probe = rects.to_vec();
                for (v, &gi) in members.iter().enumerate() {
                    probe[gi].x = search.cells[seed[v]].x;
                    probe[gi].y = search.cells[seed[v]].y;
                }
                if stage1a::rects_non_overlapping(&members, &probe, scene, o.margin.max(4.0)) {
                    search.best_cost = cost;
                    search.best = seed;
                    grid_improved = true;
                }
            }
        }
        if n < 6 || sat_used == 0 {
            let before_branch_best = search.best.clone();
            let before_branch_cost = search.best_cost;
            search.branch(0, 0., &mut Vec::new());
            if search.best_cost < before_branch_cost {
                let mut probe = rects.to_vec();
                for (v, &gi) in members.iter().enumerate() {
                    probe[gi].x = search.cells[search.best[v]].x;
                    probe[gi].y = search.cells[search.best[v]].y;
                }
                if stage1a::rects_non_overlapping(&members, &probe, scene, o.margin.max(4.0)) {
                    grid_improved = true;
                } else {
                    search.best = before_branch_best;
                    search.best_cost = before_branch_cost;
                }
            }
        }
        if !grid_improved
            && let Some((d_cells, d_cost)) = domus_best_cells
            && d_cost <= pre_grid_best + 1e-8
        {
            chosen_cells = d_cells;
            search.best = identity;
            search.best_cost = d_cost;
        }
        let step_states = (sat_used + search.states).max(1).min(rem_budget);
        stats.searched_states += step_states;
        stats.before += original_cost;
        stats.after += original_cost.min(search.best_cost);
        if search.best_cost + 1e-8 < original_cost {
            for (v, &i) in search.members.iter().enumerate() {
                let p = chosen_cells[search.best[v]];
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
    stage1a::transitive_reduce_dag(&mut succ, &mut pred);
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
        let mut intra_edges = Vec::new();
        for e in &scene.edges {
            if let (Some(su), Some(sv)) = (scene.index(&e.source.node), scene.index(&e.target.node))
                && su != sv
            {
                intra_edges.push((su, sv));
            }
        }
        let ext_zero = vec![0usize; m];
        let fas_sub = stage1a::eades_lin_smyth_subset(scene, g, &ext_zero, &ext_zero, &intra_edges);
        let mut fas_pos = vec![0usize; m];
        for (p, &gi) in fas_sub.iter().enumerate() {
            if let Some(k) = g.iter().position(|&idx| idx == gi) {
                fas_pos[k] = p;
            }
        }
        let mut order_in: Vec<usize> = (0..m).collect();
        order_in.sort_by_key(|&k| (dist[k], fas_pos[k], k));
        let mut pos_in = vec![0usize; m];
        for (p, &k) in order_in.iter().enumerate() {
            pos_in[k] = p;
        }
        let mut inner = vec![0usize; m];
        for &u in &order_in {
            let uid = &scene.nodes[g[u]].id;
            for e in &scene.edges {
                if &e.source.node == uid
                    && let Some(v) = g
                        .iter()
                        .position(|&idx| scene.nodes[idx].id == e.target.node)
                    && u != v
                    && (dist[v] > dist[u] || (dist[v] == dist[u] && pos_in[u] < pos_in[v]))
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

mod stage1a {
    use super::{FlowAxis, SideHints, TopologyPlan, components};
    use crate::formal::satisfy;
    use crate::labels::{connection_text, text_height, text_width};
    use crate::layout::{LayoutOptions, Point, Rect, effective_annotations};
    use crate::model::{Axis, SceneGraph, Side};
    use std::cmp::Reverse;
    use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

    pub(super) struct FasResult {
        pub(super) topo_pos: Vec<usize>,
        pub(super) dag_edges: Vec<(usize, usize, usize, bool)>,
        pub(super) scc_id: Vec<usize>,
    }

    pub(super) struct DomusResult {
        pub(super) gx: Vec<i32>,
        pub(super) gy: Vec<i32>,
        pub(super) edge_hints: Vec<(usize, Side, Side)>,
    }

    struct DagEdge {
        u: usize,
        v: usize,
        weight: i64,
        min_len: i32,
    }

    pub(super) fn sat_var(model: &[bool], vars: usize, var: usize) -> bool {
        if model.len() == vars + 1 {
            model[var]
        } else {
            model[var - 1]
        }
    }

    pub(super) fn is_node_pinned(scene: &SceneGraph, idx: usize, o: &LayoutOptions) -> bool {
        let n = &scene.nodes[idx];
        effective_annotations(scene, &n.id, &n.annotations, o.variant.as_deref())
            .iter()
            .any(|a| a.name == "pin" || a.name == "lock")
    }

    fn edge_is_constrained(scene: &SceneGraph, idx: usize, o: &LayoutOptions) -> bool {
        let e = &scene.edges[idx];
        !matches!(e.source.constraint, crate::model::PortConstraint::Free)
            || !matches!(e.target.constraint, crate::model::PortConstraint::Free)
            || !effective_annotations(scene, &e.id, &e.annotations, o.variant.as_deref()).is_empty()
    }

    fn rule_row_span(scene: &SceneGraph, members: &[usize]) -> usize {
        let member_set: HashSet<usize> = members.iter().copied().collect();
        let mut max_span = 0usize;
        for rel in &scene.relations {
            if matches!(rel.axis, Axis::Row) {
                let count = rel
                    .members
                    .iter()
                    .filter_map(|id| scene.index(id))
                    .filter(|i| member_set.contains(i))
                    .count();
                max_span = max_span.max(count);
            }
        }
        max_span
    }

    pub(super) fn eades_lin_smyth_subset(
        scene: &SceneGraph,
        subset: &[usize],
        ext_in: &[usize],
        ext_out: &[usize],
        edges: &[(usize, usize)],
    ) -> Vec<usize> {
        let m = subset.len();
        if m <= 1 {
            return subset.to_vec();
        }
        let mut local = HashMap::with_capacity(m);
        for (li, &gi) in subset.iter().enumerate() {
            local.insert(gi, li);
        }
        let mut weights: BTreeMap<(usize, usize), usize> = BTreeMap::new();
        for &(gu, gv) in edges {
            if let (Some(&lu), Some(&lv)) = (local.get(&gu), local.get(&gv))
                && lu != lv
            {
                *weights.entry((lu, lv)).or_insert(0) += 1;
            }
        }
        let mut adj_out: Vec<Vec<(usize, usize)>> = vec![Vec::new(); m];
        let mut adj_in: Vec<Vec<(usize, usize)>> = vec![Vec::new(); m];
        let mut active_in = vec![0usize; m];
        let mut active_out = vec![0usize; m];
        for (&(u, v), &w) in &weights {
            adj_out[u].push((v, w));
            adj_in[v].push((u, w));
            active_out[u] += w;
            active_in[v] += w;
        }
        let mut alive = vec![true; m];
        let mut from_s1 = vec![0usize; m];
        let mut to_s2 = vec![0usize; m];
        let mut s1 = Vec::with_capacity(m);
        let mut s2 = Vec::with_capacity(m);
        let mut rem = m;
        while rem > 0 {
            let mut peeled = false;
            loop {
                let best_sink =
                    (0..m)
                        .filter(|&u| alive[u] && active_out[u] == 0)
                        .max_by(|&a, &b| {
                            let ka = (
                                to_s2[a] + ext_out[a],
                                Reverse(from_s1[a] + ext_in[a]),
                                Reverse(scene.nodes[subset[a]].id.as_str()),
                            );
                            let kb = (
                                to_s2[b] + ext_out[b],
                                Reverse(from_s1[b] + ext_in[b]),
                                Reverse(scene.nodes[subset[b]].id.as_str()),
                            );
                            ka.cmp(&kb)
                        });
                let Some(u) = best_sink else { break };
                peeled = true;
                alive[u] = false;
                rem -= 1;
                s2.push(subset[u]);
                for &(p, w) in &adj_in[u] {
                    if alive[p] {
                        active_out[p] = active_out[p].saturating_sub(w);
                        to_s2[p] += w;
                    }
                }
            }
            loop {
                let best_src =
                    (0..m)
                        .filter(|&u| alive[u] && active_in[u] == 0)
                        .max_by(|&a, &b| {
                            let ka = (
                                from_s1[a] + ext_in[a],
                                Reverse(to_s2[a] + ext_out[a]),
                                Reverse(scene.nodes[subset[a]].id.as_str()),
                            );
                            let kb = (
                                from_s1[b] + ext_in[b],
                                Reverse(to_s2[b] + ext_out[b]),
                                Reverse(scene.nodes[subset[b]].id.as_str()),
                            );
                            ka.cmp(&kb)
                        });
                let Some(u) = best_src else { break };
                peeled = true;
                alive[u] = false;
                rem -= 1;
                s1.push(subset[u]);
                for &(q, w) in &adj_out[u] {
                    if alive[q] {
                        active_in[q] = active_in[q].saturating_sub(w);
                        from_s1[q] += w;
                    }
                }
            }
            if peeled || rem == 0 {
                continue;
            }
            let best_cycle = (0..m).filter(|&u| alive[u]).max_by(|&a, &b| {
                let da = active_out[a] as isize - active_in[a] as isize;
                let db = active_out[b] as isize - active_in[b] as isize;
                let ka = (
                    da,
                    from_s1[a] + ext_in[a],
                    Reverse(to_s2[a] + ext_out[a]),
                    Reverse(scene.nodes[subset[a]].id.as_str()),
                );
                let kb = (
                    db,
                    from_s1[b] + ext_in[b],
                    Reverse(to_s2[b] + ext_out[b]),
                    Reverse(scene.nodes[subset[b]].id.as_str()),
                );
                ka.cmp(&kb)
            });
            if let Some(u) = best_cycle {
                alive[u] = false;
                rem -= 1;
                s1.push(subset[u]);
                for &(q, w) in &adj_out[u] {
                    if alive[q] {
                        active_in[q] = active_in[q].saturating_sub(w);
                        from_s1[q] += w;
                    }
                }
                for &(p, w) in &adj_in[u] {
                    if alive[p] {
                        active_out[p] = active_out[p].saturating_sub(w);
                    }
                }
            }
        }
        s1.extend(s2.into_iter().rev());
        s1
    }

    pub(super) fn transitive_reduce_dag(succ: &mut [Vec<usize>], pred: &mut [Vec<usize>]) {
        let count = succ.len();
        if count <= 2 {
            return;
        }
        for c in 0..count {
            succ[c].sort_unstable();
            succ[c].dedup();
            pred[c].sort_unstable();
            pred[c].dedup();
        }
        let mut in_deg: Vec<usize> = pred.iter().map(Vec::len).collect();
        let mut heap: BTreeSet<usize> = (0..count).filter(|&c| in_deg[c] == 0).collect();
        let mut topo = Vec::with_capacity(count);
        while let Some(c) = heap.pop_first() {
            topo.push(c);
            for &d in &succ[c] {
                in_deg[d] -= 1;
                if in_deg[d] == 0 {
                    heap.insert(d);
                }
            }
        }
        if topo.len() != count {
            return;
        }
        let words = count.div_ceil(64);
        let mut reach = vec![vec![0u64; words]; count];
        for &c in topo.iter().rev() {
            reach[c][c / 64] |= 1u64 << (c % 64);
            for &d in &succ[c] {
                let row_d = reach[d].clone();
                for (dst, src) in reach[c].iter_mut().zip(&row_d) {
                    *dst |= *src;
                }
            }
        }
        let mut new_succ = vec![Vec::new(); count];
        let mut new_pred = vec![Vec::new(); count];
        for c in 0..count {
            for &d in &succ[c] {
                let is_indirect = succ[c]
                    .iter()
                    .any(|&x| x != d && ((reach[x][d / 64] >> (d % 64)) & 1) != 0);
                if !is_indirect {
                    new_succ[c].push(d);
                    new_pred[d].push(c);
                }
            }
        }
        for c in 0..count {
            succ[c] = std::mem::take(&mut new_succ[c]);
            pred[c] = std::mem::take(&mut new_pred[c]);
        }
    }

    pub(super) fn eades_lin_smyth_fas(scene: &SceneGraph) -> FasResult {
        let n = scene.nodes.len();
        let sccs = components(scene);
        let sc = sccs.len();
        let mut scc_id = vec![0usize; n];
        for (cid, members) in sccs.iter().enumerate() {
            for &u in members {
                scc_id[u] = cid;
            }
        }
        let mut ext_in = vec![0usize; n];
        let mut ext_out = vec![0usize; n];
        let mut scc_succ: Vec<Vec<usize>> = vec![Vec::new(); sc];
        let mut scc_in_deg = vec![0usize; sc];
        let mut intra_edges: Vec<Vec<(usize, usize)>> = vec![Vec::new(); sc];
        let mut all_edges = Vec::with_capacity(scene.edges.len());
        for e in &scene.edges {
            let (Some(su), Some(sv)) = (scene.index(&e.source.node), scene.index(&e.target.node))
            else {
                all_edges.push((usize::MAX, usize::MAX));
                continue;
            };
            all_edges.push((su, sv));
            if su == sv {
                continue;
            }
            let cu = scc_id[su];
            let cv = scc_id[sv];
            if cu == cv {
                intra_edges[cu].push((su, sv));
            } else {
                scc_succ[cu].push(cv);
                ext_out[su] += 1;
                ext_in[sv] += 1;
            }
        }
        for succs in &mut scc_succ {
            succs.sort_unstable();
            succs.dedup();
            for &d in succs.iter() {
                scc_in_deg[d] += 1;
            }
        }
        let scc_key = |cid: usize| -> &str {
            sccs[cid]
                .iter()
                .map(|&u| scene.nodes[u].id.as_str())
                .min()
                .unwrap_or("")
        };
        let mut avail: BTreeSet<(&str, usize)> = (0..sc)
            .filter(|&c| scc_in_deg[c] == 0)
            .map(|c| (scc_key(c), c))
            .collect();
        let mut scc_order = Vec::with_capacity(sc);
        while let Some((_, c)) = avail.pop_first() {
            scc_order.push(c);
            for &d in &scc_succ[c] {
                scc_in_deg[d] -= 1;
                if scc_in_deg[d] == 0 {
                    avail.insert((scc_key(d), d));
                }
            }
        }
        let mut topo_order = Vec::with_capacity(n);
        for cid in scc_order {
            let members = &sccs[cid];
            if members.len() <= 1 {
                topo_order.extend_from_slice(members);
            } else {
                let ei_sub: Vec<usize> = members.iter().map(|&u| ext_in[u]).collect();
                let eo_sub: Vec<usize> = members.iter().map(|&u| ext_out[u]).collect();
                let ordered =
                    eades_lin_smyth_subset(scene, members, &ei_sub, &eo_sub, &intra_edges[cid]);
                topo_order.extend(ordered);
            }
        }
        let mut topo_pos = vec![0usize; n];
        for (pos, &u) in topo_order.iter().enumerate() {
            topo_pos[u] = pos;
        }
        let mut dag_edges = Vec::with_capacity(scene.edges.len());
        for (ei, &(su, sv)) in all_edges.iter().enumerate() {
            if su == usize::MAX || sv == usize::MAX || su == sv {
                continue;
            }
            if topo_pos[su] > topo_pos[sv] {
                dag_edges.push((sv, su, ei, true));
            } else {
                dag_edges.push((su, sv, ei, false));
            }
        }
        FasResult {
            topo_pos,
            dag_edges,
            scc_id,
        }
    }

    pub(super) fn gansner_ranks(scene: &SceneGraph, n: usize, fas: &FasResult) -> Vec<i32> {
        if n == 0 {
            return Vec::new();
        }
        let mut agg: BTreeMap<(usize, usize), i64> = BTreeMap::new();
        for &(u, v, ei, rev) in &fas.dag_edges {
            if u == v {
                continue;
            }
            let cap = connection_text(scene, &scene.edges[ei]);
            let w = if rev || cap.is_empty() {
                1
            } else {
                let cw = text_width(&cap);
                2 + (cw / 100.0).floor().clamp(0.0, 2.0) as i64
            };
            *agg.entry((u, v)).or_insert(0) += w;
        }
        let mut adj_undir = vec![Vec::new(); n];
        for &(u, v) in agg.keys() {
            adj_undir[u].push(v);
            adj_undir[v].push(u);
        }
        let mut visited = vec![false; n];
        let mut ranks = vec![0i32; n];
        for start in 0..n {
            if visited[start] {
                continue;
            }
            let mut comp = Vec::new();
            let mut q = VecDeque::new();
            visited[start] = true;
            q.push_back(start);
            while let Some(x) = q.pop_front() {
                comp.push(x);
                for &y in &adj_undir[x] {
                    if !visited[y] {
                        visited[y] = true;
                        q.push_back(y);
                    }
                }
            }
            if comp.len() <= 1 {
                continue;
            }
            comp.sort_by_key(|&u| (fas.topo_pos[u], scene.nodes[u].id.as_str()));
            let m = comp.len();
            let mut local = HashMap::with_capacity(m);
            for (li, &gi) in comp.iter().enumerate() {
                local.insert(gi, li);
            }
            let mut w_edges = Vec::new();
            let mut succ = vec![Vec::new(); m];
            let mut in_deg = vec![0usize; m];
            for (&(gu, gv), &weight) in &agg {
                if let (Some(&u), Some(&v)) = (local.get(&gu), local.get(&gv)) {
                    let idx = w_edges.len();
                    w_edges.push(DagEdge {
                        u,
                        v,
                        weight,
                        min_len: 1,
                    });
                    succ[u].push((v, idx));
                    in_deg[v] += 1;
                }
            }
            if w_edges.is_empty() {
                continue;
            }
            let mut c_rank = vec![0i32; m];
            let mut deg_tmp = in_deg.clone();
            let mut kq: BTreeSet<usize> = (0..m).filter(|&u| deg_tmp[u] == 0).collect();
            while let Some(u) = kq.pop_first() {
                for &(v, ei) in &succ[u] {
                    c_rank[v] = c_rank[v].max(c_rank[u] + w_edges[ei].min_len);
                    deg_tmp[v] -= 1;
                    if deg_tmp[v] == 0 {
                        kq.insert(v);
                    }
                }
            }
            let mut in_tree_node = vec![false; m];
            let mut tree_edge = vec![false; w_edges.len()];
            let mut incident: Vec<Vec<(usize, usize)>> = vec![Vec::new(); m];
            for (ei, e) in w_edges.iter().enumerate() {
                incident[e.u].push((e.v, ei));
                incident[e.v].push((e.u, ei));
            }
            in_tree_node[0] = true;
            let mut tree_cnt = 1usize;
            let expand_tight =
                |in_t: &mut [bool], t_edge: &mut [bool], c_rk: &[i32], t_cnt: &mut usize| {
                    let mut bq: VecDeque<usize> = (0..m).filter(|&i| in_t[i]).collect();
                    while let Some(u) = bq.pop_front() {
                        for &(v, ei) in &incident[u] {
                            if !in_t[v] {
                                let e = &w_edges[ei];
                                if c_rk[e.v] - c_rk[e.u] == e.min_len {
                                    in_t[v] = true;
                                    t_edge[ei] = true;
                                    *t_cnt += 1;
                                    bq.push_back(v);
                                }
                            }
                        }
                    }
                };
            expand_tight(&mut in_tree_node, &mut tree_edge, &c_rank, &mut tree_cnt);
            while tree_cnt < m {
                let mut best: Option<(i32, usize, bool)> = None;
                for (ei, e) in w_edges.iter().enumerate() {
                    let s = c_rank[e.v] - c_rank[e.u] - e.min_len;
                    if in_tree_node[e.u] && !in_tree_node[e.v] {
                        let cand = (s, ei, true);
                        if best.is_none_or(|b| cand.0 < b.0 || (cand.0 == b.0 && cand.1 < b.1)) {
                            best = Some(cand);
                        }
                    } else if !in_tree_node[e.u] && in_tree_node[e.v] {
                        let cand = (s, ei, false);
                        if best.is_none_or(|b| cand.0 < b.0 || (cand.0 == b.0 && cand.1 < b.1)) {
                            best = Some(cand);
                        }
                    }
                }
                let Some((slack, _ei, leaves_tree)) = best else {
                    break;
                };
                let shift = if leaves_tree { slack } else { -slack };
                for (x, r) in c_rank.iter_mut().enumerate() {
                    if in_tree_node[x] {
                        *r += shift;
                    }
                }
                expand_tight(&mut in_tree_node, &mut tree_edge, &c_rank, &mut tree_cnt);
            }
            let obj = |rk: &[i32]| -> i64 {
                w_edges
                    .iter()
                    .map(|e| e.weight * i64::from(rk[e.v] - rk[e.u]))
                    .sum()
            };
            let mut best_rank = c_rank.clone();
            let mut best_obj = obj(&best_rank);
            let max_pivots = (m * w_edges.len() * 4).clamp(64, 2048);
            let mut zero_streak = 0usize;
            for _ in 0..max_pivots {
                let mut t_adj = vec![Vec::new(); m];
                for (ei, &in_t) in tree_edge.iter().enumerate() {
                    if in_t {
                        let e = &w_edges[ei];
                        t_adj[e.u].push((e.v, ei));
                        t_adj[e.v].push((e.u, ei));
                    }
                }
                let mut tin = vec![0usize; m];
                let mut tout = vec![0usize; m];
                let mut parent_node = vec![usize::MAX; m];
                let mut timer = 0usize;
                let mut stack = vec![(0usize, usize::MAX, 0usize)];
                while let Some((u, par, step)) = stack.pop() {
                    if step == 0 {
                        timer += 1;
                        tin[u] = timer;
                        parent_node[u] = par;
                        stack.push((u, par, 1));
                        for &(v, _) in t_adj[u].iter().rev() {
                            if v != par {
                                stack.push((v, u, 0));
                            }
                        }
                    } else {
                        tout[u] = timer;
                    }
                }
                let mut leave: Option<(i64, usize, usize, bool)> = None;
                for (ei, &in_t) in tree_edge.iter().enumerate() {
                    if !in_t {
                        continue;
                    }
                    let e = &w_edges[ei];
                    let (sub_root, u_is_child) = if parent_node[e.v] == e.u {
                        (e.v, false)
                    } else {
                        (e.u, true)
                    };
                    let t0 = tin[sub_root];
                    let t1 = tout[sub_root];
                    let in_head = |x: usize| -> bool {
                        let in_s = tin[x] >= t0 && tin[x] <= t1;
                        if u_is_child { !in_s } else { in_s }
                    };
                    let mut cut = 0i64;
                    for f in &w_edges {
                        let fh = in_head(f.u);
                        let th = in_head(f.v);
                        if !fh && th {
                            cut += f.weight;
                        } else if fh && !th {
                            cut -= f.weight;
                        }
                    }
                    if cut < 0 {
                        let cand = (cut, ei, sub_root, u_is_child);
                        if leave.is_none_or(|b| cand.0 < b.0 || (cand.0 == b.0 && cand.1 < b.1)) {
                            leave = Some(cand);
                        }
                    }
                }
                let Some((_, e_leave, sub_root, u_is_child)) = leave else {
                    break;
                };
                let t0 = tin[sub_root];
                let t1 = tout[sub_root];
                let in_head = |x: usize| -> bool {
                    let in_s = tin[x] >= t0 && tin[x] <= t1;
                    if u_is_child { !in_s } else { in_s }
                };
                let mut enter: Option<(i32, usize)> = None;
                for (fi, f) in w_edges.iter().enumerate() {
                    if !tree_edge[fi] && in_head(f.u) && !in_head(f.v) {
                        let s = c_rank[f.v] - c_rank[f.u] - f.min_len;
                        if enter.is_none_or(|b| s < b.0 || (s == b.0 && fi < b.1)) {
                            enter = Some((s, fi));
                        }
                    }
                }
                let Some((delta, e_enter)) = enter else {
                    break;
                };
                if delta == 0 {
                    zero_streak += 1;
                    if zero_streak > m {
                        break;
                    }
                } else {
                    zero_streak = 0;
                    for (x, slot) in c_rank.iter_mut().enumerate() {
                        if in_head(x) {
                            *slot += delta;
                        }
                    }
                }
                tree_edge[e_leave] = false;
                tree_edge[e_enter] = true;
                let cur_obj = obj(&c_rank);
                if cur_obj < best_obj {
                    best_obj = cur_obj;
                    best_rank.clone_from(&c_rank);
                }
            }
            for u in 0..m {
                if in_deg[u] == 0
                    && !succ[u].is_empty()
                    && let Some(target) = succ[u]
                        .iter()
                        .map(|&(v, ei)| best_rank[v] - w_edges[ei].min_len)
                        .min()
                    && best_rank[u] < target
                {
                    best_rank[u] = target;
                }
            }
            let min_r = *best_rank.iter().min().unwrap_or(&0);
            for (li, &gi) in comp.iter().enumerate() {
                ranks[gi] = best_rank[li] - min_r;
            }
        }
        ranks
    }

    fn count_bilayer_crossings(
        edges: &[(usize, usize)],
        pos_u: &[usize],
        pos_v: &[usize],
        len_v: usize,
    ) -> usize {
        if edges.len() <= 1 || len_v <= 1 {
            return 0;
        }
        let mut pairs: Vec<(usize, usize)> =
            edges.iter().map(|&(u, v)| (pos_u[u], pos_v[v])).collect();
        pairs.sort_unstable();
        let mut tree = vec![0usize; len_v + 2];
        let mut crossings = 0usize;
        for (total, (_, pv)) in pairs.into_iter().enumerate() {
            let mut idx = pv + 1;
            let mut le = 0usize;
            while idx > 0 {
                le += tree[idx];
                idx -= idx.isolate_lowest_one();
            }
            crossings += total - le;
            let mut uidx = pv + 1;
            while uidx < tree.len() {
                tree[uidx] += 1;
                uidx += uidx.isolate_lowest_one();
            }
        }
        crossings
    }

    fn median_value(positions: &[usize]) -> f64 {
        let len = positions.len();
        if len == 0 {
            return -1.0;
        }
        if len % 2 == 1 {
            positions[len / 2] as f64
        } else if len == 2 {
            (positions[0] + positions[1]) as f64 * 0.5
        } else {
            let m = len / 2;
            let left = (positions[m - 1] - positions[0]) as f64;
            let right = (positions[len - 1] - positions[m]) as f64;
            if left + right > 0.0 {
                (positions[m - 1] as f64 * right + positions[m] as f64 * left) / (left + right)
            } else {
                (positions[m - 1] + positions[m]) as f64 * 0.5
            }
        }
    }

    fn pair_cross_delta(
        up_a: &[usize],
        up_b: &[usize],
        dn_a: &[usize],
        dn_b: &[usize],
        pos: &[usize],
    ) -> isize {
        let mut cur = 0isize;
        let mut swp = 0isize;
        for &ya in up_a {
            let pa = pos[ya];
            for &yb in up_b {
                let pb = pos[yb];
                if pa > pb {
                    cur += 1;
                } else if pa < pb {
                    swp += 1;
                }
            }
        }
        for &ya in dn_a {
            let pa = pos[ya];
            for &yb in dn_b {
                let pb = pos[yb];
                if pa > pb {
                    cur += 1;
                } else if pa < pb {
                    swp += 1;
                }
            }
        }
        swp - cur
    }

    pub(super) struct BkResult {
        pub(super) order: Vec<i32>,
        pub(super) edge_guides: BTreeMap<String, Vec<(i32, f64)>>,
        pub(super) crossings: usize,
    }

    pub(super) fn brandes_kopf_order_and_guides(scene: &SceneGraph, ranks: &[i32]) -> BkResult {
        let n = scene.nodes.len();
        if n == 0 {
            return BkResult {
                order: Vec::new(),
                edge_guides: BTreeMap::new(),
                crossings: 0,
            };
        }
        let min_r = *ranks.iter().min().unwrap_or(&0);
        let shifted: Vec<usize> = ranks.iter().map(|&r| (r - min_r) as usize).collect();
        let max_r = *shifted.iter().max().unwrap_or(&0);
        let mut v_rank: Vec<usize> = shifted.clone();
        let mut v_key: Vec<(u8, String)> =
            (0..n).map(|u| (0u8, scene.nodes[u].id.clone())).collect();
        let mut layer_edges: Vec<Vec<(usize, usize)>> = vec![Vec::new(); max_r];
        let mut edge_dummies: Vec<(usize, bool, Vec<usize>)> = Vec::new();
        let mut total_span = 0usize;
        for (ei, e) in scene.edges.iter().enumerate() {
            let (Some(su), Some(sv)) = (scene.index(&e.source.node), scene.index(&e.target.node))
            else {
                continue;
            };
            let ru = shifted[su];
            let rv = shifted[sv];
            if ru == rv {
                continue;
            }
            let (lo, hi, rev_dir) = if ru < rv {
                (su, sv, false)
            } else {
                (sv, su, true)
            };
            let r_lo = shifted[lo];
            let r_hi = shifted[hi];
            if r_hi - r_lo == 1 {
                layer_edges[r_lo].push((lo, hi));
            } else if total_span + (r_hi - r_lo - 1) <= 4096 {
                let mut dummies = Vec::with_capacity(r_hi - r_lo - 1);
                let mut prev = lo;
                for r in (r_lo + 1)..r_hi {
                    let vid = v_rank.len();
                    v_rank.push(r);
                    v_key.push((1u8, format!("{}#{}:{}", e.id, r, ei)));
                    layer_edges[r - 1].push((prev, vid));
                    dummies.push(vid);
                    prev = vid;
                }
                layer_edges[r_hi - 1].push((prev, hi));
                total_span += dummies.len();
                edge_dummies.push((ei, rev_dir, dummies));
            }
        }
        let tot = v_rank.len();
        let mut up_nbrs = vec![Vec::new(); tot];
        let mut dn_nbrs = vec![Vec::new(); tot];
        for r_edges in &layer_edges {
            for &(u, v) in r_edges {
                dn_nbrs[u].push(v);
                up_nbrs[v].push(u);
            }
        }
        let mut layers = vec![Vec::new(); max_r + 1];
        for (v, &r) in v_rank.iter().enumerate() {
            layers[r].push(v);
        }
        for l in &mut layers {
            l.sort_by(|&a, &b| v_key[a].cmp(&v_key[b]));
        }
        let mut pos = vec![0usize; tot];
        for l in &layers {
            for (idx, &v) in l.iter().enumerate() {
                pos[v] = idx;
            }
        }
        let sort_and_transpose = |layer: &mut Vec<usize>,
                                  nbrs: &[Vec<usize>],
                                  other_up: &[Vec<usize>],
                                  other_dn: &[Vec<usize>],
                                  pos: &mut [usize]| {
            let len_l = layer.len();
            let mut scored: Vec<(f64, usize, &(u8, String), usize)> = layer
                .iter()
                .enumerate()
                .map(|(old_idx, &v)| {
                    let mut ns: Vec<usize> = nbrs[v].iter().map(|&u| pos[u]).collect();
                    ns.sort_unstable();
                    let med = median_value(&ns);
                    let s = if med >= 0.0 { med } else { old_idx as f64 };
                    (s, old_idx, &v_key[v], v)
                })
                .collect();
            scored.sort_by(|a, b| {
                a.0.total_cmp(&b.0)
                    .then_with(|| a.2.cmp(b.2))
                    .then_with(|| a.1.cmp(&b.1))
            });
            *layer = scored.into_iter().map(|x| x.3).collect();
            for (idx, &v) in layer.iter().enumerate() {
                pos[v] = idx;
            }
            if len_l >= 2 {
                for _ in 0..4 {
                    let mut changed = false;
                    for i in 0..(len_l - 1) {
                        let a = layer[i];
                        let b = layer[i + 1];
                        if pair_cross_delta(
                            &other_up[a],
                            &other_up[b],
                            &other_dn[a],
                            &other_dn[b],
                            pos,
                        ) < 0
                        {
                            layer.swap(i, i + 1);
                            pos[a] = i + 1;
                            pos[b] = i;
                            changed = true;
                        }
                    }
                    if !changed {
                        break;
                    }
                }
            }
        };
        for layer in layers.iter_mut().take(max_r + 1).skip(1) {
            sort_and_transpose(layer, &up_nbrs, &up_nbrs, &dn_nbrs, &mut pos);
        }
        let total_cross = |ls: &[Vec<usize>], p: &[usize]| -> usize {
            (0..max_r)
                .map(|r| count_bilayer_crossings(&layer_edges[r], p, p, ls[r + 1].len()))
                .sum()
        };
        let mut best_layers = layers.clone();
        let mut best_cross = total_cross(&layers, &pos);
        for iter in 0..10 {
            if iter % 2 == 0 {
                for layer in layers.iter_mut().take(max_r + 1).skip(1) {
                    sort_and_transpose(layer, &up_nbrs, &up_nbrs, &dn_nbrs, &mut pos);
                }
            } else {
                for r in (0..max_r).rev() {
                    sort_and_transpose(&mut layers[r], &dn_nbrs, &up_nbrs, &dn_nbrs, &mut pos);
                }
            }
            let c = total_cross(&layers, &pos);
            if c < best_cross {
                best_cross = c;
                best_layers.clone_from(&layers);
            }
        }
        let mut order = vec![0i32; n];
        let mut dummy_pos = vec![0.0f64; tot];
        for l in &best_layers {
            let mut real_slots: Vec<usize> = Vec::new();
            for (idx, &v) in l.iter().enumerate() {
                if v < n {
                    order[v] = real_slots.len() as i32;
                    real_slots.push(idx);
                }
            }
            if real_slots.is_empty() {
                for (k, &v) in l.iter().enumerate() {
                    dummy_pos[v] = k as f64;
                }
                continue;
            }
            let first_real = real_slots[0];
            for k in 0..first_real {
                dummy_pos[l[k]] = -0.5 * ((first_real - k) as f64);
            }
            for s in 0..(real_slots.len() - 1) {
                let a = real_slots[s];
                let b = real_slots[s + 1];
                let cnt = b - a - 1;
                for k in 0..cnt {
                    dummy_pos[l[a + 1 + k]] = s as f64 + (k + 1) as f64 / (cnt + 1) as f64;
                }
            }
            let last_slot = *real_slots.last().unwrap();
            let tail_cnt = l.len() - 1 - last_slot;
            if tail_cnt > 0 {
                let base = (real_slots.len() - 1) as f64;
                for k in 0..tail_cnt {
                    dummy_pos[l[last_slot + 1 + k]] = base + 0.5 * ((k + 1) as f64);
                }
            }
        }
        let mut edge_guides = BTreeMap::new();
        for (ei, rev_dir, dummies) in edge_dummies {
            let mut pts: Vec<(i32, f64)> = dummies
                .into_iter()
                .map(|vid| (min_r + v_rank[vid] as i32, dummy_pos[vid]))
                .collect();
            if rev_dir {
                pts.reverse();
            }
            edge_guides.insert(scene.edges[ei].id.clone(), pts);
        }
        BkResult {
            order,
            edge_guides,
            crossings: best_cross,
        }
    }

    pub(super) struct FoldResult {
        pub(super) ranks: Vec<i32>,
        pub(super) order: Vec<i32>,
        pub(super) fold_row: Vec<usize>,
        pub(super) side_hints: SideHints,
    }

    pub(super) fn try_serpentine_fold(
        scene: &SceneGraph,
        members: &[usize],
        ranks: &[i32],
        order: &[i32],
        fas: &FasResult,
        o: &LayoutOptions,
    ) -> Option<FoldResult> {
        if members.len() < 5 {
            return None;
        }
        if members.iter().any(|&u| {
            scene.nodes[u].container
                || scene.nodes[u].parent.is_some()
                || is_node_pinned(scene, u, o)
        }) {
            return None;
        }
        if rule_row_span(scene, members) > 4 {
            return None;
        }
        let mset: HashSet<usize> = members.iter().copied().collect();
        let mut scc_counts: HashMap<usize, usize> = HashMap::new();
        for &u in members {
            *scc_counts.entry(fas.scc_id[u]).or_insert(0) += 1;
        }
        if scc_counts.values().any(|&c| c > 1) {
            return None;
        }
        let min_r = members.iter().map(|&u| ranks[u]).min()?;
        let max_r = members.iter().map(|&u| ranks[u]).max()?;
        let longest_path = (max_r - min_r) as usize;
        let span = longest_path + 1;
        if span < 5 {
            return None;
        }
        let mut rank_groups = vec![Vec::new(); span];
        for &u in members {
            rank_groups[(ranks[u] - min_r) as usize].push(u);
        }
        if rank_groups.iter().any(|g| g.len() > 2) {
            return None;
        }
        for g in &mut rank_groups {
            g.sort_by(|&a, &b| {
                order[a]
                    .cmp(&order[b])
                    .then_with(|| scene.nodes[a].id.cmp(&scene.nodes[b].id))
            });
        }

        // Detect consecutive rank pairs connected by multi-edge parallel bundles
        // so boustrophedon row turns never split parallel captions across rows.
        let mut pair_mult: HashMap<(usize, usize), usize> = HashMap::new();
        for e in &scene.edges {
            let (Some(su), Some(sv)) = (scene.index(&e.source.node), scene.index(&e.target.node))
            else {
                continue;
            };
            if !mset.contains(&su) || !mset.contains(&sv) || su == sv {
                continue;
            }
            let key = (su.min(sv), su.max(sv));
            *pair_mult.entry(key).or_insert(0) += 1;
        }
        let mut parallel_adj = vec![false; span];
        for ((u, v), cnt) in pair_mult {
            if cnt >= 2 {
                let ru = (ranks[u] - min_r) as usize;
                let rv = (ranks[v] - min_r) as usize;
                let (lo, hi) = (ru.min(rv), ru.max(rv));
                if hi == lo + 1 {
                    parallel_adj[lo] = true;
                }
            }
        }

        // Per-boundary row-turn cost. Folding between rank r and r + 1 is free
        // only at a clean single-node articulation neck (e.g. `join0 -> d1`);
        // turning inside a branch/join diamond, fan, or parallel bundle stacks
        // siblings in one fold cell and forces long detours around the turn.
        let mut cross_edge_count = vec![0usize; span];
        for e in &scene.edges {
            let (Some(su), Some(sv)) = (scene.index(&e.source.node), scene.index(&e.target.node))
            else {
                continue;
            };
            if !mset.contains(&su) || !mset.contains(&sv) || su == sv {
                continue;
            }
            let ru = (ranks[su] - min_r) as usize;
            let rv = (ranks[sv] - min_r) as usize;
            for c in &mut cross_edge_count[ru.min(rv)..ru.max(rv)] {
                *c += 1;
            }
        }
        let turn_cost: Vec<usize> = (0..span.saturating_sub(1))
            .map(|r| {
                let mut cost = 0usize;
                if parallel_adj[r] {
                    cost += 10;
                }
                if rank_groups[r].len() > 1
                    || rank_groups[r + 1].len() > 1
                    || cross_edge_count[r] > 1
                {
                    cost += 4;
                }
                cost
            })
            .collect();
        // (parallel bundle splits, summed turn cost) over all row turns.
        let tally = |row_of_r: &[usize]| -> (usize, usize) {
            let mut splits = 0usize;
            let mut cost = 0usize;
            for r in 0..span.saturating_sub(1) {
                if row_of_r[r] != row_of_r[r + 1] {
                    cost += turn_cost[r];
                    if parallel_adj[r] {
                        splits += 1;
                    }
                }
            }
            (splits, cost)
        };

        let build_modulo = |cw: usize| -> (Vec<usize>, Vec<usize>, usize) {
            let num_rows = span.div_ceil(cw);
            let mut row_of_r = vec![0usize; span];
            let mut col_of_r = vec![0usize; span];
            for r in 0..span {
                let row = r / cw;
                let off = r % cw;
                row_of_r[r] = row;
                col_of_r[r] = if row.is_multiple_of(2) {
                    off
                } else {
                    cw - 1 - off
                };
            }
            (row_of_r, col_of_r, num_rows)
        };

        let build_walk = |cw: usize| -> (Vec<usize>, Vec<usize>, usize) {
            let mut row_of_r = vec![0usize; span];
            let mut col_of_r = vec![0usize; span];
            let mut start = 0usize;
            let mut row = 0usize;
            let mut start_col = 0usize;
            let mut dir: isize = 1;
            while start < span {
                let max_slots = if dir > 0 {
                    cw - start_col
                } else {
                    start_col + 1
                };
                let mut len = max_slots.min(span - start);
                if len >= 2 && start + len < span && turn_cost[start + len - 1] > 0 {
                    // Step back to the cleanest earlier cut (articulation neck)
                    // that still leaves room for the next row.
                    let cur = turn_cost[start + len - 1];
                    let mut pick: Option<(usize, usize)> = None;
                    for new_len in (2..len).rev() {
                        let c = turn_cost[start + new_len - 1];
                        if c >= cur {
                            continue;
                        }
                        let new_end_col =
                            (start_col as isize + dir * (new_len as isize - 1)) as usize;
                        let next_max = if dir > 0 {
                            new_end_col + 1
                        } else {
                            cw - new_end_col
                        };
                        if next_max < 2 {
                            continue;
                        }
                        if pick.is_none_or(|(pc, _)| c < pc) {
                            pick = Some((c, new_len));
                        }
                        if c == 0 {
                            break;
                        }
                    }
                    if let Some((_, new_len)) = pick {
                        len = new_len;
                    }
                }
                for off in 0..len {
                    let r = start + off;
                    row_of_r[r] = row;
                    col_of_r[r] = (start_col as isize + dir * off as isize) as usize;
                }
                start_col = (start_col as isize + dir * (len as isize - 1)) as usize;
                dir = -dir;
                start += len;
                row += 1;
            }
            (row_of_r, col_of_r, row.max(1))
        };

        struct FoldChoice {
            splits: usize,
            score: f64,
            non_base: bool,
            row_of_r: Vec<usize>,
            col_of_r: Vec<usize>,
            num_rows: usize,
        }
        let base_w =
            ((members.len() as f64 * o.target_aspect_ratio).sqrt().ceil() as usize).clamp(3, 4);
        let mut widths = vec![3usize, 4usize];
        if span >= 10 && rank_groups.iter().any(|g| g.len() > 1) {
            widths.push(5usize);
        }
        let mut best_choice: Option<FoldChoice> = None;
        for cw in widths {
            for (walk, (row_of_r, col_of_r, num_rows)) in
                [(false, build_modulo(cw)), (true, build_walk(cw))]
            {
                let (splits, cost) = tally(&row_of_r);
                let est_ar = (cw as f64 * 150.0) / (num_rows as f64 * 96.0);
                let da = (est_ar - o.target_aspect_ratio).abs();
                // Irregular walks must buy a real reduction in turn cost.
                let score = cost as f64 + 10. * da + if walk { 2. } else { 0. };
                let non_base = cw != base_w;
                let replace = match &best_choice {
                    None => true,
                    Some(b) => {
                        splits < b.splits
                            || (splits == b.splits
                                && (score < b.score - 1e-9
                                    || ((score - b.score).abs() <= 1e-9
                                        && !non_base
                                        && b.non_base)))
                    }
                };
                if replace {
                    best_choice = Some(FoldChoice {
                        splits,
                        score,
                        non_base,
                        row_of_r,
                        col_of_r,
                        num_rows,
                    });
                }
            }
        }
        let FoldChoice {
            row_of_r,
            col_of_r,
            num_rows,
            ..
        } = best_choice?;

        let mut row_heights = vec![1usize; num_rows];
        for (r, g) in rank_groups.iter().enumerate() {
            let row = row_of_r[r];
            row_heights[row] = row_heights[row].max(g.len());
        }
        let mut row_base = vec![0usize; num_rows];
        for row in 1..num_rows {
            row_base[row] = row_base[row - 1] + row_heights[row - 1];
        }
        let mut out_ranks = ranks.to_vec();
        let mut out_order = order.to_vec();
        let mut fold_row = vec![0usize; scene.nodes.len()];
        let mut slot_of = vec![0usize; scene.nodes.len()];
        for (r, g) in rank_groups.iter().enumerate() {
            let row = row_of_r[r];
            let col = col_of_r[r];
            for (slot, &u) in g.iter().enumerate() {
                out_ranks[u] = col as i32;
                out_order[u] = (row_base[row] + slot) as i32;
                fold_row[u] = row;
                slot_of[u] = slot;
            }
        }
        // A vertical S -> N turn hint is only clean when it leaves the bottom-most
        // node of its fold cell and enters the top-most node of the next row's
        // cell; otherwise it would pass straight through a stacked sibling.
        let clean_turn = |su: usize, sv: usize| -> bool {
            let ru = (ranks[su] - min_r) as usize;
            slot_of[su] + 1 == rank_groups[ru].len() && slot_of[sv] == 0
        };
        let mut side_hints = vec![None; scene.edges.len()];
        for (ei, e) in scene.edges.iter().enumerate() {
            let (Some(su), Some(sv)) = (scene.index(&e.source.node), scene.index(&e.target.node))
            else {
                continue;
            };
            if !mset.contains(&su) || !mset.contains(&sv) || su == sv {
                continue;
            }
            let ru = (ranks[su] - min_r) as usize;
            let rv = (ranks[sv] - min_r) as usize;
            let row_u = fold_row[su];
            let row_v = fold_row[sv];
            if rv == ru + 1 {
                if row_u == row_v {
                    let (fwd_src, fwd_dst) = if out_ranks[sv] > out_ranks[su] {
                        (Side::E, Side::W)
                    } else {
                        (Side::W, Side::E)
                    };
                    // Branch/join diamonds: the top branch stays a straight
                    // facing edge; the stacked bottom branch leaves/enters
                    // through the single node's S vertex as a 1-bend L.
                    let (gu, gv) = (rank_groups[ru].len(), rank_groups[rv].len());
                    side_hints[ei] = Some(if gu == 1 && gv == 2 && slot_of[sv] == 1 {
                        (Side::S, fwd_dst)
                    } else if gu == 2 && gv == 1 && slot_of[su] == 1 {
                        (fwd_src, Side::S)
                    } else {
                        (fwd_src, fwd_dst)
                    });
                } else if row_v == row_u + 1 && clean_turn(su, sv) {
                    side_hints[ei] = Some((Side::S, Side::N));
                }
            } else if rv > ru + 1 && row_u == row_v {
                if row_u == 0 {
                    side_hints[ei] = Some((Side::N, Side::N));
                } else if row_u + 1 == num_rows {
                    side_hints[ei] = Some((Side::S, Side::S));
                }
            } else if rv > ru + 1
                && row_v == row_u + 1
                && out_ranks[su] == out_ranks[sv]
                && clean_turn(su, sv)
            {
                side_hints[ei] = Some((Side::S, Side::N));
            }
        }
        Some(FoldResult {
            ranks: out_ranks,
            order: out_order,
            fold_row,
            side_hints,
        })
    }

    fn connected_components(n: usize, scene: &SceneGraph) -> Vec<Vec<usize>> {
        let mut adj = vec![Vec::new(); n];
        for e in &scene.edges {
            if let (Some(u), Some(v)) = (scene.index(&e.source.node), scene.index(&e.target.node))
                && u != v
            {
                adj[u].push(v);
                adj[v].push(u);
            }
        }
        let mut vis = vec![false; n];
        let mut comps = Vec::new();
        for s in 0..n {
            if vis[s] {
                continue;
            }
            let mut c = Vec::new();
            let mut q = VecDeque::new();
            vis[s] = true;
            q.push_back(s);
            while let Some(u) = q.pop_front() {
                c.push(u);
                for &v in &adj[u] {
                    if !vis[v] {
                        vis[v] = true;
                        q.push_back(v);
                    }
                }
            }
            c.sort_by(|&a, &b| scene.nodes[a].id.cmp(&scene.nodes[b].id));
            comps.push(c);
        }
        comps
    }

    fn solve_axis_coords(
        nv: usize,
        lt_edges: &[(usize, usize)],
        eq_edges: &[(usize, usize)],
    ) -> Vec<i32> {
        let mut par: Vec<usize> = (0..nv).collect();
        let find = |par: &mut [usize], mut x: usize| -> usize {
            while par[x] != x {
                par[x] = par[par[x]];
                x = par[x];
            }
            x
        };
        let reaches = |a: usize, b: usize, p: &mut [usize]| -> bool {
            let ra = find(p, a);
            let rb = find(p, b);
            if ra == rb {
                return true;
            }
            let mut seen = vec![false; nv];
            let mut q = VecDeque::new();
            seen[ra] = true;
            q.push_back(ra);
            while let Some(x) = q.pop_front() {
                if x == rb {
                    return true;
                }
                for &(u, v) in lt_edges {
                    let ru = find(p, u);
                    let rv = find(p, v);
                    if ru == x && !seen[rv] {
                        seen[rv] = true;
                        q.push_back(rv);
                    }
                }
            }
            false
        };
        for &(a, b) in eq_edges {
            if !reaches(a, b, &mut par) && !reaches(b, a, &mut par) {
                let ra = find(&mut par, a);
                let rb = find(&mut par, b);
                if ra != rb {
                    par[rb] = ra;
                }
            }
        }
        let mut roots = Vec::new();
        let mut root_map = HashMap::new();
        for i in 0..nv {
            let r = find(&mut par, i);
            if let std::collections::hash_map::Entry::Vacant(e) = root_map.entry(r) {
                e.insert(roots.len());
                roots.push(r);
            }
        }
        let nr = roots.len();
        let mut dag_adj = vec![Vec::new(); nr];
        let mut in_deg = vec![0usize; nr];
        for &(u, v) in lt_edges {
            let cu = root_map[&find(&mut par, u)];
            let cv = root_map[&find(&mut par, v)];
            if cu != cv && !dag_adj[cu].contains(&cv) {
                dag_adj[cu].push(cv);
                in_deg[cv] += 1;
            }
        }
        let mut coords = vec![0i32; nr];
        let mut processed = vec![false; nr];
        let mut ready: BTreeSet<usize> = (0..nr).filter(|&i| in_deg[i] == 0).collect();
        let mut done = 0usize;
        while done < nr {
            let u = if let Some(next) = ready.pop_first() {
                next
            } else {
                (0..nr)
                    .filter(|&i| !processed[i])
                    .min_by_key(|&i| (in_deg[i], i))
                    .unwrap()
            };
            if processed[u] {
                continue;
            }
            processed[u] = true;
            done += 1;
            for &v in &dag_adj[u] {
                if !processed[v] {
                    coords[v] = coords[v].max(coords[u] + 1);
                    in_deg[v] = in_deg[v].saturating_sub(1);
                    if in_deg[v] == 0 {
                        ready.insert(v);
                    }
                }
            }
        }
        (0..nv)
            .map(|i| coords[root_map[&find(&mut par, i)]])
            .collect()
    }

    pub(super) fn domus_solve_component(
        scene: &SceneGraph,
        members: &[usize],
        budget: usize,
        ext_dirs: &[(usize, i32, i32)],
    ) -> (Option<DomusResult>, usize) {
        let m = members.len();
        if m < 3 || budget == 0 {
            return (None, 0);
        }
        let mut local = HashMap::with_capacity(m);
        for (li, &gi) in members.iter().enumerate() {
            local.insert(gi, li);
        }
        let mut uedges: BTreeSet<(usize, usize)> = BTreeSet::new();
        for e in &scene.edges {
            if let (Some(su), Some(sv)) = (scene.index(&e.source.node), scene.index(&e.target.node))
                && let (Some(&lu), Some(&lv)) = (local.get(&su), local.get(&sv))
                && lu != lv
            {
                uedges.insert((lu.min(lv), lu.max(lv)));
            }
        }
        let mut base_edges: Vec<(usize, usize)> = uedges.into_iter().collect();
        base_edges.sort_by(|&(a1, b1), &(a2, b2)| {
            (
                scene.nodes[members[a1]].id.as_str(),
                scene.nodes[members[b1]].id.as_str(),
            )
                .cmp(&(
                    scene.nodes[members[a2]].id.as_str(),
                    scene.nodes[members[b2]].id.as_str(),
                ))
        });
        if base_edges.len() < 3 {
            return (None, 0);
        }
        let mut adj = vec![Vec::new(); m];
        for (ei, &(u, v)) in base_edges.iter().enumerate() {
            adj[u].push((v, ei));
            adj[v].push((u, ei));
        }
        let root_start = ext_dirs
            .iter()
            .filter(|x| x.1 < 0)
            .map(|x| x.0)
            .min_by_key(|&u| scene.nodes[members[u]].id.as_str())
            .unwrap_or(0);
        let mut depth = vec![usize::MAX; m];
        let mut parent = vec![(usize::MAX, usize::MAX); m];
        let mut in_tree = vec![false; base_edges.len()];
        let mut bq = VecDeque::new();
        depth[root_start] = 0;
        bq.push_back(root_start);
        while let Some(u) = bq.pop_front() {
            for &(v, ei) in &adj[u] {
                if depth[v] == usize::MAX {
                    depth[v] = depth[u] + 1;
                    parent[v] = (u, ei);
                    in_tree[ei] = true;
                    bq.push_back(v);
                }
            }
        }
        let mut basis_cycles: Vec<Vec<(usize, bool)>> = Vec::new();
        for (ei, &(u0, v0)) in base_edges.iter().enumerate() {
            if in_tree[ei] || depth[u0] == usize::MAX || depth[v0] == usize::MAX {
                continue;
            }
            let mut left_steps = Vec::new();
            let mut right_steps = Vec::new();
            let mut a = u0;
            let mut b = v0;
            while depth[a] > depth[b] {
                let (p, pe) = parent[a];
                left_steps.push((pe, base_edges[pe].0 == a));
                a = p;
            }
            while depth[b] > depth[a] {
                let (p, pe) = parent[b];
                right_steps.push((pe, base_edges[pe].0 == p));
                b = p;
            }
            while a != b {
                let (pa, pea) = parent[a];
                left_steps.push((pea, base_edges[pea].0 == a));
                a = pa;
                let (pb, peb) = parent[b];
                right_steps.push((peb, base_edges[peb].0 == pb));
                b = pb;
            }
            let mut cycle = left_steps;
            right_steps.reverse();
            cycle.extend(right_steps);
            cycle.push((ei, base_edges[ei].0 == v0));
            if cycle.len() >= 3 {
                basis_cycles.push(cycle);
            }
        }
        if basis_cycles.is_empty() {
            return (None, 0);
        }
        basis_cycles.sort_by_key(Vec::len);

        let build_and_solve = |edges_spec: &[(usize, usize)],
                               cycles_spec: &[Vec<(usize, bool)>],
                               nv: usize,
                               require_4dir_all: bool,
                               use_port_mutex: bool,
                               use_cut_hints: bool|
         -> Option<Vec<u8>> {
            let ne = edges_spec.len();
            let vars = ne * 4;
            let v_dir = |ei: usize, d: u8| -> i32 { (ei * 4 + d as usize + 1) as i32 };
            let step_dir = |ei: usize, fwd: bool, d: u8| -> i32 {
                let actual = if fwd { d } else { d ^ 1 };
                v_dir(ei, actual)
            };
            let mut clauses: Vec<Vec<i32>> = Vec::new();
            for ei in 0..ne {
                clauses.push(vec![v_dir(ei, 0), v_dir(ei, 1), v_dir(ei, 2), v_dir(ei, 3)]);
                for d1 in 0..4u8 {
                    for d2 in (d1 + 1)..4u8 {
                        clauses.push(vec![-v_dir(ei, d1), -v_dir(ei, d2)]);
                    }
                }
            }
            for (ci, cyc) in cycles_spec.iter().enumerate() {
                let e_lits: Vec<i32> = cyc.iter().map(|&(ei, f)| step_dir(ei, f, 0)).collect();
                let w_lits: Vec<i32> = cyc.iter().map(|&(ei, f)| step_dir(ei, f, 1)).collect();
                let s_lits: Vec<i32> = cyc.iter().map(|&(ei, f)| step_dir(ei, f, 2)).collect();
                let n_lits: Vec<i32> = cyc.iter().map(|&(ei, f)| step_dir(ei, f, 3)).collect();
                for k in 0..cyc.len() {
                    let mut c1 = vec![-e_lits[k]];
                    c1.extend_from_slice(&w_lits);
                    clauses.push(c1);
                    let mut c2 = vec![-w_lits[k]];
                    c2.extend_from_slice(&e_lits);
                    clauses.push(c2);
                    let mut c3 = vec![-s_lits[k]];
                    c3.extend_from_slice(&n_lits);
                    clauses.push(c3);
                    let mut c4 = vec![-n_lits[k]];
                    c4.extend_from_slice(&s_lits);
                    clauses.push(c4);
                }
                let want_4 = if require_4dir_all {
                    true
                } else {
                    ci == 0 || cyc.len() >= 4
                };
                if want_4 && cyc.len() >= 4 {
                    clauses.push(e_lits);
                    clauses.push(w_lits);
                    clauses.push(s_lits);
                    clauses.push(n_lits);
                } else {
                    let mut all_h = Vec::with_capacity(cyc.len() * 2);
                    all_h.extend_from_slice(&e_lits);
                    all_h.extend_from_slice(&w_lits);
                    let mut all_v = Vec::with_capacity(cyc.len() * 2);
                    all_v.extend_from_slice(&s_lits);
                    all_v.extend_from_slice(&n_lits);
                    clauses.push(all_h);
                    clauses.push(all_v);
                }
                if cyc.len() == 4 {
                    for k in 0..4 {
                        let (e1, f1) = cyc[k];
                        let (e2, f2) = cyc[(k + 1) % 4];
                        for d in 0..4u8 {
                            clauses.push(vec![-step_dir(e1, f1, d), -step_dir(e2, f2, d ^ 1)]);
                        }
                    }
                }
            }
            if use_port_mutex {
                let mut inc: Vec<Vec<(usize, bool)>> = vec![Vec::new(); nv];
                for (ei, &(u, v)) in edges_spec.iter().enumerate() {
                    inc[u].push((ei, true));
                    inc[v].push((ei, false));
                }
                for list in &inc {
                    if (2..=4).contains(&list.len()) {
                        for i in 0..list.len() {
                            for j in (i + 1)..list.len() {
                                let (e1, f1) = list[i];
                                let (e2, f2) = list[j];
                                for d in 0..4u8 {
                                    clauses.push(vec![-step_dir(e1, f1, d), -step_dir(e2, f2, d)]);
                                }
                            }
                        }
                    }
                }
            }
            if use_cut_hints {
                let mut inc: Vec<Vec<(usize, bool)>> = vec![Vec::new(); nv];
                for (ei, &(u, v)) in edges_spec.iter().enumerate() {
                    inc[u].push((ei, true));
                    inc[v].push((ei, false));
                }
                for &(u, dx, dy) in ext_dirs {
                    if u < nv {
                        let ban_dir = if dx.abs() >= dy.abs() {
                            if dx < 0 {
                                Some(1u8)
                            } else if dx > 0 {
                                Some(0u8)
                            } else {
                                None
                            }
                        } else if dy < 0 {
                            Some(3u8)
                        } else if dy > 0 {
                            Some(2u8)
                        } else {
                            None
                        };
                        if let Some(bd) = ban_dir {
                            for &(ei, fwd) in &inc[u] {
                                clauses.push(vec![-step_dir(ei, fwd, bd)]);
                            }
                        }
                    }
                }
            }
            if let Ok(Some(model)) = satisfy(vars, &clauses) {
                let mut dirs = vec![0u8; ne];
                for (ei, slot) in dirs.iter_mut().enumerate() {
                    for d in 0..4u8 {
                        if sat_var(&model, vars, v_dir(ei, d) as usize) {
                            *slot = d;
                            break;
                        }
                    }
                }
                Some(dirs)
            } else {
                None
            }
        };

        type DomusSolved = (Vec<u8>, Vec<(usize, usize)>, usize);
        let mut searched = 0usize;
        let mut solved: Option<DomusSolved> = None;
        let attempts = [(true, true, true), (true, true, false)];
        for &(req4, port_m, hints) in &attempts {
            if searched >= budget {
                break;
            }
            searched += 1;
            if let Some(dirs) = build_and_solve(&base_edges, &basis_cycles, m, req4, port_m, hints)
            {
                solved = Some((dirs, base_edges.clone(), m));
                break;
            }
        }
        if solved.is_none() && searched < budget {
            let split_edge_idx = basis_cycles[0][0].0;
            let (su, sv) = base_edges[split_edge_idx];
            let bend_v = m;
            let mut sub_edges = base_edges.clone();
            let e_new = sub_edges.len();
            sub_edges[split_edge_idx] = (su, bend_v);
            sub_edges.push((bend_v, sv));
            let mut sub_cycles = Vec::with_capacity(basis_cycles.len());
            for cyc in &basis_cycles {
                let mut nc = Vec::with_capacity(cyc.len() + 1);
                for &(ei, fwd) in cyc {
                    if ei == split_edge_idx {
                        if fwd {
                            nc.push((split_edge_idx, true));
                            nc.push((e_new, true));
                        } else {
                            nc.push((e_new, false));
                            nc.push((split_edge_idx, false));
                        }
                    } else {
                        nc.push((ei, fwd));
                    }
                }
                sub_cycles.push(nc);
            }
            let sub_attempts = [
                (true, true, true),
                (true, true, false),
                (false, true, false),
                (false, false, false),
            ];
            for &(req4, port_m, hints) in &sub_attempts {
                if searched >= budget {
                    break;
                }
                searched += 1;
                if let Some(dirs) =
                    build_and_solve(&sub_edges, &sub_cycles, m + 1, req4, port_m, hints)
                {
                    solved = Some((dirs, sub_edges, m + 1));
                    break;
                }
            }
        }
        let Some((dirs, edges_used, nv)) = solved else {
            return (None, searched);
        };
        let mut x_lt = Vec::new();
        let mut y_lt = Vec::new();
        let mut x_eq = Vec::new();
        let mut y_eq = Vec::new();
        for (ei, &(u, v)) in edges_used.iter().enumerate() {
            match dirs[ei] {
                0 => {
                    x_lt.push((u, v));
                    y_eq.push((u, v));
                }
                1 => {
                    x_lt.push((v, u));
                    y_eq.push((u, v));
                }
                2 => {
                    y_lt.push((u, v));
                    x_eq.push((u, v));
                }
                _ => {
                    y_lt.push((v, u));
                    x_eq.push((u, v));
                }
            }
        }
        let gx_all = solve_axis_coords(nv, &x_lt, &x_eq);
        let gy_all = solve_axis_coords(nv, &y_lt, &y_eq);
        let mut gx: Vec<i32> = gx_all[..m].to_vec();
        let mut gy: Vec<i32> = gy_all[..m].to_vec();
        let mut occ: HashSet<(i32, i32)> = HashSet::with_capacity(m);
        for i in 0..m {
            let mut p = (gx[i], gy[i]);
            if occ.contains(&p) {
                let mut radius = 1i32;
                'find_free: loop {
                    for dy in -radius..=radius {
                        for dx in -radius..=radius {
                            if dx.abs() != radius && dy.abs() != radius {
                                continue;
                            }
                            let cand = (gx[i] + dx, gy[i] + dy);
                            if !occ.contains(&cand) {
                                p = cand;
                                break 'find_free;
                            }
                        }
                    }
                    radius += 1;
                }
                gx[i] = p.0;
                gy[i] = p.1;
            }
            occ.insert(p);
        }
        if gy.iter().all(|&y| y == gy[0]) && m >= 3 {
            gy[m - 1] += 1;
        }
        let min_x = *gx.iter().min().unwrap_or(&0);
        let min_y = *gy.iter().min().unwrap_or(&0);
        for x in &mut gx {
            *x -= min_x;
        }
        for y in &mut gy {
            *y -= min_y;
        }
        let max_x = *gx.iter().max().unwrap_or(&0);
        let max_y = *gy.iter().max().unwrap_or(&0);
        if !ext_dirs.is_empty() {
            let flips = [(false, false), (true, false), (false, true), (true, true)];
            let mut best_flip = (gx.clone(), gy.clone());
            let mut best_align = i64::MIN;
            for &(fx, fy) in &flips {
                let cx: Vec<i32> = gx.iter().map(|&x| if fx { max_x - x } else { x }).collect();
                let cy: Vec<i32> = gy.iter().map(|&y| if fy { max_y - y } else { y }).collect();
                let mut score = 0i64;
                for &(u, dx, dy) in ext_dirs {
                    if u < m {
                        score += i64::from(dx) * i64::from(cx[u] * 2 - max_x)
                            + i64::from(dy) * i64::from(cy[u] * 2 - max_y);
                    }
                }
                if score > best_align {
                    best_align = score;
                    best_flip = (cx, cy);
                }
            }
            gx = best_flip.0;
            gy = best_flip.1;
        }
        let mut edge_hints = Vec::new();
        for (ei, e) in scene.edges.iter().enumerate() {
            if let (Some(su), Some(sv)) = (scene.index(&e.source.node), scene.index(&e.target.node))
                && let (Some(&lu), Some(&lv)) = (local.get(&su), local.get(&sv))
                && lu != lv
            {
                let dx = gx[lv] - gx[lu];
                let dy = gy[lv] - gy[lu];
                let (ss, ts) = if dx.abs() >= dy.abs() {
                    if dx >= 0 {
                        (Side::E, Side::W)
                    } else {
                        (Side::W, Side::E)
                    }
                } else if dy >= 0 {
                    (Side::S, Side::N)
                } else {
                    (Side::N, Side::S)
                };
                edge_hints.push((ei, ss, ts));
            }
        }
        (Some(DomusResult { gx, gy, edge_hints }), searched)
    }

    pub(super) fn rects_non_overlapping(
        members: &[usize],
        rects: &[Rect],
        scene: &SceneGraph,
        pad: f64,
    ) -> bool {
        let inflate = |r: Rect| Rect::new(r.x, r.y, r.w + 2.0 * pad, r.h + 2.0 * pad);
        let mset: HashSet<usize> = members.iter().copied().collect();
        let parent = scene.nodes[members[0]].parent.as_deref();
        for (i, &u) in members.iter().enumerate() {
            let ru = rects[u];
            if !ru.x.is_finite() || !ru.y.is_finite() {
                return false;
            }
            let pru = inflate(ru);
            for &v in members.iter().skip(i + 1) {
                if pru.intersects(inflate(rects[v])) {
                    return false;
                }
            }
            for (k, node) in scene.nodes.iter().enumerate() {
                if !mset.contains(&k)
                    && node.parent.as_deref() == parent
                    && !node.container
                    && pru.intersects(inflate(rects[k]))
                {
                    return false;
                }
            }
        }
        true
    }

    fn owner_grouped_ranks(scene: &SceneGraph) -> Vec<i32> {
        let n = scene.nodes.len();
        if n == 0 {
            return Vec::new();
        }
        if !scene.nodes.iter().any(|nd| nd.container) {
            let fas = eades_lin_smyth_fas(scene);
            return gansner_ranks(scene, n, &fas);
        }
        let mut root_of = vec![0usize; n];
        for (i, slot) in root_of.iter_mut().enumerate() {
            let mut cur = i;
            let mut steps = 0usize;
            while let Some(pid) = scene.nodes[cur].parent.as_deref()
                && steps < n
                && let Some(pidx) = scene.index(pid)
            {
                cur = pidx;
                steps += 1;
            }
            *slot = cur;
        }
        let mut macro_roots: Vec<usize> = Vec::new();
        let mut root_to_macro: HashMap<usize, usize> = HashMap::new();
        for (i, &r) in root_of.iter().enumerate() {
            if (scene.nodes[i].parent.is_none() || r == i)
                && let std::collections::hash_map::Entry::Vacant(e) = root_to_macro.entry(r)
            {
                let mi = macro_roots.len();
                e.insert(mi);
                macro_roots.push(r);
            }
        }
        let m = macro_roots.len();
        let mut macro_nodes = Vec::with_capacity(m);
        for &r in &macro_roots {
            let mut nd = scene.nodes[r].clone();
            nd.parent = None;
            macro_nodes.push(nd);
        }
        let mut macro_edges = Vec::new();
        for e in &scene.edges {
            if let (Some(su), Some(sv)) = (scene.index(&e.source.node), scene.index(&e.target.node))
            {
                let mu = root_to_macro[&root_of[su]];
                let mv = root_to_macro[&root_of[sv]];
                if mu != mv {
                    let mut me = e.clone();
                    me.source.node = macro_nodes[mu].id.clone();
                    me.target.node = macro_nodes[mv].id.clone();
                    macro_edges.push(me);
                }
            }
        }
        let mut macro_scene = SceneGraph {
            nodes: macro_nodes,
            edges: macro_edges,
            ..Default::default()
        };
        macro_scene.node_index.clear();
        let macro_fas = eades_lin_smyth_fas(&macro_scene);
        let macro_r = gansner_ranks(&macro_scene, m, &macro_fas);

        let mut by_mr: BTreeMap<i32, Vec<usize>> = BTreeMap::new();
        for (mi, &mr) in macro_r.iter().enumerate() {
            by_mr.entry(mr).or_default().push(mi);
        }
        let mut ranks = vec![0i32; n];
        let mut cursor = 0i32;
        for (_, mut units) in by_mr {
            units.sort_by(|&a, &b| {
                scene.nodes[macro_roots[a]]
                    .id
                    .cmp(&scene.nodes[macro_roots[b]].id)
            });
            let mut any_leaf = false;
            for &mi in &units {
                let r = macro_roots[mi];
                let has_kids = (0..n).any(|u| u != r && root_of[u] == r);
                if !has_kids {
                    ranks[r] = cursor;
                    any_leaf = true;
                }
            }
            if any_leaf {
                cursor += 1;
            }
            for &mi in &units {
                let r = macro_roots[mi];
                let r_id = scene.nodes[r].id.as_str();
                let children: Vec<usize> = (0..n).filter(|&u| u != r && root_of[u] == r).collect();
                if children.is_empty() {
                    continue;
                }
                let child_set: HashSet<usize> = children.iter().copied().collect();
                let mut sub_nodes = Vec::with_capacity(children.len());
                for &u in &children {
                    let mut nd = scene.nodes[u].clone();
                    if nd.parent.as_deref() == Some(r_id) {
                        nd.parent = None;
                    }
                    sub_nodes.push(nd);
                }
                let mut sub_edges = Vec::new();
                for e in &scene.edges {
                    if let (Some(su), Some(sv)) =
                        (scene.index(&e.source.node), scene.index(&e.target.node))
                        && child_set.contains(&su)
                        && child_set.contains(&sv)
                    {
                        sub_edges.push(e.clone());
                    }
                }
                let mut sub_scene = SceneGraph {
                    nodes: sub_nodes,
                    edges: sub_edges,
                    ..Default::default()
                };
                sub_scene.node_index.clear();
                let sub_ranks = owner_grouped_ranks(&sub_scene);
                let min_s = *sub_ranks.iter().min().unwrap_or(&0);
                let max_s = *sub_ranks.iter().max().unwrap_or(&0);
                for (li, &u) in children.iter().enumerate() {
                    ranks[u] = cursor + (sub_ranks[li] - min_s);
                }
                ranks[r] = cursor;
                cursor += (max_s - min_s + 1).max(1);
            }
        }
        ranks
    }

    fn build_canonical_ltr(
        scene: &SceneGraph,
        _sizes: &[Point],
        options: &LayoutOptions,
        fas: &FasResult,
        unfolded_only: bool,
    ) -> TopologyPlan {
        let n = scene.nodes.len();
        if n == 0 {
            return TopologyPlan {
                axis: FlowAxis::LeftToRight,
                ranks: Vec::new(),
                order: Vec::new(),
                fold_row: Vec::new(),
                edge_guides: BTreeMap::new(),
                side_hints: Vec::new(),
                search_states: 0,
            };
        }
        let mut ranks = owner_grouped_ranks(scene);
        let mut side_hints = vec![None; scene.edges.len()];
        let mut states = 1usize;
        let mut domus_overrides: Vec<(Vec<usize>, Vec<i32>)> = Vec::new();

        let sccs = components(scene);
        let comps = connected_components(n, scene);
        for comp in &comps {
            let mset: HashSet<usize> = comp.iter().copied().collect();
            let mut sccs_in_comp: Vec<usize> = Vec::new();
            for &u in comp {
                let cid = fas.scc_id[u];
                if !sccs_in_comp.contains(&cid) {
                    sccs_in_comp.push(cid);
                }
            }
            for cid in sccs_in_comp {
                let scc = &sccs[cid];
                if scc.len() >= 3
                    && scc.len() <= 12
                    && scc.len() == comp.len()
                    && options.shape_budget > 0
                {
                    let mut internal_e = 0usize;
                    for &(u, v, _, _) in &fas.dag_edges {
                        if fas.scc_id[u] == cid && fas.scc_id[v] == cid {
                            internal_e += 1;
                        }
                    }
                    if internal_e <= scc.len() + 2 {
                        let mut local = HashMap::new();
                        for (li, &gi) in scc.iter().enumerate() {
                            local.insert(gi, li);
                        }
                        let mut ext = Vec::new();
                        for e in &scene.edges {
                            if let (Some(su), Some(sv)) =
                                (scene.index(&e.source.node), scene.index(&e.target.node))
                            {
                                if !mset.contains(&su) && local.contains_key(&sv) {
                                    ext.push((local[&sv], -1, 0));
                                } else if local.contains_key(&su) && !mset.contains(&sv) {
                                    ext.push((local[&su], 1, 0));
                                }
                            }
                        }
                        let (domus_opt, used) =
                            domus_solve_component(scene, scc, options.shape_budget.min(64), &ext);
                        states += used;
                        if let Some(res) = domus_opt
                            && res.gy.iter().any(|&y| y > 0)
                        {
                            for (li, &gi) in scc.iter().enumerate() {
                                ranks[gi] = res.gx[li];
                            }
                            for (ei, ss, ts) in res.edge_hints {
                                side_hints[ei] = Some((ss, ts));
                            }
                            domus_overrides.push((scc.clone(), res.gy));
                        }
                    }
                }
            }
        }

        let mut bk = brandes_kopf_order_and_guides(scene, &ranks);
        for (scc, gy) in domus_overrides {
            for (li, &gi) in scc.iter().enumerate() {
                bk.order[gi] = gy[li];
            }
        }
        let mut fold_row = vec![0usize; n];
        if !unfolded_only {
            let min_fold_len = if comps.len() > 1 { 7 } else { 5 };
            for comp in &comps {
                if comp.len() < min_fold_len {
                    continue;
                }
                if let Some(folded) =
                    try_serpentine_fold(scene, comp, &ranks, &bk.order, fas, options)
                {
                    for &u in comp {
                        ranks[u] = folded.ranks[u];
                        bk.order[u] = folded.order[u];
                        fold_row[u] = folded.fold_row[u];
                    }
                    for (ei, hint) in folded.side_hints.into_iter().enumerate() {
                        if let Some(h) = hint {
                            side_hints[ei] = Some(h);
                        }
                    }
                    let cset: HashSet<usize> = comp.iter().copied().collect();
                    bk.edge_guides.retain(|eid, _| {
                        scene
                            .edge(eid)
                            .and_then(|e| scene.index(&e.source.node))
                            .is_none_or(|su| !cset.contains(&su))
                    });
                }
            }
        }

        TopologyPlan {
            axis: FlowAxis::LeftToRight,
            ranks,
            order: bk.order,
            fold_row,
            edge_guides: bk.edge_guides,
            side_hints,
            search_states: states,
        }
    }

    fn should_emit_top_to_bottom(scene: &SceneGraph, sizes: &[Point], ranks: &[i32]) -> bool {
        let n = scene.nodes.len();
        if n < 3 {
            return false;
        }
        let mut in_deg = vec![0usize; n];
        let mut out_deg = vec![0usize; n];
        let mut ns_ports = 0usize;
        let mut ew_ports = 0usize;
        for e in &scene.edges {
            for p in [&e.source.constraint, &e.target.constraint] {
                if let Some(side) = p.side() {
                    match side {
                        Side::N | Side::S => ns_ports += 1,
                        Side::E | Side::W => ew_ports += 1,
                    }
                }
            }
            if let (Some(su), Some(sv)) = (scene.index(&e.source.node), scene.index(&e.target.node))
                && su != sv
            {
                out_deg[su] += 1;
                in_deg[sv] += 1;
            }
        }
        if ns_ports > ew_ports && ns_ports >= 2 {
            return true;
        }
        let min_r = *ranks.iter().min().unwrap_or(&0);
        let max_r = *ranks.iter().max().unwrap_or(&0);
        let depth = (max_r - min_r) as usize;
        let branching = out_deg.iter().filter(|&&d| d >= 2).count();
        let is_multilevel_tree = in_deg.iter().all(|&d| d <= 1) && depth >= 2 && branching >= 2;
        if is_multilevel_tree {
            return true;
        }
        let num_ranks = depth + 1;
        if (2..=4).contains(&num_ranks) {
            let mut col_w = vec![0.0f64; num_ranks];
            for (u, &r) in ranks.iter().enumerate() {
                let c = (r - min_r) as usize;
                let w = sizes.get(u).map_or(112.0, |p| p.x);
                col_w[c] = col_w[c].max(w);
            }
            let mut gap_w = vec![56.0f64; num_ranks - 1];
            let mut hop_cap_w = vec![0.0f64; num_ranks - 1];
            let mut max_cap_w = 0.0f64;
            for e in &scene.edges {
                if let (Some(su), Some(sv)) =
                    (scene.index(&e.source.node), scene.index(&e.target.node))
                    && su != sv
                {
                    let cap = connection_text(scene, e);
                    if !cap.is_empty() {
                        let cw = text_width(&cap);
                        max_cap_w = max_cap_w.max(cw);
                        let c0 = (ranks[su] - min_r) as usize;
                        let c1 = (ranks[sv] - min_r) as usize;
                        let (lo, hi) = (c0.min(c1), c0.max(c1));
                        if hi == lo + 1 {
                            hop_cap_w[lo] = hop_cap_w[lo].max(cw);
                            gap_w[lo] = gap_w[lo].max(cw + 40.0);
                        }
                    }
                }
            }
            let sum_hop_cap: f64 = hop_cap_w.iter().sum();
            let node_sum: f64 = col_w.iter().sum();
            let lr_span: f64 = node_sum + gap_w.iter().sum::<f64>();
            if max_cap_w >= 180.0 || sum_hop_cap >= 260.0 || lr_span > 900.0 {
                return true;
            }
        }
        false
    }

    fn try_quadrant_cross(
        scene: &SceneGraph,
        ranks: &[i32],
        crossings: usize,
    ) -> Option<TopologyPlan> {
        if crossings == 0 || scene.nodes.len() < 5 {
            return None;
        }
        let n = scene.nodes.len();
        let min_r = *ranks.iter().min().unwrap_or(&0);
        let max_r = *ranks.iter().max().unwrap_or(&0);
        if max_r - min_r != 1 {
            return None;
        }
        let mut u_nodes: Vec<usize> = (0..n).filter(|&i| ranks[i] == min_r).collect();
        let mut v_nodes: Vec<usize> = (0..n).filter(|&i| ranks[i] == max_r).collect();
        let dense = (u_nodes.len() >= 2 && v_nodes.len() >= 3)
            || (u_nodes.len() >= 3 && v_nodes.len() >= 2);
        // Large or nested bipartite meshes force long quadrant detours and
        // bypass container packing; keep the cross only for small flat graphs.
        if !dense
            || u_nodes.len() > 4
            || v_nodes.len() > 4
            || scene
                .nodes
                .iter()
                .any(|nd| nd.container || nd.parent.is_some())
        {
            return None;
        }
        let mut deg = vec![0usize; n];
        for e in &scene.edges {
            if let (Some(su), Some(sv)) = (scene.index(&e.source.node), scene.index(&e.target.node))
                && su != sv
            {
                if (ranks[su] == min_r && ranks[sv] == min_r)
                    || (ranks[su] == max_r && ranks[sv] == max_r)
                {
                    return None;
                }
                deg[su] += 1;
                deg[sv] += 1;
            }
        }
        u_nodes.sort_by(|&a, &b| {
            deg[b]
                .cmp(&deg[a])
                .then_with(|| scene.nodes[a].id.cmp(&scene.nodes[b].id))
        });
        v_nodes.sort_by(|&a, &b| {
            deg[b]
                .cmp(&deg[a])
                .then_with(|| scene.nodes[a].id.cmp(&scene.nodes[b].id))
        });
        let mut q_ranks = vec![0i32; n];
        let mut q_order = vec![0i32; n];
        let u_left = u_nodes.len() / 2;
        for (i, &u) in u_nodes.iter().enumerate() {
            q_ranks[u] = if i < u_left {
                -(u_left as i32) + i as i32
            } else {
                (i - u_left + 1) as i32
            };
            q_order[u] = 0;
        }
        let v_top = v_nodes.len() / 2;
        for (i, &v) in v_nodes.iter().enumerate() {
            q_ranks[v] = 0;
            q_order[v] = if i < v_top {
                -(v_top as i32) + i as i32
            } else {
                (i - v_top + 1) as i32
            };
        }
        let mut side_hints = vec![None; scene.edges.len()];
        for (ei, e) in scene.edges.iter().enumerate() {
            if let (Some(su), Some(sv)) = (scene.index(&e.source.node), scene.index(&e.target.node))
                && su != sv
            {
                let (u_node, v_node, u_is_src) = if ranks[su] == min_r && ranks[sv] == max_r {
                    (su, sv, true)
                } else if ranks[sv] == min_r && ranks[su] == max_r {
                    (sv, su, false)
                } else {
                    continue;
                };
                let u_side = if q_order[v_node] < 0 {
                    Side::N
                } else {
                    Side::S
                };
                let v_side = if q_ranks[u_node] < 0 {
                    Side::W
                } else {
                    Side::E
                };
                side_hints[ei] = Some(if u_is_src {
                    (u_side, v_side)
                } else {
                    (v_side, u_side)
                });
            }
        }
        Some(TopologyPlan {
            axis: FlowAxis::QuadrantCross,
            ranks: q_ranks,
            order: q_order,
            fold_row: vec![0; n],
            edge_guides: BTreeMap::new(),
            side_hints,
            search_states: 1,
        })
    }

    fn try_hub_ns_mount(scene: &SceneGraph, canonical: &TopologyPlan) -> Option<TopologyPlan> {
        let n = scene.nodes.len();
        if n < 4
            || canonical.fold_row.iter().any(|&r| r > 0)
            || scene
                .nodes
                .iter()
                .any(|nd| nd.container || nd.parent.is_some())
        {
            return None;
        }
        let mut adj_ranks: Vec<BTreeSet<i32>> = vec![BTreeSet::new(); n];
        let mut deg = vec![0usize; n];
        let mut pair_mult: HashMap<(usize, usize), usize> = HashMap::new();
        for e in &scene.edges {
            if let (Some(su), Some(sv)) = (scene.index(&e.source.node), scene.index(&e.target.node))
                && su != sv
            {
                deg[su] += 1;
                deg[sv] += 1;
                adj_ranks[su].insert(canonical.ranks[sv]);
                adj_ranks[sv].insert(canonical.ranks[su]);
                *pair_mult.entry((su.min(sv), su.max(sv))).or_insert(0) += 1;
            }
        }
        let hub = (0..n)
            .filter(|&u| deg[u] >= 3 && adj_ranks[u].len() >= 3)
            .max_by(|&a, &b| {
                (deg[a], adj_ranks[a].len())
                    .cmp(&(deg[b], adj_ranks[b].len()))
                    .then_with(|| scene.nodes[b].id.cmp(&scene.nodes[a].id))
            })?;
        let mut side_hints = canonical.side_hints.clone();
        let mut any_changed = false;
        for (ei, e) in scene.edges.iter().enumerate() {
            if let (Some(su), Some(sv)) = (scene.index(&e.source.node), scene.index(&e.target.node))
                && (su == hub || sv == hub)
                && su != sv
                && pair_mult
                    .get(&(su.min(sv), su.max(sv)))
                    .copied()
                    .unwrap_or(0)
                    <= 1
            {
                let rank_gap = (canonical.ranks[sv] - canonical.ranks[su]).abs();
                if rank_gap >= 2 {
                    let other = if su == hub { sv } else { su };
                    let ns = if canonical.order[other] < canonical.order[hub] {
                        Side::N
                    } else {
                        Side::S
                    };
                    let other_side = if canonical.ranks[other] < canonical.ranks[hub] {
                        Side::E
                    } else {
                        Side::W
                    };
                    side_hints[ei] = Some(if su == hub {
                        (ns, other_side)
                    } else {
                        (other_side, ns)
                    });
                    any_changed = true;
                }
            }
        }
        if !any_changed {
            return None;
        }
        let mut alt = canonical.clone();
        alt.side_hints = side_hints;
        alt.search_states = alt.search_states.saturating_add(1);
        Some(alt)
    }

    pub(super) fn generate_candidates(
        scene: &SceneGraph,
        sizes: &[Point],
        options: &LayoutOptions,
    ) -> Vec<TopologyPlan> {
        let fas = eades_lin_smyth_fas(scene);
        let raw_ranks = gansner_ranks(scene, scene.nodes.len(), &fas);
        let raw_bk = brandes_kopf_order_and_guides(scene, &raw_ranks);
        let plan0 = build_canonical_ltr(scene, sizes, options, &fas, false);
        let mut out = vec![plan0.clone()];
        if !options.explore_candidates {
            return out;
        }
        let folded = plan0.fold_row.iter().any(|&r| r > 0);
        // Flat raw-rank candidates bypass owner-grouped ranks and container
        // packing, so containers only explore owner-aware LeftToRight plans.
        let has_containers = scene.nodes.iter().any(|nd| nd.container);
        if !has_containers && !folded && should_emit_top_to_bottom(scene, sizes, &raw_ranks) {
            out.push(TopologyPlan {
                axis: FlowAxis::TopToBottom,
                ranks: raw_ranks.clone(),
                order: raw_bk.order.clone(),
                fold_row: vec![0; scene.nodes.len()],
                edge_guides: raw_bk.edge_guides.clone(),
                side_hints: vec![None; scene.edges.len()],
                search_states: plan0.search_states,
            });
        }
        if !has_containers && let Some(qc) = try_quadrant_cross(scene, &raw_ranks, raw_bk.crossings)
        {
            out.push(qc);
        }
        if folded {
            out.push(build_canonical_ltr(scene, sizes, options, &fas, true));
        } else if let Some(hub_alt) = try_hub_ns_mount(scene, &plan0) {
            out.push(hub_alt);
        }
        out.truncate(4);
        out
    }

    pub(super) fn apply_readable_serpentine(
        scene: &SceneGraph,
        rects: &mut [Rect],
        o: &LayoutOptions,
        place_nodes: bool,
    ) -> Option<SideHints> {
        let n = scene.nodes.len();
        if n < 7
            || scene
                .edges
                .iter()
                .enumerate()
                .any(|(i, _)| edge_is_constrained(scene, i, o))
        {
            return None;
        }
        let comps = connected_components(n, scene);
        if comps.len() != 1 {
            return None;
        }
        let fas = eades_lin_smyth_fas(scene);
        let ranks = gansner_ranks(scene, n, &fas);
        let bk = brandes_kopf_order_and_guides(scene, &ranks);
        let folded = try_serpentine_fold(scene, &comps[0], &ranks, &bk.order, &fas, o)?;
        let cols: Vec<usize> = folded.ranks.iter().map(|&r| r.max(0) as usize).collect();
        let rows: Vec<usize> = folded.order.iter().map(|&r| r.max(0) as usize).collect();
        let num_cols = cols.iter().copied().max().unwrap_or(0) + 1;
        let num_rows = rows.iter().copied().max().unwrap_or(0) + 1;
        if place_nodes {
            let cap_w = scene
                .edges
                .iter()
                .map(|e| text_width(&connection_text(scene, e)))
                .fold(0.0f64, f64::max);
            let cap_h = scene
                .edges
                .iter()
                .map(|e| text_height(&connection_text(scene, e)))
                .fold(0.0f64, f64::max);
            let mut col_w = vec![0.0f64; num_cols];
            let mut row_h = vec![0.0f64; num_rows];
            for u in 0..n {
                col_w[cols[u]] = col_w[cols[u]].max(rects[u].w);
                row_h[rows[u]] = row_h[rows[u]].max(rects[u].h);
            }
            let x_gap = (cap_w + 16.0).max(40.0).max(2.0 * o.stub + 8.0);
            let y_gap = (cap_h + 24.0).max(40.0).max(2.0 * o.stub + 8.0);
            let mut col_x = vec![0.0f64; num_cols];
            for c in 1..num_cols {
                col_x[c] = col_x[c - 1] + (col_w[c - 1] + col_w[c]) * 0.5 + x_gap;
            }
            let mut row_y = vec![0.0f64; num_rows];
            for r in 1..num_rows {
                row_y[r] = row_y[r - 1] + (row_h[r - 1] + row_h[r]) * 0.5 + y_gap;
            }
            for u in 0..n {
                rects[u].x = col_x[cols[u]];
                rects[u].y = row_y[rows[u]];
            }
        } else {
            for (ei, hint) in folded.side_hints.iter().enumerate() {
                if let Some((ss, _)) = hint
                    && let (Some(su), Some(sv)) = (
                        scene.index(&scene.edges[ei].source.node),
                        scene.index(&scene.edges[ei].target.node),
                    )
                {
                    let valid = match ss {
                        Side::E => rects[su].right() + 2.0 * o.stub < rects[sv].left(),
                        Side::W => rects[sv].right() + 2.0 * o.stub < rects[su].left(),
                        Side::S => rects[su].bottom() + 2.0 * o.stub < rects[sv].top(),
                        Side::N => {
                            rects[su].right() + 2.0 * o.stub < rects[sv].left()
                                || rects[sv].right() + 2.0 * o.stub < rects[su].left()
                        }
                    };
                    if !valid {
                        return None;
                    }
                }
            }
        }
        Some(folded.side_hints)
    }
}

pub(crate) type SideHints = Vec<Option<(Side, Side)>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlowAxis {
    LeftToRight,
    TopToBottom,
    QuadrantCross,
}

#[derive(Debug, Clone)]
pub(crate) struct TopologyPlan {
    pub axis: FlowAxis,
    pub ranks: Vec<i32>,
    pub order: Vec<i32>,
    pub fold_row: Vec<usize>,
    pub edge_guides: BTreeMap<String, Vec<(i32, f64)>>,
    pub side_hints: SideHints,
    pub search_states: usize,
}

pub(crate) fn candidates(
    scene: &SceneGraph,
    sizes: &[Point],
    options: &LayoutOptions,
) -> Vec<TopologyPlan> {
    stage1a::generate_candidates(scene, sizes, options)
}
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
    stage1a::apply_readable_serpentine(scene, rects, o, place_nodes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn scene_of(src: &str) -> SceneGraph {
        crate::compile(src).expect("valid test diagram").scene
    }

    #[test]
    fn fas_and_gansner_respect_source_pull_forward_and_decl_order() {
        let s_fwd = scene_of("s0 -> s1 -> s2 -> s3 -> s4 -> s5 -> s6 -> s7\ns0 -> s3\ns2 -> s6\n");
        let s_rev = scene_of(
            "s7\ns2\ns5\ns0\ns3\ns1\ns6\ns4\ns0 -> s1 -> s2 -> s3 -> s4 -> s5 -> s6 -> s7\ns0 -> s3\ns2 -> s6\n",
        );
        let o = LayoutOptions::default();
        let sz_fwd = vec![Point::new(112.0, 52.0); s_fwd.nodes.len()];
        let sz_rev = vec![Point::new(112.0, 52.0); s_rev.nodes.len()];
        let c_fwd = candidates(&s_fwd, &sz_fwd, &o);
        let c_rev = candidates(&s_rev, &sz_rev, &o);
        assert!(!c_fwd.is_empty());
        assert_eq!(c_fwd[0].fold_row.iter().copied().max(), Some(2));
        for id in ["s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7"] {
            let uf = s_fwd.index(id).unwrap();
            let ur = s_rev.index(id).unwrap();
            assert_eq!(
                c_fwd[0].ranks[uf], c_rev[0].ranks[ur],
                "rank mismatch on {id}"
            );
            assert_eq!(
                c_fwd[0].order[uf], c_rev[0].order[ur],
                "order mismatch on {id}"
            );
            assert_eq!(
                c_fwd[0].fold_row[uf], c_rev[0].fold_row[ur],
                "fold_row mismatch on {id}"
            );
        }

        // Unconditional source pull-forward without any satellites
        let pull = scene_of("a -> b -> c -> d\nhelper -> d\n");
        let sz_p = vec![Point::new(112.0, 52.0); pull.nodes.len()];
        let cp = candidates(&pull, &sz_p, &o);
        let helper = pull.index("helper").unwrap();
        let d = pull.index("d").unwrap();
        assert_eq!(cp[0].ranks[helper] + 1, cp[0].ranks[d]);
    }

    #[test]
    fn transitive_reduction_and_intra_scc_tie_in_coffman_graham() {
        let s = scene_of("a -> b -> c -> a\nb -> c\na -> c\n");
        let r = coffman_graham(&s, 4);
        assert_eq!(r.len(), 3);
        let a = s.index("a").unwrap();
        let b = s.index("b").unwrap();
        let c = s.index("c").unwrap();
        assert!(r[b] < r[c] || r[a] < r[b]);

        // Pure DAG with redundant transitive shortcut a -> d
        let dag = scene_of("a -> b -> c -> d\na -> d\na -> c\n");
        let dr = coffman_graham(&dag, 4);
        for e in &dag.edges {
            let u = dag.index(&e.source.node).unwrap();
            let v = dag.index(&e.target.node).unwrap();
            assert!(dr[u] < dr[v]);
        }
    }

    #[test]
    fn brandes_kopf_records_edge_guides_for_multi_rank_edges() {
        let s = scene_of("a -> b -> c -> d\na -> d\n");
        let o = LayoutOptions {
            explore_candidates: false,
            ..LayoutOptions::default()
        };
        let sz = vec![Point::new(112.0, 52.0); s.nodes.len()];
        let c = candidates(&s, &sz, &o);
        assert_eq!(c.len(), 1);
        let skip = s
            .edges
            .iter()
            .find(|e| e.source.node == "a" && e.target.node == "d")
            .unwrap();
        let guides = c[0]
            .edge_guides
            .get(&skip.id)
            .expect("expected edge_guides on skip");
        assert_eq!(guides.len(), 2);
    }

    #[test]
    fn domus_4compass_solves_cycles_bends_and_tala_ring_queue() {
        let o = LayoutOptions::default();
        // 4-node cyclic ring becomes a 2D rectangle
        let ring = scene_of("a -> b -> c -> d -> a\n");
        let mut rects = vec![
            Rect::new(0.0, 0.0, 112.0, 52.0),
            Rect::new(200.0, 0.0, 112.0, 52.0),
            Rect::new(400.0, 0.0, 112.0, 52.0),
            Rect::new(600.0, 0.0, 112.0, 52.0),
        ];
        let st = optimize(&ring, &mut rects, &o);
        assert!(st.after <= st.before);
        assert_eq!(st.optimized_components, 1);
        assert!(st.searched_states > 0 && st.searched_states <= o.shape_budget);
        assert!(rects.iter().any(|r| r.y != 0.0));

        // 3-node dense cycle requires virtual bend subdivision and succeeds within budget
        let tri = scene_of("a -> b -> c -> a\n");
        let mut tri_rects = vec![
            Rect::new(0.0, 0.0, 112.0, 52.0),
            Rect::new(200.0, 0.0, 112.0, 52.0),
            Rect::new(400.0, 0.0, 112.0, 52.0),
        ];
        let st3 = optimize(&tri, &mut tri_rects, &o);
        assert!(st3.after <= st3.before);
        assert!(st3.searched_states > 0);
        assert!(tri_rects.iter().any(|r| r.y != 0.0));

        // tala_ring_queue couples external cut edges
        let tq = scene_of(
            "ingress -> ring_a -> ring_b -> ring_c -> ring_d -> ring_a\nring_b -> dlq\nring_d -> worker\n",
        );
        let sz = vec![Point::new(112.0, 52.0); tq.nodes.len()];
        let cp = candidates(&tq, &sz, &o);
        let ra = tq.index("ring_a").unwrap();
        let rb = tq.index("ring_b").unwrap();
        let rc = tq.index("ring_c").unwrap();
        let rd = tq.index("ring_d").unwrap();
        let orders: BTreeSet<i32> = [ra, rb, rc, rd]
            .into_iter()
            .map(|i| cp[0].order[i])
            .collect();
        assert!(orders.len() >= 2, "ring should span 2 rows: {orders:?}");
    }

    #[test]
    fn candidates_top_to_bottom_and_quadrant_cross_behave_structurally() {
        let o_explore = LayoutOptions {
            explore_candidates: true,
            ..LayoutOptions::default()
        };
        let o_no = LayoutOptions {
            explore_candidates: false,
            ..LayoutOptions::default()
        };

        // Multi-level tree emits TopToBottom; 1-hop star does NOT
        let tree = scene_of("r -> a\nr -> b\na -> a1\na -> a2\nb -> b1\nb -> b2\n");
        let c_tree = candidates(
            &tree,
            &vec![Point::new(112.0, 52.0); tree.nodes.len()],
            &o_explore,
        );
        assert!(c_tree.iter().any(|p| p.axis == FlowAxis::TopToBottom));
        let c_tree_no = candidates(
            &tree,
            &vec![Point::new(112.0, 52.0); tree.nodes.len()],
            &o_no,
        );
        assert_eq!(c_tree_no.len(), 1);

        let star = scene_of("hub -> l0\nhub -> l1\nhub -> l2\nhub -> l3\nhub -> l4\n");
        let c_star = candidates(
            &star,
            &vec![Point::new(112.0, 52.0); star.nodes.len()],
            &o_explore,
        );
        assert!(!c_star.iter().any(|p| p.axis == FlowAxis::TopToBottom));

        // K3,3 emits QuadrantCross with centered axes and L-corridor side_hints
        let k33 = scene_of(
            "u0 -> v0\nu0 -> v1\nu0 -> v2\nu1 -> v0\nu1 -> v1\nu1 -> v2\nu2 -> v0\nu2 -> v1\nu2 -> v2\n",
        );
        let c_k33 = candidates(
            &k33,
            &vec![Point::new(112.0, 52.0); k33.nodes.len()],
            &o_explore,
        );
        let qc = c_k33
            .iter()
            .find(|p| p.axis == FlowAxis::QuadrantCross)
            .expect("K33 must emit QuadrantCross");
        assert!(qc.side_hints.iter().all(|h| h.is_some()));
        assert!(qc.ranks.iter().any(|&r| r < 0) && qc.ranks.iter().any(|&r| r > 0));
        assert!(qc.order.iter().any(|&y| y < 0) && qc.order.iter().any(|&y| y > 0));
    }

    #[test]
    fn check_long_chain_readable_concept() {
        let src = "s0 -> s1: data\ns1 -> s2: data\ns2 -> s3: data\ns3 -> s4: data\ns4 -> s5: data\ns5 -> s6: data\ns6 -> s7: data\n";
        let s = scene_of(src);
        let o = LayoutOptions::default();
        let mut rects = vec![Rect::new(0.0, 0.0, 112.0, 52.0); s.nodes.len()];
        let hints = readable_concept(&s, &mut rects, &o, true);
        assert!(
            hints.is_some(),
            "expected readable_concept Some for 8-node chain"
        );
        let min_x = rects.iter().map(|r| r.left()).fold(f64::INFINITY, f64::min);
        let max_x = rects
            .iter()
            .map(|r| r.right())
            .fold(f64::NEG_INFINITY, f64::max);
        let min_y = rects.iter().map(|r| r.top()).fold(f64::INFINITY, f64::min);
        let max_y = rects
            .iter()
            .map(|r| r.bottom())
            .fold(f64::NEG_INFINITY, f64::max);
        let doc = crate::compile(src).unwrap();
        let lx0 = doc
            .layout
            .nodes
            .values()
            .map(|r| r.left())
            .fold(f64::INFINITY, f64::min);
        let lx1 = doc
            .layout
            .nodes
            .values()
            .map(|r| r.right())
            .fold(f64::NEG_INFINITY, f64::max);
        let ly0 = doc
            .layout
            .nodes
            .values()
            .map(|r| r.top())
            .fold(f64::INFINITY, f64::min);
        let ly1 = doc
            .layout
            .nodes
            .values()
            .map(|r| r.bottom())
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(max_x > min_x && max_y > min_y);
        assert!(
            (lx1 - lx0) / (ly1 - ly0).max(1.0) < 2.5,
            "folded 8-node chain aspect ratio should be < 2.5, got {:.2}",
            (lx1 - lx0) / (ly1 - ly0).max(1.0)
        );
    }

    #[test]
    fn serpentine_folds_five_stage_bypass_and_keeps_parallel_bundles_on_same_row() {
        let bypass_src =
            "n0 -> n1 -> n2 -> n3 -> n4\nn2 -> n4: a\nn2 -> n4: b\nn3 -> n4: c\nn1 -> n4: ack\n";
        let s_bypass = crate::model::lower(&crate::syntax::parse(bypass_src));
        let o = LayoutOptions::default();
        let sz_bypass = vec![Point::new(112.0, 52.0); s_bypass.nodes.len()];
        let c_bypass = candidates(&s_bypass, &sz_bypass, &o);
        assert!(
            c_bypass.len() >= 2,
            "folded 5-stage bypass should emit folded + unfolded candidates"
        );
        let folded = &c_bypass[0];
        let n1 = s_bypass.index("n1").unwrap();
        let n3 = s_bypass.index("n3").unwrap();
        let n4 = s_bypass.index("n4").unwrap();
        assert_eq!(folded.fold_row[n1], 0);
        assert_eq!(folded.fold_row[n4], 1);
        assert_eq!(folded.fold_row[n3], folded.fold_row[n4]);
        assert_eq!(folded.ranks[n1], folded.ranks[n4]);
        let bypass_ei = s_bypass
            .edges
            .iter()
            .position(|e| e.source.node == "n1" && e.target.node == "n4")
            .unwrap();
        assert_eq!(folded.side_hints[bypass_ei], Some((Side::S, Side::N)));
        assert!(c_bypass[1].fold_row.iter().all(|&r| r == 0));

        let par_src = include_str!("../tests/fixtures/quality/c0ffee77-parallel.diagram");
        let s_par = crate::model::lower(&crate::syntax::parse(par_src));
        let sz_par = vec![Point::new(112.0, 52.0); s_par.nodes.len()];
        let c_par = candidates(&s_par, &sz_par, &o);
        let n8 = s_par.index("n8").unwrap();
        let n9 = s_par.index("n9").unwrap();
        assert_eq!(
            c_par[0].fold_row[n8], c_par[0].fold_row[n9],
            "parallel bundle n8->n9 must stay on the same fold row"
        );
        assert_ne!(c_par[0].ranks[n8], c_par[0].ranks[n9]);

        let dual_split_src =
            "n0 -> n1 -> n2 -> n3 -> n4 -> n5 -> n6 -> n7 -> n8 -> n9\nn2 -> n3: b\nn7 -> n8: c\n";
        let s_dual = crate::model::lower(&crate::syntax::parse(dual_split_src));
        let sz_dual = vec![Point::new(112.0, 52.0); s_dual.nodes.len()];
        let c_dual = candidates(&s_dual, &sz_dual, &o);
        let d2 = s_dual.index("n2").unwrap();
        let d3 = s_dual.index("n3").unwrap();
        let d7 = s_dual.index("n7").unwrap();
        let d8 = s_dual.index("n8").unwrap();
        assert_eq!(c_dual[0].fold_row[d2], c_dual[0].fold_row[d3]);
        assert_eq!(c_dual[0].fold_row[d7], c_dual[0].fold_row[d8]);
    }

    #[test]
    fn long_caption_short_chain_emits_top_to_bottom_without_triggering_on_short_loops() {
        let o = LayoutOptions::default();
        let long_src = include_str!("../tests/fixtures/quality/14-long-labels.diagram");
        let s_long = crate::model::lower(&crate::syntax::parse(long_src));
        let sz_long = vec![Point::new(220.0, 68.0); s_long.nodes.len()];
        let c_long = candidates(&s_long, &sz_long, &o);
        assert!(
            c_long.iter().any(|p| p.axis == FlowAxis::TopToBottom),
            "short chain with long edge captions must emit TopToBottom candidate"
        );

        let retry_src = "api -> queue: enqueue\nqueue -> worker: consume\nworker -> db: persist\nworker -> queue: retry\n";
        let s_retry = crate::model::lower(&crate::syntax::parse(retry_src));
        let sz_retry = vec![Point::new(112.0, 52.0); s_retry.nodes.len()];
        let c_retry = candidates(&s_retry, &sz_retry, &o);
        assert!(
            !c_retry.iter().any(|p| p.axis == FlowAxis::TopToBottom),
            "short-caption 4-stage retry chain should stay LeftToRight"
        );
    }

    #[test]
    fn container_grouped_ranks_keep_raft_bundle_adjacent_and_compact() {
        let raft_src = "cli: Admin Console [shape: actor]\n\
            cp: Control Plane {\n\
              leader: Raft Leader\n\
              follower: Raft Follower\n\
              store: WAL Store [shape: database]\n\
              leader -> follower: appendEntries\n\
              leader -> follower: installSnapshot\n\
              follower -> leader: voteGranted\n\
              leader -> leader: leaseRenew\n\
              leader -> store: fsync\n\
            }\n\
            dp: Data Plane {\n\
              envoy: Proxy Mesh\n\
              metrics: StatsD [shape: queue]\n\
              envoy <-> metrics: xDS / telemetry\n\
            }\n\
            cli -> cp.leader: apply\n\
            cp.leader -> dp.envoy: push-config\n\
            dp.metrics -> cp.follower: health\n";
        let s_raft = crate::model::lower(&crate::syntax::parse(raft_src));
        let o = LayoutOptions::default();
        let sz_raft = vec![Point::new(112.0, 52.0); s_raft.nodes.len()];
        let c_raft = candidates(&s_raft, &sz_raft, &o);
        let leader = s_raft.index("cp.leader").unwrap();
        let follower = s_raft.index("cp.follower").unwrap();
        let metrics = s_raft.index("dp.metrics").unwrap();
        assert_eq!(
            (c_raft[0].ranks[follower] - c_raft[0].ranks[leader]).abs(),
            1,
            "intra-container parallel peers must keep adjacent ranks"
        );
        assert!(c_raft[0].ranks[metrics] > c_raft[0].ranks[follower]);

        let doc = crate::compile(raft_src).unwrap();
        let bounds = crate::render::bounds(&doc.scene, &doc.layout);
        assert!(
            bounds.w < 1150.0 && bounds.h < 650.0,
            "expected compact container bounds, got {bounds:?}"
        );
    }

    #[test]
    fn multi_component_graphs_require_seven_nodes_to_fold() {
        let src = include_str!("../tests/fixtures/quality/12-independent-systems.diagram");
        let s = crate::model::lower(&crate::syntax::parse(src));
        let o = LayoutOptions::default();
        let sz = vec![Point::new(112.0, 52.0); s.nodes.len()];
        let c = candidates(&s, &sz, &o);
        assert!(
            c[0].fold_row.iter().all(|&r| r == 0),
            "5-node component in a multi-component graph must stay unfolded"
        );
    }
}
