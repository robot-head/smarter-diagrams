//! Fresh placement treats ownership and independent components as whole units.
use crate::{layout::*, model::*};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn pack(scene: &SceneGraph, rects: &mut [Rect], o: &LayoutOptions) {
    if !scene.relations.is_empty()
        || !scene.layout_rules.is_empty()
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
        group(scene, rects, o, &children);
        let bounds = children
            .iter()
            .map(|&i| rects[i])
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
    if roots.len() < 2 {
        return;
    }
    let projected = project(scene, &roots);
    if !owners.is_empty() || components(&projected).len() > 1 {
        group(scene, rects, o, &roots);
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
fn group(scene: &SceneGraph, rects: &mut [Rect], o: &LayoutOptions, units: &[usize]) {
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
        let ranks = crate::topology::coffman_graham(&local, part.len().max(2));
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
        for (&rank, &width) in &widths {
            x += (last_width + width) / 2.;
            xs.insert(rank, x);
            x += (caption_width + 2. * o.stub + 32.).max(96.);
            last_width = width;
        }
        let mut rows = BTreeMap::<usize, f64>::new();
        for (i, r) in boxes.iter_mut().enumerate() {
            r.x = xs[&ranks[i]];
            let y = rows.entry(ranks[i]).or_insert(0.);
            r.y = *y + r.h / 2.;
            *y += r.h + caption_height + 48.;
        }
        // Short-chain orientation applies to leaf bodies. Small owner groups
        // retain scope packing, which reserves their frame-to-frame channels.
        if part.len() > 4 || part.iter().all(|&unit| !scene.nodes[units[unit]].container) {
            crate::topology::readable_concept(&local, &mut boxes, o, true);
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
                boxes[a].x = prefix_width + caption_width + 96. + owner_width / 2.;
                boxes[b].x = boxes[a].x;
                boxes[a].y = boxes[a].h / 2.;
                boxes[b].y = boxes[a].h + caption_height + 96. + boxes[b].h / 2.;
                let mid = boxes[b].bottom() / 2.;
                let step =
                    prefixes.iter().map(|&i| boxes[i].h).fold(0., f64::max) + caption_height + 32.;
                for (row, &i) in prefixes.iter().enumerate() {
                    boxes[i].x = prefix_width / 2.;
                    boxes[i].y = mid + (row as f64 - (prefixes.len() - 1) as f64 / 2.) * step;
                }
            }
        }
        let bounds = boxes.iter().copied().reduce(Rect::union).unwrap();
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
        bottom += bounds.h + 96.;
    }
}
