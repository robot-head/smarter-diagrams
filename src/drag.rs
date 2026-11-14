//! Transactional canvas gestures: preview and commit share one validated diagram.
use crate::{
    Diagram, Error, compile_with,
    edit::{self, DiagramEditOp, Drag, PinExpr, Placement},
    layout::*,
    model::*,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

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
        let mut broken: BTreeSet<(NodeId, bool)> = BTreeSet::new();
        for r in &self.baseline.scene.relations {
            let is_x = edit::relation_constrains_x(r);
            let is_row = r.axis == Axis::Row;
            let disp = |node: &str| {
                let Some(rect) = self.baseline.layout.nodes.get(node) else {
                    return 0.;
                };
                let old = rect.center();
                let new = moved.get(node).copied().unwrap_or(old);
                if is_x { new.x - old.x } else { new.y - old.y }
            };
            for id in moved.keys() {
                if r.members.contains(id)
                    && r.members
                        .iter()
                        .any(|peer| (disp(peer) - disp(id)).abs() > 1e-7)
                {
                    broken.insert((id.clone(), is_row));
                }
            }
        }
        loop {
            let mut changed = false;
            for r in &self.baseline.scene.relations {
                if !r.distribute {
                    continue;
                }
                let is_row = r.axis == Axis::Row;
                let remaining: Vec<&str> = r
                    .members
                    .iter()
                    .filter(|m| !broken.contains(&(m.to_string(), is_row)))
                    .map(String::as_str)
                    .collect();
                let min_remaining = if r.gap_grid.is_some() { 2 } else { 3 };
                if remaining.len() >= min_remaining
                    && remaining.len() < r.members.len()
                    && !edit::distribute_subset_valid(
                        r,
                        &remaining,
                        &self.baseline.layout.nodes,
                        &moved,
                        self.options.grid,
                    )
                {
                    for m in remaining {
                        changed |= broken.insert((m.to_string(), is_row));
                    }
                }
            }
            if !changed {
                break;
            }
        }
        for (node, is_row) in broken {
            let axis = if is_row { Axis::Row } else { Axis::Col };
            ops.push(DiagramEditOp::BreakAlignment { node, axis });
        }
        for (id, position) in &moved {
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
        // Evaluate an attachment change as a complete route/caption candidate.
        // A side-pair estimate alone must not cause a costly visual transition.
        let fresh_layout = candidate.layout.clone();
        let attachment_changed = candidate.layout.ports.iter().any(|(key, p)| {
            self.prepared
                .layout
                .ports
                .get(key)
                .is_some_and(|old| old.side != p.side)
        });
        if attachment_changed {
            if let Ok(labels) = crate::labels::place_with_prior(
                &candidate.scene,
                &self.options,
                &candidate.layout,
                Some(&self.prepared.layout),
            ) {
                candidate.layout.labels = labels;
            }
            for mask in [3, 1, 2] {
                let mut locked = candidate.scene.clone();
                let mut changed = false;
                for edge in &mut locked.edges {
                    for source in [true, false] {
                        let key = port_key(edge, source);
                        let ep = if source {
                            &mut edge.source
                        } else {
                            &mut edge.target
                        };
                        if ep.constraint.side().is_none()
                            && let Some(old) = self.prepared.layout.ports.get(&key)
                        {
                            let side = if mask & if source { 1 } else { 2 } != 0 {
                                old.side
                            } else {
                                fresh_layout.ports[&key].side
                            };
                            changed |= side != fresh_layout.ports[&key].side;
                            ep.constraint = PortConstraint::FixedSide { side };
                        }
                    }
                }
                if !changed {
                    continue;
                }
                if let Ok(mut alternative) =
                    crate::layout::layout(&locked, &self.options, Some(&self.prepared.layout))
                    && let Ok(labels) = crate::labels::place_with_prior(
                        &candidate.scene,
                        &self.options,
                        &alternative,
                        Some(&self.prepared.layout),
                    )
                {
                    alternative.labels = labels;
                    let length = |l: &Layout| {
                        l.routes
                            .values()
                            .flat_map(|r| r.points.windows(2))
                            .map(|p| p[0].distance(p[1]))
                            .sum::<f64>()
                    };
                    let cost = |l: &Layout| {
                        length(l)
                            + l.routes
                                .values()
                                .map(|r| {
                                    crate::routing::coalesced(&r.points).len().saturating_sub(2)
                                        as f64
                                        * self.options.weights.bend
                                })
                                .sum::<f64>()
                            + l.labels
                                .iter()
                                .filter_map(|label| {
                                    self.prepared
                                        .layout
                                        .labels
                                        .iter()
                                        .find(|old| old.feature == label.feature)
                                        .map(|old| {
                                            8. * old.bounds.center().distance(label.bounds.center())
                                        })
                                })
                                .sum::<f64>()
                            + l.ports
                                .iter()
                                .filter(|(key, p)| {
                                    self.prepared
                                        .layout
                                        .ports
                                        .get(*key)
                                        .is_some_and(|old| old.side != p.side)
                                })
                                .count() as f64
                                * 8.
                                * self.options.grid
                    };
                    if length(&alternative) <= length(&fresh_layout) * 1.25 + 64.
                        && cost(&alternative) <= cost(&candidate.layout)
                        && crate::layout::validate(&candidate.scene, &alternative, &self.options)
                            .is_ok()
                        && crate::labels::placements_clear(&candidate.scene, &alternative)
                    {
                        candidate.layout = alternative;
                    }
                }
            }
        }
        // Prefer the immutable gesture baseline for stationary connections.
        // Moving obstacles must still invalidate an old path. Stateful detour
        // retention below has separate validity and excess-length bounds.
        let mut stable = candidate.layout.clone();
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
            }
        }
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
            }
        }
        // Caption continuity is part of the complete candidate, including
        // straight routes and simple elbows, not only retained detours.
        if let Ok(labels) = crate::labels::place_with_prior(
            &candidate.scene,
            &self.options,
            &stable,
            Some(&self.prepared.layout),
        ) {
            stable.labels = labels;
            if crate::layout::validate(&candidate.scene, &stable, &self.options).is_ok()
                && crate::labels::placements_clear(&candidate.scene, &stable)
            {
                candidate.layout = stable;
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
    /// Reposition an edge caption without changing node or route geometry.
    pub fn update_label(&mut self, feature: &str, position: Point) -> Result<(), Error> {
        self.valid = false;
        let result = (|| {
            let old = self
                .baseline
                .layout
                .labels
                .iter()
                .find(|l| l.feature == feature)
                .ok_or_else(|| Error::Edit(format!("unknown label {feature}")))?;
            if !position.x.is_finite() || !position.y.is_finite() {
                return Err(Error::Edit("label coordinates must be finite".into()));
            }
            if old.bounds.center().distance(position) < 1e-7 {
                return Ok(self.baseline.clone());
            }
            let source = edit::apply_ops(
                &self.anchors,
                &[DiagramEditOp::MoveLabel {
                    edge: feature.into(),
                    position,
                }],
                Placement::Layout,
            )?
            .source;
            let mut next = (*self.baseline).clone();
            next.parse = crate::syntax::parse(&source);
            next.scene = crate::model::lower(&next.parse);
            next.layout.labels = crate::labels::place_with_prior(
                &next.scene,
                &self.options,
                &next.layout,
                Some(&self.prepared.layout),
            )?;
            crate::layout::validate(&next.scene, &next.layout, &self.options)?;
            if !crate::labels::placements_clear(&next.scene, &next.layout) {
                return Err(Error::Layout("label placement is obstructed".into()));
            }
            Ok(Arc::new(next))
        })();
        if let Ok(next) = result {
            self.last.clear();
            self.prepared = next;
            self.valid = true;
            Ok(())
        } else {
            result.map(|_| ())
        }
    }
    /// Restore automatic caption placement while preserving node/route geometry.
    pub fn reset_label(&mut self, feature: &str) -> Result<(), Error> {
        self.valid = false;
        let result = (|| {
            let source = edit::apply_ops(
                &self.baseline.parse.source,
                &[DiagramEditOp::ResetLabel {
                    edge: feature.into(),
                }],
                Placement::Layout,
            )?
            .source;
            let mut next = (*self.baseline).clone();
            next.parse = crate::syntax::parse(&source);
            next.scene = crate::model::lower(&next.parse);
            let mut prior = self.baseline.layout.clone();
            prior.labels.retain(|l| l.feature != feature);
            next.layout.labels = crate::labels::place_with_prior(
                &next.scene,
                &self.options,
                &next.layout,
                Some(&prior),
            )?;
            crate::layout::validate(&next.scene, &next.layout, &self.options)?;
            if !crate::labels::placements_clear(&next.scene, &next.layout) {
                return Err(Error::Layout(
                    "automatic label placement is obstructed".into(),
                ));
            }
            Ok(Arc::new(next))
        })();
        if let Ok(next) = result {
            self.last.clear();
            self.prepared = next;
            self.valid = true;
            Ok(())
        } else {
            result.map(|_| ())
        }
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
