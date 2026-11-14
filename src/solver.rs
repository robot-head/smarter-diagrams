//! Weighted active-set projection (VPSC + primal-dual KKT) and bounded L-BFGS.
use crate::Error;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const COEFF_ZERO_EPS: f64 = 1e-12;
const EMPTY_ROW_TOL: f64 = 1e-6;
const FEAS_TOL: f64 = 1e-5;
const ACTIVATE_TOL: f64 = 1e-9;
const PIN_WEIGHT_THRESHOLD: f64 = 1e4;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Weights {
    pub pin: f64,
    pub crossing: f64,
    pub flip: f64,
    pub bend: f64,
    pub pill: f64,
    pub backward: f64,
    pub length: f64,
    pub centeredness: f64,
    pub source_order: f64,
    pub symmetry: f64,
}
impl Default for Weights {
    fn default() -> Self {
        Self {
            pin: 1e5,
            crossing: 2000.,
            flip: 450.,
            bend: 120.,
            pill: 60.,
            backward: 80.,
            length: 1.5,
            centeredness: 4.,
            source_order: 0.25,
            symmetry: 0.1,
        }
    }
}

/// Sparse linear constraint: `sum(coefficients[i] * x[i]) >= rhs`, or equality.
#[derive(Debug, Clone)]
pub struct Constraint {
    pub coefficients: Vec<(usize, f64)>,
    pub rhs: f64,
    pub equality: bool,
}
impl Constraint {
    pub fn separation(left: usize, right: usize, gap: f64) -> Self {
        Self {
            coefficients: vec![(left, -1.), (right, 1.)],
            rhs: gap,
            equality: false,
        }
    }
    pub fn fixed(index: usize, position: f64) -> Self {
        Self {
            coefficients: vec![(index, 1.)],
            rhs: position,
            equality: true,
        }
    }
    pub fn equality(a: usize, b: usize) -> Self {
        Self {
            coefficients: vec![(a, 1.), (b, -1.)],
            rhs: 0.,
            equality: true,
        }
    }
}

/// Sparse L1 compaction term: `weight * |sum(coefficients[i] * x[i]) - target|`.
#[derive(Debug, Clone)]
pub struct L1Term {
    pub coefficients: Vec<(usize, f64)>,
    pub target: f64,
    pub weight: f64,
}
impl L1Term {
    pub fn new(coefficients: Vec<(usize, f64)>, target: f64, weight: f64) -> Self {
        Self {
            coefficients,
            target,
            weight,
        }
    }
    pub fn pair(left: usize, right: usize, weight: f64) -> Self {
        Self {
            coefficients: vec![(left, -1.), (right, 1.)],
            target: 0.,
            weight,
        }
    }
}

#[derive(Debug, Clone)]
struct CanonicalRow {
    coeffs: Vec<(usize, f64)>,
    rhs: f64,
    equality: bool,
}
impl CanonicalRow {
    fn eval(&self, x: &[f64]) -> f64 {
        self.coeffs.iter().map(|&(i, a)| a * x[i]).sum()
    }
    fn inv_w_norm_sq(&self, inv_w: &[f64]) -> f64 {
        self.coeffs.iter().map(|&(i, a)| a * a * inv_w[i]).sum()
    }
    fn dot_inv_w(&self, other: &Self, inv_w: &[f64]) -> f64 {
        let (a, b) = (&self.coeffs, &other.coeffs);
        let (mut ia, mut ib) = (0, 0);
        let mut sum = 0.0;
        while ia < a.len() && ib < b.len() {
            match a[ia].0.cmp(&b[ib].0) {
                std::cmp::Ordering::Less => ia += 1,
                std::cmp::Ordering::Greater => ib += 1,
                std::cmp::Ordering::Equal => {
                    sum += a[ia].1 * b[ib].1 * inv_w[a[ia].0];
                    ia += 1;
                    ib += 1;
                }
            }
        }
        sum
    }
    /// Returns `(u, v, gap)` representing `x[v] - x[u] >=/= gap` if standard VPSC.
    fn as_vpsc(&self, anchor: usize) -> Option<(usize, usize, f64)> {
        match self.coeffs.as_slice() {
            [(i, a)] => {
                if *a > 0. {
                    Some((anchor, *i, self.rhs))
                } else {
                    Some((*i, anchor, self.rhs))
                }
            }
            [(i, a), (j, b)] if (a + b).abs() < COEFF_ZERO_EPS => {
                if *a > 0. {
                    Some((*j, *i, self.rhs))
                } else {
                    Some((*i, *j, self.rhs))
                }
            }
            _ => None,
        }
    }
}

fn coalesce_coeffs(raw: &[(usize, f64)]) -> Vec<(usize, f64)> {
    let mut sorted = raw.to_vec();
    sorted.sort_unstable_by_key(|&(i, _)| i);
    let mut out: Vec<(usize, f64)> = Vec::with_capacity(sorted.len());
    for (i, a) in sorted {
        if let Some(last) = out.last_mut()
            && last.0 == i
        {
            last.1 += a;
        } else {
            out.push((i, a));
        }
    }
    out.retain(|&(_, a)| a.abs() >= COEFF_ZERO_EPS);
    out
}

fn canonicalize_constraints(
    n: usize,
    constraints: &[Constraint],
) -> Result<Vec<CanonicalRow>, Error> {
    let mut rows = Vec::with_capacity(constraints.len());
    for c in constraints {
        if !c.rhs.is_finite()
            || c.coefficients
                .iter()
                .any(|&(i, a)| i >= n || !a.is_finite())
        {
            return Err(Error::Layout("invalid projection input".into()));
        }
        let mut coeffs = coalesce_coeffs(&c.coefficients);
        if coeffs.is_empty() {
            let feasible = if c.equality {
                c.rhs.abs() <= EMPTY_ROW_TOL
            } else {
                c.rhs <= EMPTY_ROW_TOL
            };
            if !feasible {
                return Err(Error::Layout("inconsistent constant constraint".into()));
            }
            continue;
        }
        let scale = coeffs.iter().map(|(_, a)| a.abs()).fold(0.0_f64, f64::max);
        for (_, a) in &mut coeffs {
            *a /= scale;
        }
        let rhs = c.rhs / scale;
        if !rhs.is_finite() {
            return Err(Error::Layout(
                "constraint scaling exceeds finite range".into(),
            ));
        }
        rows.push(CanonicalRow {
            coeffs,
            rhs,
            equality: c.equality,
        });
    }
    Ok(rows)
}

fn validate_objective(desired: &[f64], weights: &[f64]) -> Result<(), Error> {
    if weights.len() != desired.len()
        || desired.iter().any(|x| !x.is_finite())
        || weights.iter().any(|w| !w.is_finite() || *w <= 0.)
    {
        return Err(Error::Layout("invalid projection input".into()));
    }
    Ok(())
}

/// Minimize `sum w_i (x_i - desired_i)^2` subject to linear constraints.
/// Uses exact Dwyer-Marriott-Stuckey VPSC blocks on separation components and
/// warm-started Goldfarb-Idnani active-set LDLT on components with general linear rows.
pub fn project(
    desired: &[f64],
    weights: &[f64],
    constraints: &[Constraint],
) -> Result<Vec<f64>, Error> {
    let n = desired.len();
    validate_objective(desired, weights)?;
    let rows = canonicalize_constraints(n, constraints)?;
    if n == 0 || rows.is_empty() {
        return Ok(desired.to_vec());
    }

    // Partition variables and constraints into independent connected components.
    let mut parent: Vec<usize> = (0..n).collect();
    let find_root = |parent: &mut [usize], mut x: usize| -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    };
    for r in &rows {
        let first = find_root(&mut parent, r.coeffs[0].0);
        for &(v, _) in &r.coeffs[1..] {
            let rv = find_root(&mut parent, v);
            if rv != first {
                // Keep smallest index as representative for deterministic ordering.
                if first < rv {
                    parent[rv] = first;
                } else {
                    parent[first] = rv;
                }
            }
        }
    }

    let mut comp_vars: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..n {
        let r = find_root(&mut parent, i);
        comp_vars.entry(r).or_default().push(i);
    }
    let mut comp_rows: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (ri, r) in rows.iter().enumerate() {
        let root = find_root(&mut parent, r.coeffs[0].0);
        comp_rows.entry(root).or_default().push(ri);
    }

    let mut out = desired.to_vec();
    let mut local_idx = vec![0usize; n];
    for (root, vars) in comp_vars {
        let Some(row_ids) = comp_rows.get(&root) else {
            continue;
        };
        let k = vars.len();
        for (li, &gi) in vars.iter().enumerate() {
            local_idx[gi] = li;
        }
        let local_d: Vec<f64> = vars.iter().map(|&gi| desired[gi]).collect();
        let local_w: Vec<f64> = vars.iter().map(|&gi| weights[gi]).collect();
        let local_rows: Vec<CanonicalRow> = row_ids
            .iter()
            .map(|&ri| CanonicalRow {
                coeffs: rows[ri]
                    .coeffs
                    .iter()
                    .map(|&(gi, a)| (local_idx[gi], a))
                    .collect(),
                rhs: rows[ri].rhs,
                equality: rows[ri].equality,
            })
            .collect();

        let all_vpsc = local_rows.iter().all(|r| r.as_vpsc(k).is_some());
        let sol = if all_vpsc {
            solve_vpsc_component(&local_d, &local_w, &local_rows)?
        } else {
            solve_kkt_component(&local_d, &local_w, &local_rows)?
        };
        for (li, &gi) in vars.iter().enumerate() {
            out[gi] = sol[li];
        }
    }
    Ok(out)
}

