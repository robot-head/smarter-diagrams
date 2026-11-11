//! Continuous gesture geometry. Semantic constraints are committed only on drop.
use crate::{Error, edit::Drag, layout::*, model::*, render, routing};
use std::collections::BTreeMap;

pub(crate) struct Preview {
    scene: SceneGraph,
    baseline: Layout,
    layout: Layout,
    label_offsets: BTreeMap<String, Point>,
}
impl Preview {
    pub fn new(scene: &SceneGraph, layout: &Layout) -> Self {
        Self {
            scene: scene.clone(),
            baseline: layout.clone(),
            layout: layout.clone(),
            label_offsets: label_offsets(layout),
        }
    }
    #[cfg(feature = "wasm")]
    pub fn svg(&self) -> String {
        render::svg(&self.scene, &self.layout)
    }
    pub fn update(&mut self, drags: &[Drag]) -> Result<(), Error> {
        let mut offsets = BTreeMap::new();
        for drag in drags {
            if !drag.position.x.is_finite() || !drag.position.y.is_finite() {
                return Err(Error::Edit("drag coordinates must be finite".into()));
            }
            let r = self
                .baseline
                .nodes
                .get(&drag.node)
                .ok_or_else(|| Error::Edit(format!("unknown node {}", drag.node)))?;
            if offsets
                .insert(
                    drag.node.clone(),
                    Point::new(drag.position.x - r.x, drag.position.y - r.y),
                )
                .is_some()
            {
                return Err(Error::Edit("duplicate drag node".into()));
            }
        }
        let mut next = self.layout.clone();
        let mut deltas = BTreeMap::new();
        for node in &self.scene.nodes {
            let mut owner = Some(node.id.as_str());
            let mut delta = Point::default();
            while let Some(id) = owner {
                if let Some(offset) = offsets.get(id) {
                    delta = *offset;
                    break;
                }
                owner = self.scene.node(id).and_then(|n| n.parent.as_deref());
            }
            let mut r = self.baseline.nodes[&node.id];
            r.x += delta.x;
            r.y += delta.y;
            if !r.x.is_finite() || !r.y.is_finite() {
                return Err(Error::Edit("drag coordinates exceed finite range".into()));
            }
            let old = self.layout.nodes[&node.id];
            deltas.insert(node.id.clone(), Point::new(r.x - old.x, r.y - old.y));
            next.nodes.insert(node.id.clone(), r);
        }
        let mut owners = BTreeMap::new();
        for edge in &self.scene.edges {
            owners.insert(port_key(edge, true), &edge.source.node);
            owners.insert(port_key(edge, false), &edge.target.node);
        }
        for (key, port) in &mut next.ports {
            let owner = owners
                .get(key)
                .map(|id| id.as_str())
                .or_else(|| key.rsplit_once('#').map(|(node, _)| node));
            if let Some(delta) = owner.and_then(|node| deltas.get(node)) {
                port.point = port.point.offset(delta.x, delta.y);
                port.stub = port.stub.offset(delta.x, delta.y);
            }
        }
        for edge in &self.scene.edges {
            let (a, b) = (deltas[&edge.source.node], deltas[&edge.target.node]);
            if a == Point::default() && b == Point::default() {
                continue;
            }
            let route = next.routes.get_mut(&edge.id).unwrap();
            if a == b {
                for point in &mut route.points {
                    *point = point.offset(a.x, a.y)
                }
            } else {
                route.points = elastic(
                    &route.points,
                    &next.ports[&port_key(edge, true)],
                    &next.ports[&port_key(edge, false)],
                    next.nodes[&edge.source.node],
                    next.nodes[&edge.target.node],
                );
            }
        }
        follow_labels(&self.scene, &self.layout, &mut next, &self.label_offsets);
        for zone in &self.scene.zones {
            let padding = value(&zone.attrs, "padding").unwrap_or(1.5) * GRID;
            let points = zone
                .members
                .iter()
                .filter_map(|id| next.nodes.get(id))
                .flat_map(|r| {
                    let r = r.inflate(padding);
                    [
                        Point::new(r.left(), r.top()),
                        Point::new(r.right(), r.top()),
                        Point::new(r.right(), r.bottom()),
                        Point::new(r.left(), r.bottom()),
                    ]
                })
                .collect();
            next.zones.insert(zone.id.clone(), convex_hull(points));
        }
        self.layout = next;
        Ok(())
    }
    pub fn settle(&mut self) -> Result<(), Error> {
        let mut prior = self.layout.clone();
        for edge in &self.scene.edges {
            let route = &prior.routes[&edge.id];
            let endpoints = [
                prior.nodes[&edge.source.node].inflate(8.),
                prior.nodes[&edge.target.node].inflate(8.),
            ];
            if route.points.windows(2).enumerate().any(|(i, p)| {
                i > 0
                    && i + 2 < route.points.len()
                    && endpoints.iter().any(|r| r.blocks(p[0], p[1]))
            }) {
                prior.routes.remove(&edge.id);
            }
        }
        let mut next = self.layout.clone();
        routing::route_all(
            &self.scene,
            &LayoutOptions::default(),
            Some(&prior),
            &mut next,
        )?;
        follow_labels(&self.scene, &self.layout, &mut next, &self.label_offsets);
        // Caption search and obstacle repair happen after the pointer pauses.
        // Node coordinates and valid route channels remain untouched.
        if !crate::labels::placements_clear(&self.scene, &next) {
            next.labels = crate::labels::place(&self.scene, &LayoutOptions::default(), &next)?;
            self.label_offsets = label_offsets(&next);
        }
        self.layout = next;
        Ok(())
    }
}

