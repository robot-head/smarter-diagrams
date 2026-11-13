//! Fresh placement treats ownership and independent components as whole units.
use crate::{layout::*, model::*};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn pack(scene: &SceneGraph, rects: &mut [Rect], o: &LayoutOptions, spacing: f64) {
    if !scene.layout_rules.is_empty()
        || scene.nodes.iter().any(|n| {
            effective_annotations(scene, &n.id, &n.annotations, o.variant.as_deref())
                .iter()
                .any(|a| a.name == "pin")
        })
        || scene.edges.iter().any(|e| {
            !effective_annotations(scene, &e.id, &e.annotations, o.variant.as_deref()).is_empty()
        })
    {
        return;
    }
    let owners: Vec<_> = scene
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.container)
        .map(|(i, _)| i)
        .collect();
    for &owner in owners.iter().rev() {
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
        group(scene, rects, o, &children, spacing);
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
    let roots: Vec<_> = scene
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.parent.is_none())
        .map(|(i, _)| i)
        .collect();
    if roots.len() >= 2 {
        let projected = project(scene, &roots);
        if !owners.is_empty() || components(&projected).len() > 1 {
            group(scene, rects, o, &roots, spacing);
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
        .filter_map(|(_, owner)| {
            let children: Vec<usize> = scene
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, c)| c.parent.as_ref() == Some(&owner.id))
                .map(|(i, _)| i)
                .collect();
            if children.len() < 2
                || children.iter().any(|&i| {
                    scene.nodes[i].container || rel_nodes.contains(scene.nodes[i].id.as_str())
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
fn group(scene: &SceneGraph, rects: &mut [Rect], o: &LayoutOptions, units: &[usize], spacing: f64) {
    let graph = project(scene, units);
    let mut bottom = 0.;
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
        let ranks = crate::topology::coffman_graham(&ranking_graph, part.len().max(2));
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
        for (i, r) in boxes.iter_mut().enumerate() {
            r.x = xs[&ranks[i]];
            let extra = top_extra(&scene.nodes[units[part[i]]]);
            let y = rows.entry(ranks[i]).or_insert(0.);
            r.y = *y + extra + r.h / 2.;
            *y += extra + r.h + y_gap;
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
            if let Some(oid) = owner_id
                && should_stack_ranks
            {
                let mut by_rank: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
                for (i, &rk) in ranks.iter().enumerate().take(part.len()) {
                    by_rank.entry(rk).or_default().push(i);
                }
                // If this container only transmits out of its final rank into a non-container
                // bridge between root platforms, place that exit rank at the top so the dish/port
                // faces the upper bridge directly without crossing internal stages.
                let max_rk = *by_rank.keys().max().unwrap();
                let mut ext_in_any = false;
                let mut ext_out_max_only = false;
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
                let rank_seq: Vec<Vec<usize>> = if ext_out_max_only && !ext_in_any {
                    by_rank.into_values().rev().collect()
                } else {
                    by_rank.into_values().collect()
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
        for (at, &unit) in part.iter().enumerate() {
            let i = units[unit];
            let ref_x = if in_container {
                bounds.x
            } else {
                bounds.left()
            };
            let dx = boxes[at].x - ref_x - rects[i].x;
            let dy = boxes[at].y - bounds.top() + bottom - rects[i].y;
            let id = &scene.nodes[i].id;
            for (j, r) in rects.iter_mut().enumerate() {
                if j == i || ancestors(scene, &scene.nodes[j].id).contains(id) {
                    r.x += dx;
                    r.y += dy;
                }
            }
            rects[i].w = boxes[at].w;
            rects[i].h = boxes[at].h;
        }
        bottom += bounds.h + (if in_container { 36. } else { 96. }) * spacing;
    }
}