/// Minimize `0.5 * sum w_i (x_i - desired_i)^2 + sum linear_i * x_i` subject to constraints.
pub fn project_with_linear(
    desired: &[f64],
    weights: &[f64],
    constraints: &[Constraint],
    linear: &[f64],
) -> Result<Vec<f64>, Error> {
    let n = desired.len();
    validate_objective(desired, weights)?;
    if !linear.is_empty() && (linear.len() != n || linear.iter().any(|v| !v.is_finite())) {
        return Err(Error::Layout("invalid linear objective input".into()));
    }
    if linear.is_empty() {
        return project(desired, weights, constraints);
    }
    let shifted: Vec<f64> = (0..n)
        .map(|i| desired[i] - linear[i] / weights[i])
        .collect();
    project(&shifted, weights, constraints)
}

/// Minimize quadratic target distance plus linear terms (e.g. one-sided wall spans
/// `lambda_w * (u_hi - u_lo)`) and L1 compaction terms `lambda_k * |a_k^T u - b_k|`.
/// L1 terms use epigraph variables `z_k >= 0`, `-z_k <= a_k^T u - b_k <= z_k` with
/// quadratic regularization `1e-6 * z_k^2` and linear cost `lambda_k * z_k`.
pub fn project_compacted(
    desired: &[f64],
    weights: &[f64],
    constraints: &[Constraint],
    linear: &[f64],
    l1_terms: &[L1Term],
) -> Result<Vec<f64>, Error> {
    let n = desired.len();
    validate_objective(desired, weights)?;
    if !linear.is_empty() && (linear.len() != n || linear.iter().any(|v| !v.is_finite())) {
        return Err(Error::Layout("invalid linear objective input".into()));
    }
    for term in l1_terms {
        if !term.target.is_finite()
            || !term.weight.is_finite()
            || term.weight < 0.
            || term
                .coefficients
                .iter()
                .any(|&(i, a)| i >= n || !a.is_finite())
        {
            return Err(Error::Layout("invalid L1 compaction term".into()));
        }
    }

    let mut ext_desired = desired.to_vec();
    if !linear.is_empty() {
        for i in 0..n {
            ext_desired[i] -= linear[i] / weights[i];
        }
    }
    let mut ext_weights = weights.to_vec();
    let mut ext_constraints = constraints.to_vec();

    const EPI_REG_W: f64 = 1e-6;
    for term in l1_terms {
        if term.weight <= COEFF_ZERO_EPS {
            continue;
        }
        let coeffs = coalesce_coeffs(&term.coefficients);
        if coeffs.is_empty() {
            continue;
        }
        let z = ext_desired.len();
        ext_desired.push(-term.weight / EPI_REG_W);
        ext_weights.push(EPI_REG_W);

        ext_constraints.push(Constraint {
            coefficients: vec![(z, 1.)],
            rhs: 0.,
            equality: false,
        });
        let mut pos_coeffs = coeffs.clone();
        pos_coeffs.push((z, 1.));
        ext_constraints.push(Constraint {
            coefficients: pos_coeffs,
            rhs: term.target,
            equality: false,
        });
        let mut neg_coeffs: Vec<(usize, f64)> = coeffs.into_iter().map(|(i, a)| (i, -a)).collect();
        neg_coeffs.push((z, 1.));
        ext_constraints.push(Constraint {
            coefficients: neg_coeffs,
            rhs: -term.target,
            equality: false,
        });
    }

    let solution = project(&ext_desired, &ext_weights, &ext_constraints)?;
    Ok(solution[..n].to_vec())
}

// ---------- VPSC Active-Set Block Solver (Dwyer-Marriott-Stuckey + Dual Feasibility) ----------

#[derive(Debug, Clone, Copy)]
struct VpscIneq {
    u: usize,
    v: usize,
    gap: f64,
}

#[derive(Debug, Clone)]
struct ActiveEdge {
    ineq_idx: usize,
    u: usize,
    v: usize,
    gap: f64,
    lm: f64,
    alive: bool,
}

fn uf_find(parent: &mut [usize], rel: &mut [f64], x: usize) -> (usize, f64) {
    if parent[x] == x {
        return (x, 0.0);
    }
    let p = parent[x];
    let (root, p_off) = uf_find(parent, rel, p);
    parent[x] = root;
    rel[x] += p_off;
    (root, rel[x])
}