fn label_offsets(layout: &Layout) -> BTreeMap<String, Point> {
    layout
        .labels
        .iter()
        .map(|label| {
            (
                label.feature.clone(),
                Point::new(
                    label.bounds.x - label.anchor.x,
                    label.bounds.y - label.anchor.y,
                ),
            )
        })
        .collect()
}
fn follow_labels(
    scene: &SceneGraph,
    old: &Layout,
    next: &mut Layout,
    offsets: &BTreeMap<String, Point>,
) {
    for label in &mut next.labels {
        let delta = if let Some(node) = scene.node(&label.feature) {
            let a = old.nodes[&node.id];
            let b = next.nodes[&node.id];
            Point::new(b.x - a.x, b.y - a.y)
        } else if let Some((a, b)) = old
            .ports
            .get(&label.feature)
            .zip(next.ports.get(&label.feature))
        {
            Point::new(b.point.x - a.point.x, b.point.y - a.point.y)
        } else if let Some(route) = next.routes.get(&label.feature) {
            if old.routes[&label.feature].points == route.points {
                continue;
            }
            let offset = offsets[&label.feature];
            let on_horizontal = if label.callout {
                offset.y.abs() >= offset.x.abs()
            } else {
                label.horizontal
            };
            let edge = scene.edge(&label.feature).unwrap();
            let a = next.nodes[&edge.source.node];
            let b = old.nodes[&edge.source.node];
            let c = next.nodes[&edge.target.node];
            let d = old.nodes[&edge.target.node];
            let (source, target) = (
                Point::new(a.x - b.x, a.y - b.y),
                Point::new(c.x - d.x, c.y - d.y),
            );
            let desired = if source == target {
                label.anchor.offset(source.x, source.y)
            } else {
                label.anchor
            };
            // Keep the caption on its existing horizontal/vertical run instead
            // of chasing the route's changing halfway point around every bend.
            let anchor = route
                .points
                .windows(2)
                .filter(|p| {
                    p[0].distance(p[1]) > 1e-7 && ((p[0].y - p[1].y).abs() < 1e-7) == on_horizontal
                })
                .map(|p| {
                    Point::new(
                        desired.x.clamp(p[0].x.min(p[1].x), p[0].x.max(p[1].x)),
                        desired.y.clamp(p[0].y.min(p[1].y), p[0].y.max(p[1].y)),
                    )
                })
                .min_by(|a, b| a.distance(desired).total_cmp(&b.distance(desired)))
                .unwrap_or_else(|| render::at_arc(&route.points, label.slide));
            label.bounds.x = anchor.x + offset.x;
            label.bounds.y = anchor.y + offset.y;
            label.anchor = anchor;
            if !label.knockout {
                for p in route.points.windows(2) {
                    if on_horizontal
                        && p[0].x.min(p[1].x) <= label.bounds.right() + 4.
                        && p[0].x.max(p[1].x) >= label.bounds.left() - 4.
                    {
                        if offset.y < 0. {
                            label.bounds.y = label
                                .bounds
                                .y
                                .min(p[0].y.min(p[1].y) - label.bounds.h / 2. - 6.)
                        } else {
                            label.bounds.y = label
                                .bounds
                                .y
                                .max(p[0].y.max(p[1].y) + label.bounds.h / 2. + 6.)
                        }
                    } else if !on_horizontal
                        && p[0].y.min(p[1].y) <= label.bounds.bottom() + 4.
                        && p[0].y.max(p[1].y) >= label.bounds.top() - 4.
                    {
                        if offset.x < 0. {
                            label.bounds.x = label
                                .bounds
                                .x
                                .min(p[0].x.min(p[1].x) - label.bounds.w / 2. - 6.)
                        } else {
                            label.bounds.x = label
                                .bounds
                                .x
                                .max(p[0].x.max(p[1].x) + label.bounds.w / 2. + 6.)
                        }
                    }
                }
            }
            continue;
        } else {
            Point::default()
        };
        label.bounds.x += delta.x;
        label.bounds.y += delta.y;
        label.anchor = label.anchor.offset(delta.x, delta.y);
    }
}

