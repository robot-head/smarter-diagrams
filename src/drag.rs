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
        // A fresh route pass has no stretched/stale paths to accumulate. This is
        // also the result obtained by reopening the saved diagram source.
        let candidate = compile_with(&source, &self.options, None)?;
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