fn solve_vpsc_component(
    desired: &[f64],
    weights: &[f64],
    rows: &[CanonicalRow],
) -> Result<Vec<f64>, Error> {
    let n = desired.len();
    let anchor_orig = n;
    let mut uf_parent: Vec<usize> = (0..=n).collect();
    let mut uf_rel = vec![0.0_f64; n + 1];
    let mut uf_size = vec![1usize; n + 1];
    let mut raw_ineqs = Vec::new();

    for r in rows {
        let (u, v, gap) = r
            .as_vpsc(anchor_orig)
            .ok_or_else(|| Error::Layout("non-VPSC row in VPSC solver".into()))?;
        if r.equality {
            let (ru, du) = uf_find(&mut uf_parent, &mut uf_rel, u);
            let (rv, dv) = uf_find(&mut uf_parent, &mut uf_rel, v);
            if ru == rv {
                if ((dv - du) - gap).abs() > FEAS_TOL {
                    return Err(Error::Layout("contradictory geometric constraints".into()));
                }
            } else if rv == anchor_orig || (ru != anchor_orig && uf_size[ru] < uf_size[rv]) {
                uf_parent[ru] = rv;
                uf_rel[ru] = dv - gap - du;
                uf_size[rv] += uf_size[ru];
            } else {
                uf_parent[rv] = ru;
                uf_rel[rv] = du + gap - dv;
                uf_size[ru] += uf_size[rv];
            }
        } else {
            raw_ineqs.push(VpscIneq { u, v, gap });
        }
    }

    for i in 0..=n {
        uf_find(&mut uf_parent, &mut uf_rel, i);
    }
    let anchor_root = uf_parent[anchor_orig];
    let anchor_off = uf_rel[anchor_orig];

    let mut root_to_super = vec![usize::MAX; n + 1];
    let mut super_roots = Vec::new();
    for &r in uf_parent.iter().take(n) {
        if r != anchor_root && root_to_super[r] == usize::MAX {
            root_to_super[r] = super_roots.len();
            super_roots.push(r);
        }
    }
    let k = super_roots.len();
    let anchor = k;
    root_to_super[anchor_root] = anchor;

    let mut node_super = vec![0usize; n + 1];
    let mut node_off = vec![0.0_f64; n + 1];
    let mut comp_w = vec![0.0_f64; k + 1];
    let mut comp_d_num = vec![0.0_f64; k];
    for i in 0..n {
        let r = uf_parent[i];
        let s = root_to_super[r];
        node_super[i] = s;
        if s == anchor {
            node_off[i] = uf_rel[i] - anchor_off;
        } else {
            let off = uf_rel[i];
            node_off[i] = off;
            comp_w[s] += weights[i];
            comp_d_num[s] += weights[i] * (desired[i] - off);
        }
    }
    node_super[anchor_orig] = anchor;
    node_off[anchor_orig] = 0.0;
    let comp_d: Vec<f64> = (0..k)
        .map(|s| comp_d_num[s] / comp_w[s])
        .chain(std::iter::once(0.0))
        .collect();

    // Reduce inequalities onto super-variables, deduplicating parallel edges deterministically.
    let mut edge_map: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    for ineq in raw_ineqs {
        let su = node_super[ineq.u];
        let sv = node_super[ineq.v];
        let g_eff = ineq.gap - node_off[ineq.v] + node_off[ineq.u];
        if su == sv {
            if g_eff > FEAS_TOL {
                return Err(Error::Layout("infeasible separation constraints".into()));
            }
            continue;
        }
        edge_map
            .entry((su, sv))
            .and_modify(|g| *g = g.max(g_eff))
            .or_insert(g_eff);
    }
    for (&(su, sv), &g_uv) in &edge_map {
        if let Some(&g_vu) = edge_map.get(&(sv, su))
            && g_uv + g_vu > FEAS_TOL
        {
            return Err(Error::Layout("infeasible separation constraints".into()));
        }
    }
    let ineqs: Vec<VpscIneq> = edge_map
        .into_iter()
        .map(|((u, v), gap)| VpscIneq { u, v, gap })
        .collect();

    let num_nodes = k + 1;
    let mut x = comp_d.clone();
    if k == 0 || ineqs.is_empty() {
        return Ok((0..n).map(|i| x[node_super[i]] + node_off[i]).collect());
    }

    let mut active_edges: Vec<ActiveEdge> = Vec::with_capacity(k);
    let mut free_slots: Vec<usize> = Vec::new();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); num_nodes];

    // Reusable DFS/scratch buffers.
    let mut visited = vec![0u64; num_nodes];
    let mut stamp = 0u64;
    let mut parent_node = vec![usize::MAX; num_nodes];
    let mut parent_edge = vec![usize::MAX; num_nodes];
    let mut order_u = Vec::with_capacity(num_nodes);
    let mut order_v = Vec::with_capacity(num_nodes);
    let mut stack = Vec::with_capacity(num_nodes);
    let mut sub_w = vec![0.0_f64; num_nodes];
    let mut offset = vec![0.0_f64; num_nodes];
    let mut f_sub = vec![0.0_f64; num_nodes];
    let mut updates_u: Vec<(usize, f64)> = Vec::new();
    let mut updates_v: Vec<(usize, f64)> = Vec::new();
    let mut path_updates: Vec<(usize, f64)> = Vec::new();

    let dfs_component = |start: usize,
                         adj: &[Vec<usize>],
                         active_edges: &[ActiveEdge],
                         visited: &mut [u64],
                         stamp: &mut u64,
                         parent_node: &mut [usize],
                         parent_edge: &mut [usize],
                         order: &mut Vec<usize>,
                         stack: &mut Vec<usize>|
     -> bool {
        *stamp += 1;
        let cur_stamp = *stamp;
        order.clear();
        stack.clear();
        visited[start] = cur_stamp;
        parent_node[start] = usize::MAX;
        parent_edge[start] = usize::MAX;
        stack.push(start);
        let mut has_anchor = false;
        while let Some(curr) = stack.pop() {
            order.push(curr);
            if curr == anchor {
                has_anchor = true;
            }
            for &slot in &adj[curr] {
                let e = &active_edges[slot];
                if !e.alive {
                    continue;
                }
                let next = if e.u == curr { e.v } else { e.u };
                if visited[next] != cur_stamp {
                    visited[next] = cur_stamp;
                    parent_node[next] = curr;
                    parent_edge[next] = slot;
                    stack.push(next);
                }
            }
        }
        has_anchor
    };

    let remove_active_edge = |slot: usize,
                              adj: &mut [Vec<usize>],
                              active_edges: &mut [ActiveEdge],
                              free: &mut Vec<usize>| {
        let u = active_edges[slot].u;
        let v = active_edges[slot].v;
        active_edges[slot].alive = false;
        if let Some(pos) = adj[u].iter().position(|&s| s == slot) {
            adj[u].swap_remove(pos);
        }
        if let Some(pos) = adj[v].iter().position(|&s| s == slot) {
            adj[v].swap_remove(pos);
        }
        free.push(slot);
    };

    let mut recompute_block_exact = |start: usize,
                                     adj: &[Vec<usize>],
                                     active_edges: &mut [ActiveEdge],
                                     visited: &mut [u64],
                                     stamp: &mut u64,
                                     parent_node: &mut [usize],
                                     parent_edge: &mut [usize],
                                     order: &mut Vec<usize>,
                                     stack: &mut Vec<usize>,
                                     x: &mut [f64]| {
        let mut has_anchor = dfs_component(
            start,
            adj,
            active_edges,
            visited,
            stamp,
            parent_node,
            parent_edge,
            order,
            stack,
        );
        let root = if has_anchor {
            if start != anchor {
                has_anchor = dfs_component(
                    anchor,
                    adj,
                    active_edges,
                    visited,
                    stamp,
                    parent_node,
                    parent_edge,
                    order,
                    stack,
                );
            }
            anchor
        } else {
            start
        };
        offset[root] = 0.0;
        for &w in &order[1..] {
            let p = parent_node[w];
            let e = &active_edges[parent_edge[w]];
            offset[w] = if e.u == p && e.v == w {
                offset[p] + e.gap
            } else {
                offset[p] - e.gap
            };
        }
        let pos = if has_anchor {
            0.0
        } else {
            let mut sw = 0.0;
            let mut swd = 0.0;
            for &w in order.iter() {
                sw += comp_w[w];
                swd += comp_w[w] * (comp_d[w] - offset[w]);
            }
            swd / sw
        };
        for &w in order.iter() {
            x[w] = pos + offset[w];
            f_sub[w] = if w == anchor {
                0.0
            } else {
                comp_w[w] * (x[w] - comp_d[w])
            };
        }
        // Spanning-tree Lagrange multiplier calculation (`compute_lm`).
        for &w in order[1..].iter().rev() {
            let p = parent_node[w];
            let slot = parent_edge[w];
            let val = f_sub[w];
            f_sub[p] += val;
            let e = &mut active_edges[slot];
            let lm = if e.u == p && e.v == w { val } else { -val };
            e.lm = if (-1e-12..0.0).contains(&lm) { 0.0 } else { lm };
        }
    };

    let m = ineqs.len();
    let mut cursor = 0usize;
    let mut clean_streak = 0usize;
    let max_outer = (m + k + 1) * 32 + 256;
    let mut outer_iters = 0usize;

    while clean_streak < m {
        outer_iters += 1;
        if outer_iters > max_outer {
            // Fall back to exact KKT active-set solver on the super-variables if degenerate cycle.
            return solve_kkt_component(desired, weights, rows);
        }
        let ineq_idx = cursor;
        cursor = (cursor + 1) % m;
        let ineq = ineqs[ineq_idx];
        let viol = ineq.gap - (x[ineq.v] - x[ineq.u]);
        if viol <= ACTIVATE_TOL {
            clean_streak += 1;
            continue;
        }
        clean_streak = 0;

        let mut lambda_new = 0.0_f64;
        let max_inner = active_edges.len() + k + 64;
        let mut inner_iters = 0usize;
        loop {
            inner_iters += 1;
            if inner_iters > max_inner {
                return Err(Error::Layout("VPSC inner pivot budget exceeded".into()));
            }
            let cur_viol = ineq.gap - (x[ineq.v] - x[ineq.u]);
            if cur_viol <= 1e-11 {
                if lambda_new > 0.0 {
                    // Edge became tight during partial steps; if disconnected, insert.
                    let anchored_u = dfs_component(
                        ineq.u,
                        &adj,
                        &active_edges,
                        &mut visited,
                        &mut stamp,
                        &mut parent_node,
                        &mut parent_edge,
                        &mut order_u,
                        &mut stack,
                    );
                    let same_block = visited[ineq.v] == stamp;
                    if !same_block {
                        let _ = anchored_u;
                        let slot = if let Some(s) = free_slots.pop() {
                            active_edges[s] = ActiveEdge {
                                ineq_idx,
                                u: ineq.u,
                                v: ineq.v,
                                gap: ineq.gap,
                                lm: lambda_new,
                                alive: true,
                            };
                            s
                        } else {
                            let s = active_edges.len();
                            active_edges.push(ActiveEdge {
                                ineq_idx,
                                u: ineq.u,
                                v: ineq.v,
                                gap: ineq.gap,
                                lm: lambda_new,
                                alive: true,
                            });
                            s
                        };
                        adj[ineq.u].push(slot);
                        adj[ineq.v].push(slot);
                        recompute_block_exact(
                            ineq.u,
                            &adj,
                            &mut active_edges,
                            &mut visited,
                            &mut stamp,
                            &mut parent_node,
                            &mut parent_edge,
                            &mut order_u,
                            &mut stack,
                            &mut x,
                        );
                    }
                }
                break;
            }

            let anchored_u = dfs_component(
                ineq.u,
                &adj,
                &active_edges,
                &mut visited,
                &mut stamp,
                &mut parent_node,
                &mut parent_edge,
                &mut order_u,
                &mut stack,
            );
            if visited[ineq.v] == stamp {
                // Case 2: u and v are in the same block; find forward edges along path u ~> v.
                path_updates.clear();
                let mut curr = ineq.v;
                let mut drop_slot: Option<(usize, usize, f64)> = None;
                while curr != ineq.u {
                    let p = parent_node[curr];
                    let slot = parent_edge[curr];
                    let e = &active_edges[slot];
                    let dot = if e.u == p && e.v == curr { -1.0 } else { 1.0 };
                    path_updates.push((slot, dot));
                    if dot < 0.0 {
                        let lm_val = e.lm.max(0.0);
                        let key = (lm_val, e.ineq_idx);
                        let better = match drop_slot {
                            None => true,
                            Some((_, best_ineq, best_lm)) => {
                                key.0 < best_lm - 1e-12
                                    || ((key.0 - best_lm).abs() <= 1e-12 && key.1 < best_ineq)
                            }
                        };
                        if better {
                            drop_slot = Some((slot, e.ineq_idx, lm_val));
                        }
                    }
                    curr = p;
                }
                let Some((slot_star, _, t_dual)) = drop_slot else {
                    if cur_viol > FEAS_TOL {
                        return Err(Error::Layout("infeasible separation constraints".into()));
                    }
                    break;
                };
                for &(slot, dot) in &path_updates {
                    active_edges[slot].lm = (active_edges[slot].lm + t_dual * dot).max(0.0);
                }
                lambda_new += t_dual;
                remove_active_edge(slot_star, &mut adj, &mut active_edges, &mut free_slots);
                continue;
            }

            // Case 1: u and v are in distinct blocks.
            updates_u.clear();
            let w_u: f64 = if anchored_u {
                let mut curr = anchor;
                while curr != ineq.u {
                    let p = parent_node[curr];
                    let slot = parent_edge[curr];
                    let e = &active_edges[slot];
                    let dot = if e.u == p && e.v == curr { -1.0 } else { 1.0 };
                    updates_u.push((slot, dot));
                    curr = p;
                }
                0.0
            } else {
                let mut total_w = 0.0;
                for &w in &order_u {
                    sub_w[w] = comp_w[w];
                    total_w += comp_w[w];
                }
                for &w in order_u[1..].iter().rev() {
                    let p = parent_node[w];
                    sub_w[p] += sub_w[w];
                    let slot = parent_edge[w];
                    let e = &active_edges[slot];
                    let dot = if e.u == p && e.v == w {
                        -sub_w[w] / total_w
                    } else {
                        sub_w[w] / total_w
                    };
                    updates_u.push((slot, dot));
                }
                total_w
            };

            let anchored_v = dfs_component(
                ineq.v,
                &adj,
                &active_edges,
                &mut visited,
                &mut stamp,
                &mut parent_node,
                &mut parent_edge,
                &mut order_v,
                &mut stack,
            );
            updates_v.clear();
            let w_v: f64 = if anchored_v {
                let mut curr = anchor;
                while curr != ineq.v {
                    let p = parent_node[curr];
                    let slot = parent_edge[curr];
                    let e = &active_edges[slot];
                    let dot = if e.u == curr && e.v == p { -1.0 } else { 1.0 };
                    updates_v.push((slot, dot));
                    curr = p;
                }
                0.0
            } else {
                let mut total_w = 0.0;
                for &w in &order_v {
                    sub_w[w] = comp_w[w];
                    total_w += comp_w[w];
                }
                for &w in order_v[1..].iter().rev() {
                    let p = parent_node[w];
                    sub_w[p] += sub_w[w];
                    let slot = parent_edge[w];
                    let e = &active_edges[slot];
                    let dot = if e.u == w && e.v == p {
                        -sub_w[w] / total_w
                    } else {
                        sub_w[w] / total_w
                    };
                    updates_v.push((slot, dot));
                }
                total_w
            };

            let inv_wu = if anchored_u { 0.0 } else { 1.0 / w_u };
            let inv_wv = if anchored_v { 0.0 } else { 1.0 / w_v };
            let t_full = cur_viol / (inv_wu + inv_wv);

            let mut blocker: Option<(usize, usize, f64)> = None;
            for &(slot, dot) in updates_u.iter().chain(&updates_v) {
                if dot < -COEFF_ZERO_EPS {
                    let e = &active_edges[slot];
                    let ratio = e.lm.max(0.0) / (-dot);
                    let better = match blocker {
                        None => true,
                        Some((_, best_ineq, best_ratio)) => {
                            ratio < best_ratio - 1e-12
                                || ((ratio - best_ratio).abs() <= 1e-12 && e.ineq_idx < best_ineq)
                        }
                    };
                    if better {
                        blocker = Some((slot, e.ineq_idx, ratio));
                    }
                }
            }

            let t_dual = blocker.map(|b| b.2).unwrap_or(f64::INFINITY);
            let t = t_full.min(t_dual);

            if !anchored_u {
                let step = t * inv_wu;
                for &w in &order_u {
                    x[w] -= step;
                }
            }
            if !anchored_v {
                let step = t * inv_wv;
                for &w in &order_v {
                    x[w] += step;
                }
            }
            for &(slot, dot) in updates_u.iter().chain(&updates_v) {
                active_edges[slot].lm = (active_edges[slot].lm + t * dot).max(0.0);
            }
            lambda_new += t;

            if t_full <= t_dual {
                let slot = if let Some(s) = free_slots.pop() {
                    active_edges[s] = ActiveEdge {
                        ineq_idx,
                        u: ineq.u,
                        v: ineq.v,
                        gap: ineq.gap,
                        lm: lambda_new,
                        alive: true,
                    };
                    s
                } else {
                    let s = active_edges.len();
                    active_edges.push(ActiveEdge {
                        ineq_idx,
                        u: ineq.u,
                        v: ineq.v,
                        gap: ineq.gap,
                        lm: lambda_new,
                        alive: true,
                    });
                    s
                };
                adj[ineq.u].push(slot);
                adj[ineq.v].push(slot);
                recompute_block_exact(
                    ineq.u,
                    &adj,
                    &mut active_edges,
                    &mut visited,
                    &mut stamp,
                    &mut parent_node,
                    &mut parent_edge,
                    &mut order_u,
                    &mut stack,
                    &mut x,
                );
                break;
            } else if let Some((drop_slot, _, _)) = blocker {
                remove_active_edge(drop_slot, &mut adj, &mut active_edges, &mut free_slots);
            }
        }
    }

    // Belt-and-suspenders dual verification: split any residual negative multiplier and polish.
    for _ in 0..k + 4 {
        let mut neg_slot: Option<(usize, f64)> = None;
        for (slot, e) in active_edges.iter().enumerate() {
            if e.alive && e.lm < -1e-7 && neg_slot.is_none_or(|(_, best)| e.lm < best) {
                neg_slot = Some((slot, e.lm));
            }
        }
        let Some((slot, _)) = neg_slot else {
            break;
        };
        let u = active_edges[slot].u;
        let v = active_edges[slot].v;
        remove_active_edge(slot, &mut adj, &mut active_edges, &mut free_slots);
        recompute_block_exact(
            u,
            &adj,
            &mut active_edges,
            &mut visited,
            &mut stamp,
            &mut parent_node,
            &mut parent_edge,
            &mut order_u,
            &mut stack,
            &mut x,
        );
        recompute_block_exact(
            v,
            &adj,
            &mut active_edges,
            &mut visited,
            &mut stamp,
            &mut parent_node,
            &mut parent_edge,
            &mut order_v,
            &mut stack,
            &mut x,
        );
        // Re-check feasibility after split; if violated, defer to KKT solver.
        if ineqs
            .iter()
            .any(|e| e.gap - (x[e.v] - x[e.u]) > ACTIVATE_TOL)
        {
            return solve_kkt_component(desired, weights, rows);
        }
    }

    // Reconstruct original variable positions and verify feasibility.
    let result: Vec<f64> = (0..n).map(|i| x[node_super[i]] + node_off[i]).collect();
    for r in rows {
        let val = r.eval(&result);
        let ok = if r.equality {
            (val - r.rhs).abs() <= FEAS_TOL
        } else {
            val - r.rhs >= -FEAS_TOL
        };
        if !ok {
            return Err(Error::Layout("infeasible separation constraints".into()));
        }
    }
    Ok(result)
}

