//! Canonical view projection, causal focus, composable lenses and continuous zoom.
use crate::{
    Error,
    layout::{Layout, LayoutOptions, Point, Rect},
    model::*,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
pub fn project(scene: &SceneGraph, name: &str) -> Result<SceneGraph, Error> {
    let view = scene
        .views
        .iter()
        .find(|v| v.id == name)
        .ok_or_else(|| Error::Layout(format!("unknown view {name}")))?;
    let mut keep = BTreeSet::new();
    for pattern in &view.include {
        let mut last = Vec::new();
        for part in pattern.split("->").map(str::trim) {
            if part == "*" {
                let mut frontier: VecDeque<_> = last.iter().cloned().collect();
                while let Some(id) = frontier.pop_front() {
                    for e in scene.edges.iter().filter(|e| e.source.node == id) {
                        if keep.insert(e.target.node.clone()) {
                            frontier.push_back(e.target.node.clone());
                        }
                    }
                }
                continue;
            }
            let id = part.trim_matches(['[', ']', ',', ' ']);
            if scene.node(id).is_none() {
                return Err(Error::Layout(format!("unknown view member {id}")));
            }
            keep.insert(id.into());
            last = vec![id.to_string()];
        }
    }
    let explicit = keep.clone();
    for id in explicit {
        keep.extend(ancestors(scene, &id));
    }
    Ok(filter(scene, &keep))
}
pub fn filter(scene: &SceneGraph, keep: &BTreeSet<String>) -> SceneGraph {
    let mut out = scene.clone();
    out.nodes.retain(|n| keep.contains(&n.id));
    out.edges
        .retain(|e| keep.contains(&e.source.node) && keep.contains(&e.target.node));
    for z in &mut out.zones {
        z.members.retain(|id| keep.contains(id));
    }
    out.zones.retain(|z| !z.members.is_empty());
    for r in &mut out.relations {
        r.members.retain(|id| keep.contains(id));
    }
    out.relations.retain(|r| r.members.len() >= 2);
    for f in &mut out.flows {
        f.steps.retain(|id| keep.contains(id));
        f.edges.retain(|id| out.edges.iter().any(|e| e.id == *id));
    }
    out.layout_rules
        .retain(|r| keep.contains(&r.target) || out.edges.iter().any(|e| e.id == r.target));
    out.story.retain(|id| keep.contains(id));
    out.reindex();
    out
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Direction {
    Forward,
    Backward,
    Both,
}
pub fn causal_cone(
    scene: &SceneGraph,
    focus: &str,
    hops: usize,
    direction: Direction,
) -> BTreeSet<NodeId> {
    let mut visited = BTreeSet::from([focus.into()]);
    let mut queue = VecDeque::from([(focus.to_string(), 0)]);
    while let Some((id, k)) = queue.pop_front() {
        if k >= hops {
            continue;
        }
        for e in &scene.edges {
            let next = if e.source.node == id
                && matches!(direction, Direction::Forward | Direction::Both)
            {
                Some(&e.target.node)
            } else if e.target.node == id
                && matches!(direction, Direction::Backward | Direction::Both)
            {
                Some(&e.source.node)
            } else {
                None
            };
            if let Some(n) = next
                && visited.insert(n.clone())
            {
                queue.push_back((n.clone(), k + 1));
            }
        }
    }
    visited
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LensKind {
    Members(BTreeSet<NodeId>),
    SqlSchema,
    IamThreat,
    ByteLayout,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lens {
    pub center: Point,
    pub radius: f64,
    pub strength: f64,
    pub kind: LensKind,
}
impl Lens {
    pub fn members(&self, scene: &SceneGraph, layout: &Layout) -> BTreeSet<NodeId> {
        scene
            .nodes
            .iter()
            .filter(|n| {
                let semantic = match &self.kind {
                    LensKind::Members(ids) => ids.contains(&n.id),
                    LensKind::SqlSchema => n.shape == "sql_table" || n.shape == "database",
                    LensKind::IamThreat => {
                        n.id.contains("auth")
                            || n.id.contains("kms")
                            || n.attrs.contains_key("security")
                    }
                    LensKind::ByteLayout => n.attrs.contains_key("bytes") || n.shape == "memory",
                };
                semantic
                    && layout.nodes.get(&n.id).is_some_and(|r| {
                        (r.x - self.center.x).hypot(r.y - self.center.y) <= self.radius
                    })
            })
            .map(|n| n.id.clone())
            .collect()
    }
}
pub fn lens_intersection(scene: &SceneGraph, l: &Layout, lenses: &[Lens]) -> BTreeSet<NodeId> {
    let mut sets = lenses.iter().map(|lens| lens.members(scene, l));
    let Some(mut out) = sets.next() else {
        return BTreeSet::new();
    };
    for set in sets {
        out.retain(|id| set.contains(id));
    }
    out
}
pub fn fisheye(
    scene: &SceneGraph,
    l: &Layout,
    lenses: &[Lens],
    options: &LayoutOptions,
) -> Result<Layout, Error> {
    if lenses.iter().any(|lens| {
        !lens.radius.is_finite()
            || lens.radius <= 0.
            || !lens.strength.is_finite()
            || lens.strength < 0.
    }) {
        return Err(Error::Layout("invalid lens parameters".into()));
    }
    let mut prior = l.clone();
    for (n, r) in &mut prior.nodes {
        if scene
            .node(n)
            .is_some_and(|n| n.annotations.iter().any(|a| a.name == "pin"))
        {
            continue;
        }
        for lens in lenses {
            let dx = r.x - lens.center.x;
            let dy = r.y - lens.center.y;
            let d = dx.hypot(dy);
            if d > 0. && d < lens.radius {
                let scale = 1. + lens.strength * (1. - d / lens.radius).powi(2);
                r.x = lens.center.x + dx * scale;
                r.y = lens.center.y + dy * scale;
            }
        }
    }
    prior.cache_key.clear();
    prior.cache_config.clear();
    prior.entity_keys.clear();
    crate::layout::layout(scene, options, Some(&prior))
}
/// Cubic smoothstep has zero first derivative at either end, giving C1-continuous transitions.
pub fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoomFrame {
    pub nodes: BTreeMap<NodeId, Rect>,
    pub opacity: BTreeMap<NodeId, f64>,
}
pub fn zoom(scene: &SceneGraph, l: &Layout, depth: f64) -> ZoomFrame {
    let mut out = ZoomFrame {
        nodes: BTreeMap::new(),
        opacity: BTreeMap::new(),
    };
    for n in &scene.nodes {
        let ancestors = ancestors(scene, &n.id);
        let level = ancestors.len() as f64;
        let t = smoothstep(depth - level + 1.);
        let mut r = l.nodes[&n.id];
        if let Some(parent) = n.parent.as_ref().and_then(|p| l.nodes.get(p)) {
            r.x = parent.x + (r.x - parent.x) * t;
            r.y = parent.y + (r.y - parent.y) * t;
        }
        r.w *= t;
        r.h *= t;
        out.nodes.insert(n.id.clone(), r);
        out.opacity.insert(n.id.clone(), t);
    }
    out
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tone {
    pub lightness: f64,
    pub chroma: f64,
    pub hue: f64,
    pub opacity: f64,
}
pub fn attention(scene: &SceneGraph, focus: Option<&str>, hops: usize) -> BTreeMap<NodeId, Tone> {
    let active = focus.map(|id| causal_cone(scene, id, hops, Direction::Both));
    scene
        .nodes
        .iter()
        .map(|n| {
            let quiet = active.as_ref().is_some_and(|a| !a.contains(&n.id));
            let story = scene.story.iter().position(|id| *id == n.id);
            (
                n.id.clone(),
                Tone {
                    lightness: if quiet { 0.85 } else { 0.65 },
                    chroma: if quiet { 0.01 } else { 0.16 },
                    hue: story.map(|i| 240. - i as f64 * 12.).unwrap_or(250.),
                    opacity: if quiet { 0.35 } else { 1. },
                },
            )
        })
        .collect()
}
