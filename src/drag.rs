//! Transactional canvas gestures: preview and commit share one validated diagram.
use crate::{
    Diagram, Error, compile_with,
    edit::{self, DiagramEditOp, Drag, PinExpr, Placement},
    layout::*,
    model::*,
};
use std::{collections::BTreeMap, sync::Arc};

/// A gesture owns its original snapshot and prepares complete, valid candidates.
/// Source and history change only when `Document::commit_drag` accepts a candidate.
#[derive(Debug, Clone)]
pub struct DragSession {
    pub(crate) baseline: Arc<Diagram>,
    pub(crate) revision: u64,
    pub(crate) prepared: Arc<Diagram>,
    options: LayoutOptions,
    anchors: String,
    last: Vec<Drag>,
    pub(crate) valid: bool,
}
impl DragSession {
    pub(crate) fn new(
        baseline: Arc<Diagram>,
        revision: u64,
        options: LayoutOptions,
    ) -> Result<Self, Error> {
        let mut anchors = baseline.parse.source.to_string();
        // Capture automatic positions once. Otherwise adding the first pin can
        // disable shape search and change unrelated nodes on a subsequent render.
        for node in &baseline.scene.nodes {
            anchors = edit::anchor_geometry(
                &anchors,
                &node.id,
                baseline.layout.nodes[&node.id],
                options.grid,
            )?;
        }
        Ok(Self {
            prepared: baseline.clone(),
            baseline,
            revision,
            options,
            anchors,
            last: Vec::new(),
            valid: true,
        })
    }
    /// The exact candidate that will be saved; failed updates retain the last good image.
    pub fn snapshot(&self) -> Arc<Diagram> {
        self.prepared.clone()
    }
    /// Prepare the current pointer position from the immutable gesture baseline.
    /// Coordinates retain sub-grid precision; snapping is an explicit semantic edit.
    pub fn update(&mut self, drags: &[Drag]) -> Result<(), Error> {
        if self.valid && self.last == drags {
            return Ok(());
        }
        self.valid = false;
        let result = self.prepare(drags);
        match result {
            Ok(candidate) => {
                self.prepared = candidate;
                self.last = drags.to_vec();
                self.valid = true;
                Ok(())
            }
            Err(error) => Err(error),
        }
    }
    fn prepare(&self, drags: &[Drag]) -> Result<Arc<Diagram>, Error> {
        let mut requests = BTreeMap::new();
        for drag in drags {
            if !drag.position.x.is_finite() || !drag.position.y.is_finite() {
                return Err(Error::Edit("drag coordinates must be finite".into()));
            }
            let node = self
                .baseline
                .layout
                .nodes
                .get(&drag.node)
                .ok_or_else(|| Error::Edit(format!("unknown node {}", drag.node)))?;
            if requests
                .insert(
                    drag.node.clone(),
                    Point::new(drag.position.x - node.x, drag.position.y - node.y),
                )
                .is_some()
            {
                return Err(Error::Edit("duplicate drag node".into()));
            }
        }
        let mut moved = BTreeMap::new();
        for node in &self.baseline.scene.nodes {
            let mut owner = Some(node.id.as_str());
            while let Some(id) = owner {
                if let Some(delta) = requests.get(id) {
                    if delta.distance(Point::default()) > 1e-7 {
                        let position = self.baseline.layout.nodes[&node.id]
                            .center()
                            .offset(delta.x, delta.y);
                        if !position.x.is_finite() || !position.y.is_finite() {
                            return Err(Error::Edit("drag coordinates exceed finite range".into()));
                        }
                        moved.insert(node.id.clone(), position);
                    }
                    break;
                }
                owner = self
                    .baseline
                    .scene
                    .node(id)
                    .and_then(|n| n.parent.as_deref());
            }
        }
        if moved.is_empty() {
            return Ok(self.baseline.clone());
        }
        let mut ops = Vec::new();
        for (id, position) in &moved {
            for axis in [Axis::Row, Axis::Col] {
                let displacement = |node: &str| {
                    let old = self.baseline.layout.nodes[node].center();
                    let new = moved.get(node).copied().unwrap_or(old);
                    if axis == Axis::Row {
                        new.y - old.y
                    } else {
                        new.x - old.x
                    }
                };
                if self.baseline.scene.relations.iter().any(|r| {
                    !r.distribute
                        && r.axis == axis
                        && r.members.contains(id)
                        && r.members
                            .iter()
                            .any(|peer| (displacement(peer) - displacement(id)).abs() > 1e-7)
                }) {
                    ops.push(DiagramEditOp::BreakAlignment {
                        node: id.clone(),
                        axis,
                    });
                }
            }
            ops.push(DiagramEditOp::PinNode {
                node: id.clone(),
                pin: PinExpr::Relative {
                    x: format!("{}g", position.x / self.options.grid),
                    y: format!("{}g", position.y / self.options.grid),
                },
                size_lock: None,
            });
        }
        let source = edit::apply_ops(&self.anchors, &ops, Placement::Layout)?.source;
        // Compile the anchored geometry with the last validated layout supplying
        // attachment switching costs and checked route reuse.
        let mut candidate = compile_with(&source, &self.options, Some(&self.prepared.layout))?;
        // Prefer the immutable gesture baseline for stationary connections.
        // Moving obstacles must still invalidate an old path. Stateful detour
        // retention below has separate validity and excess-length bounds.
        let mut stable = candidate.layout.clone();
        let mut changed = false;
        for edge in &candidate.scene.edges {
            let source = port_key(edge, true);
            let target = port_key(edge, false);
            if stable.ports.get(&source) == self.baseline.layout.ports.get(&source)
                && stable.ports.get(&target) == self.baseline.layout.ports.get(&target)
                && let Some(route) = self.baseline.layout.routes.get(&edge.id)
                && crate::routing::route_is_valid(
                    &candidate.scene,
                    &stable,
                    edge,
                    route,
                    self.options.margin,
                )
                && stable.routes.get(&edge.id) != Some(route)
            {
                stable.routes.insert(edge.id.clone(), route.clone());
                changed = true;
            }
        }
        let mut retained = std::collections::BTreeSet::new();
        for edge in &candidate.scene.edges {
            if effective_annotations(
                &candidate.scene,
                &edge.id,
                &edge.annotations,
                self.options.variant.as_deref(),
            )
            .iter()
            .any(|a| a.name == "route")
            {
                continue;
            }
            if let Some(route) = crate::routing::retain_detour(
                &candidate.scene,
                &stable,
                edge,
                &self.prepared.layout,
                self.options.margin,
            ) {
                stable.routes.insert(edge.id.clone(), route);
                retained.insert(edge.id.clone());
                changed = true;
            }
        }
        if changed {
            // Refit captions to the retained paths. If that cannot be done
            // safely, the fully validated canonical candidate remains available.
            if let Ok(labels) = crate::labels::place(&candidate.scene, &self.options, &stable) {
                stable.labels = labels;
                // Keep a detour caption's slot until it is actually obstructed,
                // instead of reranking equally good slots every pointer frame.
                for old in &self.prepared.layout.labels {
                    if !retained.contains(&old.feature) {
                        continue;
                    }
                    if let Some(edge) = candidate.scene.edge(&old.feature)
                        && effective_annotations(
                            &candidate.scene,
                            &edge.id,
                            &edge.annotations,
                            self.options.variant.as_deref(),
                        )
                        .iter()
                        .any(|a| a.name == "label")
                    {
                        continue;
                    }

                    let Some(index) = stable.labels.iter().position(|l| l.feature == old.feature)
                    else {
                        continue;
                    };
                    let route = &stable.routes[&old.feature];
                    let anchor = route
                        .points
                        .windows(2)
                        .map(|p| {
                            let dx = p[1].x - p[0].x;
                            let dy = p[1].y - p[0].y;
                            let t = (((old.anchor.x - p[0].x) * dx + (old.anchor.y - p[0].y) * dy)
                                / (dx * dx + dy * dy))
                                .clamp(0., 1.);
                            Point::new(p[0].x + t * dx, p[0].y + t * dy)
                        })
                        .min_by(|a, b| a.distance(old.anchor).total_cmp(&b.distance(old.anchor)))
                        .unwrap();
                    let mut label = old.clone();
                    label.bounds.x += anchor.x - old.anchor.x;
                    label.bounds.y += anchor.y - old.anchor.y;
                    label.anchor = anchor;
                    let fresh = std::mem::replace(&mut stable.labels[index], label);
                    if !crate::labels::placements_clear(&candidate.scene, &stable) {
                        stable.labels[index] = fresh;
                    }
                }

                if crate::layout::validate(&candidate.scene, &stable, &self.options).is_ok()
                    && crate::labels::placements_clear(&candidate.scene, &stable)
                {
                    candidate.layout = stable;
                }
            }
        }
        for (id, position) in moved {
            if candidate.layout.nodes[&id].center().distance(position) > 1e-5 {
                return Err(Error::Edit(format!(
                    "placement of {id} conflicts with its layout constraints"
                )));
            }
        }
        if !crate::labels::placements_clear(&candidate.scene, &candidate.layout) {
            return Err(Error::Layout(
                "placement leaves a caption obstructed".into(),
            ));
        }
        Ok(Arc::new(candidate))
    }
    /// Validate without changing routes. Kept for hosts using the preview API.
    pub fn settle(&self) -> Result<(), Error> {
        if !self.valid {
            return Err(Error::Edit("the current placement is invalid".into()));
        }
        crate::layout::validate(&self.prepared.scene, &self.prepared.layout, &self.options)
    }
}

