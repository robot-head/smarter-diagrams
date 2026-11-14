//! Fresh placement treats ownership and independent components as whole units.
use crate::{layout::*, model::*};
use std::collections::{BTreeMap, BTreeSet};

/// Return container node indices sorted by descending ownership depth
/// (children strictly before parents regardless of declaration order),
/// breaking ties deterministically by scene index.
pub(crate) fn containers_postorder(scene: &SceneGraph) -> Vec<usize> {
    let n = scene.nodes.len();
    let mut depth = vec![0usize; n];
    for (i, d_slot) in depth.iter_mut().enumerate() {
        let mut d = 0usize;
        let mut cur = scene.nodes[i].parent.as_deref();
        let mut steps = 0usize;
        while let Some(p) = cur {
            if steps >= n {
                break;
            }
            d += 1;
            steps += 1;
            cur = scene.node(p).and_then(|node| node.parent.as_deref());
        }
        *d_slot = d;
    }
    let mut containers: Vec<usize> = (0..n).filter(|&i| scene.nodes[i].container).collect();
    containers.sort_by_key(|&i| (std::cmp::Reverse(depth[i]), i));
    containers
}

fn owner_subtree_set(scene: &SceneGraph, owner_id: &str) -> std::collections::HashSet<usize> {
    let mut set = std::collections::HashSet::new();
    for (idx, node) in scene.nodes.iter().enumerate() {
        let mut cur = node.parent.as_deref();
        while let Some(pid) = cur {
            if pid == owner_id {
                set.insert(idx);
                break;
            }
            cur = scene.node(pid).and_then(|p| p.parent.as_deref());
        }
    }
    set
}

fn has_node_annotation(scene: &SceneGraph, idx: usize, o: &LayoutOptions, names: &[&str]) -> bool {
    let n = &scene.nodes[idx];
    effective_annotations(scene, &n.id, &n.annotations, o.variant.as_deref())
        .iter()
        .any(|a| names.iter().any(|k| a.name == *k))
}

fn should_skip_owner_pack(scene: &SceneGraph, owner_idx: Option<usize>, o: &LayoutOptions) -> bool {
    let owner_id = owner_idx.map(|i| scene.nodes[i].id.as_str());
    if let Some(oi) = owner_idx
        && has_node_annotation(scene, oi, o, &["lock"])
    {
        return true;
    }
    let direct_children: Vec<usize> = scene
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.parent.as_deref() == owner_id)
        .map(|(i, _)| i)
        .collect();
    if direct_children.is_empty() {
        return true;
    }
    let mut pinned_direct = 0usize;
    let mut has_relative_pin = false;
    for &ci in &direct_children {
        let cn = &scene.nodes[ci];
        let anns = effective_annotations(scene, &cn.id, &cn.annotations, o.variant.as_deref());
        for a in anns {
            if a.name == "lock" {
                return true;
            }
            if a.name == "pin" {
                pinned_direct += 1;
                if a.attrs.contains_key("from")
                    || a.attrs
                        .get("x")
                        .or_else(|| a.attrs.get("y"))
                        .is_some_and(|v| v.text.contains('.'))
                {
                    has_relative_pin = true;
                }
            }
        }
    }
    if owner_idx.is_some() && (pinned_direct >= 2 || has_relative_pin) {
        return true;
    }
    if owner_idx.is_none() {
        let root_of_idx = |mut idx: usize| -> usize {
            while let Some(p) = scene.nodes[idx].parent.as_deref().and_then(|id| scene.index(id)) {
                idx = p;
            }
            idx
        };
        for (ni, n) in scene.nodes.iter().enumerate() {
            if n.parent.is_none() {
                continue;
            }
            let my_root = root_of_idx(ni);
            for a in effective_annotations(scene, &n.id, &n.annotations, o.variant.as_deref()) {
                if a.name != "pin" {
                    continue;
                }
                for key in ["x", "y", "from"] {
                    if let Some(val) = a.attrs.get(key) {
                        for tok in val.text.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-' && c != '.') {
                            let cand = tok.trim_end_matches('.');
                            if cand.is_empty() {
                                continue;
                            }
                            let mut probe = cand;
                            loop {
                                if let Some(ti) = scene.index(probe) {
                                    if root_of_idx(ti) != my_root {
                                        return true;
                                    }
                                    break;
                                }
                                let Some((prefix, _)) = probe.rsplit_once('.') else {
                                    break;
                                };
                                probe = prefix;
                            }
                        }
                    }
                }
            }
        }
    }
    let direct_set: BTreeSet<usize> = direct_children.iter().copied().collect();
    let subtree = owner_id
        .map(|id| owner_subtree_set(scene, id))
        .unwrap_or_else(|| (0..scene.nodes.len()).collect());
    for rel in &scene.relations {
        let mut in_sub = 0usize;
        let mut out_sub = 0usize;
        let mut in_direct = 0usize;
        for mid in &rel.members {
            if let Some(mi) = scene.index(mid) {
                if subtree.contains(&mi) {
                    in_sub += 1;
                } else {
                    out_sub += 1;
                }
                if direct_set.contains(&mi) {
                    in_direct += 1;
                }
            }
        }
        if owner_idx.is_some() && in_sub >= 2 && out_sub >= 1 {
            return true;
        }
        if in_direct >= 2 && rel.distribute {
            return true;
        }
    }
    false
}

#[derive(Default)]
struct MacroFlow {
    /// For each node index (by scope unit), its parent-scoped DAG rank.
    scope_rank: BTreeMap<usize, usize>,
    /// Pairs of sibling container indices arranged vertically by reciprocal feedback.
    reciprocal_vertical: BTreeSet<(usize, usize)>,
    /// Overhead leaf bridge node index in 3-root (Container <-> Leaf <-> Container) arch.
    bridge_leaf: Option<usize>,
}

fn compute_macro_flow(scene: &SceneGraph, owners: &[usize]) -> MacroFlow {
    let mut flow = MacroFlow::default();
    let mut scopes: Vec<Option<usize>> = Vec::new();
    scopes.push(None);
    for &oi in owners {
        scopes.push(Some(oi));
    }
    for scope_owner in scopes {
        let parent_id = scope_owner.map(|oi| scene.nodes[oi].id.as_str());
        let units: Vec<usize> = scene
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.parent.as_deref() == parent_id)
            .map(|(i, _)| i)
            .collect();
        if units.len() < 2 {
            continue;
        }
        let g = project(scene, &units);
        let parts = components(&g);
        for part in parts {
            let ids: BTreeSet<&str> = part.iter().map(|&i| g.nodes[i].id.as_str()).collect();
            let mut local = SceneGraph {
                nodes: part.iter().map(|&i| g.nodes[i].clone()).collect(),
                edges: g
                    .edges
                    .iter()
                    .filter(|e| {
                        ids.contains(e.source.node.as_str()) && ids.contains(e.target.node.as_str())
                    })
                    .cloned()
                    .collect(),
                ..Default::default()
            };
            local.node_index.clear();
            let sccs = crate::topology::components(&local);
            let mut scc_of = vec![0; local.nodes.len()];
            for (sid, members) in sccs.iter().enumerate() {
                for &m in members {
                    scc_of[m] = sid;
                }
            }
            let mut dag_in = vec![0i32; part.len()];
            let mut local_out = vec![0i32; part.len()];
            let mut local_in = vec![0i32; part.len()];
            for e in &local.edges {
                let (Some(u), Some(v)) = (local.index(&e.source.node), local.index(&e.target.node))
                else {
                    continue;
                };
                if u != v {
                    local_out[u] += 1;
                    local_in[v] += 1;
                    if scc_of[u] != scc_of[v] {
                        dag_in[v] += 1;
                    }
                }
            }
            let prio = |idx: usize| (dag_in[idx] > 0, local_out[idx] - local_in[idx], std::cmp::Reverse(idx));
            let mut dag = local.clone();
            dag.edges.retain(|e| {
                let (Some(u), Some(v)) = (local.index(&e.source.node), local.index(&e.target.node))
                else {
                    return false;
                };
                let sid = scc_of[u];
                !(sid == scc_of[v] && sccs[sid].len() > 1 && prio(v) > prio(u))
            });
            let ranks = crate::topology::coffman_graham(&dag, part.len().max(2));
            for (li, &ui) in part.iter().enumerate() {
                flow.scope_rank.insert(units[ui], ranks[li]);
            }
            let part_owners: Vec<usize> = part
                .iter()
                .enumerate()
                .filter(|(_, u)| scene.nodes[units[**u]].container)
                .map(|(at, _)| at)
                .collect();
            if part_owners.len() == 2 {
                let (a, b) = (part_owners[0], part_owners[1]);
                let fwd = local
                    .edges
                    .iter()
                    .any(|e| e.source.node == local.nodes[a].id && e.target.node == local.nodes[b].id);
                let rev = local
                    .edges
                    .iter()
                    .any(|e| e.source.node == local.nodes[b].id && e.target.node == local.nodes[a].id);
                let prefixes_clean = (0..part.len())
                    .filter(|i| !part_owners.contains(i))
                    .all(|i| local.edges.iter().all(|e| e.target.node != local.nodes[i].id));
                if fwd && rev && prefixes_clean {
                    flow.reciprocal_vertical
                        .insert((units[part[a]], units[part[b]]));
                }
            }
        }
        if scope_owner.is_none() && units.len() == 3 {
            let root_c = units.iter().filter(|&&i| scene.nodes[i].container).count();
            let root_l: Vec<usize> = units
                .iter()
                .copied()
                .filter(|&i| !scene.nodes[i].container)
                .collect();
            if root_c == 2 && root_l.len() == 1 {
                let lid = &scene.nodes[root_l[0]].id;
                if scene.edges.iter().any(|e| &e.target.node == lid)
                    && scene.edges.iter().any(|e| &e.source.node == lid)
                {
                    flow.bridge_leaf = Some(root_l[0]);
                }
            }
        }
    }
    flow
}

fn pinned_grid_pos(scene: &SceneGraph, idx: usize, o: &LayoutOptions) -> Option<(f64, f64)> {
    for ann in effective_annotations(
        scene,
        &scene.nodes[idx].id,
        &scene.nodes[idx].annotations,
        o.variant.as_deref(),
    ) {
        if ann.name == "pin"
            && let Some(gv) = ann.attrs.get("grid")
        {
            let mut nums = gv
                .text
                .split(',')
                .filter_map(|s| s.trim().parse::<f64>().ok());
            if let (Some(gx), Some(gy)) = (nums.next(), nums.next()) {
                return Some((gx * o.grid, gy * o.grid));
            }
        }
    }
    None
}