// ---------- General Primal-Dual Active-Set KKT Solver (Incremental Pivoted LDLT) ----------

fn ldlt_solve(l_mat: &[Vec<f64>], d_vec: &[f64], rhs: &[f64]) -> Vec<f64> {
    let m = d_vec.len();
    let mut y = vec![0.0; m];
    for i in 0..m {
        let mut s = rhs[i];
        let row = &l_mat[i];
        for j in 0..i {
            s -= row[j] * y[j];
        }
        y[i] = s;
    }
    for i in 0..m {
        y[i] /= d_vec[i];
    }
    for i in (0..m).rev() {
        let yi = y[i];
        let row = &l_mat[i];
        for j in 0..i {
            y[j] -= row[j] * yi;
        }
    }
    y
}

fn ldlt_remove_row_col(l_mat: &mut Vec<Vec<f64>>, d_vec: &mut Vec<f64>, p: usize) {
    let m = d_vec.len();
    let mut alpha = d_vec.remove(p);
    l_mat.remove(p);
    let rem = m - 1 - p;
    let mut v = Vec::with_capacity(rem);
    for row in l_mat.iter_mut().take(m - 1).skip(p) {
        v.push(row.remove(p));
    }
    // Gill-Murray-Saunders positive rank-1 update: L' D' L'^T = L D L^T + alpha * v * v^T.
    for k in 0..rem {
        let r = p + k;
        let vk = v[k];
        if vk == 0.0 {
            continue;
        }
        let d_old = d_vec[r];
        let d_new = d_old + alpha * vk * vk;
        let beta = (alpha * vk) / d_new;
        d_vec[r] = d_new;
        alpha *= d_old / d_new;
        for (j, vj) in v.iter_mut().enumerate().take(rem).skip(k + 1) {
            let rj = p + j;
            let ljr = l_mat[rj][r];
            *vj -= vk * ljr;
            l_mat[rj][r] = ljr + beta * *vj;
        }
    }
}

