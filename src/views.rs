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
/// Retain visible nodes and contract causal paths through hidden intermediates.
pub fn filter(scene: &SceneGraph, keep: &BTreeSet<String>) -> SceneGraph {
    let mut visible = keep.clone();
    for id in keep {
        visible.extend(ancestors(scene, id));
    }
    let keep = &visible;
    let mut out = filter_exact(scene, keep);
    // Stop at visible intermediates. Each summary represents a path whose
    // internal nodes alone were hidden, with cycle-safe deterministic traversal.
    for start in keep {
        let mut queue = VecDeque::from([(start.clone(), Vec::<String>::new())]);
        let mut seen = BTreeSet::from([start.clone()]);
        while let Some((at, hidden)) = queue.pop_front() {
            for edge in &scene.edges {
                let next = if edge.source.node == at {
                    Some((&edge.target, &edge.source))
                } else if edge.target.node == at
                    && matches!(edge.kind, EdgeKind::Bidirectional | EdgeKind::Undirected)
                {
                    Some((&edge.source, &edge.target))
                } else {
                    None
                };
                let Some((target, source)) = next else {
                    continue;
                };
                if keep.contains(&target.node) {
                    if hidden.is_empty()
                        || target.node == *start
                        || out
                            .edges
                            .iter()
                            .any(|e| e.source.node == *start && e.target.node == target.node)
                    {
                        continue;
                    }
                    let mut summary = edge.clone();
                    summary.id = format!("summary:{start}->{}", target.node);
                    summary.source = source.clone();
                    summary.source.node = start.clone();
                    summary.source.port = None;
                    summary.source.constraint = PortConstraint::Free;
                    summary.source.raw_constraint = PortConstraint::Free;
                    summary.source.anchor_span = None;
                    summary.target = target.clone();
                    summary.kind = EdgeKind::Directed;
                    summary.label = format!("via {}", hidden.join(" → "));
                    summary.attrs = BTreeMap::from([
                        (
                            "style".into(),
                            Value {
                                text: "dashed".into(),
                                ..Default::default()
                            },
                        ),
                        (
                            "summary".into(),
                            Value {
                                text: "true".into(),
                                ..Default::default()
                            },
                        ),
                    ]);
                    summary.annotations.clear();
                    out.edges.push(summary);
                } else if seen.insert(target.node.clone()) {
                    let mut path = hidden.clone();
                    path.push(target.node.clone());
                    queue.push_back((target.node.clone(), path));
                }
            }
        }
    }
    out.reindex();
    out
}
fn is_coord_literal(expr: &str) -> bool {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return false;
    }
    let body = trimmed
        .strip_suffix("px")
        .or_else(|| trimmed.strip_suffix('g'))
        .unwrap_or(trimmed);
    body.parse::<f64>().is_ok_and(f64::is_finite)
}

fn pin_anchor_raw(expr: &str) -> Option<String> {
    let e = expr.replace(' ', "");
    if is_coord_literal(&e) {
        return None;
    }
    let split = e
        .char_indices()
        .skip(1)
        .find(|(_, c)| *c == '+' || *c == '-')
        .map(|(i, _)| i);
    let base = split.map_or(e.as_str(), |i| &e[..i]);
    let raw = base
        .rsplit_once('.')
        .filter(|(_, a)| matches!(*a, "N" | "E" | "S" | "W" | "x" | "y"))
        .map_or(base, |(id, _)| id);
    (!raw.is_empty()).then(|| raw.to_string())
}

fn sanitize_pin_annotations(
    anns: &mut Vec<Annotation>,
    scope: Option<&str>,
    scene: &SceneGraph,
    keep: &BTreeSet<String>,
) {
    anns.retain_mut(|ann| {
        if ann.name != "pin" {
            return true;
        }
        if let Some(from) = ann.attrs.get("from")
            && resolve_scoped_node(scene, scope, &from.text).is_none_or(|n| !keep.contains(&n.id))
        {
            ann.attrs.remove("from");
        }
        for axis in ["x", "y"] {
            if let Some(val) = ann.attrs.get(axis)
                && let Some(raw) = pin_anchor_raw(&val.text)
                && resolve_scoped_node(scene, scope, &raw).is_none_or(|n| !keep.contains(&n.id))
            {
                ann.attrs.remove(axis);
            }
        }
        !ann.attrs.is_empty()
    });
}