fn horizontal(side: Side) -> bool {
    matches!(side, Side::E | Side::W)
}
fn elastic(old: &[Point], a: &PortPlacement, b: &PortPlacement, sr: Rect, tr: Rect) -> Vec<Point> {
    let mut points = routing::coalesced(old);
    // A channel can fold back on itself while an endpoint passes it. Remove
    // those transient collinear reversals before adjusting its perpendiculars.
    let mut i = 1;
    while i + 1 < points.len() {
        let (a, b, c) = (points[i - 1], points[i], points[i + 1]);
        if ((a.x - b.x).abs() < 1e-7 && (b.x - c.x).abs() < 1e-7)
            || ((a.y - b.y).abs() < 1e-7 && (b.y - c.y).abs() < 1e-7)
        {
            points.remove(i);
        } else {
            i += 1;
        }
    }
    let (ah, bh) = (horizontal(a.side), horizontal(b.side));
    if points.len() <= 2 {
        let middle = if ah {
            (old[1].x + old[old.len() - 2].x) / 2.
        } else {
            (old[1].y + old[old.len() - 2].y) / 2.
        };
        points = if ah {
            vec![
                a.point,
                Point::new(middle, a.point.y),
                Point::new(middle, b.point.y),
                b.point,
            ]
        } else {
            vec![
                a.point,
                Point::new(a.point.x, middle),
                Point::new(b.point.x, middle),
                b.point,
            ]
        };
    }
    let last = points.len() - 1;
    points[0] = a.point;
    points[last] = b.point;
    if points.len() == 3 {
        let corner = if ah {
            Point::new(b.point.x, a.point.y)
        } else {
            Point::new(a.point.x, b.point.y)
        };
        let (ax, ay) = a.side.normal();
        let (bx, by) = b.side.normal();
        let reaches = |p: Point, stub: Point, nx: f64, ny: f64| {
            (p.x - stub.x) * nx + (p.y - stub.y) * ny >= 0.
        };
        points = if reaches(corner, a.stub, ax, ay) && reaches(corner, b.stub, bx, by) {
            vec![a.point, corner, b.point]
        } else if ah {
            vec![
                a.point,
                a.stub,
                Point::new(a.stub.x, b.stub.y),
                b.stub,
                b.point,
            ]
        } else {
            vec![
                a.point,
                a.stub,
                Point::new(b.stub.x, a.stub.y),
                b.stub,
                b.point,
            ]
        };
    } else {
        if ah {
            points[1].y = a.point.y
        } else {
            points[1].x = a.point.x
        }
        if bh {
            points[last - 1].y = b.point.y
        } else {
            points[last - 1].x = b.point.x
        }
        if points.len() == 4 && ah == bh {
            let (start, end) = if ah {
                (a.stub.x, b.stub.x)
            } else {
                (a.stub.y, b.stub.y)
            };
            let (an, bn) = if ah {
                (a.side.normal().0, b.side.normal().0)
            } else {
                (a.side.normal().1, b.side.normal().1)
            };
            let mut low = f64::NEG_INFINITY;
            let mut high = f64::INFINITY;
            for (value, normal) in [(start, an), (end, bn)] {
                if normal > 0. {
                    low = low.max(value)
                } else {
                    high = high.min(value)
                }
            }
            if low <= high {
                if ah {
                    let lane = points[1].x.clamp(low, high);
                    points[1].x = lane;
                    points[2].x = lane
                } else {
                    let lane = points[1].y.clamp(low, high);
                    points[1].y = lane;
                    points[2].y = lane
                }
            } else if ah {
                let lane = sr.top().min(tr.top()) - 16.;
                points = vec![
                    a.point,
                    a.stub,
                    Point::new(a.stub.x, lane),
                    Point::new(b.stub.x, lane),
                    b.stub,
                    b.point,
                ];
            } else {
                let lane = sr.left().min(tr.left()) - 16.;
                points = vec![
                    a.point,
                    a.stub,
                    Point::new(lane, a.stub.y),
                    Point::new(lane, b.stub.y),
                    b.stub,
                    b.point,
                ];
            }
        } else {
            let last = points.len() - 1;
            for (source, port) in [(true, a), (false, b)] {
                let (i, j) = if source { (1, 2) } else { (last - 1, last - 2) };
                let (nx, ny) = port.side.normal();
                if horizontal(port.side) && (points[i].x - port.stub.x) * nx < 0. {
                    points[i].x = port.stub.x;
                    points[j].x = port.stub.x
                }
                if !horizontal(port.side) && (points[i].y - port.stub.y) * ny < 0. {
                    points[i].y = port.stub.y;
                    points[j].y = port.stub.y
                }
            }
        }
    }
    let mut out = vec![a.point, a.stub];
    out.extend_from_slice(&points[1..points.len() - 1]);
    out.extend([b.stub, b.point]);
    out.dedup_by(|a, b| a.distance(*b) < 1e-7);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn motion_crosses_snap_thresholds_without_moving_peers_or_rounding() {
        let diagram = crate::compile("a -> b\nb -> c\nalign row [a,b,c]\n").unwrap();
        let mut preview = Preview::new(&diagram.scene, &diagram.layout);
        let start = diagram.layout.nodes["b"];
        for y in [2., 5., 6., 7., 8., 9., 15., 17., 23., 25.] {
            preview
                .update(&[Drag {
                    node: "b".into(),
                    position: Point::new(start.x + 64.25, y),
                }])
                .unwrap();
            assert_eq!(
                preview.layout.nodes["b"].center(),
                Point::new(start.x + 64.25, y)
            );
            for id in ["a", "c"] {
                assert_eq!(preview.layout.nodes[id], diagram.layout.nodes[id]);
            }
            for edge in &diagram.scene.edges {
                let points = &preview.layout.routes[&edge.id].points;
                assert_eq!(points[0], preview.layout.ports[&port_key(edge, true)].point);
                assert_eq!(
                    *points.last().unwrap(),
                    preview.layout.ports[&port_key(edge, false)].point
                );
                assert!(
                    points
                        .windows(2)
                        .all(|p| (p[0].x - p[1].x).abs() < 1e-7 || (p[0].y - p[1].y).abs() < 1e-7)
                );
            }
        }
        preview.settle().unwrap();
        for id in ["a", "c"] {
            assert_eq!(preview.layout.nodes[id], diagram.layout.nodes[id]);
        }
    }
    #[test]
    fn container_preview_translates_its_children_and_named_ports() {
        let diagram =
            crate::compile("prod {\n api { port out [side: E] }\n db\n api#out -> db\n}\nother\n")
                .unwrap();
        let mut preview = Preview::new(&diagram.scene, &diagram.layout);
        let r = diagram.layout.nodes["prod"];
        preview
            .update(&[Drag {
                node: "prod".into(),
                position: r.center().offset(47.5, 31.25),
            }])
            .unwrap();
        for id in ["prod", "prod.api", "prod.db"] {
            let before = diagram.layout.nodes[id];
            assert!(
                preview.layout.nodes[id]
                    .center()
                    .distance(before.center().offset(47.5, 31.25))
                    < 1e-7
            );
        }
        assert_eq!(preview.layout.nodes["other"], diagram.layout.nodes["other"]);
        for (key, port) in &diagram.layout.ports {
            assert!(
                preview.layout.ports[key]
                    .point
                    .distance(port.point.offset(47.5, 31.25))
                    < 1e-7
            );
        }
        assert!(diagram.layout.ports.keys().any(|key| key == "prod.api#out"));
    }
    #[test]
    fn elastic_connections_keep_cardinal_attachments_in_every_direction() {
        for source in ["N", "E", "S", "W"] {
            for target in ["N", "E", "S", "W"] {
                let diagram = crate::compile(&format!(
                    "a @pin(grid: 0,0)\nb @pin(grid: 20,10)\na.{source} -> b.{target}\n"
                ))
                .unwrap();
                let mut preview = Preview::new(&diagram.scene, &diagram.layout);
                for point in [
                    Point::new(13.25, 17.75),
                    Point::new(64., -32.),
                    Point::new(335., 128.),
                    Point::new(13.25, 17.75),
                ] {
                    preview
                        .update(&[Drag {
                            node: "a".into(),
                            position: point,
                        }])
                        .unwrap();
                    let edge = &diagram.scene.edges[0];
                    let route = &preview.layout.routes[&edge.id].points;
                    assert_eq!(route[0], preview.layout.ports[&port_key(edge, true)].point);
                    assert_eq!(
                        *route.last().unwrap(),
                        preview.layout.ports[&port_key(edge, false)].point
                    );
                    assert!(
                        route
                            .windows(2)
                            .all(|p| (p[0].x - p[1].x).abs() < 1e-7
                                || (p[0].y - p[1].y).abs() < 1e-7),
                        "{source} -> {target}: {route:?}"
                    );
                    assert_eq!(preview.layout.nodes["b"], diagram.layout.nodes["b"]);
                }
                preview.settle().unwrap();
                crate::layout::validate(&diagram.scene, &preview.layout, &LayoutOptions::default())
                    .unwrap();
            }
        }
    }
    #[test]
    fn captions_stay_clear_of_their_stretching_connectors() {
        let diagram = crate::compile(include_str!("../examples/simulation.diagram")).unwrap();
        let mut preview = Preview::new(&diagram.scene, &diagram.layout);
        for y in [64., -64., 2., 9., 128., 0.] {
            preview
                .update(&[Drag {
                    node: "gateway".into(),
                    position: Point::new(272., y),
                }])
                .unwrap();
            for label in &preview.layout.labels {
                if label.knockout {
                    continue;
                }
                if let Some(route) = preview.layout.routes.get(&label.feature) {
                    assert!(
                        route
                            .points
                            .windows(2)
                            .all(|p| !crate::labels::segment_hits(p[0], p[1], label.bounds)),
                        "{} at {y}",
                        label.feature
                    );
                }
            }
        }
    }
    #[test]
    fn idle_settlement_keeps_captions_that_already_have_clearance() {
        let diagram = crate::compile(include_str!("../examples/simulation.diagram")).unwrap();
        let mut preview = Preview::new(&diagram.scene, &diagram.layout);
        preview
            .update(&[Drag {
                node: "gateway".into(),
                position: Point::new(272., 64.),
            }])
            .unwrap();
        assert!(crate::labels::placements_clear(
            &preview.scene,
            &preview.layout
        ));
        let captions = preview.layout.labels.clone();
        preview.settle().unwrap();
        assert_eq!(preview.layout.labels, captions);
    }
}