fn subtree_has_pin_or_lock(scene: &SceneGraph, owner_idx: usize, o: &LayoutOptions) -> bool {
    if has_node_annotation(scene, owner_idx, o, &["pin", "lock"]) {
        return true;
    }
    if !scene.nodes[owner_idx].container {
        return false;
    }
    let subtree = owner_subtree_set(scene, &scene.nodes[owner_idx].id);
    subtree
        .into_iter()
        .any(|d| has_node_annotation(scene, d, o, &["pin", "lock"]))
}

fn translate_subtree(scene: &SceneGraph, rects: &mut [Rect], root_idx: usize, dx: f64, dy: f64) {
    if dx.abs() < 1e-6 && dy.abs() < 1e-6 {
        return;
    }
    let rid = &scene.nodes[root_idx].id;
    for (j, r) in rects.iter_mut().enumerate() {
        if j == root_idx || ancestors(scene, &scene.nodes[j].id).contains(rid) {
            r.x += dx;
            r.y += dy;
        }
    }
}

pub(crate) fn pack(scene: &SceneGraph, rects: &mut [Rect], o: &LayoutOptions, spacing: f64) {
    let owners = containers_postorder(scene);
    let macro_flow = compute_macro_flow(scene, &owners);
    for &owner in &owners {
        let id = &scene.nodes[owner].id;
        let children: Vec<_> = scene
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.parent.as_ref() == Some(id))
            .map(|(i, _)| i)
            .collect();
        if children.is_empty() {
            continue;
        }
        if should_skip_owner_pack(scene, Some(owner), o) {
            if !has_node_annotation(scene, owner, o, &["lock", "pin"]) {
                for &c in &children {
                    if let Some((px, py)) = pinned_grid_pos(scene, c, o) {
                        rects[c].x = px;
                        rects[c].y = py;
                    }
                }
                let bounds = children
                    .iter()
                    .map(|&i| with_top_extra(&scene.nodes[i], rects[i]))
                    .reduce(Rect::union)
                    .unwrap();
                let header = header_height(&scene.nodes[owner]);
                rects[owner] = Rect::new(
                    bounds.x,
                    bounds.y - header / 2.,
                    (bounds.w + 2. * o.container_padding).max(title_width(&scene.nodes[owner])),
                    bounds.h + 2. * o.container_padding + header,
                );
            }
            continue;
        }
        let owner_pinned = has_node_annotation(scene, owner, o, &["pin"]);
        let orig_owner_center = (rects[owner].x, rects[owner].y);
        group(scene, rects, o, &children, Some(owner), &macro_flow, spacing);
        let bounds = children
            .iter()
            .map(|&i| with_top_extra(&scene.nodes[i], rects[i]))
            .reduce(Rect::union)
            .unwrap();
        let header = header_height(&scene.nodes[owner]);
        rects[owner] = Rect::new(
            bounds.x,
            bounds.y - header / 2.,
            (bounds.w + 2. * o.container_padding).max(title_width(&scene.nodes[owner])),
            bounds.h + 2. * o.container_padding + header,
        );
        if owner_pinned {
            let dx = orig_owner_center.0 - rects[owner].x;
            let dy = orig_owner_center.1 - rects[owner].y;
            translate_subtree(scene, rects, owner, dx, dy);
        }
    }
    let roots: Vec<_> = scene
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.parent.is_none())
        .map(|(i, _)| i)
        .collect();
    if roots.len() >= 2 && !should_skip_owner_pack(scene, None, o) {
        let projected = project(scene, &roots);
        if !owners.is_empty() || components(&projected).len() > 1 {
            group(scene, rects, o, &roots, None, &macro_flow, spacing);
        }
    }
    if !owners.is_empty() {
        align_container_columns(scene, rects, o);
    }
}
fn align_container_columns(scene: &SceneGraph, rects: &mut [Rect], o: &LayoutOptions) {
    let rel_nodes: BTreeSet<&str> = scene
        .relations
        .iter()
        .flat_map(|r| r.members.iter().map(String::as_str))
        .collect();
    let mut columns: Vec<Vec<usize>> = scene
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.container)
        .filter_map(|(oi, owner)| {
            if has_node_annotation(scene, oi, o, &["lock", "pin"]) {
                return None;
            }
            let children: Vec<usize> = scene
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, c)| c.parent.as_ref() == Some(&owner.id))
                .map(|(i, _)| i)
                .collect();
            if children.len() < 2
                || children.iter().any(|&i| {
                    scene.nodes[i].container
                        || rel_nodes.contains(scene.nodes[i].id.as_str())
                        || has_node_annotation(scene, i, o, &["lock", "pin"])
                })
            {
                return None;
            }
            let r0 = rects[children[0]];
            if children
                .iter()
                .any(|&i| rects[i].left() >= r0.right() || rects[i].right() <= r0.left())
            {
                return None;
            }
            let child_ids: BTreeSet<&str> = children
                .iter()
                .map(|&i| scene.nodes[i].id.as_str())
                .collect();
            if scene.edges.iter().any(|e| {
                child_ids.contains(e.source.node.as_str())
                    && child_ids.contains(e.target.node.as_str())
            }) {
                return None;
            }
            Some(children)
        })
        .collect();
    let mut root_by_x: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    for (i, n) in scene.nodes.iter().enumerate() {
        if n.parent.is_none() && !n.container && !rel_nodes.contains(n.id.as_str()) {
            root_by_x
                .entry((rects[i].x * 16.).round() as i64)
                .or_default()
                .push(i);
        }
    }
    for group_col in root_by_x.into_values() {
        if group_col.len() < 2 {
            continue;
        }
        let ids: BTreeSet<&str> = group_col
            .iter()
            .map(|&i| scene.nodes[i].id.as_str())
            .collect();
        if !scene
            .edges
            .iter()
            .any(|e| ids.contains(e.source.node.as_str()) && ids.contains(e.target.node.as_str()))
        {
            columns.push(group_col);
        }
    }
    let endpoint_y = |ep: &Endpoint, rects: &[Rect]| -> Option<f64> {
        let idx = scene.index(&ep.node)?;
        let r = rects[idx];
        if let Some(port_id) = &ep.port
            && let Some(port) = scene.nodes[idx].ports.get(port_id)
        {
            match port.constraint {
                PortConstraint::FixedRatio {
                    side: Side::W | Side::E,
                    ratio,
                } => return Some(r.top() + ratio * r.h),
                PortConstraint::FixedPos {
                    side: Side::W | Side::E,
                    offset_grid,
                } => return Some(r.top() + offset_grid * o.grid),
                _ => {}
            }
        }
        Some(r.y)
    };
    let horiz_ep = |ep: &Endpoint| !matches!(ep.constraint.side(), Some(Side::N) | Some(Side::S));
    for sweep in 0..4 {
        let iter: Box<dyn Iterator<Item = &Vec<usize>>> = if sweep % 2 == 0 {
            Box::new(columns.iter())
        } else {
            Box::new(columns.iter().rev())
        };
        for children in iter {
            let mut current = children.clone();
            current.sort_by(|&a, &b| rects[a].y.total_cmp(&rects[b].y));
            let top = current
                .iter()
                .map(|&i| with_top_extra(&scene.nodes[i], rects[i]).top())
                .fold(f64::INFINITY, f64::min);
            let bottom = current
                .iter()
                .map(|&i| rects[i].bottom())
                .fold(f64::NEG_INFINITY, f64::max);
            let total_body: f64 = current
                .iter()
                .map(|&i| rects[i].h + top_extra(&scene.nodes[i]))
                .sum();
            let gap = if current.len() > 1 {
                ((bottom - top - total_body) / (current.len() - 1) as f64).max(24.)
            } else {
                0.
            };
            let mut keyed: Vec<(f64, usize, usize)> = current
                .iter()
                .enumerate()
                .map(|(slot, &i)| {
                    let id = &scene.nodes[i].id;
                    let mut ys = Vec::new();
                    for e in &scene.edges {
                        if &e.source.node == id {
                            if let Some(y) = endpoint_y(&e.target, rects) {
                                ys.push(y);
                            }
                        } else if &e.target.node == id
                            && let Some(y) = endpoint_y(&e.source, rects)
                        {
                            ys.push(y);
                        }
                    }
                    let bary = if ys.is_empty() {
                        rects[i].y
                    } else {
                        ys.iter().sum::<f64>() / ys.len() as f64
                    };
                    (bary, slot, i)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            let mut cursor = top;
            for (_, _, i) in keyed {
                let extra = top_extra(&scene.nodes[i]);
                rects[i].y = cursor + extra + rects[i].h / 2.;
                cursor += extra + rects[i].h + gap;
            }
        }
    }
    let flow_pairs: BTreeSet<(&str, &str)> = scene
        .flows
        .iter()
        .flat_map(|f| {
            f.steps.windows(2).map(|w| {
                let a = w[0].split('#').next().unwrap_or(&w[0]);
                let b = w[1].split('#').next().unwrap_or(&w[1]);
                (a, b)
            })
        })
        .collect();
    // Align single-node columns inside multi-column containers along declared flows, shared
    // zones, or external horizontal peers so primary spines stay straight without taller boxes.
    for _ in 0..2 {
        for (owner_idx, owner) in scene.nodes.iter().enumerate() {
            if !owner.container {
                continue;
            }
            let children: Vec<usize> = scene
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, c)| c.parent.as_ref() == Some(&owner.id))
                .map(|(i, _)| i)
                .collect();
            if children.len() < 3
                || children.iter().any(|&i| {
                    scene.nodes[i].container || rel_nodes.contains(scene.nodes[i].id.as_str())
                })
            {
                continue;
            }
            let mut cols: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
            for &i in &children {
                cols.entry((rects[i].x * 4.).round() as i64)
                    .or_default()
                    .push(i);
            }
            if cols.len() < 2 || !cols.values().any(|c| c.len() >= 2) {
                continue;
            }
            let min_top = children
                .iter()
                .map(|&i| with_top_extra(&scene.nodes[i], rects[i]).top())
                .fold(f64::INFINITY, f64::min);
            let max_bot = children
                .iter()
                .map(|&i| rects[i].bottom())
                .fold(f64::NEG_INFINITY, f64::max);
            let child_set: BTreeSet<&str> = children
                .iter()
                .map(|&i| scene.nodes[i].id.as_str())
                .collect();
            let owner_rect = children
                .iter()
                .map(|&i| rects[i])
                .reduce(Rect::union)
                .unwrap_or_default();
            for col in cols.into_values() {
                if col.len() != 1 {
                    continue;
                }
                let u = col[0];
                let uid = scene.nodes[u].id.as_str();
                let mut internal_peer_counts: BTreeMap<&str, usize> = BTreeMap::new();
                for e in &scene.edges {
                    if e.source.node != e.target.node {
                        if e.source.node == uid && child_set.contains(e.target.node.as_str()) {
                            *internal_peer_counts
                                .entry(e.target.node.as_str())
                                .or_default() += 1;
                        } else if e.target.node == uid && child_set.contains(e.source.node.as_str())
                        {
                            *internal_peer_counts
                                .entry(e.source.node.as_str())
                                .or_default() += 1;
                        }
                    }
                }
                if internal_peer_counts.values().any(|&c| c >= 2) {
                    continue;
                }
                let lo = min_top + top_extra(&scene.nodes[u]) + rects[u].h / 2.;
                let hi = max_bot - rects[u].h / 2.;
                if lo > hi + 1e-6 {
                    continue;
                }
                let horiz_peer_ok = |peer_id: &str, y: f64| -> bool {
                    let Some(pi) = scene.index(peer_id) else {
                        return false;
                    };
                    let pr = rects[pi];
                    let x_sep = pr.left() - owner_rect.right() >= 2. * o.stub
                        || owner_rect.left() - pr.right() >= 2. * o.stub;
                    x_sep && y >= owner_rect.top() - 32. && y <= owner_rect.bottom() + 32.
                };
                let mut targets = Vec::new();
                for e in &scene.edges {
                    if e.source.node == uid && !child_set.contains(e.target.node.as_str()) {
                        if horiz_ep(&e.source)
                            && horiz_ep(&e.target)
                            && let Some(y) = endpoint_y(&e.target, rects)
                            && horiz_peer_ok(&e.target.node, y)
                        {
                            targets.push(y);
                        }
                    } else if e.target.node == uid && !child_set.contains(e.source.node.as_str()) {
                        if horiz_ep(&e.source)
                            && horiz_ep(&e.target)
                            && let Some(y) = endpoint_y(&e.source, rects)
                            && horiz_peer_ok(&e.source.node, y)
                        {
                            targets.push(y);
                        }
                    } else if e.source.node == uid
                        && child_set.contains(e.target.node.as_str())
                        && flow_pairs.contains(&(uid, e.target.node.as_str()))
                        && let Some(y) = endpoint_y(&e.target, rects)
                    {
                        targets.push(y);
                    } else if e.target.node == uid
                        && child_set.contains(e.source.node.as_str())
                        && flow_pairs.contains(&(e.source.node.as_str(), uid))
                        && let Some(y) = endpoint_y(&e.source, rects)
                    {
                        targets.push(y);
                    }
                }
                for z in &scene.zones {
                    if z.members.iter().any(|m| m == uid) {
                        for &peer in &children {
                            if peer != u && z.members.iter().any(|m| m == &scene.nodes[peer].id) {
                                targets.push(rects[peer].y);
                            }
                        }
                    }
                }
                if !targets.is_empty() {
                    let avg = targets.iter().sum::<f64>() / targets.len() as f64;
                    rects[u].y = avg.clamp(lo, hi);
                }
            }
            let _ = owner_idx;
        }
    }
    // Shift root containers vertically within a local window when doing so straightens
    // horizontal cross-container connections (including sql_table record slots).
    let mut roots: Vec<usize> = scene
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.parent.is_none() && n.container && !rel_nodes.contains(n.id.as_str()))
        .map(|(i, _)| i)
        .collect();
    roots.sort_by(|&a, &b| rects[a].x.total_cmp(&rects[b].x));
    if roots.len() >= 2 {
        let root_of = |id: &str| -> Option<usize> {
            let mut cur = scene.index(id)?;
            while let Some(p) = scene.nodes[cur].parent.as_deref() {
                cur = scene.index(p)?;
            }
            Some(cur)
        };
        for pass in 0..2 {
            let order: Vec<usize> = if pass % 2 == 0 {
                roots.clone()
            } else {
                roots.iter().rev().copied().collect()
            };
            for r_idx in order {
                let mut deltas = Vec::new();
                for e in &scene.edges {
                    if !horiz_ep(&e.source) || !horiz_ep(&e.target) {
                        continue;
                    }
                    let (Some(ra), Some(rb)) = (root_of(&e.source.node), root_of(&e.target.node))
                    else {
                        continue;
                    };
                    if ra == rb {
                        continue;
                    }
                    let x_apart = rects[rb].left() - rects[ra].right() >= 2. * o.stub
                        || rects[ra].left() - rects[rb].right() >= 2. * o.stub;
                    if !x_apart {
                        continue;
                    }
                    let (Some(ya), Some(yb)) =
                        (endpoint_y(&e.source, rects), endpoint_y(&e.target, rects))
                    else {
                        continue;
                    };
                    if ra == r_idx {
                        deltas.push(yb - ya);
                    } else if rb == r_idx {
                        deltas.push(ya - yb);
                    }
                }
                if deltas.is_empty() {
                    continue;
                }
                deltas.sort_by(f64::total_cmp);
                let shift = deltas[deltas.len() / 2].clamp(-96., 96.);
                if shift.abs() < 1. {
                    continue;
                }
                let moved = Rect::new(
                    rects[r_idx].x,
                    rects[r_idx].y + shift,
                    rects[r_idx].w,
                    rects[r_idx].h,
                );
                let collides = roots.iter().any(|&other| {
                    other != r_idx && moved.inflate(24.).intersects(rects[other].inflate(24.))
                });
                if !collides {
                    let rid = &scene.nodes[r_idx].id;
                    for (j, r) in rects.iter_mut().enumerate() {
                        if j == r_idx || ancestors(scene, &scene.nodes[j].id).contains(rid) {
                            r.y += shift;
                        }
                    }
                }
            }
        }
    }
}
#[derive(Clone)]
struct BorderAnchor {
    unit_pos: usize,
    side: Side,
    outgoing: bool,
    explicit_side: bool,
    ext_key: String,
    hint_pos: f64,
}