#[cfg(test)]
mod neighbor_tests {
    use super::*;
    #[test]
    fn stationary_paths_remain_fixed_until_they_are_obstructed() {
        let doc = crate::Document::new(
            "a @pin(grid: 0,0)\nb @pin(grid: 24,12)\nm @pin(grid: 12,24)\nm.E -> a.W\na.E -> b.W\n",
        )
        .unwrap();
        let mut saved = (*doc.snapshot()).clone();
        let edge = saved.scene.edge("a->b").unwrap().clone();
        let a = &saved.layout.ports[&port_key(&edge, true)];
        let b = &saved.layout.ports[&port_key(&edge, false)];
        // A valid existing corridor, rather than whichever equal-cost corridor
        // a fresh A* search happens to choose on this visibility lattice.
        saved.layout.routes.get_mut(&edge.id).unwrap().points = vec![
            a.point,
            a.stub,
            Point::new(176., a.stub.y),
            Point::new(176., b.stub.y),
            b.stub,
            b.point,
        ];
        saved.layout.labels =
            crate::labels::place(&saved.scene, &LayoutOptions::default(), &saved.layout).unwrap();
        crate::layout::validate(&saved.scene, &saved.layout, &LayoutOptions::default()).unwrap();
        let baseline = Arc::new(saved);
        let original = &baseline.layout.routes[&edge.id];
        let mut gesture = DragSession::new(baseline.clone(), 0, LayoutOptions::default()).unwrap();
        let mut retained = 0;
        let mut unnecessary_reroutes = 0;
        for x in [96., 160., 224., 288., 352.] {
            for y in [-160., 224., 320., 400.] {
                if gesture
                    .update(&[Drag {
                        node: "m".into(),
                        position: Point::new(x, y),
                    }])
                    .is_err()
                {
                    continue;
                }
                let shown = gesture.snapshot();
                if crate::routing::route_is_valid(&shown.scene, &shown.layout, &edge, original, 8.)
                {
                    assert_eq!(
                        &shown.layout.routes[&edge.id], original,
                        "neighbor changed at {x},{y}"
                    );
                    let fresh = crate::compile(&shown.parse.source).unwrap();
                    if fresh.layout.routes[&edge.id] != *original {
                        unnecessary_reroutes += 1;
                    }
                    retained += 1;
                }
                gesture.settle().unwrap();
            }
        }
        assert!(retained >= 10);
        assert!(
            unnecessary_reroutes > 0,
            "fixture must exercise a route that fresh search would change"
        );
        // Move the obstacle onto the old shaft. Keeping that path would be
        // unsafe: a fresh route must replace it or the update must be rejected.
        let segment = original
            .points
            .windows(2)
            .max_by(|a, b| a[0].distance(a[1]).total_cmp(&b[0].distance(b[1])))
            .unwrap();
        let position = Point::new(
            (segment[0].x + segment[1].x) / 2.,
            (segment[0].y + segment[1].y) / 2.,
        );
        if gesture
            .update(&[Drag {
                node: "m".into(),
                position,
            }])
            .is_ok()
        {
            assert_ne!(&gesture.snapshot().layout.routes[&edge.id], original);
            gesture.settle().unwrap();
        }
    }
}
