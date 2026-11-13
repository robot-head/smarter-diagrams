//! Deterministic staged layout, compound geometry, port assignment and mental-map metrics.
use crate::{
    Error,
    model::*,
    solver::{self, Constraint, Weights},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const GRID: f64 = 16.;
pub(crate) fn header_height(node: &Node) -> f64 {
    24. + (node.label.split('\n').count() - 1) as f64 * 20.
}
pub(crate) fn container_header_rect(node: &Node, r: Rect) -> Option<Rect> {
    if !node.container || node.shape == "sql_table" {
        return None;
    }
    let width = (crate::labels::text_width(&node.label) * 16. / 15. + 8.).min((r.w - 24.).max(0.));
    let h = header_height(node);
    Some(Rect::new(
        r.left() + 12. + width / 2.,
        r.top() + 19. + (h - 24.) / 2.,
        width,
        28. + h - 24.,
    ))
}
pub(crate) fn sql_table_title_rect(node: &Node, r: Rect) -> Option<Rect> {
    if node.shape != "sql_table" {
        return None;
    }
    let title = crate::labels::node_title(node);
    let w = crate::labels::text_width(&title) * 16. / 15. + 16.;
    let h = crate::labels::text_height(&title);
    Some(Rect::new(r.x, r.top() - h / 2. - 6., w.min(r.w), h))
}
pub(crate) fn top_extra(node: &Node) -> f64 {
    if node.shape == "sql_table" {
        crate::labels::text_height(&crate::labels::node_title(node)) + 12.
    } else {
        0.
    }
}
pub(crate) fn with_top_extra(node: &Node, r: Rect) -> Rect {
    let extra = top_extra(node);
    Rect::new(r.x, r.y - extra / 2., r.w, r.h + extra)
}
pub(crate) fn title_width(node: &Node) -> f64 {
    crate::labels::text_width(&crate::labels::node_title(node)) * 16. / 15.
        + 32.
        + crate::shapes::title_gutter(&node.shape)
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    /// Euclidean distance in drawing units.
    pub fn distance(self, b: Self) -> f64 {
        (self.x - b.x).hypot(self.y - b.y)
    }
    /// L1 distance, used by the orthogonal router's admissible heuristic.
    pub fn manhattan_distance(self, b: Self) -> f64 {
        (self.x - b.x).abs() + (self.y - b.y).abs()
    }
    pub fn offset(self, x: f64, y: f64) -> Self {
        Self::new(self.x + x, self.y + y)
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}
impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }
    pub fn left(self) -> f64 {
        self.x - self.w / 2.
    }
    pub fn right(self) -> f64 {
        self.x + self.w / 2.
    }
    pub fn top(self) -> f64 {
        self.y - self.h / 2.
    }
    pub fn bottom(self) -> f64 {
        self.y + self.h / 2.
    }
    pub fn center(self) -> Point {
        Point::new(self.x, self.y)
    }
    pub fn inflate(self, p: f64) -> Self {
        Self {
            w: self.w + 2. * p,
            h: self.h + 2. * p,
            ..self
        }
    }
    pub fn intersects(self, b: Self) -> bool {
        self.left() < b.right() - 1e-6
            && b.left() < self.right() - 1e-6
            && self.top() < b.bottom() - 1e-6
            && b.top() < self.bottom() - 1e-6
    }
    pub fn contains(self, p: Point) -> bool {
        p.x > self.left() + 1e-6
            && p.x < self.right() - 1e-6
            && p.y > self.top() + 1e-6
            && p.y < self.bottom() - 1e-6
    }
    pub fn encloses(self, b: Self) -> bool {
        self.left() <= b.left() + 1e-6
            && self.right() >= b.right() - 1e-6
            && self.top() <= b.top() + 1e-6
            && self.bottom() >= b.bottom() - 1e-6
    }
    pub fn union(self, b: Self) -> Self {
        let l = self.left().min(b.left());
        let r = self.right().max(b.right());
        let t = self.top().min(b.top());
        let d = self.bottom().max(b.bottom());
        Self::new((l + r) / 2., (t + d) / 2., r - l, d - t)
    }
    pub fn blocks(self, a: Point, b: Point) -> bool {
        if (a.y - b.y).abs() < 1e-6 {
            a.y > self.top() + 1e-6
                && a.y < self.bottom() - 1e-6
                && a.x.min(b.x) < self.right() - 1e-6
                && a.x.max(b.x) > self.left() + 1e-6
        } else if (a.x - b.x).abs() < 1e-6 {
            a.x > self.left() + 1e-6
                && a.x < self.right() - 1e-6
                && a.y.min(b.y) < self.bottom() - 1e-6
                && a.y.max(b.y) > self.top() + 1e-6
        } else {
            let mut lo: f64 = 0.;
            let mut hi: f64 = 1.;
            for (start, delta, min, max) in [
                (a.x, b.x - a.x, self.left() + 1e-6, self.right() - 1e-6),
                (a.y, b.y - a.y, self.top() + 1e-6, self.bottom() - 1e-6),
            ] {
                if min >= max {
                    return false;
                }
                let u = (min - start) / delta;
                let v = (max - start) / delta;
                lo = lo.max(u.min(v));
                hi = hi.min(u.max(v));
            }
            lo <= hi
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutOptions {
    pub grid: f64,
    pub margin: f64,
    pub stub: f64,
    pub port_spacing: f64,
    pub corner: f64,
    pub lane_spacing: f64,
    pub container_padding: f64,
    pub weights: Weights,
    pub variant: Option<String>,
    pub polish_steps: usize,
    pub shape_budget: usize,
    pub max_layer_width: usize,
    pub target_aspect_ratio: f64,
}
impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            grid: GRID,
            margin: 8.,
            stub: 16.,
            port_spacing: 16.,
            corner: 12.,
            lane_spacing: 8.,
            container_padding: 32.,
            weights: Weights::default(),
            variant: None,
            polish_steps: 20,
            shape_budget: 2048,
            max_layer_width: 0,
            target_aspect_ratio: 1.6,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortPlacement {
    pub point: Point,
    pub stub: Point,
    pub side: Side,
    pub key: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Route {
    pub edge: EdgeId,
    pub points: Vec<Point>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StabilityMetrics {
    pub translation: Point,
    pub squared_drift: f64,
    pub order_flips: usize,
    pub proximity_distortion: f64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Layout {
    pub nodes: BTreeMap<NodeId, Rect>,
    pub ports: BTreeMap<String, PortPlacement>,
    pub routes: BTreeMap<EdgeId, Route>,
    pub labels: Vec<crate::labels::LabelPlacement>,
    pub zones: BTreeMap<String, Vec<Point>>,
    pub stability: StabilityMetrics,
    pub diagnostics: Vec<String>,
    pub reused_routes: usize,
    pub solved_nodes: usize,
    pub shape_search_states: usize,
    #[serde(skip)]
    pub cache_key: String,
    #[serde(skip)]
    pub entity_keys: BTreeMap<String, String>,
    #[serde(skip)]
    pub cache_config: String,
}
#[derive(Clone)]
struct PortRequest {
    key: String,
    node: usize,
    side: Side,
    spec: PortConstraint,
    target: f64,
}
pub fn layout(
    scene: &SceneGraph,
    options: &LayoutOptions,
    previous: Option<&Layout>,
) -> Result<Layout, Error> {
    // Replacing an entire source has no mental map to preserve. Treat it as a
    // fresh layout so topology presentation also works in a running editor.
    let previous =
        previous.filter(|prior| scene.nodes.iter().any(|n| prior.nodes.contains_key(&n.id)));
    layout_pass(scene, options, previous, 1.)
}
fn layout_pass(
    scene: &SceneGraph,
    options: &LayoutOptions,
    previous: Option<&Layout>,
    spacing: f64,
) -> Result<Layout, Error> {
    let o = options;
    if !o.grid.is_finite()
        || o.grid <= 0.
        || [
            o.margin,
            o.stub,
            o.port_spacing,
            o.corner,
            o.lane_spacing,
            o.container_padding,
        ]
        .iter()
        .any(|x| !x.is_finite() || *x <= 0.)
        || !o.target_aspect_ratio.is_finite()
        || o.target_aspect_ratio <= 0.
        || o.stub < o.margin
    {
        return Err(Error::Layout(
            "layout distances must be positive and stub >= margin".into(),
        ));
    }
    validate_model(scene, o)?;
    // Build each semantic entity key once. Previously the global key serialized
    // all entities again, making a small container edit pay twice for the graph.
    let (entity_keys, cache_config) = query_keys(scene, options);
    let cache_key = "entity-geometry-v1".to_string();
    if let Some(prior) = previous.filter(|p| {
        p.cache_key == cache_key && p.cache_config == cache_config && p.entity_keys == entity_keys
    }) {
        let mut cached = prior.clone();
        cached.reused_routes = cached.routes.len();
        cached.solved_nodes = 0;
        cached.stability = StabilityMetrics::default();
        for label in &mut cached.labels {
            if let Some(edge) = scene.edge(&label.feature) {
                label.text = crate::labels::connection_text(scene, edge);
            } else if let Some(node) = scene.node(&label.feature) {
                label.text = crate::labels::node_title(node);
            } else if let Some((node, port)) = label.feature.rsplit_once('#')
                && let Some(p) = scene.node(node).and_then(|n| n.ports.get(port))
            {
                label.text = p.label.clone();
            }
        }
        if validate(scene, &cached, o).is_ok() && crate::labels::placements_clear(scene, &cached) {
            return Ok(cached);
        }
    }
    if let Some(prior) = previous.filter(|p| p.cache_config == cache_config)
        && let Some(mut local) = local_query(scene, options, prior, &entity_keys)
    {
        local.cache_key = cache_key;
        local.cache_config = cache_config;
        local.entity_keys = entity_keys;
        return Ok(local);
    }
    let n = scene.nodes.len();
    if n == 0 {
        return Ok(Layout::default());
    }
    let layer_width = if o.max_layer_width == 0 {
        ((n as f64 * 240. / (112. * o.target_aspect_ratio))
            .sqrt()
            .ceil() as usize)
            .max(2)
    } else {
        o.max_layer_width
    };
    let ranks = crate::topology::coffman_graham(scene, layer_width);
    let mut rows = BTreeMap::new();
    let mut rects = Vec::new();
    for (i, node) in scene.nodes.iter().enumerate() {
        let row = rows.entry(ranks[i]).or_insert(0);
        let mut w = title_width(node).max(112.).max(
            node.ports
                .values()
                .map(|p| crate::labels::text_width(&p.label) + 32.)
                .reduce(f64::max)
                .unwrap_or(0.),
        );
        let mut h = if node.ports.is_empty() {
            48.
        } else {
            (node.ports.len() as f64 + 2.) * o.grid
        };
        let title = crate::labels::node_title(node);
        h = h.max(title.split('\n').count() as f64 * 20. + 28.);
        if node.shape == "sql_table" {
            for port in node.ports.values() {
                if let PortConstraint::FixedPos { offset_grid, .. } = port.constraint {
                    h = h.max(
                        offset_grid * o.grid
                            + 8.
                            + (port.label.split('\n').count() - 1) as f64 * 18.,
                    );
                }
            }
        }
        // Sloping sides must contain the entire text rectangle, not just its
        // center line. Keep room for ascenders/descenders at both title ends.
        if matches!(node.shape.as_str(), "diamond" | "decision") {
            let multiline = title.contains('\n');
            w *= if multiline { 1.8 } else { 1.5 };
            h = h.max(80.).max(crate::labels::text_height(&title) * 3.);
        }
        // Simulation nodes reserve a readable occupancy strip. An explicit
        // size lock below still wins; the viewer only paints status ink that fits.
        let h = if node.shape != "sql_table" && node.annotations.iter().any(|a| a.name == "sim") {
            h.max(title.lines().count() as f64 * 20. + 56.).max(
                if matches!(node.shape.as_str(), "diamond" | "decision") {
                    96.
                } else {
                    80.
                },
            )
        } else {
            h
        };
        let mut r = Rect::new(ranks[i] as f64 * 240., *row as f64 * 112., w, h);
        *row += 1;
        if let Some(p) = previous.and_then(|p| p.nodes.get(&node.id)) {
            r.x = p.x;
            r.y = p.y;
        }
        rects.push(r);
    }
    let requests = port_requests(scene, &rects, o, previous);
    for (i, rect) in rects.iter_mut().enumerate() {
        for side in [Side::N, Side::E, Side::S, Side::W] {
            let ps: Vec<_> = requests
                .iter()
                .filter(|p| p.node == i && p.side == side)
                .collect();
            let mut min = (ps.len().saturating_sub(1)) as f64 * o.port_spacing + 2. * o.corner;
            let mut named_ratios = Vec::new();
            for p in ps {
                match p.spec {
                    PortConstraint::FixedPos { offset_grid, .. } => {
                        min = min.max(offset_grid * o.grid + o.corner)
                    }
                    PortConstraint::FixedRatio { ratio, .. } => {
                        if ratio <= 0. || ratio >= 1. {
                            return Err(Error::Layout(
                                "ratio ports must lie strictly inside a side".into(),
                            ));
                        }
                        min = min.max(o.corner / ratio.min(1. - ratio));
                        if let Some((_, port_id)) = p.key.rsplit_once('#')
                            && let Some(port) = scene.nodes[i].ports.get(port_id)
                            && !port.label.trim().is_empty()
                        {
                            let span = if matches!(side, Side::N | Side::S) {
                                crate::labels::text_width(&port.label) + 20.
                            } else {
                                crate::labels::text_height(&port.label) + 20.
                            };
                            named_ratios.push((ratio, span));
                        }
                    }
                    _ => {}
                }
            }
            if named_ratios.len() >= 3 {
                named_ratios.sort_by(|a, b| a.0.total_cmp(&b.0));
                for pair in named_ratios.windows(2) {
                    let diff = pair[1].0 - pair[0].0;
                    if diff > 1e-6 {
                        min = min.max(pair[0].1.max(pair[1].1) / diff);
                    }
                }
            }
            if matches!(side, Side::N | Side::S) {
                rect.w = rect.w.max(min);
            } else {
                rect.h = rect.h.max(min);
            }
        }
    }
    if rects
        .iter()
        .any(|r| [r.x, r.y, r.w, r.h].iter().any(|v| !v.is_finite()))
    {
        return Err(Error::Layout("non-finite port perimeter dimensions".into()));
    }
    if previous.is_none()
        && o.shape_budget > 0
        && scene.relations.is_empty()
        && scene
            .nodes
            .iter()
            .all(|n| !n.container && n.parent.is_none())
    {
        order_flat_ranks(scene, &ranks, &mut rects, o);
    }
    let shape_stats = if previous.is_none() {
        crate::topology::optimize(scene, &mut rects, o)
    } else {
        Default::default()
    };
    let concept_sides = if previous.is_none() {
        crate::duplex::present(scene, &mut rects, o, true)
            .or_else(|| crate::records::present(scene, &mut rects, o, true))
            .or_else(|| crate::topology::readable_concept(scene, &mut rects, o, true))
    } else {
        None
    };
    if spacing > 1. {
        for r in &mut rects {
            r.x *= spacing;
            r.y *= spacing;
        }
    }
    if previous.is_none() {
        crate::ownership::pack(scene, &mut rects, o, spacing);
    }
    // Size innermost containers before introducing hard containment inequalities.
    for i in (0..n).rev() {
        if scene.nodes[i].container {
            let child_ids: Vec<_> = scene
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, c)| c.parent.as_deref() == Some(&scene.nodes[i].id))
                .map(|(j, _)| j)
                .collect();
            let mut placed: Vec<usize> = Vec::new();
            for &j in &child_ids {
                for &k in &placed {
                    if with_top_extra(&scene.nodes[j], rects[j])
                        .inflate(16.)
                        .intersects(with_top_extra(&scene.nodes[k], rects[k]).inflate(16.))
                    {
                        let delta =
                            rects[k].bottom() + 48. + top_extra(&scene.nodes[j]) - rects[j].top();
                        let id = scene.nodes[j].id.clone();
                        for (t, rect) in rects.iter_mut().enumerate() {
                            if t == j || ancestors(scene, &scene.nodes[t].id).contains(&id) {
                                rect.y += delta;
                            }
                        }
                    }
                }
                placed.push(j);
            }
            let children: Vec<_> = scene
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, c)| c.parent.as_deref() == Some(&scene.nodes[i].id))
                .map(|(j, c)| with_top_extra(c, rects[j]))
                .collect();
            if let Some(bounds) = children.into_iter().reduce(Rect::union) {
                let b = bounds.inflate(o.container_padding);
                let header = header_height(&scene.nodes[i]);
                rects[i] = Rect::new(b.x, b.y - header / 2., b.w, b.h + header);
            }
        }
    }
    let mut pins = vec![(None, None); n];
    let mut relatives = vec![(None, None); n];
    // Dependency evaluation diagnoses unresolved/cyclic relational anchors rather than using stale pixels.
    let annotations: Vec<Vec<Annotation>> = scene
        .nodes
        .iter()
        .map(|node| effective_annotations(scene, &node.id, &node.annotations, o.variant.as_deref()))
        .collect();
    for _ in 0..=n {
        let before = rects.clone();
        for i in 0..n {
            for a in annotations[i].iter().filter(|a| a.name == "pin") {
                if let Some(v) = a.attrs.get("grid") {
                    let pair: Vec<_> = v.text.split(',').filter_map(number).collect();
                    if pair.len() != 2 {
                        return Err(Error::Layout("@pin(grid:) requires two numbers".into()));
                    }
                    pins[i] = (Some(pair[0] * o.grid), Some(pair[1] * o.grid));
                }
                if let Some(v) = a.attrs.get("x") {
                    pins[i].0 = Some(coordinate(
                        &v.text,
                        scene,
                        &rects,
                        true,
                        o.grid,
                        scene.nodes[i].parent.as_deref(),
                    )?);
                    relatives[i].0 = relative_constraint(
                        &v.text,
                        scene,
                        &rects,
                        true,
                        o.grid,
                        scene.nodes[i].parent.as_deref(),
                    )?;
                }
                if let Some(v) = a.attrs.get("y") {
                    pins[i].1 = Some(coordinate(
                        &v.text,
                        scene,
                        &rects,
                        false,
                        o.grid,
                        scene.nodes[i].parent.as_deref(),
                    )?);
                    relatives[i].1 = relative_constraint(
                        &v.text,
                        scene,
                        &rects,
                        false,
                        o.grid,
                        scene.nodes[i].parent.as_deref(),
                    )?;
                }
                if let Some(v) = a.attrs.get("lock") {
                    let parts: Vec<_> = v.text.trim_matches(['[', ']']).split(',').collect();
                    for p in parts {
                        if let Some((key, value)) = p.split_once(':') {
                            let size = number(value)
                                .ok_or_else(|| Error::Layout("invalid size lock".into()))?
                                * o.grid;
                            if size <= 0. {
                                return Err(Error::Layout("size locks must be positive".into()));
                            }
                            match key.trim() {
                                "w" => rects[i].w = size,
                                "h" => rects[i].h = size,
                                _ => {}
                            }
                        }
                    }
                }
            }
            if let Some(x) = pins[i].0 {
                rects[i].x = x;
            }
            if let Some(y) = pins[i].1 {
                rects[i].y = y;
            }
        }
        if before == rects {
            break;
        }
    }
    if previous.is_none()
        && pins.iter().any(|p| p.0.is_some() || p.1.is_some())
        && scene.nodes.iter().any(|nd| nd.container)
    {
        let rel_nodes: BTreeSet<&str> = scene
            .relations
            .iter()
            .flat_map(|r| r.members.iter().map(String::as_str))
            .collect();
        let mut by_col: BTreeMap<(Option<String>, usize), Vec<usize>> = BTreeMap::new();
        for (i, nd) in scene.nodes.iter().enumerate() {
            if !nd.container
                && pins[i].0.is_none()
                && pins[i].1.is_none()
                && !rel_nodes.contains(nd.id.as_str())
            {
                by_col
                    .entry((nd.parent.clone(), ranks[i]))
                    .or_default()
                    .push(i);
            }
        }
        let cols: Vec<Vec<usize>> = by_col
            .into_values()
            .filter(|col| {
                if col.len() < 2 {
                    return false;
                }
                let ids: BTreeSet<&str> = col.iter().map(|&i| scene.nodes[i].id.as_str()).collect();
                !scene.edges.iter().any(|e| {
                    ids.contains(e.source.node.as_str()) && ids.contains(e.target.node.as_str())
                })
            })
            .collect();
        for sweep in 0..4 {
            let iter: Box<dyn Iterator<Item = &Vec<usize>>> = if sweep % 2 == 0 {
                Box::new(cols.iter())
            } else {
                Box::new(cols.iter().rev())
            };
            for col in iter {
                let mut cur = col.clone();
                cur.sort_by(|&a, &b| rects[a].y.total_cmp(&rects[b].y));
                let top = cur
                    .iter()
                    .map(|&i| with_top_extra(&scene.nodes[i], rects[i]).top())
                    .fold(f64::INFINITY, f64::min);
                let bottom = cur
                    .iter()
                    .map(|&i| rects[i].bottom())
                    .fold(f64::NEG_INFINITY, f64::max);
                let body: f64 = cur
                    .iter()
                    .map(|&i| rects[i].h + top_extra(&scene.nodes[i]))
                    .sum();
                let gap = ((bottom - top - body) / (cur.len() - 1) as f64).max(32.);
                let mut keyed: Vec<(f64, usize, usize)> = cur
                    .iter()
                    .enumerate()
                    .map(|(slot, &i)| {
                        let id = &scene.nodes[i].id;
                        let mut sum = 0.;
                        let mut wsum = 0.;
                        for e in &scene.edges {
                            let peer = if &e.source.node == id {
                                scene.index(&e.target.node)
                            } else if &e.target.node == id {
                                scene.index(&e.source.node)
                            } else {
                                None
                            };
                            if let Some(j) = peer {
                                let w = if pins[j].1.is_some() { 2. } else { 1. };
                                sum += w * rects[j].y;
                                wsum += w;
                            }
                        }
                        let bary = if wsum > 0. { sum / wsum } else { rects[i].y };
                        (bary, slot, i)
                    })
                    .collect();
                keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                let mut cursor = top;
                for (_, _, i) in keyed {
                    let extra = top_extra(&scene.nodes[i]);
                    rects[i].y = cursor + extra + rects[i].h / 2.;
                    cursor += extra + rects[i].h + gap;
                }
            }
        }
        for i in (0..n).rev() {
            if scene.nodes[i].container && pins[i].0.is_none() && pins[i].1.is_none() {
                let children: Vec<_> = scene
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.parent.as_deref() == Some(&scene.nodes[i].id))
                    .map(|(j, c)| with_top_extra(c, rects[j]))
                    .collect();
                if let Some(bounds) = children.into_iter().reduce(Rect::union) {
                    let b = bounds.inflate(o.container_padding);
                    let header = header_height(&scene.nodes[i]);
                    rects[i].x = b.x;
                    rects[i].y = b.y - header / 2.;
                    rects[i].w = rects[i].w.max(b.w);
                    rects[i].h = rects[i].h.max(b.h + header);
                }
            }
        }
    }
    let concept_sides = concept_sides
        .or_else(|| crate::duplex::present(scene, &mut rects, o, false))
        .or_else(|| crate::records::present(scene, &mut rects, o, false))
        .or_else(|| crate::scope_feedback::hints(scene, &rects, o))
        .or_else(|| crate::topology::readable_concept(scene, &mut rects, o, false));
    let mut requests = port_requests(scene, &rects, o, previous);
    let readable_routes = concept_sides.is_some();
    let readable_fan =
        readable_routes && scene.nodes.len() > 2 && scene.edges.len() + 1 == scene.nodes.len();
    if let Some(sides) = concept_sides {
        for (edge, hint) in scene.edges.iter().zip(sides) {
            let Some((source_side, target_side)) = hint else {
                continue;
            };
            for (source, side, other) in [
                (true, source_side, &edge.target),
                (false, target_side, &edge.source),
            ] {
                if let Some(request) = requests
                    .iter_mut()
                    .find(|p| p.key == port_key(edge, source))
                {
                    request.side = side;
                    let opposite = rects[scene.index(&other.node).unwrap()];
                    request.target = port_target(
                        opposite,
                        &other.constraint,
                        side,
                        if source { target_side } else { source_side },
                        o.grid,
                    );
                }
            }
        }
    }
    if readable_routes {
        for edge in &scene.edges {
            for source in [true, false] {
                if let Some(request) = requests
                    .iter_mut()
                    .find(|p| p.key == port_key(edge, source))
                    && let Some(target) =
                        crate::bundles::order(scene, &rects, edge, source, request.side)
                {
                    request.target = target;
                }
            }
        }
    }
    let endpoint_sides: BTreeMap<_, _> = requests.iter().map(|p| (p.key.clone(), p.side)).collect();
    for edge in &scene.edges {
        let sa = endpoint_sides[&port_key(edge, true)];
        let sb = endpoint_sides[&port_key(edge, false)];
        for source in [true, false] {
            if let Some(request) = requests
                .iter_mut()
                .find(|p| p.key == port_key(edge, source))
                && let Some(target) = crate::sequence_routes::parallel_order(
                    scene,
                    edge,
                    source,
                    sa,
                    sb,
                    &endpoint_sides,
                    request.target,
                )
            {
                request.target = target;
            }
        }
    }
    grow_port_perimeters(scene, &requests, &mut rects, o)?;
    let mut w_locked = vec![false; n];
    let mut h_locked = vec![false; n];
    for i in 0..n {
        let lock = annotations[i]
            .iter()
            .find(|a| a.name == "pin")
            .and_then(|a| a.attrs.get("lock"))
            .map(|v| v.text.as_str())
            .unwrap_or("");
        w_locked[i] = lock.contains("w:") || lock.contains("w :");
        h_locked[i] = lock.contains("h:") || lock.contains("h :");
    }
    // Absolute child pins can enlarge an auto-sized container; explicit size locks stay hard.
    for i in (0..n).rev() {
        if !scene.nodes[i].container {
            continue;
        }
        let children: Vec<_> = scene
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, c)| c.parent.as_deref() == Some(&scene.nodes[i].id))
            .map(|(j, c)| {
                if h_locked[i] {
                    rects[j]
                } else {
                    with_top_extra(c, rects[j])
                }
            })
            .collect();
        if let Some(bounds) = children.into_iter().reduce(Rect::union) {
            let x = pins[i].0.unwrap_or(bounds.x);
            let y = pins[i]
                .1
                .unwrap_or(bounds.y - header_height(&scene.nodes[i]) / 2.);
            let required_w = 2.
                * (x - bounds.left() + o.container_padding)
                    .max(bounds.right() + o.container_padding - x);
            let required_h = 2.
                * (y - bounds.top() + o.container_padding + header_height(&scene.nodes[i]))
                    .max(bounds.bottom() + o.container_padding - y);
            if !w_locked[i] {
                rects[i].w = rects[i].w.max(required_w);
            }
            if !h_locked[i] {
                rects[i].h = rects[i].h.max(required_h);
            }
            rects[i].x = x;
            rects[i].y = y;
        }
    }
    // Anchor offsets use final container dimensions, then remain linear equalities during projection.
    for i in 0..n {
        for a in annotations[i].iter().filter(|a| a.name == "pin") {
            if let Some(v) = a.attrs.get("x") {
                relatives[i].0 = relative_constraint(
                    &v.text,
                    scene,
                    &rects,
                    true,
                    o.grid,
                    scene.nodes[i].parent.as_deref(),
                )?;
            }
            if let Some(v) = a.attrs.get("y") {
                relatives[i].1 = relative_constraint(
                    &v.text,
                    scene,
                    &rects,
                    false,
                    o.grid,
                    scene.nodes[i].parent.as_deref(),
                )?;
            }
        }
    }
    // Detect pin-reference cycles explicitly.
    check_pin_cycles(scene, &annotations)?;
    let referenced_container: Vec<bool> = (0..n)
        .map(|i| {
            relatives
                .iter()
                .any(|(x, y)| x.is_some_and(|(j, _)| j == i) || y.is_some_and(|(j, _)| j == i))
        })
        .collect();
    let mut desired_x: Vec<_> = rects.iter().map(|r| r.x).collect();
    let mut desired_y: Vec<_> = rects.iter().map(|r| r.y).collect();
    let mut weights = vec![1.; n];
    for i in 0..n {
        let degree = scene
            .edges
            .iter()
            .filter(|e| e.source.node == scene.nodes[i].id || e.target.node == scene.nodes[i].id)
            .count() as f64;
        weights[i] = if previous.is_some_and(|p| p.nodes.contains_key(&scene.nodes[i].id)) {
            degree.max(1.) * 20.
        } else {
            o.weights.source_order.max(0.01)
        };
        if pins[i].0.is_some() || pins[i].1.is_some() {
            weights[i] = o.weights.pin;
        }
    }
    if let Some(prior) = previous {
        for (i, node) in scene.nodes.iter().enumerate() {
            if let Some(old) = prior.nodes.get(&node.id) {
                if pins[i].0.is_none() || relatives[i].0.is_some() {
                    desired_x[i] = old.x;
                }
                if pins[i].1.is_none() || relatives[i].1.is_some() {
                    desired_y[i] = old.y;
                }
            }
        }
    }
    if let Some(prior) = previous {
        preserve_proximity(scene, prior, &rects, &mut desired_x, &mut desired_y, &pins);
    }
    let mut base_cx = Vec::new();
    let mut base_cy = Vec::new();
    for i in 0..n {
        if let Some((j, offset)) = relatives[i].0 {
            base_cx.push(Constraint {
                coefficients: vec![(i, 1.), (j, -1.)],
                rhs: offset,
                equality: true,
            });
        } else if let Some(x) = pins[i].0 {
            base_cx.push(Constraint::fixed(i, x));
        }
        if let Some((j, offset)) = relatives[i].1 {
            base_cy.push(Constraint {
                coefficients: vec![(i, 1.), (j, -1.)],
                rhs: offset,
                equality: true,
            });
        } else if let Some(y) = pins[i].1 {
            base_cy.push(Constraint::fixed(i, y));
        }
    }
    for rel in &scene.relations {
        let ids: Vec<_> = rel
            .members
            .iter()
            .filter_map(|id| scene.index(id))
            .collect();
        if rel.distribute {
            let cs = if rel.axis == Axis::Row {
                &mut base_cx
            } else {
                &mut base_cy
            };
            if let Some(gap) = rel.gap_grid {
                for pair in ids.windows(2) {
                    let (a, b) = (pair[0], pair[1]);
                    let size = if rel.axis == Axis::Row {
                        (rects[a].w + rects[b].w) / 2.
                    } else {
                        (rects[a].h + rects[b].h) / 2.
                    };
                    cs.push(Constraint {
                        coefficients: vec![(a, -1.), (b, 1.)],
                        rhs: size + gap * o.grid,
                        equality: true,
                    });
                }
            } else {
                for p in ids.windows(3) {
                    cs.push(Constraint {
                        coefficients: vec![(p[0], 1.), (p[1], -2.), (p[2], 1.)],
                        rhs: 0.,
                        equality: true,
                    });
                }
            }
        } else {
            let cs = if rel.axis == Axis::Row {
                &mut base_cy
            } else {
                &mut base_cx
            };
            for pair in ids.windows(2) {
                cs.push(Constraint::equality(pair[0], pair[1]));
            }
        }
    }
    let build_cx = |rects: &[Rect], sep: &[Constraint], include_unlocked: bool| {
        let mut out = base_cx.clone();
        for i in 0..n {
            if let Some(p) = scene.nodes[i]
                .parent
                .as_deref()
                .and_then(|p| scene.index(p))
                && (include_unlocked
                    || w_locked[p]
                    || referenced_container[p]
                    || previous.is_some())
            {
                out.push(Constraint::separation(
                    p,
                    i,
                    -rects[p].w / 2. + o.container_padding + rects[i].w / 2.,
                ));
                out.push(Constraint::separation(
                    i,
                    p,
                    -rects[p].w / 2. + o.container_padding + rects[i].w / 2.,
                ));
            }
        }
        out.extend_from_slice(sep);
        out
    };
    let build_cy = |rects: &[Rect], sep: &[Constraint], include_unlocked: bool| {
        let mut out = base_cy.clone();
        for i in 0..n {
            if let Some(p) = scene.nodes[i]
                .parent
                .as_deref()
                .and_then(|p| scene.index(p))
                && (include_unlocked
                    || h_locked[p]
                    || referenced_container[p]
                    || previous.is_some())
            {
                let extra = if h_locked[p] {
                    0.
                } else {
                    top_extra(&scene.nodes[i])
                };
                out.push(Constraint::separation(
                    p,
                    i,
                    -rects[p].h / 2.
                        + o.container_padding
                        + header_height(&scene.nodes[p])
                        + extra
                        + rects[i].h / 2.,
                ));
                out.push(Constraint::separation(
                    i,
                    p,
                    -rects[p].h / 2. + o.container_padding + rects[i].h / 2.,
                ));
            }
        }
        out.extend_from_slice(sep);
        out
    };
    let mut sep_cx = Vec::new();
    let mut sep_cy = Vec::new();
    let preserve_series = readable_routes && crate::series::recognized(scene);
    // Straighten only opposing single-fanout ports, on their tangential coordinate.
    let sides: BTreeMap<_, _> = requests.iter().cloned().map(|p| (p.key, p.side)).collect();
    for e in &scene.edges {
        if preserve_series {
            break;
        }
        let (a, b) = (
            scene.index(&e.source.node).unwrap(),
            scene.index(&e.target.node).unwrap(),
        );
        let pair_bundle = previous.is_none()
            && !readable_routes
            && a != b
            && matches!(
                e.source.constraint,
                PortConstraint::Free | PortConstraint::FixedSide { .. }
            )
            && matches!(
                e.target.constraint,
                PortConstraint::Free | PortConstraint::FixedSide { .. }
            )
            && scene
                .edges
                .iter()
                .filter(|x| {
                    (x.source.node == e.source.node && x.target.node == e.target.node)
                        || (x.source.node == e.target.node && x.target.node == e.source.node)
                })
                .count()
                >= 2;
        if !pair_bundle
            && (scene
                .edges
                .iter()
                .filter(|x| x.source.node == e.source.node)
                .count()
                != 1
                || scene
                    .edges
                    .iter()
                    .filter(|x| x.target.node == e.target.node)
                    .count()
                    != 1)
        {
            continue;
        }
        let sa = sides[&port_key(e, true)];
        let sb = sides[&port_key(e, false)];
        if matches!((sa, sb), (Side::E, Side::W) | (Side::W, Side::E))
            && pins[a].1.is_none()
            && pins[b].1.is_none()
        {
            let y =
                (desired_y[a] * weights[a] + desired_y[b] * weights[b]) / (weights[a] + weights[b]);
            desired_y[a] = y;
            desired_y[b] = y;
        }
        if matches!((sa, sb), (Side::N, Side::S) | (Side::S, Side::N))
            && pins[a].0.is_none()
            && pins[b].0.is_none()
        {
            let x =
                (desired_x[a] * weights[a] + desired_x[b] * weights[b]) / (weights[a] + weights[b]);
            desired_x[a] = x;
            desired_x[b] = x;
        }
    }
    // Generate disjunctive non-overlap constraints deterministically, preserving source/previous order.
    let mut stub_sides = vec![[false; 4]; n];
    for p in &requests {
        stub_sides[p.node][side_index(p.side)] = true;
    }
    let ancestry: Vec<_> = scene
        .nodes
        .iter()
        .map(|n| ancestors(scene, &n.id))
        .collect();
    for _ in 0..n * 2 + 8 {
        let (xs, ys) = if let Some(prior) = previous {
            let cx = build_cx(&rects, &sep_cx, true);
            let cy = build_cy(&rects, &sep_cy, true);
            let px: Vec<_> = scene
                .nodes
                .iter()
                .map(|n| prior.nodes.get(&n.id).map(|r| r.x))
                .collect();
            let py: Vec<_> = scene
                .nodes
                .iter()
                .map(|n| prior.nodes.get(&n.id).map(|r| r.y))
                .collect();
            (
                solver::project_stable(&desired_x, &weights, &cx, &px, o.weights.flip)?,
                solver::project_stable(&desired_y, &weights, &cy, &py, o.weights.flip)?,
            )
        } else {
            let xs = match solver::project(&desired_x, &weights, &build_cx(&rects, &sep_cx, true)) {
                Ok(xs) => xs,
                Err(_)
                    if (0..n).any(|i| {
                        scene.nodes[i].container && !w_locked[i] && !referenced_container[i]
                    }) =>
                {
                    let free =
                        solver::project(&desired_x, &weights, &build_cx(&rects, &sep_cx, false))?;
                    for i in (0..n).rev() {
                        if !scene.nodes[i].container || w_locked[i] || referenced_container[i] {
                            continue;
                        }
                        if let Some((left, right)) = scene
                            .nodes
                            .iter()
                            .enumerate()
                            .filter(|(_, c)| c.parent.as_deref() == Some(&scene.nodes[i].id))
                            .map(|(j, _)| (free[j] - rects[j].w / 2., free[j] + rects[j].w / 2.))
                            .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
                        {
                            let req = if let Some(px) = pins[i].0 {
                                2. * (px - left + o.container_padding)
                                    .max(right + o.container_padding - px)
                            } else {
                                right - left + 2. * o.container_padding
                            };
                            rects[i].w = rects[i].w.max(req);
                        }
                    }
                    solver::project(&desired_x, &weights, &build_cx(&rects, &sep_cx, true))?
                }
                Err(err) => return Err(err),
            };
            let ys = match solver::project(&desired_y, &weights, &build_cy(&rects, &sep_cy, true)) {
                Ok(ys) => ys,
                Err(_)
                    if (0..n).any(|i| {
                        scene.nodes[i].container && !h_locked[i] && !referenced_container[i]
                    }) =>
                {
                    let free =
                        solver::project(&desired_y, &weights, &build_cy(&rects, &sep_cy, false))?;
                    for i in (0..n).rev() {
                        if !scene.nodes[i].container || h_locked[i] || referenced_container[i] {
                            continue;
                        }
                        if let Some((top, bottom)) = scene
                            .nodes
                            .iter()
                            .enumerate()
                            .filter(|(_, c)| c.parent.as_deref() == Some(&scene.nodes[i].id))
                            .map(|(j, c)| {
                                (
                                    free[j] - rects[j].h / 2. - top_extra(c),
                                    free[j] + rects[j].h / 2.,
                                )
                            })
                            .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
                        {
                            let header = header_height(&scene.nodes[i]);
                            let req = if let Some(py) = pins[i].1 {
                                2. * (py - top + o.container_padding + header)
                                    .max(bottom + o.container_padding - py)
                            } else {
                                bottom - top + 2. * o.container_padding + header
                            };
                            rects[i].h = rects[i].h.max(req);
                        }
                    }
                    solver::project(&desired_y, &weights, &build_cy(&rects, &sep_cy, true))?
                }
                Err(err) => return Err(err),
            };
            (xs, ys)
        };
        for i in 0..n {
            rects[i].x = xs[i];
            rects[i].y = ys[i];
        }
        let mut added = false;
        for a in 0..n {
            for b in a + 1..n {
                if ancestry[a].contains(&scene.nodes[b].id)
                    || ancestry[b].contains(&scene.nodes[a].id)
                {
                    continue;
                }
                // Exact canvas pins constrain bodies, not a rectangular
                // overapproximation of every occupied port side. Actual stubs
                // are checked against silhouettes by the router afterwards.
                let exact = pins[a].0.is_some()
                    && pins[a].1.is_some()
                    && pins[b].0.is_some()
                    && pins[b].1.is_some();
                if exact
                    && rects[a]
                        .inflate(o.margin / 2.)
                        .intersects(rects[b].inflate(o.margin / 2.))
                {
                    return Err(Error::Layout(format!(
                        "{} and {} overlap or leave less than {} px body clearance",
                        scene.nodes[a].id, scene.nodes[b].id, o.margin
                    )));
                }
                let body = |i| {
                    if exact {
                        rects[i]
                    } else {
                        with_top_extra(&scene.nodes[i], rects[i])
                    }
                };
                let envelope = |i| {
                    if exact {
                        rects[i]
                    } else {
                        with_top_extra(
                            &scene.nodes[i],
                            stub_envelope(rects[i], stub_sides[i], o.stub),
                        )
                    }
                };
                let fresh_unpinned =
                    previous.is_none() && pins[a] == (None, None) && pins[b] == (None, None);
                if !envelope(a)
                    .inflate(o.margin / 2.)
                    .intersects(body(b).inflate(o.margin / 2.))
                    && !envelope(b)
                        .inflate(o.margin / 2.)
                        .intersects(body(a).inflate(o.margin / 2.))
                    && !(fresh_unpinned
                        && envelope(a)
                            .inflate(o.margin / 2.)
                            .intersects(envelope(b).inflate(o.margin / 2.)))
                {
                    continue;
                }
                let dx = (rects[a].x - rects[b].x).abs();
                let dy = (rects[a].y - rects[b].y).abs();
                let x_stubs = if exact {
                    0
                } else if rects[a].x <= rects[b].x {
                    usize::from(stub_sides[a][1])
                        + usize::from(fresh_unpinned && stub_sides[b][3])
                        + usize::from(!fresh_unpinned && !stub_sides[a][1] && stub_sides[b][3])
                } else {
                    usize::from(stub_sides[a][3])
                        + usize::from(fresh_unpinned && stub_sides[b][1])
                        + usize::from(!fresh_unpinned && !stub_sides[a][3] && stub_sides[b][1])
                };
                let xgap = (rects[a].w + rects[b].w) / 2. + o.margin + x_stubs as f64 * o.stub;
                let y_stubs = if exact {
                    0
                } else if rects[a].y <= rects[b].y {
                    usize::from(stub_sides[a][2])
                        + usize::from(fresh_unpinned && stub_sides[b][0])
                        + usize::from(!fresh_unpinned && !stub_sides[a][2] && stub_sides[b][0])
                } else {
                    usize::from(stub_sides[a][0])
                        + usize::from(fresh_unpinned && stub_sides[b][2])
                        + usize::from(!fresh_unpinned && !stub_sides[a][0] && stub_sides[b][2])
                };
                let title_extra = if exact {
                    0.
                } else if rects[a].y <= rects[b].y {
                    top_extra(&scene.nodes[b])
                } else {
                    top_extra(&scene.nodes[a])
                };
                let ygap = (rects[a].h + rects[b].h) / 2.
                    + o.margin
                    + title_extra
                    + y_stubs as f64 * o.stub;
                let row_align = scene.relations.iter().any(|r| {
                    !r.distribute
                        && r.axis == Axis::Row
                        && r.members.contains(&scene.nodes[a].id)
                        && r.members.contains(&scene.nodes[b].id)
                });
                let col_align = scene.relations.iter().any(|r| {
                    !r.distribute
                        && r.axis == Axis::Col
                        && r.members.contains(&scene.nodes[a].id)
                        && r.members.contains(&scene.nodes[b].id)
                });
                let use_x = row_align
                    || (!col_align
                        && (xgap - dx < ygap - dy || pins[a].1.is_some() && pins[b].1.is_some()));
                let (va, vb) = if use_x {
                    (rects[a].x, rects[b].x)
                } else {
                    (rects[a].y, rects[b].y)
                };
                let (left, right) = if va <= vb { (a, b) } else { (b, a) };
                let c = Constraint::separation(left, right, if use_x { xgap } else { ygap });
                let cs = if use_x { &mut sep_cx } else { &mut sep_cy };
                if !cs.iter().any(|old| {
                    !old.equality
                        && old.coefficients == c.coefficients
                        && (old.rhs - c.rhs).abs() < 1e-6
                }) {
                    cs.push(c);
                    added = true;
                }
            }
        }
        if !added {
            break;
        }
    }
    let mut out = Layout {
        solved_nodes: n,
        shape_search_states: shape_stats.searched_states,
        ..Default::default()
    };
    // Refit unconstrained fresh containers after their children have settled.
    // Shrinking stays within the solved box; referenced/pinned containers and
    // incremental reserved frames retain their hard geometry.
    if previous.is_none() {
        for i in (0..n).rev() {
            let node = &scene.nodes[i];
            if !node.container
                || !node.ports.is_empty()
                || annotations[i].iter().any(|a| a.name == "pin")
                || scene
                    .edges
                    .iter()
                    .any(|e| e.source.node == node.id || e.target.node == node.id)
                || scene.relations.iter().any(|r| r.members.contains(&node.id))
                || referenced_container[i]
            {
                continue;
            }
            if let Some(bounds) = scene
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, c)| c.parent.as_deref() == Some(&node.id))
                .map(|(j, c)| with_top_extra(c, rects[j]))
                .reduce(Rect::union)
            {
                rects[i] = Rect::new(
                    bounds.x,
                    bounds.y - header_height(node) / 2.,
                    (bounds.w + 2. * o.container_padding).max(title_width(node)),
                    bounds.h + 2. * o.container_padding + header_height(node),
                );
            }
        }
    }
    for (i, node) in scene.nodes.iter().enumerate() {
        out.nodes.insert(node.id.clone(), rects[i]);
    }
    assign_ports(
        scene,
        o,
        &requests,
        &rects,
        &pins,
        previous.is_none() && !readable_fan,
        &mut out,
    )?;
    crate::routing::route_all(scene, o, previous, &mut out)?;
    crate::routing::nudge_channels(scene, o, &mut out);
    let coordinated_fan = readable_fan && crate::routing::coordinate_fan(scene, o, &mut out);
    let coordinated_tree =
        readable_routes && crate::presentation::coordinate_tree(scene, o, &mut out);
    let coordinated_mesh =
        readable_routes && crate::presentation::coordinate_mesh(scene, o, &mut out);
    let coordinated_backbone =
        readable_routes && crate::presentation::coordinate_backbone(scene, o, &mut out);
    let coordinated_series = readable_routes && crate::series::coordinate(scene, o, &mut out);
    let coordinated_bundles = readable_routes && crate::bundles::coordinate(scene, o, &mut out);
    let coordinated_duplex = readable_routes && crate::duplex::coordinate(scene, o, &mut out);
    let coordinated_gather = readable_routes && crate::gather::coordinate(scene, o, &mut out);
    let coordinated_records = readable_routes && crate::records::coordinate(scene, o, &mut out);
    let coordinated_scopes =
        readable_routes && crate::scope_feedback::coordinate(scene, o, &mut out);
    let coordinated_loops =
        readable_routes && crate::routing::coordinate_outer_routes(scene, o, &mut out);
    let has_hard_geometry = annotations
        .iter()
        .any(|list| list.iter().any(|a| a.name == "pin"))
        || scene.edges.iter().any(|e| {
            effective_annotations(scene, &e.id, &e.annotations, o.variant.as_deref())
                .iter()
                .any(|a| a.name == "route")
        })
        || scene.relations.iter().any(|r| r.gap_grid.is_some());
    out.labels = match crate::labels::place(scene, o, &out) {
        Ok(labels) => labels,
        Err(Error::Layout(message)) if message.starts_with("no feasible label position") => {
            if previous.is_none() && spacing < 2.25 && !has_hard_geometry {
                // Caption space is part of fresh layout feasibility. Retry with
                // more whitespace rather than rejecting an unconstrained graph.
                return layout_pass(scene, options, None, spacing * 1.5);
            }
            crate::labels::place_with_fallback(scene, o, &out)?
        }
        Err(error) => return Err(error),
    };
    if !coordinated_fan
        && !coordinated_loops
        && !coordinated_tree
        && !coordinated_mesh
        && !coordinated_backbone
        && !coordinated_series
        && !coordinated_bundles
        && !coordinated_duplex
        && !coordinated_gather
        && !coordinated_records
        && !coordinated_scopes
    {
        crate::labels::polish_channels(scene, o, &mut out);
    }
    for zone in &scene.zones {
        let padding = value(&zone.attrs, "padding").unwrap_or(1.5) * o.grid;
        let points: Vec<_> = zone
            .members
            .iter()
            .filter_map(|id| out.nodes.get(id))
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
        out.zones.insert(zone.id.clone(), convex_hull(points));
    }
    if let Some(prior) = previous {
        out.stability = stability(scene, prior, &out);
    }
    out.cache_key = cache_key;
    out.cache_config = cache_config;
    out.entity_keys = entity_keys;
    validate(scene, &out, o)?;
    Ok(out)
}
pub fn effective_annotations(
    scene: &SceneGraph,
    id: &str,
    inline: &[Annotation],
    variant: Option<&str>,
) -> Vec<Annotation> {
    let mut out = inline.to_vec();
    for rule in &scene.layout_rules {
        if rule.target == id && (rule.variant.is_none() || rule.variant.as_deref() == variant) {
            for ann in &rule.annotations {
                if let Some(i) = out.iter().position(|a| a.name == ann.name) {
                    out[i] = ann.clone();
                } else {
                    out.push(ann.clone());
                }
            }
        }
    }
    out
}
fn coordinate(
    expr: &str,
    scene: &SceneGraph,
    rects: &[Rect],
    x: bool,
    grid: f64,
    scope: Option<&str>,
) -> Result<f64, Error> {
    if let Some(n) = number(expr) {
        return Ok(n * grid);
    }
    let e = expr.replace(' ', "");
    let split = e
        .char_indices()
        .skip(1)
        .find(|(_, c)| *c == '+' || *c == '-')
        .map(|(i, _)| i);
    let (base, delta) = if let Some(i) = split {
        (
            &e[..i],
            number(&e[i..]).ok_or_else(|| Error::Layout(format!("invalid coordinate {expr}")))?
                * grid,
        )
    } else {
        (e.as_str(), 0.)
    };
    let (id, anchor) = base
        .rsplit_once('.')
        .filter(|(_, a)| matches!(*a, "N" | "E" | "S" | "W" | "x" | "y"))
        .unwrap_or((base, if x { "x" } else { "y" }));
    let i = coordinate_index(scene, id, scope)
        .ok_or_else(|| Error::Layout(format!("unknown coordinate anchor {id}")))?;
    let r = rects[i];
    Ok(match anchor {
        "N" => r.top(),
        "E" => r.right(),
        "S" => r.bottom(),
        "W" => r.left(),
        "x" => r.x,
        "y" => r.y,
        _ => unreachable!(),
    } + delta)
}
fn coordinate_index(scene: &SceneGraph, id: &str, scope: Option<&str>) -> Option<usize> {
    let mut at = scope;
    while let Some(prefix) = at {
        if let Some(i) = scene.index(&format!("{prefix}.{id}")) {
            return Some(i);
        }
        at = prefix.rsplit_once('.').map(|(parent, _)| parent);
    }
    scene.index(id)
}
fn relative_constraint(
    expr: &str,
    scene: &SceneGraph,
    rects: &[Rect],
    x: bool,
    grid: f64,
    scope: Option<&str>,
) -> Result<Option<(usize, f64)>, Error> {
    if number(expr).is_some() {
        return Ok(None);
    }
    let e = expr.replace(' ', "");
    let base = e.split(['+', '-']).next().unwrap();
    let (id, anchor) = base
        .rsplit_once('.')
        .filter(|(_, a)| matches!(*a, "N" | "E" | "S" | "W" | "x" | "y"))
        .unwrap_or((base, if x { "x" } else { "y" }));
    if x && matches!(anchor, "N" | "S" | "y") || !x && matches!(anchor, "E" | "W" | "x") {
        return Err(Error::Layout(
            "a relative pin must reference the same coordinate axis".into(),
        ));
    }
    let j = coordinate_index(scene, id, scope)
        .ok_or_else(|| Error::Layout(format!("unknown coordinate anchor {id}")))?;
    let value = coordinate(expr, scene, rects, x, grid, scope)?;
    Ok(Some((j, value - if x { rects[j].x } else { rects[j].y })))
}
fn check_pin_cycles(scene: &SceneGraph, anns: &[Vec<Annotation>]) -> Result<(), Error> {
    fn visit(i: usize, edges: &[Vec<usize>], state: &mut [u8]) -> bool {
        if state[i] == 1 {
            return false;
        }
        if state[i] == 2 {
            return true;
        }
        state[i] = 1;
        for &j in &edges[i] {
            if !visit(j, edges, state) {
                return false;
            }
        }
        state[i] = 2;
        true
    }
    let mut deps = vec![Vec::new(); scene.nodes.len()];
    for (i, list) in anns.iter().enumerate() {
        for a in list.iter().filter(|a| a.name == "pin") {
            for key in ["x", "y"] {
                if let Some(v) = a.attrs.get(key) {
                    let base = v.text.split(['+', '-']).next().unwrap().trim();
                    let id = base
                        .rsplit_once('.')
                        .filter(|(_, a)| matches!(*a, "N" | "E" | "S" | "W" | "x" | "y"))
                        .map(|(id, _)| id)
                        .unwrap_or(base);
                    if let Some(j) = scene.index(id) {
                        deps[i].push(j);
                    }
                }
            }
        }
    }
    let mut state = vec![0; deps.len()];
    for i in 0..deps.len() {
        if !visit(i, &deps, &mut state) {
            return Err(Error::Layout("cyclic relative pin anchors".into()));
        }
    }
    Ok(())
}
// Select free attachment sides jointly. Center-vector heuristics alone can
// choose a side whose outward stub points into another node or forces a loop.
fn attachment_pair(
    scene: &SceneGraph,
    rects: &[Rect],
    e: &Edge,
    o: &LayoutOptions,
    prior: Option<&Layout>,
) -> (Side, Side) {
    let ai = scene.index(&e.source.node).unwrap();
    let bi = scene.index(&e.target.node).unwrap();
    if ai == bi {
        return (
            e.source.constraint.side().unwrap_or(Side::E),
            e.target.constraint.side().unwrap_or(Side::N),
        );
    }
    let point = |r: Rect, s: Side| match s {
        Side::N => Point::new(r.x, r.top()),
        Side::E => Point::new(r.right(), r.y),
        Side::S => Point::new(r.x, r.bottom()),
        Side::W => Point::new(r.left(), r.y),
    };
    let mut best = (f64::INFINITY, Side::E, Side::W);
    let sides = [Side::E, Side::W, Side::S, Side::N];
    for sa in sides {
        for sb in sides {
            if e.source.constraint.side().is_some_and(|s| s != sa)
                || e.target.constraint.side().is_some_and(|s| s != sb)
            {
                continue;
            }
            let a = point(rects[ai], sa);
            let b = point(rects[bi], sb);
            let (ax, ay) = sa.normal();
            let (bx, by) = sb.normal();
            let u = a.offset(ax * o.stub, ay * o.stub);
            let v = b.offset(bx * o.stub, by * o.stub);
            for corner in [Point::new(u.x, v.y), Point::new(v.x, u.y)] {
                let path = [a, u, corner, v, b];
                let mut cost = path.windows(2).map(|p| p[0].distance(p[1])).sum::<f64>();
                cost += crate::routing::coalesced(&path).len().saturating_sub(2) as f64
                    * o.weights.bend;
                for (index, p) in path.windows(2).enumerate() {
                    if p[0].distance(p[1]) < 1e-6 {
                        continue;
                    }
                    for (i, r) in rects.iter().enumerate() {
                        if i == ai && index == 0 || i == bi && index == 3 {
                            continue;
                        }
                        if scene.nodes[i].container {
                            continue;
                        }
                        if r.inflate(o.margin).blocks(p[0], p[1]) {
                            cost += 100000.;
                        }
                    }
                }
                // Penalize departures and arrivals that immediately reverse their
                // mandatory outward stubs, including zero-length corner legs.
                let after = if u.distance(corner) > 1e-6 { corner } else { v };
                let before = if v.distance(corner) > 1e-6 { corner } else { u };
                if (after.x - u.x) * ax + (after.y - u.y) * ay < -1e-6 {
                    cost += 100000.;
                }
                if (before.x - v.x) * bx + (before.y - v.y) * by < -1e-6 {
                    cost += 100000.;
                }
                let ra = rects[ai];
                let rb = rects[bi];
                let mixed_axes = matches!(sa, Side::E | Side::W) != matches!(sb, Side::E | Side::W);
                let x_sep =
                    rb.left() - ra.right() >= 2. * o.stub || ra.left() - rb.right() >= 2. * o.stub;
                let y_ov = ra.top() < rb.bottom() && rb.top() < ra.bottom();
                let y_sep =
                    rb.top() - ra.bottom() >= 2. * o.stub || ra.top() - rb.bottom() >= 2. * o.stub;
                let x_ov = ra.left() < rb.right() && rb.left() < ra.right();
                if mixed_axes && ((x_sep && y_ov) || (y_sep && x_ov)) {
                    cost += o.weights.bend;
                }
                if let Some(prior) = prior {
                    for (key, side) in [(port_key(e, true), sa), (port_key(e, false), sb)] {
                        if prior.ports.get(&key).is_some_and(|p| p.side != side) {
                            cost += 2. * o.grid;
                        }
                    }
                }
                if cost < best.0 - 1e-6 {
                    best = (cost, sa, sb);
                }
            }
        }
    }
    (best.1, best.2)
}