#[derive(Default)]
struct BorderProjection {
    anchors: Vec<BorderAnchor>,
    /// Keyed by `(u_unit_pos, v_unit_pos, directed_edge_idx_in_local)` -> `(src_frac, dst_frac)` in `0.0..=1.0`.
    internal_fracs: BTreeMap<(usize, usize), Vec<(f64, f64)>>,
}

fn opposite_side(s: Side) -> Side {
    match s {
        Side::N => Side::S,
        Side::S => Side::N,
        Side::E => Side::W,
        Side::W => Side::E,
    }
}

fn flow_is_vertical(scene: &SceneGraph, container_idx: Option<usize>, o: &LayoutOptions) -> bool {
    let Some(ci) = container_idx else {
        return false;
    };
    let n = &scene.nodes[ci];
    if n.attrs
        .get("flow")
        .or_else(|| n.attrs.get("direction"))
        .is_some_and(|v| matches!(v.text.as_str(), "TB" | "TD" | "BT" | "vertical"))
    {
        return true;
    }
    effective_annotations(scene, &n.id, &n.annotations, o.variant.as_deref())
        .iter()
        .any(|a| {
            matches!(a.name.as_str(), "flow" | "direction")
                && a.attrs
                    .get("dir")
                    .or_else(|| a.attrs.get("direction"))
                    .or_else(|| a.attrs.values().next())
                    .is_some_and(|v| matches!(v.text.as_str(), "TB" | "TD" | "BT" | "vertical"))
        })
}

fn endpoint_y_fraction(scene: &SceneGraph, rects: &[Rect], unit_idx: usize, node_id: &str) -> f64 {
    let Some(ni) = scene.index(node_id) else {
        return 0.5;
    };
    if ni == unit_idx {
        return 0.5;
    }
    let ur = rects[unit_idx];
    if ur.h <= 1.0 {
        return 0.5;
    }
    ((rects[ni].y - ur.top()) / ur.h).clamp(0.05, 0.95)
}

