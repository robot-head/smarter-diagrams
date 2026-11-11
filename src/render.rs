//! Accessible, deterministic vector output. User strings are always XML escaped.
use crate::{layout::*, model::*};
use std::fmt::Write;
pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
pub fn svg(scene: &SceneGraph, l: &Layout) -> String {
    let bounds = l
        .nodes
        .values()
        .copied()
        .chain(l.labels.iter().map(|a| a.bounds))
        .chain(
            l.routes
                .values()
                .flat_map(|r| r.points.iter().map(|p| Rect::new(p.x, p.y, 1., 1.))),
        )
        .chain(
            l.zones
                .values()
                .flat_map(|z| z.iter().map(|p| Rect::new(p.x, p.y, 1., 1.))),
        )
        .reduce(Rect::union)
        .unwrap_or(Rect::new(0., 0., 160., 80.))
        .inflate(32.);
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{:.2} {:.2} {:.2} {:.2}\" width=\"{:.0}\" height=\"{:.0}\" role=\"img\" aria-labelledby=\"diagram-title diagram-description\"><title id=\"diagram-title\">Architecture diagram</title><desc id=\"diagram-description\">{} nodes and {} connections</desc><style>text{{font-family:Inter,ui-sans-serif,system-ui,sans-serif;font-size:13px;fill:#233347}}.node-title{{font-weight:600;font-size:14px}}.port-label{{font-size:11px}}.edge{{fill:none;stroke:#53657a;stroke-width:1.7;stroke-linejoin:round}}.flow{{fill:none;stroke:#356bd9;stroke-width:3;stroke-linejoin:round}}</style>",
        bounds.left(),
        bounds.top(),
        bounds.w,
        bounds.h,
        bounds.w,
        bounds.h,
        scene.nodes.len(),
        scene.edges.len()
    );
    for z in &scene.zones {
        if let Some(hull) = l.zones.get(&z.id) {
            let tint = z
                .attrs
                .get("tint")
                .map(|v| v.text.as_str())
                .unwrap_or("oklch(0.8 0.08 230 / 0.18)");
            let (tint, alpha) = paint(tint);
            let _ = write!(
                s,
                "<g aria-label=\"{}\"><path d=\"{}\" fill=\"{}\" fill-opacity=\"{}\"/><text x=\"{:.2}\" y=\"{:.2}\">{}</text></g>",
                escape(&z.label),
                rounded_hull(hull),
                escape(&tint),
                alpha,
                hull.first().map(|p| p.x).unwrap_or(0.),
                hull.first().map(|p| p.y - 10.).unwrap_or(0.),
                escape(&z.label)
            );
        }
    }
    for n in scene
        .nodes
        .iter()
        .filter(|n| n.container && n.shape != "sql_table")
    {
        let r = l.nodes[&n.id];
        let _ = write!(
            s,
            "<g data-node=\"{}\" aria-label=\"{}\"><rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"12\" fill=\"#f0f4f9\" fill-opacity=\"0.55\" stroke=\"#c5d0dd\"/><text class=\"node-title\" x=\"{:.2}\" y=\"{:.2}\">{}</text></g>",
            escape(&n.id),
            escape(&n.label),
            r.left(),
            r.top(),
            r.w,
            r.h,
            r.left() + 16.,
            r.top() + 22.,
            escape(&n.label)
        );
    }
    for e in &scene.edges {
        let r = &l.routes[&e.id];
        let dashed = e.attrs.get("style").is_some_and(|v| v.text == "dashed");
        let dash = if dashed && !e.attrs.contains_key("phantom") {
            " stroke-dasharray=\"6 4\""
        } else {
            ""
        };
        let phantom = if e.attrs.contains_key("phantom") {
            " stroke=\"#c345ac\" stroke-dasharray=\"5 4\""
        } else {
            ""
        };
        let color = if e.attrs.contains_key("active") {
            "#356bd9"
        } else if e.attrs.contains_key("phantom") {
            "#c345ac"
        } else {
            "#53657a"
        };
        let width = if e.attrs.contains_key("active") {
            4.0
        } else if e.kind == EdgeKind::Bold {
            3.0
        } else {
            1.7
        };
        let (shaft, heads) = connector(&r.points, e.kind, width);
        let _ = write!(
            s,
            "<path data-edge=\"{}\" class=\"edge\" style=\"stroke:{color};stroke-width:{width}\" d=\"{}\"{dash}{phantom}><title>{}</title></path>",
            escape(&e.id),
            path(&shaft),
            escape(&format!(
                "{} → {}: {}",
                e.source.node, e.target.node, e.label
            ))
        );
        for (end, points) in heads {
            let _ = write!(
                s,
                "<path data-edge=\"{}\" data-arrowhead=\"{end}\" d=\"{}Z\" fill=\"{color}\"/>",
                escape(&e.id),
                path(&points)
            );
        }
    }
    // Optical crossing bridges: white under-stroke separates crossing ink without changing topology.
    let routes: Vec<_> = l.routes.values().collect();
    for (i, a) in routes.iter().enumerate() {
        for b in &routes[i + 1..] {
            for p in a.points.windows(2) {
                for q in b.points.windows(2) {
                    if crate::routing::proper_crossing(p[0], p[1], q[0], q[1]) {
                        let (x, y) = if (p[0].y - p[1].y).abs() < 1e-6 {
                            (q[0].x, p[0].y)
                        } else {
                            (p[0].x, q[0].y)
                        };
                        let _ = write!(
                            s,
                            "<path d=\"M {:.2} {:.2} Q {x:.2} {:.2} {:.2} {y:.2}\" fill=\"none\" stroke=\"white\" stroke-width=\"6\"/><path d=\"M {:.2} {y:.2} Q {x:.2} {:.2} {:.2} {y:.2}\" fill=\"none\" stroke=\"#53657a\" stroke-width=\"1.7\"/>",
                            x - 5.,
                            y,
                            y - 7.,
                            x + 5.,
                            x - 5.,
                            y - 7.,
                            x + 5.
                        );
                    }
                }
            }
        }
    }
    for n in scene
        .nodes
        .iter()
        .filter(|n| !n.container || n.shape == "sql_table")
    {
        let r = l.nodes[&n.id];
        let _ = write!(
            s,
            "<g data-node=\"{}\" aria-label=\"{}\"><title>{}</title>",
            escape(&n.id),
            escape(&n.id),
            escape(&n.label)
        );
        match n.shape.as_str() {
            "actor" => {
                let _ = write!(
                    s,
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"18\" fill=\"#e4f2ea\" stroke=\"#a6c9b5\"/><circle cx=\"{}\" cy=\"{}\" r=\"5\" fill=\"none\" stroke=\"#608b75\"/><path d=\"M {} {} Q {} {} {} {}\" fill=\"none\" stroke=\"#608b75\"/>",
                    r.left(),
                    r.top(),
                    r.w,
                    r.h,
                    r.left() + 14.,
                    r.y - 5.,
                    r.left() + 7.,
                    r.y + 10.,
                    r.left() + 14.,
                    r.y,
                    r.left() + 21.,
                    r.y + 10.
                );
            }
            "cache" | "diamond" | "decision" => {
                let vertices = crate::shapes::polygon(&n.shape, r).unwrap();
                let (fill, stroke) = if n.shape == "cache" {
                    ("#fff1d6", "#ceb98c")
                } else {
                    ("#fff0d8", "#cfab70")
                };
                let _ = write!(
                    s,
                    "<path d=\"{}Z\" fill=\"{fill}\" stroke=\"{stroke}\"/>",
                    path(&vertices)
                );
            }
            "database" => {
                let ry = crate::shapes::CYLINDER_RY.min(r.h / 2.);
                let _ = write!(
                    s,
                    "<path d=\"M {} {} A {} {ry} 0 0 1 {} {} L {} {} A {} {ry} 0 0 1 {} {} Z\" fill=\"#e1edf9\" stroke=\"#9ab6d4\"/><ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{ry}\" fill=\"#d1e3f5\" stroke=\"#9ab6d4\"/>",
                    r.left(),
                    r.top() + ry,
                    r.w / 2.,
                    r.right(),
                    r.top() + ry,
                    r.right(),
                    r.bottom() - ry,
                    r.w / 2.,
                    r.left(),
                    r.bottom() - ry,
                    r.x,
                    r.top() + ry,
                    r.w / 2.
                );
            }
            _ => {
                let fill = match n.shape.as_str() {
                    "queue" => "#e7e0fa",
                    "sql_table" => "#e1edf9",
                    _ => "#e5eef6",
                };
                let _ = write!(
                    s,
                    "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"{}\" fill=\"{fill}\" stroke=\"#b8c9da\"/>",
                    r.left(),
                    r.top(),
                    r.w,
                    r.h,
                    if n.shape == "sql_table" { 2 } else { 8 }
                );
                if n.shape == "queue" {
                    for k in 1..4 {
                        let x = r.left() + k as f64 * 10.;
                        let _ = write!(
                            s,
                            "<path d=\"M {x} {} V {}\" stroke=\"#b4a5d4\"/>",
                            r.top() + 8.,
                            r.bottom() - 8.
                        );
                    }
                }
            }
        }
        if let Some(index) = scene.story.iter().position(|id| id == &n.id) {
            let _ = write!(
                s,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"8\" fill=\"none\" stroke=\"#3f6dc5\" stroke-width=\"2.5\"><title>Story step {}</title></rect>",
                r.left(),
                r.top(),
                r.w,
                r.h,
                index + 1
            );
        }
        if n.shape == "sql_table" {
            let _ = write!(
                s,
                "<text class=\"node-title\" style=\"font-size:10px\" x=\"{}\" y=\"{}\">{}</text>",
                r.left() + 12.,
                r.top() + 8.,
                escape(&n.label)
            );
            for (pidx, p) in n.ports.values().enumerate() {
                if !matches!(p.constraint, PortConstraint::FixedPos { .. }) {
                    continue;
                }
                let y = l
                    .ports
                    .get(&format!("{}#{}", n.id, p.id))
                    .map(|p| p.point.y)
                    .unwrap_or(r.top() + 32. + pidx as f64 * 16.);
                let _ = write!(
                    s,
                    "<text class=\"port-label\" x=\"{}\" y=\"{}\">{}</text>",
                    r.left() + 14.,
                    y + 4.,
                    escape(&p.label)
                );
            }
        } else {
            let _ = write!(
                s,
                "<text class=\"node-title\" text-anchor=\"middle\" dominant-baseline=\"central\" x=\"{:.2}\" y=\"{:.2}\">{}</text>",
                r.x,
                r.y,
                escape(&n.label)
            );
        }
        s.push_str("</g>");
    }
    for p in l.ports.values() {
        // Keep invisible anchors for consumers, without covering arrow tips or
        // turning every implicit attachment into a visible junction dot.
        let _ = write!(
            s,
            "<circle data-port=\"{}\" cx=\"{:.2}\" cy=\"{:.2}\" r=\"4\" fill=\"transparent\"><title>{}</title></circle>",
            escape(&p.key),
            p.point.x,
            p.point.y,
            escape(&p.key)
        );
    }
    for flow in &scene.flows {
        for (step, id) in flow.edges.iter().enumerate() {
            if let Some(r) = l.routes.get(id) {
                let mid = at_arc(&r.points, 0.5);
                let (shaft, heads) = connector(&r.points, scene.edge(id).unwrap().kind, 3.);
                let _ = write!(
                    s,
                    "<g aria-label=\"{} step {}\"><path data-edge=\"{}\" class=\"flow\" d=\"{}\"/><circle cx=\"{}\" cy=\"{}\" r=\"10\" fill=\"#356bd9\"/><text x=\"{}\" y=\"{}\" text-anchor=\"middle\" style=\"fill:white;font-size:11px\">{}</text></g>",
                    escape(&flow.label),
                    step + 1,
                    escape(id),
                    path(&shaft),
                    mid.x,
                    mid.y,
                    mid.x,
                    mid.y + 4.,
                    step + 1
                );
                for (end, points) in heads {
                    let _ = write!(
                        s,
                        "<path data-edge=\"{}\" data-arrowhead=\"{end}\" d=\"{}Z\" fill=\"#356bd9\"/>",
                        escape(id),
                        path(&points)
                    );
                }
            }
        }
    }
    for label in &l.labels {
        let _ = write!(s, "<g data-label=\"{}\">", escape(&label.feature));
        let r = label.bounds;
        if label.callout {
            let _ = write!(
                s,
                "<path d=\"M {} {} L {} {}\" stroke=\"#9eacba\" fill=\"none\"/>",
                label.anchor.x,
                label.anchor.y,
                r.x,
                r.top()
            );
        }
        if label.knockout && !label.callout {
            let _ = write!(
                s,
                "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"5\" fill=\"white\"/>",
                r.left() - 4.,
                r.top() - 4.,
                r.w + 8.,
                r.h + 8.
            );
        }
        let _ = write!(
            s,
            "<text x=\"{:.2}\" y=\"{:.2}\" text-anchor=\"middle\" dominant-baseline=\"central\">{}</text>",
            r.x,
            r.y,
            escape(&label.text)
        );
        s.push_str("</g>");
    }
    s.push_str("</svg>\n");
    s
}

