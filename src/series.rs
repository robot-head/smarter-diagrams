//! Compact, ordered presentation of repeated decision/branch/join regions.
use crate::{layout::*, model::*, topology::SideHints};
use std::collections::{BTreeMap, BTreeSet};
struct Sequence {
    entry: usize,
    regions: Vec<[usize; 4]>,
    exit: usize,
}
fn sequence(scene: &SceneGraph) -> Option<Sequence> {
    let n = scene.nodes.len();
    if n < 10 {
        return None;
    }
    let mut outgoing = vec![Vec::new(); n];
    let mut incoming = vec![0; n];
    for e in &scene.edges {
        let a = scene.index(&e.source.node)?;
        let b = scene.index(&e.target.node)?;
        outgoing[a].push(b);
        incoming[b] += 1;
    }
    let roots: Vec<_> = (0..n).filter(|&i| incoming[i] == 0).collect();
    if roots.len() != 1 {
        return None;
    }
    let entry = roots[0];
    if outgoing[entry].len() != 1 {
        return None;
    }
    let mut at = outgoing[entry][0];
    let mut seen = BTreeSet::from([entry]);
    let mut regions = Vec::new();
    loop {
        if outgoing[at].len() != 2 {
            return None;
        }
        let (b, c) = (outgoing[at][0], outgoing[at][1]);
        if incoming[b] != 1
            || incoming[c] != 1
            || outgoing[b].len() != 1
            || outgoing[c].len() != 1
            || outgoing[b][0] != outgoing[c][0]
        {
            return None;
        }
        let d = outgoing[b][0];
        if incoming[d] != 2 {
            return None;
        }
        if [at, b, c, d].iter().any(|&i| !seen.insert(i)) {
            return None;
        }
        regions.push([at, b, c, d]);
        if outgoing[d].len() != 1 {
            return None;
        }
        at = outgoing[d][0];
        if outgoing[at].is_empty() {
            break;
        }
    }
    if regions.len() < 2 || seen.contains(&at) || seen.len() + 1 != n {
        return None;
    }
    Some(Sequence {
        entry,
        regions,
        exit: at,
    })
}
pub(crate) fn recognized(scene: &SceneGraph) -> bool {
    sequence(scene).is_some()
}
pub(crate) fn present(
    scene: &SceneGraph,
    rects: &mut [Rect],
    o: &LayoutOptions,
    place: bool,
) -> Option<SideHints> {
    let s = sequence(scene)?;
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
        let left_w = s
            .regions
            .iter()
            .flat_map(|r| [r[0], r[2]])
            .map(|i| rects[i].w)
            .fold(0., f64::max);
        let right_w = s
            .regions
            .iter()
            .flat_map(|r| [r[1], r[3]])
            .map(|i| rects[i].w)
            .fold(0., f64::max);
        let top_h = s
            .regions
            .iter()
            .flat_map(|r| [r[0], r[1]])
            .map(|i| rects[i].h)
            .fold(0., f64::max);
        let bottom_h = s
            .regions
            .iter()
            .flat_map(|r| [r[2], r[3]])
            .map(|i| rects[i].h)
            .fold(0., f64::max);
        let caption = |a: usize, b: usize| {
            let text = scene
                .edges
                .iter()
                .find(|e| e.source.node == scene.nodes[a].id && e.target.node == scene.nodes[b].id)
                .map(|e| crate::labels::connection_text(scene, e))
                .unwrap_or_default();
            (
                crate::labels::text_width(&text),
                crate::labels::text_height(&text),
            )
        };
        let mut between_w = 0.0f64;
        let mut between_h = 0.0f64;
        for r in s.regions.windows(2) {
            let (w, h) = caption(r[0][3], r[1][0]);
            between_w = between_w.max(w);
            between_h = between_h.max(h);
        }
        let prefix_h = caption(s.entry, s.regions[0][0]).1;
        let suffix_w = caption(s.regions.last().unwrap()[3], s.exit).0;
        let dx = (left_w + right_w) / 2. + cap_w + 2. * o.stub + 12.;
        let dy = (top_h + bottom_h) / 2. + cap_h + 2. * o.stub + 12.;
        let cell_w = dx + (left_w + right_w) / 2.;
        let cell_h = dy + (top_h + bottom_h) / 2.;
        let gap_w = between_w + 2. * o.stub + 12.;
        let gap_h = between_h + 2. * o.stub + 12.;
        let columns = (1..=s.regions.len())
            .max_by(|&a, &b| {
                let fit = |columns: usize| {
                    let w = columns as f64 * cell_w
                        + (columns - 1) as f64 * gap_w
                        + cap_w
                        + 48.
                        + if s.regions.len().is_multiple_of(columns) {
                            rects[s.exit].w + suffix_w + 2. * o.stub + 12.
                        } else {
                            0.
                        };
                    let h = s.regions.len().div_ceil(columns) as f64 * cell_h
                        + (s.regions.len().div_ceil(columns) - 1) as f64 * gap_h
                        + rects[s.entry].h
                        + prefix_h
                        + 2. * o.stub
                        + 48.;
                    (450. * o.target_aspect_ratio / w).min(450. / h).min(1.)
                };
                fit(a).total_cmp(&fit(b))
            })
            .unwrap();
        for (index, &[a, b, c, d]) in s.regions.iter().enumerate() {
            let x = (index % columns) as f64 * (cell_w + gap_w);
            let y = (index / columns) as f64 * (cell_h + gap_h);
            for (i, x, y) in [
                (a, x, y),
                (b, x + dx, y),
                (c, x, y + dy),
                (d, x + dx, y + dy),
            ] {
                rects[i].x = x;
                rects[i].y = y;
            }
        }
        let first = rects[s.regions[0][0]];
        rects[s.entry].x = first.x;
        rects[s.entry].y = first.top() - rects[s.entry].h / 2. - prefix_h - 2. * o.stub - 12.;
        let last = rects[s.regions.last().unwrap()[3]];
        rects[s.exit].x = last.right() + rects[s.exit].w / 2. + suffix_w + 2. * o.stub + 12.;
        rects[s.exit].y = last.y;
    }
    let mut hints = BTreeMap::new();
    let ids = |a: usize, b: usize| (scene.nodes[a].id.clone(), scene.nodes[b].id.clone());
    for &[a, b, c, d] in &s.regions {
        hints.insert(ids(a, b), (Side::E, Side::W));
        hints.insert(ids(a, c), (Side::S, Side::N));
        hints.insert(ids(b, d), (Side::S, Side::N));
        hints.insert(ids(c, d), (Side::E, Side::W));
    }
    hints.insert(ids(s.entry, s.regions[0][0]), (Side::S, Side::N));
    for regions in s.regions.windows(2) {
        let (a, b) = (regions[0][3], regions[1][0]);
        hints.insert(
            ids(a, b),
            if rects[a].x + 2. * o.stub < rects[b].x {
                (Side::E, Side::W)
            } else {
                (Side::S, Side::N)
            },
        );
    }
    hints.insert(
        ids(s.regions.last().unwrap()[3], s.exit),
        (Side::E, Side::W),
    );
    // Reopening saved pins must preserve their node positions; only apply an
    // attachment plan while its outward stubs still have a clear channel.
    let result: SideHints = scene
        .edges
        .iter()
        .map(|e| {
            hints
                .get(&(e.source.node.clone(), e.target.node.clone()))
                .copied()
        })
        .collect();
    if result.iter().any(Option::is_none) {
        None
    } else {
        Some(result)
    }
}
pub(crate) fn coordinate(scene: &SceneGraph, o: &LayoutOptions, l: &mut Layout) -> bool {
    let mut rects: Vec<_> = scene.nodes.iter().map(|n| l.nodes[&n.id]).collect();
    let Some(hints) = present(scene, &mut rects, o, false) else {
        return false;
    };
    let mut routes = BTreeMap::new();
    for (index, e) in scene.edges.iter().enumerate() {
        let a = &l.ports[&port_key(e, true)];
        let b = &l.ports[&port_key(e, false)];
        if hints[index] != Some((a.side, b.side)) {
            return false;
        }
        let mut p = vec![a.point, a.stub];
        if matches!(a.side, Side::E | Side::W) {
            let x = (a.stub.x + b.stub.x) / 2.;
            p.extend([Point::new(x, a.stub.y), Point::new(x, b.stub.y)]);
        } else {
            let y = (a.stub.y + b.stub.y) / 2.;
            p.extend([Point::new(a.stub.x, y), Point::new(b.stub.x, y)]);
        }
        p.extend([b.stub, b.point]);
        p.dedup_by(|a, b| a.distance(*b) < 1e-7);
        let r = Route {
            edge: e.id.clone(),
            points: p,
        };
        if !crate::routing::route_is_valid(scene, l, e, &r, o.margin) {
            return false;
        }
        routes.insert(e.id.clone(), r);
    }
    l.routes.extend(routes);
    true
}
