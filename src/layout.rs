//! Deterministic staged layout, compound geometry, port assignment and mental-map metrics.
use crate::{
    Error,
    model::*,
    solver::{self, Constraint, L1Term, Weights},
    topology::{FlowAxis, TopologyPlan},
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
pub(crate) fn container_header_rect_with_ports(
    node: &Node,
    r: Rect,
    ports: &BTreeMap<String, PortPlacement>,
) -> Option<Rect> {
    let base = container_header_rect(node, r)?;
    if base.w <= 0. {
        return Some(base);
    }
    let min_x = r.left() + 12.;
    let max_x = r.right() - 12.;
    if max_x - min_x < base.w {
        return Some(base);
    }
    let mut blocked: Vec<(f64, f64)> = ports
        .values()
        .filter(|p| {
            p.side == Side::N
                && (p.point.y - r.top()).abs() <= 12. + 1e-6
                && p.point.x >= r.left() - 1e-6
                && p.point.x <= r.right() + 1e-6
        })
        .map(|p| ((p.point.x - 12.).max(min_x), (p.point.x + 12.).min(max_x)))
        .filter(|(lo, hi)| hi > lo)
        .collect();
    if blocked.is_empty() {
        return Some(base);
    }
    blocked.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let mut cursor = min_x;
    for (lo, hi) in blocked {
        if cursor + base.w <= lo + 1e-6 {
            return Some(Rect::new(cursor + base.w / 2., base.y, base.w, base.h));
        }
        cursor = cursor.max(hi);
    }
    if cursor + base.w <= max_x + 1e-6 {
        return Some(Rect::new(cursor + base.w / 2., base.y, base.w, base.h));
    }
    Some(base)
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
fn default_explore_candidates() -> bool {
    true
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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
    #[serde(default = "default_explore_candidates")]
    pub explore_candidates: bool,
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
            explore_candidates: true,
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

#[derive(Debug, Clone, Copy)]
enum AxisVar {
    Leaf(usize),
    Walls { lo: usize, hi: usize },
}

impl AxisVar {
    fn center_terms(self) -> Vec<(usize, f64)> {
        match self {
            Self::Leaf(v) => vec![(v, 1.)],
            Self::Walls { lo, hi } => vec![(lo, 0.5), (hi, 0.5)],
        }
    }

    fn min_edge_terms(self, half: f64) -> (Vec<(usize, f64)>, f64) {
        match self {
            Self::Leaf(v) => (vec![(v, 1.)], -half),
            Self::Walls { lo, .. } => (vec![(lo, 1.)], 0.),
        }
    }

    fn max_edge_terms(self, half: f64) -> (Vec<(usize, f64)>, f64) {
        match self {
            Self::Leaf(v) => (vec![(v, 1.)], half),
            Self::Walls { hi, .. } => (vec![(hi, 1.)], 0.),
        }
    }

    fn anchor_terms(self, part: AnchorPart, half: f64) -> (Vec<(usize, f64)>, f64) {
        match part {
            AnchorPart::Center => (self.center_terms(), 0.),
            AnchorPart::MinWall => self.min_edge_terms(half),
            AnchorPart::MaxWall => self.max_edge_terms(half),
        }
    }
}

fn linear_ge_constraint(
    lhs_terms: &[(usize, f64)],
    lhs_const: f64,
    rhs_terms: &[(usize, f64)],
    rhs_const: f64,
    equality: bool,
) -> Option<Constraint> {
    let mut coeffs: Vec<(usize, f64)> = Vec::with_capacity(lhs_terms.len() + rhs_terms.len());
    for &(idx, c) in lhs_terms {
        if let Some(entry) = coeffs.iter_mut().find(|e| e.0 == idx) {
            entry.1 += c;
        } else {
            coeffs.push((idx, c));
        }
    }
    for &(idx, c) in rhs_terms {
        if let Some(entry) = coeffs.iter_mut().find(|e| e.0 == idx) {
            entry.1 -= c;
        } else {
            coeffs.push((idx, -c));
        }
    }
    coeffs.retain(|(_, c)| c.abs() > 1e-12);
    if coeffs.is_empty() {
        return None;
    }
    Some(Constraint {
        coefficients: coeffs,
        rhs: rhs_const - lhs_const,
        equality,
    })
}

struct PinLockInfo {
    w_locked: Vec<bool>,
    h_locked: Vec<bool>,
    locked_sizes: Vec<(Option<f64>, Option<f64>)>,
    has_pos_pin: bool,
}

fn parse_pin_locks(annotations: &[Vec<Annotation>], o: &LayoutOptions) -> Result<PinLockInfo, Error> {
    let n = annotations.len();
    let mut w_locked = vec![false; n];
    let mut h_locked = vec![false; n];
    let mut locked_sizes = vec![(None, None); n];
    let mut has_pos_pin = false;
    for (i, list) in annotations.iter().enumerate() {
        for a in list.iter().filter(|a| a.name == "pin") {
            if a.attrs.contains_key("grid")
                || a.attrs.contains_key("x")
                || a.attrs.contains_key("y")
            {
                has_pos_pin = true;
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
                            "w" => {
                                w_locked[i] = true;
                                locked_sizes[i].0 = Some(size);
                            }
                            "h" => {
                                h_locked[i] = true;
                                locked_sizes[i].1 = Some(size);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    Ok(PinLockInfo {
        w_locked,
        h_locked,
        locked_sizes,
        has_pos_pin,
    })
}

fn compute_base_sizes(
    scene: &SceneGraph,
    locks: &PinLockInfo,
    o: &LayoutOptions,
) -> Result<Vec<Point>, Error> {
    let n = scene.nodes.len();
    let mut sizes = Vec::with_capacity(n);
    for (i, node) in scene.nodes.iter().enumerate() {
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
        if node.container && node.shape != "sql_table" {
            h = h.max(header_height(node) + 24.);
        }
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
        h = if node.shape != "sql_table" && node.annotations.iter().any(|a| a.name == "sim") {
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
        // Only apply explicitly declared fixed-side port sizing before placement;
        // Free-side port growth runs after Stage 1A positions settle.
        for side in [Side::N, Side::E, Side::S, Side::W] {
            let horiz = matches!(side, Side::N | Side::S);
            if (horiz && locks.w_locked[i]) || (!horiz && locks.h_locked[i]) {
                continue;
            }
            let mut min = 0f64;
            let mut named_ratios: Vec<(f64, f64)> = Vec::new();
            let mut fixed_count = 0usize;
            for p in node.ports.values() {
                if p.constraint.side() == Some(side) {
                    fixed_count += 1;
                    match p.constraint {
                        PortConstraint::FixedPos { offset_grid, .. } => {
                            min = min.max(offset_grid * o.grid + o.corner);
                        }
                        PortConstraint::FixedRatio { ratio, .. } => {
                            if ratio <= 0. || ratio >= 1. {
                                return Err(Error::Layout(
                                    "ratio ports must lie strictly inside a side".into(),
                                ));
                            }
                            min = min.max(o.corner / ratio.min(1. - ratio));
                            if !p.label.trim().is_empty() {
                                let span = if horiz {
                                    crate::labels::text_width(&p.label) + 20.
                                } else {
                                    crate::labels::text_height(&p.label) + 20.
                                };
                                named_ratios.push((ratio, span));
                            }
                        }
                        _ => {}
                    }
                }
            }
            for e in &scene.edges {
                for (ep_node, ep_c) in [
                    (&e.source.node, &e.source.constraint),
                    (&e.target.node, &e.target.constraint),
                ] {
                    if ep_node == &node.id && ep_c.side() == Some(side) {
                        fixed_count += 1;
                        match *ep_c {
                            PortConstraint::FixedPos { offset_grid, .. } => {
                                min = min.max(offset_grid * o.grid + o.corner);
                            }
                            PortConstraint::FixedRatio { ratio, .. } => {
                                if ratio <= 0. || ratio >= 1. {
                                    return Err(Error::Layout(
                                        "ratio ports must lie strictly inside a side".into(),
                                    ));
                                }
                                min = min.max(o.corner / ratio.min(1. - ratio));
                            }
                            _ => {}
                        }
                    }
                }
            }
            if fixed_count > 0 {
                min = min.max((fixed_count.saturating_sub(1)) as f64 * o.port_spacing + 2. * o.corner);
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
            if horiz {
                w = w.max(min);
            } else {
                h = h.max(min);
            }
        }
        if let Some(lw) = locks.locked_sizes[i].0 {
            w = lw;
        }
        if let Some(lh) = locks.locked_sizes[i].1 {
            h = lh;
        }
        if !w.is_finite() || !h.is_finite() {
            return Err(Error::Layout("non-finite port perimeter dimensions".into()));
        }
        sizes.push(Point::new(w, h));
    }
    Ok(sizes)
}

fn candidate_compatible_with_relations(scene: &SceneGraph, plan: &TopologyPlan) -> bool {
    for rel in &scene.relations {
        let ids: Vec<usize> = rel
            .members
            .iter()
            .filter_map(|id| scene.index(id))
            .collect();
        if ids.len() < 2 {
            continue;
        }
        let first_fold = plan.fold_row[ids[0]];
        if ids.iter().any(|&id| plan.fold_row[id] != first_fold) {
            return false;
        }
        if plan.axis == FlowAxis::QuadrantCross {
            return false;
        }
        if rel.distribute {
            let first_rank = plan.ranks[ids[0]];
            let same_rank = ids.iter().all(|&id| plan.ranks[id] == first_rank);
            if rel.axis == Axis::Row && plan.axis == FlowAxis::TopToBottom && !same_rank {
                return false;
            }
            if rel.axis == Axis::Col && plan.axis == FlowAxis::LeftToRight && !same_rank {
                return false;
            }
        } else {
            let first_rank = plan.ranks[ids[0]];
            let same_rank = ids.iter().all(|&id| plan.ranks[id] == first_rank);
            let mut distinct_ranks: Vec<i32> = ids.iter().map(|&id| plan.ranks[id]).collect();
            distinct_ranks.sort_unstable();
            distinct_ranks.dedup();
            let all_distinct = distinct_ranks.len() == ids.len();
            match (rel.axis, plan.axis) {
                (Axis::Row, FlowAxis::TopToBottom) if !same_rank => return false,
                (Axis::Row, FlowAxis::LeftToRight) if !all_distinct => return false,
                (Axis::Col, FlowAxis::LeftToRight) if !same_rank => return false,
                (Axis::Col, FlowAxis::TopToBottom) if !all_distinct => return false,
                _ => {}
            }
        }
    }
    true
}

fn has_named_multiport(scene: &SceneGraph, node_idx: usize) -> bool {
    if scene.nodes[node_idx].shape == "sql_table" {
        return false;
    }
    let mut count = 0usize;
    for e in &scene.edges {
        if (scene.index(&e.source.node) == Some(node_idx) && e.source.port.is_some())
            || (scene.index(&e.target.node) == Some(node_idx) && e.target.port.is_some())
        {
            count += 1;
            if count >= 3 {
                return true;
            }
        }
    }
    false
}

fn named_port_corridor_width(
    scene: &SceneGraph,
    edge: &Edge,
    a: usize,
    b: usize,
    lanes: usize,
    o: &LayoutOptions,
) -> f64 {
    let port_w = [
        (a, edge.source.port.as_deref()),
        (b, edge.target.port.as_deref()),
    ]
    .into_iter()
    .filter_map(|(idx, p)| {
        (scene.nodes[idx].shape != "sql_table")
            .then_some(p)
            .flatten()
    })
    .map(|name| crate::labels::text_width(name) + 16.)
    .fold(0., f64::max);
    let text = crate::labels::connection_text(scene, edge);
    let edge_w = if text.trim().is_empty() {
        0.
    } else {
        crate::labels::text_width(&text) + 16.
    };
    let lanes_w = lanes.max(1) as f64 * o.port_spacing;
    (port_w + edge_w + lanes_w + 2. * o.stub + 2. * o.margin).min(280.)
}

fn has_labeled_parallel_bundle(scene: &SceneGraph) -> bool {
    let mut pair_counts: BTreeMap<(usize, usize), (usize, usize)> = BTreeMap::new();
    for e in &scene.edges {
        let (Some(a), Some(b)) = (scene.index(&e.source.node), scene.index(&e.target.node))
        else {
            continue;
        };
        if a == b {
            continue;
        }
        let key = (a.min(b), a.max(b));
        let entry = pair_counts.entry(key).or_insert((0, 0));
        entry.0 += 1;
        if !crate::labels::connection_text(scene, e).trim().is_empty() {
            entry.1 += 1;
        }
    }
    pair_counts
        .values()
        .any(|&(total, labeled)| total >= 2 && labeled >= 2)
}

fn parallel_bundle_corridor_width(
    scene: &SceneGraph,
    ra: Rect,
    rb: Rect,
    a: usize,
    b: usize,
    left_is_a: bool,
    o: &LayoutOptions,
) -> Option<f64> {
    if a == b {
        return None;
    }
    let mut pair_count = 0usize;
    let mut widths = Vec::new();
    for e in &scene.edges {
        let (Some(sa), Some(sb)) = (scene.index(&e.source.node), scene.index(&e.target.node))
        else {
            continue;
        };
        if !((sa == a && sb == b) || (sa == b && sb == a)) {
            continue;
        }
        pair_count += 1;
        let text = crate::labels::connection_text(scene, e);
        if !text.trim().is_empty() {
            widths.push(crate::labels::text_width(&text));
        }
    }
    if pair_count < 2 || widths.len() < 2 {
        return None;
    }
    widths.sort_by(|x, y| y.total_cmp(x));
    let top2 = widths[0] + widths[1];
    let (left_idx, rl, right_idx, rr) = if left_is_a {
        (a, ra, b, rb)
    } else {
        (b, rb, a, ra)
    };
    let face_inset = |idx: usize, r: Rect, side: Side| -> f64 {
        let corner = o.corner.min(r.h / 4.);
        let max_half = (r.h * 0.5 - corner).max(0.);
        let half_span = ((pair_count.saturating_sub(1)) as f64 * 0.5 * o.port_spacing).min(max_half);
        let mid = r.h * 0.5;
        let att_top = crate::shapes::attachment(
            &scene.nodes[idx].shape,
            r,
            side,
            (mid - half_span).clamp(0., r.h),
        );
        let att_bot = crate::shapes::attachment(
            &scene.nodes[idx].shape,
            r,
            side,
            (mid + half_span).clamp(0., r.h),
        );
        match side {
            Side::E => (r.right() - att_top.x).max(r.right() - att_bot.x).max(0.),
            Side::W => (att_top.x - r.left()).max(att_bot.x - r.left()).max(0.),
            _ => 0.,
        }
    };
    let shape_side_inset = face_inset(left_idx, rl, Side::E) + face_inset(right_idx, rr, Side::W);
    Some((top2 + 2. * o.stub + 2. * o.margin + 48. + shape_side_inset).min(320.))
}

fn guide_corridor_gaps(plan: &TopologyPlan, members: &[usize], step: f64) -> Vec<f64> {
    let mut gaps = vec![0.; members.len().saturating_sub(1)];
    if plan.edge_guides.is_empty() || members.len() < 2 {
        return gaps;
    }
    for k in 1..members.len() {
        let (u, v) = (members[k - 1], members[k]);
        let rank = plan.ranks[u];
        if plan.ranks[v] != rank {
            continue;
        }
        let lo = plan.order[u].min(plan.order[v]) as f64;
        let hi = plan.order[u].max(plan.order[v]) as f64;
        let count = plan
            .edge_guides
            .values()
            .filter(|pts| {
                pts.iter()
                    .any(|&(r, pos)| r == rank && pos > lo + 1e-6 && pos < hi - 1e-6)
            })
            .count();
        gaps[k - 1] = count as f64 * step;
    }
    gaps
}

#[allow(clippy::needless_range_loop)]
fn build_plan_rects(
    scene: &SceneGraph,
    base_sizes: &[Point],
    plan: &TopologyPlan,
    o: &LayoutOptions,
    spacing: f64,
) -> Vec<Rect> {
    let n = scene.nodes.len();
    let mut rects: Vec<Rect> = base_sizes
        .iter()
        .map(|s| Rect::new(0., 0., s.x, s.y))
        .collect();
    if n == 0 {
        return rects;
    }
    let min_gap = (2. * o.stub + 2. * o.margin).max(56.) * spacing;
    let cross_gap = 48. * spacing;
    match plan.axis {
        FlowAxis::QuadrantCross => {
            let mut cols: Vec<i32> = plan.ranks.clone();
            cols.sort_unstable();
            cols.dedup();
            let mut rows: Vec<i32> = plan.order.clone();
            rows.sort_unstable();
            rows.dedup();
            let mut col_w = vec![0f64; cols.len()];
            let mut row_h = vec![0f64; rows.len()];
            for i in 0..n {
                let ci = cols.binary_search(&plan.ranks[i]).unwrap();
                let ri = rows.binary_search(&plan.order[i]).unwrap();
                col_w[ci] = col_w[ci].max(rects[i].w);
                row_h[ri] = row_h[ri].max(rects[i].h + top_extra(&scene.nodes[i]));
            }
            let gap = min_gap.max(72. * spacing);
            let mut col_x = vec![0f64; cols.len()];
            let mut cur_x = 0.;
            for c in 0..cols.len() {
                col_x[c] = cur_x + col_w[c] * 0.5;
                cur_x += col_w[c] + gap;
            }
            let mid_x = if let (Some(&f), Some(&l)) = (col_x.first(), col_x.last()) {
                (f + l) * 0.5
            } else {
                0.
            };
            for x in &mut col_x {
                *x -= mid_x;
            }
            let mut row_y = vec![0f64; rows.len()];
            let mut cur_y = 0.;
            for r in 0..rows.len() {
                row_y[r] = cur_y + row_h[r] * 0.5;
                cur_y += row_h[r] + gap;
            }
            let mid_y = if let (Some(&f), Some(&l)) = (row_y.first(), row_y.last()) {
                (f + l) * 0.5
            } else {
                0.
            };
            for y in &mut row_y {
                *y -= mid_y;
            }
            let mut cell_counts: BTreeMap<(usize, usize), usize> = BTreeMap::new();
            for i in 0..n {
                let ci = cols.binary_search(&plan.ranks[i]).unwrap();
                let ri = rows.binary_search(&plan.order[i]).unwrap();
                let slot = cell_counts.entry((ci, ri)).or_insert(0);
                rects[i].x = col_x[ci];
                rects[i].y = row_y[ri] + *slot as f64 * (rects[i].h + cross_gap);
                *slot += 1;
            }
        }
        FlowAxis::LeftToRight => {
            // Normalize fold columns: if higher fold_rows already share rank coordinates,
            // treat plan.ranks directly as visual X column; otherwise snake-fold disjoint rows.
            let mut row_bounds: BTreeMap<usize, (i32, i32)> = BTreeMap::new();
            for i in 0..n {
                let b = row_bounds
                    .entry(plan.fold_row[i])
                    .or_insert((i32::MAX, i32::MIN));
                b.0 = b.0.min(plan.ranks[i]);
                b.1 = b.1.max(plan.ranks[i]);
            }
            let overlapping_cols = row_bounds.len() <= 1 || {
                let rows: Vec<_> = row_bounds.values().copied().collect();
                rows.windows(2).any(|w| w[1].0 <= w[0].1)
            };
            let mut col_idx = vec![0i32; n];
            if overlapping_cols {
                col_idx.copy_from_slice(&plan.ranks);
            } else {
                let mut by_fold: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
                for i in 0..n {
                    by_fold.entry(plan.fold_row[i]).or_default().push(i);
                }
                for (f, members) in by_fold {
                    let mut ur: Vec<i32> = members.iter().map(|&i| plan.ranks[i]).collect();
                    ur.sort_unstable();
                    ur.dedup();
                    let k = ur.len() as i32;
                    for i in members {
                        let p = ur.binary_search(&plan.ranks[i]).unwrap() as i32;
                        col_idx[i] = if f % 2 == 1 { k - 1 - p } else { p };
                    }
                }
            }
            let mut cols: Vec<i32> = col_idx.clone();
            cols.sort_unstable();
            cols.dedup();
            let mut col_w = vec![0f64; cols.len()];
            for i in 0..n {
                let ci = cols.binary_search(&col_idx[i]).unwrap();
                col_w[ci] = col_w[ci].max(rects[i].w);
            }
            let mut folds: Vec<usize> = plan.fold_row.clone();
            folds.sort_unstable();
            folds.dedup();
            let single_fold = folds.len() <= 1;
            let row_cross_gap = if single_fold {
                cross_gap.max(56. * spacing)
            } else {
                cross_gap
            };
            let named_port_corridors = (0..n).any(|i| has_named_multiport(scene, i));
            let has_labeled_parallel = has_labeled_parallel_bundle(scene);
            let is_default_lr = single_fold
                && plan.search_states == 0
                && plan.side_hints.is_empty()
                && plan.edge_guides.is_empty()
                && !named_port_corridors
                && !has_labeled_parallel;
            if is_default_lr {
                let mut rows = BTreeMap::new();
                for i in 0..n {
                    let row = rows.entry(plan.ranks[i]).or_insert(0);
                    rects[i].x = plan.ranks[i] as f64 * 240.;
                    rects[i].y = *row as f64 * 112.;
                    *row += 1;
                }
                return rects;
            }
            let mut gap_after = vec![min_gap.max(72. * spacing); cols.len().saturating_sub(1)];
            if !single_fold || spacing > 1. || named_port_corridors || has_labeled_parallel {
                let mut corridor_edge_counts = vec![0usize; cols.len().saturating_sub(1)];
                for e in &scene.edges {
                    let (Some(a), Some(b)) =
                        (scene.index(&e.source.node), scene.index(&e.target.node))
                    else {
                        continue;
                    };
                    if plan.fold_row[a] != plan.fold_row[b] || col_idx[a] == col_idx[b] {
                        continue;
                    }
                    let ca = cols.binary_search(&col_idx[a]).unwrap();
                    let cb = cols.binary_search(&col_idx[b]).unwrap();
                    let (lo, hi) = (ca.min(cb), ca.max(cb));
                    for c in lo..hi {
                        corridor_edge_counts[c] += 1;
                    }
                }
                for e in &scene.edges {
                    let (Some(a), Some(b)) =
                        (scene.index(&e.source.node), scene.index(&e.target.node))
                    else {
                        continue;
                    };
                    if plan.fold_row[a] != plan.fold_row[b] || col_idx[a] == col_idx[b] {
                        continue;
                    }
                    let ca = cols.binary_search(&col_idx[a]).unwrap();
                    let cb = cols.binary_search(&col_idx[b]).unwrap();
                    let (lo, hi) = (ca.min(cb), ca.max(cb));
                    let span = (hi - lo) as f64;
                    let text = crate::labels::connection_text(scene, e);
                    if !text.trim().is_empty() {
                        let lw = crate::labels::text_width(&text) + 2. * o.stub + 2. * o.margin;
                        let per_hop = (lw.min(180.) / span).max(min_gap);
                        for g in &mut gap_after[lo..hi] {
                            *g = g.max(per_hop * spacing);
                        }
                    }
                    if named_port_corridors
                        && (has_named_multiport(scene, a) || has_named_multiport(scene, b))
                    {
                        let lanes = corridor_edge_counts[lo..hi]
                            .iter()
                            .copied()
                            .max()
                            .unwrap_or(1);
                        let corridor_w = named_port_corridor_width(scene, e, a, b, lanes, o) / span;
                        for g in &mut gap_after[lo..hi] {
                            *g = g.max(corridor_w * spacing);
                        }
                    }
                    if let Some(par_w) = parallel_bundle_corridor_width(
                        scene,
                        rects[a],
                        rects[b],
                        a,
                        b,
                        ca < cb,
                        o,
                    ) {
                        let per_hop = (par_w / span).max(min_gap);
                        for g in &mut gap_after[lo..hi] {
                            *g = g.max(per_hop * spacing);
                        }
                    }
                }
            }
            let mut col_x = vec![0f64; cols.len()];
            for c in 1..cols.len() {
                let min_step = (col_w[c - 1] + col_w[c]) * 0.5 + gap_after[c - 1];
                let step = if single_fold {
                    (240. * spacing).max(min_step)
                } else {
                    min_step
                };
                col_x[c] = col_x[c - 1] + step;
            }
            let mut by_fold_col: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
            for i in 0..n {
                let ci = cols.binary_search(&col_idx[i]).unwrap();
                by_fold_col.entry((plan.fold_row[i], ci)).or_default().push(i);
            }
            let mut fold_cursor_y = 0f64;
            for (fi, &f) in folds.iter().enumerate() {
                let mut col_max_half_up = 0f64;
                let mut col_max_half_down = 0f64;
                let mut fold_rel_y = Vec::new();
                for ((ff, _), mut members) in by_fold_col
                    .iter()
                    .filter(|((ff, _), _)| *ff == f)
                    .map(|(k, v)| (*k, v.clone()))
                {
                    let _ = ff;
                    members.sort_by_key(|&i| (plan.order[i], i));
                    let extra_gaps = guide_corridor_gaps(plan, &members, 24. * spacing);
                    let total: f64 = members
                        .iter()
                        .map(|&i| top_extra(&scene.nodes[i]) + rects[i].h)
                        .sum::<f64>()
                        + (members.len().saturating_sub(1)) as f64 * row_cross_gap
                        + extra_gaps.iter().sum::<f64>();
                    let mut cursor = -total * 0.5;
                    for (idx, i) in members.into_iter().enumerate() {
                        let extra = top_extra(&scene.nodes[i]);
                        let y = cursor + extra + rects[i].h * 0.5;
                        col_max_half_up = col_max_half_up.max(-(y - extra - rects[i].h * 0.5));
                        col_max_half_down = col_max_half_down.max(y + rects[i].h * 0.5);
                        fold_rel_y.push((i, y));
                        let guide_pad = extra_gaps.get(idx).copied().unwrap_or(0.);
                        cursor += extra + rects[i].h + row_cross_gap + guide_pad;
                    }
                }
                if fi > 0 {
                    let prev_f = folds[fi - 1];
                    let mut cross_pair_counts: BTreeMap<(usize, usize), usize> = BTreeMap::new();
                    let mut has_cross_fold_caption = false;
                    for e in &scene.edges {
                        let (Some(a), Some(b)) =
                            (scene.index(&e.source.node), scene.index(&e.target.node))
                        else {
                            continue;
                        };
                        let crosses = (plan.fold_row[a] == prev_f && plan.fold_row[b] == f)
                            || (plan.fold_row[b] == prev_f && plan.fold_row[a] == f);
                        if crosses {
                            *cross_pair_counts.entry((a.min(b), a.max(b))).or_default() += 1;
                            if !crate::labels::connection_text(scene, e).trim().is_empty() {
                                has_cross_fold_caption = true;
                            }
                        }
                    }
                    let has_cross_fold_parallel =
                        cross_pair_counts.values().any(|&count| count >= 2);
                    let fold_gap = if has_cross_fold_parallel {
                        let extra_caption_gap = if has_cross_fold_caption { 28. } else { 0. };
                        (2. * o.stub + 2. * o.margin + 48.).max(96.) + extra_caption_gap
                    } else {
                        2. * o.stub + 2. * o.margin + 32.
                    };
                    fold_cursor_y += col_max_half_up + fold_gap * spacing;
                }
                for (i, ry) in fold_rel_y {
                    let ci = cols.binary_search(&col_idx[i]).unwrap();
                    rects[i].x = col_x[ci];
                    rects[i].y = fold_cursor_y + ry;
                }
                fold_cursor_y += col_max_half_down;
            }
        }
        FlowAxis::TopToBottom => {
            let mut rows: Vec<i32> = plan.ranks.clone();
            rows.sort_unstable();
            rows.dedup();
            let mut row_h = vec![0f64; rows.len()];
            for i in 0..n {
                let ri = rows.binary_search(&plan.ranks[i]).unwrap();
                row_h[ri] = row_h[ri].max(rects[i].h + top_extra(&scene.nodes[i]));
            }
            let mut gap_after = vec![min_gap.max(64. * spacing); rows.len().saturating_sub(1)];
            for e in &scene.edges {
                let (Some(a), Some(b)) = (scene.index(&e.source.node), scene.index(&e.target.node))
                else {
                    continue;
                };
                if plan.ranks[a] == plan.ranks[b] {
                    continue;
                }
                let ra = rows.binary_search(&plan.ranks[a]).unwrap();
                let rb = rows.binary_search(&plan.ranks[b]).unwrap();
                let (lo, hi) = (ra.min(rb), ra.max(rb));
                let text = crate::labels::connection_text(scene, e);
                if !text.trim().is_empty() {
                    let lh = (crate::labels::text_height(&text) + 2. * o.stub - 4.)
                        .max(2. * o.stub + 2. * o.margin + 24.);
                    let per_hop = lh.max(min_gap) * spacing;
                    for g in &mut gap_after[lo..hi] {
                        *g = g.max(per_hop);
                    }
                }
            }
            let mut row_y = vec![0f64; rows.len()];
            let mut cur_y = 0.;
            for r in 0..rows.len() {
                row_y[r] = cur_y + row_h[r] * 0.5;
                if r < gap_after.len() {
                    cur_y += row_h[r] + gap_after[r];
                }
            }
            let mut by_row: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
            for i in 0..n {
                let ri = rows.binary_search(&plan.ranks[i]).unwrap();
                by_row.entry(ri).or_default().push(i);
            }
            for (ri, mut members) in by_row {
                members.sort_by_key(|&i| (plan.order[i], i));
                let x_gap = 48. * spacing;
                let extra_gaps = guide_corridor_gaps(plan, &members, 24. * spacing);
                let total: f64 = members.iter().map(|&i| rects[i].w).sum::<f64>()
                    + (members.len().saturating_sub(1)) as f64 * x_gap
                    + extra_gaps.iter().sum::<f64>();
                let mut cursor = -total * 0.5;
                for (idx, i) in members.into_iter().enumerate() {
                    rects[i].x = cursor + rects[i].w * 0.5;
                    rects[i].y = row_y[ri] + top_extra(&scene.nodes[i]) * 0.5;
                    let guide_pad = extra_gaps.get(idx).copied().unwrap_or(0.);
                    cursor += rects[i].w + x_gap + guide_pad;
                }
            }
        }
    }
    rects
}

#[allow(clippy::too_many_arguments)]
fn seg_query_range(
    seg: &[f64],
    cap: usize,
    node: usize,
    nl: usize,
    nr: usize,
    ql: usize,
    qr: usize,
    min_right: f64,
    out: &mut Vec<usize>,
) {
    if ql >= nr || qr <= nl || seg[node] < min_right - 1e-6 {
        return;
    }
    if node >= cap {
        let idx = node - cap;
        if idx >= ql && idx < qr {
            out.push(idx);
        }
        return;
    }
    let mid = (nl + nr) / 2;
    seg_query_range(seg, cap, 2 * node, nl, mid, ql, qr, min_right, out);
    seg_query_range(seg, cap, 2 * node + 1, mid, nr, ql, qr, min_right, out);
}

fn seg_update(seg: &mut [f64], cap: usize, pos: usize, val: f64) {
    let mut i = cap + pos;
    seg[i] = val;
    while i > 1 {
        i >>= 1;
        seg[i] = seg[2 * i].max(seg[2 * i + 1]);
    }
}

fn scanline_overlapping_pairs(rects: &[Rect], nodes: &[usize]) -> Vec<(usize, usize)> {
    let m = nodes.len();
    if m < 2 {
        return Vec::new();
    }
    let mut x_order: Vec<usize> = nodes.to_vec();
    x_order.sort_by(|&a, &b| {
        rects[a]
            .left()
            .total_cmp(&rects[b].left())
            .then(a.cmp(&b))
    });
    let eff_left: Vec<f64> = x_order.iter().map(|&i| rects[i].left()).collect();
    let eff_right: Vec<f64> = x_order.iter().map(|&i| rects[i].right()).collect();
    #[derive(Clone, Copy)]
    struct SweepEvent {
        y: f64,
        is_open: bool,
        rank: usize,
    }
    let mut events = Vec::with_capacity(2 * m);
    for (rank, &i) in x_order.iter().enumerate() {
        events.push(SweepEvent {
            y: rects[i].top(),
            is_open: true,
            rank,
        });
        events.push(SweepEvent {
            y: rects[i].bottom(),
            is_open: false,
            rank,
        });
    }
    events.sort_by(|a, b| {
        a.y.total_cmp(&b.y)
            .then_with(|| a.is_open.cmp(&b.is_open))
            .then_with(|| a.rank.cmp(&b.rank))
    });
    let cap = m.next_power_of_two();
    let mut seg = vec![f64::NEG_INFINITY; 2 * cap];
    let mut hits = Vec::new();
    let mut pairs = Vec::new();
    for ev in events {
        if ev.is_open {
            hits.clear();
            let l = eff_left[ev.rank];
            let r = eff_right[ev.rank];
            if ev.rank > 0 {
                seg_query_range(&seg, cap, 1, 0, cap, 0, ev.rank, l, &mut hits);
            }
            let end_rank = eff_left.partition_point(|&v| v < r - 1e-6);
            if ev.rank + 1 < end_rank {
                seg_query_range(&seg, cap, 1, 0, cap, ev.rank + 1, end_rank, l, &mut hits);
            }
            let a = x_order[ev.rank];
            for &other_rank in &hits {
                let b = x_order[other_rank];
                if rects[a].intersects(rects[b]) {
                    pairs.push((a.min(b), a.max(b)));
                }
            }
            seg_update(&mut seg, cap, ev.rank, r);
        } else {
            seg_update(&mut seg, cap, ev.rank, f64::NEG_INFINITY);
        }
    }
    pairs.sort_unstable();
    pairs.dedup();
    pairs
}

struct Stage1bEnv<'a> {
    scene: &'a SceneGraph,
    o: &'a LayoutOptions,
    spacing: f64,
    requests: &'a [PortRequest],
    plan_hints: &'a [Option<(Side, Side)>],
    pins: &'a [(Option<f64>, Option<f64>)],
    relatives: &'a [(Option<RelativePin>, Option<RelativePin>)],
    w_locked: &'a [bool],
    h_locked: &'a [bool],
    previous: Option<&'a Layout>,
    stub_sides: &'a [[bool; 4]],
    ancestry: &'a [BTreeSet<String>],
    postorder: &'a [usize],
}

fn solve_incremental_partitioned(
    desired: &[f64],
    weights: &[f64],
    constraints: &[Constraint],
    prev_vals: &[Option<f64>],
    is_elastic_wall: &[bool],
    flip_penalty: f64,
) -> Result<Vec<f64>, Error> {
    let n = desired.len();
    if n == 0 {
        return Ok(Vec::new());
    }
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    fn unite(p: &mut [usize], a: usize, b: usize) {
        let ra = find(p, a);
        let rb = find(p, b);
        if ra != rb {
            p[rb] = ra;
        }
    }
    for c in constraints {
        if let Some(&(first, _)) = c.coefficients.first() {
            for &(other, _) in &c.coefficients[1..] {
                unite(&mut parent, first, other);
            }
        }
    }
    let mut comp_vars: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        comp_vars.entry(r).or_default().push(i);
    }
    let mut comp_cs: BTreeMap<usize, Vec<&Constraint>> = BTreeMap::new();
    for c in constraints {
        if let Some(&(first, _)) = c.coefficients.first() {
            let r = find(&mut parent, first);
            comp_cs.entry(r).or_default().push(c);
        }
    }
    let mut out = vec![0.; n];
    for (root, vars) in comp_vars {
        let cs = comp_cs.get(&root).map(Vec::as_slice).unwrap_or(&[]);
        let unchanged_vars = vars.iter().all(|&v| {
            prev_vals[v].is_some_and(|pv| (desired[v] - pv).abs() < 1e-9)
        });
        let all_feasible = unchanged_vars
            && cs.iter().all(|c| {
                let lhs: f64 = c
                    .coefficients
                    .iter()
                    .map(|&(v, a)| a * prev_vals[v].unwrap())
                    .sum();
                if c.equality {
                    (lhs - c.rhs).abs() <= 1e-6
                } else {
                    lhs >= c.rhs - 1e-6
                }
            });
        if all_feasible {
            for &v in &vars {
                out[v] = prev_vals[v].unwrap();
            }
            continue;
        }
        let mut local_idx = vec![usize::MAX; n];
        for (k, &v) in vars.iter().enumerate() {
            local_idx[v] = k;
        }
        let comp_d: Vec<f64> = vars.iter().map(|&v| desired[v]).collect();
        let comp_w: Vec<f64> = vars.iter().map(|&v| weights[v]).collect();
        let comp_prev: Vec<Option<f64>> = vars
            .iter()
            .map(|&v| {
                if is_elastic_wall[v] {
                    None
                } else {
                    prev_vals[v]
                }
            })
            .collect();
        let local_cs: Vec<Constraint> = cs
            .iter()
            .map(|c| Constraint {
                coefficients: c
                    .coefficients
                    .iter()
                    .map(|&(v, a)| (local_idx[v], a))
                    .collect(),
                rhs: c.rhs,
                equality: c.equality,
            })
            .collect();
        let solved = solver::project_stable(&comp_d, &comp_w, &local_cs, &comp_prev, flip_penalty)
            .or_else(|_| solver::project_stable(&comp_d, &comp_w, &local_cs, &comp_prev, 0.))
            .or_else(|_| solver::project(&comp_d, &comp_w, &local_cs))?;
        for (k, &v) in vars.iter().enumerate() {
            out[v] = solved[k];
        }
    }
    Ok(out)
}

fn endpoint_axis_expr(
    env: &Stage1bEnv,
    node_idx: usize,
    side: Side,
    spec: &PortConstraint,
    var: AxisVar,
    rects: &[Rect],
    is_x: bool,
) -> (Vec<(usize, f64)>, f64) {
    let r = rects[node_idx];
    let dim = if is_x { r.w } else { r.h };
    let primary = (is_x && matches!(side, Side::E | Side::W))
        || (!is_x && matches!(side, Side::N | Side::S));
    if primary {
        let is_max = (is_x && side == Side::E) || (!is_x && side == Side::S);
        return if is_max {
            var.max_edge_terms(dim * 0.5)
        } else {
            var.min_edge_terms(dim * 0.5)
        };
    }
    match *spec {
        PortConstraint::FixedPos { side: cs, offset_grid } if cs == side => {
            let offset = offset_grid * env.o.grid;
            match var {
                AxisVar::Leaf(v) => (vec![(v, 1.)], -dim * 0.5 + offset),
                AxisVar::Walls { lo, .. } => (vec![(lo, 1.)], offset),
            }
        }
        PortConstraint::FixedRatio { side: cs, ratio } if cs == side => match var {
            AxisVar::Leaf(v) => (vec![(v, 1.)], (ratio - 0.5) * dim),
            AxisVar::Walls { lo, hi } => (vec![(lo, 1. - ratio), (hi, ratio)], 0.),
        },
        _ => (var.center_terms(), 0.),
    }
}

#[allow(clippy::needless_range_loop)]
fn solve_stage1b(
    env: &Stage1bEnv,
    rects: &mut [Rect],
    min_spans: &[(f64, f64)],
    readable_routes: bool,
) -> Result<(), Error> {
    let n = env.scene.nodes.len();
    if n == 0 {
        return Ok(());
    }
    let mut x_vars = Vec::with_capacity(n);
    let mut y_vars = Vec::with_capacity(n);
    let mut nx = 0usize;
    let mut ny = 0usize;
    for i in 0..n {
        if env.scene.nodes[i].container {
            x_vars.push(AxisVar::Walls { lo: nx, hi: nx + 1 });
            y_vars.push(AxisVar::Walls { lo: ny, hi: ny + 1 });
            nx += 2;
            ny += 2;
        } else {
            x_vars.push(AxisVar::Leaf(nx));
            y_vars.push(AxisVar::Leaf(ny));
            nx += 1;
            ny += 1;
        }
    }

    let mut desired_x = vec![0.; nx];
    let mut desired_y = vec![0.; ny];
    let mut weights_x = vec![0.; nx];
    let mut weights_y = vec![0.; ny];
    let mut linear_x = vec![0.; nx];
    let mut linear_y = vec![0.; ny];
    let mut prev_x = vec![None; nx];
    let mut prev_y = vec![None; ny];
    let mut elastic_x = vec![false; nx];
    let mut elastic_y = vec![false; ny];

    let lambda_wall = env.o.weights.length.max(1.5);
    let mut base_w = vec![0.; n];
    for i in 0..n {
        let deg = env
            .scene
            .edges
            .iter()
            .filter(|e| {
                e.source.node == env.scene.nodes[i].id || e.target.node == env.scene.nodes[i].id
            })
            .count() as f64;
        base_w[i] = if env
            .previous
            .is_some_and(|p| p.nodes.contains_key(&env.scene.nodes[i].id))
        {
            deg.max(1.) * 20.
        } else {
            env.o.weights.source_order.max(0.01)
        };
    }

    for i in 0..n {
        let prior_r = env.previous.and_then(|p| p.nodes.get(&env.scene.nodes[i].id));
        let abs_px = env.pins[i].0.is_some() && env.relatives[i].0.is_none();
        let abs_py = env.pins[i].1.is_some() && env.relatives[i].1.is_none();
        match x_vars[i] {
            AxisVar::Leaf(vx) => {
                desired_x[vx] = if abs_px {
                    env.pins[i].0.unwrap()
                } else if let Some(old) = prior_r {
                    old.x
                } else {
                    rects[i].x
                };
                weights_x[vx] = if abs_px { env.o.weights.pin } else { base_w[i] };
                prev_x[vx] = prior_r.map(|r| r.x);
            }
            AxisVar::Walls { lo, hi } => {
                let (cx, cw) = if let Some(old) = prior_r {
                    (
                        env.pins[i].0.unwrap_or(old.x),
                        if env.w_locked[i] { rects[i].w } else { old.w },
                    )
                } else {
                    (env.pins[i].0.unwrap_or(rects[i].x), rects[i].w)
                };
                desired_x[lo] = cx - cw * 0.5;
                desired_x[hi] = cx + cw * 0.5;
                prev_x[lo] = prior_r.map(|r| r.left());
                prev_x[hi] = prior_r.map(|r| r.right());
                if env.w_locked[i] && abs_px {
                    weights_x[lo] = env.o.weights.pin;
                    weights_x[hi] = env.o.weights.pin;
                } else if env.w_locked[i] {
                    weights_x[lo] = base_w[i] * 0.5;
                    weights_x[hi] = base_w[i] * 0.5;
                } else {
                    elastic_x[lo] = true;
                    elastic_x[hi] = true;
                    if env.previous.is_some() {
                        weights_x[lo] = 0.5;
                        weights_x[hi] = 0.5;
                    } else {
                        weights_x[lo] = 0.05;
                        weights_x[hi] = 0.05;
                        linear_x[lo] = -lambda_wall;
                        linear_x[hi] = lambda_wall;
                    }
                }
            }
        }
        match y_vars[i] {
            AxisVar::Leaf(vy) => {
                desired_y[vy] = if abs_py {
                    env.pins[i].1.unwrap()
                } else if let Some(old) = prior_r {
                    old.y
                } else {
                    rects[i].y
                };
                weights_y[vy] = if abs_py { env.o.weights.pin } else { base_w[i] };
                prev_y[vy] = prior_r.map(|r| r.y);
            }
            AxisVar::Walls { lo, hi } => {
                let (cy, ch) = if let Some(old) = prior_r {
                    (
                        env.pins[i].1.unwrap_or(old.y),
                        if env.h_locked[i] { rects[i].h } else { old.h },
                    )
                } else {
                    (env.pins[i].1.unwrap_or(rects[i].y), rects[i].h)
                };
                desired_y[lo] = cy - ch * 0.5;
                desired_y[hi] = cy + ch * 0.5;
                prev_y[lo] = prior_r.map(|r| r.top());
                prev_y[hi] = prior_r.map(|r| r.bottom());
                if env.h_locked[i] && abs_py {
                    weights_y[lo] = env.o.weights.pin;
                    weights_y[hi] = env.o.weights.pin;
                } else if env.h_locked[i] {
                    weights_y[lo] = base_w[i] * 0.5;
                    weights_y[hi] = base_w[i] * 0.5;
                } else {
                    elastic_y[lo] = true;
                    elastic_y[hi] = true;
                    if env.previous.is_some() {
                        weights_y[lo] = 0.5;
                        weights_y[hi] = 0.5;
                    } else {
                        weights_y[lo] = 0.05;
                        weights_y[hi] = 0.05;
                        linear_y[lo] = -lambda_wall;
                        linear_y[hi] = lambda_wall;
                    }
                }
            }
        }
    }

    if let Some(prior) = env.previous {
        let mut comp_parent: Vec<usize> = (0..n).collect();
        fn find_c(p: &mut [usize], mut x: usize) -> usize {
            while p[x] != x {
                p[x] = p[p[x]];
                x = p[x];
            }
            x
        }
        fn unite_c(p: &mut [usize], a: usize, b: usize) {
            let ra = find_c(p, a);
            let rb = find_c(p, b);
            if ra != rb {
                p[rb] = ra;
            }
        }
        for e in &env.scene.edges {
            if let (Some(a), Some(b)) = (
                env.scene.index(&e.source.node),
                env.scene.index(&e.target.node),
            ) {
                unite_c(&mut comp_parent, a, b);
            }
        }
        for (i, node) in env.scene.nodes.iter().enumerate() {
            if let Some(p) = node.parent.as_deref().and_then(|id| env.scene.index(id)) {
                unite_c(&mut comp_parent, i, p);
            }
        }
        for rel in &env.scene.relations {
            let ids: Vec<usize> = rel
                .members
                .iter()
                .filter_map(|id| env.scene.index(id))
                .collect();
            for pair in ids.windows(2) {
                unite_c(&mut comp_parent, pair[0], pair[1]);
            }
        }
        for i in 0..n {
            if let Some(rx) = env.relatives[i].0 {
                unite_c(&mut comp_parent, i, rx.target);
            }
            if let Some(ry) = env.relatives[i].1 {
                unite_c(&mut comp_parent, i, ry.target);
            }
        }
        for (a, b) in delaunay_edges(env.scene, prior) {
            if find_c(&mut comp_parent, a) != find_c(&mut comp_parent, b) {
                continue;
            }
            let pa = prior.nodes[&env.scene.nodes[a].id];
            let pb = prior.nodes[&env.scene.nodes[b].id];
            if env.pins[a].0.is_none()
                && env.relatives[a].0.is_none()
                && let AxisVar::Leaf(vx) = x_vars[a]
            {
                desired_x[vx] += (rects[b].x + (pa.x - pb.x) - desired_x[vx]) * 0.05;
            }
            if env.pins[a].1.is_none()
                && env.relatives[a].1.is_none()
                && let AxisVar::Leaf(vy) = y_vars[a]
            {
                desired_y[vy] += (rects[b].y + (pa.y - pb.y) - desired_y[vy]) * 0.05;
            }
        }
    }

    let mut base_cx = Vec::new();
    let mut base_cy = Vec::new();
    for i in 0..n {
        if let AxisVar::Walls { lo, hi } = x_vars[i] {
            if env.w_locked[i] {
                base_cx.push(Constraint {
                    coefficients: vec![(lo, -1.), (hi, 1.)],
                    rhs: rects[i].w,
                    equality: true,
                });
            } else {
                base_cx.push(Constraint::separation(lo, hi, min_spans[i].0));
            }
        }
        if let AxisVar::Walls { lo, hi } = y_vars[i] {
            if env.h_locked[i] {
                base_cy.push(Constraint {
                    coefficients: vec![(lo, -1.), (hi, 1.)],
                    rhs: rects[i].h,
                    equality: true,
                });
            } else {
                base_cy.push(Constraint::separation(lo, hi, min_spans[i].1));
            }
        }
    }

    for i in 0..n {
        if let Some(rel) = env.relatives[i].0 {
            let lhs = x_vars[i].center_terms();
            let (rhs, rhs_c) = x_vars[rel.target].anchor_terms(rel.part, rects[rel.target].w * 0.5);
            if let Some(c) = linear_ge_constraint(&lhs, 0., &rhs, rhs_c + rel.delta, true) {
                base_cx.push(c);
            }
        } else if let Some(x) = env.pins[i].0 {
            match x_vars[i] {
                AxisVar::Leaf(vx) => base_cx.push(Constraint::fixed(vx, x)),
                AxisVar::Walls { lo, hi } => {
                    if env.w_locked[i] {
                        base_cx.push(Constraint::fixed(lo, x - rects[i].w * 0.5));
                        base_cx.push(Constraint::fixed(hi, x + rects[i].w * 0.5));
                    } else {
                        base_cx.push(Constraint {
                            coefficients: vec![(lo, 0.5), (hi, 0.5)],
                            rhs: x,
                            equality: true,
                        });
                    }
                }
            }
        }

        if let Some(rel) = env.relatives[i].1 {
            let lhs = y_vars[i].center_terms();
            let (rhs, rhs_c) = y_vars[rel.target].anchor_terms(rel.part, rects[rel.target].h * 0.5);
            if let Some(c) = linear_ge_constraint(&lhs, 0., &rhs, rhs_c + rel.delta, true) {
                base_cy.push(c);
            }
        } else if let Some(y) = env.pins[i].1 {
            match y_vars[i] {
                AxisVar::Leaf(vy) => base_cy.push(Constraint::fixed(vy, y)),
                AxisVar::Walls { lo, hi } => {
                    if env.h_locked[i] {
                        base_cy.push(Constraint::fixed(lo, y - rects[i].h * 0.5));
                        base_cy.push(Constraint::fixed(hi, y + rects[i].h * 0.5));
                    } else {
                        base_cy.push(Constraint {
                            coefficients: vec![(lo, 0.5), (hi, 0.5)],
                            rhs: y,
                            equality: true,
                        });
                    }
                }
            }
        }
    }

    for rel in &env.scene.relations {
        let ids: Vec<_> = rel
            .members
            .iter()
            .filter_map(|id| env.scene.index(id))
            .collect();
        if rel.distribute {
            let (cs, vars, is_x) = if rel.axis == Axis::Row {
                (&mut base_cx, &x_vars, true)
            } else {
                (&mut base_cy, &y_vars, false)
            };
            if let Some(gap) = rel.gap_grid {
                for pair in ids.windows(2) {
                    let (a, b) = (pair[0], pair[1]);
                    let ha = if is_x { rects[a].w * 0.5 } else { rects[a].h * 0.5 };
                    let hb = if is_x { rects[b].w * 0.5 } else { rects[b].h * 0.5 };
                    let (lhs, lc) = vars[b].min_edge_terms(hb);
                    let (rhs, rc) = vars[a].max_edge_terms(ha);
                    if let Some(c) =
                        linear_ge_constraint(&lhs, lc, &rhs, rc + gap * env.o.grid, true)
                    {
                        cs.push(c);
                    }
                }
            } else {
                for p in ids.windows(3) {
                    let mut coeffs: Vec<(usize, f64)> = Vec::new();
                    for (t, w) in [
                        (vars[p[0]].center_terms(), 1.),
                        (vars[p[1]].center_terms(), -2.),
                        (vars[p[2]].center_terms(), 1.),
                    ] {
                        for (idx, a) in t {
                            if let Some(entry) = coeffs.iter_mut().find(|e| e.0 == idx) {
                                entry.1 += w * a;
                            } else {
                                coeffs.push((idx, w * a));
                            }
                        }
                    }
                    coeffs.retain(|(_, c)| c.abs() > 1e-12);
                    if !coeffs.is_empty() {
                        cs.push(Constraint {
                            coefficients: coeffs,
                            rhs: 0.,
                            equality: true,
                        });
                    }
                }
            }
        } else {
            let (cs, vars) = if rel.axis == Axis::Row {
                (&mut base_cy, &y_vars)
            } else {
                (&mut base_cx, &x_vars)
            };
            for pair in ids.windows(2) {
                let lhs = vars[pair[0]].center_terms();
                let rhs = vars[pair[1]].center_terms();
                if let Some(c) = linear_ge_constraint(&lhs, 0., &rhs, 0., true) {
                    cs.push(c);
                }
            }
        }
    }

    // Containment inequalities for direct children inside container 4-wall variables.
    for i in 0..n {
        if let Some(p) = env.scene.nodes[i]
            .parent
            .as_deref()
            .and_then(|p| env.scene.index(p))
        {
            let pad = env.o.container_padding;
            let extra = if env.h_locked[p] {
                0.
            } else {
                top_extra(&env.scene.nodes[i])
            };
            let hdr_pad = pad + header_height(&env.scene.nodes[p]) + extra;
            let (lx_c, lx_cc) = x_vars[i].min_edge_terms(rects[i].w * 0.5);
            let (lx_p, lx_pc) = x_vars[p].min_edge_terms(rects[p].w * 0.5);
            if let Some(c) = linear_ge_constraint(&lx_c, lx_cc, &lx_p, lx_pc + pad, false) {
                base_cx.push(c);
            }
            let (rx_p, rx_pc) = x_vars[p].max_edge_terms(rects[p].w * 0.5);
            let (rx_c, rx_cc) = x_vars[i].max_edge_terms(rects[i].w * 0.5);
            if let Some(c) = linear_ge_constraint(&rx_p, rx_pc, &rx_c, rx_cc + pad, false) {
                base_cx.push(c);
            }
            let (ty_c, ty_cc) = y_vars[i].min_edge_terms(rects[i].h * 0.5);
            let (ty_p, ty_pc) = y_vars[p].min_edge_terms(rects[p].h * 0.5);
            if let Some(c) = linear_ge_constraint(&ty_c, ty_cc, &ty_p, ty_pc + hdr_pad, false) {
                base_cy.push(c);
            }
            let (by_p, by_pc) = y_vars[p].max_edge_terms(rects[p].h * 0.5);
            let (by_c, by_cc) = y_vars[i].max_edge_terms(rects[i].h * 0.5);
            if let Some(c) = linear_ge_constraint(&by_p, by_pc, &by_c, by_cc + pad, false) {
                base_cy.push(c);
            }
        }
    }

    if env.previous.is_none() {
        let has_locked_w_ancestor = |mut idx: usize| -> bool {
            while let Some(parent_id) = env.scene.nodes[idx].parent.as_deref() {
                let Some(pi) = env.scene.index(parent_id) else {
                    break;
                };
                if env.w_locked[pi] {
                    return true;
                }
                idx = pi;
            }
            false
        };
        let mut checked_pairs: BTreeSet<(usize, usize)> = BTreeSet::new();
        for e in &env.scene.edges {
            let (Some(a), Some(b)) = (
                env.scene.index(&e.source.node),
                env.scene.index(&e.target.node),
            ) else {
                continue;
            };
            if a == b || !checked_pairs.insert((a.min(b), a.max(b))) {
                continue;
            }
            let same_row = (rects[a].y - rects[b].y).abs() <= (rects[a].h + rects[b].h) * 0.5 + 32.;
            if !same_row || (rects[a].x - rects[b].x).abs() <= 1e-6 {
                continue;
            }
            if env.pins[a].0.is_some()
                || env.pins[b].0.is_some()
                || env.relatives[a].0.is_some()
                || env.relatives[b].0.is_some()
                || has_locked_w_ancestor(a)
                || has_locked_w_ancestor(b)
            {
                continue;
            }
            let (left, right) = if rects[a].x < rects[b].x {
                (a, b)
            } else {
                (b, a)
            };
            if let Some(req_gap) = parallel_bundle_corridor_width(
                env.scene,
                rects[left],
                rects[right],
                left,
                right,
                true,
                env.o,
            ) {
                let scaled_gap = req_gap * env.spacing;
                let (rx, rc) = x_vars[right].min_edge_terms(rects[right].w * 0.5);
                let (lx, lc) = x_vars[left].max_edge_terms(rects[left].w * 0.5);
                if let Some(c) = linear_ge_constraint(&rx, rc, &lx, lc + scaled_gap, false) {
                    base_cx.push(c);
                }
            }
        }
    }

    // Build gentle L1 transverse alignment terms for unpinned/slot-anchored fresh layouts,
    // without collapsing rank spacing or multi-neighbor hubs.
    let sides: BTreeMap<_, _> = env
        .requests
        .iter()
        .cloned()
        .map(|p| (p.key, p.side))
        .collect();
    let mut node_deg = vec![0usize; n];
    let mut anchored_face_counts: BTreeMap<(usize, Side), usize> = BTreeMap::new();
    for e in &env.scene.edges {
        if let (Some(a), Some(b)) = (
            env.scene.index(&e.source.node),
            env.scene.index(&e.target.node),
        ) && a != b
        {
            node_deg[a] += 1;
            node_deg[b] += 1;
            if matches!(
                e.source.constraint,
                PortConstraint::FixedPos { .. } | PortConstraint::FixedRatio { .. }
            ) {
                *anchored_face_counts
                    .entry((a, sides[&port_key(e, true)]))
                    .or_default() += 1;
            }
            if matches!(
                e.target.constraint,
                PortConstraint::FixedPos { .. } | PortConstraint::FixedRatio { .. }
            ) {
                *anchored_face_counts
                    .entry((b, sides[&port_key(e, false)]))
                    .or_default() += 1;
            }
        }
    }
    let pin_fixed_x: Vec<bool> = (0..n)
        .map(|i| {
            env.pins[i].0.is_some()
                && env.relatives[i].0.is_none()
                && (!env.scene.nodes[i].container || env.w_locked[i])
        })
        .collect();
    let pin_fixed_y: Vec<bool> = (0..n)
        .map(|i| {
            env.pins[i].1.is_some()
                && env.relatives[i].1.is_none()
                && (!env.scene.nodes[i].container || env.h_locked[i])
        })
        .collect();
    let mut l1_x = Vec::new();
    let mut l1_y = Vec::new();
    let mut face_peers_map: BTreeMap<(usize, Side), Vec<usize>> = BTreeMap::new();
    if env.previous.is_none() {
        let mut side_counts: BTreeMap<(usize, Side), usize> = BTreeMap::new();
        for req in env.requests {
            *side_counts.entry((req.node, req.side)).or_default() += 1;
        }
        let push_diff = |out: &mut Vec<L1Term>,
                         ta: &[(usize, f64)],
                         ca: f64,
                         tb: &[(usize, f64)],
                         cb: f64,
                         gap: f64,
                         weight: f64| {
            let mut coeffs: Vec<(usize, f64)> = Vec::new();
            for &(idx, c) in tb {
                if let Some(entry) = coeffs.iter_mut().find(|e| e.0 == idx) {
                    entry.1 += c;
                } else {
                    coeffs.push((idx, c));
                }
            }
            for &(idx, c) in ta {
                if let Some(entry) = coeffs.iter_mut().find(|e| e.0 == idx) {
                    entry.1 -= c;
                } else {
                    coeffs.push((idx, -c));
                }
            }
            coeffs.retain(|(_, c)| c.abs() > 1e-12);
            if !coeffs.is_empty() && weight > 0. {
                out.push(L1Term::new(coeffs, gap + ca - cb, weight));
            }
        };

        for (edge_idx, e) in env.scene.edges.iter().enumerate() {
            let (a, b) = (
                env.scene.index(&e.source.node).unwrap(),
                env.scene.index(&e.target.node).unwrap(),
            );
            if a == b {
                continue;
            }
            let sa = sides[&port_key(e, true)];
            let sb = sides[&port_key(e, false)];
            face_peers_map.entry((a, sa)).or_default().push(b);
            face_peers_map.entry((b, sb)).or_default().push(a);

            let src_anchor = matches!(
                e.source.constraint,
                PortConstraint::FixedPos { .. } | PortConstraint::FixedRatio { .. }
            );
            let dst_anchor = matches!(
                e.target.constraint,
                PortConstraint::FixedPos { .. } | PortConstraint::FixedRatio { .. }
            );
            let has_anchor = src_anchor || dst_anchor;
            let dense_anchored_face = (src_anchor
                && anchored_face_counts.get(&(a, sa)).copied().unwrap_or(0) > 2)
                || (dst_anchor
                    && anchored_face_counts.get(&(b, sb)).copied().unwrap_or(0) > 2);
            let hinted_facing = matches!(
                env.plan_hints.get(edge_idx).and_then(|h| *h),
                Some((h_sa, h_sb))
                    if h_sa == sa
                        && h_sb == sb
                        && matches!(
                            (h_sa, h_sb),
                            (Side::N, Side::S)
                                | (Side::S, Side::N)
                                | (Side::E, Side::W)
                                | (Side::W, Side::E)
                        )
                        && side_counts.get(&(a, sa)).copied().unwrap_or(0) == 1
                        && side_counts.get(&(b, sb)).copied().unwrap_or(0) == 1
            );
            let ortho_w = if has_anchor && !dense_anchored_face {
                env.o.weights.length.max(1.5) * 4.0
            } else if hinted_facing && !dense_anchored_face {
                env.o.weights.length.max(1.5) * 2.0
            } else if !has_anchor && node_deg[a] <= 2 && node_deg[b] <= 2 {
                0.02
            } else {
                0.
            };
            if ortho_w <= 0. {
                continue;
            }

            let (ax, ax_c) =
                endpoint_axis_expr(env, a, sa, &e.source.constraint, x_vars[a], rects, true);
            let (bx, bx_c) =
                endpoint_axis_expr(env, b, sb, &e.target.constraint, x_vars[b], rects, true);
            let (ay, ay_c) =
                endpoint_axis_expr(env, a, sa, &e.source.constraint, y_vars[a], rects, false);
            let (by, by_c) =
                endpoint_axis_expr(env, b, sb, &e.target.constraint, y_vars[b], rects, false);

            match (sa, sb) {
                (Side::E, Side::W) | (Side::W, Side::E) if !(pin_fixed_y[a] && pin_fixed_y[b]) => {
                    push_diff(&mut l1_y, &ay, ay_c, &by, by_c, 0., ortho_w);
                }
                (Side::N, Side::S) | (Side::S, Side::N) if !(pin_fixed_x[a] && pin_fixed_x[b]) => {
                    push_diff(&mut l1_x, &ax, ax_c, &bx, bx_c, 0., ortho_w);
                }
                _ => {}
            }
        }

        // Preserve tangential straightening on 1-to-1 / parallel bundles,
        // and center 1-to-many (deg >= 3) fan hubs on the median of their leaves.
        let mut aligned_bundles: BTreeSet<(usize, usize)> = BTreeSet::new();
        for e in &env.scene.edges {
            let (a, b) = (
                env.scene.index(&e.source.node).unwrap(),
                env.scene.index(&e.target.node).unwrap(),
            );
            let pair_bundle = !readable_routes
                && a != b
                && matches!(
                    e.source.constraint,
                    PortConstraint::Free | PortConstraint::FixedSide { .. }
                )
                && matches!(
                    e.target.constraint,
                    PortConstraint::Free | PortConstraint::FixedSide { .. }
                )
                && env
                    .scene
                    .edges
                    .iter()
                    .filter(|x| {
                        (x.source.node == e.source.node && x.target.node == e.target.node)
                            || (x.source.node == e.target.node && x.target.node == e.source.node)
                    })
                    .count()
                    >= 2;
            if !pair_bundle
                && (env
                    .scene
                    .edges
                    .iter()
                    .filter(|x| x.source.node == e.source.node)
                    .count()
                    != 1
                    || env
                        .scene
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
            let same_row =
                (rects[a].y - rects[b].y).abs() <= (rects[a].h + rects[b].h) * 0.5 + 32.;
            let same_col =
                (rects[a].x - rects[b].x).abs() <= (rects[a].w + rects[b].w) * 0.5 + 32.;
            if same_row && matches!((sa, sb), (Side::E, Side::W) | (Side::W, Side::E)) {
                if env.pins[a].1.is_none()
                    && env.pins[b].1.is_none()
                    && let (AxisVar::Leaf(va), AxisVar::Leaf(vb)) = (y_vars[a], y_vars[b])
                {
                    let y = (desired_y[va] * weights_y[va] + desired_y[vb] * weights_y[vb])
                        / (weights_y[va] + weights_y[vb]);
                    desired_y[va] = y;
                    desired_y[vb] = y;
                }
                if pair_bundle
                    && !(pin_fixed_y[a] && pin_fixed_y[b])
                    && aligned_bundles.insert((a.min(b), a.max(b)))
                {
                    let ay = y_vars[a].center_terms();
                    let by = y_vars[b].center_terms();
                    push_diff(
                        &mut l1_y,
                        &ay,
                        0.,
                        &by,
                        0.,
                        0.,
                        env.o.weights.length.max(1.5) * 4.0,
                    );
                }
            }
            if same_col && matches!((sa, sb), (Side::N, Side::S) | (Side::S, Side::N)) {
                if env.pins[a].0.is_none()
                    && env.pins[b].0.is_none()
                    && let (AxisVar::Leaf(va), AxisVar::Leaf(vb)) = (x_vars[a], x_vars[b])
                {
                    let x = (desired_x[va] * weights_x[va] + desired_x[vb] * weights_x[vb])
                        / (weights_x[va] + weights_x[vb]);
                    desired_x[va] = x;
                    desired_x[vb] = x;
                }
                if pair_bundle
                    && !(pin_fixed_x[a] && pin_fixed_x[b])
                    && aligned_bundles.insert((a.min(b), a.max(b)))
                {
                    let ax = x_vars[a].center_terms();
                    let bx = x_vars[b].center_terms();
                    push_diff(
                        &mut l1_x,
                        &ax,
                        0.,
                        &bx,
                        0.,
                        0.,
                        env.o.weights.length.max(1.5) * 4.0,
                    );
                }
            }
        }
        for ((hub, side), peers) in &face_peers_map {
            let mut unique_peers: Vec<usize> = peers.clone();
            unique_peers.sort_unstable();
            unique_peers.dedup();
            if unique_peers.len() < 3 {
                continue;
            }
            let is_ew = matches!(side, Side::E | Side::W);
            let span = unique_peers
                .iter()
                .map(|&p| if is_ew { rects[p].x } else { rects[p].y })
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                    (lo.min(v), hi.max(v))
                });
            if span.1 - span.0 > 64. {
                continue;
            }
            if is_ew && env.pins[*hub].1.is_none() {
                if let AxisVar::Leaf(vh) = y_vars[*hub] {
                    let mut ys: Vec<f64> = unique_peers.iter().map(|&p| rects[p].y).collect();
                    ys.sort_by(f64::total_cmp);
                    let mid = if ys.len() % 2 == 1 {
                        ys[ys.len() / 2]
                    } else {
                        0.5 * (ys[ys.len() / 2 - 1] + ys[ys.len() / 2])
                    };
                    desired_y[vh] = mid;
                }
            } else if !is_ew
                && env.pins[*hub].0.is_none()
                && let AxisVar::Leaf(vh) = x_vars[*hub]
            {
                let mut xs: Vec<f64> = unique_peers.iter().map(|&p| rects[p].x).collect();
                    xs.sort_by(f64::total_cmp);
                    let mid = if xs.len() % 2 == 1 {
                        xs[xs.len() / 2]
                    } else {
                        0.5 * (xs[xs.len() / 2 - 1] + xs[xs.len() / 2])
                    };
                desired_x[vh] = mid;
            }
        }
    }

    // Pre-group siblings for O(n log n) scanline visibility separation.
    let mut sibling_groups: BTreeMap<Option<&str>, Vec<usize>> = BTreeMap::new();
    for (i, node) in env.scene.nodes.iter().enumerate() {
        sibling_groups
            .entry(node.parent.as_deref())
            .or_default()
            .push(i);
    }
    let exact_nodes: Vec<usize> = (0..n)
        .filter(|&i| {
            env.pins[i].0.is_some()
                && env.pins[i].1.is_some()
                && env.relatives[i].0.is_none()
                && env.relatives[i].1.is_none()
        })
        .collect();
    let exact_flag: Vec<bool> = (0..n)
        .map(|i| {
            env.pins[i].0.is_some()
                && env.pins[i].1.is_some()
                && env.relatives[i].0.is_none()
                && env.relatives[i].1.is_none()
        })
        .collect();

    let mut sep_cx: Vec<Constraint> = Vec::new();
    let mut sep_cy: Vec<Constraint> = Vec::new();

    for _ in 0..n * 2 + 8 {
        let mut all_cx = base_cx.clone();
        all_cx.extend(sep_cx.iter().cloned());
        let mut all_cy = base_cy.clone();
        all_cy.extend(sep_cy.iter().cloned());

        let solve_fresh = |desired: &[f64],
                           weights: &[f64],
                           cs: &[Constraint],
                           linear: &[f64],
                           l1: &[L1Term]|
         -> Result<Vec<f64>, Error> {
            if l1.is_empty() && linear.iter().all(|v| v.abs() < 1e-12) {
                solver::project(desired, weights, cs)
            } else {
                solver::project_compacted(desired, weights, cs, linear, l1)
            }
        };

        let (xs, ys) = if env.previous.is_some() {
            (
                solve_incremental_partitioned(
                    &desired_x,
                    &weights_x,
                    &all_cx,
                    &prev_x,
                    &elastic_x,
                    env.o.weights.flip,
                )?,
                solve_incremental_partitioned(
                    &desired_y,
                    &weights_y,
                    &all_cy,
                    &prev_y,
                    &elastic_y,
                    env.o.weights.flip,
                )?,
            )
        } else {
            (
                solve_fresh(&desired_x, &weights_x, &all_cx, &linear_x, &l1_x)?,
                solve_fresh(&desired_y, &weights_y, &all_cy, &linear_y, &l1_y)?,
            )
        };

        for i in 0..n {
            match x_vars[i] {
                AxisVar::Leaf(vx) => rects[i].x = xs[vx],
                AxisVar::Walls { lo, hi } => {
                    rects[i].w = (xs[hi] - xs[lo]).max(min_spans[i].0);
                    rects[i].x = 0.5 * (xs[lo] + xs[hi]);
                }
            }
            match y_vars[i] {
                AxisVar::Leaf(vy) => rects[i].y = ys[vy],
                AxisVar::Walls { lo, hi } => {
                    rects[i].h = (ys[hi] - ys[lo]).max(min_spans[i].1);
                    rects[i].y = 0.5 * (ys[lo] + ys[hi]);
                }
            }
        }

        // Re-center 1-to-many (deg >= 3) fan hubs to the median midpoint after leaves shift.
        if env.previous.is_none() {
            for ((hub, side), peers) in &face_peers_map {
                let mut unique_peers: Vec<usize> = peers.clone();
                unique_peers.sort_unstable();
                unique_peers.dedup();
                if unique_peers.len() < 3 {
                    continue;
                }
                let is_ew = matches!(side, Side::E | Side::W);
                let span = unique_peers
                    .iter()
                    .map(|&p| if is_ew { rects[p].x } else { rects[p].y })
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                        (lo.min(v), hi.max(v))
                    });
                if span.1 - span.0 > 64. {
                    continue;
                }
                if is_ew && env.pins[*hub].1.is_none() {
                    if let AxisVar::Leaf(vh) = y_vars[*hub] {
                        let mut ys_p: Vec<f64> = unique_peers.iter().map(|&p| rects[p].y).collect();
                        ys_p.sort_by(f64::total_cmp);
                        let mid = if ys_p.len() % 2 == 1 {
                            ys_p[ys_p.len() / 2]
                        } else {
                            0.5 * (ys_p[ys_p.len() / 2 - 1] + ys_p[ys_p.len() / 2])
                        };
                        desired_y[vh] = mid;
                    }
                } else if !is_ew
                    && env.pins[*hub].0.is_none()
                    && let AxisVar::Leaf(vh) = x_vars[*hub]
                {
                    let mut xs_p: Vec<f64> = unique_peers.iter().map(|&p| rects[p].x).collect();
                        xs_p.sort_by(f64::total_cmp);
                        let mid = if xs_p.len() % 2 == 1 {
                            xs_p[xs_p.len() / 2]
                        } else {
                            0.5 * (xs_p[xs_p.len() / 2 - 1] + xs_p[xs_p.len() / 2])
                        };
                    desired_x[vh] = mid;
                }
            }
        }

        if exact_nodes.len() >= 2 {
            let exact_inflated: Vec<Rect> = rects
                .iter()
                .map(|r| r.inflate(env.o.margin / 2.))
                .collect();
            for (a, b) in scanline_overlapping_pairs(&exact_inflated, &exact_nodes) {
                if !env.ancestry[a].contains(&env.scene.nodes[b].id)
                    && !env.ancestry[b].contains(&env.scene.nodes[a].id)
                {
                    return Err(Error::Layout(format!(
                        "{} and {} overlap or leave less than {} px body clearance",
                        env.scene.nodes[a].id, env.scene.nodes[b].id, env.o.margin
                    )));
                }
            }
        }

        let body_r = |i: usize| {
            if exact_flag[i] {
                rects[i]
            } else {
                with_top_extra(&env.scene.nodes[i], rects[i])
            }
        };
        let env_r = |i: usize| {
            if exact_flag[i] {
                rects[i]
            } else {
                with_top_extra(
                    &env.scene.nodes[i],
                    stub_envelope(rects[i], env.stub_sides[i], env.o.stub),
                )
            }
        };
        let broad_rects: Vec<Rect> = (0..n)
            .map(|i| env_r(i).inflate(env.o.margin / 2.))
            .collect();

        let mut added = false;
        for group in sibling_groups.values() {
            if group.len() < 2 {
                continue;
            }
            for (a, b) in scanline_overlapping_pairs(&broad_rects, group) {
                let exact = exact_flag[a] && exact_flag[b];
                if exact {
                    return Err(Error::Layout(format!(
                        "{} and {} overlap or leave less than {} px body clearance",
                        env.scene.nodes[a].id, env.scene.nodes[b].id, env.o.margin
                    )));
                }
                let fresh_unpinned = env.previous.is_none()
                    && env.pins[a] == (None, None)
                    && env.pins[b] == (None, None);
                if !env_r(a)
                    .inflate(env.o.margin / 2.)
                    .intersects(body_r(b).inflate(env.o.margin / 2.))
                    && !env_r(b)
                        .inflate(env.o.margin / 2.)
                        .intersects(body_r(a).inflate(env.o.margin / 2.))
                    && !(fresh_unpinned
                        && env_r(a)
                            .inflate(env.o.margin / 2.)
                            .intersects(env_r(b).inflate(env.o.margin / 2.)))
                {
                    continue;
                }
                let dx = (rects[a].x - rects[b].x).abs();
                let dy = (rects[a].y - rects[b].y).abs();
                let x_stubs = if exact {
                    0
                } else if rects[a].x <= rects[b].x {
                    usize::from(env.stub_sides[a][1])
                        + usize::from(fresh_unpinned && env.stub_sides[b][3])
                        + usize::from(
                            !fresh_unpinned && !env.stub_sides[a][1] && env.stub_sides[b][3],
                        )
                } else {
                    usize::from(env.stub_sides[a][3])
                        + usize::from(fresh_unpinned && env.stub_sides[b][1])
                        + usize::from(
                            !fresh_unpinned && !env.stub_sides[a][3] && env.stub_sides[b][1],
                        )
                };
                let x_extra = env.o.margin + x_stubs as f64 * env.o.stub;
                let xgap = (rects[a].w + rects[b].w) * 0.5 + x_extra;
                let y_stubs = if exact {
                    0
                } else if rects[a].y <= rects[b].y {
                    usize::from(env.stub_sides[a][2])
                        + usize::from(fresh_unpinned && env.stub_sides[b][0])
                        + usize::from(
                            !fresh_unpinned && !env.stub_sides[a][2] && env.stub_sides[b][0],
                        )
                } else {
                    usize::from(env.stub_sides[a][0])
                        + usize::from(fresh_unpinned && env.stub_sides[b][2])
                        + usize::from(
                            !fresh_unpinned && !env.stub_sides[a][0] && env.stub_sides[b][2],
                        )
                };
                let title_extra = if exact {
                    0.
                } else if rects[a].y <= rects[b].y {
                    top_extra(&env.scene.nodes[b])
                } else {
                    top_extra(&env.scene.nodes[a])
                };
                let y_extra = env.o.margin + title_extra + y_stubs as f64 * env.o.stub;
                let ygap = (rects[a].h + rects[b].h) * 0.5 + y_extra;
                let row_align = env.scene.relations.iter().any(|r| {
                    !r.distribute
                        && r.axis == Axis::Row
                        && r.members.contains(&env.scene.nodes[a].id)
                        && r.members.contains(&env.scene.nodes[b].id)
                });
                let col_align = env.scene.relations.iter().any(|r| {
                    !r.distribute
                        && r.axis == Axis::Col
                        && r.members.contains(&env.scene.nodes[a].id)
                        && r.members.contains(&env.scene.nodes[b].id)
                });
                let hard_x_pair = env.pins[a].0.is_some()
                    && env.pins[b].0.is_some()
                    && env.relatives[a].0.is_none()
                    && env.relatives[b].0.is_none()
                    && (!env.scene.nodes[a].container || env.w_locked[a])
                    && (!env.scene.nodes[b].container || env.w_locked[b]);
                let hard_y_pair = env.pins[a].1.is_some()
                    && env.pins[b].1.is_some()
                    && env.relatives[a].1.is_none()
                    && env.relatives[b].1.is_none()
                    && (!env.scene.nodes[a].container || env.h_locked[a])
                    && (!env.scene.nodes[b].container || env.h_locked[b]);
                let use_x = if row_align || (hard_y_pair && !hard_x_pair) {
                    true
                } else if col_align || (hard_x_pair && !hard_y_pair) {
                    false
                } else {
                    xgap - dx < ygap - dy
                };
                let (left, right) = if use_x {
                    if rects[a].x <= rects[b].x { (a, b) } else { (b, a) }
                } else if rects[a].y <= rects[b].y {
                    (a, b)
                } else {
                    (b, a)
                };
                let c_opt = if use_x {
                    let (lhs, lc) = x_vars[right].min_edge_terms(rects[right].w * 0.5);
                    let (rhs, rc) = x_vars[left].max_edge_terms(rects[left].w * 0.5);
                    linear_ge_constraint(&lhs, lc, &rhs, rc + x_extra, false)
                } else {
                    let (lhs, lc) = y_vars[right].min_edge_terms(rects[right].h * 0.5);
                    let (rhs, rc) = y_vars[left].max_edge_terms(rects[left].h * 0.5);
                    linear_ge_constraint(&lhs, lc, &rhs, rc + y_extra, false)
                };
                if let Some(c) = c_opt {
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
        }
        if !added {
            break;
        }
    }

    if env.previous.is_none() {
        for &i in env.postorder {
            let node = &env.scene.nodes[i];
            if !node.ports.is_empty()
                || env.pins[i].0.is_some()
                || env.pins[i].1.is_some()
                || env.w_locked[i]
                || env.h_locked[i]
                || env
                    .scene
                    .edges
                    .iter()
                    .any(|e| e.source.node == node.id || e.target.node == node.id)
                || env.scene.relations.iter().any(|r| r.members.contains(&node.id))
            {
                continue;
            }
            if let Some(bounds) = env
                .scene
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, c)| c.parent.as_deref() == Some(&node.id))
                .map(|(j, c)| with_top_extra(c, rects[j]))
                .reduce(Rect::union)
            {
                rects[i] = Rect::new(
                    bounds.x,
                    bounds.y - header_height(node) * 0.5,
                    (bounds.w + 2. * env.o.container_padding).max(min_spans[i].0),
                    (bounds.h + 2. * env.o.container_padding + header_height(node))
                        .max(min_spans[i].1),
                );
            }
        }
    }
    let anchor_coord = |r: Rect, part: AnchorPart, is_x: bool| -> f64 {
        match (is_x, part) {
            (true, AnchorPart::Center) => r.x,
            (true, AnchorPart::MinWall) => r.left(),
            (true, AnchorPart::MaxWall) => r.right(),
            (false, AnchorPart::Center) => r.y,
            (false, AnchorPart::MinWall) => r.top(),
            (false, AnchorPart::MaxWall) => r.bottom(),
        }
    };
    let is_descendant = |anc: usize, mut cur: usize| -> bool {
        let anc_id = &env.scene.nodes[anc].id;
        while let Some(pid) = env.scene.nodes[cur].parent.as_deref() {
            if pid == anc_id {
                return true;
            }
            let Some(p_idx) = env.scene.nodes.iter().position(|node| node.id == pid) else {
                break;
            };
            cur = p_idx;
        }
        false
    };
    let mut may_follow: Vec<bool> = env
        .scene
        .nodes
        .iter()
        .map(|node| {
            env.scene
                .nodes
                .iter()
                .any(|c| c.parent.as_deref() == Some(&node.id))
        })
        .collect();
    for _ in 0..n {
        let mut any_moved = false;
        for k in 0..n {
            let mut cand = rects[k];
            if !pin_fixed_x[k]
                && let Some(rp) = env.relatives[k].0
                && (may_follow[rp.target] || env.previous.is_none())
                && !is_descendant(rp.target, k)
            {
                cand.x = anchor_coord(rects[rp.target], rp.part, true) + rp.delta;
            }
            if !pin_fixed_y[k]
                && let Some(rp) = env.relatives[k].1
                && (may_follow[rp.target] || env.previous.is_none())
                && !is_descendant(rp.target, k)
            {
                cand.y = anchor_coord(rects[rp.target], rp.part, false) + rp.delta;
            }
            let dx = cand.x - rects[k].x;
            let dy = cand.y - rects[k].y;
            if dx.abs() <= 1e-6 && dy.abs() <= 1e-6 {
                continue;
            }
            let overlaps_sibling = (0..n).any(|j| {
                j != k
                    && env.scene.nodes[j].parent == env.scene.nodes[k].parent
                    && (cand.x - rects[j].x).abs() < (cand.w + rects[j].w) * 0.5 - 1e-3
                    && (cand.y - rects[j].y).abs() < (cand.h + rects[j].h) * 0.5 - 1e-3
            });
            if !overlaps_sibling {
                rects[k] = cand;
                may_follow[k] = true;
                for j in 0..n {
                    if j != k && is_descendant(k, j) {
                        rects[j].x += dx;
                        rects[j].y += dy;
                        may_follow[j] = true;
                    }
                }
                any_moved = true;
            }
        }
        if !any_moved {
            break;
        }
    }
    for i in 0..n {
        if pin_fixed_x[i]
            && let Some(px) = env.pins[i].0
            && (rects[i].x - px).abs() <= 1e-6
        {
            rects[i].x = px;
        }
        if pin_fixed_y[i]
            && let Some(py) = env.pins[i].1
            && (rects[i].y - py).abs() <= 1e-6
        {
            rects[i].y = py;
        }
    }
    Ok(())
}

fn candidate_is_flat_chain_or_star(scene: &SceneGraph, plan: &TopologyPlan) -> bool {
    if plan.fold_row.iter().any(|&r| r > 0) || scene.nodes.len() < 2 {
        return false;
    }
    let n = scene.nodes.len();
    let mut out_deg = vec![0usize; n];
    let mut in_deg = vec![0usize; n];
    for e in &scene.edges {
        if let (Some(a), Some(b)) = (
            scene.index(&e.source.node),
            scene.index(&e.target.node),
        ) && a != b
        {
            out_deg[a] += 1;
            in_deg[b] += 1;
        }
    }
    let is_chain = scene.edges.len() <= n && out_deg.iter().all(|&d| d <= 1) && in_deg.iter().all(|&d| d <= 1);
    let is_one_hop_star = (0..n).any(|h| {
        scene.edges.iter().all(|e| {
            let a = scene.index(&e.source.node);
            let b = scene.index(&e.target.node);
            (a == Some(h) && in_deg[b.unwrap()] == 1 && out_deg[b.unwrap()] == 0)
                || (b == Some(h) && out_deg[a.unwrap()] == 1 && in_deg[a.unwrap()] == 0)
        })
    });
    is_chain || is_one_hop_star
}

fn score_candidate_preview(
    scene: &SceneGraph,
    o: &LayoutOptions,
    plan: &TopologyPlan,
    layout: &Layout,
) -> f64 {
    let mut crossings = 0usize;
    let mut wire_length = 0.;
    let mut bends = 0usize;
    let route_list: Vec<_> = scene
        .edges
        .iter()
        .filter_map(|e| layout.routes.get(&e.id).map(|r| (e, r)))
        .collect();
    for (idx, (_, ra)) in route_list.iter().enumerate() {
        wire_length += ra.points.windows(2).map(|p| p[0].distance(p[1])).sum::<f64>();
        bends += crate::routing::coalesced(&ra.points).len().saturating_sub(2);
        for (_, rb) in &route_list[idx + 1..] {
            for pa in ra.points.windows(2) {
                for pb in rb.points.windows(2) {
                    if crate::routing::proper_crossing(pa[0], pa[1], pb[0], pb[1]) {
                        crossings += 1;
                    }
                }
            }
        }
    }

    let mut placed_labels: Vec<Rect> = Vec::new();
    let mut estimated_callouts = 0usize;
    for (e, r) in &route_list {
        let text = crate::labels::connection_text(scene, e);
        if text.trim().is_empty() {
            continue;
        }
        let tw = crate::labels::text_width(&text) + 8.;
        let th = crate::labels::text_height(&text) + 4.;
        let mut found = None;
        for seg in r.points.windows(2) {
            let len = seg[0].distance(seg[1]);
            let mid = Point::new((seg[0].x + seg[1].x) * 0.5, (seg[0].y + seg[1].y) * 0.5);
            let horiz = (seg[0].y - seg[1].y).abs() < 1e-6;
            let req = if horiz { tw } else { th };
            if len + 1e-6 < req && len < 20. {
                continue;
            }
            let offsets = if horiz {
                [
                    Point::new(mid.x, mid.y - th * 0.5 - 4.),
                    Point::new(mid.x, mid.y + th * 0.5 + 4.),
                ]
            } else {
                [
                    Point::new(mid.x + tw * 0.5 + 6., mid.y),
                    Point::new(mid.x - tw * 0.5 - 6., mid.y),
                ]
            };
            for c in offsets {
                let cand = Rect::new(c.x, c.y, tw, th);
                let hits_node = layout
                    .nodes
                    .values()
                    .any(|nr| nr.inflate(2.).intersects(cand));
                let hits_lbl = placed_labels.iter().any(|lr| lr.intersects(cand));
                if !hits_node && !hits_lbl {
                    found = Some(cand);
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }
        if let Some(b) = found {
            placed_labels.push(b);
        } else {
            estimated_callouts += 1;
        }
    }

    let mut bounds: Option<Rect> = None;
    for nd in &scene.nodes {
        if let Some(&r) = layout.nodes.get(&nd.id) {
            let r = with_top_extra(nd, r);
            bounds = Some(bounds.map_or(r, |b| b.union(r)));
        }
    }
    for p in layout.ports.values() {
        let pr = Rect::new(p.stub.x, p.stub.y, 1., 1.);
        bounds = Some(bounds.map_or(pr, |b| b.union(pr)));
    }
    for lb in &placed_labels {
        bounds = Some(bounds.map_or(*lb, |b| b.union(*lb)));
    }
    let (bw, bh) = bounds.map_or((1., 1.), |b| (b.w.max(1.), b.h.max(1.)));
    let vw = 730.;
    let vh = 450.;
    let fit_scale = (vw / bw).min(vh / bh).min(1.);
    let fitted_pt = 15. * fit_scale;
    let shortfall = (1. / fit_scale - 1.).max(0.);
    let min_fit_text_penalty =
        180. * shortfall + 80. * (10.5 - fitted_pt).max(0.).powi(2);

    let ratio = (bw / bh).max(bh / bw);
    let aspect_penalty = if bw <= vw && bh <= vh {
        4. * (ratio - 4.0).max(0.)
    } else {
        150. * ((bw - vw).max(0.) / vw + (bh - vh).max(0.) / vh)
            + 40. * (ratio - o.target_aspect_ratio.max(2.5)).max(0.)
    };
    let axis_tiebreak = if plan.axis == FlowAxis::LeftToRight
        && candidate_is_flat_chain_or_star(scene, plan)
    {
        -35.
    } else {
        0.
    };

    let constraint_side = |c: &PortConstraint| -> Option<Side> {
        match *c {
            PortConstraint::Free => None,
            PortConstraint::FixedSide { side }
            | PortConstraint::FixedOrder { side, .. }
            | PortConstraint::FixedRatio { side, .. }
            | PortConstraint::FixedPos { side, .. } => Some(side),
        }
    };
    let port_axis_mismatches = route_list
        .iter()
        .filter(|(e, _)| {
            let (Some(a), Some(b)) = (scene.index(&e.source.node), scene.index(&e.target.node))
            else {
                return false;
            };
            if a == b || plan.fold_row[a] != plan.fold_row[b] || plan.ranks[a] == plan.ranks[b] {
                return false;
            }
            let (Some(sa), Some(sb)) = (
                constraint_side(&e.source.constraint),
                constraint_side(&e.target.constraint),
            ) else {
                return false;
            };
            match plan.axis {
                FlowAxis::TopToBottom => {
                    matches!(sa, Side::E | Side::W) && matches!(sb, Side::E | Side::W)
                }
                FlowAxis::LeftToRight => {
                    matches!(sa, Side::N | Side::S) && matches!(sb, Side::N | Side::S)
                }
                FlowAxis::QuadrantCross => false,
            }
        })
        .count();

    2000. * crossings as f64
        + 800. * estimated_callouts as f64
        + 1600. * port_axis_mismatches as f64
        + wire_length
        + 24. * bends as f64
        + aspect_penalty
        + min_fit_text_penalty
        + axis_tiebreak
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
struct CandidateInput<'a> {
    scene: &'a SceneGraph,
    options: &'a LayoutOptions,
    annotations: &'a [Vec<Annotation>],
    locks: &'a PinLockInfo,
    base_sizes: &'a [Point],
    plan: Option<&'a TopologyPlan>,
    previous: Option<&'a Layout>,
    spacing: f64,
}

fn widen_column_corridors(
    scene: &SceneGraph,
    rects: &mut [Rect],
    o: &LayoutOptions,
    spacing: f64,
) {
    let n = rects.len();
    let has_named = (0..n).any(|i| has_named_multiport(scene, i));
    let has_parallel = has_labeled_parallel_bundle(scene);
    if n < 2 || (!has_named && !has_parallel) {
        return;
    }
    let mut xs: Vec<f64> = rects.iter().map(|r| r.x).collect();
    xs.sort_by(f64::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 1.);
    if xs.len() < 2 {
        return;
    }
    let col_of = |x: f64| -> usize {
        xs.iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (*a - x).abs().total_cmp(&(*b - x).abs()))
            .map_or(0, |(c, _)| c)
    };
    let mut extra_after: Vec<f64> = vec![0.; xs.len() - 1];
    for e in &scene.edges {
        let (Some(a), Some(b)) = (scene.index(&e.source.node), scene.index(&e.target.node))
        else {
            continue;
        };
        let named_pair = has_named && (has_named_multiport(scene, a) || has_named_multiport(scene, b));
        let ca = col_of(rects[a].x);
        let cb = col_of(rects[b].x);
        if ca == cb {
            continue;
        }
        let (left_idx, right_idx, lo_c, hi_c) = if ca < cb {
            (a, b, ca, cb)
        } else {
            (b, a, cb, ca)
        };
        if hi_c != lo_c + 1 {
            continue;
        }
        let cur_gap = rects[right_idx].left() - rects[left_idx].right();
        let mut req_gap = 0f64;
        if named_pair {
            let face_edges = scene
                .edges
                .iter()
                .filter(|x| {
                    (scene.index(&x.source.node) == Some(a)
                        || scene.index(&x.target.node) == Some(a))
                        || (scene.index(&x.source.node) == Some(b)
                            || scene.index(&x.target.node) == Some(b))
                })
                .count();
            req_gap = req_gap.max(named_port_corridor_width(scene, e, a, b, face_edges, o) * spacing);
        }
        if has_parallel
            && let Some(par_w) =
                parallel_bundle_corridor_width(scene, rects[a], rects[b], a, b, ca < cb, o)
        {
            req_gap = req_gap.max(par_w * spacing);
        }
        if cur_gap < req_gap {
            extra_after[lo_c] = extra_after[lo_c].max(req_gap - cur_gap);
        }
    }
    if extra_after.iter().all(|&d| d <= 1e-6) {
        return;
    }
    let mut cum_shift = vec![0.; xs.len()];
    for c in 1..xs.len() {
        cum_shift[c] = cum_shift[c - 1] + extra_after[c - 1];
    }
    for r in rects.iter_mut() {
        let c = col_of(r.x);
        r.x += cum_shift[c];
    }
}

fn compact_unlabeled_fan_column(scene: &SceneGraph, rects: &mut [Rect], o: &LayoutOptions) {
    let n = scene.nodes.len();
    if n < 4 || scene.edges.is_empty() {
        return;
    }
    let mut deg = vec![0usize; n];
    for e in &scene.edges {
        if let (Some(a), Some(b)) = (scene.index(&e.source.node), scene.index(&e.target.node))
            && a != b
        {
            deg[a] += 1;
            deg[b] += 1;
        }
    }
    let Some((hub, _)) = deg
        .iter()
        .enumerate()
        .max_by_key(|&(i, &d)| (d, std::cmp::Reverse(i)))
        .filter(|&(_, &d)| d >= 3)
    else {
        return;
    };
    let is_star = scene.edges.iter().all(|e| {
        let (Some(a), Some(b)) = (scene.index(&e.source.node), scene.index(&e.target.node))
        else {
            return false;
        };
        (a == hub && b != hub && deg[b] == 1) || (b == hub && a != hub && deg[a] == 1)
    });
    if !is_star {
        return;
    }
    let mut leaves: Vec<usize> = (0..n).filter(|&i| i != hub && deg[i] == 1).collect();
    if leaves.len() < 3 {
        return;
    }
    let x0 = rects[leaves[0]].x;
    if !leaves.iter().all(|&i| (rects[i].x - x0).abs() < 1.0) {
        return;
    }
    leaves.sort_by(|&a, &b| rects[a].y.total_cmp(&rects[b].y).then(a.cmp(&b)));
    let max_h = leaves.iter().map(|&i| rects[i].h).fold(0.0f64, f64::max);
    let step = max_h + o.margin * 2.;
    let mid_y = rects[hub].y;
    let top = mid_y - ((leaves.len() - 1) as f64 * step) * 0.5;
    for (k, &leaf) in leaves.iter().enumerate() {
        rects[leaf].y = top + k as f64 * step;
    }
}

fn solve_candidate_layout(input: &CandidateInput<'_>) -> Result<(Layout, bool, bool), Error> {
    let scene = input.scene;
    let o = input.options;
    let annotations = input.annotations;
    let locks = input.locks;
    let base_sizes = input.base_sizes;
    let plan = input.plan;
    let previous = input.previous;
    let spacing = input.spacing;
    let n = scene.nodes.len();

    let layer_width = if o.max_layer_width == 0 {
        ((n as f64 * 240. / (112. * o.target_aspect_ratio))
            .sqrt()
            .ceil() as usize)
            .max(2)
    } else {
        o.max_layer_width
    };
    let cg_ranks = crate::topology::coffman_graham(scene, layer_width);

    let mut rects = if let Some(p) = plan {
        build_plan_rects(scene, base_sizes, p, o, spacing)
    } else {
        let mut rows = BTreeMap::new();
        let mut out_rects = Vec::with_capacity(n);
        for (i, node) in scene.nodes.iter().enumerate() {
            let row = rows.entry(cg_ranks[i]).or_insert(0);
            let mut r = Rect::new(
                cg_ranks[i] as f64 * 240.,
                *row as f64 * 112.,
                base_sizes[i].x,
                base_sizes[i].y,
            );
            *row += 1;
            if let Some(p) = previous.and_then(|p| p.nodes.get(&node.id)) {
                r.x = p.x;
                r.y = p.y;
                if node.container {
                    if !locks.w_locked[i] {
                        r.w = p.w.max(r.w);
                    }
                    if !locks.h_locked[i] {
                        r.h = p.h.max(r.h);
                    }
                }
            }
            out_rects.push(r);
        }
        out_rects
    };

    let mut pins = vec![(None, None); n];
    let mut relatives = vec![(None, None); n];
    // Evaluate pins iteratively first so seed desired positions resolve relational chains.
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
    check_pin_cycles(scene, annotations)?;

    let has_containers = scene.nodes.iter().any(|nd| nd.container);
    let plan_hints: &[Option<(Side, Side)>] = plan.map(|p| p.side_hints.as_slice()).unwrap_or(&[]);
    let init_requests = choose_port_sides(scene, &rects, plan_hints, o, previous);
    let fixed_init: Vec<PortRequest>;
    let pre_pack_requests = if previous.is_none() && !locks.has_pos_pin && has_containers {
        fixed_init = init_requests
            .iter()
            .filter(|p| !matches!(p.spec, PortConstraint::Free))
            .cloned()
            .collect();
        &fixed_init[..]
    } else {
        &init_requests[..]
    };
    grow_port_perimeters(
        scene,
        pre_pack_requests,
        &mut rects,
        &locks.w_locked,
        &locks.h_locked,
        o,
    )?;
    let has_custom_corridor =
        (0..n).any(|i| has_named_multiport(scene, i)) || has_labeled_parallel_bundle(scene);
    let has_custom_plan = plan.is_some_and(|p| {
        p.axis != FlowAxis::LeftToRight
            || p.fold_row.iter().any(|&f| f > 0)
            || !p.side_hints.is_empty()
            || !p.edge_guides.is_empty()
            || p.search_states > 0
            || has_custom_corridor
    });
    if previous.is_none()
        && !locks.has_pos_pin
        && has_custom_plan
        && let Some(p) = plan
    {
        let grown_sizes: Vec<Point> = rects.iter().map(|r| Point::new(r.w, r.h)).collect();
        rects = build_plan_rects(scene, &grown_sizes, p, o, spacing);
    }

    let is_lr_unfolded = plan
        .is_none_or(|p| p.axis == FlowAxis::LeftToRight && p.fold_row.iter().all(|&f| f == 0));
    let mut shape_stats = crate::topology::ShapeStats::default();
    let mut concept_sides = None;

    if previous.is_none() && is_lr_unfolded {
        if o.shape_budget > 0
            && !locks.has_pos_pin
            && scene.relations.is_empty()
            && scene
                .nodes
                .iter()
                .all(|nd| !nd.container && nd.parent.is_none())
        {
            let flat_ranks: Vec<usize> = if let Some(p) = plan {
                let min_r = p.ranks.iter().copied().min().unwrap_or(0);
                p.ranks.iter().map(|&r| (r - min_r) as usize).collect()
            } else {
                cg_ranks.clone()
            };
            order_flat_ranks(scene, &flat_ranks, &mut rects, o);
        }
        if !locks.has_pos_pin {
            shape_stats = crate::topology::optimize(scene, &mut rects, o);
        }
        concept_sides = crate::topology::readable_concept(scene, &mut rects, o, true);
        if concept_sides.is_some()
            && !locks.has_pos_pin
            && !has_containers
            && scene
                .edges
                .iter()
                .all(|e| crate::labels::connection_text(scene, e).trim().is_empty())
        {
            compact_unlabeled_fan_column(scene, &mut rects, o);
        }
        if !locks.has_pos_pin && !has_containers {
            widen_column_corridors(scene, &mut rects, o, spacing);
        }
        if !has_custom_plan && spacing > 1. {
            for r in &mut rects {
                r.x *= spacing;
                r.y *= spacing;
            }
        }
        if !locks.has_pos_pin {
            crate::ownership::pack(scene, &mut rects, o, spacing);
        }
    } else if previous.is_none() && !has_custom_plan && spacing > 1. {
        for r in &mut rects {
            r.x *= spacing;
            r.y *= spacing;
        }
    }

    let postorder = crate::ownership::containers_postorder(scene);
    // Resolve overlaps among freshly placed siblings or newly added incremental nodes
    // without disturbing already placed prior siblings.
    for &i in &postorder {
        let child_ids: Vec<_> = scene
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, c)| c.parent.as_deref() == Some(&scene.nodes[i].id))
            .map(|(j, _)| j)
            .collect();
        let mut placed: Vec<usize> = Vec::new();
        for &j in &child_ids {
            let is_existing = previous
                .is_some_and(|prior| prior.nodes.contains_key(&scene.nodes[j].id));
            if !is_existing && pins[j] == (None, None) {
                for &k in &placed {
                    if with_top_extra(&scene.nodes[j], rects[j])
                        .inflate(16.)
                        .intersects(with_top_extra(&scene.nodes[k], rects[k]).inflate(16.))
                    {
                        let delta = rects[k].bottom()
                            + 48.
                            + top_extra(&scene.nodes[j])
                            - rects[j].top();
                        let id = scene.nodes[j].id.clone();
                        for (t, rect) in rects.iter_mut().enumerate() {
                            if t == j || ancestors(scene, &scene.nodes[t].id).contains(&id) {
                                rect.y += delta;
                            }
                        }
                    }
                }
            }
            placed.push(j);
        }
        if previous.is_none() {
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
                if pins[i].0.is_none() {
                    rects[i].x = b.x;
                }
                if pins[i].1.is_none() {
                    rects[i].y = b.y - header * 0.5;
                }
                if !locks.w_locked[i] {
                    rects[i].w = rects[i].w.max(b.w);
                }
                if !locks.h_locked[i] {
                    rects[i].h = rects[i].h.max(b.h + header);
                }
            }
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
                    .entry((nd.parent.clone(), cg_ranks[i]))
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
                let ids: BTreeSet<&str> =
                    col.iter().map(|&i| scene.nodes[i].id.as_str()).collect();
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
                    rects[i].y = cursor + extra + rects[i].h * 0.5;
                    cursor += extra + rects[i].h + gap;
                }
            }
        }
    }

    if concept_sides.is_none() {
        concept_sides = crate::topology::readable_concept(scene, &mut rects, o, false);
    }

    let sync_requests = |rects: &[Rect], prior: Option<&Layout>| -> Vec<PortRequest> {
        let mut reqs = choose_port_sides(scene, rects, plan_hints, o, prior);
        if let Some(ref sides) = concept_sides {
            for (edge, hint) in scene.edges.iter().zip(sides) {
                let Some((source_side, target_side)) = *hint else {
                    continue;
                };
                for (source, side, other) in [
                    (true, source_side, &edge.target),
                    (false, target_side, &edge.source),
                ] {
                    if let Some(request) =
                        reqs.iter_mut().find(|p| p.key == port_key(edge, source))
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
        let endpoint_sides: BTreeMap<_, _> =
            reqs.iter().map(|p| (p.key.clone(), p.side)).collect();
        for edge in &scene.edges {
            let sa = endpoint_sides[&port_key(edge, true)];
            let sb = endpoint_sides[&port_key(edge, false)];
            for source in [true, false] {
                if let Some(request) = reqs.iter_mut().find(|p| p.key == port_key(edge, source))
                    && let Some(target) = parallel_order(
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
        reqs
    };

    let mut min_spans: Vec<(f64, f64)> = base_sizes.iter().map(|s| (s.x, s.y)).collect();
    let mut requests = sync_requests(&rects, previous);
    grow_port_perimeters(
        scene,
        &requests,
        &mut rects,
        &locks.w_locked,
        &locks.h_locked,
        o,
    )?;
    for (i, ms) in min_spans.iter_mut().enumerate() {
        ms.0 = ms.0.max(rects[i].w);
        ms.1 = ms.1.max(rects[i].h);
    }

    let ancestry: Vec<_> = scene
        .nodes
        .iter()
        .map(|nd| ancestors(scene, &nd.id))
        .collect();
    let build_stub_sides = |reqs: &[PortRequest]| {
        let mut s = vec![[false; 4]; n];
        for p in reqs {
            s[p.node][side_index(p.side)] = true;
        }
        s
    };
    let mut stub_sides = build_stub_sides(&requests);
    {
        let env = Stage1bEnv {
            scene,
            o,
            spacing,
            requests: &requests,
            plan_hints,
            pins: &pins,
            relatives: &relatives,
            w_locked: &locks.w_locked,
            h_locked: &locks.h_locked,
            previous,
            stub_sides: &stub_sides,
            ancestry: &ancestry,
            postorder: &postorder,
        };
        solve_stage1b(&env, &mut rects, &min_spans, concept_sides.is_some())?;
    }

    if previous.is_none() {
        let requests2 = sync_requests(&rects, None);
        let side_changed = requests
            .iter()
            .zip(requests2.iter())
            .any(|(a, b)| a.key != b.key || a.side != b.side);
        if side_changed {
            let prev_dims: Vec<(f64, f64)> = rects.iter().map(|r| (r.w, r.h)).collect();
            grow_port_perimeters(
                scene,
                &requests2,
                &mut rects,
                &locks.w_locked,
                &locks.h_locked,
                o,
            )?;
            let mut grew = false;
            for (i, (ow, oh)) in prev_dims.into_iter().enumerate() {
                if (rects[i].w - ow).abs() > 1e-6 || (rects[i].h - oh).abs() > 1e-6 {
                    grew = true;
                    min_spans[i].0 = min_spans[i].0.max(rects[i].w);
                    min_spans[i].1 = min_spans[i].1.max(rects[i].h);
                }
            }
            requests = requests2;
            if grew {
                stub_sides = build_stub_sides(&requests);
                let env2 = Stage1bEnv {
                    scene,
                    o,
                    spacing,
                    requests: &requests,
                    plan_hints,
                    pins: &pins,
                    relatives: &relatives,
                    w_locked: &locks.w_locked,
                    h_locked: &locks.h_locked,
                    previous: None,
                    stub_sides: &stub_sides,
                    ancestry: &ancestry,
                    postorder: &postorder,
                };
                solve_stage1b(&env2, &mut rects, &min_spans, concept_sides.is_some())?;
            }
        }
    }

    let total_states = plan.map_or(0, |p| p.search_states) + shape_stats.searched_states;
    let mut out = Layout {
        solved_nodes: n,
        shape_search_states: if o.shape_budget > 0 {
            total_states.min(o.shape_budget)
        } else {
            0
        },
        ..Default::default()
    };
    for (i, node) in scene.nodes.iter().enumerate() {
        out.nodes.insert(node.id.clone(), rects[i]);
    }
    assign_ports(scene, o, &requests, &rects, previous.is_none(), &mut out)?;
    crate::routing::route_all(scene, o, previous, &mut out)?;
    crate::routing::nudge_channels(scene, o, &mut out);

    let is_star_fan = n > 2
        && scene.edges.len() + 1 == n
        && scene.nodes.iter().any(|nd| {
            scene.edges.iter().all(|e| e.source.node == nd.id)
                || scene.edges.iter().all(|e| e.target.node == nd.id)
        });
    let coordinated_fan =
        concept_sides.is_some() && is_star_fan && crate::routing::coordinate_fan(scene, o, &mut out);

    let has_route_ann = scene.edges.iter().any(|e| {
        effective_annotations(scene, &e.id, &e.annotations, o.variant.as_deref())
            .iter()
            .any(|a| a.name == "route")
    });
    let has_nn_return = scene.edges.iter().enumerate().any(|(i, e)| {
        out.ports
            .get(&port_key(e, true))
            .is_some_and(|pa| pa.side == Side::N)
            && out
                .ports
                .get(&port_key(e, false))
                .is_some_and(|pb| pb.side == Side::N)
            && scene.edges[..i]
                .iter()
                .any(|other| other.source.node == e.target.node && other.target.node == e.source.node)
    });
    let coordinated_loops = (concept_sides.is_some() || (has_nn_return && !has_route_ann))
        && crate::routing::coordinate_outer_routes(scene, o, &mut out);

    Ok((out, coordinated_fan, coordinated_loops))
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

    let annotations: Vec<Vec<Annotation>> = scene
        .nodes
        .iter()
        .map(|node| effective_annotations(scene, &node.id, &node.annotations, o.variant.as_deref()))
        .collect();
    let locks = parse_pin_locks(&annotations, o)?;
    let base_sizes = compute_base_sizes(scene, &locks, o)?;

    let (mut out, coordinated_fan, coordinated_loops) = if previous.is_none() && !locks.has_pos_pin {
        let all_plans = crate::topology::candidates(scene, &base_sizes, o);
        let mut plans: Vec<TopologyPlan> = all_plans
            .iter()
            .filter(|p| candidate_compatible_with_relations(scene, p))
            .cloned()
            .collect();
        if plans.is_empty() {
            plans = all_plans.into_iter().take(1).collect();
        }
        if plans.len() <= 1 {
            let input = CandidateInput {
                scene,
                options: o,
                annotations: &annotations,
                locks: &locks,
                base_sizes: &base_sizes,
                plan: plans.first(),
                previous: None,
                spacing,
            };
            solve_candidate_layout(&input)?
        } else {
            let mut best: Option<(f64, Layout, bool, bool)> = None;
            let mut first_err: Option<Error> = None;
            for plan in &plans {
                let input = CandidateInput {
                    scene,
                    options: o,
                    annotations: &annotations,
                    locks: &locks,
                    base_sizes: &base_sizes,
                    plan: Some(plan),
                    previous: None,
                    spacing,
                };
                match solve_candidate_layout(&input) {
                    Ok((cand_out, fan, loops)) => {
                        let s = score_candidate_preview(scene, o, plan, &cand_out);
                        let replace = match &best {
                            None => true,
                            Some((best_s, _, _, _)) => s < *best_s - 1e-6,
                        };
                        if replace {
                            best = Some((s, cand_out, fan, loops));
                        }
                    }
                    Err(e) => {
                        if first_err.is_none() {
                            first_err = Some(e);
                        }
                    }
                }
            }
            if let Some((_, cand_out, fan, loops)) = best {
                (cand_out, fan, loops)
            } else {
                return Err(first_err.unwrap_or_else(|| {
                    Error::Layout("no feasible candidate layout".into())
                }));
            }
        }
    } else {
        let input = CandidateInput {
            scene,
            options: o,
            annotations: &annotations,
            locks: &locks,
            base_sizes: &base_sizes,
            plan: None,
            previous,
            spacing,
        };
        solve_candidate_layout(&input)?
    };

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
    if !coordinated_fan && !coordinated_loops {
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
    crate::model::resolve_scoped_index(scene, scope, id)
}
fn pin_anchor_raw(expr: &str) -> Option<String> {
    let e = expr.replace(' ', "");
    if number(&e).is_some() {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AnchorPart {
    Center,
    MinWall,
    MaxWall,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RelativePin {
    target: usize,
    part: AnchorPart,
    delta: f64,
}

fn relative_constraint(
    expr: &str,
    scene: &SceneGraph,
    rects: &[Rect],
    x: bool,
    grid: f64,
    scope: Option<&str>,
) -> Result<Option<RelativePin>, Error> {
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
    let (part, anchor_val) = match anchor {
        "W" => (AnchorPart::MinWall, rects[j].x - rects[j].w * 0.5),
        "E" => (AnchorPart::MaxWall, rects[j].x + rects[j].w * 0.5),
        "N" => (AnchorPart::MinWall, rects[j].y - rects[j].h * 0.5),
        "S" => (AnchorPart::MaxWall, rects[j].y + rects[j].h * 0.5),
        "x" => (AnchorPart::Center, rects[j].x),
        _ => (AnchorPart::Center, rects[j].y),
    };
    Ok(Some(RelativePin {
        target: j,
        part,
        delta: value - anchor_val,
    }))
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
        let scope = scene.nodes[i].parent.as_deref();
        for a in list.iter().filter(|a| a.name == "pin") {
            for key in ["x", "y"] {
                if let Some(v) = a.attrs.get(key)
                    && let Some(raw) = pin_anchor_raw(&v.text)
                    && let Some(target) = crate::model::resolve_scoped_node(scene, scope, &raw)
                    && let Some(j) = scene.index(&target.id)
                {
                    deps[i].push(j);
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
// Select attachment sides purely from geometry, obstacle exposure, face-load
// balance, and optional candidate topology hints (never conditioned on @pin).
#[allow(clippy::too_many_arguments)]
fn choose_attachment_pair(
    scene: &SceneGraph,
    rects: &[Rect],
    e: &Edge,
    is_reverse_return: bool,
    hint: Option<(Side, Side)>,
    face_peers: &[[BTreeSet<usize>; 4]],
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
    let point = |r: Rect, s: Side, c: &PortConstraint| {
        let cx = o.corner.min(r.w / 4.);
        let cy = o.corner.min(r.h / 4.);
        let tx = match *c {
            PortConstraint::FixedPos { side, offset_grid } if side == s => {
                (r.left() + offset_grid * o.grid).clamp(r.left() + cx, r.right() - cx)
            }
            PortConstraint::FixedRatio { side, ratio } if side == s => {
                (r.left() + ratio * r.w).clamp(r.left() + cx, r.right() - cx)
            }
            _ => r.x,
        };
        let ty = match *c {
            PortConstraint::FixedPos { side, offset_grid } if side == s => {
                (r.top() + offset_grid * o.grid).clamp(r.top() + cy, r.bottom() - cy)
            }
            PortConstraint::FixedRatio { side, ratio } if side == s => {
                (r.top() + ratio * r.h).clamp(r.top() + cy, r.bottom() - cy)
            }
            _ => r.y,
        };
        match s {
            Side::N => Point::new(tx, r.top()),
            Side::E => Point::new(r.right(), ty),
            Side::S => Point::new(tx, r.bottom()),
            Side::W => Point::new(r.left(), ty),
        }
    };
    let ra = rects[ai];
    let rb = rects[bi];
    let x_sep = rb.left() - ra.right() >= 2. * o.stub || ra.left() - rb.right() >= 2. * o.stub;
    let y_ov = ra.top() < rb.bottom() && rb.top() < ra.bottom();
    let y_sep = rb.top() - ra.bottom() >= 2. * o.stub || ra.top() - rb.bottom() >= 2. * o.stub;
    let x_ov = ra.left() < rb.right() && rb.left() < ra.right();
    let mut best = (f64::INFINITY, Side::E, Side::W);
    let sides = [Side::E, Side::W, Side::S, Side::N];
    for sa in sides {
        for sb in sides {
            if e.source.constraint.side().is_some_and(|s| s != sa)
                || e.target.constraint.side().is_some_and(|s| s != sb)
            {
                continue;
            }
            let a = point(ra, sa, &e.source.constraint);
            let b = point(rb, sb, &e.target.constraint);
            let (ax, ay) = sa.normal();
            let (bx, by) = sb.normal();
            let u = a.offset(ax * o.stub, ay * o.stub);
            let v = b.offset(bx * o.stub, by * o.stub);
            let mut candidates: Vec<Vec<Point>> = vec![
                vec![a, u, Point::new(u.x, v.y), v, b],
                vec![a, u, Point::new(v.x, u.y), v, b],
            ];
            if sa == sb {
                let x_lo = a.x.min(b.x) - o.margin;
                let x_hi = a.x.max(b.x) + o.margin;
                let y_lo = a.y.min(b.y) - o.margin;
                let y_hi = a.y.max(b.y) + o.margin;
                let row_top = ra.top().min(rb.top());
                let row_bottom = ra.bottom().max(rb.bottom());
                let col_left = ra.left().min(rb.left());
                let col_right = ra.right().max(rb.right());
                match sa {
                    Side::N => {
                        let lane_y = rects
                            .iter()
                            .enumerate()
                            .filter(|&(k, rk)| {
                                k != ai
                                    && k != bi
                                    && !scene.nodes[k].container
                                    && rk.right() > x_lo
                                    && rk.left() < x_hi
                                    && rk.bottom() >= row_top - o.stub
                                    && rk.top() <= row_bottom
                            })
                            .map(|(_, rk)| rk.top() - o.stub - o.margin)
                            .fold(u.y.min(v.y), f64::min);
                        if lane_y < u.y.min(v.y) - 1e-6 {
                            candidates.push(vec![
                                a,
                                Point::new(a.x, lane_y),
                                Point::new(b.x, lane_y),
                                b,
                            ]);
                        }
                    }
                    Side::S => {
                        let lane_y = rects
                            .iter()
                            .enumerate()
                            .filter(|&(k, rk)| {
                                k != ai
                                    && k != bi
                                    && !scene.nodes[k].container
                                    && rk.right() > x_lo
                                    && rk.left() < x_hi
                                    && rk.top() <= row_bottom + o.stub
                                    && rk.bottom() >= row_top
                            })
                            .map(|(_, rk)| rk.bottom() + o.stub + o.margin)
                            .fold(u.y.max(v.y), f64::max);
                        if lane_y > u.y.max(v.y) + 1e-6 {
                            candidates.push(vec![
                                a,
                                Point::new(a.x, lane_y),
                                Point::new(b.x, lane_y),
                                b,
                            ]);
                        }
                    }
                    Side::W => {
                        let lane_x = rects
                            .iter()
                            .enumerate()
                            .filter(|&(k, rk)| {
                                k != ai
                                    && k != bi
                                    && !scene.nodes[k].container
                                    && rk.bottom() > y_lo
                                    && rk.top() < y_hi
                                    && rk.right() >= col_left - o.stub
                                    && rk.left() <= col_right
                            })
                            .map(|(_, rk)| rk.left() - o.stub - o.margin)
                            .fold(u.x.min(v.x), f64::min);
                        if lane_x < u.x.min(v.x) - 1e-6 {
                            candidates.push(vec![
                                a,
                                Point::new(lane_x, a.y),
                                Point::new(lane_x, b.y),
                                b,
                            ]);
                        }
                    }
                    Side::E => {
                        let lane_x = rects
                            .iter()
                            .enumerate()
                            .filter(|&(k, rk)| {
                                k != ai
                                    && k != bi
                                    && !scene.nodes[k].container
                                    && rk.bottom() > y_lo
                                    && rk.top() < y_hi
                                    && rk.left() <= col_right + o.stub
                                    && rk.right() >= col_left
                            })
                            .map(|(_, rk)| rk.right() + o.stub + o.margin)
                            .fold(u.x.max(v.x), f64::max);
                        if lane_x > u.x.max(v.x) + 1e-6 {
                            candidates.push(vec![
                                a,
                                Point::new(lane_x, a.y),
                                Point::new(lane_x, b.y),
                                b,
                            ]);
                        }
                    }
                }
            }
            for path in &candidates {
                let last_seg = path.len().saturating_sub(2);
                let mut cost = path.windows(2).map(|p| p[0].distance(p[1])).sum::<f64>();
                cost += crate::routing::coalesced(path).len().saturating_sub(2) as f64
                    * o.weights.bend;
                for (index, p) in path.windows(2).enumerate() {
                    if p[0].distance(p[1]) < 1e-6 {
                        continue;
                    }
                    for (i, r) in rects.iter().enumerate() {
                        if i == ai && index == 0 || i == bi && index == last_seg {
                            continue;
                        }
                        if scene.nodes[i].container {
                            if let Some(hdr) = container_header_rect(&scene.nodes[i], *r)
                                && hdr.inflate(4.).blocks(p[0], p[1])
                            {
                                cost += 100000.;
                            }
                            continue;
                        }
                        if r.inflate(o.margin).blocks(p[0], p[1]) {
                            cost += 100000.;
                        }
                    }
                }
                // Penalize departures and arrivals that immediately reverse their
                // mandatory outward stubs, including zero-length corner legs.
                let first_wp = path[1];
                let after = path.get(2).copied().filter(|&c| first_wp.distance(c) > 1e-6)
                    .unwrap_or_else(|| path[path.len() - 2]);
                let last_wp = path[path.len() - 2];
                let before = path
                    .get(path.len().saturating_sub(3))
                    .copied()
                    .filter(|&c| last_wp.distance(c) > 1e-6)
                    .unwrap_or(first_wp);
                if (after.x - first_wp.x) * ax + (after.y - first_wp.y) * ay < -1e-6 {
                    cost += 100000.;
                }
                if (before.x - last_wp.x) * bx + (before.y - last_wp.y) * by < -1e-6 {
                    cost += 100000.;
                }
                let mixed_axes = matches!(sa, Side::E | Side::W) != matches!(sb, Side::E | Side::W);
                if mixed_axes && ((x_sep && y_ov) || (y_sep && x_ov)) {
                    cost += o.weights.bend;
                }
                if is_reverse_return && sa == sb && sa == Side::N {
                    cost -= 0.5 * o.grid;
                }
                let load_a = face_peers[ai][side_index(sa)]
                    .iter()
                    .filter(|&&p| p != bi)
                    .count();
                let load_b = face_peers[bi][side_index(sb)]
                    .iter()
                    .filter(|&&p| p != ai)
                    .count();
                cost += 1.5 * (load_a + load_b) as f64;
                if hint == Some((sa, sb)) {
                    cost -= 3. * o.stub;
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

fn choose_port_sides(
    scene: &SceneGraph,
    rects: &[Rect],
    side_hints: &[Option<(Side, Side)>],
    o: &LayoutOptions,
    prior: Option<&Layout>,
) -> Vec<PortRequest> {
    let mut face_peers: Vec<[BTreeSet<usize>; 4]> = vec![std::array::from_fn(|_| BTreeSet::new()); scene.nodes.len()];
    let mut chosen_pairs = Vec::with_capacity(scene.edges.len());
    let mut endpoint_sides = BTreeMap::new();
    for (ei, e) in scene.edges.iter().enumerate() {
        let hint = side_hints.get(ei).copied().flatten();
        let is_reverse_return = e.source.node != e.target.node
            && scene.edges[..ei]
                .iter()
                .any(|other| other.source.node == e.target.node && other.target.node == e.source.node);
        let pair = choose_attachment_pair(
            scene,
            rects,
            e,
            is_reverse_return,
            hint,
            &face_peers,
            o,
            prior,
        );
        let ai = scene.index(&e.source.node).unwrap();
        let bi = scene.index(&e.target.node).unwrap();
        if ai != bi {
            face_peers[ai][side_index(pair.0)].insert(bi);
            face_peers[bi][side_index(pair.1)].insert(ai);
        }
        endpoint_sides.insert(port_key(e, true), pair.0);
        endpoint_sides.insert(port_key(e, false), pair.1);
        chosen_pairs.push(pair);
    }
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for (ei, e) in scene.edges.iter().enumerate() {
        let pair = chosen_pairs[ei];
        for (source, ep, other) in [(true, &e.source, &e.target), (false, &e.target, &e.source)] {
            let i = scene.index(&ep.node).unwrap();
            let j = scene.index(&other.node).unwrap();
            let b = rects[j];
            let side = if source { pair.0 } else { pair.1 };
            let other_side = if source { pair.1 } else { pair.0 };
            let base = port_target(b, &other.constraint, side, other_side, o.grid);
            let target = parallel_order(scene, e, source, pair.0, pair.1, &endpoint_sides, base)
                .unwrap_or(base);

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
/// Parallel and antiparallel links sharing attachment sides use consistent
/// non-crossing port order at both ends, including U-turn bundles.
pub(crate) fn parallel_order(
    scene: &SceneGraph,
    e: &Edge,
    source: bool,
    sa: Side,
    sb: Side,
    endpoint_sides: &BTreeMap<String, Side>,
    base: f64,
) -> Option<f64> {
    if e.source.node == e.target.node {
        return None;
    }
    let u_id = e.source.node.as_str().min(e.target.node.as_str());
    let v_id = e.source.node.as_str().max(e.target.node.as_str());
    let (su, sv) = if e.source.node == u_id {
        (sa, sb)
    } else {
        (sb, sa)
    };
    let pairs: Vec<_> = scene
        .edges
        .iter()
        .filter(|other| {
            let ou = other.source.node.as_str().min(other.target.node.as_str());
            let ov = other.source.node.as_str().max(other.target.node.as_str());
            if (ou, ov) != (u_id, v_id) {
                return false;
            }
            let osa = endpoint_sides.get(&port_key(other, true)).copied();
            let osb = endpoint_sides.get(&port_key(other, false)).copied();
            let (osu, osv) = if other.source.node == u_id {
                (osa, osb)
            } else {
                (osb, osa)
            };
            osu == Some(su) && osv == Some(sv)
        })
        .collect();
    if pairs.len() < 2 {
        return None;
    }
    let ep = if source { &e.source } else { &e.target };
    if ep.port.is_some()
        || !matches!(
            ep.constraint,
            PortConstraint::Free | PortConstraint::FixedSide { .. }
        )
    {
        return None;
    }
    let index = pairs.iter().position(|other| other.id == e.id)?;
    let on_u = ep.node == u_id;
    let same_clockwise = matches!(su, Side::N | Side::E) == matches!(sv, Side::N | Side::E);
    Some(base + index as f64 * 1e-4 * if !on_u && same_clockwise { -1. } else { 1. })
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
    w_locked: &[bool],
    h_locked: &[bool],
    o: &LayoutOptions,
) -> Result<(), Error> {
    let port_sides: BTreeMap<&str, (usize, Side)> = requests
        .iter()
        .map(|p| (p.key.as_str(), (p.node, p.side)))
        .collect();
    for (i, r) in rects.iter_mut().enumerate() {
        for side in [Side::N, Side::E, Side::S, Side::W] {
            let horiz = matches!(side, Side::N | Side::S);
            let locked = if horiz { w_locked[i] } else { h_locked[i] };
            let locked_dim = if horiz { r.w } else { r.h };
            let eff_corner = if locked {
                o.corner.min(locked_dim / 4.)
            } else {
                o.corner
            };
            let ports: Vec<_> = requests
                .iter()
                .filter(|p| p.node == i && p.side == side)
                .collect();
            if ports.is_empty() {
                continue;
            }
            let mut length =
                ports.len().saturating_sub(1) as f64 * o.port_spacing + 2. * eff_corner;
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
                    let peer_locked = |idx: usize| {
                        if horiz {
                            w_locked[idx]
                        } else {
                            h_locked[idx]
                        }
                    };
                    if sa_i == i && sa == side && sb == opp && !peer_locked(sb_i) {
                        *facing_counts.entry(sb_i).or_default() += 1;
                    } else if sb_i == i && sb == side && sa == opp && !peer_locked(sa_i) {
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
                    let node_id = &scene.nodes[i].id;
                    (e.source.node == *node_id || e.target.node == *node_id)
                        && (ports.iter().any(|p| p.key == port_key(e, true) || p.key == port_key(e, false)))
                })
                .collect();
            let peers: BTreeSet<&str> = peer_edges
                .iter()
                .map(|e| {
                    if e.source.node == scene.nodes[i].id {
                        e.target.node.as_str()
                    } else {
                        e.source.node.as_str()
                    }
                })
                .collect();
            let unconstrained_bundle = !locked
                && ports.len() >= 2
                && peers.len() == 1
                && peer_edges.len() == ports.len()
                && peer_edges.iter().all(|e| {
                    !e.label.trim().is_empty()
                        && matches!(
                            e.source.constraint,
                            PortConstraint::Free | PortConstraint::FixedSide { .. }
                        )
                        && matches!(
                            e.target.constraint,
                            PortConstraint::Free | PortConstraint::FixedSide { .. }
                        )
                });
            if unconstrained_bundle {
                let caption_space = peer_edges
                    .iter()
                    .map(|e| {
                        if horiz {
                            crate::labels::text_width(&e.label) + 32.
                        } else {
                            crate::labels::text_height(&crate::labels::connection_text(scene, e))
                        }
                    })
                    .fold(0., f64::max)
                    + 12.;
                length = length.max((ports.len() + 1) as f64 * caption_space.max(o.port_spacing));
            }
            let unconstrained_ports = ports
                .iter()
                .filter(|p| !matches!(p.spec, PortConstraint::FixedPos { .. }))
                .count();
            if !locked && unconstrained_ports > 3 {
                length = length.max(
                    (ports.len().saturating_sub(1) as f64) * (o.port_spacing + o.margin)
                        + 2. * o.corner,
                );
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
                        let pos = offset_grid * o.grid;
                        if locked
                            && (pos < eff_corner - 1e-6 || pos > locked_dim - eff_corner + 1e-6)
                        {
                            return Err(Error::Layout(format!(
                                "fixed port {} exceeds locked face length",
                                p.key
                            )));
                        }
                        // A free attachment must still fit beside an unused
                        // fixed slot. Counting ports alone misses the clearance
                        // consumed by a slot close to the start of the side.
                        length = length.max(pos + movable as f64 * o.port_spacing + eff_corner);
                    }
                    PortConstraint::FixedRatio { ratio, .. } => {
                        if ratio <= 0. || ratio >= 1. {
                            return Err(Error::Layout("port ratio must be in (0,1)".into()));
                        }
                        if locked {
                            let pos = ratio * locked_dim;
                            if pos < eff_corner - 1e-6 || pos > locked_dim - eff_corner + 1e-6 {
                                return Err(Error::Layout(format!(
                                    "fixed ratio port {} violates corner clearance on locked face",
                                    p.key
                                )));
                            }
                        }
                        length = length.max(eff_corner / ratio.min(1. - ratio));
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
            if locked {
                let min_span =
                    ports.len().saturating_sub(1) as f64 * o.port_spacing + 2. * eff_corner;
                if min_span > locked_dim + 1e-6 {
                    return Err(Error::Layout(format!(
                        "ports on {} exceed locked face length",
                        scene.nodes[i].id
                    )));
                }
            } else if horiz {
                r.w = r.w.max(length);
            } else {
                r.h = r.h.max(length);
            }
        }
    }
    Ok(())
}
#[allow(clippy::needless_range_loop)]
fn assign_ports<'a>(
    scene: &SceneGraph,
    o: &LayoutOptions,
    requests: &'a [PortRequest],
    rects: &[Rect],
    align_facing: bool,
    out: &mut Layout,
) -> Result<(), Error> {
    let mut side_orders: Vec<(usize, Side, Vec<&'a PortRequest>, bool)> = Vec::new();
    let req_by_key: BTreeMap<&str, &PortRequest> =
        requests.iter().map(|p| (p.key.as_str(), p)).collect();
    let peer_anchor_pt = |req: &PortRequest| -> Point {
        let r = rects[req.node];
        let horiz = matches!(req.side, Side::N | Side::S);
        let t = match req.spec {
            PortConstraint::FixedRatio { ratio, .. } => {
                if horiz {
                    r.left() + ratio * r.w
                } else {
                    r.top() + ratio * r.h
                }
            }
            PortConstraint::FixedPos { offset_grid, .. } => {
                if horiz {
                    r.left() + offset_grid * o.grid
                } else {
                    r.top() + offset_grid * o.grid
                }
            }
            _ => {
                if horiz {
                    r.x
                } else {
                    r.y
                }
            }
        };
        match req.side {
            Side::N => Point::new(t, r.top()),
            Side::S => Point::new(t, r.bottom()),
            Side::E => Point::new(r.right(), t),
            Side::W => Point::new(r.left(), t),
        }
    };
    let mut peer_pt_map: BTreeMap<String, Point> = BTreeMap::new();
    for e in &scene.edges {
        let ka = port_key(e, true);
        let kb = port_key(e, false);
        if let (Some(pa), Some(pb)) = (req_by_key.get(ka.as_str()), req_by_key.get(kb.as_str())) {
            peer_pt_map.insert(ka, peer_anchor_pt(pb));
            peer_pt_map.insert(kb, peer_anchor_pt(pa));
        }
    }
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
    let is_fixed_spec = |spec: &PortConstraint| {
        matches!(
            spec,
            PortConstraint::FixedPos { .. } | PortConstraint::FixedRatio { .. }
        )
    };
    let is_order_spec = |spec: &PortConstraint| matches!(spec, PortConstraint::FixedOrder { .. });
    for (i, node) in scene.nodes.iter().enumerate() {
        for side in [Side::N, Side::E, Side::S, Side::W] {
            let mut ps: Vec<_> = requests
                .iter()
                .filter(|p| p.node == i && p.side == side)
                .collect();
            if ps.is_empty() {
                continue;
            }
            let reverse = matches!(side, Side::S | Side::W);
            let r = rects[i];
            let (nx, ny) = side.normal();
            let (tx, ty) = (-ny, nx);
            let face_mid = match side {
                Side::N => Point::new(r.x, r.top()),
                Side::S => Point::new(r.x, r.bottom()),
                Side::E => Point::new(r.right(), r.y),
                Side::W => Point::new(r.left(), r.y),
            };
            let cw_angle = |key: &str| -> f64 {
                peer_pt_map.get(key).map_or(0., |pt| {
                    let dx = pt.x - face_mid.x;
                    let dy = pt.y - face_mid.y;
                    let fwd = dx * nx + dy * ny;
                    let cw = dx * tx + dy * ty;
                    cw.atan2(fwd)
                })
            };
            ps.sort_by(|a, b| {
                let order = if (a.target - b.target).abs() <= 1e-2 {
                    cw_angle(&a.key).total_cmp(&cw_angle(&b.key))
                } else if reverse {
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
                .filter(|(_, p)| is_order_spec(&p.spec))
                .map(|(k, _)| k)
                .collect();
            let mut ordered_ports: Vec<_> = positions.iter().map(|k| ps[*k]).collect();
            ordered_ports.sort_by_key(|p| {
                if let PortConstraint::FixedOrder { order, .. } = p.spec {
                    order
                } else {
                    0
                }
            });
            for (k, p) in positions.into_iter().zip(ordered_ports) {
                ps[k] = p;
            }
            let r = rects[i];
            let horiz_side = matches!(side, Side::N | Side::S);
            let len = if horiz_side { r.w } else { r.h };
            let corner = o.corner.min(len / 4.);
            let count = ps.len();
            let hdr_hi = if node.container && side == Side::N {
                container_header_rect(node, r)
                    .map(|h| (h.right() - r.left() + 12.).min(len - corner))
                    .unwrap_or(corner)
                    .max(corner)
            } else {
                corner
            };
            let mut fixed_pos = vec![None; count];
            for (j, p) in ps.iter().enumerate() {
                fixed_pos[j] = match p.spec {
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
            }
            let fixed_order_indices: Vec<usize> = (0..count)
                .filter(|&j| is_order_spec(&ps[j].spec))
                .collect();
            let mut desired: Vec<f64> = (0..count)
                .map(|j| (j + 1) as f64 * len / (count + 1) as f64)
                .collect();
            for j in 0..count {
                if let Some(v) = fixed_pos[j] {
                    desired[j] = v;
                }
            }
            // Monotonic slot allocation into gaps between fixed anchors, preserving
            // both [lo_bound, len - corner] clearance and hard FixedOrder chains.
            type SlotPlan = (Vec<f64>, Vec<(usize, f64)>);
            let try_slot_plan = |lo_bound: f64| -> Option<SlotPlan> {
                let mut anchors: Vec<(usize, f64)> = (0..count)
                    .filter_map(|j| fixed_pos[j].map(|v| (j, v)))
                    .collect();
                anchors.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
                let mut available = Vec::new();
                let mut begin = lo_bound;
                for end in anchors
                    .iter()
                    .filter(|&&(_, v)| v >= lo_bound - 1e-6)
                    .map(|(_, v)| v - o.port_spacing)
                    .chain(std::iter::once(len - corner))
                {
                    let hi = end.min(len - corner);
                    let mut t = begin;
                    while t <= hi + 1e-6 {
                        available.push(t);
                        t += o.port_spacing;
                    }
                    let anchor_t = end + o.port_spacing;
                    begin = begin.max(anchor_t + o.port_spacing);
                }
                let unfixed: Vec<usize> =
                    (0..count).filter(|&j| !is_fixed_spec(&ps[j].spec)).collect();
                if available.len() < unfixed.len() {
                    return None;
                }
                let m = available.len();
                let mut new_desired = desired.clone();
                let mut used = vec![false; m];
                let scale_span = (len - corner - lo_bound).max(0.);
                let unfixed_cnt = unfixed.len();
                for (u_idx, &j) in unfixed.iter().enumerate() {
                    if lo_bound > corner + 1e-6 && anchors.is_empty() {
                        new_desired[j] = lo_bound
                            + (u_idx as f64 + 0.5) * scale_span / unfixed_cnt.max(1) as f64;
                    }
                }
                let k_len = fixed_order_indices.len();
                if k_len > 0 {
                    let mut dp = vec![vec![f64::INFINITY; m]; k_len];
                    let mut prev = vec![vec![0usize; m]; k_len];
                    for p in 0..m {
                        dp[0][p] = (available[p] - new_desired[fixed_order_indices[0]]).abs();
                    }
                    for k in 1..k_len {
                        let mut best_val = f64::INFINITY;
                        let mut best_q = 0usize;
                        for p in k..m {
                            if dp[k - 1][p - 1] < best_val {
                                best_val = dp[k - 1][p - 1];
                                best_q = p - 1;
                            }
                            if best_val.is_finite() {
                                dp[k][p] = best_val
                                    + (available[p] - new_desired[fixed_order_indices[k]]).abs();
                                prev[k][p] = best_q;
                            }
                        }
                    }
                    let last_p = ((k_len - 1)..m)
                        .min_by(|&a, &b| dp[k_len - 1][a].total_cmp(&dp[k_len - 1][b]))?;
                    if !dp[k_len - 1][last_p].is_finite() {
                        return None;
                    }
                    let mut chosen = vec![0usize; k_len];
                    let mut cur = last_p;
                    for k in (0..k_len).rev() {
                        chosen[k] = cur;
                        if k > 0 {
                            cur = prev[k][cur];
                        }
                    }
                    for (k, &slot_idx) in chosen.iter().enumerate() {
                        used[slot_idx] = true;
                        new_desired[fixed_order_indices[k]] = available[slot_idx];
                    }
                }
                let free_only: Vec<usize> = unfixed
                    .into_iter()
                    .filter(|&j| !is_order_spec(&ps[j].spec))
                    .collect();
                for j in free_only {
                    let ideal = new_desired[j];
                    let slot_idx = (0..m)
                        .filter(|&s| !used[s])
                        .min_by(|&a, &b| {
                            (available[a] - ideal)
                                .abs()
                                .total_cmp(&(available[b] - ideal).abs())
                                .then(a.cmp(&b))
                        })?;
                    used[slot_idx] = true;
                    new_desired[j] = available[slot_idx];
                }
                let mut new_ordered: Vec<(usize, f64)> =
                    (0..count).map(|j| (j, new_desired[j])).collect();
                new_ordered.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
                Some((new_desired, new_ordered))
            };
            let hdr_clear_unfixed = hdr_hi > corner + 1e-6;
            let mut used_hdr_lo = false;
            let mut ordered: Vec<(usize, f64)> = (0..count).map(|j| (j, desired[j])).collect();
            let order_preserved = |ord: &[(usize, f64)]| {
                let mut prev_pos = None;
                for &fo in &fixed_order_indices {
                    let pos = ord.iter().position(|&(j, _)| j == fo).unwrap();
                    if let Some(p) = prev_pos
                        && p >= pos
                    {
                        return false;
                    }
                    prev_pos = Some(pos);
                }
                true
            };
            let plan_feasible = |ord: &[(usize, f64)], lo: f64| {
                let mut lower = lo;
                ord.iter().all(|&(j, t)| {
                    let value = if fixed_pos[j].is_some() { t } else { t.max(lower) };
                    let ok = value >= lower - 1e-6 && value <= len - corner + 1e-6;
                    lower = value + o.port_spacing;
                    ok
                }) && order_preserved(ord)
            };
            if hdr_clear_unfixed
                && let Some((nd, no)) = try_slot_plan(hdr_hi)
                && order_preserved(&no)
            {
                desired = nd;
                ordered = no;
                used_hdr_lo = true;
            } else {
                ordered.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
                if (ps.iter().any(|p| is_fixed_spec(&p.spec)) || !order_preserved(&ordered))
                    && !plan_feasible(&ordered, corner)
                    && let Some((nd, no)) = try_slot_plan(corner)
                {
                    desired = nd;
                    ordered = no;
                }
            }
            let mut cs = Vec::new();
            for j in 0..count {
                if let Some(v) = fixed_pos[j] {
                    cs.push(Constraint::fixed(j, v));
                }
            }
            for pair in ordered.windows(2) {
                cs.push(Constraint::separation(pair[0].0, pair[1].0, o.port_spacing));
            }
            for pair in fixed_order_indices.windows(2) {
                cs.push(Constraint::separation(pair[0], pair[1], o.port_spacing));
            }
            if !ordered.is_empty() {
                let first_j = ordered[0].0;
                let last_j = ordered[ordered.len() - 1].0;
                let lo_bound = if used_hdr_lo && fixed_pos[first_j].is_none() {
                    hdr_hi
                } else {
                    corner
                };
                cs.push(Constraint {
                    coefficients: vec![(first_j, 1.)],
                    rhs: lo_bound,
                    equality: false,
                });
                cs.push(Constraint {
                    coefficients: vec![(last_j, -1.)],
                    rhs: -(len - corner),
                    equality: false,
                });
            }
            let positions = solver::project(&desired, &vec![1.; count], &cs)?;
            for (j, p) in ps.iter().enumerate() {
                let mut t = positions[j];
                if t < corner - 1e-5 || t > len - corner + 1e-5 {
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
                side_orders.push((i, side, final_ps, used_hdr_lo));
            }
        }
    }
    if align_facing {
        let mut facing_peers: BTreeMap<String, Vec<(usize, String)>> = BTreeMap::new();
        for e in &scene.edges {
            let ai = scene.index(&e.source.node).unwrap();
            let bi = scene.index(&e.target.node).unwrap();
            if ai == bi {
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
            let mut node_port_count = vec![0usize; scene.nodes.len()];
            for req in requests {
                node_port_count[req.node] += 1;
            }
            let mut port_meta: BTreeMap<&str, (usize, usize, Side, bool, bool)> = BTreeMap::new();
            for (face_idx, &(_i, side, ref ps, _)) in side_orders.iter().enumerate() {
                for (j, p) in ps.iter().enumerate() {
                    port_meta.insert(
                        p.key.as_str(),
                        (face_idx, j, side, is_fixed_spec(&p.spec), ps.len() == 1),
                    );
                }
            }
            for horiz_axis in [true, false] {
                let axis_faces: Vec<usize> = side_orders
                    .iter()
                    .enumerate()
                    .filter(|(_, (_, side, _, _))| matches!(side, Side::N | Side::S) == horiz_axis)
                    .map(|(idx, _)| idx)
                    .collect();
                if axis_faces.is_empty() {
                    continue;
                }
                let mut total_vars = 0usize;
                let mut face_base = BTreeMap::new();
                for &face_idx in &axis_faces {
                    let (_, _, ref ps, _) = side_orders[face_idx];
                    face_base.insert(face_idx, total_vars);
                    total_vars += ps.len();
                }
                let mut desired = vec![0.; total_vars];
                let mut weights = vec![1.; total_vars];
                let mut cs = Vec::new();
                let mut l1_terms = Vec::new();
                let mut mutual_seen: BTreeSet<(usize, usize)> = BTreeSet::new();
                let mut any_axis_facing = false;

                for &face_idx in &axis_faces {
                    let (i, side, ref ps, used_hdr_lo) = side_orders[face_idx];
                    let base = face_base[&face_idx];
                    let r = rects[i];
                    let len = if horiz_axis { r.w } else { r.h };
                    let corner = o.corner.min(len / 4.);
                    if len < 2. * corner - 1e-6 {
                        continue;
                    }
                    let hdr_lo = if used_hdr_lo {
                        container_header_rect(&scene.nodes[i], r)
                            .map(|h| (h.right() - r.left() + 12.).min(len - corner))
                            .unwrap_or(corner)
                            .max(corner)
                    } else {
                        corner
                    };
                    let reverse = matches!(side, Side::S | Side::W);
                    let count = ps.len();
                    let face_origin = if horiz_axis { r.left() } else { r.top() };
                    let (aff_s, aff_c) = if reverse {
                        (-1., face_origin + len)
                    } else {
                        (1., face_origin)
                    };

                    let first_fixed = is_fixed_spec(&ps[0].spec);
                    cs.push(Constraint {
                        coefficients: vec![(base, 1.)],
                        rhs: if first_fixed { corner } else { hdr_lo },
                        equality: false,
                    });
                    cs.push(Constraint {
                        coefficients: vec![(base + count - 1, -1.)],
                        rhs: -(len - corner),
                        equality: false,
                    });
                    let mut order_items: Vec<(usize, u32)> = ps
                        .iter()
                        .enumerate()
                        .filter_map(|(j, p)| match p.spec {
                            PortConstraint::FixedOrder { order, .. } => Some((j, order)),
                            _ => None,
                        })
                        .collect();
                    order_items.sort_by_key(|&(j, order)| (order, j));
                    for pair in order_items.windows(2) {
                        cs.push(Constraint::separation(
                            base + pair[0].0,
                            base + pair[1].0,
                            o.port_spacing,
                        ));
                    }

                    for (j, p) in ps.iter().enumerate() {
                        let v = base + j;
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
                        if let Some(fv) = fixed {
                            desired[v] = fv;
                            weights[v] = 1.;
                            cs.push(Constraint::fixed(v, fv));
                        } else {
                            desired[v] = base_t;
                            weights[v] = if node_port_count[i] >= 3 && count == 1 {
                                0.1
                            } else {
                                1.
                            };
                            if let Some(peers) = facing_peers.get(&p.key) {
                                let mut uni_targets = Vec::new();
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
                                    let peer_len = if horiz_axis { pr.w } else { pr.h };
                                    let peer_corner = o.corner.min(peer_len / 4.);
                                    let (ov_lo, ov_hi) = if horiz_axis {
                                        (
                                            (r.left() + hdr_lo).max(pr.left() + peer_corner),
                                            (r.right() - corner).min(pr.right() - peer_corner),
                                        )
                                    } else {
                                        (
                                            (r.top() + hdr_lo).max(pr.top() + peer_corner),
                                            (r.bottom() - corner).min(pr.bottom() - peer_corner),
                                        )
                                    };
                                    if ov_lo > ov_hi + 1e-6 {
                                        continue;
                                    }
                                    let peer_pt = out.ports[peer_key].point;
                                    let coord = if horiz_axis {
                                        peer_pt.x.clamp(ov_lo, ov_hi)
                                    } else {
                                        peer_pt.y.clamp(ov_lo, ov_hi)
                                    };
                                    let (p0, p1) = if horiz_axis {
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
                                    if blocked {
                                        continue;
                                    }
                                    any_axis_facing = true;
                                    let peer_yields = peers.len() == 1
                                        && facing_peers
                                            .get(peer_key.as_str())
                                            .is_some_and(|back| back.len() == 1)
                                        && port_meta.get(peer_key.as_str()).is_some_and(
                                            |&(p_face, _p_j, _, p_fixed, p_single)| {
                                                let peer_fan_skip = p_single
                                                    && side_distinct_peers
                                                        .get(&(i, side))
                                                        .is_some_and(|s| s.len() >= 3);
                                                !p_fixed && !peer_fan_skip && face_base.contains_key(&p_face)
                                            },
                                        );
                                    if peer_yields
                                        && let Some(&(p_face, p_j, p_side, _, _)) =
                                            port_meta.get(peer_key.as_str())
                                    {
                                        let pv = face_base[&p_face] + p_j;
                                        let pair_id = (v.min(pv), v.max(pv));
                                        if mutual_seen.insert(pair_id) {
                                            let p_len = if horiz_axis { pr.w } else { pr.h };
                                            let p_orig =
                                                if horiz_axis { pr.left() } else { pr.top() };
                                            let (ps_aff, pc_aff) =
                                                if matches!(p_side, Side::S | Side::W) {
                                                    (-1., p_orig + p_len)
                                                } else {
                                                    (1., p_orig)
                                                };
                                            l1_terms.push(L1Term::new(
                                                vec![(v, aff_s), (pv, -ps_aff)],
                                                pc_aff - aff_c,
                                                500.,
                                            ));
                                        }
                                    } else {
                                        let t_raw = if horiz_axis {
                                            coord - r.left()
                                        } else {
                                            coord - r.top()
                                        };
                                        let param = if reverse { len - t_raw } else { t_raw };
                                        uni_targets.push(param.clamp(hdr_lo, len - corner));
                                    }
                                }
                                if !uni_targets.is_empty() {
                                    desired[v] =
                                        uni_targets.iter().sum::<f64>() / uni_targets.len() as f64;
                                    weights[v] = 12.;
                                }
                            }
                        }
                        if j > 0 {
                            cs.push(Constraint::separation(v - 1, v, o.port_spacing));
                        }
                    }
                }
                if !any_axis_facing {
                    continue;
                }
                let linear = vec![0.; total_vars];
                if let Ok(solved) =
                    solver::project_compacted(&desired, &weights, &cs, &linear, &l1_terms)
                {
                    for &face_idx in &axis_faces {
                        let (i, side, ref ps, _) = side_orders[face_idx];
                        let r = rects[i];
                        let len = if horiz_axis { r.w } else { r.h };
                        let corner = o.corner.min(len / 4.);
                        let reverse = matches!(side, Side::S | Side::W);
                        let base = face_base[&face_idx];
                        let slice = &solved[base..base + ps.len()];
                        if slice
                            .iter()
                            .all(|&pos| pos >= corner - 1e-5 && pos <= len - corner + 1e-5)
                        {
                            let node = &scene.nodes[i];
                            for (j, p) in ps.iter().enumerate() {
                                let t = if reverse { len - slice[j] } else { slice[j] };
                                write_port(node, r, side, t, &p.key, out);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
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
                    && let Some(raw) = pin_anchor_raw(&v.text)
                    && !crate::model::resolve_scoped_node(scene, n.parent.as_deref(), &raw)
                        .is_some_and(|target| members.contains(&target.id))
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
    if validate(scene, &out, o).is_ok() && crate::labels::placements_clear(scene, &out) {
        Some(out)
    } else {
        None
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::lower;
    use crate::syntax::parse;

    #[test]
    fn layout_options_serde_defaults_and_cache_config_reflect_explore_candidates() {
        let off: LayoutOptions = serde_json::from_str(r#"{"explore_candidates":false}"#).unwrap();
        assert!(!off.explore_candidates);
        assert_eq!(off.grid, GRID);

        let default_json: LayoutOptions = serde_json::from_str("{}").unwrap();
        assert!(default_json.explore_candidates);

        let scene = lower(&parse("a -> b;"));
        let l1 = layout(&scene, &default_json, None).unwrap();
        let l2 = layout(&scene, &off, None).unwrap();
        assert_ne!(l1.cache_config, l2.cache_config);
    }

    #[test]
    fn scoped_pin_cycles_and_siblings_resolve_lexically() {
        let valid = lower(&parse("sub {\n b\n a @pin(x: b.E + 4g)\n}\n"));
        assert!(layout(&valid, &LayoutOptions::default(), None).is_ok());

        let cyclic = lower(&parse("sub {\n a @pin(x: b.E + 4g)\n b @pin(x: a.W - 4g)\n}\n"));
        let err = layout(&cyclic, &LayoutOptions::default(), None).unwrap_err();
        assert!(err.to_string().contains("cyclic relative pin anchors"));
    }

    #[test]
    fn local_query_detects_external_prefix_pin_dependency() {
        let opts = LayoutOptions::default();
        let before = lower(&parse(
            "sub {\n a @pin(x: sub_ext.E + 4g)\n}\nsub_ext [label: \"Old\"]\n",
        ));
        let prior = layout(&before, &opts, None).unwrap();
        let after = lower(&parse(
            "sub {\n a @pin(x: sub_ext.E + 4g)\n}\nsub_ext [label: \"Much Longer External Node\"]\n",
        ));
        let keys = query_keys(&after, &opts).0;
        assert!(local_query(&after, &opts, &prior, &keys).is_none());
    }

    #[test]
    fn candidate_scoring_and_relation_compatibility_behave_deterministically() {
        let opts = LayoutOptions::default();
        let chain = lower(&parse("a -> b -> c;"));
        let chain_anns: Vec<Vec<Annotation>> = chain
            .nodes
            .iter()
            .map(|n| effective_annotations(&chain, &n.id, &n.annotations, opts.variant.as_deref()))
            .collect();
        let chain_locks = parse_pin_locks(&chain_anns, &opts).unwrap();
        let chain_sizes = compute_base_sizes(&chain, &chain_locks, &opts).unwrap();
        let chain_plans = crate::topology::candidates(&chain, &chain_sizes, &opts);
        assert!(!chain_plans.is_empty());

        let mut lr_chain = chain_plans[0].clone();
        lr_chain.axis = FlowAxis::LeftToRight;
        lr_chain.fold_row = vec![0; chain.nodes.len()];
        assert!(candidate_is_flat_chain_or_star(&chain, &lr_chain));

        let mut tb_chain = lr_chain.clone();
        tb_chain.axis = FlowAxis::TopToBottom;
        assert!(candidate_is_flat_chain_or_star(&chain, &tb_chain));

        // 1-hop star (`hub -> l0..l7`) favors LeftToRight deterministically (including the -35 bonus).
        let star = lower(&parse(
            "hub -> l0; hub -> l1; hub -> l2; hub -> l3; hub -> l4; hub -> l5; hub -> l6; hub -> l7;",
        ));
        let anns: Vec<Vec<Annotation>> = star
            .nodes
            .iter()
            .map(|n| effective_annotations(&star, &n.id, &n.annotations, opts.variant.as_deref()))
            .collect();
        let locks = parse_pin_locks(&anns, &opts).unwrap();
        let base_sizes = compute_base_sizes(&star, &locks, &opts).unwrap();
        let plans = crate::topology::candidates(&star, &base_sizes, &opts);
        assert!(!plans.is_empty());

        let mut lr_plan = plans[0].clone();
        lr_plan.axis = FlowAxis::LeftToRight;
        lr_plan.fold_row = vec![0; star.nodes.len()];
        assert!(candidate_is_flat_chain_or_star(&star, &lr_plan));

        let mut tb_plan = lr_plan.clone();
        tb_plan.axis = FlowAxis::TopToBottom;
        assert!(candidate_is_flat_chain_or_star(&star, &tb_plan));

        let lr_out = solve_candidate_layout(&CandidateInput {
            scene: &star,
            options: &opts,
            annotations: &anns,
            locks: &locks,
            base_sizes: &base_sizes,
            plan: Some(&lr_plan),
            previous: None,
            spacing: 1.,
        })
        .unwrap()
        .0;
        let tb_out = solve_candidate_layout(&CandidateInput {
            scene: &star,
            options: &opts,
            annotations: &anns,
            locks: &locks,
            base_sizes: &base_sizes,
            plan: Some(&tb_plan),
            previous: None,
            spacing: 1.,
        })
        .unwrap()
        .0;
        let s_lr = score_candidate_preview(&star, &opts, &lr_plan, &lr_out);
        let s_tb = score_candidate_preview(&star, &opts, &tb_plan, &tb_out);
        assert!(
            s_lr < s_tb,
            "LR 1-hop star score {s_lr} should beat TB {s_tb}"
        );

        let mut folded_plan = lr_plan.clone();
        if folded_plan.fold_row.len() >= 2 {
            folded_plan.fold_row[1] = 1;
        }
        assert!(!candidate_is_flat_chain_or_star(&star, &folded_plan));
        let s_folded = score_candidate_preview(&star, &opts, &folded_plan, &lr_out);
        assert!(
            ((s_folded - s_lr) - 35.0).abs() < 1e-6,
            "flat chain/star should receive exact 35.0 LR bonus: lr={s_lr} no_bonus={s_folded}"
        );

        let row_scene = lower(&parse("a; b; c; align row [a, b, c];"));
        assert!(candidate_compatible_with_relations(&row_scene, &lr_chain));
        assert!(!candidate_compatible_with_relations(&row_scene, &tb_chain));

        let col_scene = lower(&parse("a; b; c; align col [a, b, c];"));
        assert!(candidate_compatible_with_relations(&col_scene, &tb_chain));
        assert!(!candidate_compatible_with_relations(&col_scene, &lr_chain));
    }

    #[test]
    fn independent_axis_pins_and_relative_container_wall_pins_solve_cleanly() {
        let opts = LayoutOptions::default();
        // Pinning x only leaves y free to align along the shared row.
        let x_only = lower(&parse(
            "a @pin(x: 0g, y: 0g);\nb @pin(x: 20g);\na -> b;\nalign row [a, b];",
        ));
        let xl = layout(&x_only, &opts, None).unwrap();
        assert!((xl.nodes["b"].x - 20. * opts.grid).abs() < 1e-6);
        assert!((xl.nodes["b"].y - xl.nodes["a"].y).abs() < 1e-6);

        // Relative pin against a container's east wall tracks the live expanded wall.
        let rel = lower(&parse(
            "box: Box {\n c1: WideFirstChildLabel\n c2: Second\n c1 -> c2\n}\nsat @pin(x: box.E + 6g, y: box.N + 2g)\nbox.c2 -> sat;",
        ));
        let rl = layout(&rel, &opts, None).unwrap();
        let box_r = rl.nodes["box"];
        let sat_r = rl.nodes["sat"];
        assert!((sat_r.x - (box_r.right() + 6. * opts.grid)).abs() < 1e-4);
        assert!((sat_r.y - (box_r.top() + 2. * opts.grid)).abs() < 1e-4);
    }

    #[test]
    fn size_locks_apply_before_port_growth_and_reject_overflowing_fixed_pos() {
        let opts = LayoutOptions::default();
        let ok = lower(&parse(
            "hub @pin(lock: [w: 10g, h: 6g]) {\n port p1 [side: E, pos: 2g]\n}\nleaf;\nhub#p1 -> leaf;",
        ));
        let ok_l = layout(&ok, &opts, None).unwrap();
        assert!((ok_l.nodes["hub"].w - 10. * opts.grid).abs() < 1e-6);
        assert!((ok_l.nodes["hub"].h - 6. * opts.grid).abs() < 1e-6);

        let overflow = lower(&parse(
            "hub @pin(lock: [w: 6g, h: 4g]) {\n port p1 [side: E, pos: 8g]\n}\nleaf;\nhub#p1 -> leaf;",
        ));
        assert!(layout(&overflow, &opts, None).is_err());

        let corner_ratio = lower(&parse(
            "hub @pin(lock: [w: 6g, h: 4g]) {\n port p1 [side: E, ratio: 0.05]\n}\nleaf;\nhub#p1 -> leaf;",
        ));
        assert!(layout(&corner_ratio, &opts, None).is_err());

        // Compact 3-port face skips >3 Prescale, while 4-port unconstrained face scales perimeter.
        let three = lower(&parse("h;\na; b; c;\nh -> a; h -> b; h -> c;"));
        let four = lower(&parse("h;\na; b; c; d;\nh -> a; h -> b; h -> c; h -> d;"));
        let l3 = layout(&three, &opts, None).unwrap();
        let l4 = layout(&four, &opts, None).unwrap();
        assert!(l4.nodes["h"].h > l3.nodes["h"].h);
    }

    #[test]
    fn fixed_order_ports_preserve_monotonic_order_around_fixed_pos_and_ratio() {
        let opts = LayoutOptions::default();
        let s = lower(&parse(
            "hub @pin(lock: [w: 10g, h: 24g]) {\n port a [side: E, order: 1]\n port p [side: E, pos: 8g]\n port r [side: E, ratio: 0.65]\n port b [side: E, order: 2]\n}\nx1; x2; x3; x4;\nhub#a -> x1;\nhub#p -> x2;\nhub#r -> x3;\nhub#b -> x4;",
        ));
        let l = layout(&s, &opts, None).unwrap();
        let hr = l.nodes["hub"];
        let corner = opts.corner.min(hr.h / 4.);
        let pa = l.ports["hub#a"].point.y;
        let pp = l.ports["hub#p"].point.y;
        let pr = l.ports["hub#r"].point.y;
        let pb = l.ports["hub#b"].point.y;
        assert!(pa + opts.port_spacing <= pb + 1e-6);
        assert!((pp - (hr.top() + 8. * opts.grid)).abs() < 1e-5);
        assert!((pr - (hr.top() + 0.65 * hr.h)).abs() < 1e-5);
        for &y in &[pa, pp, pr, pb] {
            assert!(y >= hr.top() + corner - 1e-5 && y <= hr.bottom() - corner + 1e-5);
        }
    }

    #[test]
    fn scanline_overlapping_pairs_matches_quadratic_oracle() {
        let rects = vec![
            Rect::new(0., 0., 100., 60.),
            Rect::new(80., 20., 60., 40.),
            Rect::new(140., 20., 40., 40.), // touches rect[1] at x = 110 without overlapping
            Rect::new(40., 45., 50., 30.),
            Rect::new(300., 300., 50., 50.),
        ];
        let nodes: Vec<usize> = (0..rects.len()).collect();
        let mut got = scanline_overlapping_pairs(&rects, &nodes);
        got.sort_unstable();
        let mut expected = Vec::new();
        for i in 0..rects.len() {
            for j in (i + 1)..rects.len() {
                if rects[i].intersects(rects[j]) {
                    expected.push((i, j));
                }
            }
        }
        assert_eq!(got, expected);
    }

    #[test]
    fn container_header_rect_with_ports_clears_north_ports() {
        let scene = lower(&parse("grp: \"Header\" {\n a\n}\next -> grp.N;\n"));
        let grp = scene.node("grp").unwrap();
        let r = Rect::new(200., 100., 320., 180.);
        let base = container_header_rect(grp, r).unwrap();

        let empty_ports = BTreeMap::new();
        assert_eq!(container_header_rect_with_ports(grp, r, &empty_ports), Some(base));

        let mut ports = BTreeMap::new();
        ports.insert(
            "ext->grp:target".into(),
            PortPlacement {
                point: Point::new(base.x, r.top()),
                stub: Point::new(base.x, r.top() - 16.),
                side: Side::N,
                key: "ext->grp:target".into(),
            },
        );
        let shifted = container_header_rect_with_ports(grp, r, &ports).unwrap();
        assert!(shifted.left() >= base.x + 12. - 1e-6);
        assert!(shifted.right() <= r.right() - 12. + 1e-6);
    }

    #[test]
    fn named_duplex_ports_keep_leaf_proportions_and_wide_corridor() {
        let src = include_str!("../tests/fixtures/quality/10-many-named-ports.diagram");
        let scene = lower(&parse(src));
        let opts = LayoutOptions::default();
        let l = layout(&scene, &opts, None).unwrap();
        let widths: Vec<f64> = (0..4).map(|i| l.nodes[&format!("p{i}")].w).collect();
        let max_w = widths.iter().copied().fold(0., f64::max);
        let min_w = widths.iter().copied().fold(f64::INFINITY, f64::min);
        assert!(max_w / min_w < 1.15, "leaf width ratio: {:?}", widths);
        for i in 0..4 {
            let r = l.nodes[&format!("p{i}")];
            assert!(r.w > r.h, "expected leaf-proportioned rect for p{i}: {r:?}");
        }
        let q = l.nodes["queue"];
        let p0 = l.nodes["p0"];
        let corridor = q.left() - p0.right();
        assert!(
            corridor >= 180.,
            "expected widened named-port corridor >= 180px, got {corridor} (p0={p0:?}, q={q:?})"
        );
    }

    #[test]
    fn pair_bundle_and_hinted_facing_align_in_stage1b() {
        let opts = LayoutOptions::default();
        // Parallel bundle (a <-> b with >= 2 edges) aligns rows even when b also has
        // an external incoming edge from a lower rank/row.
        let bundle_src = "u -> v -> y;\nroot -> a;\na -> b: m1;\na -> b: m2;\na -> b: m3;\ny -> b;";
        let bundle_scene = lower(&parse(bundle_src));
        let bundle_layout = layout(&bundle_scene, &opts, None).unwrap();
        let ra = bundle_layout.nodes["a"];
        let rb = bundle_layout.nodes["b"];
        assert!(
            (ra.y - rb.y).abs() < 1e-2,
            "pair_bundle should align rows: a={ra:?}, b={rb:?}"
        );

        // Hinted facing S -> N on a folded bypass aligns x even when deg >= 3 and desired x differs.
        let hint_src = "n0; n1; n2; n3; n4; s0; s1;\nn0 -> n1; n1 -> n2; n2 -> n3; n3 -> n4; n1 -> n4;\ns0 -> n1; n4 -> s1;";
        let scene = lower(&parse(hint_src));
        let annotations: Vec<Vec<Annotation>> = scene
            .nodes
            .iter()
            .map(|nd| effective_annotations(&scene, &nd.id, &nd.annotations, opts.variant.as_deref()))
            .collect();
        let locks = parse_pin_locks(&annotations, &opts).unwrap();
        let base_sizes = compute_base_sizes(&scene, &locks, &opts).unwrap();
        let mut side_hints = vec![None; scene.edges.len()];
        let bypass_idx = scene
            .edges
            .iter()
            .position(|e| e.source.node == "n1" && e.target.node == "n4")
            .unwrap();
        side_hints[bypass_idx] = Some((Side::S, Side::N));
        let plan = crate::topology::TopologyPlan {
            axis: crate::topology::FlowAxis::LeftToRight,
            ranks: vec![0, 1, 2, 2, 1, 0, 2],
            order: vec![0, 0, 0, 1, 1, 1, 2],
            fold_row: vec![0, 0, 0, 1, 1, 0, 1],
            edge_guides: BTreeMap::new(),
            side_hints,
            search_states: 1,
        };
        let input = CandidateInput {
            scene: &scene,
            options: &opts,
            annotations: &annotations,
            locks: &locks,
            base_sizes: &base_sizes,
            plan: Some(&plan),
            previous: None,
            spacing: 1.,
        };
        let (hinted_layout, _, _) = solve_candidate_layout(&input).unwrap();
        let n1 = hinted_layout.nodes["n1"];
        let n4 = hinted_layout.nodes["n4"];
        assert!(
            (n1.x - n4.x).abs() < 1e-2,
            "hinted_facing (S, N) should align n1.x == n4.x: n1={n1:?}, n4={n4:?}"
        );
    }

    #[test]
    fn edge_guides_widen_rank_corridor_gaps_in_build_plan_rects() {
        let opts = LayoutOptions::default();
        let scene = lower(&parse("a -> b -> c; d -> b;"));
        let sizes = vec![Point::new(100., 40.); scene.nodes.len()];
        let mut plain = crate::topology::TopologyPlan {
            axis: crate::topology::FlowAxis::LeftToRight,
            ranks: vec![0, 1, 2, 0],
            order: vec![0, 0, 0, 2],
            fold_row: vec![0; scene.nodes.len()],
            edge_guides: BTreeMap::new(),
            side_hints: Vec::new(),
            search_states: 0,
        };
        let rects_plain = build_plan_rects(&scene, &sizes, &plain, &opts, 1.);
        plain.edge_guides.insert("g0".into(), vec![(0, 1.0)]);
        plain.edge_guides.insert("g1".into(), vec![(0, 1.25)]);
        plain.edge_guides.insert("g2".into(), vec![(0, 1.5)]);
        let rects_guided = build_plan_rects(&scene, &sizes, &plain, &opts, 1.);
        let gap_plain = rects_plain[3].top() - rects_plain[0].bottom();
        let gap_guided = rects_guided[3].top() - rects_guided[0].bottom();
        assert!(
            gap_guided >= gap_plain + 2. * opts.lane_spacing - 1e-6,
            "expected guided gap >= plain + 2*lane_spacing, got plain={gap_plain}, guided={gap_guided}"
        );
    }

    #[test]
    fn cross_fold_parallel_bundle_does_not_collapse_fold_rows() {
        let opts = LayoutOptions::default();
        let src = "a -> b -> c: validate access;\nb -> d: validate access;\nb -> d: retry;\nc -> d;";
        let scene = lower(&parse(src));
        let annotations: Vec<Vec<Annotation>> = scene
            .nodes
            .iter()
            .map(|nd| effective_annotations(&scene, &nd.id, &nd.annotations, opts.variant.as_deref()))
            .collect();
        let locks = parse_pin_locks(&annotations, &opts).unwrap();
        let base_sizes = compute_base_sizes(&scene, &locks, &opts).unwrap();
        let mut side_hints = vec![None; scene.edges.len()];
        for (idx, e) in scene.edges.iter().enumerate() {
            if e.source.node == "b" && e.target.node == "d" {
                side_hints[idx] = Some((Side::E, Side::W));
            }
        }
        let plan = crate::topology::TopologyPlan {
            axis: crate::topology::FlowAxis::LeftToRight,
            ranks: vec![0, 1, 1, 2],
            order: vec![0, 0, 1, 1],
            fold_row: vec![0, 0, 1, 1],
            edge_guides: BTreeMap::new(),
            side_hints,
            search_states: 1,
        };
        let input = CandidateInput {
            scene: &scene,
            options: &opts,
            annotations: &annotations,
            locks: &locks,
            base_sizes: &base_sizes,
            plan: Some(&plan),
            previous: None,
            spacing: 1.,
        };
        let expected_gap = (2. * opts.stub + 2. * opts.margin + 48.).max(96.) + 28.;
        let built = build_plan_rects(&scene, &base_sizes, &plan, &opts, 1.);
        assert!(
            built[3].top() - built[1].bottom() >= expected_gap - 1e-3,
            "build_plan_rects should reserve fold gap >= {expected_gap}, got b={:?}, d={:?}",
            built[1],
            built[3]
        );
        let (out, _, _) = solve_candidate_layout(&input).unwrap();
        let rb = out.nodes["b"];
        let rd = out.nodes["d"];
        assert!(
            (rd.y - rb.y).abs() >= expected_gap + rb.h * 0.5,
            "cross-fold parallel bundle should not collapse b and d rows: b={rb:?}, d={rd:?}"
        );
    }

    #[test]
    fn labeled_parallel_bundle_reserves_wide_polygon_corridor() {
        let opts = LayoutOptions::default();
        let src = "a [shape: diamond]; b [shape: cache];\na -> b: retry;\na -> b: ack;\na -> b: read through;";
        let scene = lower(&parse(src));
        let out = layout(&scene, &opts, None).unwrap();
        let ra = out.nodes["a"];
        let rb = out.nodes["b"];
        let req = parallel_bundle_corridor_width(&scene, ra, rb, 0, 1, true, &opts).unwrap();
        assert!(
            rb.left() - ra.right() >= req - 1e-3,
            "expected corridor gap >= {req}, got a={ra:?}, b={rb:?}"
        );
    }
}
