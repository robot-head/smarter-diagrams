//! Rectilinear union contours with holes for unrelated nodes. Membership never
//! implies ownership, and disconnected members may form disconnected islands.
use crate::{layout::*, model::*};
use std::collections::{BTreeMap, BTreeSet};

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
    let padding = value(&zone.attrs, "padding").unwrap_or(1.5) * GRID;
    let mut includes = Vec::new();
    for id in &members {
        let mut r = layout.nodes[id].inflate(padding);
        for parent in ancestors(scene, id)
            .iter()
            .filter(|p| !members.contains(*p))
        {
            let p = layout.nodes[parent];
            let (left, right, top, bottom) = (
                r.left().max(p.left() + 6.),
                r.right().min(p.right() - 6.),
                r.top()
                    .max(p.top() + header_height(scene.node(parent).unwrap()) + 8.),
                r.bottom().min(p.bottom() - 6.),
            );
            r = Rect::new(
                (left + right) / 2.,
                (top + bottom) / 2.,
                right - left,
                bottom - top,
            );
        }
        if r.w > 0. && r.h > 0. {
            includes.push(r);
        }
    }
    let exclusions: Vec<_> = scene
        .nodes
        .iter()
        .filter(|n| {
            !members.contains(&n.id) && !members.iter().any(|m| ancestors(scene, m).contains(&n.id))
        })
        .map(|n| layout.nodes[&n.id].inflate(8.))
        .collect();
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for r in includes.iter().chain(&exclusions) {
        xs.extend([r.left(), r.right()]);
        ys.extend([r.top(), r.bottom()]);
    }
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    ys.sort_by(f64::total_cmp);
    ys.dedup();
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
