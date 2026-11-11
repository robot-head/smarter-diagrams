//! Typed, atomic UTF-8 source edits and semantic canvas gesture synthesis.
use crate::{
    Error,
    layout::{GRID, Layout, Point},
    model::*,
    syntax::{self, Kind, Span},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextPatch {
    pub delete_range: Span,
    pub insert_text: String,
    pub expected_text: Option<String>,
}
impl TextPatch {
    pub fn replace(source: &str, range: Span, text: impl Into<String>) -> Self {
        Self {
            delete_range: range,
            insert_text: text.into(),
            expected_text: Some(range.text(source).into()),
        }
    }
    pub fn insert(at: usize, text: impl Into<String>) -> Self {
        Self {
            delete_range: Span::new(at, at),
            insert_text: text.into(),
            expected_text: Some(String::new()),
        }
    }
}
/// Validate all patches against the same source, then apply from right to left.
pub fn apply_patches(source: &str, patches: &[TextPatch]) -> Result<String, Error> {
    let mut sorted: Vec<_> = patches.iter().collect();
    sorted.sort_by_key(|p| (p.delete_range.start, p.delete_range.end));
    for (i, p) in sorted.iter().enumerate() {
        let r = p.delete_range;
        if r.start > r.end
            || r.end > source.len()
            || !source.is_char_boundary(r.start)
            || !source.is_char_boundary(r.end)
        {
            return Err(Error::Edit("invalid UTF-8 patch range".into()));
        }
        if p.expected_text
            .as_deref()
            .is_some_and(|v| v != r.text(source))
        {
            return Err(Error::Edit("stale patch: expected text differs".into()));
        }
        if i > 0 {
            let prev = sorted[i - 1].delete_range;
            if prev.end > r.start || prev.start == r.start {
                return Err(Error::Edit("overlapping patches".into()));
            }
        }
    }
    let mut result = source.to_owned();
    for p in sorted.into_iter().rev() {
        result.replace_range(p.delete_range.range(), &p.insert_text);
    }
    Ok(result)
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum PinExpr {
    AbsoluteGrid { x: i32, y: i32 },
    Relative { x: String, y: String },
    AxisLock { axis: Axis, coord_grid: i32 },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LabelSlot {
    Above,
    Below,
    Inline,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum DiagramEditOp {
    PinNode {
        node: NodeId,
        pin: PinExpr,
        size_lock: Option<(u32, u32)>,
    },
    SnapAlign {
        axis: Axis,
        members: Vec<NodeId>,
    },
    LockPort {
        node: NodeId,
        port: String,
        spec: PortConstraint,
    },
    NudgeRouteChannel {
        edge: EdgeId,
        segment_idx: usize,
        lane_grid: f64,
    },
    SlideLabel {
        edge: EdgeId,
        slot: LabelSlot,
        t_arc: f64,
    },
    UnpinEntities {
        targets: Vec<NodeId>,
    },
    BreakAlignment {
        node: NodeId,
        axis: Axis,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Placement {
    #[default]
    Auto,
    Inline,
    Layout,
}
#[derive(Debug, Clone)]
pub struct EditResult {
    pub source: String,
    /// Batches are in application order; each batch uses offsets in its preceding source.
    pub batches: Vec<Vec<TextPatch>>,
}
pub fn apply_ops(
    source: &str,
    ops: &[DiagramEditOp],
    placement: Placement,
) -> Result<EditResult, Error> {
    let layout = placement == Placement::Layout
        || placement == Placement::Auto
            && ops
                .iter()
                .filter(|o| matches!(o, DiagramEditOp::PinNode { .. }))
                .count()
                >= 3;
    let mut result = EditResult {
        source: source.into(),
        batches: Vec::new(),
    };
    for op in ops {
        let mut patches = patches_for(&result.source, op, layout)?;
        if let DiagramEditOp::LockPort { node, port, spec } = op {
            let scene = lower(&syntax::parse(&result.source));
            if scene.node(node).is_some_and(|n| n.ports.contains_key(port)) {
                for ep in scene
                    .edges
                    .iter()
                    .flat_map(|e| [&e.source, &e.target])
                    .filter(|ep| ep.node == *node && ep.port.as_ref() == Some(port))
                {
                    let raw = &ep.raw_constraint;
                    if *raw != PortConstraint::Free
                        && (raw.side() != spec.side() || raw.tier() >= spec.tier() && raw != spec)
                        && let Some(span) = ep.anchor_span
                    {
                        patches.push(TextPatch::replace(&result.source, span, format!("#{port}")));
                    }
                }
                patches.sort_by_key(|p| p.delete_range.start);
                patches.dedup_by_key(|p| p.delete_range);
            }
        }

        let next = apply_patches(&result.source, &patches)?;
        let parse = syntax::parse(&next);
        if !parse.errors.is_empty() {
            return Err(Error::Edit(format!(
                "edit produced invalid syntax: {:?}",
                parse.errors
            )));
        }
        result.source = next;
        result.batches.push(patches);
    }
    Ok(result)
}
fn patches_for(source: &str, op: &DiagramEditOp, separate: bool) -> Result<Vec<TextPatch>, Error> {
    let parse = syntax::parse(source);
    let scene = lower(&parse);
    match op {
        DiagramEditOp::PinNode {
            node,
            pin,
            size_lock,
        } => {
            let n = scene
                .node(node)
                .ok_or_else(|| Error::Edit(format!("unknown node {node}")))?;
            let mut attrs = BTreeMap::new();
            match pin {
                PinExpr::AbsoluteGrid { x, y } => {
                    attrs.insert("grid".into(), format!("{x}, {y}"));
                }
                PinExpr::Relative { x, y } => {
                    validate_coordinate(x)?;
                    validate_coordinate(y)?;
                    attrs.insert("x".into(), x.clone());
                    attrs.insert("y".into(), y.clone());
                }
                PinExpr::AxisLock { axis, coord_grid } => {
                    attrs.insert(
                        if *axis == Axis::Row { "y" } else { "x" }.into(),
                        format!("{coord_grid}g"),
                    );
                }
            }
            if let Some((w, h)) = size_lock {
                if *w == 0 || *h == 0 {
                    return Err(Error::Edit("size locks must be positive".into()));
                }
                attrs.insert("lock".into(), format!("[w: {w}g, h: {h}g]"));
            }
            let existing = scene
                .layout_rules
                .iter()
                .rev()
                .find(|r| r.variant.is_none() && r.target == *node)
                .and_then(|r| r.annotations.iter().find(|a| a.name == "pin"))
                .or_else(|| n.annotations.iter().find(|a| a.name == "pin"));
            if let Some(a) = existing {
                return update_annotation(
                    source,
                    a,
                    &attrs,
                    if attrs.contains_key("grid") {
                        &["x", "y"]
                    } else if !attrs.contains_key("y") {
                        &["grid", "y"]
                    } else if !attrs.contains_key("x") {
                        &["grid", "x"]
                    } else {
                        &["grid"]
                    },
                );
            }
            if separate || !n.explicit || n.origin.layout_rule.is_some() {
                upsert_layout(source, &scene, node, "pin", attrs)
            } else {
                let at = inline_insertion(source, n.origin.primary_decl);
                Ok(vec![TextPatch::insert(
                    at,
                    format!(" @pin({})", format_attrs(&attrs)),
                )])
            }
        }
        DiagramEditOp::SnapAlign { axis, members } => {
            let mut unique = members.clone();
            let mut seen = std::collections::BTreeSet::new();
            unique.retain(|id| seen.insert(id.clone()));
            if unique.len() < 2 || unique.iter().any(|id| scene.node(id).is_none()) {
                return Err(Error::Edit(
                    "alignment needs at least two existing nodes".into(),
                ));
            }
            if let Some(r) = scene.relations.iter().find(|r| {
                !r.distribute && r.axis == *axis && unique.iter().any(|id| r.members.contains(id))
            }) {
                let additions: Vec<_> = unique
                    .into_iter()
                    .filter(|id| !r.members.contains(id))
                    .collect();
                if additions.is_empty() {
                    return Ok(Vec::new());
                }
                let at = r.list_span.end - 1;
                let existing = &source[r.list_span.start..at];
                let prefix = if existing.trim_end().ends_with(',') {
                    " "
                } else {
                    ", "
                };
                let text = format!("{prefix}{}", additions.join(", "));
                Ok(vec![TextPatch::insert(at, text)])
            } else {
                Ok(vec![append_statement(
                    source,
                    &format!("align {} [{}]", axis_name(*axis), unique.join(", ")),
                )])
            }
        }
        DiagramEditOp::BreakAlignment { node, axis } => {
            let mut patches = Vec::new();
            for r in scene
                .relations
                .iter()
                .filter(|r| !r.distribute && r.axis == *axis && r.members.contains(node))
            {
                let remaining: Vec<_> =
                    r.members.iter().filter(|id| *id != node).cloned().collect();
                if remaining.len() < 2 {
                    patches.push(remove_preserving_comments(
                        source,
                        whole_line(source, r.origin),
                    ));
                } else {
                    let ts: Vec<_> = syntax::lex(r.list_span.text(source))
                        .into_iter()
                        .filter(|t| {
                            !matches!(t.kind, Kind::Whitespace | Kind::Newline | Kind::Comment)
                        })
                        .collect();
                    let relative = r.list_span.text(source);
                    let mut at = 1;
                    while at + 1 < ts.len() {
                        let begin = at;
                        while at < ts.len() && !matches!(ts[at].kind, Kind::Comma | Kind::RBracket)
                        {
                            at += 1;
                        }
                        let span = Span::new(ts[begin].span.start, ts[at - 1].span.end);
                        if span
                            .text(relative)
                            .chars()
                            .filter(|c| !c.is_whitespace())
                            .collect::<String>()
                            == *node
                        {
                            let delete = if ts[at].kind == Kind::Comma {
                                Span::new(span.start, ts[at].span.end)
                            } else {
                                let prev = ts[..begin]
                                    .iter()
                                    .rev()
                                    .find(|t| t.kind == Kind::Comma)
                                    .unwrap();
                                Span::new(prev.span.start, span.end)
                            };
                            patches.push(TextPatch::replace(
                                source,
                                Span::new(
                                    delete.start + r.list_span.start,
                                    delete.end + r.list_span.start,
                                ),
                                "",
                            ));
                            break;
                        }
                        at += 1;
                    }
                }
            }
            Ok(patches)
        }
        DiagramEditOp::LockPort { node, port, spec } => {
            let n = scene
                .node(node)
                .ok_or_else(|| Error::Edit(format!("unknown node {node}")))?;
            let attrs = port_attrs(spec)?;
            if let Some(p) = n.ports.get(port) {
                if !p.explicit {
                    return promote_port(source, n, port, &attrs);
                }

                let ts = syntax::lex(p.origin.text(source));
                if let Some(open) = ts.iter().find(|t| t.kind == Kind::LBracket) {
                    let close = ts.iter().rev().find(|t| t.kind == Kind::RBracket).unwrap();
                    let range = Span::new(
                        p.origin.start + open.span.start,
                        p.origin.start + close.span.end,
                    );
                    let all = syntax::lex(range.text(source));
                    let sig: Vec<_> = syntax::significant(&all)
                        .cloned()
                        .map(|mut t| {
                            t.span.start += range.start;
                            t.span.end += range.start;
                            t
                        })
                        .collect();
                    let ann = Annotation {
                        name: "port".into(),
                        attrs: crate::model::attributes(source, &sig[1..sig.len() - 1]),
                        span: range,
                    };
                    update_annotation(
                        source,
                        &ann,
                        &attrs,
                        &["side", "order", "ratio", "pos"]
                            .into_iter()
                            .filter(|k| !attrs.contains_key(*k))
                            .collect::<Vec<_>>(),
                    )
                } else {
                    let at = inline_insertion(source, p.origin);
                    Ok(vec![TextPatch::insert(
                        at,
                        format!(" [{}]", format_attrs(&attrs)),
                    )])
                }
            } else {
                let ep = scene
                    .edges
                    .iter()
                    .find_map(|e| {
                        if format!("{}:source", e.id) == *port && e.source.node == *node {
                            Some(&e.source)
                        } else if format!("{}:target", e.id) == *port && e.target.node == *node {
                            Some(&e.target)
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| {
                        Error::Edit(format!(
                            "unknown port {port}; use a named port or edge-id:source/target"
                        ))
                    })?;
                let range = ep
                    .anchor_span
                    .unwrap_or(Span::new(ep.span.end, ep.span.end));
                let text = match spec {
                    PortConstraint::Free => String::new(),
                    PortConstraint::FixedSide { side } => format!(".{side:?}"),
                    PortConstraint::FixedOrder { side, order } => {
                        format!(".{side:?}(order: {order})")
                    }
                    PortConstraint::FixedRatio { side, ratio } => format!(".{side:?}@{ratio}"),
                    PortConstraint::FixedPos { .. } => {
                        return Err(Error::Edit(
                            "exact position requires a declared named port".into(),
                        ));
                    }
                };
                Ok(vec![TextPatch::replace(source, range, text)])
            }
        }
        DiagramEditOp::NudgeRouteChannel {
            edge,
            segment_idx,
            lane_grid,
        } => {
            if !lane_grid.is_finite() {
                return Err(Error::Edit("route lane must be finite".into()));
            }
            if scene.edge(edge).is_none() {
                return Err(Error::Edit(format!("unknown edge {edge}")));
            }
            let attrs = BTreeMap::from([
                ("segment".into(), segment_idx.to_string()),
                ("lane".into(), format!("{lane_grid}g")),
            ]);
            upsert_layout(source, &scene, edge, "route", attrs)
        }
        DiagramEditOp::SlideLabel { edge, slot, t_arc } => {
            if !t_arc.is_finite() || !(0. ..=1.).contains(t_arc) {
                return Err(Error::Edit("label slide must be in [0,1]".into()));
            }
            if scene.edge(edge).is_none() {
                return Err(Error::Edit(format!("unknown edge {edge}")));
            }
            upsert_layout(
                source,
                &scene,
                edge,
                "label",
                BTreeMap::from([
                    (
                        "slot".into(),
                        match slot {
                            LabelSlot::Above => "above",
                            LabelSlot::Below => "below",
                            LabelSlot::Inline => "inline",
                        }
                        .into(),
                    ),
                    ("slide".into(), t_arc.to_string()),
                ]),
            )
        }
        DiagramEditOp::UnpinEntities { targets } => {
            let mut patches = Vec::new();
            for target in targets {
                let n = scene
                    .node(target)
                    .ok_or_else(|| Error::Edit(format!("unknown node {target}")))?;
                for a in n.annotations.iter().filter(|a| a.name == "pin") {
                    patches.push(remove_preserving_comments(source, a.span));
                }
                for rule in scene
                    .layout_rules
                    .iter()
                    .filter(|r| r.target == *target && r.variant.is_none())
                {
                    for a in rule.annotations.iter().filter(|a| a.name == "pin") {
                        let span = if rule.annotations.len() == 1 {
                            whole_line(source, rule.span)
                        } else {
                            a.span
                        };
                        patches.push(remove_preserving_comments(source, span));
                    }
                }
            }
            patches.sort_by_key(|p| p.delete_range.start);
            patches.dedup_by_key(|p| p.delete_range);
            Ok(patches)
        }
    }
}
fn validate_coordinate(expr: &str) -> Result<(), Error> {
    let e = expr.replace(' ', "");
    if number(&e).is_some() {
        return Ok(());
    }
    let split = e
        .char_indices()
        .skip(1)
        .find(|(_, c)| *c == '+' || *c == '-')
        .map(|(i, _)| i);
    let base = if let Some(i) = split {
        if number(&e[i..]).is_none() {
            return Err(Error::Edit("invalid relative coordinate offset".into()));
        }
        &e[..i]
    } else {
        &e
    };
    if base.split('.').all(|part| {
        let mut c = part.chars();
        c.next().is_some_and(|c| c.is_alphabetic() || c == '_')
            && c.all(|c| c.is_alphanumeric() || c == '_')
    }) {
        Ok(())
    } else {
        Err(Error::Edit(
            "coordinate must be a grid literal or qualified anchor with a grid offset".into(),
        ))
    }
}
fn promote_port(
    source: &str,
    node: &Node,
    id: &str,
    attrs: &BTreeMap<String, String>,
) -> Result<Vec<TextPatch>, Error> {
    let nl = newline(source);
    let suffix = if attrs.is_empty() {
        String::new()
    } else {
        format!(" [{}]", format_attrs(attrs))
    };
    let declaration = format!("port {id}{suffix}");
    if node.explicit {
        let text = node.origin.primary_decl.text(source);
        let tokens = syntax::lex(text);
        let line_start = source[..node.origin.primary_decl.start]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        let indent = &source[line_start..node.origin.primary_decl.start];
        if let Some(open) = tokens.iter().find(|t| t.kind == Kind::LBrace) {
            let at = node.origin.primary_decl.start + open.span.end;
            Ok(vec![TextPatch::insert(
                at,
                format!("{nl}{indent}  {declaration}{nl}"),
            )])
        } else {
            let at = inline_insertion(source, node.origin.primary_decl);
            Ok(vec![TextPatch::insert(
                at,
                format!(" {{{nl}{indent}  {declaration}{nl}{indent}}}"),
            )])
        }
    } else {
        Ok(vec![append_statement(
            source,
            &format!("{} {{{nl}  {declaration}{nl}}}", node.id),
        )])
    }
}
fn axis_name(axis: Axis) -> &'static str {
    if axis == Axis::Row { "row" } else { "col" }
}
fn format_attrs(attrs: &BTreeMap<String, String>) -> String {
    attrs
        .iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect::<Vec<_>>()
        .join(", ")
}
fn port_attrs(spec: &PortConstraint) -> Result<BTreeMap<String, String>, Error> {
    let mut attrs = BTreeMap::new();
    if let Some(side) = spec.side() {
        attrs.insert("side".into(), format!("{side:?}"));
    }
    match spec {
        PortConstraint::FixedOrder { order, .. } => {
            attrs.insert("order".into(), order.to_string());
        }
        PortConstraint::FixedRatio { ratio, .. } => {
            if !ratio.is_finite() || *ratio <= 0. || *ratio >= 1. {
                return Err(Error::Edit("port ratio must be in (0,1)".into()));
            }
            attrs.insert("ratio".into(), ratio.to_string());
        }
        PortConstraint::FixedPos { offset_grid, .. } => {
            if !offset_grid.is_finite() || *offset_grid < 0. {
                return Err(Error::Edit(
                    "port position must be finite and non-negative".into(),
                ));
            }
            attrs.insert("pos".into(), format!("{offset_grid}g"));
        }
        _ => {}
    }
    Ok(attrs)
}
fn inline_insertion(source: &str, span: Span) -> usize {
    let text = span.text(source);
    let ts = syntax::lex(text);
    let end = ts
        .iter()
        .find(|t| {
            matches!(
                t.kind,
                Kind::Comment | Kind::Newline | Kind::LBrace | Kind::Semicolon
            )
        })
        .map(|t| t.span.start)
        .unwrap_or(text.len());
    span.start + text[..end].trim_end().len()
}
fn newline(source: &str) -> &'static str {
    if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}
fn append_statement(source: &str, statement: &str) -> TextPatch {
    let nl = newline(source);
    let prefix = if source.is_empty() || source.ends_with('\n') {
        ""
    } else {
        nl
    };
    TextPatch::insert(source.len(), format!("{prefix}{statement}{nl}"))
}
fn whole_line(source: &str, span: Span) -> Span {
    let start = source[..span.start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let start = if source[start..span.start].trim().is_empty() {
        start
    } else {
        span.start
    };
    let mut end = span.end;
    while end < source.len() && matches!(source.as_bytes()[end], b' ' | b'\t' | b'\r') {
        end += 1;
    }
    if end < source.len() && source.as_bytes()[end] == b'\n' {
        end += 1;
    }
    Span::new(start, end)
}
fn remove_preserving_comments(source: &str, span: Span) -> TextPatch {
    let tokens = syntax::lex(span.text(source));
    let text = if tokens.iter().any(|t| t.kind == Kind::Comment) {
        tokens
            .iter()
            .filter(|t| matches!(t.kind, Kind::Whitespace | Kind::Newline | Kind::Comment))
            .map(|t| t.span.text(span.text(source)))
            .collect::<String>()
    } else {
        String::new()
    };
    TextPatch::replace(source, span, text)
}
fn update_annotation(
    source: &str,
    a: &Annotation,
    wanted: &BTreeMap<String, String>,
    remove: &[&str],
) -> Result<Vec<TextPatch>, Error> {
    let mut patches = Vec::new();
    let mut missing = Vec::new();
    let mut entries: Vec<_> = a
        .attrs
        .iter()
        .map(|(key, value)| {
            let colon = source[a.span.start..value.span.start].rfind(':').unwrap() + a.span.start;
            let before = source[a.span.start..colon].trim_end();
            let mut start = a.span.start + before.len();
            while start > a.span.start {
                let c = source[..start].chars().next_back().unwrap();
                if c.is_alphanumeric() || c == '_' {
                    start -= c.len_utf8();
                } else {
                    break;
                }
            }
            (key.as_str(), start, value.span)
        })
        .collect();
    entries.sort_by_key(|e| e.1);
    let mut i = 0;
    let mut remaining = entries.len();
    while i < entries.len() {
        if !remove.contains(&entries[i].0) {
            i += 1;
            continue;
        }
        let first = i;
        while i < entries.len() && remove.contains(&entries[i].0) {
            i += 1;
            remaining -= 1;
        }
        let last = i - 1;
        let mut start = entries[first].1;
        let mut end = entries[last].2.end;
        if i < entries.len() {
            if let Some(comma) = source[end..entries[i].1].find(',') {
                end += comma + 1;
            }
        } else if first > 0 {
            let previous = entries[first - 1].2.end;
            if let Some(comma) = source[previous..start].find(',') {
                start = previous + comma;
            }
        } else {
            if let Some(comma) = source[end..a.span.end - 1].find(',') {
                end += comma + 1;
            }
        }
        patches.push(remove_preserving_comments(source, Span::new(start, end)));
    }
    for (k, v) in wanted {
        if let Some(old) = a.attrs.get(k) {
            if old.text != *v {
                patches.push(TextPatch::replace(source, old.span, v));
            }
        } else {
            missing.push(format!("{k}: {v}"));
        }
    }
    if !missing.is_empty() {
        let at = a.span.end - 1;
        let trailing = entries
            .last()
            .is_some_and(|e| !remove.contains(&e.0) && source[e.2.end..at].contains(','));
        let prefix = if remaining == 0 || trailing { "" } else { ", " };
        patches.push(TextPatch::insert(
            at,
            format!("{prefix}{}", missing.join(", ")),
        ));
    }
    Ok(patches)
}
fn upsert_layout(
    source: &str,
    scene: &SceneGraph,
    target: &str,
    name: &str,
    attrs: BTreeMap<String, String>,
) -> Result<Vec<TextPatch>, Error> {
    if let Some(rule) = scene
        .layout_rules
        .iter()
        .rev()
        .find(|r| r.target == target && r.variant.is_none())
    {
        if let Some(a) = rule.annotations.iter().find(|a| a.name == name) {
            return update_annotation(source, a, &attrs, &[]);
        }
        let at = inline_insertion(source, rule.span);
        return Ok(vec![TextPatch::insert(
            at,
            format!(" @{name}({})", format_attrs(&attrs)),
        )]);
    }
    let parse = syntax::parse(source);
    let block = parse
        .syntax()
        .children()
        .filter(|n| n.kind() == Kind::LayoutBlock)
        .filter(|n| {
            let s = n.text().to_string();
            !s.trim_start().starts_with("@layout(") && !s.trim_start().starts_with("@layout (")
        })
        .last();
    let target = if target.contains("->") {
        format!("({})", target.replace("->", " -> "))
    } else {
        target.into()
    };
    let line = format!("  {target} @{name}({})", format_attrs(&attrs));
    let nl = newline(source);
    if let Some(block) = block {
        let close = block
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == Kind::RBrace)
            .last()
            .unwrap();
        let at = Span::from(close.text_range()).start;
        let line_start = source[..at].rfind('\n').map(|i| i + 1).unwrap_or(at);
        let at = if source[line_start..at].trim().is_empty() {
            line_start
        } else {
            at
        };
        let prefix = if at > 0 && source.as_bytes()[at - 1] != b'\n' {
            nl
        } else {
            ""
        };
        Ok(vec![TextPatch::insert(at, format!("{prefix}{line}{nl}"))])
    } else {
        let prefix =
            if source.is_empty() || source.ends_with("\n\n") || source.ends_with("\r\n\r\n") {
                String::new()
            } else if source.ends_with('\n') {
                nl.into()
            } else {
                format!("{nl}{nl}")
            };
        Ok(vec![TextPatch::insert(
            source.len(),
            format!("{prefix}@layout {{{nl}{line}{nl}}}{nl}"),
        )])
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Drag {
    pub node: NodeId,
    pub position: Point,
}
/// Convert canvas positions into semantic edits. Positions are node centroids in pixels.
pub fn synthesize_drag(
    scene: &SceneGraph,
    layout: &Layout,
    drags: &[Drag],
) -> Result<Vec<DiagramEditOp>, Error> {
    let mut ops = Vec::new();
    for drag in drags {
        if !drag.position.x.is_finite() || !drag.position.y.is_finite() {
            return Err(Error::Edit("drag coordinates must be finite".into()));
        }
        let n = scene
            .node(&drag.node)
            .ok_or_else(|| Error::Edit(format!("unknown node {}", drag.node)))?;
        let old = layout
            .nodes
            .get(&drag.node)
            .ok_or_else(|| Error::Edit("node is absent from layout".into()))?;
        for (axis, delta) in [
            (Axis::Row, (drag.position.y - old.y).abs()),
            (Axis::Col, (drag.position.x - old.x).abs()),
        ] {
            if delta >= 24.
                && scene
                    .relations
                    .iter()
                    .any(|r| !r.distribute && r.axis == axis && r.members.contains(&n.id))
            {
                ops.push(DiagramEditOp::BreakAlignment {
                    node: n.id.clone(),
                    axis,
                });
            }
        }
        let peer = scene
            .nodes
            .iter()
            .filter(|p| {
                p.id != n.id && p.parent == n.parent && !drags.iter().any(|d| d.node == p.id)
            })
            .filter_map(|p| layout.nodes.get(&p.id).map(|r| (p, r)))
            .filter(|(p, _)| {
                scene.edges.iter().any(|e| {
                    e.source.node == n.id && e.target.node == p.id
                        || e.target.node == n.id && e.source.node == p.id
                })
            })
            .flat_map(|(p, r)| {
                [
                    (p, Axis::Row, (drag.position.y - r.y).abs()),
                    (p, Axis::Col, (drag.position.x - r.x).abs()),
                ]
            })
            .filter(|(_, _, d)| *d <= 6.)
            .min_by(|a, b| a.2.total_cmp(&b.2));
        if let Some((peer, axis, _)) = peer {
            ops.push(DiagramEditOp::SnapAlign {
                axis,
                members: vec![peer.id.clone(), n.id.clone()],
            });
            // Alignment constrains one axis. Preserve the user's movement on
            // the other axis instead of snapping back to the old coordinate.
            let (axis, coordinate) = match axis {
                Axis::Row => (Axis::Col, drag.position.x),
                Axis::Col => (Axis::Row, drag.position.y),
            };
            ops.push(DiagramEditOp::PinNode {
                node: n.id.clone(),
                pin: PinExpr::AxisLock {
                    axis,
                    coord_grid: (coordinate / GRID).round() as i32,
                },
                size_lock: None,
            });
            continue;
        }
        let container = scene
            .nodes
            .iter()
            .filter(|c| c.container && c.id != n.id && !ancestors(scene, &n.id).contains(&c.id))
            .filter_map(|c| layout.nodes.get(&c.id).map(|r| (c, r)))
            .filter(|(_, r)| {
                drag.position.x >= r.right() && drag.position.x - r.right() <= 6. * GRID
            })
            .min_by(|a, b| {
                (drag.position.x - a.1.right()).total_cmp(&(drag.position.x - b.1.right()))
            });
        let pin = if let Some((c, r)) = container {
            PinExpr::Relative {
                x: format!(
                    "{}.E + {}g",
                    c.id,
                    ((drag.position.x - r.right()) / GRID).round()
                ),
                y: format!("{}g", (drag.position.y / GRID).round()),
            }
        } else {
            PinExpr::AbsoluteGrid {
                x: (drag.position.x / GRID).round() as i32,
                y: (drag.position.y / GRID).round() as i32,
            }
        };
        ops.push(DiagramEditOp::PinNode {
            node: n.id.clone(),
            pin,
            size_lock: None,
        });
    }
    Ok(ops)
}