type Arrowheads = Vec<(&'static str, [Point; 3])>;

/// Trim the shaft to each arrow's base. Drawing the heads in user units keeps
/// stroke width, color and the exact silhouette attachment independent of SVG
/// marker scaling and avoids a square shaft protruding through the tip.
fn connector(points: &[Point], kind: EdgeKind, width: f64) -> (Vec<Point>, Arrowheads) {
    let mut shaft = points.to_vec();
    shaft.dedup();
    let mut heads = Vec::new();
    if shaft.len() < 2 || kind == EdgeKind::Undirected {
        return (shaft, heads);
    }
    let last = shaft.len() - 1;
    let start_tip = shaft[0];
    let start_neighbor = shaft[1];
    let end_tip = shaft[last];
    let end_neighbor = shaft[last - 1];
    let length = 8. + width;
    let half_width = 3. + width / 2.;
    let mut add = |end, tip: Point, neighbor: Point, index| {
        let available = tip.distance(neighbor);
        let length = length.min(
            available
                * if last == 1 && kind == EdgeKind::Bidirectional {
                    0.45
                } else {
                    0.8
                },
        );
        let scale = length / (8. + width);
        let (dx, dy) = (
            (neighbor.x - tip.x) / available,
            (neighbor.y - tip.y) / available,
        );
        let base = tip.offset(dx * length, dy * length);
        shaft[index] = base;
        heads.push((
            end,
            [
                tip,
                base.offset(-dy * half_width * scale, dx * half_width * scale),
                base.offset(dy * half_width * scale, -dx * half_width * scale),
            ],
        ));
    };
    add("end", end_tip, end_neighbor, last);
    if kind == EdgeKind::Bidirectional {
        add("start", start_tip, start_neighbor, 0);
    }
    (shaft, heads)
}

pub fn path(points: &[Point]) -> String {
    let mut s = String::new();
    for (i, p) in points.iter().enumerate() {
        let _ = write!(
            s,
            "{} {:.2} {:.2} ",
            if i == 0 { "M" } else { "L" },
            p.x,
            p.y
        );
    }
    s
}
fn rounded_hull(p: &[Point]) -> String {
    if p.len() < 3 {
        return path(p);
    }
    let mut s = String::new();
    let prev = p[p.len() - 1];
    let _ = write!(
        s,
        "M {} {} ",
        (prev.x + p[0].x) / 2.,
        (prev.y + p[0].y) / 2.
    );
    for (i, a) in p.iter().enumerate() {
        let b = p[(i + 1) % p.len()];
        let _ = write!(
            s,
            "Q {} {} {} {} ",
            a.x,
            a.y,
            (a.x + b.x) / 2.,
            (a.y + b.y) / 2.
        );
    }
    s.push('Z');
    s
}
pub fn at_arc(points: &[Point], t: f64) -> Point {
    let total = points.windows(2).map(|p| p[0].distance(p[1])).sum::<f64>();
    let mut remaining = t.clamp(0., 1.) * total;
    for pair in points.windows(2) {
        let length = pair[0].distance(pair[1]);
        if remaining <= length && length > 0. {
            let f = remaining / length;
            return Point::new(
                pair[0].x + (pair[1].x - pair[0].x) * f,
                pair[0].y + (pair[1].y - pair[0].y) * f,
            );
        }
        remaining -= length;
    }
    points.last().copied().unwrap_or_default()
}

/// Hover/focus projection using causal cones; topology and coordinates stay unchanged.
pub fn focused_svg(scene: &SceneGraph, l: &Layout, focus: &str, hops: usize) -> String {
    let mut svg = svg(scene, l);
    for (id, tone) in crate::views::attention(scene, Some(focus), hops) {
        svg = svg.replace(
            &format!("<g data-node=\"{}\"", escape(&id)),
            &format!(
                "<g opacity=\"{}\" data-node=\"{}\"",
                tone.opacity,
                escape(&id)
            ),
        );
    }
    svg
}

/// Resolve OKLCH into deterministic sRGB plus alpha for SVG, PDF and PNG backends.
pub fn paint(color: &str) -> (String, f64) {
    let Some(body) = color
        .trim()
        .strip_prefix("oklch(")
        .and_then(|s| s.strip_suffix(')'))
    else {
        return (color.into(), 1.);
    };
    let (body, alpha) = body
        .split_once('/')
        .map(|(b, a)| (b, a.trim().parse::<f64>().unwrap_or(1.).clamp(0., 1.)))
        .unwrap_or((body, 1.));
    let values: Vec<_> = body.split_whitespace().collect();
    if values.len() != 3 {
        return ("#dce8f4".into(), alpha);
    }
    let l = if let Some(percent) = values[0].strip_suffix('%') {
        percent.parse::<f64>().ok().map(|n| n / 100.)
    } else {
        values[0].parse::<f64>().ok()
    };
    let c = values[1].parse::<f64>().ok();
    let h = values[2].trim_end_matches("deg").parse::<f64>().ok();
    let (Some(l), Some(c), Some(h)) = (l, c, h) else {
        return ("#dce8f4".into(), alpha);
    };
    if !l.is_finite() || !c.is_finite() || !h.is_finite() {
        return ("#dce8f4".into(), alpha);
    }
    let a = c * h.to_radians().cos();
    let b = c * h.to_radians().sin();
    let ll = (l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m = (l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let ss = (l - 0.0894841775 * a - 1.2914855480 * b).powi(3);
    let channel = |v: f64| -> u8 {
        let v = if v <= 0.0031308 {
            12.92 * v
        } else {
            1.055 * v.powf(1. / 2.4) - 0.055
        };
        (v.clamp(0., 1.) * 255.).round() as u8
    };
    let r = channel(4.0767416621 * ll - 3.3077115913 * m + 0.2309699292 * ss);
    let g = channel(-1.2684380046 * ll + 2.6097574011 * m - 0.3413193965 * ss);
    let b = channel(-0.0041960863 * ll - 0.7034186147 * m + 1.7076147010 * ss);
    (format!("#{r:02x}{g:02x}{b:02x}"), alpha)
}

/// A protocol playback frame: visited edges/badges accumulate, and the current edge is emphasized.
pub fn flow_frame_svg(
    scene: &SceneGraph,
    l: &Layout,
    id: &str,
    step: usize,
) -> Result<String, crate::Error> {
    let flow = scene
        .flows
        .iter()
        .find(|f| f.id == id)
        .ok_or_else(|| crate::Error::Edit("unknown flow".into()))?;
    let edge = flow
        .edges
        .get(step)
        .ok_or_else(|| crate::Error::Edit("flow step is out of range".into()))?;
    let mut graph = scene.clone();
    graph.flows.retain(|f| f.id == id);
    graph.flows[0].edges.truncate(step + 1);
    graph.flows[0].steps.truncate(step + 2);
    if let Some(e) = graph.edges.iter_mut().find(|e| e.id == *edge) {
        e.attrs.insert(
            "active".into(),
            Value {
                text: "true".into(),
                ..Default::default()
            },
        );
    }
    Ok(svg(&graph, l))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrow_tips_and_shafts_join_exactly_in_every_direction() {
        for (dx, dy) in [(1., 0.), (-1., 0.), (0., 1.), (0., -1.)] {
            for length in [4., 16., 100.] {
                let start = Point::new(10., 20.);
                let end = start.offset(dx * length, dy * length);
                // Redundant route vertices must not create zero-sized heads.
                let route = [start, start, end, end];
                for width in [1.7, 3., 4.] {
                    for kind in [
                        EdgeKind::Directed,
                        EdgeKind::Bidirectional,
                        EdgeKind::Bold,
                        EdgeKind::Undirected,
                    ] {
                        let (shaft, heads) = connector(&route, kind, width);
                        assert!(shaft[0].distance(shaft[1]) > 0.);
                        assert_eq!(
                            heads.len(),
                            match kind {
                                EdgeKind::Undirected => 0,
                                EdgeKind::Bidirectional => 2,
                                _ => 1,
                            }
                        );
                        for (side, triangle) in heads {
                            let tip = if side == "start" { start } else { end };
                            let base = if side == "start" { shaft[0] } else { shaft[1] };
                            assert_eq!(triangle[0], tip);
                            assert!((base.x - (triangle[1].x + triangle[2].x) / 2.).abs() < 1e-9);
                            assert!((base.y - (triangle[1].y + triangle[2].y) / 2.).abs() < 1e-9);
                            assert!(base.distance(tip) > 0. && base.distance(tip) <= length);
                            assert!(
                                (triangle[1].x - triangle[2].x).abs()
                                    + (triangle[1].y - triangle[2].y).abs()
                                    > 0.
                            );
                        }
                    }
                }
            }
        }
    }
}