fn ldlt_rebuild(
    basis: &[usize],
    rows: &[CanonicalRow],
    inv_w: &[f64],
) -> Option<(Vec<Vec<f64>>, Vec<f64>)> {
    let m = basis.len();
    let mut l_mat: Vec<Vec<f64>> = Vec::with_capacity(m);
    let mut d_vec: Vec<f64> = Vec::with_capacity(m);
    let mut y: Vec<f64> = Vec::with_capacity(m);
    for (idx, &ri) in basis.iter().enumerate() {
        let row = &rows[ri];
        let diag0 = row.inv_w_norm_sq(inv_w);
        y.clear();
        for k in 0..idx {
            let g_k = row.dot_inv_w(&rows[basis[k]], inv_w);
            let mut s = g_k;
            let lk = &l_mat[k];
            for j in 0..k {
                s -= lk[j] * y[j];
            }
            y.push(s);
        }
        let mut l_new = Vec::with_capacity(idx);
        let mut d_new = diag0;
        for k in 0..idx {
            let l_ik = y[k] / d_vec[k];
            d_new -= d_vec[k] * l_ik * l_ik;
            l_new.push(l_ik);
        }
        if !d_new.is_finite() || d_new <= 1e-14 * diag0 {
            return None;
        }
        l_mat.push(l_new);
        d_vec.push(d_new);
    }
    Some((l_mat, d_vec))
}

fn solve_kkt_component(
    desired: &[f64],
    weights: &[f64],
    rows: &[CanonicalRow],
) -> Result<Vec<f64>, Error> {
    let n = desired.len();
    let inv_w: Vec<f64> = weights.iter().map(|&w| 1.0 / w).collect();
    let eq_indices: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r.equality)
        .map(|(i, _)| i)
        .collect();
    let ineq_indices: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, r)| !r.equality)
        .map(|(i, _)| i)
        .collect();

    let mut basis: Vec<usize> = Vec::new();
    let mut l_mat: Vec<Vec<f64>> = Vec::new();
    let mut d_vec: Vec<f64> = Vec::new();
    let mut in_basis = vec![false; rows.len()];

    // Phase 0: Rank-revealing pivoted LDLT over equalities.
    if !eq_indices.is_empty() {
        let me = eq_indices.len();
        let orig_diag: Vec<f64> = eq_indices
            .iter()
            .map(|&ri| rows[ri].inv_w_norm_sq(&inv_w))
            .collect();
        let mut rem_diag = orig_diag.clone();
        let mut used = vec![false; me];
        let mut l_unused: Vec<Vec<f64>> = vec![Vec::new(); me];

        for _ in 0..me {
            let mut best: Option<(usize, f64)> = None;
            for q in 0..me {
                if used[q] {
                    continue;
                }
                let ratio = rem_diag[q] / orig_diag[q];
                if ratio > 1e-12 && best.is_none_or(|(_, br)| ratio > br) {
                    best = Some((q, ratio));
                }
            }
            let Some((p, _)) = best else {
                break;
            };
            used[p] = true;
            let rp = eq_indices[p];
            let l_new = std::mem::take(&mut l_unused[p]);
            let mut d_new = orig_diag[p];
            for (k, &lik) in l_new.iter().enumerate() {
                d_new -= d_vec[k] * lik * lik;
            }
            if !d_new.is_finite() || d_new <= 1e-13 * orig_diag[p] {
                break;
            }
            let r_idx = basis.len();
            basis.push(rp);
            in_basis[rp] = true;
            l_mat.push(l_new);
            d_vec.push(d_new);

            let lp = &l_mat[r_idx];
            for q in 0..me {
                if used[q] {
                    continue;
                }
                let g_qp = rows[eq_indices[q]].dot_inv_w(&rows[rp], &inv_w);
                let lq = &mut l_unused[q];
                let mut y_qr = g_qp;
                for k in 0..r_idx {
                    y_qr -= lp[k] * d_vec[k] * lq[k];
                }
                let l_qr = y_qr / d_new;
                lq.push(l_qr);
                rem_diag[q] -= d_new * l_qr * l_qr;
            }
        }
    }

    let num_eq = basis.len();
    let rhs_0: Vec<f64> = basis
        .iter()
        .map(|&ri| rows[ri].rhs - rows[ri].eval(desired))
        .collect();
    let mut lambda = if num_eq > 0 {
        ldlt_solve(&l_mat, &d_vec, &rhs_0)
    } else {
        Vec::new()
    };

    let mut x = desired.to_vec();
    for (k, &ri) in basis.iter().enumerate() {
        let lk = lambda[k];
        for &(i, a) in &rows[ri].coeffs {
            x[i] += lk * a * inv_w[i];
        }
    }

    // Verify all dependent/redundant equalities are consistent.
    for &ri in &eq_indices {
        if !in_basis[ri] && (rows[ri].eval(&x) - rows[ri].rhs).abs() > FEAS_TOL {
            return Err(Error::Layout("contradictory geometric constraints".into()));
        }
    }

    if ineq_indices.is_empty() {
        return Ok(x);
    }

    // Phase 1: Warm-started Goldfarb-Idnani dual active-set for inequalities.
    let mut tolerated = vec![false; rows.len()];
    let mut g = Vec::new();
    let mut y = Vec::new();
    let mut r_vec = Vec::new();
    let mut z = vec![0.0_f64; n];
    let mut basis_ops = 0usize;
    let max_outer = (rows.len() + n + 1) * 16 + 256;
    let bland_threshold = rows.len() * 2 + 16;

    for outer in 0..max_outer {
        let use_bland = outer >= bland_threshold;
        let chosen = if use_bland {
            ineq_indices.iter().copied().find(|&ri| {
                !in_basis[ri] && !tolerated[ri] && (rows[ri].rhs - rows[ri].eval(&x)) > ACTIVATE_TOL
            })
        } else {
            let mut best: Option<(usize, f64)> = None;
            for &ri in &ineq_indices {
                if in_basis[ri] || tolerated[ri] {
                    continue;
                }
                let viol = rows[ri].rhs - rows[ri].eval(&x);
                if viol > ACTIVATE_TOL && best.is_none_or(|(_, bv)| viol > bv) {
                    best = Some((ri, viol));
                }
            }
            best.map(|(ri, _)| ri)
        };
        let Some(c) = chosen else {
            break;
        };

        let mut lambda_c = 0.0_f64;
        let max_inner = rows.len() + n + 64;
        for _ in 0..max_inner {
            let cur_viol = rows[c].rhs - rows[c].eval(&x);
            if cur_viol <= 1e-11 && lambda_c == 0.0 {
                break;
            }
            let mb = basis.len();
            g.clear();
            for &ri in &basis {
                g.push(rows[ri].dot_inv_w(&rows[c], &inv_w));
            }
            y.clear();
            for k in 0..mb {
                let mut s = g[k];
                let lk = &l_mat[k];
                for j in 0..k {
                    s -= lk[j] * y[j];
                }
                y.push(s);
            }
            let mut l_new = Vec::with_capacity(mb);
            for k in 0..mb {
                l_new.push(y[k] / d_vec[k]);
            }
            r_vec.clear();
            r_vec.extend_from_slice(&l_new);
            for k in (0..mb).rev() {
                let rk = r_vec[k];
                let lk = &l_mat[k];
                for j in 0..k {
                    r_vec[j] -= lk[j] * rk;
                }
            }

            z.fill(0.0);
            for &(i, a) in &rows[c].coeffs {
                z[i] += a * inv_w[i];
            }
            for k in 0..mb {
                let rk = r_vec[k];
                if rk != 0.0 {
                    for &(i, a) in &rows[basis[k]].coeffs {
                        z[i] -= rk * a * inv_w[i];
                    }
                }
            }

            let a_norm_sq = rows[c].inv_w_norm_sq(&inv_w);
            let mut d_new_dual = a_norm_sq;
            for k in 0..mb {
                d_new_dual -= d_vec[k] * l_new[k] * l_new[k];
            }
            let d_new_primal = rows[c].eval(&z);
            let is_independent = d_new_primal > 1e-13 * a_norm_sq && d_new_dual > 1e-13 * a_norm_sq;

            let t_full = if is_independent {
                (cur_viol.max(0.0)) / d_new_primal
            } else {
                f64::INFINITY
            };

            let mut blocker: Option<(usize, usize, f64)> = None;
            for k in num_eq..mb {
                let rk = r_vec[k];
                if rk > COEFF_ZERO_EPS {
                    let ratio = lambda[k].max(0.0) / rk;
                    let ri = basis[k];
                    let better = match blocker {
                        None => true,
                        Some((_, best_ri, best_ratio)) => {
                            ratio < best_ratio - 1e-12
                                || ((ratio - best_ratio).abs() <= 1e-12 && ri < best_ri)
                        }
                    };
                    if better {
                        blocker = Some((k, ri, ratio));
                    }
                }
            }
            let t_dual = blocker.map(|b| b.2).unwrap_or(f64::INFINITY);

            if !t_full.is_finite() && !t_dual.is_finite() {
                if cur_viol > FEAS_TOL {
                    return Err(Error::Layout("infeasible linear constraints".into()));
                }
                tolerated[c] = true;
                break;
            }

            let t = t_full.min(t_dual);
            for k in 0..mb {
                lambda[k] -= t * r_vec[k];
                if k >= num_eq && lambda[k] < 0.0 {
                    lambda[k] = 0.0;
                }
            }
            lambda_c += t;
            if is_independent {
                for i in 0..n {
                    x[i] += t * z[i];
                }
            }

            if t_full <= t_dual {
                basis.push(c);
                in_basis[c] = true;
                lambda.push(lambda_c);
                l_mat.push(l_new);
                d_vec.push(d_new_dual);
                basis_ops += 1;
                break;
            } else if let Some((k_drop, _, _)) = blocker {
                let dropped = basis.remove(k_drop);
                in_basis[dropped] = false;
                lambda.remove(k_drop);
                ldlt_remove_row_col(&mut l_mat, &mut d_vec, k_drop);
                basis_ops += 1;
            }
        }

        // Periodically refresh LDLT, lambda, and x from the active basis to eliminate drift.
        if basis_ops >= 64
            && !basis.is_empty()
            && let Some((nl, nd)) = ldlt_rebuild(&basis, rows, &inv_w)
        {
            basis_ops = 0;
            l_mat = nl;
            d_vec = nd;
            let rhs_b: Vec<f64> = basis
                .iter()
                .map(|&ri| rows[ri].rhs - rows[ri].eval(desired))
                .collect();
            lambda = ldlt_solve(&l_mat, &d_vec, &rhs_b);
            for lk in lambda.iter_mut().skip(num_eq) {
                if *lk < 0.0 {
                    *lk = 0.0;
                }
            }
            x.copy_from_slice(desired);
            for (k, &ri) in basis.iter().enumerate() {
                let lk = lambda[k];
                for &(i, a) in &rows[ri].coeffs {
                    x[i] += lk * a * inv_w[i];
                }
            }
        }
    }

    // Final exact polish from active basis and strict feasibility check.
    if !basis.is_empty()
        && let Some((nl, nd)) = ldlt_rebuild(&basis, rows, &inv_w)
    {
        let rhs_b: Vec<f64> = basis
            .iter()
            .map(|&ri| rows[ri].rhs - rows[ri].eval(desired))
            .collect();
        let exact_lm = ldlt_solve(&nl, &nd, &rhs_b);
        let mut x_polish = desired.to_vec();
        for (k, &ri) in basis.iter().enumerate() {
            let lk = exact_lm[k];
            for &(i, a) in &rows[ri].coeffs {
                x_polish[i] += lk * a * inv_w[i];
            }
        }
        let polish_ok = rows.iter().all(|r| {
            let v = r.eval(&x_polish);
            if r.equality {
                (v - r.rhs).abs() <= FEAS_TOL
            } else {
                v - r.rhs >= -FEAS_TOL
            }
        });
        if polish_ok {
            x = x_polish;
        }
    }

    for r in rows {
        let v = r.eval(&x);
        let ok = if r.equality {
            (v - r.rhs).abs() <= FEAS_TOL
        } else {
            v - r.rhs >= -FEAS_TOL
        };
        if !ok {
            return Err(Error::Layout("infeasible linear constraints".into()));
        }
    }
    Ok(x)
}

