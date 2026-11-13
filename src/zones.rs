//! Rectilinear union contours with holes for unrelated nodes. Membership never
//! implies ownership: members across containers share one contiguous Gestalt
//! splat while routing cleanly around non-member nodes and container titles.
use crate::{layout::*, model::*};
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

pub(crate) fn contours(scene: &SceneGraph, layout: &Layout, zone: &Zone) -> Vec<Vec<Point>> {
    let members: BTreeSet<_> = scene
        .nodes
        .iter()
        .filter(|n| {
            zone.members.contains(&n.id)
                || ancestors(scene, &n.id)
                    .iter()
                    .any(|id| zone.members.contains(id))
        })
        .map(|n| n.id.clone())
        .collect();
    if members.is_empty() {
        return Vec::new();
    }
    let padding = value(&zone.attrs, "padding").unwrap_or(1.5) * GRID;
    let mut exclusions = Vec::new();
    for n in &scene.nodes {
        if members.contains(&n.id) {
            continue;
        }
        let rect = layout.nodes[&n.id];
        if members.iter().any(|m| ancestors(scene, m).contains(&n.id)) {
            if let Some(hr) = container_header_rect(n, rect) {
                exclusions.push(hr.inflate(4.));
            }
        } else {
            exclusions.push(rect.inflate(8.));
            if let Some(tr) = sql_table_title_rect(n, rect) {
                exclusions.push(tr.inflate(6.));
            }
        }
    }
    let mut member_rects = Vec::new();
    let mut includes = Vec::new();
    let mut owner_envelopes: BTreeMap<Option<String>, Rect> = BTreeMap::new();
    for id in &members {
        let r = layout.nodes[id].inflate(padding);
        if r.w > 0. && r.h > 0. {
            owner_envelopes
                .entry(scene.node(id).unwrap().parent.clone())
                .and_modify(|old| *old = old.union(r))
                .or_insert(r);
            member_rects.push(r);
            includes.push(r);
        }
    }
    for env in owner_envelopes.into_values() {
        if !exclusions.iter().any(|ex| ex.intersects(env)) {
            includes.push(env);
        }
    }
    // Connect nearby member halos along aligned spans whenever clear of exclusions.
    let m_len = member_rects.len();
    if m_len >= 2 {
        let rect_dist = |a: Rect, b: Rect| {
            let dx = (b.left() - a.right()).max(a.left() - b.right()).max(0.);
            let dy = (b.top() - a.bottom()).max(a.top() - b.bottom()).max(0.);
            dx + dy
        };
        let mut in_tree = vec![false; m_len];
        in_tree[0] = true;
        for _ in 1..m_len {
            let mut best = (f64::INFINITY, 0usize, 0usize);
            for (u, &ru) in member_rects.iter().enumerate() {
                if !in_tree[u] {
                    continue;
                }
                for (v, &rv) in member_rects.iter().enumerate() {
                    if in_tree[v] {
                        continue;
                    }
                    let d = rect_dist(ru, rv);
                    if d < best.0 {
                        best = (d, u, v);
                    }
                }
            }
            if !best.0.is_finite() {
                break;
            }
            in_tree[best.2] = true;
            if best.0 > 640. {
                continue;
            }
            let (ra, rb) = (member_rects[best.1], member_rects[best.2]);
            let (ox0, ox1) = (ra.left().max(rb.left()), ra.right().min(rb.right()));
            if ox1 - ox0 >= 24. {
                let (ty, by) = (ra.top().min(rb.top()), ra.bottom().max(rb.bottom()));
                let bridge = Rect::new((ox0 + ox1) / 2., (ty + by) / 2., ox1 - ox0, by - ty);
                if !exclusions.iter().any(|ex| ex.intersects(bridge)) {
                    includes.push(bridge);
                }
            }
            let (oy0, oy1) = (ra.top().max(rb.top()), ra.bottom().min(rb.bottom()));
            if oy1 - oy0 >= 24. {
                let (lx, rx) = (ra.left().min(rb.left()), ra.right().max(rb.right()));
                let bridge = Rect::new((lx + rx) / 2., (oy0 + oy1) / 2., rx - lx, oy1 - oy0);
                if !exclusions.iter().any(|ex| ex.intersects(bridge)) {
                    includes.push(bridge);
                }
            }
        }
    }
    // Bridge remaining nearby member components around exclusions via rectilinear corridors.
    let dedup_coords = |coords: &mut Vec<f64>| {
        coords.sort_by(f64::total_cmp);
        coords.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    };
    for _ in 0..m_len {
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        for r in &includes {
            xs.extend([r.left() - 28., r.left(), r.right(), r.right() + 28.]);
            ys.extend([r.top() - 28., r.top(), r.bottom(), r.bottom() + 28.]);
        }
        for ex in &exclusions {
            xs.extend([ex.left() - 28., ex.left(), ex.right(), ex.right() + 28.]);
            ys.extend([ex.top() - 28., ex.top(), ex.bottom(), ex.bottom() + 28.]);
        }
        dedup_coords(&mut xs);
        dedup_coords(&mut ys);
        if xs.len() < 2 || ys.len() < 2 {
            break;
        }
        let w = xs.len() - 1;
        let h = ys.len() - 1;
        let mut blocked = vec![false; w * h];
        let mut filled = vec![false; w * h];
        for y in 0..h {
            let py = (ys[y] + ys[y + 1]) / 2.;
            for x in 0..w {
                let p = Point::new((xs[x] + xs[x + 1]) / 2., py);
                let blk = exclusions.iter().any(|r| r.contains(p));
                blocked[y * w + x] = blk;
                filled[y * w + x] = !blk && includes.iter().any(|r| r.contains(p));
            }
        }
        let mut comp = vec![usize::MAX; w * h];
        let mut num_comp = 0usize;
        for start in 0..w * h {
            if !filled[start] || comp[start] != usize::MAX {
                continue;
            }
            let cid = num_comp;
            num_comp += 1;
            let mut stack = vec![start];
            comp[start] = cid;
            while let Some(cur) = stack.pop() {
                let (cx, cy) = (cur % w, cur / w);
                for (nx, ny) in [
                    (cx.wrapping_sub(1), cy),
                    (cx + 1, cy),
                    (cx, cy.wrapping_sub(1)),
                    (cx, cy + 1),
                ] {
                    if nx < w && ny < h {
                        let ni = ny * w + nx;
                        if filled[ni] && comp[ni] == usize::MAX {
                            comp[ni] = cid;
                            stack.push(ni);
                        }
                    }
                }
            }
        }
        if num_comp <= 1 {
            break;
        }
        let open_span = |cx: usize, cy: usize, horizontal_move: bool| -> f64 {
            if horizontal_move {
                let mut y0 = cy;
                while y0 > 0 && !blocked[(y0 - 1) * w + cx] {
                    y0 -= 1;
                }
                let mut y1 = cy + 1;
                while y1 < h && !blocked[y1 * w + cx] {
                    y1 += 1;
                }
                ys[y1] - ys[y0]
            } else {
                let mut x0 = cx;
                while x0 > 0 && !blocked[cy * w + x0 - 1] {
                    x0 -= 1;
                }
                let mut x1 = cx + 1;
                while x1 < w && !blocked[cy * w + x1] {
                    x1 += 1;
                }
                xs[x1] - xs[x0]
            }
        };
        let states = w * h * 5;
        let mut best_pts: Option<(f64, Vec<Point>)> = None;
        for src_cid in 0..num_comp {
            let mut dist = vec![f64::INFINITY; states];
            let mut prev = vec![usize::MAX; states];
            let mut heap = BinaryHeap::<(std::cmp::Reverse<u64>, usize)>::new();
            for (idx, &cid) in comp.iter().enumerate() {
                if cid == src_cid {
                    let s = idx * 5 + 4;
                    dist[s] = 0.;
                    heap.push((std::cmp::Reverse(0f64.to_bits()), s));
                }
            }
            let mut reached = None;
            while let Some((std::cmp::Reverse(bits), s)) = heap.pop() {
                let d = f64::from_bits(bits);
                if d > dist[s] + 1e-9 {
                    continue;
                }
                if best_pts.as_ref().is_some_and(|(bd, _)| d >= *bd) {
                    break;
                }
                let (cell_idx, dir) = (s / 5, s % 5);
                if comp[cell_idx] != usize::MAX && comp[cell_idx] != src_cid {
                    reached = Some((d, s));
                    break;
                }
                let (cx, cy) = (cell_idx % w, cell_idx / w);
                let mx = (xs[cx] + xs[cx + 1]) / 2.;
                let my = (ys[cy] + ys[cy + 1]) / 2.;
                for (ndir, nx, ny) in [
                    (0usize, cx + 1, cy),
                    (1usize, cx, cy + 1),
                    (2usize, cx.wrapping_sub(1), cy),
                    (3usize, cx, cy.wrapping_sub(1)),
                ] {
                    if nx >= w || ny >= h {
                        continue;
                    }
                    let ncell = ny * w + nx;
                    if blocked[ncell] {
                        continue;
                    }
                    let nmx = (xs[nx] + xs[nx + 1]) / 2.;
                    let nmy = (ys[ny] + ys[ny + 1]) / 2.;
                    let mut step = (mx - nmx).abs() + (my - nmy).abs();
                    if dir < 4 && dir != ndir {
                        step += 80.;
                    }
                    if open_span(nx, ny, ndir == 0 || ndir == 2) < 20. {
                        step += 2000.;
                    }
                    let ns = ncell * 5 + ndir;
                    let nd = d + step;
                    if nd + 1e-9 < dist[ns] {
                        dist[ns] = nd;
                        prev[ns] = s;
                        heap.push((std::cmp::Reverse(nd.to_bits()), ns));
                    }
                }
            }
            if let Some((cost, mut cur)) = reached {
                let mut cells = Vec::new();
                let mut pts = Vec::new();
                while cur != usize::MAX {
                    let cell_idx = cur / 5;
                    let (cx, cy) = (cell_idx % w, cell_idx / w);
                    if cells.last() != Some(&(cx, cy)) {
                        cells.push((cx, cy));
                        pts.push(Point::new(
                            (xs[cx] + xs[cx + 1]) / 2.,
                            (ys[cy] + ys[cy + 1]) / 2.,
                        ));
                    }
                    cur = prev[cur];
                }
                let boundary_pt = |a: (usize, usize), b: (usize, usize)| {
                    if a.0 != b.0 {
                        let bx = xs[a.0.max(b.0)];
                        Point::new(bx, (ys[a.1] + ys[a.1 + 1]) / 2.)
                    } else {
                        let by = ys[a.1.max(b.1)];
                        Point::new((xs[a.0] + xs[a.0 + 1]) / 2., by)
                    }
                };
                let empty_len = if pts.len() >= 2 {
                    let p_start = boundary_pt(cells[0], cells[1]);
                    let p_end = boundary_pt(cells[cells.len() - 2], cells[cells.len() - 1]);
                    if pts.len() == 2 {
                        p_start.manhattan_distance(p_end)
                    } else {
                        p_start.manhattan_distance(pts[1])
                            + pts[1..pts.len() - 1]
                                .windows(2)
                                .map(|p| p[0].manhattan_distance(p[1]))
                                .sum::<f64>()
                            + pts[pts.len() - 2].manhattan_distance(p_end)
                    }
                } else {
                    0.
                };
                if empty_len <= 680. && best_pts.as_ref().is_none_or(|(bcost, _)| cost < *bcost) {
                    best_pts = Some((cost, pts));
                }
            }
        }
        let Some((_, pts)) = best_pts else {
            break;
        };
        for seg in crate::routing::coalesced(&pts).windows(2) {
            let (lx, rx) = (seg[0].x.min(seg[1].x) - 14., seg[0].x.max(seg[1].x) + 14.);
            let (ty, by) = (seg[0].y.min(seg[1].y) - 14., seg[0].y.max(seg[1].y) + 14.);
            includes.push(Rect::new((lx + rx) / 2., (ty + by) / 2., rx - lx, by - ty));
        }
    }
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for r in includes.iter().chain(&exclusions) {
        xs.extend([r.left(), r.right()]);
        ys.extend([r.top(), r.bottom()]);
    }
    dedup_coords(&mut xs);
    dedup_coords(&mut ys);
    if xs.len() < 2 || ys.len() < 2 {
        return Vec::new();
    }
    let width = xs.len() - 1;
    let height = ys.len() - 1;
    let filled: Vec<_> = (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .map(|(x, y)| {
            let p = Point::new((xs[x] + xs[x + 1]) / 2., (ys[y] + ys[y + 1]) / 2.);
            includes.iter().any(|r| r.contains(p)) && !exclusions.iter().any(|r| r.contains(p))
        })
        .collect();
    let cell = |x: isize, y: isize| {
        x >= 0
            && y >= 0
            && x < width as isize
            && y < height as isize
            && filled[y as usize * width + x as usize]
    };
    type Vertex = (usize, usize);
    let mut edges = BTreeMap::<Vertex, Vec<Vertex>>::new();
    for y in 0..height {
        for x in 0..width {
            if !cell(x as isize, y as isize) {
                continue;
            }
            for (neighbor, a, b) in [
                ((x as isize, y as isize - 1), (x, y), (x + 1, y)),
                ((x as isize + 1, y as isize), (x + 1, y), (x + 1, y + 1)),
                ((x as isize, y as isize + 1), (x + 1, y + 1), (x, y + 1)),
                ((x as isize - 1, y as isize), (x, y + 1), (x, y)),
            ] {
                if !cell(neighbor.0, neighbor.1) {
                    edges.entry(a).or_default().push(b);
                }
            }
        }
    }
    let direction = |a: Vertex, b: Vertex| {
        if b.0 > a.0 {
            0
        } else if b.1 > a.1 {
            1
        } else if b.0 < a.0 {
            2
        } else {
            3
        }
    };
    let mut result = Vec::new();
    while let Some((&start, _)) = edges.first_key_value() {
        let mut at = start;
        let mut previous = 3;
        let mut polygon = Vec::new();
        loop {
            polygon.push(Point::new(xs[at.0], ys[at.1]));
            let choices = edges.get_mut(&at).unwrap();
            let i = choices
                .iter()
                .enumerate()
                .min_by_key(|(_, to)| match (direction(at, **to) + 4 - previous) % 4 {
                    1 => 0,
                    0 => 1,
                    3 => 2,
                    _ => 3,
                })
                .unwrap()
                .0;
            let next = choices.remove(i);
            if choices.is_empty() {
                edges.remove(&at);
            }
            previous = direction(at, next);
            at = next;
            if at == start {
                break;
            }
        }
        let simplified: Vec<_> = (0..polygon.len())
            .filter_map(|i| {
                let a = polygon[(i + polygon.len() - 1) % polygon.len()];
                let b = polygon[i];
                let c = polygon[(i + 1) % polygon.len()];
                if (a.x == b.x && b.x == c.x) || (a.y == b.y && b.y == c.y) {
                    None
                } else {
                    Some(b)
                }
            })
            .collect();
        if simplified.len() >= 3 {
            result.push(simplified);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unrelated_nodes_are_excluded_even_between_members() {
        let d=crate::compile("a @pin(grid: 0,0)\nb @pin(grid: 20,0)\nx @pin(grid: 10,0)\nzone scope around [a,b] { padding: 20 }\n").unwrap();
        let polygons = contours(&d.scene, &d.layout, &d.scene.zones[0]);
        assert!(!polygons.is_empty());
        let p = d.layout.nodes["x"].center();
        let crossings = polygons
            .iter()
            .flat_map(|poly| (0..poly.len()).map(move |i| (poly[i], poly[(i + 1) % poly.len()])))
            .filter(|(a, b)| {
                (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x
            })
            .count();
        assert_eq!(
            crossings % 2,
            0,
            "the excluded node must be outside even-odd contour fill"
        );
    }
}
