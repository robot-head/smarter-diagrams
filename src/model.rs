//! Scoped relational scenegraph. Cross-cutting operators never change node ownership.
use crate::syntax::{self, Kind, Parse, Span, SyntaxNode, Token};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub type NodeId = String;
pub type EdgeId = String;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub span: Span,
    pub message: String,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Value {
    pub text: String,
    pub span: Span,
}
pub type Attributes = BTreeMap<String, Value>;
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    pub name: String,
    pub attrs: Attributes,
    pub span: Span,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SourceOrigin {
    pub primary_decl: Span,
    pub inline_pin: Option<Span>,
    pub layout_rule: Option<Span>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Side {
    N,
    E,
    S,
    W,
}
impl Side {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "N" => Some(Self::N),
            "E" => Some(Self::E),
            "S" => Some(Self::S),
            "W" => Some(Self::W),
            _ => None,
        }
    }
    pub fn normal(self) -> (f64, f64) {
        match self {
            Self::N => (0., -1.),
            Self::E => (1., 0.),
            Self::S => (0., 1.),
            Self::W => (-1., 0.),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub enum PortConstraint {
    #[default]
    Free,
    FixedSide {
        side: Side,
    },
    FixedOrder {
        side: Side,
        order: u32,
    },
    FixedRatio {
        side: Side,
        ratio: f64,
    },
    FixedPos {
        side: Side,
        offset_grid: f64,
    },
}
impl PortConstraint {
    pub fn tier(&self) -> u8 {
        match self {
            Self::Free => 0,
            Self::FixedSide { .. } => 1,
            Self::FixedOrder { .. } => 2,
            Self::FixedRatio { .. } => 3,
            Self::FixedPos { .. } => 4,
        }
    }
    pub fn side(&self) -> Option<Side> {
        match self {
            Self::Free => None,
            Self::FixedSide { side }
            | Self::FixedOrder { side, .. }
            | Self::FixedRatio { side, .. }
            | Self::FixedPos { side, .. } => Some(*side),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Port {
    pub explicit: bool,
    pub id: String,
    pub label: String,
    pub constraint: PortConstraint,
    pub origin: Span,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub label: String,
    pub parent: Option<NodeId>,
    pub shape: String,
    pub container: bool,
    pub ports: BTreeMap<String, Port>,
    pub attrs: Attributes,
    pub annotations: Vec<Annotation>,
    pub origin: SourceOrigin,
    pub declaration_index: usize,
    pub explicit: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    pub raw_constraint: PortConstraint,
    pub node: NodeId,
    pub port: Option<String>,
    pub constraint: PortConstraint,
    pub span: Span,
    pub anchor_span: Option<Span>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeKind {
    Directed,
    Bidirectional,
    Undirected,
    Bold,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: EdgeId,
    pub source: Endpoint,
    pub target: Endpoint,
    pub label: String,
    pub kind: EdgeKind,
    pub attrs: Attributes,
    pub annotations: Vec<Annotation>,
    pub origin: SourceOrigin,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    Row,
    Col,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub axis: Axis,
    pub members: Vec<NodeId>,
    pub distribute: bool,
    pub gap_grid: Option<f64>,
    pub origin: Span,
    pub list_span: Span,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Zone {
    pub id: String,
    pub label: String,
    pub members: Vec<NodeId>,
    pub attrs: Attributes,
    pub origin: Span,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flow {
    pub id: String,
    pub label: String,
    pub steps: Vec<NodeId>,
    pub edges: Vec<EdgeId>,
    pub origin: Span,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutRule {
    pub target: String,
    pub annotations: Vec<Annotation>,
    pub span: Span,
    pub variant: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct View {
    pub id: String,
    pub include: Vec<String>,
    pub origin: Span,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SceneGraph {
    #[serde(skip)]
    pub node_index: BTreeMap<NodeId, usize>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub zones: Vec<Zone>,
    pub relations: Vec<Relation>,
    pub flows: Vec<Flow>,
    pub layout_rules: Vec<LayoutRule>,
    pub diagnostics: Vec<Diagnostic>,
    pub views: Vec<View>,
    pub strata: Vec<String>,
    pub story: Vec<NodeId>,
}
impl SceneGraph {
    pub fn index(&self, id: &str) -> Option<usize> {
        if let Some(&i) = self.node_index.get(id)
            && self.nodes.get(i).is_some_and(|n| n.id == id)
        {
            return Some(i);
        }
        self.nodes.iter().position(|n| n.id == id)
    }
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.index(id).map(|i| &self.nodes[i])
    }
    pub fn edge(&self, id: &str) -> Option<&Edge> {
        self.edges.iter().find(|e| e.id == id)
    }
    pub fn reindex(&mut self) {
        self.node_index = self
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
    }
}

pub fn number(s: &str) -> Option<f64> {
    s.trim()
        .trim_end_matches('g')
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|x| x.is_finite())
}
pub fn value(attrs: &Attributes, k: &str) -> Option<f64> {
    attrs.get(k).and_then(|v| number(&v.text))
}
fn unquote(s: &str) -> String {
    serde_json::from_str::<String>(s).unwrap_or_else(|_| s.to_owned())
}
fn tokens(source: &str, span: Span) -> Vec<Token> {
    syntax::lex(span.text(source))
        .into_iter()
        .filter(|t| {
            !matches!(
                t.kind,
                Kind::Whitespace | Kind::Newline | Kind::Comment | Kind::Semicolon
            )
        })
        .map(|mut t| {
            t.span.start += span.start;
            t.span.end += span.start;
            t
        })
        .collect()
}
fn matching(ts: &[Token], at: usize) -> usize {
    let (open, close) = match ts[at].kind {
        Kind::LBracket => (Kind::LBracket, Kind::RBracket),
        Kind::LParen => (Kind::LParen, Kind::RParen),
        Kind::LBrace => (Kind::LBrace, Kind::RBrace),
        _ => return at,
    };
    let mut d = 0;
    for (i, t) in ts.iter().enumerate().skip(at) {
        if t.kind == open {
            d += 1;
        }
        if t.kind == close {
            d -= 1;
            if d == 0 {
                return i;
            }
        }
    }
    ts.len().saturating_sub(1)
}
pub(crate) fn attributes(source: &str, ts: &[Token]) -> Attributes {
    let mut out = Attributes::new();
    let mut i = 0;
    while i < ts.len() {
        if ts[i].kind == Kind::Comma {
            i += 1;
            continue;
        }
        let key = ts[i].text(source).to_string();
        let start = i;
        i += 1;
        if ts.get(i).is_none_or(|t| t.kind != Kind::Colon) {
            out.insert(
                key,
                Value {
                    text: "true".into(),
                    span: ts[start].span,
                },
            );
            continue;
        }
        i += 1;
        let begin = i;
        let mut depth = 0;
        while i < ts.len() {
            match ts[i].kind {
                Kind::LParen | Kind::LBracket => depth += 1,
                Kind::RParen | Kind::RBracket => depth -= 1,
                Kind::Comma if depth == 0 => {
                    if i + 2 < ts.len() && ts[i + 2].kind == Kind::Colon {
                        break;
                    }
                    if i + 1 == ts.len() {
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        if begin < i {
            let span = Span::new(ts[begin].span.start, ts[i - 1].span.end);
            out.insert(
                key,
                Value {
                    text: span.text(source).to_string(),
                    span,
                },
            );
        }
    }
    out
}
pub(crate) fn annotations(source: &str, ts: &[Token]) -> Vec<Annotation> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 2 < ts.len() {
        if ts[i].kind == Kind::At && ts[i + 1].kind == Kind::Ident && ts[i + 2].kind == Kind::LParen
        {
            let end = matching(ts, i + 2);
            out.push(Annotation {
                name: ts[i + 1].text(source).into(),
                attrs: attributes(source, &ts[i + 3..end]),
                span: Span::new(ts[i].span.start, ts[end].span.end),
            });
            i = end + 1;
        } else {
            i += 1;
        }
    }
    out
}
fn port_spec(attrs: &Attributes, side: Option<Side>) -> PortConstraint {
    let side = attrs
        .get("side")
        .and_then(|v| Side::parse(&v.text))
        .or(side);
    match side {
        None => PortConstraint::Free,
        Some(side) => {
            if let Some(v) = value(attrs, "pos") {
                PortConstraint::FixedPos {
                    side,
                    offset_grid: v,
                }
            } else if let Some(v) = value(attrs, "ratio") {
                PortConstraint::FixedRatio { side, ratio: v }
            } else if let Some(v) = value(attrs, "order") {
                PortConstraint::FixedOrder {
                    side,
                    order: v.max(0.) as u32,
                }
            } else {
                PortConstraint::FixedSide { side }
            }
        }
    }
}
struct Lower<'a> {
    source: &'a str,
    scene: SceneGraph,
    pending: Vec<(SyntaxNode, String)>,
    edge_counts: BTreeMap<String, usize>,
}
pub fn lower(parse: &Parse) -> SceneGraph {
    let mut l = Lower {
        source: &parse.source,
        scene: SceneGraph::default(),
        pending: Vec::new(),
        edge_counts: BTreeMap::new(),
    };
    for (span, msg) in &parse.errors {
        l.error(*span, msg.clone());
    }
    if !parse.errors.is_empty() {
        return l.scene;
    }
    l.walk(&parse.syntax(), "");
    for i in 0..l.scene.nodes.len() {
        if l.scene.nodes[i].parent.is_none()
            && let Some((parent, _)) = l.scene.nodes[i].id.rsplit_once('.')
            && let Some(p) = l.scene.index(parent)
        {
            let parent = parent.to_string();
            l.scene.nodes[p].container = true;
            l.scene.nodes[i].parent = Some(parent);
        }
    }

    let pending = std::mem::take(&mut l.pending);
    for (stmt, scope) in &pending {
        if stmt.kind() == Kind::EdgeStmt {
            l.edge_stmt(stmt, scope);
        }
    }
    for (stmt, scope) in &pending {
        match stmt.kind() {
            Kind::ZoneDecl => l.zone(stmt, scope),
            Kind::AlignDecl | Kind::DistributeDecl => l.relation(stmt, scope),
            Kind::FlowDecl => l.flow(stmt, scope),
            Kind::LayoutBlock => l.layout_block(stmt, scope),
            Kind::ViewDecl => l.view(stmt, scope),
            Kind::StrataDecl => l.strata(stmt),
            Kind::StoryDecl => l.story(stmt, scope),
            _ => {}
        }
    }
    // A side-qualified named endpoint declares a virtual port by use; record slots still
    // require explicit declarations. Collect all such definitions before resolving uses.
    let virtual_ports: Vec<_> = l
        .scene
        .edges
        .iter()
        .flat_map(|e| [&e.source, &e.target])
        .filter(|ep| ep.port.is_some() && ep.constraint.side().is_some())
        .cloned()
        .collect();
    for ep in virtual_ports {
        let name = ep.port.unwrap();
        let i = l.scene.index(&ep.node).unwrap();
        l.scene.nodes[i].ports.entry(name.clone()).or_insert(Port {
            explicit: false,
            id: name.clone(),
            label: name,
            constraint: ep.constraint,
            origin: ep.span,
        });
    }
    let uses: Vec<_> = l
        .scene
        .edges
        .iter()
        .flat_map(|e| [&e.source, &e.target])
        .filter(|ep| ep.port.is_some())
        .cloned()
        .collect();
    for ep in uses {
        let name = ep.port.as_ref().unwrap();
        let index = l.scene.index(&ep.node).unwrap();
        if let Some(port) = l.scene.nodes[index].ports.get(name) {
            let declared = port.constraint.clone();
            let raw = &ep.raw_constraint;
            if declared.side().is_some() && raw.side().is_some() && declared.side() != raw.side()
                || declared.tier() == raw.tier() && declared != *raw
            {
                l.error(
                    ep.span,
                    format!("conflicting constraints for {}#{}", ep.node, name),
                );
            } else if raw.tier() > declared.tier() {
                l.scene.nodes[index].ports.get_mut(name).unwrap().constraint = raw.clone();
            }
        }
    }
    // Resolve named ports after declarations and implicit nodes are known.
    for i in 0..l.scene.edges.len() {
        for source in [true, false] {
            let ep = if source {
                &l.scene.edges[i].source
            } else {
                &l.scene.edges[i].target
            };
            if let Some(name) = &ep.port {
                let port = l
                    .scene
                    .node(&ep.node)
                    .and_then(|n| n.ports.get(name))
                    .cloned();
                if let Some(port) = port {
                    let ep = if source {
                        &mut l.scene.edges[i].source
                    } else {
                        &mut l.scene.edges[i].target
                    };
                    ep.constraint = port.constraint;
                } else {
                    l.error(ep.span, format!("unknown port {}#{}", ep.node, name));
                }
            }
        }
    }
    let specs: Vec<_> = l
        .scene
        .edges
        .iter()
        .flat_map(|e| {
            [
                (&e.source.constraint, e.source.span),
                (&e.target.constraint, e.target.span),
            ]
        })
        .chain(
            l.scene
                .nodes
                .iter()
                .flat_map(|n| n.ports.values().map(|p| (&p.constraint, p.origin))),
        )
        .map(|(s, r)| (s.clone(), r))
        .collect();
    for (spec, span) in specs {
        match spec {
            PortConstraint::FixedRatio { ratio, .. } if !(0. ..=1.).contains(&ratio) => {
                l.error(span, "port ratio must be in [0, 1]".into())
            }
            PortConstraint::FixedPos { offset_grid, .. } if offset_grid < 0. => {
                l.error(span, "port offset must be non-negative".into())
            }
            _ => {}
        }
    }
    l.scene
}

/// Reuse resolved topology for an isolated, unadorned leaf declaration edit.
/// Parse both declarations to prove that only its label/trivia changed; all
/// structural edits and declarations with attributes/ports take the full path.
pub(crate) fn lower_incremental(
    parse: &Parse,
    previous: &Parse,
    scene: &SceneGraph,
    range: Span,
    inserted: &str,
) -> SceneGraph {
    fn isolated(parse: &Parse) -> Option<Node> {
        let graph = lower(parse);
        if !graph.diagnostics.is_empty()
            || graph.nodes.len() != 1
            || !graph.edges.is_empty()
            || !graph.zones.is_empty()
            || !graph.relations.is_empty()
            || !graph.flows.is_empty()
            || !graph.views.is_empty()
            || !graph.strata.is_empty()
            || !graph.story.is_empty()
            || !graph.layout_rules.is_empty()
        {
            return None;
        }
        let node = graph.nodes.into_iter().next()?;
        (node.explicit
            && !node.container
            && node.ports.is_empty()
            && node.attrs.is_empty()
            && node.annotations.is_empty())
        .then_some(node)
    }
    let reuse = || -> Option<SceneGraph> {
        // Limit reuse to plain label characters. Quotes, comment delimiters,
        // statement separators and line breaks can change adjacent syntax.
        let plain = |text: &str| {
            text.chars()
                .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '_'))
        };
        if !parse.errors.is_empty()
            || !scene.diagnostics.is_empty()
            || !plain(inserted)
            || !plain(range.text(&previous.source))
        {
            return None;
        }
        let (index, node) = scene.nodes.iter().enumerate().find(|(_, n)| {
            n.explicit
                && !n.container
                && n.ports.is_empty()
                && n.attrs.is_empty()
                && n.annotations.is_empty()
                && n.origin.primary_decl.start < range.start
                && n.origin.primary_decl.end >= range.end
        })?;
        let span = node.origin.primary_decl;
        let old_text = span.text(&previous.source);
        let header = tokens(old_text, Span::new(0, old_text.len()));
        let colon = header.iter().position(|t| t.kind == Kind::Colon)?;
        let label = Span::new(header.get(colon + 1)?.span.start, header.last()?.span.end);
        if !plain(label.text(old_text))
            || range.start < span.start + label.start
            || range.end > span.start + label.end
        {
            return None;
        }
        let old = isolated(&syntax::parse(old_text))?;
        let mut new_text = old_text.to_owned();
        new_text.replace_range(range.start - span.start..range.end - span.start, inserted);
        let new = isolated(&syntax::parse(&new_text))?;
        if old.id != new.id
            || old.label != node.label
            || old.shape != node.shape
            || old.shape != new.shape
        {
            return None;
        }
        let mut next = scene.clone();
        next.nodes[index].label = new.label;
        shift_origins(&mut next, range, inserted.len());
        Some(next)
    };
    reuse().unwrap_or_else(|| lower(parse))
}

fn shift_origins(scene: &mut SceneGraph, range: Span, inserted: usize) {
    let delta = inserted as isize - (range.end - range.start) as isize;
    let shift = |span: &mut Span| {
        if span.start >= range.end {
            span.start = span.start.saturating_add_signed(delta);
            span.end = span.end.saturating_add_signed(delta);
        } else if span.end > range.start {
            span.end = span.end.saturating_add_signed(delta);
        }
    };
    let origin = |o: &mut SourceOrigin| {
        shift(&mut o.primary_decl);
        if let Some(span) = &mut o.inline_pin {
            shift(span);
        }
        if let Some(span) = &mut o.layout_rule {
            shift(span);
        }
    };
    let attrs = |attrs: &mut Attributes| {
        for value in attrs.values_mut() {
            shift(&mut value.span);
        }
    };
    let annotations = |annotations: &mut Vec<Annotation>| {
        for annotation in annotations {
            shift(&mut annotation.span);
            attrs(&mut annotation.attrs);
        }
    };
    for node in &mut scene.nodes {
        origin(&mut node.origin);
        attrs(&mut node.attrs);
        annotations(&mut node.annotations);
        for port in node.ports.values_mut() {
            shift(&mut port.origin);
        }
    }
    for edge in &mut scene.edges {
        origin(&mut edge.origin);
        attrs(&mut edge.attrs);
        annotations(&mut edge.annotations);
        for endpoint in [&mut edge.source, &mut edge.target] {
            shift(&mut endpoint.span);
            if let Some(span) = &mut endpoint.anchor_span {
                shift(span);
            }
        }
    }
    for zone in &mut scene.zones {
        shift(&mut zone.origin);
        attrs(&mut zone.attrs);
    }
    for relation in &mut scene.relations {
        shift(&mut relation.origin);
        shift(&mut relation.list_span);
    }
    for flow in &mut scene.flows {
        shift(&mut flow.origin);
    }
    for rule in &mut scene.layout_rules {
        shift(&mut rule.span);
        annotations(&mut rule.annotations);
    }
    for view in &mut scene.views {
        shift(&mut view.origin);
    }
    for diagnostic in &mut scene.diagnostics {
        shift(&mut diagnostic.span);
    }
}

impl Lower<'_> {
    fn error(&mut self, span: Span, message: String) {
        self.scene.diagnostics.push(Diagnostic {
            severity: Severity::Error,
            span,
            message,
        });
    }
    fn resolve(&self, id: &str, scope: &str) -> String {
        let mut prefix = scope;
        loop {
            let q = if prefix.is_empty() {
                id.into()
            } else {
                format!("{prefix}.{id}")
            };
            if self.scene.node_index.contains_key(&q) {
                return q;
            }
            if let Some((p, _)) = prefix.rsplit_once('.') {
                prefix = p;
            } else if !prefix.is_empty() {
                prefix = "";
            } else {
                break;
            }
        }
        if scope.is_empty() || id.contains('.') {
            id.into()
        } else {
            format!("{scope}.{id}")
        }
    }
    fn ensure(&mut self, id: &str, scope: &str, span: Span) -> String {
        let id = self.resolve(id, scope);
        if !self.scene.node_index.contains_key(&id) {
            let parent = id
                .rsplit_once('.')
                .map(|(p, _)| p.to_string())
                .filter(|p| self.scene.node_index.contains_key(p));
            let label = id.rsplit('.').next().unwrap().to_string();
            let shape = infer_shape(&label);
            self.scene
                .node_index
                .insert(id.clone(), self.scene.nodes.len());
            self.scene.nodes.push(Node {
                id: id.clone(),
                label,
                parent,
                shape,
                container: false,
                ports: BTreeMap::new(),
                attrs: Attributes::new(),
                annotations: Vec::new(),
                origin: SourceOrigin {
                    primary_decl: span,
                    ..Default::default()
                },
                declaration_index: self.scene.nodes.len(),
                explicit: false,
            });
        }
        id
    }
    fn header(&self, stmt: &SyntaxNode) -> Vec<Token> {
        let span = Span::from(stmt.text_range());
        let ts = tokens(self.source, span);
        let mut d = 0;
        let end = ts
            .iter()
            .position(|t| {
                match t.kind {
                    Kind::LParen | Kind::LBracket => d += 1,
                    Kind::RParen | Kind::RBracket => d -= 1,
                    _ => {}
                }
                t.kind == Kind::LBrace && d == 0
            })
            .unwrap_or(ts.len());
        ts[..end].to_vec()
    }
    fn walk(&mut self, root: &SyntaxNode, scope: &str) {
        for stmt in root.children() {
            match stmt.kind() {
                Kind::NodeDecl => self.node_decl(&stmt, scope),
                Kind::PortDecl => self.port_decl(&stmt, scope),
                Kind::EdgeStmt
                | Kind::ZoneDecl
                | Kind::AlignDecl
                | Kind::DistributeDecl
                | Kind::FlowDecl
                | Kind::LayoutBlock
                | Kind::ViewDecl
                | Kind::StrataDecl
                | Kind::StoryDecl => self.pending.push((stmt, scope.into())),
                _ => {}
            }
        }
    }
    fn node_decl(&mut self, stmt: &SyntaxNode, scope: &str) {
        let ts = self.header(stmt);
        if ts.is_empty() {
            return;
        }
        let (raw, mut i) = qualified(self.source, &ts, 0);
        if raw.is_empty() {
            self.error(ts[0].span, "expected node identifier".into());
            return;
        }
        let id = if scope.is_empty() {
            raw
        } else {
            format!("{scope}.{raw}")
        };
        let mut label = id.rsplit('.').next().unwrap().to_string();
        if ts.get(i).is_some_and(|t| t.kind == Kind::Colon) {
            i += 1;
            let begin = i;
            while i < ts.len() && !matches!(ts[i].kind, Kind::LBracket | Kind::At) {
                i += 1;
            }
            if begin < i {
                label = unquote(
                    Span::new(ts[begin].span.start, ts[i - 1].span.end)
                        .text(self.source)
                        .trim(),
                );
            }
        }
        let mut attrs = Attributes::new();
        if ts.get(i).is_some_and(|t| t.kind == Kind::LBracket) {
            let end = matching(&ts, i);
            attrs = attributes(self.source, &ts[i + 1..end]);
        }
        let anns = annotations(self.source, &ts);
        let span = Span::from(stmt.text_range());
        let has_owned = stmt
            .children()
            .any(|n| matches!(n.kind(), Kind::NodeDecl | Kind::EdgeStmt));
        let has_ports = stmt.children().any(|n| n.kind() == Kind::PortDecl);
        let has_block = stmt
            .children_with_tokens()
            .any(|e| e.kind() == Kind::LBrace);
        let container = has_owned || has_block && !has_ports;
        let shape = attrs
            .get("shape")
            .map(|v| unquote(&v.text))
            .unwrap_or_else(|| infer_shape(&id));
        let node = Node {
            id: id.clone(),
            label,
            parent: (!scope.is_empty()).then(|| scope.to_owned()),
            shape,
            container,
            ports: BTreeMap::new(),
            attrs,
            origin: SourceOrigin {
                primary_decl: span,
                inline_pin: anns.iter().find(|a| a.name == "pin").map(|a| a.span),
                layout_rule: None,
            },
            annotations: anns,
            declaration_index: self.scene.nodes.len(),
            explicit: true,
        };
        if let Some(index) = self.scene.index(&id) {
            if self.scene.nodes[index].explicit {
                self.error(span, format!("duplicate node {id}"));
            } else {
                self.scene.nodes[index] = node;
            }
        } else {
            self.scene
                .node_index
                .insert(node.id.clone(), self.scene.nodes.len());
            self.scene.nodes.push(node);
        }
        self.walk(stmt, &id);
    }
    fn validate_port_attrs(&mut self, attrs: &Attributes) {
        if let Some(v) = attrs.get("side")
            && Side::parse(&v.text).is_none()
        {
            self.error(v.span, "port side must be N, E, S, or W".into());
        }
        for key in ["order", "ratio", "pos"] {
            if let Some(v) = attrs.get(key) {
                let valid = number(&v.text).is_some_and(|n| match key {
                    "order" => n >= 0. && n.fract() == 0. && n <= u32::MAX as f64,
                    "ratio" => n > 0. && n < 1.,
                    _ => n >= 0.,
                });
                if !valid {
                    self.error(v.span, format!("invalid port {key}"));
                }
            }
        }
    }
    fn port_decl(&mut self, stmt: &SyntaxNode, scope: &str) {
        let ts = self.header(stmt);
        if ts.len() < 2 {
            return;
        }
        if scope.is_empty() {
            self.error(ts[0].span, "port must belong to a node".into());
            return;
        }
        let id = ts[1].text(self.source).to_string();
        let mut attrs = Attributes::new();
        if let Some(i) = ts.iter().position(|t| t.kind == Kind::LBracket) {
            let end = matching(&ts, i);
            attrs = attributes(self.source, &ts[i + 1..end]);
        }
        self.validate_port_attrs(&attrs);
        let label = if ts.get(2).is_some_and(|t| t.kind == Kind::Colon) {
            let end = ts[3..]
                .iter()
                .position(|t| matches!(t.kind, Kind::LBracket | Kind::At))
                .map_or(ts.len(), |i| i + 3);
            format!(
                "{}: {}",
                id,
                if end > 3 {
                    unquote(
                        Span::new(ts[3].span.start, ts[end - 1].span.end)
                            .text(self.source)
                            .trim(),
                    )
                } else {
                    String::new()
                }
            )
        } else {
            id.clone()
        };
        let port = Port {
            explicit: true,
            id: id.clone(),
            label,
            constraint: port_spec(&attrs, None),
            origin: Span::from(stmt.text_range()),
        };
        if let Some(i) = self.scene.index(scope)
            && self.scene.nodes[i].ports.insert(id.clone(), port).is_some()
        {
            self.error(
                Span::from(stmt.text_range()),
                format!("duplicate port {id}"),
            );
        }
    }
    fn endpoint(&mut self, ts: &[Token], scope: &str) -> Option<Endpoint> {
        if ts.is_empty() {
            return None;
        }
        let (raw, mut i) = qualified(self.source, ts, 0);
        if raw.is_empty() {
            return None;
        }
        let mut parts: Vec<_> = raw.split('.').collect();
        let side = if parts.len() > 1 {
            parts.last().and_then(|s| Side::parse(s))
        } else {
            None
        };
        let anchor_start = if side.is_some() {
            parts.pop();
            Some(ts[i - 2].span.start)
        } else {
            None
        };
        let raw = parts.join(".");
        let span = Span::new(ts[0].span.start, ts.last().unwrap().span.end);
        let node = self.ensure(&raw, scope, span);
        let mut spec = side
            .map(|side| PortConstraint::FixedSide { side })
            .unwrap_or_default();
        let mut port = None;
        if ts.get(i).is_some_and(|t| t.kind == Kind::At) {
            if let (Some(side), Some(t)) = (side, ts.get(i + 1)) {
                if let Some(ratio) = number(t.text(self.source)) {
                    spec = PortConstraint::FixedRatio { side, ratio };
                } else {
                    self.error(t.span, "expected ratio".into());
                }
            }
            i += 2;
        }
        if ts.get(i).is_some_and(|t| t.kind == Kind::LParen) {
            let end = matching(ts, i);
            let attrs = attributes(self.source, &ts[i + 1..end]);
            self.validate_port_attrs(&attrs);
            spec = port_spec(&attrs, side);
            i = end + 1;
        }
        if ts.get(i).is_some_and(|t| t.kind == Kind::Hash) {
            port = ts.get(i + 1).map(|t| t.text(self.source).into());
            i += 2;
        }
        if i != ts.len() {
            self.error(span, "invalid endpoint suffix".into());
        }
        Some(Endpoint {
            node,
            port,
            raw_constraint: spec.clone(),
            constraint: spec,
            span,
            anchor_span: anchor_start.map(|s| Span::new(s, span.end)),
        })
    }
    fn edge_stmt(&mut self, stmt: &SyntaxNode, scope: &str) {
        let ts = self.header(stmt);
        let mut depth = 0;
        let mut ops = Vec::new();
        let mut suffix = ts.len();
        for (i, t) in ts.iter().enumerate() {
            match t.kind {
                Kind::LParen => depth += 1,
                Kind::RParen => depth -= 1,
                _ => {}
            }
            if depth == 0 {
                if syntax::is_edge(t.kind) {
                    ops.push(i);
                }
                if matches!(t.kind, Kind::Colon | Kind::LBracket)
                    || (t.kind == Kind::At
                        && ts.get(i + 1).is_some_and(|t| t.kind == Kind::Ident)
                        && ts.get(i + 2).is_some_and(|t| t.kind == Kind::LParen))
                {
                    suffix = i;
                    break;
                }
            }
        }
        let mut endpoints = Vec::new();
        let mut begin = 0;
        for &op in &ops {
            if let Some(ep) = self.endpoint(&ts[begin..op], scope) {
                endpoints.push(ep);
            }
            begin = op + 1;
        }
        if let Some(ep) = self.endpoint(&ts[begin..suffix], scope) {
            endpoints.push(ep);
        }
        if endpoints.len() != ops.len() + 1 || ops.is_empty() {
            self.error(
                Span::from(stmt.text_range()),
                "edge requires complete endpoints".into(),
            );
            return;
        }
        let mut label = String::new();
        let mut attrs = Attributes::new();
        let mut i = suffix;
        if ts.get(i).is_some_and(|t| t.kind == Kind::Colon) {
            i += 1;
            let start = i;
            while i < ts.len() && !matches!(ts[i].kind, Kind::LBracket | Kind::At) {
                i += 1;
            }
            if start < i {
                label =
                    unquote(Span::new(ts[start].span.start, ts[i - 1].span.end).text(self.source));
            }
        }
        if ts.get(i).is_some_and(|t| t.kind == Kind::LBracket) {
            let end = matching(&ts, i);
            attrs = attributes(self.source, &ts[i + 1..end]);
        }
        let anns = annotations(self.source, &ts);
        for (j, &op) in ops.iter().enumerate() {
            let from = &endpoints[j];
            let to = &endpoints[j + 1];
            let base = format!("{}->{}", endpoint_key(from), endpoint_key(to));
            let entry = self.edge_counts.entry(base.clone()).or_default();
            let count = *entry;
            *entry += 1;
            let id = if count == 0 {
                base
            } else {
                format!("{base}~{count}")
            };
            let kind = match ts[op].kind {
                Kind::BiArrow => EdgeKind::Bidirectional,
                Kind::Line => EdgeKind::Undirected,
                Kind::BoldArrow => EdgeKind::Bold,
                _ => EdgeKind::Directed,
            };
            self.scene.edges.push(Edge {
                id,
                source: from.clone(),
                target: to.clone(),
                label: label.clone(),
                kind,
                attrs: attrs.clone(),
                annotations: anns.clone(),
                origin: SourceOrigin {
                    primary_decl: Span::from(stmt.text_range()),
                    ..Default::default()
                },
            });
        }
    }
    fn refs(&mut self, ts: &[Token], scope: &str) -> (Vec<String>, Span) {
        let Some(i) = ts.iter().position(|t| t.kind == Kind::LBracket) else {
            return (Vec::new(), Span::default());
        };
        let end = matching(ts, i);
        let mut ids = Vec::new();
        let mut at = i + 1;
        while at < end {
            if ts[at].kind == Kind::Comma {
                at += 1;
                continue;
            }
            let (raw, next) = qualified(self.source, ts, at);
            if next == at {
                self.error(ts[at].span, "invalid reference".into());
                break;
            }
            let id = self.resolve(&raw, scope);
            if !self.scene.node_index.contains_key(&id) {
                self.error(ts[at].span, format!("unknown reference {raw}"));
            }
            ids.push(id);
            at = next;
        }
        (ids, Span::new(ts[i].span.start, ts[end].span.end))
    }
    fn relation(&mut self, stmt: &SyntaxNode, scope: &str) {
        let ts = self.header(stmt);
        let (members, list_span) = self.refs(&ts, scope);
        let axis = if ts.get(1).is_some_and(|t| t.text(self.source) == "col") {
            Axis::Col
        } else {
            Axis::Row
        };
        let mut attrs = Attributes::new();
        if let Some(i) = ts
            .iter()
            .position(|t| t.span.start > list_span.end && t.kind == Kind::LBracket)
        {
            let end = matching(&ts, i);
            attrs = attributes(self.source, &ts[i + 1..end]);
        }
        if members.len() < 2 {
            self.error(list_span, "relation requires at least two members".into());
        }
        self.scene.relations.push(Relation {
            axis,
            members,
            distribute: stmt.kind() == Kind::DistributeDecl,
            gap_grid: value(&attrs, "gap"),
            origin: Span::from(stmt.text_range()),
            list_span,
        });
    }
    fn zone(&mut self, stmt: &SyntaxNode, scope: &str) {
        let ts = self.header(stmt);
        if ts.len() < 2 {
            return;
        }
        let (members, refspan) = self.refs(&ts, scope);
        let id = ts[1].text(self.source).into();
        let label = ts
            .get(2)
            .filter(|t| t.kind == Kind::String)
            .map(|t| unquote(t.text(self.source)))
            .unwrap_or_else(|| ts[1].text(self.source).into());
        let mut attrs = Attributes::new();
        if let Some(i) = ts
            .iter()
            .position(|t| t.kind == Kind::LBracket && t.span.start > refspan.end)
        {
            let end = matching(&ts, i);
            attrs = attributes(self.source, &ts[i + 1..end]);
        }
        for child in stmt.children() {
            let t = self.header(&child);
            if t.len() >= 4
                && t[2].kind == Kind::Colon
                && matches!(t[0].text(self.source), "invariant" | "state")
            {
                let span = Span::new(t[3].span.start, t.last().unwrap().span.end);
                attrs.insert(
                    format!("{}:{}", t[0].text(self.source), t[1].text(self.source)),
                    Value {
                        text: span.text(self.source).into(),
                        span,
                    },
                );
            }
            if t.len() >= 3 && t[1].kind == Kind::Colon {
                let span = Span::new(t[2].span.start, t.last().unwrap().span.end);
                attrs.insert(
                    t[0].text(self.source).into(),
                    Value {
                        text: span.text(self.source).into(),
                        span,
                    },
                );
            }
        }
        self.scene.zones.push(Zone {
            id,
            label,
            members,
            attrs,
            origin: Span::from(stmt.text_range()),
        });
    }
    fn flow(&mut self, stmt: &SyntaxNode, scope: &str) {
        let ts = self.header(stmt);
        if ts.len() < 2 {
            return;
        }
        let mut steps = Vec::new();
        let mut edges = Vec::new();
        for child in stmt.children().filter(|c| c.kind() == Kind::EdgeStmt) {
            let ts = self.header(&child);
            let mut begin = 0;
            let mut eps = Vec::new();
            for i in 0..=ts.len() {
                if i == ts.len() || syntax::is_edge(ts[i].kind) {
                    if let Some(ep) = self.endpoint(&ts[begin..i], scope) {
                        eps.push(ep);
                    }
                    begin = i + 1;
                }
            }
            for pair in eps.windows(2) {
                if let Some(e) = self
                    .scene
                    .edges
                    .iter()
                    .find(|e| e.source.node == pair[0].node && e.target.node == pair[1].node)
                {
                    edges.push(e.id.clone());
                } else {
                    self.error(
                        pair[0].span,
                        "flow step must follow an existing edge".into(),
                    );
                }
            }
            for ep in eps {
                if steps.last() != Some(&ep.node) {
                    steps.push(ep.node);
                }
            }
        }
        let id = ts[1].text(self.source).into();
        let label = ts
            .get(2)
            .filter(|t| t.kind == Kind::String)
            .map(|t| unquote(t.text(self.source)))
            .unwrap_or_else(|| ts[1].text(self.source).into());
        self.scene.flows.push(Flow {
            id,
            label,
            steps,
            edges,
            origin: Span::from(stmt.text_range()),
        });
    }
    fn view(&mut self, stmt: &SyntaxNode, _scope: &str) {
        let ts = self.header(stmt);
        if ts.len() < 2 {
            return;
        }
        let mut include = Vec::new();
        for child in stmt.children() {
            let t = self.header(&child);
            if t.first().is_some_and(|t| t.text(self.source) == "include") && t.len() > 1 {
                include.push(
                    Span::new(t[1].span.start, t.last().unwrap().span.end)
                        .text(self.source)
                        .into(),
                );
            }
        }
        self.scene.views.push(View {
            id: ts[1].text(self.source).into(),
            include,
            origin: Span::from(stmt.text_range()),
        });
    }
    fn strata(&mut self, stmt: &SyntaxNode) {
        let ts = self.header(stmt);
        self.scene.strata = ts
            .iter()
            .skip(1)
            .filter(|t| t.kind == Kind::Ident)
            .map(|t| t.text(self.source).to_string())
            .collect();
    }
    fn story(&mut self, stmt: &SyntaxNode, scope: &str) {
        let ts = self.header(stmt);
        let mut i = 2;
        while i < ts.len() {
            if ts[i].kind == Kind::Ident {
                let (id, end) = qualified(self.source, &ts, i);
                self.scene.story.push(self.resolve(&id, scope));
                i = end;
            } else {
                i += 1;
            }
        }
    }
    fn layout_block(&mut self, stmt: &SyntaxNode, scope: &str) {
        let ts = self.header(stmt);
        let variant = ts
            .iter()
            .position(|t| t.kind == Kind::LParen)
            .and_then(|i| ts.get(i + 1))
            .map(|t| t.text(self.source).to_owned());
        for child in stmt
            .children()
            .filter(|c| matches!(c.kind(), Kind::NodeDecl | Kind::EdgeStmt))
        {
            let ts = self.header(&child);
            let Some(end) = ts.iter().enumerate().position(|(i, t)| {
                t.kind == Kind::At && ts.get(i + 1).is_some_and(|t| t.kind == Kind::Ident)
            }) else {
                continue;
            };
            if end == 0 {
                self.error(
                    Span::from(child.text_range()),
                    "layout rule needs a node or edge target".into(),
                );
                continue;
            }
            let raw = Span::new(ts[0].span.start, ts[end - 1].span.end)
                .text(self.source)
                .trim();
            let target = if raw.starts_with('(') {
                let inner = &ts[1..end - 1];
                if let Some(op) = inner.iter().position(|t| syntax::is_edge(t.kind)) {
                    if let (Some(a), Some(b)) = (
                        self.endpoint(&inner[..op], scope),
                        self.endpoint(&inner[op + 1..], scope),
                    ) {
                        format!("{}->{}", endpoint_key(&a), endpoint_key(&b))
                    } else {
                        raw.to_string()
                    }
                } else {
                    raw.to_string()
                }
            } else {
                self.resolve(raw, scope)
            };
            let anns = annotations(self.source, &ts);
            let span = Span::from(child.text_range());
            if variant.is_none() {
                if let Some(i) = self.scene.index(&target) {
                    self.scene.nodes[i].origin.layout_rule = Some(span);
                } else if self.scene.edge(&target).is_none() {
                    self.error(span, format!("unknown layout target {target}"));
                }
            }
            self.scene.layout_rules.push(LayoutRule {
                target,
                annotations: anns,
                span,
                variant: variant.clone(),
            });
        }
    }
}
fn qualified(source: &str, ts: &[Token], start: usize) -> (String, usize) {
    let mut i = start;
    let mut out = String::new();
    if ts.get(i).is_none_or(|t| t.kind != Kind::Ident) {
        return (out, i);
    }
    out.push_str(ts[i].text(source));
    i += 1;
    while i + 1 < ts.len() && ts[i].kind == Kind::Dot && ts[i + 1].kind == Kind::Ident {
        out.push('.');
        out.push_str(ts[i + 1].text(source));
        i += 2;
    }
    (out, i)
}
fn infer_shape(id: &str) -> String {
    let id = id.to_lowercase();
    if id.contains("postgres") || id.ends_with("db") || id.contains("database") {
        "database"
    } else if id.contains("queue") || id == "dlq" || id.contains("broker") || id.contains("kafka") {
        "queue"
    } else if id.contains("decision") || id.contains("condition") {
        "diamond"
    } else if id.contains("cache") {
        "cache"
    } else if id.contains("client") || id.contains("user") {
        "actor"
    } else {
        "rect"
    }
    .into()
}
pub fn endpoint_key(ep: &Endpoint) -> String {
    match &ep.port {
        Some(p) => format!("{}#{p}", ep.node),
        None => ep.node.clone(),
    }
}
pub fn ancestors(scene: &SceneGraph, id: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut at = scene.node(id).and_then(|n| n.parent.as_deref());
    while let Some(p) = at {
        if !out.insert(p.to_owned()) {
            break;
        }
        at = scene.node(p).and_then(|n| n.parent.as_deref());
    }
    out
}