#[derive(Debug, Clone)]
pub struct Dual {
    pub value: f64,
    pub gradient: Vec<f64>,
}
impl Dual {
    pub fn variable(value: f64, index: usize, n: usize) -> Self {
        let mut gradient = vec![0.; n];
        gradient[index] = 1.;
        Self { value, gradient }
    }
    pub fn constant(value: f64, n: usize) -> Self {
        Self {
            value,
            gradient: vec![0.; n],
        }
    }
    pub fn add(&self, b: &Self) -> Self {
        Self {
            value: self.value + b.value,
            gradient: self
                .gradient
                .iter()
                .zip(&b.gradient)
                .map(|(a, b)| a + b)
                .collect(),
        }
    }
    pub fn sub(&self, b: &Self) -> Self {
        self.add(&b.scale(-1.))
    }
    pub fn scale(&self, a: f64) -> Self {
        Self {
            value: self.value * a,
            gradient: self.gradient.iter().map(|x| x * a).collect(),
        }
    }
    pub fn square(&self) -> Self {
        Self {
            value: self.value * self.value,
            gradient: self.gradient.iter().map(|g| 2. * self.value * g).collect(),
        }
    }
    pub fn sqrt(&self) -> Self {
        let value = self.value.max(1e-20).sqrt();
        Self {
            value,
            gradient: self.gradient.iter().map(|g| g / (2. * value)).collect(),
        }
    }
    pub fn softplus(&self, beta: f64) -> Self {
        let z = beta * self.value;
        let value = (z.max(0.) + (-z.abs()).exp().ln_1p()) / beta;
        let sigmoid = if z >= 0. {
            1. / (1. + (-z).exp())
        } else {
            z.exp() / (1. + z.exp())
        };
        Self {
            value,
            gradient: self.gradient.iter().map(|g| g * sigmoid).collect(),
        }
    }
}

/// Bounded L-BFGS with projected Armijo line search and exact forward-mode gradients.
pub fn lbfgs(
    initial: &[f64],
    bounds: &[(f64, f64)],
    steps: usize,
    energy: impl Fn(&[Dual]) -> Dual,
) -> Vec<f64> {
    let n = initial.len();
    if n == 0 {
        return Vec::new();
    }
    let eval = |x: &[f64]| {
        energy(
            &x.iter()
                .enumerate()
                .map(|(i, v)| Dual::variable(*v, i, n))
                .collect::<Vec<_>>(),
        )
    };
    let mut x = initial.to_vec();
    for (i, v) in x.iter_mut().enumerate() {
        *v = v.clamp(bounds[i].0, bounds[i].1);
    }
    let mut f = eval(&x);
    let mut history: Vec<(Vec<f64>, Vec<f64>, f64)> = Vec::new();
    for _ in 0..steps {
        let mut q = f.gradient.clone();
        let mut alphas = Vec::new();
        for (s, y, rho) in history.iter().rev() {
            let a = rho * dot(s, &q);
            alphas.push(a);
            for i in 0..n {
                q[i] -= a * y[i];
            }
        }
        let scale = history
            .last()
            .map(|(s, y, _)| dot(s, y) / dot(y, y).max(1e-20))
            .unwrap_or(1.);
        for v in &mut q {
            *v *= scale;
        }
        for ((s, y, rho), alpha) in history.iter().zip(alphas.iter().rev()) {
            let b = rho * dot(y, &q);
            for i in 0..n {
                q[i] += s[i] * (alpha - b);
            }
        }
        let mut direction: Vec<_> = q.iter().map(|v| -v).collect();
        if dot(&direction, &f.gradient) >= 0. {
            direction = f.gradient.iter().map(|v| -v).collect();
        }
        let mut step = 1.;
        let mut accepted = None;
        for _ in 0..24 {
            let candidate: Vec<_> = x
                .iter()
                .enumerate()
                .map(|(i, v)| (v + step * direction[i]).clamp(bounds[i].0, bounds[i].1))
                .collect();
            let delta: Vec<_> = candidate.iter().zip(&x).map(|(a, b)| a - b).collect();
            let next = eval(&candidate);
            if next.value.is_finite() && next.value <= f.value + 1e-4 * dot(&f.gradient, &delta) {
                accepted = Some((candidate, next, delta));
                break;
            }
            step *= 0.5;
        }
        let Some((next, nf, s)) = accepted else {
            break;
        };
        if dot(&s, &s) < 1e-16 {
            break;
        }
        let y: Vec<_> = nf
            .gradient
            .iter()
            .zip(&f.gradient)
            .map(|(a, b)| a - b)
            .collect();
        let sy = dot(&s, &y);
        if sy > 1e-10 {
            if history.len() == 5 {
                history.remove(0);
            }
            history.push((s, y, 1. / sy));
        }
        x = next;
        f = nf;
    }
    x
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}

