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
            let x0 = rects[children[0]].x;
            if children.iter().any(|&i| (rects[i].x - x0).abs() > 1e-3) {
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
    if columns.is_empty() {
        return;
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
        let caption_width = local
            .edges
            .iter()
            .map(|e| crate::labels::text_width(&crate::labels::connection_text(&local, e)))
            .fold(0., f64::max);
        let caption_height = local
            .edges
            .iter()
            .map(|e| crate::labels::text_height(&crate::labels::connection_text(&local, e)))
            .fold(0., f64::max);
        let mut widths = BTreeMap::<usize, f64>::new();
        for (i, r) in boxes.iter().enumerate() {
            widths
                .entry(ranks[i])
                .and_modify(|w| *w = w.max(r.w))
                .or_insert(r.w);
        }
        let mut xs = BTreeMap::new();
        let mut x = 0.;
        let mut last_width = 0.;
        let x_gap = (caption_width + 2. * o.stub + 20.).max(80.) * spacing;
        let y_gap = (caption_height + 40.) * spacing;
        for (&rank, &width) in &widths {
            x += (last_width + width) / 2.;
            xs.insert(rank, x);
            x += x_gap;
            last_width = width;
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
        // Short-chain orientation applies to leaf bodies. Small owner groups
        // retain scope packing, which reserves their frame-to-frame channels.
        if part.iter().all(|&unit| !scene.nodes[units[unit]].container) {
            crate::topology::readable_concept(&local, &mut boxes, o, true);
            let root_count = scene.nodes.iter().filter(|n| n.parent.is_none()).count();
            let in_root_owner = scene.nodes[units[part[0]]]
                .parent
                .as_deref()
                .and_then(|p| scene.node(p))
                .is_some_and(|owner| owner.parent.is_none());
            if in_root_owner
                && root_count >= 4
                && (3..=4).contains(&part.len())
                && local.edges.len() + 1 == part.len()
            {
                let mut indeg = vec![0usize; part.len()];
                let mut out = vec![Vec::new(); part.len()];
                for e in &local.edges {
                    if let (Some(a), Some(b)) =
                        (local.index(&e.source.node), local.index(&e.target.node))
                    {
                        indeg[b] += 1;
                        out[a].push(b);
                    }
                }
                if indeg.iter().all(|&d| d <= 1)
                    && out.iter().all(|v| v.len() <= 1)
                    && let Some(start) = (0..part.len()).find(|&i| indeg[i] == 0)
                {
                    let mut chain = vec![start];
                    while let Some(&nxt) = out[*chain.last().unwrap()].first() {
                        if chain.contains(&nxt) {
                            break;
                        }
                        chain.push(nxt);
                    }
                    if chain.len() == part.len() {
                        let vgap = (caption_height + 40.).max(72.) * spacing;
                        let mut cursor_y = 0.;
                        for &idx in &chain {
                            let extra = top_extra(&scene.nodes[units[part[idx]]]);
                            boxes[idx].x = 0.;
                            boxes[idx].y = cursor_y + extra + boxes[idx].h / 2.;
                            cursor_y += extra + boxes[idx].h + vgap;
                        }
                    }
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
            }
        }
        let bounds = boxes
            .iter()
            .enumerate()
            .map(|(at, &r)| with_top_extra(&scene.nodes[units[part[at]]], r))
            .reduce(Rect::union)
            .unwrap();
        for (at, &unit) in part.iter().enumerate() {
            let i = units[unit];
            let dx = boxes[at].x - bounds.left() - rects[i].x;
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
        bottom += bounds.h + 96. * spacing;
    }
}