fn port_target(
    b: Rect,
    constraint: &PortConstraint,
    side: Side,
    other_side: Side,
    grid: f64,
) -> f64 {
    match *constraint {
        PortConstraint::FixedPos { offset_grid, .. } => {
            if matches!(side, Side::E | Side::W) && matches!(other_side, Side::E | Side::W) {
                b.top() + offset_grid * grid
            } else if matches!(side, Side::N | Side::S) && matches!(other_side, Side::N | Side::S) {
                b.left() + offset_grid * grid
            } else if matches!(side, Side::E | Side::W) {
                b.y
            } else {
                b.x
            }
        }
        PortConstraint::FixedRatio { ratio, .. } => {
            if matches!(side, Side::E | Side::W) && matches!(other_side, Side::E | Side::W) {
                b.top() + ratio * b.h
            } else if matches!(side, Side::N | Side::S) && matches!(other_side, Side::N | Side::S) {
                b.left() + ratio * b.w
            } else if matches!(side, Side::E | Side::W) {
                b.y
            } else {
                b.x
            }
        }
        _ => {
            if matches!(side, Side::E | Side::W) {
                match other_side {
                    Side::N => b.top(),
                    Side::S => b.bottom(),
                    _ => b.y,
                }
            } else {
                match other_side {
                    Side::W => b.left(),
                    Side::E => b.right(),
                    _ => b.x,
                }
            }
        }
    }
}