fn transitive_reduced_dominance(
    previous: &[Option<f64>],
    is_pinned: &[bool],
) -> Vec<(usize, usize)> {
    let mut ranked: Vec<(usize, f64)> = previous
        .iter()
        .enumerate()
        .filter(|&(i, _)| !is_pinned[i])
        .filter_map(|(i, v)| v.map(|val| (i, val)))
        .collect();
    ranked.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));

    let mut orders = Vec::new();
    for idx in 0..ranked.len() {
        let (i, pi) = ranked[idx];
        let mut first_pos: Option<f64> = None;
        for &(j, pj) in &ranked[idx + 1..] {
            if pj - pi < 1.0 {
                continue;
            }
            match first_pos {
                None => {
                    first_pos = Some(pj);
                    orders.push((i, j));
                }
                Some(fp) => {
                    if pj >= fp + 1.0 {
                        break;
                    }
                    orders.push((i, j));
                }
            }
        }
    }
    orders
}

/// Translation-adjusted anchoring with elastic prior-order hinges. A common translation
/// variable removes rigid motion from unpinned anchors; order slacks carry a linear penalty.
pub fn project_stable(
    desired: &[f64],
    weights: &[f64],
    constraints: &[Constraint],
    previous: &[Option<f64>],
    flip_penalty: f64,
) -> Result<Vec<f64>, Error> {
    let n = desired.len();
    validate_objective(desired, weights)?;
    if previous.len() != n
        || previous.iter().any(|v| v.is_some_and(|x| !x.is_finite()))
        || !flip_penalty.is_finite()
        || flip_penalty < 0.
    {
        return Err(Error::Layout("invalid stability anchors".into()));
    }

    let is_pinned: Vec<bool> = weights.iter().map(|&w| w >= PIN_WEIGHT_THRESHOLD).collect();
    let unpinned: Vec<usize> = (0..n).filter(|&i| !is_pinned[i]).collect();
    if unpinned.is_empty() {
        return project(desired, weights, constraints);
    }

    let orders = if flip_penalty > COEFF_ZERO_EPS {
        transitive_reduced_dominance(previous, &is_pinned)
    } else {
        Vec::new()
    };

    let unpinned_weight_sum: f64 = unpinned.iter().map(|&i| weights[i]).sum();
    let delta_weight = (1e-6 * unpinned_weight_sum).clamp(1e-6, 1e-3);

    let mut hinges = Vec::<(usize, usize)>::new();
    let solve = |hinges: &[(usize, usize)]| -> Result<Vec<f64>, Error> {
        let mut d = desired.to_vec();
        let mut w = weights.to_vec();
        // Rigid-body shift Delta applies exclusively to unpinned coordinates.
        d.push(0.);
        w.push(delta_weight);

        let mut cs = constraints.to_vec();
        for c in &mut cs {
            let sum_unpinned: f64 = c
                .coefficients
                .iter()
                .filter(|&&(i, _)| i < n && !is_pinned[i])
                .map(|&(_, a)| a)
                .sum();
            if sum_unpinned.abs() > 1e-10 {
                c.coefficients.push((n, sum_unpinned));
            }
        }
        const SLACK_QUAD_W: f64 = 1e-3;
        for &(a, b) in hinges {
            let slack = d.len();
            d.push(-flip_penalty / (2. * SLACK_QUAD_W));
            w.push(SLACK_QUAD_W);
            cs.push(Constraint {
                coefficients: vec![(a, -1.), (b, 1.), (slack, 1.)],
                rhs: 0.,
                equality: false,
            });
            cs.push(Constraint {
                coefficients: vec![(slack, 1.)],
                rhs: 0.,
                equality: false,
            });
        }
        let solution = project(&d, &w, &cs)?;
        let mut result = solution[..n].to_vec();
        for &i in &unpinned {
            result[i] += solution[n];
        }
        project(&result, weights, constraints)
    };

    let mut current = solve(&[])?;
    for _ in 0..orders.len() + 1 {
        let additions: Vec<_> = orders
            .iter()
            .filter(|(a, b)| current[*a] > current[*b] + 1e-6 && !hinges.contains(&(*a, *b)))
            .copied()
            .collect();
        if additions.is_empty() {
            return Ok(current);
        }
        hinges.extend(additions);
        current = solve(&hinges)?;
    }
    Err(Error::Layout(
        "elastic order projection did not converge".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_duplicate_near_zero_and_empty_constraints() {
        // Duplicate variable indices in one row must coalesce before computing norms/steps:
        // 1*x + 2*x == 6 => 3*x == 6 => x == 2 (old dual_project miscomputed norm).
        let x = project(
            &[0.0],
            &[1.0],
            &[Constraint {
                coefficients: vec![(0, 1.0), (0, 2.0)],
                rhs: 6.0,
                equality: true,
            }],
        )
        .unwrap();
        assert!((x[0] - 2.0).abs() < 1e-9);

        // Coalesced cancellation leaves empty row; check >= rhs and == rhs feasibility rules.
        let ok_ineq = project(
            &[4.0],
            &[1.0],
            &[
                Constraint {
                    coefficients: vec![(0, 1.0), (0, -1.0)],
                    rhs: 0.0,
                    equality: false,
                },
                Constraint {
                    coefficients: vec![(0, 1e-13)],
                    rhs: -5.0,
                    equality: false,
                },
                Constraint {
                    coefficients: vec![(0, 5e-13), (0, -5e-13)],
                    rhs: 5e-7,
                    equality: true,
                },
            ],
        )
        .unwrap();
        assert!((ok_ineq[0] - 4.0).abs() < 1e-9);

        // Infeasible empty inequality (0 >= 1e-4) and infeasible empty equality (0 == 1e-4).
        assert!(
            project(
                &[0.0],
                &[1.0],
                &[Constraint {
                    coefficients: vec![(0, 1.0), (0, -1.0)],
                    rhs: 1e-4,
                    equality: false,
                }]
            )
            .is_err()
        );
        assert!(
            project(
                &[0.0],
                &[1.0],
                &[Constraint {
                    coefficients: vec![(0, 1e-14)],
                    rhs: 1e-4,
                    equality: true,
                }]
            )
            .is_err()
        );
    }

    #[test]
    fn vpsc_block_split_and_anchored_cycles() {
        // Matches transitive_active_constraints_can_be_released exactly.
        let x = project(
            &[0., 0., 0.],
            &[1., 1., 1.],
            &[
                Constraint::separation(0, 2, 15.),
                Constraint::separation(0, 1, 10.),
                Constraint::separation(1, 2, 10.),
            ],
        )
        .unwrap();
        assert!((x[0] + 10.).abs() < 1e-9);
        assert!(x[1].abs() < 1e-9);
        assert!((x[2] - 10.).abs() < 1e-9);

        // Container containment cycle (p -> i and i -> p with negative gaps) + box bounds.
        let y = project(
            &[0., 100.],
            &[2., 1.],
            &[
                Constraint::separation(0, 1, -30.),
                Constraint::separation(1, 0, -30.),
                Constraint {
                    coefficients: vec![(0, 1.)],
                    rhs: 10.,
                    equality: false,
                },
                Constraint {
                    coefficients: vec![(1, -1.)],
                    rhs: -80.,
                    equality: false,
                },
            ],
        )
        .unwrap();
        assert!(y[1] - y[0] >= -30. - 1e-6);
        assert!(y[0] - y[1] >= -30. - 1e-6);
        assert!(y[0] >= 10. - 1e-6);
        assert!(y[1] <= 80. + 1e-6);

        // Conflicting positive-sum cycle is infeasible.
        assert!(
            project(
                &[0., 0., 0.],
                &[1., 1., 1.],
                &[
                    Constraint::separation(0, 1, 5.),
                    Constraint::separation(1, 2, 5.),
                    Constraint::separation(2, 0, -8.),
                ]
            )
            .is_err()
        );
    }

    #[test]
    fn general_kkt_three_var_equality_and_inequality() {
        // Distribute midpoint equality: 0.5*u_0 + 0.5*u_2 - u_1 == 0, plus dependent redundancy
        // and inequalities where u_2 - u_0 >= 30 becomes slack once u_2 - u_1 >= 25 activates.
        let x = project(
            &[0., 0., 0.],
            &[1., 10., 1.],
            &[
                Constraint {
                    coefficients: vec![(0, 0.5), (1, -1.0), (2, 0.5)],
                    rhs: 0.0,
                    equality: true,
                },
                Constraint {
                    coefficients: vec![(0, 1.0), (1, -2.0), (2, 1.0)],
                    rhs: 0.0,
                    equality: true,
                },
                Constraint::fixed(0, 0.0),
                Constraint::separation(0, 2, 30.0),
                Constraint::separation(1, 2, 25.0),
                Constraint {
                    coefficients: vec![(2, -1.0)],
                    rhs: -60.0,
                    equality: false,
                },
            ],
        )
        .unwrap();
        assert!((x[0] - 0.0).abs() < 1e-7);
        assert!((x[1] - 25.0).abs() < 1e-7);
        assert!((x[2] - 50.0).abs() < 1e-7);
        assert!((0.5 * x[0] + 0.5 * x[2] - x[1]).abs() < 1e-7);

        // Coupled VPSC + general row detects infeasibility cleanly.
        assert!(
            project(
                &[0., 0., 0.],
                &[1., 1., 1.],
                &[
                    Constraint {
                        coefficients: vec![(0, 0.5), (1, -1.0), (2, 0.5)],
                        rhs: 0.0,
                        equality: true,
                    },
                    Constraint::fixed(0, 0.0),
                    Constraint::fixed(2, 20.0),
                    Constraint {
                        coefficients: vec![(1, 1.0)],
                        rhs: 15.0,
                        equality: false,
                    },
                ]
            )
            .is_err()
        );
    }

    #[test]
    fn project_compacted_shrinks_l1_wires_and_wall_spans() {
        // L1 wire compaction pulls x[1] toward fixed x[0] = 0 by lambda / w = 12.0.
        let wire = project_compacted(
            &[0., 100.],
            &[1e5, 1.],
            &[Constraint::fixed(0, 0.), Constraint::separation(0, 1, 20.)],
            &[],
            &[L1Term::pair(0, 1, 12.)],
        )
        .unwrap();
        assert!(wire[0].abs() < 1e-6);
        assert!((wire[1] - 88.).abs() < 1e-3);

        // Strong L1 compaction clamps tightly at the minimum separation gap.
        let tight = project_compacted(
            &[0., 100.],
            &[1e5, 1.],
            &[Constraint::fixed(0, 0.), Constraint::separation(0, 1, 20.)],
            &[],
            &[L1Term::pair(0, 1, 200.)],
        )
        .unwrap();
        assert!((tight[1] - 20.).abs() < 1e-3);

        // One-sided wall span lambda_w * (u_hi - u_lo) via linear objective shifts.
        let span = project_compacted(
            &[0., 100.],
            &[1., 1.],
            &[Constraint::separation(0, 1, 30.)],
            &[-15., 15.],
            &[],
        )
        .unwrap();
        assert!((span[0] - 15.).abs() < 1e-6);
        assert!((span[1] - 85.).abs() < 1e-6);
    }

    #[test]
    fn project_stable_isolates_pinned_from_delta_and_reduces_dominance() {
        // Node 0 is pinned at 100 (weight 1e5); nodes 1 and 2 are unpinned (weight 20).
        // Separation forces unpinned node 1 past 150; Delta translates {1, 2} rigidly (+130)
        // without pulling node 0 away from 100 or coupling 1e5 * Delta^2.
        let x = project_stable(
            &[100., 20., 80.],
            &[1e5, 20., 20.],
            &[
                Constraint::fixed(0, 100.),
                Constraint::separation(0, 1, 50.),
                Constraint::separation(1, 2, 40.),
            ],
            &[Some(0.), Some(20.), Some(80.)],
            450.,
        )
        .unwrap();
        assert!((x[0] - 100.).abs() < 1e-6);
        assert!((x[1] - 150.).abs() < 0.01);
        assert!((x[2] - 210.).abs() < 0.01);

        // Verify transitive-reduced dominance bridges across closely-spaced (< 1.0) sibling nodes.
        let is_pinned = [false, false, false, false];
        let orders = transitive_reduced_dominance(
            &[Some(0.0), Some(5.0), Some(5.2), Some(10.0)],
            &is_pinned,
        );
        assert_eq!(orders, vec![(0, 1), (0, 2), (1, 3), (2, 3)]);

        // Unpinned reversal happens cleanly when pinned constraints force it, while other unpinned
        // sibling order holds.
        let flipped = project_stable(
            &[0., 10., 20., 30.],
            &[1e5, 1., 1., 1e5],
            &[
                Constraint::fixed(0, 50.),
                Constraint::fixed(3, 0.),
                Constraint::separation(3, 1, 5.),
                Constraint::separation(1, 2, 5.),
            ],
            &[Some(0.), Some(10.), Some(20.), Some(30.)],
            450.,
        )
        .unwrap();
        assert!((flipped[0] - 50.).abs() < 1e-6);
        assert!(flipped[3].abs() < 1e-6);
        assert!(flipped[1] >= 5. - 1e-6);
        assert!(flipped[2] >= flipped[1] + 5. - 1e-6);
    }

    #[test]
    fn vpsc_matches_kkt_oracle_and_scales_to_500_chain() {
        // Cross-check VPSC against general active-set KKT on tricky multi-block cases
        // with asymmetric weights, anchored bounds, equalities, and negative-gap containment cycles.
        let desired = [50., -20., 80., 10., 40., 0.];
        let weights = [1., 3., 0.5, 2., 5., 1e5];
        let constraints = [
            Constraint::separation(0, 2, 15.),
            Constraint::separation(0, 1, 10.),
            Constraint::separation(1, 2, 10.),
            Constraint::equality(3, 4),
            Constraint::separation(2, 3, -5.),
            Constraint::separation(3, 2, -25.),
            Constraint::fixed(5, 100.),
            Constraint {
                coefficients: vec![(4, -1.)],
                rhs: -75.,
                equality: false,
            },
            Constraint {
                coefficients: vec![(0, 1.)],
                rhs: 5.,
                equality: false,
            },
        ];
        let vpsc = project(&desired, &weights, &constraints).unwrap();
        let rows = canonicalize_constraints(desired.len(), &constraints).unwrap();
        let kkt = solve_kkt_component(&desired, &weights, &rows).unwrap();
        for (a, b) in vpsc.iter().zip(&kkt) {
            assert!((a - b).abs() < 1e-7, "vpsc={vpsc:?} kkt={kkt:?}");
        }

        // 500-node chain with box bounds and reversed/transitive constraints solves cleanly.
        let n = 500usize;
        let d: Vec<f64> = (0..n).map(|i| ((i * 37) % 100) as f64).collect();
        let w: Vec<f64> = (0..n).map(|i| 1. + (i % 5) as f64).collect();
        let mut cs = Vec::with_capacity(n + 20);
        for i in 0..n - 1 {
            cs.push(Constraint::separation(i, i + 1, 12.));
        }
        for i in (0..n - 2).step_by(7) {
            cs.push(Constraint::separation(i, i + 2, 20.));
        }
        cs.push(Constraint {
            coefficients: vec![(0, 1.)],
            rhs: 0.,
            equality: false,
        });
        let chain = project(&d, &w, &cs).unwrap();
        for i in 0..n - 1 {
            assert!(chain[i + 1] - chain[i] >= 12. - 1e-6);
        }
    }

    #[test]
    fn project_with_linear_and_mixed_compaction() {
        let lin = project_with_linear(
            &[0., 10.],
            &[2., 2.],
            &[Constraint::separation(0, 1, 4.)],
            &[-4., 4.],
        )
        .unwrap();
        assert!((lin[0] - 2.).abs() < 1e-6);
        assert!((lin[1] - 8.).abs() < 1e-6);

        // Combine L1 compaction on edge (0, 2) with 3-var distribute equality and pinned 0.
        let comp = project_compacted(
            &[0., 50., 100.],
            &[1e5, 1., 1.],
            &[
                Constraint::fixed(0, 0.),
                Constraint {
                    coefficients: vec![(0, 0.5), (1, -1.), (2, 0.5)],
                    rhs: 0.,
                    equality: true,
                },
                Constraint::separation(0, 2, 40.),
            ],
            &[],
            &[L1Term::pair(0, 2, 10.)],
        )
        .unwrap();
        assert!(comp[0].abs() < 1e-6);
        assert!((comp[1] - 0.5 * comp[2]).abs() < 1e-6);
        assert!(comp[2] < 100. - 5. && comp[2] >= 40. - 1e-6);
    }
}