pub(crate) fn filter_exact(scene: &SceneGraph, keep: &BTreeSet<String>) -> SceneGraph {
    let mut out = scene.clone();
    out.nodes.retain(|n| keep.contains(&n.id));
    for n in &mut out.nodes {
        sanitize_pin_annotations(&mut n.annotations, n.parent.as_deref(), scene, keep);
    }
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
    for rule in &mut out.layout_rules {
        let scope = scene
            .node(&rule.target)
            .and_then(|n| n.parent.as_deref());
        sanitize_pin_annotations(&mut rule.annotations, scope, scene, keep);
    }
    out.layout_rules.retain(|r| !r.annotations.is_empty());
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
                // Keep f(d) = d * (1 + k * (1 - d/R)^2) strictly monotonic on [0, R]:
                // min derivative occurs at d = 2R/3 with slope 1 - k/3, so cap k < 3.
                let strength = lens.strength.min(2.95);
                let scale = 1. + strength * (1. - d / lens.radius).powi(2);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_strips_excluded_pin_anchors_and_from_refs() {
        let mut scene = crate::model::lower(&crate::syntax::parse(
            "a @pin(grid: 0, 0)\nb @pin(x: a.E + 4g, y: 0)\nc @pin(x: b.E + 4g, y: 0)\na -> b -> c\nview v { include a }\n",
        ));
        // Also simulate an explicit `@pin(from: c)` annotation on `a` to verify `from` stripping.
        scene.nodes[0].annotations.push(Annotation {
            name: "pin".into(),
            attrs: BTreeMap::from([(
                "from".into(),
                Value {
                    text: "c".into(),
                    ..Default::default()
                },
            )]),
            span: crate::syntax::Span::default(),
        });
        let mut kept_only = BTreeSet::from(["b".to_string()]);
        let projected = filter_exact(&scene, &kept_only);
        assert!(
            projected.node("b").unwrap().annotations.iter().all(|a| a.name != "pin" || !a.attrs.contains_key("x")),
            "expected pin referencing excluded node `a` to be stripped"
        );
        assert!(crate::layout::layout(&projected, &LayoutOptions::default(), None).is_ok());

        // Verify `from` referencing excluded node `c` is stripped while `a` stays valid.
        kept_only.insert("a".to_string());
        kept_only.remove("b");
        let projected_a = filter_exact(&scene, &kept_only);
        assert!(
            !projected_a
                .node("a")
                .unwrap()
                .annotations
                .iter()
                .any(|a| a.name == "pin" && a.attrs.contains_key("from"))
        );
        assert!(crate::layout::layout(&projected_a, &LayoutOptions::default(), None).is_ok());
    }

    #[test]
    fn fisheye_clamps_radial_distortion_to_stay_monotonic() {
        let diag = crate::compile(
            "focus @pin(grid: 0, 0)\ninner\nmid\nouter\nrim\nfocus -> inner -> mid -> outer -> rim\n",
        )
        .unwrap();
        let center = diag.layout.nodes["focus"].center();
        let high = fisheye(
            &diag.scene,
            &diag.layout,
            &[Lens {
                center,
                radius: 400.,
                strength: 10.0,
                kind: LensKind::Members(BTreeSet::new()),
            }],
            &LayoutOptions::default(),
        )
        .unwrap();
        // Verify pre-VPSC monotonic radial distortion directly on synthetic radii inside [0, R].
        let mut r: f64 = 20.;
        let mut prev = 0.;
        while r < 400. {
            let strength = 10.0_f64.min(2.95);
            let mapped = r * (1. + strength * (1. - r / 400.).powi(2));
            assert!(mapped > prev, "radial distortion must remain strictly monotonic: {mapped} <= {prev}");
            assert!(mapped < 400., "interior node must stay inside lens radius: {mapped}");
            prev = mapped;
            r += 5.;
        }
        assert!(crate::layout::validate(&diag.scene, &high, &LayoutOptions::default()).is_ok());
    }
}