fn port_requests(
    scene: &SceneGraph,
    rects: &[Rect],
    o: &LayoutOptions,
    prior: Option<&Layout>,
) -> Vec<PortRequest> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for e in &scene.edges {
        let pair = attachment_pair(scene, rects, e, o, prior);
        for (source, ep, other) in [(true, &e.source, &e.target), (false, &e.target, &e.source)] {
            let i = scene.index(&ep.node).unwrap();
            let j = scene.index(&other.node).unwrap();
            let b = rects[j];
            let side = if source { pair.0 } else { pair.1 };
            let other_side = if source { pair.1 } else { pair.0 };
            let target = port_target(b, &other.constraint, side, other_side, o.grid);

            let key = port_key(e, source);
            if seen.insert(key.clone()) {
                out.push(PortRequest {
                    key,
                    node: i,
                    side,
                    spec: ep.constraint.clone(),
                    target,
                });
            }
        }
    }
    for (i, n) in scene.nodes.iter().enumerate() {
        for p in n.ports.values() {
            let key = format!("{}#{}", n.id, p.id);
            if seen.insert(key.clone()) {
                out.push(PortRequest {
                    key,
                    node: i,
                    side: p.constraint.side().unwrap_or(Side::E),
                    spec: p.constraint.clone(),
                    target: 0.,
                });
            }
        }
    }
    out
}
pub fn port_key(edge: &Edge, source: bool) -> String {
    let ep = if source { &edge.source } else { &edge.target };
    ep.port
        .as_ref()
        .map(|p| format!("{}#{p}", ep.node))
        .unwrap_or_else(|| format!("{}:{}", edge.id, if source { "source" } else { "target" }))
}
fn side_index(side: Side) -> usize {
    match side {
        Side::N => 0,
        Side::E => 1,
        Side::S => 2,
        Side::W => 3,
    }
}
fn stub_envelope(r: Rect, s: [bool; 4], stub: f64) -> Rect {
    let n = if s[0] { stub } else { 0. };
    let e = if s[1] { stub } else { 0. };
    let south = if s[2] { stub } else { 0. };
    let w = if s[3] { stub } else { 0. };
    Rect::new(
        r.x + (e - w) / 2.,
        r.y + (south - n) / 2.,
        r.w + e + w,
        r.h + n + south,
    )
}
fn grow_port_perimeters(
    scene: &SceneGraph,
    requests: &[PortRequest],
    rects: &mut [Rect],
    o: &LayoutOptions,
) -> Result<(), Error> {
    let port_sides: BTreeMap<&str, (usize, Side)> = requests
        .iter()
        .map(|p| (p.key.as_str(), (p.node, p.side)))
        .collect();
    let has_lock = |idx: usize| {
        effective_annotations(
            scene,
            &scene.nodes[idx].id,
            &scene.nodes[idx].annotations,
            o.variant.as_deref(),
        )
        .iter()
        .any(|a| a.name == "pin" && a.attrs.contains_key("lock"))
    };
    for (i, r) in rects.iter_mut().enumerate() {
        let locked = has_lock(i);
        for side in [Side::N, Side::E, Side::S, Side::W] {
            let ports: Vec<_> = requests
                .iter()
                .filter(|p| p.node == i && p.side == side)
                .collect();
            let mut length = ports.len().saturating_sub(1) as f64 * o.port_spacing + 2. * o.corner;
            if !locked && ports.len() >= 2 {
                let opp = match side {
                    Side::N => Side::S,
                    Side::S => Side::N,
                    Side::E => Side::W,
                    Side::W => Side::E,
                };
                let mut facing_counts: BTreeMap<usize, usize> = BTreeMap::new();
                for e in &scene.edges {
                    let Some(&(sa_i, sa)) = port_sides.get(port_key(e, true).as_str()) else {
                        continue;
                    };
                    let Some(&(sb_i, sb)) = port_sides.get(port_key(e, false).as_str()) else {
                        continue;
                    };
                    if sa_i == sb_i {
                        continue;
                    }
                    if sa_i == i && sa == side && sb == opp && !has_lock(sb_i) {
                        *facing_counts.entry(sb_i).or_default() += 1;
                    } else if sb_i == i && sb == side && sa == opp && !has_lock(sa_i) {
                        *facing_counts.entry(sa_i).or_default() += 1;
                    }
                }
                for (peer_idx, count) in facing_counts {
                    if count >= 2 {
                        let peer_ports = requests
                            .iter()
                            .filter(|q| q.node == peer_idx && q.side == opp)
                            .count();
                        length = length.max(
                            peer_ports.saturating_sub(1) as f64 * o.port_spacing + 2. * o.corner,
                        );
                    }
                }
            }
            let peer_edges: Vec<_> = scene
                .edges
                .iter()
                .filter(|e| {
                    e.source.node == scene.nodes[i].id || e.target.node == scene.nodes[i].id
                })
                .collect();
            let peers: BTreeSet<_> = peer_edges
                .iter()
                .map(|e| {
                    if e.source.node == scene.nodes[i].id {
                        &e.target.node
                    } else {
                        &e.source.node
                    }
                })
                .collect();
            if ports.len() > 1
                && peers.len() == 1
                && !locked
                && ports.iter().all(|p| {
                    matches!(
                        p.spec,
                        PortConstraint::Free | PortConstraint::FixedSide { .. }
                    )
                })
            {
                let caption_space = peer_edges
                    .iter()
                    .map(|e| {
                        if matches!(side, Side::N | Side::S) {
                            crate::labels::text_width(&e.label) + 32.
                        } else {
                            crate::labels::text_height(&crate::labels::connection_text(scene, e))
                        }
                    })
                    .fold(0., f64::max)
                    + 12.;
                length = length.max((ports.len() + 1) as f64 * caption_space.max(o.port_spacing));
            }
            let mut ratios = Vec::new();
            let movable = ports
                .iter()
                .filter(|p| {
                    !matches!(
                        p.spec,
                        PortConstraint::FixedPos { .. } | PortConstraint::FixedRatio { .. }
                    )
                })
                .count();
            for p in &ports {
                match p.spec {
                    PortConstraint::FixedPos { offset_grid, .. } => {
                        // A free attachment must still fit beside an unused
                        // fixed slot. Counting ports alone misses the clearance
                        // consumed by a slot close to the start of the side.
                        length = length
                            .max(offset_grid * o.grid + movable as f64 * o.port_spacing + o.corner)
                    }
                    PortConstraint::FixedRatio { ratio, .. } => {
                        if ratio <= 0. || ratio >= 1. {
                            return Err(Error::Layout("port ratio must be in (0,1)".into()));
                        }
                        length = length.max(o.corner / ratio.min(1. - ratio));
                        ratios.push(ratio);
                    }
                    _ => {}
                }
            }
            ratios.sort_by(f64::total_cmp);
            for pair in ratios.windows(2) {
                let difference = pair[1] - pair[0];
                if difference <= f64::EPSILON {
                    return Err(Error::Layout(
                        "distinct ports cannot share the same fixed ratio".into(),
                    ));
                }
                length = length.max(o.port_spacing / difference);
            }
            if !length.is_finite() {
                return Err(Error::Layout(
                    "port perimeter exceeds finite coordinate range".into(),
                ));
            }
            if matches!(side, Side::N | Side::S) {
                r.w = r.w.max(length);
            } else {
                r.h = r.h.max(length);
            }
        }
    }
    Ok(())
}
fn assign_ports<'a>(
    scene: &SceneGraph,
    o: &LayoutOptions,
    requests: &'a [PortRequest],
    rects: &[Rect],
    pins: &[(Option<f64>, Option<f64>)],
    align_facing: bool,
    out: &mut Layout,
) -> Result<(), Error> {
    let mut side_orders: Vec<(usize, Side, Vec<&'a PortRequest>)> = Vec::new();
    let write_port = |node: &Node, r: Rect, side: Side, t: f64, key: &str, out: &mut Layout| {
        let box_point = match side {
            Side::N => Point::new(r.left() + t, r.top()),
            Side::E => Point::new(r.right(), r.top() + t),
            Side::S => Point::new(r.left() + t, r.bottom()),
            Side::W => Point::new(r.left(), r.top() + t),
        };
        let shape = if node.container && node.shape != "sql_table" {
            "container"
        } else {
            &node.shape
        };
        let point = crate::shapes::attachment(shape, r, side, t);
        let (nx, ny) = side.normal();
        out.ports.insert(
            key.to_string(),
            PortPlacement {
                point,
                stub: box_point.offset(nx * o.stub, ny * o.stub),
                side,
                key: key.to_string(),
            },
        );
    };
    for (i, node) in scene.nodes.iter().enumerate() {
        for side in [Side::N, Side::E, Side::S, Side::W] {
            let mut ps: Vec<_> = requests
                .iter()
                .filter(|p| p.node == i && p.side == side)
                .collect();
            let reverse = matches!(side, Side::S | Side::W);
            ps.sort_by(|a, b| {
                let order = if reverse {
                    b.target.total_cmp(&a.target)
                } else {
                    a.target.total_cmp(&b.target)
                };
                order.then_with(|| {
                    if reverse {
                        b.key.cmp(&a.key)
                    } else {
                        a.key.cmp(&b.key)
                    }
                })
            });
            let positions: Vec<_> = ps
                .iter()
                .enumerate()
                .filter(|(_, p)| matches!(p.spec, PortConstraint::FixedOrder { .. }))
                .map(|(i, _)| i)
                .collect();
            let mut ordered_ports: Vec<_> = positions.iter().map(|i| ps[*i]).collect();
            ordered_ports.sort_by_key(|p| {
                if let PortConstraint::FixedOrder { order, .. } = p.spec {
                    order
                } else {
                    0
                }
            });
            for (i, p) in positions.into_iter().zip(ordered_ports) {
                ps[i] = p;
            }
            let r = rects[i];
            let len = if matches!(side, Side::N | Side::S) {
                r.w
            } else {
                r.h
            };
            let count = ps.len();
            let mut desired = Vec::new();
            let mut cs = Vec::new();
            for (j, p) in ps.iter().enumerate() {
                let initial = (j + 1) as f64 * len / (count + 1) as f64;
                desired.push(initial);
                let fixed = match p.spec {
                    PortConstraint::FixedRatio { ratio, .. } => Some(if reverse {
                        len - ratio * len
                    } else {
                        ratio * len
                    }),
                    PortConstraint::FixedPos { offset_grid, .. } => Some(if reverse {
                        len - offset_grid * o.grid
                    } else {
                        offset_grid * o.grid
                    }),
                    _ => None,
                };
                if let Some(v) = fixed {
                    cs.push(Constraint::fixed(j, v));
                }
                if j > 0 {
                    cs.push(Constraint::separation(j - 1, j, o.port_spacing));
                }
            }
            // Fixed geometric slots participate in ordering by position, independent of endpoint declaration order.
            let mut ordered: Vec<_> = ps
                .iter()
                .enumerate()
                .map(|(j, p)| {
                    let t = match p.spec {
                        PortConstraint::FixedRatio { ratio, .. } => {
                            if reverse {
                                len - ratio * len
                            } else {
                                ratio * len
                            }
                        }
                        PortConstraint::FixedPos { offset_grid, .. } => {
                            if reverse {
                                len - offset_grid * o.grid
                            } else {
                                offset_grid * o.grid
                            }
                        }
                        _ => desired[j],
                    };
                    (j, t)
                })
                .collect();
            if ps.iter().any(|p| {
                matches!(
                    p.spec,
                    PortConstraint::FixedRatio { .. } | PortConstraint::FixedPos { .. }
                )
            }) {
                ordered.sort_by(|a, b| a.1.total_cmp(&b.1));
                // Evenly spaced ideals can put a free endpoint on the wrong
                // side of a fixed slot, forcing it beyond the node corner.
                // Reassign only those infeasible orders to available gaps.
                let fixed = |j: usize| {
                    matches!(
                        ps[j].spec,
                        PortConstraint::FixedPos { .. } | PortConstraint::FixedRatio { .. }
                    )
                };
                let mut lower = o.corner;
                let feasible = ordered.iter().all(|&(j, t)| {
                    let value = if fixed(j) { t } else { t.max(lower) };
                    let ok = value >= lower - 1e-6 && value <= len - o.corner + 1e-6;
                    lower = value + o.port_spacing;
                    ok
                });
                if !feasible {
                    let anchors: Vec<_> = ordered
                        .iter()
                        .filter(|&&(j, _)| fixed(j))
                        .copied()
                        .collect();
                    let mut available = Vec::new();
                    let mut begin = o.corner;
                    for end in anchors
                        .iter()
                        .map(|(_, t)| t - o.port_spacing)
                        .chain(std::iter::once(len - o.corner))
                    {
                        let mut t = begin;
                        while t <= end + 1e-6 {
                            available.push(t);
                            t += o.port_spacing;
                        }
                        begin = end + 2. * o.port_spacing;
                    }
                    let free: Vec<_> = ordered
                        .iter()
                        .filter(|&&(j, _)| !fixed(j))
                        .copied()
                        .collect();
                    if available.len() >= free.len() {
                        let mut slots = Vec::new();
                        for &(j, ideal) in &free {
                            let k = (0..available.len())
                                .min_by(|&a, &b| {
                                    (available[a] - ideal)
                                        .abs()
                                        .total_cmp(&(available[b] - ideal).abs())
                                })
                                .unwrap();
                            slots.push(available.remove(k));
                            let _ = j;
                        }
                        slots.sort_by(f64::total_cmp);
                        ordered = anchors;
                        for ((j, _), t) in free.into_iter().zip(slots) {
                            desired[j] = t;
                            ordered.push((j, t));
                        }
                        ordered.sort_by(|a, b| a.1.total_cmp(&b.1));
                    }
                }
                cs.retain(|c| c.equality);
                for pair in ordered.windows(2) {
                    cs.push(Constraint::separation(pair[0].0, pair[1].0, o.port_spacing));
                }
            }
            let positions = solver::project(&desired, &vec![1.; count], &cs)?;
            for (j, p) in ps.iter().enumerate() {
                let mut t = positions[j];
                if t < o.corner - 1e-5 || t > len - o.corner + 1e-5 {
                    return Err(Error::Layout(format!(
                        "port clearance conflict at {}",
                        p.key
                    )));
                }
                if reverse {
                    t = len - t;
                }
                write_port(node, r, side, t, &p.key, out);
            }
            let final_ps: Vec<&'a PortRequest> = ordered.into_iter().map(|(j, _)| ps[j]).collect();
            if !final_ps.is_empty() {
                side_orders.push((i, side, final_ps));
            }
        }
    }
    if align_facing {
        let mut facing_peers: BTreeMap<String, Vec<(usize, String)>> = BTreeMap::new();
        for e in &scene.edges {
            let ai = scene.index(&e.source.node).unwrap();
            let bi = scene.index(&e.target.node).unwrap();
            if ai == bi || pins[ai] != (None, None) || pins[bi] != (None, None) {
                continue;
            }
            let ka = port_key(e, true);
            let kb = port_key(e, false);
            let (Some(pa), Some(pb)) = (out.ports.get(&ka), out.ports.get(&kb)) else {
                continue;
            };
            let valid_facing = match (pa.side, pb.side) {
                (Side::E, Side::W) => rects[ai].right() + 2. * o.stub <= rects[bi].left() + 1e-6,
                (Side::W, Side::E) => rects[bi].right() + 2. * o.stub <= rects[ai].left() + 1e-6,
                (Side::S, Side::N) => rects[ai].bottom() + 2. * o.stub <= rects[bi].top() + 1e-6,
                (Side::N, Side::S) => rects[bi].bottom() + 2. * o.stub <= rects[ai].top() + 1e-6,
                _ => false,
            };
            if valid_facing {
                facing_peers
                    .entry(ka.clone())
                    .or_default()
                    .push((bi, kb.clone()));
                facing_peers.entry(kb).or_default().push((ai, ka));
            }
        }
        if !facing_peers.is_empty() {
            let mut side_distinct_peers: BTreeMap<(usize, Side), BTreeSet<usize>> = BTreeMap::new();
            for e in &scene.edges {
                let ai = scene.index(&e.source.node).unwrap();
                let bi = scene.index(&e.target.node).unwrap();
                if ai == bi {
                    continue;
                }
                if let Some(pa) = out.ports.get(&port_key(e, true)) {
                    side_distinct_peers
                        .entry((ai, pa.side))
                        .or_default()
                        .insert(bi);
                }
                if let Some(pb) = out.ports.get(&port_key(e, false)) {
                    side_distinct_peers
                        .entry((bi, pb.side))
                        .or_default()
                        .insert(ai);
                }
            }
            for sweep in 0..4 {
                let idxs: Vec<usize> = if sweep % 2 == 0 {
                    (0..side_orders.len()).collect()
                } else {
                    (0..side_orders.len()).rev().collect()
                };
                for idx in idxs {
                    let (i, side, ref ps) = side_orders[idx];
                    let r = rects[i];
                    let horiz_side = matches!(side, Side::N | Side::S);
                    let len = if horiz_side { r.w } else { r.h };
                    let reverse = matches!(side, Side::S | Side::W);
                    let count = ps.len();
                    if len < 2. * o.corner - 1e-6 {
                        continue;
                    }
                    let mut desired = Vec::with_capacity(count);
                    let mut weights = Vec::with_capacity(count);
                    let mut cs = Vec::new();
                    cs.push(Constraint {
                        coefficients: vec![(0, 1.)],
                        rhs: o.corner,
                        equality: false,
                    });
                    cs.push(Constraint {
                        coefficients: vec![(count - 1, -1.)],
                        rhs: -(len - o.corner),
                        equality: false,
                    });
                    let mut any_facing = false;
                    for (j, p) in ps.iter().enumerate() {
                        let base_t = (j + 1) as f64 * len / (count + 1) as f64;
                        let fixed = match p.spec {
                            PortConstraint::FixedRatio { ratio, .. } => Some(if reverse {
                                len - ratio * len
                            } else {
                                ratio * len
                            }),
                            PortConstraint::FixedPos { offset_grid, .. } => Some(if reverse {
                                len - offset_grid * o.grid
                            } else {
                                offset_grid * o.grid
                            }),
                            _ => None,
                        };
                        if let Some(v) = fixed {
                            desired.push(v);
                            weights.push(1.);
                            cs.push(Constraint::fixed(j, v));
                        } else if let Some(peers) = facing_peers.get(&p.key) {
                            let mut targets = Vec::new();
                            for (peer_idx, peer_key) in peers {
                                let peer_side = out.ports[peer_key].side;
                                if count == 1
                                    && side_distinct_peers
                                        .get(&(*peer_idx, peer_side))
                                        .is_some_and(|s| s.len() >= 3)
                                {
                                    continue;
                                }
                                let pr = rects[*peer_idx];
                                let (ov_lo, ov_hi) = if horiz_side {
                                    (
                                        (r.left() + o.corner).max(pr.left() + o.corner),
                                        (r.right() - o.corner).min(pr.right() - o.corner),
                                    )
                                } else {
                                    (
                                        (r.top() + o.corner).max(pr.top() + o.corner),
                                        (r.bottom() - o.corner).min(pr.bottom() - o.corner),
                                    )
                                };
                                if ov_lo > ov_hi + 1e-6 {
                                    continue;
                                }
                                let peer_pt = out.ports[peer_key].point;
                                let coord = if horiz_side {
                                    peer_pt.x.clamp(ov_lo, ov_hi)
                                } else {
                                    peer_pt.y.clamp(ov_lo, ov_hi)
                                };
                                let (p0, p1) = if horiz_side {
                                    (
                                        Point::new(coord, r.bottom().min(pr.bottom())),
                                        Point::new(coord, r.top().max(pr.top())),
                                    )
                                } else {
                                    (
                                        Point::new(r.right().min(pr.right()), coord),
                                        Point::new(r.left().max(pr.left()), coord),
                                    )
                                };
                                let blocked = rects.iter().enumerate().any(|(k, rk)| {
                                    k != i
                                        && k != *peer_idx
                                        && !scene.nodes[k].container
                                        && rk.inflate(o.margin).blocks(p0, p1)
                                });
                                if !blocked {
                                    let t_raw = if horiz_side {
                                        coord - r.left()
                                    } else {
                                        coord - r.top()
                                    };
                                    let param = if reverse { len - t_raw } else { t_raw };
                                    targets.push(param.clamp(o.corner, len - o.corner));
                                }
                            }
                            if targets.is_empty() {
                                desired.push(base_t);
                                weights.push(1.);
                            } else {
                                any_facing = true;
                                desired.push(targets.iter().sum::<f64>() / targets.len() as f64);
                                weights.push(12.);
                            }
                        } else {
                            desired.push(base_t);
                            weights.push(1.);
                        }
                        if j > 0 {
                            cs.push(Constraint::separation(j - 1, j, o.port_spacing));
                        }
                    }
                    if !any_facing {
                        continue;
                    }
                    if let Ok(positions) = solver::project(&desired, &weights, &cs)
                        && positions
                            .iter()
                            .all(|&pos| pos >= o.corner - 1e-5 && pos <= len - o.corner + 1e-5)
                    {
                        let node = &scene.nodes[i];
                        for (j, p) in ps.iter().enumerate() {
                            let mut t = positions[j];
                            if reverse {
                                t = len - t;
                            }
                            write_port(node, r, side, t, &p.key, out);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
fn preserve_proximity(
    scene: &SceneGraph,
    prior: &Layout,
    rects: &[Rect],
    dx: &mut [f64],
    dy: &mut [f64],
    pins: &[(Option<f64>, Option<f64>)],
) {
    for (a, b) in delaunay_edges(scene, prior) {
        let pa = prior.nodes[&scene.nodes[a].id];
        let pb = prior.nodes[&scene.nodes[b].id];
        if pins[a].0.is_none() {
            dx[a] += (rects[b].x + (pa.x - pb.x) - dx[a]) * 0.05;
        }
        if pins[a].1.is_none() {
            dy[a] += (rects[b].y + (pa.y - pb.y) - dy[a]) * 0.05;
        }
    }
}
fn triangulated_edges(points: &[delaunator::Point]) -> Vec<(usize, usize)> {
    let tri = delaunator::triangulate(points);
    let mut edges = Vec::with_capacity(tri.triangles.len());
    for t in tri.triangles.as_chunks::<3>().0 {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            edges.push((a.min(b), a.max(b)));
        }
    }
    if edges.is_empty() {
        edges.extend((1..points.len()).map(|i| (i - 1, i)));
    }
    edges.sort_unstable();
    edges.dedup();
    edges
}
fn delaunay_edges(scene: &SceneGraph, layout: &Layout) -> Vec<(usize, usize)> {
    let ids: Vec<_> = scene
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| {
            layout
                .nodes
                .get(&n.id)
                .map(|r| (i, delaunator::Point { x: r.x, y: r.y }))
        })
        .collect();
    triangulated_edges(
        &ids.iter()
            .map(|(_, p)| delaunator::Point { x: p.x, y: p.y })
            .collect::<Vec<_>>(),
    )
    .into_iter()
    .map(|(a, b)| (ids[a].0, ids[b].0))
    .collect()
}
pub fn stability(scene: &SceneGraph, prior: &Layout, current: &Layout) -> StabilityMetrics {
    let pairs: Vec<_> = scene
        .nodes
        .iter()
        .filter_map(|n| Some((*prior.nodes.get(&n.id)?, *current.nodes.get(&n.id)?)))
        .collect();
    if pairs.iter().all(|(a, b)| a.x == b.x && a.y == b.y) {
        return StabilityMetrics::default();
    }
    let n = pairs.len().max(1) as f64;
    let delta = Point::new(
        pairs.iter().map(|(a, b)| b.x - a.x).sum::<f64>() / n,
        pairs.iter().map(|(a, b)| b.y - a.y).sum::<f64>() / n,
    );
    let drift = pairs
        .iter()
        .map(|(a, b)| (b.x - a.x - delta.x).powi(2) + (b.y - a.y - delta.y).powi(2))
        .sum();
    let changed: Vec<_> = pairs
        .iter()
        .map(|(a, b)| a.x != b.x || a.y != b.y)
        .collect();
    let mut flips = 0;
    for (i, (a, b)) in pairs.iter().enumerate().filter(|(i, _)| changed[*i]) {
        for (j, (c, d)) in pairs.iter().enumerate() {
            if i == j || changed[j] && j < i {
                continue;
            }
            flips += usize::from((a.x - c.x) * (b.x - d.x) < -1.);
            flips += usize::from((a.y - c.y) * (b.y - d.y) < -1.);
        }
    }
    let mut distortion = 0.;
    // Use the already indexed surviving rectangles. Looking up both endpoints
    // in string-keyed maps for every triangle edge dominated local Wasm edits.
    let points: Vec<_> = pairs
        .iter()
        .map(|(a, _)| delaunator::Point { x: a.x, y: a.y })
        .collect();
    for (a, b) in triangulated_edges(&points) {
        let old = pairs[a].0.center().distance(pairs[b].0.center());
        let new = pairs[a].1.center().distance(pairs[b].1.center());
        distortion += (new / old.max(1.) - 1.).powi(2);
    }
    StabilityMetrics {
        translation: delta,
        squared_drift: drift,
        order_flips: flips,
        proximity_distortion: distortion,
    }
}
pub fn validate(scene: &SceneGraph, l: &Layout, o: &LayoutOptions) -> Result<(), Error> {
    for (i, a) in scene.nodes.iter().enumerate() {
        let ra = l.nodes[&a.id];
        if [ra.x, ra.y, ra.w, ra.h].iter().any(|x| !x.is_finite()) || ra.w <= 0. || ra.h <= 0. {
            return Err(Error::Layout("non-finite or empty node geometry".into()));
        }
        let title = crate::labels::node_title(a);
        let title_height = (title.lines().count().saturating_sub(1)) as f64 * 20.
            + 18.
            + if a.shape == "database" { 16. } else { 0. };
        if !a.container
            && !matches!(a.shape.as_str(), "sql_table" | "diamond" | "decision")
            && (title_width(a) - 32. > ra.w + 1e-6 || title_height > ra.h + 1e-6)
        {
            return Err(Error::Layout(format!(
                "title does not fit the locked size of {}",
                a.id
            )));
        }
        if matches!(a.shape.as_str(), "diamond" | "decision")
            && (title_width(a) - 16.) / ra.w
                + crate::labels::text_height(&crate::labels::node_title(a)) * 16. / 15. / ra.h
                > 1.
        {
            return Err(Error::Layout(format!(
                "decision title does not fit the locked size of {}",
                a.id
            )));
        }
        if a.shape == "sql_table" {
            let mut fields = Vec::new();
            for port in a
                .ports
                .values()
                .filter(|p| matches!(p.constraint, PortConstraint::FixedPos { .. }))
            {
                let placement = &l.ports[&format!("{}#{}", a.id, port.id)];
                let height = 15. + (port.label.split('\n').count() - 1) as f64 * 18.;
                let width = crate::labels::text_width(&port.label) * 13. / 15.;
                let bounds = Rect::new(
                    ra.left() + 14. + width / 2.,
                    placement.point.y - 7. + height / 2.,
                    width,
                    height,
                );
                if !ra.encloses(bounds) || fields.iter().any(|r: &Rect| r.intersects(bounds)) {
                    return Err(Error::Layout(format!(
                        "record field {}#{} overlaps another row or exceeds its locked size; increase pos spacing or record dimensions",
                        a.id, port.id
                    )));
                }
                fields.push(bounds);
            }
        }
        if let Some(p) = &a.parent
            && !l.nodes[p].encloses(ra)
        {
            return Err(Error::Layout(format!("{} escapes container {p}", a.id)));
        }
        let aa = ancestors(scene, &a.id);
        for b in &scene.nodes[i + 1..] {
            if !aa.contains(&b.id)
                && !ancestors(scene, &b.id).contains(&a.id)
                && ra.intersects(l.nodes[&b.id])
            {
                return Err(Error::Layout(format!(
                    "node collision: {} and {}",
                    a.id, b.id
                )));
            }
        }
    }
    for e in &scene.edges {
        let route = l
            .routes
            .get(&e.id)
            .ok_or_else(|| Error::Layout(format!("missing route {}", e.id)))?;
        if !crate::routing::route_is_valid(scene, l, e, route, o.margin) {
            return Err(Error::Layout(format!(
                "route {} violates attachment, clearance or simple-path constraints",
                e.id
            )));
        }
        if route
            .points
            .iter()
            .any(|p| !p.x.is_finite() || !p.y.is_finite())
        {
            return Err(Error::Layout("non-finite route coordinates".into()));
        }
        if route.points.len() < 2 {
            return Err(Error::Layout("empty route".into()));
        }
        let obstacles = crate::routing::obstacles(scene, l, e, o.margin);
        for pair in route.points.windows(2) {
            if (pair[0].x - pair[1].x).abs() > 1e-6 && (pair[0].y - pair[1].y).abs() > 1e-6 {
                return Err(Error::Layout("non-orthogonal route".into()));
            }
            if obstacles.iter().any(|r| r.blocks(pair[0], pair[1])) {
                return Err(Error::Layout(format!("route {} intersects a node", e.id)));
            }
        }
    }
    Ok(())
}
pub fn convex_hull(mut points: Vec<Point>) -> Vec<Point> {
    points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    points.dedup();
    if points.len() < 3 {
        return points;
    }
    fn cross(a: Point, b: Point, c: Point) -> f64 {
        (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
    }
    let mut lower = Vec::new();
    for &p in &points {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0. {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper = Vec::new();
    for &p in points.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0. {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

fn semantic_key(scene: &SceneGraph, options: &LayoutOptions) -> String {
    fn attrs(a: &Attributes) -> Vec<(&str, &str)> {
        a.iter()
            .map(|(k, v)| (k.as_str(), v.text.as_str()))
            .collect()
    }
    fn anns(a: &[Annotation]) -> Vec<(&str, Vec<(&str, &str)>)> {
        a.iter()
            .map(|a| (a.name.as_str(), attrs(&a.attrs)))
            .collect()
    }
    let nodes: Vec<_> = scene
        .nodes
        .iter()
        .map(|n| {
            (
                &n.id,
                title_width(n).max(112.),
                crate::labels::node_title(n).lines().count(),
                &n.parent,
                &n.shape,
                n.container,
                attrs(&n.attrs),
                anns(&n.annotations),
                n.ports
                    .values()
                    .map(|p| {
                        (
                            &p.id,
                            crate::labels::text_width(&p.label),
                            p.label.chars().count(),
                            &p.constraint,
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    let edges: Vec<_> = scene
        .edges
        .iter()
        .map(|e| {
            (
                &e.id,
                (&e.source.node, &e.source.port, &e.source.constraint),
                (&e.target.node, &e.target.port, &e.target.constraint),
                crate::labels::text_width(&crate::labels::connection_text(scene, e)),
                crate::labels::text_height(&crate::labels::connection_text(scene, e)),
                e.kind,
                attrs(&e.attrs),
                anns(&e.annotations),
            )
        })
        .collect();
    let zones: Vec<_> = scene
        .zones
        .iter()
        .map(|z| (&z.id, &z.label, &z.members, attrs(&z.attrs)))
        .collect();
    let relations: Vec<_> = scene
        .relations
        .iter()
        .map(|r| (r.axis, &r.members, r.distribute, r.gap_grid))
        .collect();
    let rules: Vec<_> = scene
        .layout_rules
        .iter()
        .map(|r| (&r.target, &r.variant, anns(&r.annotations)))
        .collect();
    serde_json::to_string(&(nodes, edges, zones, relations, rules, options))
        .expect("serializable semantic geometry")
}

// Query keys exclude source origins. Byte-only edits can reuse geometry, and local container
// changes only invalidate an independent container when its reserved bounds remain feasible.
fn query_keys(scene: &SceneGraph, o: &LayoutOptions) -> (BTreeMap<String, String>, String) {
    fn attrs(a: &Attributes) -> Vec<(&str, &str)> {
        a.iter()
            .map(|(k, v)| (k.as_str(), v.text.as_str()))
            .collect()
    }
    fn anns(a: &[Annotation]) -> Vec<(&str, Vec<(&str, &str)>)> {
        a.iter()
            .map(|a| (a.name.as_str(), attrs(&a.attrs)))
            .collect()
    }
    let mut keys = BTreeMap::new();
    for n in &scene.nodes {
        let key = serde_json::to_string(&(
            &n.id,
            title_width(n).max(112.),
            crate::labels::node_title(n).lines().count(),
            &n.parent,
            &n.shape,
            n.container,
            attrs(&n.attrs),
            anns(&n.annotations),
            n.ports
                .values()
                .map(|p| {
                    (
                        &p.id,
                        crate::labels::text_width(&p.label),
                        p.label.chars().count(),
                        &p.constraint,
                    )
                })
                .collect::<Vec<_>>(),
        ))
        .unwrap();
        keys.insert(format!("n:{}", n.id), key);
    }
    for e in &scene.edges {
        let key = serde_json::to_string(&(
            &e.id,
            (&e.source.node, &e.source.port, &e.source.constraint),
            (&e.target.node, &e.target.port, &e.target.constraint),
            crate::labels::text_width(&crate::labels::connection_text(scene, e)),
            crate::labels::text_height(&crate::labels::connection_text(scene, e)),
            e.kind,
            attrs(&e.attrs),
            anns(&e.annotations),
        ))
        .unwrap();
        keys.insert(format!("e:{}", e.id), key);
    }
    let config = SceneGraph {
        zones: scene.zones.clone(),
        relations: scene.relations.clone(),
        layout_rules: scene.layout_rules.clone(),
        ..Default::default()
    };
    (keys, semantic_key(&config, o))
}
fn local_query(
    scene: &SceneGraph,
    o: &LayoutOptions,
    prior: &Layout,
    keys: &BTreeMap<String, String>,
) -> Option<Layout> {
    if prior.entity_keys.is_empty() {
        return None;
    }
    // Both maps are ordered: a merge detects changes in linear time without
    // doing two tree lookups per unaffected entity.
    let mut changed = Vec::new();
    let mut before = prior.entity_keys.iter().peekable();
    let mut after = keys.iter().peekable();
    loop {
        match (before.peek(), after.peek()) {
            (Some((a, av)), Some((b, bv))) => match a.cmp(b) {
                std::cmp::Ordering::Less => changed.push(before.next()?.0.as_str()),
                std::cmp::Ordering::Greater => changed.push(after.next()?.0.as_str()),
                std::cmp::Ordering::Equal => {
                    if av != bv {
                        changed.push(b.as_str());
                    }
                    before.next();
                    after.next();
                }
            },
            (Some(_), None) => {
                changed.extend(before.map(|(key, _)| key.as_str()));
                break;
            }
            (None, Some(_)) => {
                changed.extend(after.map(|(key, _)| key.as_str()));
                break;
            }
            (None, None) => break,
        }
    }
    if changed.is_empty() {
        return None;
    }
    let root_of = |id: &str| -> Option<String> {
        let mut n = scene.node(id)?;
        let mut root = None;
        while let Some(parent) = &n.parent {
            root = Some(parent.clone());
            n = scene.node(parent)?;
        }
        root
    };
    let mut root = None;
    for key in changed {
        let ids = if let Some(id) = key.strip_prefix("n:") {
            vec![id]
        } else {
            let e = scene.edge(key.strip_prefix("e:")?)?;
            vec![e.source.node.as_str(), e.target.node.as_str()]
        };
        for id in ids {
            let r = root_of(id)?;
            if root.as_ref().is_some_and(|old| old != &r) {
                return None;
            }
            root = Some(r);
        }
    }
    let root = root?;
    let members: BTreeSet<_> = scene
        .nodes
        .iter()
        .filter(|n| n.id == root || ancestors(scene, &n.id).contains(&root))
        .map(|n| n.id.clone())
        .collect();
    if scene
        .edges
        .iter()
        .any(|e| members.contains(&e.source.node) != members.contains(&e.target.node))
        || scene.relations.iter().any(|r| {
            r.members.iter().any(|id| members.contains(id))
                && r.members.iter().any(|id| !members.contains(id))
        })
    {
        return None;
    }
    let mut graph = crate::views::filter_exact(scene, &members);
    // Outside relational anchors need the enclosing query and cannot be solved in isolation.
    for n in &graph.nodes {
        for a in effective_annotations(scene, &n.id, &n.annotations, o.variant.as_deref())
            .iter()
            .filter(|a| a.name == "pin")
        {
            for axis in ["x", "y"] {
                if let Some(v) = a.attrs.get(axis)
                    && number(&v.text).is_none()
                    && !v.text.trim().starts_with(&root)
                {
                    return None;
                }
            }
        }
    }
    let bounds = *prior.nodes.get(&root)?;
    let index = graph.index(&root)?;
    graph.nodes[index].parent = None;
    graph.nodes[index].annotations.retain(|a| a.name != "pin");
    let make = |text: String| Value {
        text,
        ..Default::default()
    };
    graph.nodes[index].annotations.push(Annotation {
        name: "pin".into(),
        attrs: BTreeMap::from([
            ("x".into(), make(format!("{}g", bounds.x / o.grid))),
            ("y".into(), make(format!("{}g", bounds.y / o.grid))),
            (
                "lock".into(),
                make(format!(
                    "[w: {}g, h: {}g]",
                    bounds.w / o.grid,
                    bounds.h / o.grid
                )),
            ),
        ]),
        ..Default::default()
    });
    // A source layout rule overrides inline pins, so remove just the root's memory-only rule.
    graph.layout_rules.retain(|r| r.target != root);
    let internal_edges: BTreeSet<_> = graph.edges.iter().map(|e| e.id.clone()).collect();
    let owned = |key: &str| {
        if let Some(edge) = key
            .strip_suffix(":source")
            .or_else(|| key.strip_suffix(":target"))
        {
            internal_edges.contains(edge)
        } else {
            key.rsplit_once('#')
                .is_some_and(|(id, _)| members.contains(id))
        }
    };
    let partial_prior = Layout {
        nodes: prior
            .nodes
            .iter()
            .filter(|(id, _)| members.contains(*id))
            .map(|(id, r)| (id.clone(), *r))
            .collect(),
        ports: prior
            .ports
            .iter()
            .filter(|(key, _)| owned(key))
            .map(|(key, p)| (key.clone(), p.clone()))
            .collect(),
        routes: prior
            .routes
            .iter()
            .filter(|(id, _)| internal_edges.contains(*id))
            .map(|(id, r)| (id.clone(), r.clone()))
            .collect(),
        labels: prior
            .labels
            .iter()
            .filter(|p| internal_edges.contains(&p.feature) || owned(&p.feature))
            .cloned()
            .collect(),
        ..Default::default()
    };
    let local = layout(&graph, o, Some(&partial_prior)).ok()?;
    if local.nodes.get(&root) != Some(&bounds) {
        return None;
    }
    let mut out = Layout {
        nodes: prior.nodes.clone(),
        ports: prior.ports.clone(),
        routes: prior.routes.clone(),
        labels: prior.labels.clone(),
        zones: prior.zones.clone(),
        diagnostics: prior.diagnostics.clone(),
        ..Default::default()
    };
    out.solved_nodes = local.solved_nodes;
    let old_ids: BTreeSet<_> = prior
        .nodes
        .keys()
        .filter(|id| *id == &root || id.starts_with(&(root.clone() + ".")))
        .cloned()
        .collect();
    out.nodes.retain(|id, _| !old_ids.contains(id));
    out.nodes.extend(local.nodes);
    let internal_edges: BTreeSet<_> = scene
        .edges
        .iter()
        .filter(|e| members.contains(&e.source.node) && members.contains(&e.target.node))
        .map(|e| e.id.clone())
        .collect();
    let owned_port = |key: &str| {
        if let Some(edge) = key
            .strip_suffix(":source")
            .or_else(|| key.strip_suffix(":target"))
        {
            internal_edges.contains(edge)
        } else {
            key.rsplit_once('#')
                .is_some_and(|(id, _)| old_ids.contains(id))
        }
    };
    out.ports.retain(|key, _| !owned_port(key));
    out.ports.extend(local.ports);
    out.routes.retain(|id, _| !internal_edges.contains(id));
    out.routes.extend(local.routes);
    out.labels
        .retain(|label| !internal_edges.contains(&label.feature) && !owned_port(&label.feature));
    out.labels.extend(local.labels);
    out.reused_routes = out.routes.len().saturating_sub(internal_edges.len()) + local.reused_routes;
    out.stability = stability(scene, prior, &out);
    for zone in &scene.zones {
        let padding = value(&zone.attrs, "padding").unwrap_or(1.5) * o.grid;
        let points = zone
            .members
            .iter()
            .filter_map(|id| out.nodes.get(id))
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
        out.zones.insert(zone.id.clone(), convex_hull(points));
    }
    // Outside routes cannot enter the unchanged container boundary, so geometry validity composes.
    Some(out)
}

fn validate_model(scene: &SceneGraph, o: &LayoutOptions) -> Result<(), Error> {
    let weights = &o.weights;
    if [
        weights.pin,
        weights.crossing,
        weights.flip,
        weights.bend,
        weights.pill,
        weights.backward,
        weights.length,
        weights.centeredness,
        weights.source_order,
        weights.symmetry,
    ]
    .iter()
    .any(|w| !w.is_finite() || *w < 0.)
        || weights.pin == 0.
    {
        return Err(Error::Layout(
            "objective weights must be finite and nonnegative; pin weight must be positive".into(),
        ));
    }
    let mut ids = BTreeSet::new();
    for node in &scene.nodes {
        if !ids.insert(&node.id) {
            return Err(Error::Layout(format!("duplicate node ID {}", node.id)));
        }
        if let Some(parent) = &node.parent
            && (scene.node(parent).is_none() || parent == &node.id)
        {
            return Err(Error::Layout(format!("invalid parent of {}", node.id)));
        }
    }
    let check = |p: &PortConstraint| -> bool {
        match p {
            PortConstraint::FixedRatio { ratio, .. } => {
                ratio.is_finite() && *ratio > 0. && *ratio < 1.
            }
            PortConstraint::FixedPos { offset_grid, .. } => {
                offset_grid.is_finite() && *offset_grid >= 0.
            }
            _ => true,
        }
    };
    let mut edges = BTreeSet::new();
    for e in &scene.edges {
        if !edges.insert(&e.id) {
            return Err(Error::Layout(format!("duplicate edge ID {}", e.id)));
        }
        for ep in [&e.source, &e.target] {
            let node = scene
                .node(&ep.node)
                .ok_or_else(|| Error::Layout(format!("unknown endpoint {}", ep.node)))?;
            if !check(&ep.constraint)
                || ep
                    .port
                    .as_ref()
                    .is_some_and(|p| !node.ports.contains_key(p))
            {
                return Err(Error::Layout(format!(
                    "invalid endpoint port at {}",
                    ep.node
                )));
            }
        }
    }
    for node in &scene.nodes {
        if node.ports.values().any(|p| !check(&p.constraint))
            || ancestors(scene, &node.id).contains(&node.id)
        {
            return Err(Error::Layout(format!(
                "invalid ports or cyclic ownership at {}",
                node.id
            )));
        }
    }
    Ok(())
}

fn order_flat_ranks(scene: &SceneGraph, ranks: &[usize], rects: &mut [Rect], o: &LayoutOptions) {
    let explicit_side = |ep: &Endpoint| -> Option<Side> {
        ep.constraint.side().or_else(|| {
            let idx = scene.index(&ep.node)?;
            let port_id = ep.port.as_ref()?;
            scene.nodes[idx].ports.get(port_id)?.constraint.side()
        })
    };
    let mut degree = vec![0usize; scene.nodes.len()];
    for e in &scene.edges {
        if let Some(a) = scene.index(&e.source.node) {
            degree[a] += 1;
        }
        if let Some(b) = scene.index(&e.target.node) {
            degree[b] += 1;
        }
    }
    let mut satellites: Vec<(usize, usize, Side)> = Vec::new();
    let mut is_sat = vec![false; scene.nodes.len()];
    for (i, node) in scene.nodes.iter().enumerate() {
        if degree[i] != 1
            || effective_annotations(scene, &node.id, &node.annotations, o.variant.as_deref())
                .iter()
                .any(|a| a.name == "pin")
        {
            continue;
        }
        if let Some(edge) = scene
            .edges
            .iter()
            .find(|e| e.source.node == node.id || e.target.node == node.id)
        {
            let hub_ep = if edge.source.node == node.id {
                &edge.target
            } else {
                &edge.source
            };
            if let Some(hub) = scene.index(&hub_ep.node)
                && hub != i
                && degree[hub] >= 2
                && let Some(side @ (Side::N | Side::S)) = explicit_side(hub_ep)
            {
                satellites.push((i, hub, side));
                is_sat[i] = true;
            }
        }
    }
    let mut effective_ranks = ranks.to_vec();
    let mut by_rank: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, &r) in effective_ranks.iter().enumerate() {
        if !is_sat[i] {
            by_rank.entry(r).or_default().push(i);
        }
    }
    if !satellites.is_empty() {
        for i in 0..scene.nodes.len() {
            if is_sat[i] {
                continue;
            }
            let id = &scene.nodes[i].id;
            let has_in = scene.edges.iter().any(|e| &e.target.node == id);
            if has_in {
                continue;
            }
            let min_succ = scene
                .edges
                .iter()
                .filter(|e| &e.source.node == id)
                .filter_map(|e| scene.index(&e.target.node))
                .filter(|&t| !is_sat[t])
                .map(|t| effective_ranks[t])
                .min();
            if let Some(sr) = min_succ
                && sr > effective_ranks[i] + 1
            {
                let target_r = sr - 1;
                if by_rank.get(&target_r).map_or(0, Vec::len) < 4 {
                    let old_r = effective_ranks[i];
                    if let Some(v) = by_rank.get_mut(&old_r) {
                        v.retain(|&x| x != i);
                        if v.is_empty() {
                            by_rank.remove(&old_r);
                        }
                    }
                    effective_ranks[i] = target_r;
                    by_rank.entry(target_r).or_default().push(i);
                }
            }
        }
    }
    let ranks = &effective_ranks[..];
    if !by_rank.values().any(|m| m.len() > 1) && satellites.is_empty() {
        return;
    }
    let caption_h = scene
        .edges
        .iter()
        .map(|e| crate::labels::text_height(&crate::labels::connection_text(scene, e)))
        .fold(0., f64::max);
    let row_gap = (caption_h + 48.).max(64.);
    let has_explicit_ports = scene
        .nodes
        .iter()
        .any(|n| n.shape == "sql_table" || !n.ports.is_empty());
    let corridor = if has_explicit_ports {
        160.
    } else {
        o.margin.max(48.)
    };
    let mut x = 0.;
    let mut prev_r = 0usize;
    let mut prev_w = 0.;
    for (idx, (&r, members)) in by_rank.iter().enumerate() {
        let w = members.iter().map(|&i| rects[i].w).fold(0., f64::max);
        if idx == 0 {
            x = r as f64 * 240.;
        } else {
            x += ((r - prev_r) as f64 * 240.).max((prev_w + w) / 2. + 2. * o.stub + corridor);
        }
        for &i in members {
            rects[i].x = x;
        }
        prev_r = r;
        prev_w = w;
    }
    let stack = |members: &[usize], rects: &mut [Rect]| {
        if members.is_empty() {
            return;
        }
        let total: f64 = members
            .iter()
            .map(|&i| top_extra(&scene.nodes[i]) + rects[i].h)
            .sum::<f64>()
            + (members.len() - 1) as f64 * row_gap;
        let mut cursor = -total / 2.;
        for &i in members {
            let extra = top_extra(&scene.nodes[i]);
            rects[i].y = cursor + extra + rects[i].h / 2.;
            cursor += extra + rects[i].h + row_gap;
        }
    };
    for members in by_rank.values() {
        stack(members, rects);
    }
    let endpoint_y = |ep: &Endpoint, rects: &[Rect]| -> Option<(usize, f64)> {
        let idx = scene.index(&ep.node)?;
        let r = rects[idx];
        if let Some(port_id) = &ep.port
            && let Some(port) = scene.nodes[idx].ports.get(port_id)
        {
            match port.constraint {
                PortConstraint::FixedRatio {
                    side: Side::W | Side::E,
                    ratio,
                } => return Some((idx, r.top() + ratio * r.h)),
                PortConstraint::FixedPos {
                    side: Side::W | Side::E,
                    offset_grid,
                } => return Some((idx, r.top() + offset_grid * o.grid)),
                _ => {}
            }
        }
        Some((idx, r.y))
    };
    let rank_keys: Vec<usize> = by_rank.keys().copied().collect();
    for sweep in 0..4 {
        let keys: Vec<usize> = if sweep % 2 == 0 {
            rank_keys.clone()
        } else {
            rank_keys.iter().copied().rev().collect()
        };
        for r in keys {
            let members = by_rank.get_mut(&r).unwrap();
            if members.len() < 2 {
                continue;
            }
            let mut keyed: Vec<(f64, usize, usize)> = members
                .iter()
                .map(|&i| {
                    let id = &scene.nodes[i].id;
                    let mut ys = Vec::new();
                    for e in &scene.edges {
                        if &e.source.node == id {
                            if let Some((other, y)) = endpoint_y(&e.target, rects)
                                && !is_sat[other]
                                && ranks[other] != r
                                && (sweep > 0 || ranks[other] < r)
                            {
                                ys.push(y);
                            }
                        } else if &e.target.node == id
                            && let Some((other, y)) = endpoint_y(&e.source, rects)
                            && !is_sat[other]
                            && ranks[other] != r
                            && (sweep > 0 || ranks[other] < r)
                        {
                            ys.push(y);
                        }
                    }
                    let bary = if ys.is_empty() {
                        rects[i].y
                    } else {
                        ys.iter().sum::<f64>() / ys.len() as f64
                    };
                    (bary, i, i)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            *members = keyed.into_iter().map(|(_, _, i)| i).collect();
            stack(members, rects);
        }
    }
    for (hub_side_key, group) in {
        let mut grouped: BTreeMap<(usize, u8), Vec<usize>> = BTreeMap::new();
        for &(sat, hub, side) in &satellites {
            grouped
                .entry((hub, u8::from(side == Side::S)))
                .or_default()
                .push(sat);
        }
        grouped
    } {
        let (hub, is_south) = hub_side_key;
        let hub_r = rects[hub];
        let gap = (row_gap + 24.).max(88.);
        let total_w: f64 =
            group.iter().map(|&i| rects[i].w).sum::<f64>() + (group.len() - 1) as f64 * 32.;
        let mut cursor_x = hub_r.x - total_w / 2.;
        for &sat in &group {
            rects[sat].x = cursor_x + rects[sat].w / 2.;
            cursor_x += rects[sat].w + 32.;
            if is_south == 0 {
                rects[sat].y = hub_r.top() - top_extra(&scene.nodes[hub]) - gap - rects[sat].h / 2.;
            } else {
                rects[sat].y =
                    hub_r.bottom() + gap + top_extra(&scene.nodes[sat]) + rects[sat].h / 2.;
            }
        }
    }
}