fn ancestor_pair_sharing_parent(
    scene: &SceneGraph,
    a_id: &str,
    b_id: &str,
) -> Option<(usize, usize, Option<usize>)> {
    let mut a_chain = Vec::new();
    let mut cur = Some(a_id);
    while let Some(id) = cur {
        let idx = scene.index(id)?;
        let parent = scene.nodes[idx].parent.as_deref().and_then(|p| scene.index(p));
        a_chain.push((idx, parent));
        cur = scene.nodes[idx].parent.as_deref();
    }
    let mut b_cur = Some(b_id);
    while let Some(id) = b_cur {
        let b_idx = scene.index(id)?;
        let b_parent = scene.nodes[b_idx].parent.as_deref().and_then(|p| scene.index(p));
        for &(a_idx, a_parent) in &a_chain {
            if a_parent == b_parent && a_idx != b_idx {
                return Some((a_idx, b_idx, a_parent));
            }
        }
        b_cur = scene.nodes[b_idx].parent.as_deref();
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn macro_border_side(
    scene: &SceneGraph,
    owner_idx: usize,
    internal_ep: &Endpoint,
    external_ep: &Endpoint,
    outgoing: bool,
    macro_flow: &MacroFlow,
    rects: &[Rect],
    o: &LayoutOptions,
) -> (Side, f64) {
    let ext_hint = if let Some(ext_i) = scene.index(&external_ep.node) {
        let r = rects[ext_i];
        if r.y.abs() > 1e-3 {
            (0.5 + (r.y - rects[owner_idx].y) / 600.0).clamp(0.05, 0.95)
        } else {
            (ext_i as f64 + 0.5) / scene.nodes.len().max(1) as f64
        }
    } else {
        0.5
    };
    if let Some(side) = internal_ep.constraint.side() {
        return (side, ext_hint);
    }
    if let Some(ext_side) = external_ep.constraint.side() {
        return (opposite_side(ext_side), ext_hint);
    }
    let owner_id = &scene.nodes[owner_idx].id;
    if let Some((a_anc, b_anc, shared_parent)) =
        ancestor_pair_sharing_parent(scene, owner_id, &external_ep.node)
    {
        if macro_flow.bridge_leaf == Some(b_anc) {
            return (Side::N, ext_hint);
        }
        if macro_flow.reciprocal_vertical.contains(&(a_anc, b_anc)) {
            return (Side::S, ext_hint);
        }
        if macro_flow.reciprocal_vertical.contains(&(b_anc, a_anc)) {
            return (Side::N, ext_hint);
        }
        let vertical_parent = flow_is_vertical(scene, shared_parent, o);
        if let (Some(&rk_a), Some(&rk_b)) = (
            macro_flow.scope_rank.get(&a_anc),
            macro_flow.scope_rank.get(&b_anc),
        ) {
            if rk_b < rk_a {
                return (if vertical_parent { Side::N } else { Side::W }, ext_hint);
            }
            if rk_b > rk_a {
                return (if vertical_parent { Side::S } else { Side::E }, ext_hint);
            }
            return if b_anc < a_anc {
                (Side::N, ext_hint)
            } else {
                (Side::S, ext_hint)
            };
        }
    }
    (if outgoing { Side::E } else { Side::W }, ext_hint)
}

fn project_border_info(
    scene: &SceneGraph,
    part_units: &[usize],
    rects: &[Rect],
    owner_idx: Option<usize>,
    macro_flow: &MacroFlow,
    o: &LayoutOptions,
) -> BorderProjection {
    let mut info = BorderProjection::default();
    let unit_for_node = |id: &str| -> Option<usize> {
        let mut i = scene.index(id)?;
        loop {
            if let Some(pos) = part_units.iter().position(|&j| i == j) {
                return Some(pos);
            }
            i = scene.nodes[i].parent.as_deref().and_then(|p| scene.index(p))?;
        }
    };
    let mut seen_anchors: BTreeMap<(usize, u8, bool, String), usize> = BTreeMap::new();
    for edge in &scene.edges {
        let su = unit_for_node(&edge.source.node);
        let tu = unit_for_node(&edge.target.node);
        match (su, tu) {
            (Some(a), Some(b)) if a != b => {
                let sf = endpoint_y_fraction(scene, rects, part_units[a], &edge.source.node);
                let df = endpoint_y_fraction(scene, rects, part_units[b], &edge.target.node);
                info.internal_fracs.entry((a, b)).or_default().push((sf, df));
            }
            (Some(a), None) => {
                if let Some(oi) = owner_idx {
                    let (side, hint_pos) = macro_border_side(
                        scene,
                        oi,
                        &edge.source,
                        &edge.target,
                        true,
                        macro_flow,
                        rects,
                        o,
                    );
                    let ext_key = scene
                        .node(&edge.target.node)
                        .and_then(|n| n.parent.clone())
                        .unwrap_or_else(|| edge.target.node.clone());
                    let side_code = match side {
                        Side::N => 0,
                        Side::S => 1,
                        Side::W => 2,
                        Side::E => 3,
                    };
                    let key = (a, side_code, true, ext_key.clone());
                    if let std::collections::btree_map::Entry::Vacant(e) = seen_anchors.entry(key) {
                        e.insert(info.anchors.len());
                        info.anchors.push(BorderAnchor {
                            unit_pos: a,
                            side,
                            outgoing: true,
                            explicit_side: edge.source.constraint.side().is_some(),
                            ext_key,
                            hint_pos,
                        });
                    }
                }
            }
            (None, Some(b)) => {
                if let Some(oi) = owner_idx {
                    let (side, hint_pos) = macro_border_side(
                        scene,
                        oi,
                        &edge.target,
                        &edge.source,
                        false,
                        macro_flow,
                        rects,
                        o,
                    );
                    let ext_key = scene
                        .node(&edge.source.node)
                        .and_then(|n| n.parent.clone())
                        .unwrap_or_else(|| edge.source.node.clone());
                    let side_code = match side {
                        Side::N => 0,
                        Side::S => 1,
                        Side::W => 2,
                        Side::E => 3,
                    };
                    let key = (b, side_code, false, ext_key.clone());
                    if let std::collections::btree_map::Entry::Vacant(e) = seen_anchors.entry(key) {
                        e.insert(info.anchors.len());
                        info.anchors.push(BorderAnchor {
                            unit_pos: b,
                            side,
                            outgoing: false,
                            explicit_side: edge.target.constraint.side().is_some(),
                            ext_key,
                            hint_pos,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    info.anchors
        .sort_by(|a, b| a.unit_pos.cmp(&b.unit_pos).then(a.side.cmp(&b.side)).then(a.ext_key.cmp(&b.ext_key)));
    info
}

fn constraint_reaches(
    src: usize,
    dst: usize,
    active_edges: &[(usize, usize)],
    block_of: &[usize],
    num_blocks: usize,
) -> bool {
    if src == dst {
        return true;
    }
    let mut vis = vec![false; num_blocks];
    let mut stack = vec![src];
    vis[src] = true;
    while let Some(cur) = stack.pop() {
        for &(u, v) in active_edges {
            let bu = block_of[u];
            let bv = block_of[v];
            if bu == cur && bu != bv && !vis[bv] {
                if bv == dst {
                    return true;
                }
                vis[bv] = true;
                stack.push(bv);
            }
        }
    }
    false
}

/// Forster (2005) constrained two-level barycentric block-merging sort for one rank.
fn forster_constrained_order(
    nodes_in_rank: &[usize],
    raw_bary: &BTreeMap<usize, f64>,
    weights: &BTreeMap<usize, usize>,
    constraints: &[(usize, usize)],
    tie_slot: &BTreeMap<usize, usize>,
) -> Vec<usize> {
    let k = nodes_in_rank.len();
    if k <= 1 {
        return nodes_in_rank.to_vec();
    }
    #[derive(Clone)]
    struct Block {
        members: Vec<usize>,
        sum_wb: f64,
        weight: usize,
        key: usize,
        alive: bool,
    }
    impl Block {
        fn avg(&self) -> f64 {
            self.sum_wb / self.weight.max(1) as f64
        }
    }
    let max_node = nodes_in_rank.iter().copied().max().unwrap_or(0);
    let mut in_rank = vec![false; max_node + 1];
    for &u in nodes_in_rank {
        in_rank[u] = true;
    }
    let mut active_constraints: Vec<(usize, usize)> = constraints
        .iter()
        .copied()
        .filter(|&(u, v)| u <= max_node && v <= max_node && u != v && in_rank[u] && in_rank[v])
        .collect();
    active_constraints.sort_unstable();
    active_constraints.dedup();

    let mut block_of = vec![0usize; max_node + 1];
    let mut blocks = Vec::with_capacity(k);
    for (bi, &u) in nodes_in_rank.iter().enumerate() {
        let w = weights.get(&u).copied().unwrap_or(1).max(1);
        let b = raw_bary.get(&u).copied().unwrap_or(0.0);
        let key = tie_slot.get(&u).copied().unwrap_or(bi);
        block_of[u] = bi;
        blocks.push(Block {
            members: vec![u],
            sum_wb: b * w as f64,
            weight: w,
            key,
            alive: true,
        });
    }
    loop {
        let mut merge_pair: Option<(usize, usize)> = None;
        for idx in 0..active_constraints.len() {
            let (u, v) = active_constraints[idx];
            let bu = block_of[u];
            let bv = block_of[v];
            if bu == bv || !blocks[bu].alive || !blocks[bv].alive {
                continue;
            }
            let av_u = blocks[bu].avg();
            let av_v = blocks[bv].avg();
            let violated = av_u > av_v + 1e-9
                || ((av_u - av_v).abs() <= 1e-9 && blocks[bu].key > blocks[bv].key);
            if violated {
                if constraint_reaches(bv, bu, &active_constraints, &block_of, blocks.len()) {
                    continue;
                }
                merge_pair = Some((bu, bv));
                break;
            }
        }
        let Some((bu, bv)) = merge_pair else {
            break;
        };
        let bv_members = std::mem::take(&mut blocks[bv].members);
        for &m in &bv_members {
            block_of[m] = bu;
        }
        blocks[bu].members.extend(bv_members);
        blocks[bu].sum_wb += blocks[bv].sum_wb;
        blocks[bu].weight += blocks[bv].weight;
        blocks[bu].key = blocks[bu].key.min(blocks[bv].key);
        blocks[bv].alive = false;
    }
    let mut live: Vec<Block> = blocks.into_iter().filter(|b| b.alive).collect();
    live.sort_by(|a, b| a.avg().total_cmp(&b.avg()).then(a.key.cmp(&b.key)));
    live.into_iter().flat_map(|b| b.members).collect()
}

fn project(scene: &SceneGraph, units: &[usize]) -> SceneGraph {
    let mut graph = SceneGraph {
        nodes: units
            .iter()
            .map(|&i| {
                let mut n = scene.nodes[i].clone();
                n.parent = None;
                n.container = false;
                n.ports.clear();
                n.annotations.clear();
                n
            })
            .collect(),
        ..Default::default()
    };
    let root_for = |id: &str| -> Option<usize> {
        let mut i = scene.index(id)?;
        loop {
            if let Some(unit) = units.iter().position(|&j| i == j) {
                return Some(unit);
            }
            i = scene.nodes[i]
                .parent
                .as_deref()
                .and_then(|p| scene.index(p))?;
        }
    };
    for edge in &scene.edges {
        let (Some(a), Some(b)) = (root_for(&edge.source.node), root_for(&edge.target.node)) else {
            continue;
        };
        if a == b {
            continue;
        }
        let mut e = edge.clone();
        for (endpoint, index) in [(&mut e.source, a), (&mut e.target, b)] {
            endpoint.node = graph.nodes[index].id.clone();
            endpoint.port = None;
            endpoint.constraint = PortConstraint::Free;
            endpoint.raw_constraint = PortConstraint::Free;
        }
        e.annotations.clear();
        graph.edges.push(e);
    }
    graph
}
fn components(graph: &SceneGraph) -> Vec<Vec<usize>> {
    let mut neighbors = vec![Vec::new(); graph.nodes.len()];
    for e in &graph.edges {
        let a = graph.index(&e.source.node).unwrap();
        let b = graph.index(&e.target.node).unwrap();
        neighbors[a].push(b);
        neighbors[b].push(a);
    }
    let mut seen = BTreeSet::new();
    let mut parts = Vec::new();
    for i in 0..graph.nodes.len() {
        if seen.contains(&i) {
            continue;
        }
        let mut stack = vec![i];
        let mut part = Vec::new();
        while let Some(i) = stack.pop() {
            if seen.insert(i) {
                part.push(i);
                stack.extend(&neighbors[i]);
            }
        }
        part.sort_unstable();
        parts.push(part);
    }
    parts
}
fn build_forster_constraints(
    scene: &SceneGraph,
    units: &[usize],
    part: &[usize],
    ranks: &[usize],
    rects: &[Rect],
    o: &LayoutOptions,
) -> Vec<(usize, usize)> {
    let mut constraints = Vec::new();
    let pinned: Vec<(usize, f64, usize)> = part
        .iter()
        .enumerate()
        .filter_map(|(li, &ui)| {
            let gi = units[ui];
            has_node_annotation(scene, gi, o, &["pin", "lock"]).then_some((
                ranks[li],
                rects[gi].y,
                li,
            ))
        })
        .collect();
    for a in 0..pinned.len() {
        for b in (a + 1)..pinned.len() {
            if pinned[a].0 != pinned[b].0 {
                continue;
            }
            if pinned[a].1 < pinned[b].1 - 1e-6 {
                constraints.push((pinned[a].2, pinned[b].2));
            } else if pinned[b].1 < pinned[a].1 - 1e-6 {
                constraints.push((pinned[b].2, pinned[a].2));
            }
        }
    }
    let pos_of_id = |id: &str| -> Option<usize> {
        part.iter().position(|&ui| scene.nodes[units[ui]].id == id)
    };
    for rel in &scene.relations {
        if rel.distribute && rel.axis == Axis::Col {
            let mapped: Vec<usize> = rel.members.iter().filter_map(|m| pos_of_id(m)).collect();
            for w in mapped.windows(2) {
                if ranks[w[0]] == ranks[w[1]] && w[0] != w[1] {
                    constraints.push((w[0], w[1]));
                }
            }
        }
    }
    constraints
}

fn constraint_path_exists(src: usize, dst: usize, constraints: &[(usize, usize)]) -> bool {
    if src == dst {
        return true;
    }
    let mut seen = BTreeSet::new();
    let mut stack = vec![src];
    seen.insert(src);
    while let Some(cur) = stack.pop() {
        for &(u, v) in constraints {
            if u == cur && seen.insert(v) {
                if v == dst {
                    return true;
                }
                stack.push(v);
            }
        }
    }
    false
}

fn count_two_level_crossings(
    order: &[usize],
    adj_slot: &BTreeMap<usize, Vec<(f64, f64)>>,
) -> usize {
    let mut pairs: Vec<(usize, f64)> = Vec::new();
    for (slot_u, &u) in order.iter().enumerate() {
        if let Some(targets) = adj_slot.get(&u) {
            for &(_, tgt) in targets {
                pairs.push((slot_u, tgt));
            }
        }
    }
    let mut crosses = 0usize;
    for i in 0..pairs.len() {
        for j in (i + 1)..pairs.len() {
            if pairs[i].0 < pairs[j].0 && pairs[j].1 + 1e-6 < pairs[i].1 {
                crosses += 1;
            }
        }
    }
    crosses
}

fn run_forster_sweeps(
    local: &SceneGraph,
    ranks: &[usize],
    border: &BorderProjection,
    constraints: &[(usize, usize)],
) -> BTreeMap<usize, Vec<usize>> {
    let mut order_in_rank: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (li, &rk) in ranks.iter().enumerate() {
        order_in_rank.entry(rk).or_default().push(li);
    }
    let rank_keys: Vec<usize> = order_in_rank.keys().copied().collect();
    if rank_keys.is_empty() {
        return order_in_rank;
    }
    let mut base_edges: Vec<(usize, usize, f64, f64)> = Vec::new();
    let mut seen_pair_count: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    for e in &local.edges {
        let (Some(u), Some(v)) = (local.index(&e.source.node), local.index(&e.target.node)) else {
            continue;
        };
        if u == v || ranks[u] == ranks[v] {
            continue;
        }
        let occ = seen_pair_count.entry((u, v)).or_default();
        let (sf, df) = border
            .internal_fracs
            .get(&(u, v))
            .and_then(|list| list.get(*occ))
            .copied()
            .unwrap_or((0.5, 0.5));
        *occ += 1;
        base_edges.push((u, v, sf, df));
    }

    let min_rk = rank_keys[0];
    let max_rk = *rank_keys.last().unwrap();
    let sweeps = if rank_keys.len() >= 2 { 8 } else { 1 };
    let mut slot_norm: BTreeMap<usize, f64> = BTreeMap::new();

    for sweep in 0..sweeps {
        let r_seq: Vec<usize> = if sweep % 2 == 0 {
            rank_keys.clone()
        } else {
            rank_keys.iter().copied().rev().collect()
        };
        for &rk in &r_seq {
            slot_norm.clear();
            for (&k, list) in &order_in_rank {
                let len = list.len().max(1) as f64;
                for (pos, &node_i) in list.iter().enumerate() {
                    slot_norm.insert(node_i, (pos as f64 + 0.5) / len);
                    let _ = k;
                }
            }
            let cur_list = &order_in_rank[&rk];
            if cur_list.len() <= 1 {
                continue;
            }
            let prefer_left = sweep % 2 == 0 && rk > min_rk;
            let prefer_right = sweep % 2 == 1 && rk < max_rk;
            let mut raw_bary = BTreeMap::new();
            let mut weights = BTreeMap::new();
            let mut tie_slot = BTreeMap::new();
            let mut adj_targets: BTreeMap<usize, Vec<(f64, f64)>> = BTreeMap::new();

            for (pos, &u) in cur_list.iter().enumerate() {
                tie_slot.insert(u, pos);
                let mut sum_dir = 0.0;
                let mut w_dir = 0usize;
                let mut sum_all = 0.0;
                let mut w_all = 0usize;
                let mut targets = Vec::new();
                for &(su, tv, sf, df) in &base_edges {
                    let (other, self_f, other_f) = if su == u {
                        (tv, sf, df)
                    } else if tv == u {
                        (su, df, sf)
                    } else {
                        continue;
                    };
                    let other_pos = slot_norm[&other] + (other_f - 0.5) * 0.35 - (self_f - 0.5) * 0.25;
                    let on_pref = (prefer_left && ranks[other] < rk)
                        || (prefer_right && ranks[other] > rk);
                    if on_pref {
                        sum_dir += other_pos;
                        w_dir += 1;
                        targets.push((self_f, other_pos));
                    }
                    sum_all += other_pos;
                    w_all += 1;
                    if !prefer_left && !prefer_right {
                        targets.push((self_f, other_pos));
                    }
                }
                for a in &border.anchors {
                    if a.unit_pos != u {
                        continue;
                    }
                    let (anchor_pos, w_a) = match a.side {
                        Side::N => (-0.35, 2usize),
                        Side::S => (1.35, 2usize),
                        Side::W | Side::E => (a.hint_pos, 1usize),
                    };
                    let on_boundary = matches!(a.side, Side::N | Side::S)
                        || (a.side == Side::W && rk == min_rk)
                        || (a.side == Side::E && rk == max_rk);
                    if on_boundary || rank_keys.len() == 1 {
                        sum_dir += anchor_pos * w_a as f64;
                        w_dir += w_a;
                        targets.push((0.5, anchor_pos));
                    }
                    sum_all += anchor_pos * w_a as f64;
                    w_all += w_a;
                }
                let (bary, wt) = if w_dir > 0 {
                    (sum_dir / w_dir as f64, w_dir)
                } else if w_all > 0 {
                    (sum_all / w_all as f64, w_all)
                } else {
                    (slot_norm[&u], 1)
                };
                raw_bary.insert(u, bary);
                weights.insert(u, wt);
                adj_targets.insert(u, targets);
            }

            let mut ordered =
                forster_constrained_order(cur_list, &raw_bary, &weights, constraints, &tie_slot);

            // Constrained adjacent-exchange pass to remove remaining local two-level inversions.
            let mut improved = true;
            let mut iters = 0usize;
            while improved && iters < ordered.len() * 2 {
                improved = false;
                iters += 1;
                for p in 0..(ordered.len().saturating_sub(1)) {
                    let a = ordered[p];
                    let b = ordered[p + 1];
                    if constraint_path_exists(a, b, constraints) {
                        continue;
                    }
                    let before = count_two_level_crossings(&ordered, &adj_targets);
                    ordered.swap(p, p + 1);
                    let after = count_two_level_crossings(&ordered, &adj_targets);
                    if after < before {
                        improved = true;
                    } else {
                        ordered.swap(p, p + 1);
                    }
                }
            }
            order_in_rank.insert(rk, ordered);
        }
    }
    order_in_rank
}

fn apply_boundary_slack_alignment(
    ranking_graph: &SceneGraph,
    ranks: &mut [usize],
    border: &BorderProjection,
) {
    let max_rk = ranks.iter().copied().max().unwrap_or(0);
    if max_rk == 0 {
        return;
    }
    let n = ranks.len();
    let mut in_deg = vec![0usize; n];
    let mut out_deg = vec![0usize; n];
    for e in &ranking_graph.edges {
        let (Some(u), Some(v)) = (
            ranking_graph.index(&e.source.node),
            ranking_graph.index(&e.target.node),
        ) else {
            continue;
        };
        if u != v {
            out_deg[u] += 1;
            in_deg[v] += 1;
        }
    }
    let mut has_w = vec![false; n];
    let mut has_e = vec![false; n];
    for a in &border.anchors {
        if a.unit_pos < n {
            match a.side {
                Side::W => has_w[a.unit_pos] = true,
                Side::E => has_e[a.unit_pos] = true,
                _ => {}
            }
        }
    }
    for i in 0..n {
        if out_deg[i] == 0 && has_e[i] && !has_w[i] && ranks[i] < max_rk {
            ranks[i] = max_rk;
        } else if in_deg[i] == 0 && has_w[i] && !has_e[i] && ranks[i] > 0 {
            ranks[i] = 0;
        }
    }
}

fn group(
    scene: &SceneGraph,
    rects: &mut [Rect],
    o: &LayoutOptions,
    units: &[usize],
    owner_idx: Option<usize>,
    macro_flow: &MacroFlow,
    spacing: f64,
) {
    let graph = project(scene, units);
    let mut bottom = 0.;
    let mut placed_parts: Vec<(BTreeSet<String>, Rect)> = Vec::new();
    let root_id_of = |id: &str| -> Option<String> {
        let mut i = scene.index(id)?;
        loop {
            if units.contains(&i) {
                return Some(scene.nodes[i].id.clone());
            }
            i = scene.nodes[i].parent.as_deref().and_then(|p| scene.index(p))?;
        }
    };
    let mut row_coupled: BTreeSet<(String, String)> = BTreeSet::new();
    for rel in &scene.relations {
        if rel.axis == Axis::Row {
            let roots: Vec<String> = rel.members.iter().filter_map(|m| root_id_of(m)).collect();
            for a in 0..roots.len() {
                for b in (a + 1)..roots.len() {
                    if roots[a] != roots[b] {
                        row_coupled.insert((roots[a].clone(), roots[b].clone()));
                        row_coupled.insert((roots[b].clone(), roots[a].clone()));
                    }
                }
            }
        }
    }
    let mut parts = components(&graph);
    parts.sort_by_cached_key(|part| {
        std::cmp::Reverse(
            part.iter()
                .map(|&i| {
                    let id = &scene.nodes[units[i]].id;
                    scene
                        .nodes
                        .iter()
                        .filter(|n| n.id == *id || ancestors(scene, &n.id).contains(id))
                        .count()
                })
                .sum::<usize>(),
        )
    });
    for part in parts {
        let ids: BTreeSet<_> = part.iter().map(|&i| graph.nodes[i].id.as_str()).collect();
        let mut local = SceneGraph {
            nodes: part.iter().map(|&i| graph.nodes[i].clone()).collect(),
            edges: graph
                .edges
                .iter()
                .filter(|e| {
                    ids.contains(e.source.node.as_str()) && ids.contains(e.target.node.as_str())
                })
                .cloned()
                .collect(),
            ..Default::default()
        };
        local.node_index.clear();
        let part_units: Vec<usize> = part.iter().map(|&i| units[i]).collect();
        let border = project_border_info(scene, &part_units, rects, owner_idx, macro_flow, o);
        let mut boxes: Vec<_> = part.iter().map(|&i| rects[units[i]]).collect();
        let ranking_graph = {
            let sccs = crate::topology::components(&local);
            let mut scc_of = vec![0; local.nodes.len()];
            for (sid, members) in sccs.iter().enumerate() {
                for &m in members {
                    scc_of[m] = sid;
                }
            }
            let unit_of_node = |id: &str| -> Option<usize> {
                let mut cur = id;
                loop {
                    if let Some(pos) = part.iter().position(|&u| scene.nodes[units[u]].id == cur) {
                        return Some(pos);
                    }
                    cur = scene.node(cur)?.parent.as_deref()?;
                }
            };
            let mut ext_in = vec![0i32; part.len()];
            let mut ext_out = vec![0i32; part.len()];
            for e in &scene.edges {
                let su = unit_of_node(&e.source.node);
                let tu = unit_of_node(&e.target.node);
                match (su, tu) {
                    (None, Some(t)) => ext_in[t] += 1,
                    (Some(s), None) => ext_out[s] += 1,
                    _ => {}
                }
            }
            let mut dag_in = vec![0i32; part.len()];
            let mut local_out = vec![0i32; part.len()];
            let mut local_in = vec![0i32; part.len()];
            for e in &local.edges {
                let u = local.index(&e.source.node).unwrap();
                let v = local.index(&e.target.node).unwrap();
                if u != v {
                    local_out[u] += 1;
                    local_in[v] += 1;
                    if scc_of[u] != scc_of[v] {
                        dag_in[v] += 1;
                    }
                }
            }
            let has_containers = part.iter().any(|&unit| scene.nodes[units[unit]].container);
            let priority = |idx: usize| {
                if has_containers {
                    (
                        dag_in[idx] > 0 || ext_in[idx] > 0,
                        ext_in[idx] - ext_out[idx],
                        local_out[idx] - local_in[idx],
                        std::cmp::Reverse(idx),
                    )
                } else {
                    (
                        dag_in[idx] > 0,
                        i32::from(ext_in[idx] > 0),
                        local_out[idx] - local_in[idx],
                        std::cmp::Reverse(idx),
                    )
                }
            };
            let mut dag = local.clone();
            dag.edges.retain(|e| {
                let u = local.index(&e.source.node).unwrap();
                let v = local.index(&e.target.node).unwrap();
                let sid = scc_of[u];
                !(sid == scc_of[v] && sccs[sid].len() > 1 && priority(v) > priority(u))
            });
            dag
        };
        let mut ranks = crate::topology::coffman_graham(&ranking_graph, part.len().max(2));
        apply_boundary_slack_alignment(&ranking_graph, &mut ranks, &border);
        let forster_constraints =
            build_forster_constraints(scene, units, &part, &ranks, rects, o);
        let order_in_rank =
            run_forster_sweeps(&local, &ranks, &border, &forster_constraints);
        let edge_caption_w = |e: &Edge| {
            let text = scene
                .edge(&e.id)
                .map(|orig| crate::labels::connection_text(scene, orig))
                .unwrap_or_else(|| crate::labels::connection_text(&local, e));
            crate::labels::text_width(&text)
        };
        let caption_width = local.edges.iter().map(edge_caption_w).fold(0., f64::max);
        let caption_height = local
            .edges
            .iter()
            .map(|e| {
                let text = scene
                    .edge(&e.id)
                    .map(|orig| crate::labels::connection_text(scene, orig))
                    .unwrap_or_else(|| crate::labels::connection_text(&local, e));
                crate::labels::text_height(&text)
            })
            .fold(0., f64::max);
        let mut widths = BTreeMap::<usize, f64>::new();
        let mut rank_has_container = BTreeMap::<usize, bool>::new();
        for (i, r) in boxes.iter().enumerate() {
            widths
                .entry(ranks[i])
                .and_modify(|w| *w = w.max(r.w))
                .or_insert(r.w);
            let is_c = scene.nodes[units[part[i]]].container;
            rank_has_container
                .entry(ranks[i])
                .and_modify(|c| *c |= is_c)
                .or_insert(is_c);
        }
        let has_parallel_or_loop = local.edges.iter().enumerate().any(|(ei, ea)| {
            ea.source.node == ea.target.node
                || local.edges[..ei].iter().any(|eb| {
                    (eb.source.node == ea.source.node && eb.target.node == ea.target.node)
                        || (eb.source.node == ea.target.node && eb.target.node == ea.source.node)
                })
        });
        let mut xs = BTreeMap::new();
        let mut x = 0.;
        let mut prev_rank: Option<(usize, f64)> = None;
        let y_gap = (caption_height + 40.) * spacing;
        for (&rank, &width) in &widths {
            if let Some((pr, last_width)) = prev_rank {
                let left_c = rank_has_container[&pr];
                let right_c = rank_has_container[&rank];
                let direct_w = local
                    .edges
                    .iter()
                    .filter_map(|e| {
                        let (u, v) = (local.index(&e.source.node)?, local.index(&e.target.node)?);
                        let adj = (ranks[u] == pr && ranks[v] == rank)
                            || (ranks[v] == pr && ranks[u] == rank);
                        if !adj {
                            return None;
                        }
                        let (lu, rv) = if ranks[u] == pr { (u, v) } else { (v, u) };
                        let inset = if !left_c && !right_c && !has_parallel_or_loop {
                            (last_width - boxes[lu].w).max(0.) / 2.
                                + (width - boxes[rv].w).max(0.) / 2.
                        } else {
                            0.
                        };
                        Some((edge_caption_w(e) - inset).max(1.))
                    })
                    .fold(0., f64::max);
                let span_w = if direct_w > 0. {
                    direct_w
                } else {
                    local
                        .edges
                        .iter()
                        .filter(|e| {
                            let (Some(u), Some(v)) =
                                (local.index(&e.source.node), local.index(&e.target.node))
                            else {
                                return false;
                            };
                            let (lo, hi) = (ranks[u].min(ranks[v]), ranks[u].max(ranks[v]));
                            lo <= pr && hi >= rank
                        })
                        .map(edge_caption_w)
                        .fold(0., f64::max)
                };
                let pair_gap = if !left_c && !right_c {
                    if has_parallel_or_loop {
                        (span_w + 2. * o.stub + 20.).max(80.) * spacing
                    } else {
                        (span_w + 20.).max(56.) * spacing
                    }
                } else {
                    let credit = (if left_c { o.container_padding } else { 0. })
                        + (if right_c { o.container_padding } else { 0. });
                    let min_gap = if left_c && right_c { 42. } else { 58. };
                    (span_w + 18. - credit).max(min_gap) * spacing
                };
                x += (last_width + width) / 2. + pair_gap;
            } else {
                x += width / 2.;
            }
            xs.insert(rank, x);
            prev_rank = Some((rank, width));
        }
        let mut rows = BTreeMap::<usize, f64>::new();
        for (&rk, rank_order) in &order_in_rank {
            let y = rows.entry(rk).or_insert(0.);
            for &i in rank_order {
                boxes[i].x = xs[&rk];
                let extra = top_extra(&scene.nodes[units[part[i]]]);
                boxes[i].y = *y + extra + boxes[i].h / 2.;
                *y += extra + boxes[i].h + y_gap;
            }
        }
        let max_col_h = rows
            .values()
            .map(|h| (h - y_gap).max(0.))
            .fold(0., f64::max);
        for (i, r) in boxes.iter_mut().enumerate() {
            let col_h = (rows[&ranks[i]] - y_gap).max(0.);
            r.y += (max_col_h - col_h) / 2.;
        }
        let mut rank_counts = BTreeMap::<usize, usize>::new();
        for &rk in &ranks {
            *rank_counts.entry(rk).or_default() += 1;
        }
        for i in 0..boxes.len() {
            if rank_counts[&ranks[i]] != 1 {
                continue;
            }
            let mut peer_bundles = BTreeMap::<usize, usize>::new();
            for e in &local.edges {
                let (Some(u), Some(v)) = (local.index(&e.source.node), local.index(&e.target.node))
                else {
                    continue;
                };
                if u == v {
                    continue;
                }
                if u == i && ranks[v].abs_diff(ranks[i]) == 1 {
                    *peer_bundles.entry(v).or_default() += 1;
                } else if v == i && ranks[u].abs_diff(ranks[i]) == 1 {
                    *peer_bundles.entry(u).or_default() += 1;
                }
            }
            if let Some((&best_peer, &cnt)) = peer_bundles.iter().max_by_key(|(_, c)| *c)
                && cnt >= 2
                && peer_bundles
                    .iter()
                    .filter(|(k, c)| **k != best_peer && **c == cnt)
                    .count()
                    == 0
            {
                boxes[i].y = boxes[best_peer].y;
            }
        }
        // Short-chain and wide-DAG orientation applies to leaf bodies inside root containers.
        if part.iter().all(|&unit| !scene.nodes[units[unit]].container) {
            crate::topology::readable_concept(&local, &mut boxes, o, true);
            let root_count = scene.nodes.iter().filter(|n| n.parent.is_none()).count();
            let owner_id = scene.nodes[units[part[0]]]
                .parent
                .as_deref()
                .filter(|p| scene.node(p).is_some_and(|owner| owner.parent.is_none()));
            let is_dag = ranking_graph.edges.len() == local.edges.len();
            let cur_bounds = boxes
                .iter()
                .copied()
                .reduce(Rect::union)
                .unwrap_or_default();
            let is_leaf_bridge_topology = root_count == 3 && {
                let root_containers = scene
                    .nodes
                    .iter()
                    .filter(|n| n.parent.is_none() && n.container)
                    .count();
                let root_leaves: Vec<&str> = scene
                    .nodes
                    .iter()
                    .filter(|n| n.parent.is_none() && !n.container)
                    .map(|n| n.id.as_str())
                    .collect();
                root_containers == 2
                    && root_leaves.len() == 1
                    && scene.edges.iter().any(|e| e.target.node == root_leaves[0])
                    && scene.edges.iter().any(|e| e.source.node == root_leaves[0])
            };
            let should_stack_ranks = owner_id.is_some()
                && is_dag
                && ((widths.len() >= 3 && cur_bounds.w > 760.)
                    || (is_leaf_bridge_topology && widths.len() >= 2 && cur_bounds.w > 360.)
                    || (root_count >= 4
                        && (3..=4).contains(&part.len())
                        && local.edges.len() + 1 == part.len()
                        && cur_bounds.w > 560.));
            let ns_ports = border
                .anchors
                .iter()
                .filter(|a| matches!(a.side, Side::N | Side::S))
                .count();
            let ew_ports = border
                .anchors
                .iter()
                .filter(|a| matches!(a.side, Side::E | Side::W))
                .count();
            let n_in_s_out = border
                .anchors
                .iter()
                .filter(|a| (!a.outgoing && a.side == Side::N) || (a.outgoing && a.side == Side::S))
                .count();
            let s_in_n_out = border
                .anchors
                .iter()
                .filter(|a| (!a.outgoing && a.side == Side::S) || (a.outgoing && a.side == Side::N))
                .count();
            let has_n = border.anchors.iter().any(|a| a.side == Side::N);
            let has_s = border.anchors.iter().any(|a| a.side == Side::S);
            let has_in_n_out_s = border.anchors.iter().any(|a| !a.outgoing && a.side == Side::N)
                && border.anchors.iter().any(|a| a.outgoing && a.side == Side::S);
            let has_in_s_out_n = border.anchors.iter().any(|a| !a.outgoing && a.side == Side::S)
                && border.anchors.iter().any(|a| a.outgoing && a.side == Side::N);
            let explicit_ns_ports = border
                .anchors
                .iter()
                .filter(|a| matches!(a.side, Side::N | Side::S) && a.explicit_side)
                .count();
            let port_prefers_tb = owner_idx.is_some()
                && o.explore_candidates
                && widths.len() >= 2
                && ns_ports > ew_ports
                && ((has_in_n_out_s || has_in_s_out_n)
                    || (has_n && has_s && ew_ports == 0)
                    || (explicit_ns_ports > 0 && ew_ports == 0))
                && forster_constraints.is_empty()
                && !owner_idx.is_some_and(|oi| has_node_annotation(scene, oi, o, &["flow"]));
            if (owner_id.is_some() && should_stack_ranks) || port_prefers_tb {
                let by_rank = &order_in_rank;
                // If this container only transmits out of its final rank into a non-container
                // bridge between root platforms, place that exit rank at the top so the dish/port
                // faces the upper bridge directly without crossing internal stages.
                let max_rk = *by_rank.keys().max().unwrap();
                let mut ext_in_any = false;
                let mut ext_out_max_only = false;
                if let Some(oid) = owner_id {
                    for e in &scene.edges {
                        let s_in = ancestors(scene, &e.source.node).contains(oid);
                        let t_in = ancestors(scene, &e.target.node).contains(oid);
                        if !s_in && t_in {
                            ext_in_any = true;
                        } else if s_in
                            && !t_in
                            && let Some(u) = local.index(&e.source.node)
                        {
                            let bridge_target = scene
                                .node(&e.target.node)
                                .is_some_and(|tn| tn.parent.is_none() && !tn.container);
                            if ranks[u] == max_rk && bridge_target && root_count == 3 {
                                ext_out_max_only = true;
                            } else {
                                ext_in_any = true;
                            }
                        }
                    }
                }
                let reverse_tb = (ext_out_max_only && !ext_in_any)
                    || (port_prefers_tb && s_in_n_out > n_in_s_out);
                let rank_seq: Vec<Vec<usize>> = if reverse_tb {
                    by_rank.values().rev().cloned().collect()
                } else {
                    by_rank.values().cloned().collect()
                };
                let vgap = (caption_height + 28.).max(52.) * spacing;
                let hgap = 36. * spacing;
                let mut cursor_y = 0.;
                for row_nodes in rank_seq {
                    let row_extra = row_nodes
                        .iter()
                        .map(|&idx| top_extra(&scene.nodes[units[part[idx]]]))
                        .fold(0., f64::max);
                    let row_h = row_nodes.iter().map(|&idx| boxes[idx].h).fold(0., f64::max);
                    let row_w: f64 = row_nodes.iter().map(|&idx| boxes[idx].w).sum::<f64>()
                        + hgap * row_nodes.len().saturating_sub(1) as f64;
                    let mut cursor_x = -row_w / 2.;
                    for idx in row_nodes {
                        boxes[idx].x = cursor_x + boxes[idx].w / 2.;
                        boxes[idx].y = cursor_y + row_extra + row_h / 2.;
                        cursor_x += boxes[idx].w + hgap;
                    }
                    cursor_y += row_extra + row_h + vgap;
                }
            }
        }
        // Two regions with reciprocal feedback should read as peer rows,
        // rather than one very long pipeline through both owners.
        let owners: Vec<_> = part
            .iter()
            .enumerate()
            .filter(|(_, unit)| scene.nodes[units[**unit]].container)
            .map(|(at, _)| at)
            .collect();
        let mut folded_owners = false;
        if owners.len() == 2 {
            let (a, b) = (owners[0], owners[1]);
            let forward = local
                .edges
                .iter()
                .any(|e| e.source.node == local.nodes[a].id && e.target.node == local.nodes[b].id);
            let reverse = local
                .edges
                .iter()
                .any(|e| e.source.node == local.nodes[b].id && e.target.node == local.nodes[a].id);
            let prefixes: Vec<_> = (0..part.len()).filter(|i| !owners.contains(i)).collect();
            if forward
                && reverse
                && prefixes.iter().all(|&i| {
                    local
                        .edges
                        .iter()
                        .all(|e| e.target.node != local.nodes[i].id)
                })
            {
                let prefix_width = prefixes.iter().map(|&i| boxes[i].w).fold(0., f64::max);
                let owner_width = boxes[a].w.max(boxes[b].w);
                boxes[a].x = prefix_width + (caption_width + 64.) * spacing + owner_width / 2.;
                boxes[b].x = boxes[a].x;
                boxes[a].y = boxes[a].h / 2.;
                boxes[b].y = boxes[a].h + (caption_height + 64.) * spacing + boxes[b].h / 2.;
                let mid = boxes[b].bottom() / 2.;
                let step = prefixes.iter().map(|&i| boxes[i].h).fold(0., f64::max)
                    + (caption_height + 32.) * spacing;
                for (row, &i) in prefixes.iter().enumerate() {
                    boxes[i].x = prefix_width / 2.;
                    boxes[i].y = mid + (row as f64 - (prefixes.len() - 1) as f64 / 2.) * step;
                }
                folded_owners = true;
            }
        }
        // Fold an overly wide 3-stage root bridge (Platform -> Link -> Platform) into a balanced
        // 2-column arch with the link centered overhead so the diagram fits standard viewports.
        let is_root_group = units
            .first()
            .is_some_and(|&u| scene.nodes[u].parent.is_none());
        if !folded_owners
            && is_root_group
            && !owners.is_empty()
            && ranking_graph.edges.len() == local.edges.len()
            && widths.len() == 3
            && rank_counts.values().all(|&c| c == 1)
        {
            let row_bounds = boxes
                .iter()
                .copied()
                .reduce(Rect::union)
                .unwrap_or_default();
            if row_bounds.w > 980. && row_bounds.w / row_bounds.h.max(1.) > 1.8 {
                let mut seq: Vec<usize> = (0..part.len()).collect();
                seq.sort_by_key(|&i| ranks[i]);
                let (u0, u1, u2) = (seq[0], seq[1], seq[2]);
                if !scene.nodes[units[part[u1]]].container
                    && boxes[u1].h < boxes[u0].h * 0.85
                    && boxes[u1].h < boxes[u2].h * 0.85
                {
                    let center_gap = 72. * spacing;
                    let top_extra_1 = top_extra(&scene.nodes[units[part[u1]]]);
                    boxes[u0].x = boxes[u0].w / 2.;
                    boxes[u2].x = boxes[u0].w + center_gap + boxes[u2].w / 2.;
                    boxes[u1].x = (boxes[u0].right() + boxes[u2].left()) / 2.;
                    boxes[u1].y = top_extra_1 + boxes[u1].h / 2.;
                    let row_top = boxes[u1].bottom() + (caption_height + 36.).max(52.) * spacing;
                    boxes[u0].y =
                        row_top + top_extra(&scene.nodes[units[part[u0]]]) + boxes[u0].h / 2.;
                    boxes[u2].y =
                        row_top + top_extra(&scene.nodes[units[part[u2]]]) + boxes[u2].h / 2.;
                }
            }
        }
        let in_container = units
            .first()
            .is_some_and(|&u| scene.nodes[u].parent.is_some());
        let bounds = boxes
            .iter()
            .enumerate()
            .map(|(at, &r)| with_top_extra(&scene.nodes[units[part[at]]], r))
            .reduce(Rect::union)
            .unwrap();
        let ref_x = if in_container { bounds.x } else { bounds.left() };
        let mut base_x_shift = 0.0;
        let mut base_y_top = bottom;
        if !in_container && !placed_parts.is_empty() {
            let cur_ids: BTreeSet<String> = part
                .iter()
                .map(|&u| scene.nodes[units[u]].id.clone())
                .collect();
            let coupled_prev = placed_parts.iter().find(|(prev_ids, _)| {
                cur_ids.iter().any(|cid| {
                    prev_ids
                        .iter()
                        .any(|pid| row_coupled.contains(&(cid.clone(), pid.clone())))
                })
            });
            if let Some((_, prev_rect)) = coupled_prev {
                let max_right = placed_parts
                    .iter()
                    .map(|(_, r)| r.right())
                    .fold(f64::NEG_INFINITY, f64::max);
                base_x_shift = max_right + 96. * spacing;
                base_y_top = prev_rect.top();
            }
        }
        let mut pin_shift = (0.0, 0.0);
        if !in_container
            && let Some(p_at) = (0..part.len())
                .find(|&at| {
                    let gi = units[part[at]];
                    has_node_annotation(scene, gi, o, &["pin"])
                })
                .or_else(|| {
                    (0..part.len()).find(|&at| {
                        let gi = units[part[at]];
                        subtree_has_pin_or_lock(scene, gi, o)
                    })
                })
        {
            let gi = units[part[p_at]];
            let orig_r = rects[gi];
            let (pin_x, pin_y) = pinned_grid_pos(scene, gi, o).unwrap_or((orig_r.x, orig_r.y));
            let placed_x = boxes[p_at].x - ref_x + base_x_shift;
            let placed_y = boxes[p_at].y - bounds.top() + base_y_top;
            pin_shift = (pin_x - placed_x, pin_y - placed_y);
        }
        let mut placed_box: Option<Rect> = None;
        for (at, &unit) in part.iter().enumerate() {
            let i = units[unit];
            if !in_container && subtree_has_pin_or_lock(scene, i, o) {
                continue;
            }
            let dx = boxes[at].x - ref_x + base_x_shift + pin_shift.0 - rects[i].x;
            let dy = boxes[at].y - bounds.top() + base_y_top + pin_shift.1 - rects[i].y;
            let id = &scene.nodes[i].id;
            for (j, r) in rects.iter_mut().enumerate() {
                if j == i || ancestors(scene, &scene.nodes[j].id).contains(id) {
                    r.x += dx;
                    r.y += dy;
                }
            }
            rects[i].w = boxes[at].w;
            rects[i].h = boxes[at].h;
            let cur_r = with_top_extra(&scene.nodes[i], rects[i]);
            placed_box = Some(match placed_box {
                Some(b) => b.union(cur_r),
                None => cur_r,
            });
        }
        let part_rect = placed_box.unwrap_or(Rect::new(base_x_shift, base_y_top, bounds.w, bounds.h));
        let part_ids: BTreeSet<String> = part
            .iter()
            .map(|&u| scene.nodes[units[u]].id.clone())
            .collect();
        placed_parts.push((part_ids, part_rect));
        bottom = bottom.max(part_rect.bottom() + (if in_container { 36. } else { 96. }) * spacing);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Node, SceneGraph, SourceOrigin};

    fn test_node(id: &str, parent: Option<&str>, container: bool) -> Node {
        Node {
            id: id.into(),
            label: id.into(),
            parent: parent.map(str::to_owned),
            shape: "rect".into(),
            container,
            ports: Default::default(),
            attrs: Default::default(),
            annotations: Vec::new(),
            origin: SourceOrigin::default(),
            declaration_index: 0,
            explicit: true,
        }
    }

    #[test]
    fn containers_postorder_puts_children_before_parents_for_any_decl_order() {
        let mut scene = SceneGraph::default();
        // Child declared before parent, and deeper grandchild declared first.
        scene.nodes.push(test_node("p.c.gc", Some("p.c"), true)); // idx 0 (depth 2)
        scene.nodes.push(test_node("p", None, true)); // idx 1 (depth 0)
        scene.nodes.push(test_node("p.c", Some("p"), true)); // idx 2 (depth 1)
        scene.nodes.push(test_node("other", None, true)); // idx 3 (depth 0)
        scene.nodes.push(test_node("leaf", None, false));
        assert_eq!(containers_postorder(&scene), vec![0, 2, 1, 3]);
    }

    #[test]
    fn non_conflicting_annotations_and_single_pin_still_pack() {
        let src = "client -> prod.api: req @label(slot: above)\nprod {\n  api -> worker\n}\nclient @pin(grid: 0, 0)\n";
        let d = crate::compile(src).expect("should compile");
        let opts = LayoutOptions::default();
        assert!(!should_skip_owner_pack(&d.scene, None, &opts));
        let prod_idx = d.scene.index("prod").unwrap();
        assert!(!should_skip_owner_pack(&d.scene, Some(prod_idx), &opts));
        let prod = d.layout.nodes["prod"];
        let api = d.layout.nodes["prod.api"];
        let worker = d.layout.nodes["prod.worker"];
        assert!(api.x < worker.x, "client->prod.api->worker should pack L->R inside prod");
        assert!(prod.left() <= api.left() && worker.right() <= prod.right());
    }

    #[test]
    fn selective_skip_only_when_cross_container_relation_pins_children() {
        let src = "a {\n  a1 -> a2\n}\nb {\n  b1 -> b2\n}\nalign row [a.a1, a.a2, b.b1]\n";
        let compiled = crate::compile(src).unwrap();
        let scene = compiled.scene;
        let opts = LayoutOptions::default();
        let a_idx = scene.index("a").unwrap();
        let b_idx = scene.index("b").unwrap();
        assert!(should_skip_owner_pack(&scene, Some(a_idx), &opts));
        assert!(!should_skip_owner_pack(&scene, Some(b_idx), &opts));
    }

    #[test]
    fn project_border_anchors_preserve_cross_boundary_directions() {
        let src = "west {\n  ingress -> queue\n}\neast {\n  worker -> store\n}\nwest.queue -> east.worker\n";
        let compiled = crate::compile(src).unwrap();
        let scene = compiled.scene;
        let opts = LayoutOptions::default();
        let owners = containers_postorder(&scene);
        let macro_flow = compute_macro_flow(&scene, &owners);
        let rects = vec![Rect::new(0., 0., 100., 50.); scene.nodes.len()];

        let west_idx = scene.index("west").unwrap();
        let west_children: Vec<usize> = scene
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.parent.as_deref() == Some("west"))
            .map(|(i, _)| i)
            .collect();
        let west_border =
            project_border_info(&scene, &west_children, &rects, Some(west_idx), &macro_flow, &opts);
        assert_eq!(west_border.anchors.len(), 1);
        assert_eq!(west_border.anchors[0].side, Side::E);
        assert!(west_border.anchors[0].outgoing);

        let east_idx = scene.index("east").unwrap();
        let east_children: Vec<usize> = scene
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.parent.as_deref() == Some("east"))
            .map(|(i, _)| i)
            .collect();
        let east_border =
            project_border_info(&scene, &east_children, &rects, Some(east_idx), &macro_flow, &opts);
        assert_eq!(east_border.anchors.len(), 1);
        assert_eq!(east_border.anchors[0].side, Side::W);
        assert!(!east_border.anchors[0].outgoing);
    }

    #[test]
    fn forster_constrained_order_respects_precedence_and_minimizes_crossings() {
        // Nodes 0, 1, 2 in rank; unconstrained barycenters say [0: 0.9, 1: 0.2, 2: 0.8].
        // With constraint (0 < 1), 0 & 1 merge with weighted bary (0.9 + 0.2)/2 = 0.55 < 0.8,
        // so final order must keep 0 before 1, then 2 => [0, 1, 2].
        let nodes = vec![0usize, 1, 2];
        let raw_bary = BTreeMap::from([(0, 0.9), (1, 0.2), (2, 0.8)]);
        let weights = BTreeMap::from([(0, 1), (1, 1), (2, 1)]);
        let tie_slot = BTreeMap::from([(0, 0), (1, 1), (2, 2)]);
        let ordered = forster_constrained_order(&nodes, &raw_bary, &weights, &[(0, 1)], &tie_slot);
        assert_eq!(ordered, vec![0, 1, 2]);
        let pos_0 = ordered.iter().position(|&x| x == 0).unwrap();
        let pos_1 = ordered.iter().position(|&x| x == 1).unwrap();
        assert!(pos_0 < pos_1);
    }

    #[test]
    fn container_transposes_to_tb_when_ns_border_ports_predominate() {
        let src = "top\nbot\nbox_c {\n  s1 -> s2 -> s3\n}\ntop -> box_c.s1.N\nbox_c.s3.S -> bot\n";
        let compiled = crate::compile(src).unwrap();
        let scene = compiled.scene;
        let mut rects: Vec<Rect> = scene.nodes.iter().map(|_| Rect::new(0., 0., 110., 48.)).collect();
        let opts_explore = LayoutOptions {
            explore_candidates: true,
            ..LayoutOptions::default()
        };
        pack(&scene, &mut rects, &opts_explore, 1.0);
        let s1 = rects[scene.index("box_c.s1").unwrap()];
        let s2 = rects[scene.index("box_c.s2").unwrap()];
        let s3 = rects[scene.index("box_c.s3").unwrap()];
        assert!(
            s1.y < s2.y && s2.y < s3.y && (s1.x - s2.x).abs() < 1e-3,
            "N/S border ports with explore_candidates should orient internal pipeline TB"
        );
    }
}
