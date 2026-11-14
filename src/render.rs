//! Accessible, deterministic vector output. User strings are always XML escaped.
use crate::{layout::*, model::*};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
};
pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
/// Deterministic per-drawing IDs keep masks, hatches and accessible titles local.
pub(crate) fn namespace_svg(svg: String) -> String {
    let hash = svg.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    });
    let prefix = format!("sd-{hash:016x}");
    let ids: BTreeMap<_, _> = svg
        .split(" id=\"")
        .skip(1)
        .filter_map(|s| s.split('"').next())
        .map(|id| (id.to_owned(), format!("{prefix}-{id}")))
        .collect();
    // The emitter quotes attributes and escapes user strings. Rewrite only
    // reference attributes, preserving captions, descriptions and aria-labels.
    let mut output = String::with_capacity(svg.len());
    let mut cursor = 0;
    while let Some(open) = svg[cursor..].find('<') {
        let start = cursor + open;
        output.push_str(&svg[cursor..start]);
        let Some(close) = svg[start..].find('>') else {
            output.push_str(&svg[start..]);
            return output;
        };
        let end = start + close + 1;
        let tag = &svg[start..end];
        let mut at = 0;
        while let Some(equal) = tag[at..].find("=\"") {
            let equal = at + equal;
            let begin = equal + 2;
            let Some(close) = tag[begin..].find('"') else {
                break;
            };
            let finish = begin + close;
            let name = tag[..equal].split_whitespace().next_back().unwrap_or("");
            let value = &tag[begin..finish];
            output.push_str(&tag[at..begin]);
            let value = match name {
                "id" => ids.get(value).cloned().unwrap_or_else(|| value.to_owned()),
                "aria-labelledby" => value
                    .split_whitespace()
                    .map(|id| ids.get(id).map(String::as_str).unwrap_or(id))
                    .collect::<Vec<_>>()
                    .join(" "),
                "href" | "xlink:href" => value
                    .strip_prefix('#')
                    .and_then(|id| ids.get(id))
                    .map(|id| format!("#{id}"))
                    .unwrap_or_else(|| value.to_owned()),
                "mask" | "clip-path" | "filter" | "fill" | "stroke" | "style" | "marker-start"
                | "marker-mid" | "marker-end" => {
                    let mut value = value.to_owned();
                    for (old, new) in &ids {
                        value = value.replace(&format!("url(#{old})"), &format!("url(#{new})"));
                    }
                    value
                }
                _ => value.to_owned(),
            };
            output.push_str(&value);
            at = finish;
        }
        output.push_str(&tag[at..]);
        cursor = end;
    }
    output.push_str(&svg[cursor..]);
    output
}
pub(crate) fn flow_caption(scene: &SceneGraph, edge: &Edge) -> Option<String> {
    let steps = scene
        .flows
        .iter()
        .flat_map(|f| f.edges.iter().enumerate())
        .filter(|(_, id)| **id == edge.id)
        .map(|(i, _)| (i + 1).to_string())
        .collect::<Vec<_>>();
    if steps.is_empty() {
        None
    } else {
        Some(format!(
            "{}{}",
            steps.join("/"),
            if edge.label.is_empty() {
                String::new()
            } else {
                format!(" · {}", edge.label)
            }
        ))
    }
}
fn text_content(text: &str, x: f64, height: f64) -> String {
    let lines: Vec<_> = text.split('\n').collect();
    if lines.len() == 1 {
        return escape(text);
    }
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            format!(
                "<tspan x=\"{x}\" dy=\"{}\">{}</tspan>",
                if i == 0 {
                    -(lines.len() as f64 - 1.) * height / 2.
                } else {
                    height
                },
                escape(line)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
/// Fillets are bounded by half of each adjacent segment; arrow bases stay exact.
fn rounded_path(points: &[Point]) -> String {
    let points = crate::routing::coalesced(points);
    if points.len() < 3 {
        return path(&points);
    }
    let mut s = path(&points[..1]);
    for p in points.windows(3) {
        let (a, b, c) = (p[0], p[1], p[2]);
        let before = a.distance(b);
        let after = b.distance(c);
        let radius = 5_f64.min(before / 2.).min(after / 2.);
        if radius < 1e-6 {
            continue;
        }
        let from = b.offset((a.x - b.x) * radius / before, (a.y - b.y) * radius / before);
        let to = b.offset((c.x - b.x) * radius / after, (c.y - b.y) * radius / after);
        let _ = write!(
            s,
            "L {:.2} {:.2} Q {:.2} {:.2} {:.2} {:.2} ",
            from.x, from.y, b.x, b.y, to.x, to.y
        );
    }
    let p = points.last().unwrap();
    let _ = write!(s, "L {:.2} {:.2}", p.x, p.y);
    s
}
/// A text alternative for vector exports and the documentation's graph tables.
pub fn description(scene: &SceneGraph) -> String {
    let endpoint = |ep: &Endpoint| {
        format!(
            "{}{}",
            ep.node,
            ep.port
                .as_ref()
                .map(|p| format!("#{p}"))
                .unwrap_or_else(|| ep
                    .constraint
                    .side()
                    .map(|side| format!(".{side:?}"))
                    .unwrap_or_default())
        )
    };
    let inline = |text: &str| {
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let nodes = scene
        .nodes
        .iter()
        .map(|n| {
            format!(
                "- {} ({}, {}{}{})",
                n.id,
                inline(&n.label),
                n.shape,
                n.parent
                    .as_ref()
                    .map(|p| format!(", inside {p}"))
                    .unwrap_or_default(),
                if n.ports.is_empty() {
                    String::new()
                } else {
                    format!(
                        ", ports {}",
                        n.ports
                            .values()
                            .map(|p| format!("{}: {}", p.id, inline(&p.label)))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let edges = if scene.edges.is_empty() {
        "- none".to_string()
    } else {
        scene
            .edges
            .iter()
            .map(|e| {
                format!(
                    "- {} {} {}{}",
                    endpoint(&e.source),
                    match e.kind {
                        EdgeKind::Undirected => "connects to",
                        EdgeKind::Bidirectional => "exchanges with",
                        _ => "points to",
                    },
                    endpoint(&e.target),
                    if e.label.is_empty() {
                        String::new()
                    } else {
                        format!(" via {}", inline(&e.label))
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let zones = scene
        .zones
        .iter()
        .map(|z| format!("- {} contains {}", inline(&z.label), z.members.join(", ")))
        .collect::<Vec<_>>()
        .join("\n");
    let flows = scene
        .flows
        .iter()
        .map(|f| format!("- {}: {}", inline(&f.label), f.steps.join(" → ")))
        .collect::<Vec<_>>()
        .join("\n");
    let mut sections = vec![format!(
        "{} nodes and {} connections.",
        scene.nodes.len(),
        scene.edges.len()
    )];
    if !nodes.is_empty() {
        sections.push(format!("Nodes:\n{nodes}"));
    }
    sections.push(format!("Connections:\n{edges}"));
    if !zones.is_empty() {
        sections.push(format!("Zones:\n{zones}"));
    }
    if !flows.is_empty() {
        sections.push(format!("Flows:\n{flows}"));
    }
    if !scene.story.is_empty() {
        sections.push(format!("Story:\n- {}", scene.story.join(" → ")));
    }
    sections.join("\n\n")
}
/// Smooth oriented enclosing ellipse contours for a zone. `Layout::zones` also
/// stores the sampled enclosing ellipse polygon for layout consumers.
pub fn zone_contours(scene: &SceneGraph, l: &Layout, zone: &Zone) -> Vec<Vec<Point>> {
    crate::zones::contours(scene, l, zone)
}
fn all_zone_ellipses(scene: &SceneGraph, l: &Layout) -> Vec<Option<crate::zones::ZoneEllipse>> {
    scene
        .zones
        .iter()
        .map(|zone| crate::zones::ellipse(scene, l, zone))
        .collect()
}
fn zone_titles_from_ellipses<'a>(
    scene: &'a SceneGraph,
    l: &Layout,
    ellipses: &[Option<crate::zones::ZoneEllipse>],
) -> Vec<(&'a Zone, Rect)> {
    let mut titles = Vec::new();
    let mut occupied = Vec::new();
    for (idx, (zone, ellipse)) in scene.zones.iter().zip(ellipses).enumerate() {
        let Some(e) = ellipse else {
            continue;
        };
        let others: Vec<crate::zones::ZoneEllipse> = ellipses
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != idx)
            .filter_map(|(_, other)| *other)
            .collect();
        if let Some(r) = zone_title_at(scene, zone, l, *e, &others, &occupied) {
            occupied.push(r);
            titles.push((zone, r));
        }
    }
    titles
}
fn ellipse_y_range_on_span(e: crate::zones::ZoneEllipse, x0: f64, x1: f64) -> (f64, f64) {
    let eb = e.bounds();
    let lo = x0.min(x1).max(eb.left());
    let hi = x0.max(x1).min(eb.right());
    if lo > hi {
        return (eb.top(), eb.bottom());
    }
    let (c, s) = (e.theta.cos(), e.theta.sin());
    let (ax, bx) = (e.rx * c, -e.ry * s);
    let (ay, by) = (e.rx * s, e.ry * c);
    let rx_eff = ax.hypot(bx).max(1e-9);
    let ry_eff = ay.hypot(by);
    let phi_x = bx.atan2(ax);
    let t_top = (-by).atan2(-ay);
    let x_top = e.cx + ax * t_top.cos() + bx * t_top.sin();
    let x_bot = 2. * e.cx - x_top;

    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    if x_top >= lo - 1e-6 && x_top <= hi + 1e-6 {
        min_y = min_y.min(e.cy - ry_eff);
    }
    if x_bot >= lo - 1e-6 && x_bot <= hi + 1e-6 {
        max_y = max_y.max(e.cy + ry_eff);
    }
    for i in 0..=8 {
        let x = lo + (hi - lo) * (i as f64) / 8.;
        let delta = ((x - e.cx) / rx_eff).clamp(-1., 1.).acos();
        for t in [phi_x + delta, phi_x - delta] {
            let y = e.cy + ay * t.cos() + by * t.sin();
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
    }
    if min_y.is_finite() && max_y.is_finite() {
        (min_y, max_y)
    } else {
        (eb.top(), eb.bottom())
    }
}
fn zone_title_at(
    scene: &SceneGraph,
    zone: &Zone,
    l: &Layout,
    e: crate::zones::ZoneEllipse,
    other_ellipses: &[crate::zones::ZoneEllipse],
    occupied: &[Rect],
) -> Option<Rect> {
    let eb = e.bounds();
    let w = crate::labels::text_width(&zone.label) + 16.;
    let h = crate::labels::text_height(&zone.label) + 4.;
    let t_top = (-e.ry * e.theta.cos()).atan2(-e.rx * e.theta.sin());
    let x_top = e.cx + e.rx * e.theta.cos() * t_top.cos() - e.ry * e.theta.sin() * t_top.sin();
    let x_bot = 2. * e.cx - x_top;
    let members: std::collections::BTreeSet<&str> = scene
        .nodes
        .iter()
        .filter(|n| {
            zone.members.contains(&n.id)
                || crate::model::ancestors(scene, &n.id)
                    .iter()
                    .any(|id| zone.members.contains(id))
        })
        .map(|n| n.id.as_str())
        .collect();

    let clear = |r: Rect| {
        !e.contains(r.center())
            && !occupied.iter().any(|old| r.inflate(4.).intersects(*old))
            && !scene.nodes.iter().any(|n| {
                let node = l.nodes[&n.id];
                if crate::layout::container_header_rect_with_ports(n, node, &l.ports)
                    .is_some_and(|hr| r.inflate(4.).intersects(hr))
                {
                    return true;
                }
                if crate::layout::sql_table_title_rect(n, node)
                    .is_some_and(|tr| r.inflate(4.).intersects(tr))
                {
                    return true;
                }
                let node_pad = if !members.contains(n.id.as_str())
                    && (!n.container || crate::layout::is_record_shape(&n.shape))
                {
                    12.
                } else {
                    4.
                };
                r.inflate(node_pad).intersects(node)
                    && !(n.container && node.inflate(-6.).encloses(r.inflate(4.)))
            })
            && !l.routes.values().any(|route| {
                route
                    .points
                    .windows(2)
                    .any(|p| r.inflate(4.).blocks(p[0], p[1]))
            })
            && !l
                .labels
                .iter()
                .any(|label| r.inflate(4.).intersects(label.bounds))
    };

    let xs_from = |anchor: f64| -> Vec<f64> {
        let min_x = eb.left() + w / 2.;
        let max_x = eb.right() - w / 2.;
        if max_x <= min_x {
            return vec![e.cx];
        }
        let start = anchor.clamp(min_x, max_x);
        let mut xs = vec![start];
        let max_steps = (((max_x - min_x) / 8.).ceil() as usize).clamp(1, 256);
        for k in 1..=max_steps {
            let right_x = start + (k as f64) * 8.;
            if right_x <= max_x + 1e-6 {
                xs.push(right_x.min(max_x));
            }
            let left_x = start - (k as f64) * 8.;
            if left_x >= min_x - 1e-6 {
                xs.push(left_x.max(min_x));
            }
        }
        xs
    };
    let top_xs = xs_from(x_top);
    let bot_xs = xs_from(x_bot);

    for avoid_other_zones in [true, false] {
        for row in 0..scene.nodes.len() + l.labels.len() + 4 {
            let offset = (row as f64) * 20.;
            for (upper, xs) in [(true, &top_xs), (false, &bot_xs)] {
                for &x in xs {
                    let (y_min, y_max) = ellipse_y_range_on_span(e, x - w / 2., x + w / 2.);
                    let y = if upper {
                        y_min - h / 2. - 6. - offset
                    } else {
                        y_max + h / 2. + 6. + offset
                    };
                    let r = Rect::new(x, y, w, h);
                    if clear(r)
                        && (!avoid_other_zones
                            || !other_ellipses
                                .iter()
                                .any(|other| other.contains(r.center())))
                    {
                        return Some(r);
                    }
                }
            }
        }
    }

    let top = l
        .nodes
        .values()
        .map(|nr| nr.top())
        .chain(l.labels.iter().map(|lb| lb.bounds.top()))
        .chain(occupied.iter().map(|nr| nr.top()))
        .chain(
            l.routes
                .values()
                .flat_map(|rt| rt.points.iter().map(|p| p.y)),
        )
        .fold(eb.top(), f64::min);
    Some(Rect::new(e.cx, top - h / 2. - 16., w, h))
}
fn crossing_gaps(l: &Layout, visible: impl Fn(&str) -> bool) -> BTreeMap<String, Vec<Point>> {
    let routes: Vec<_> = l
        .routes
        .iter()
        .filter(|(id, _)| visible(id))
        .map(|(id, r)| (id, crate::routing::coalesced(&r.points)))
        .collect();
    let mut gaps: BTreeMap<String, Vec<Point>> = BTreeMap::new();
    for (i, (aid, a)) in routes.iter().enumerate() {
        for (bid, b) in &routes[i + 1..] {
            for p in a.windows(2) {
                for q in b.windows(2) {
                    if !crate::routing::proper_crossing(p[0], p[1], q[0], q[1]) {
                        continue;
                    }
                    let (vertical, point) = if (p[0].x - p[1].x).abs() < 1e-6 {
                        (*aid, Point::new(p[0].x, q[0].y))
                    } else {
                        (*bid, Point::new(q[0].x, p[0].y))
                    };
                    if [a.first(), a.last(), b.first(), b.last()]
                        .into_iter()
                        .flatten()
                        .any(|end| end.distance(point) < 12.)
                    {
                        continue;
                    }
                    let points = gaps.entry(vertical.clone()).or_default();
                    if !points.iter().any(|p| p.distance(point) < 1e-6) {
                        points.push(point);
                    }
                }
            }
        }
    }
    gaps
}
#[derive(Default)]
struct EdgeMask {
    crossings: Vec<Point>,
    knockouts: Vec<Rect>,
}

fn edge_masks(
    scene: &SceneGraph,
    l: &Layout,
    visible: impl Fn(&str) -> bool,
) -> BTreeMap<String, EdgeMask> {
    let mut masks: BTreeMap<String, EdgeMask> = crossing_gaps(l, &visible)
        .into_iter()
        .map(|(id, crossings)| {
            (
                id,
                EdgeMask {
                    crossings,
                    knockouts: Vec::new(),
                },
            )
        })
        .collect();
    for label in &l.labels {
        if label.knockout
            && !label.callout
            && visible(&label.feature)
            && scene.edge(&label.feature).is_some()
        {
            masks
                .entry(label.feature.clone())
                .or_default()
                .knockouts
                .push(label.bounds);
        }
    }
    masks
}

fn gap_mask(id: &str, masks: &BTreeMap<String, EdgeMask>) -> String {
    masks
        .keys()
        .position(|key| key == id)
        .map(|i| format!(" mask=\"url(#gap-{i})\""))
        .unwrap_or_default()
}
fn bounds_with(
    scene: &SceneGraph,
    l: &Layout,
    ellipses: &[Option<crate::zones::ZoneEllipse>],
    titles: &[(&Zone, Rect)],
) -> Rect {
    let ink = l
        .nodes
        .values()
        .copied()
        .chain(l.labels.iter().map(|a| a.bounds.inflate(4.)))
        .chain(
            l.ports
                .values()
                .map(|p| Rect::new(p.point.x, p.point.y, 8., 8.)),
        )
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
        .chain(ellipses.iter().flatten().map(|e| e.bounds()))
        .chain(titles.iter().map(|(_, r)| *r))
        .chain(
            scene
                .flows
                .iter()
                .flat_map(|flow| flow.edges.iter())
                .filter_map(|id| {
                    l.routes.get(id).map(|route| {
                        let p = at_arc(&route.points, 0.5);
                        Rect::new(p.x, p.y, 22., 22.)
                    })
                }),
        )
        .reduce(Rect::union)
        .unwrap_or(Rect::new(0., 0., 160., 80.));
    Rect::new(ink.x, ink.y, ink.w + 96., ink.h + 64.)
}
/// Bounds of every painted feature, including external record and zone titles.
pub fn bounds(scene: &SceneGraph, l: &Layout) -> Rect {
    let ellipses = all_zone_ellipses(scene, l);
    let titles = zone_titles_from_ellipses(scene, l, &ellipses);
    bounds_with(scene, l, &ellipses, &titles)
}

pub fn svg(scene: &SceneGraph, l: &Layout) -> String {
    svg_with_theme(scene, l, None)
}

/// Render SVG using an explicit theme override (falling back to `scene.theme` if `None`).
pub fn svg_with_theme(
    scene: &SceneGraph,
    l: &Layout,
    theme: Option<&crate::themes::ThemePalette>,
) -> String {
    let owned;
    let resolved = match theme {
        Some(t) => Some(t),
        None => {
            owned = scene.theme.as_deref().and_then(crate::themes::by_name);
            owned.as_ref()
        }
    };
    svg_themed_with_decorations(scene, l, resolved, |_| true, |_, _| String::new())
}

// Decorations belong inside the node's opacity group and below its text.
pub(crate) fn svg_with_decorations(
    scene: &SceneGraph,
    l: &Layout,
    edge_visible: impl Fn(&str) -> bool,
    decorate: impl Fn(&Node, Rect) -> String,
) -> String {
    let owned = scene.theme.as_deref().and_then(crate::themes::by_name);
    svg_themed_with_decorations(scene, l, owned.as_ref(), edge_visible, decorate)
}

fn write_link_open(s: &mut String, href: &str) {
    let lower = href.trim_start().to_ascii_lowercase();
    let external = lower.starts_with("https://") || lower.starts_with("http://");
    let _ = write!(
        s,
        "<a class=\"sd-link\" data-link=\"true\" href=\"{}\"{}>",
        escape(href),
        if external {
            " target=\"_blank\" rel=\"noopener noreferrer\""
        } else {
            ""
        }
    );
}

fn edge_stroke_style(e: &Edge) -> (&'static str, &'static str) {
    let style_val = e
        .attrs
        .get("style")
        .map(|v| crate::model::unquote(v.text.trim()))
        .unwrap_or_default();
    let animate = e
        .attrs
        .get("animate")
        .is_some_and(|v| crate::model::unquote(v.text.trim()) == "true");
    if animate
        || matches!(
            style_val.as_str(),
            "marching-ants" | "marching_ants" | "animated"
        )
    {
        (" marching-ants", " stroke-dasharray=\"6 4\"")
    } else if style_val == "dashed" {
        ("", " stroke-dasharray=\"6 4\"")
    } else if style_val == "dotted" {
        ("", " stroke-dasharray=\"2 4\" stroke-linecap=\"round\"")
    } else {
        ("", "")
    }
}

pub(crate) fn svg_themed_with_decorations(
    scene: &SceneGraph,
    l: &Layout,
    theme: Option<&crate::themes::ThemePalette>,
    edge_visible: impl Fn(&str) -> bool,
    decorate: impl Fn(&Node, Rect) -> String,
) -> String {
    let ellipses = all_zone_ellipses(scene, l);
    let titles = zone_titles_from_ellipses(scene, l, &ellipses);
    let bounds = bounds_with(scene, l, &ellipses, &titles);
    let masks = edge_masks(scene, l, &edge_visible);
    let theme_attrs = theme
        .map(|t| {
            format!(
                " data-explicit-theme=\"{}\" style=\"{};background:var(--sd-bg,{})\"",
                escape(&t.id),
                t.css_vars(),
                escape(&t.bg)
            )
        })
        .unwrap_or_default();
    let theme_bg_rect = theme
        .map(|t| {
            format!(
                "<rect data-theme-bg=\"true\" x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" fill=\"var(--sd-bg,{})\"/>",
                bounds.left(),
                bounds.top(),
                bounds.w,
                bounds.h,
                escape(&t.bg)
            )
        })
        .unwrap_or_default();
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{:.2} {:.2} {:.2} {:.2}\" width=\"{:.0}\" height=\"{:.0}\"{theme_attrs} role=\"img\" aria-labelledby=\"diagram-title diagram-description\"><title id=\"diagram-title\">Architecture diagram</title><desc id=\"diagram-description\">{}</desc><style>text{{font-family:Inter,ui-sans-serif,system-ui,sans-serif;font-size:15px;fill:var(--sd-fg,#233347)}}.node-title{{font-weight:600;font-size:16px}}.port-label{{font-size:13px}}.edge{{fill:none;stroke:var(--sd-edge,#53657a);stroke-width:1.85;stroke-linejoin:round}}.flow{{fill:none;stroke:var(--sd-flow,#356bd9);stroke-width:3.2;stroke-linejoin:round}}.class-hdr{{fill:var(--sd-node,#e5eef6)}}.class-sub{{font:500 11.5px/1.2 var(--sd-font,Inter,ui-sans-serif,system-ui,sans-serif);fill:var(--sd-edge,#53657a)}}.port-type{{font:500 12px/1.35 ui-monospace,'SFMono-Regular',Consolas,monospace;fill:var(--sd-edge,#53657a)}}.class-rule{{stroke:var(--sd-border,#6b8198);stroke-opacity:.36}}.row-rule{{stroke:var(--sd-border,#6b8198);stroke-opacity:.22}}.code-box{{fill:var(--sd-code-bg,#f1f5fa);stroke:var(--sd-code-border,#c2d0e0);stroke-width:1}}.code-hdr{{fill:var(--sd-code-header,#e4ecf5)}}.code-lang{{font:600 10.5px/1 ui-monospace,'SFMono-Regular',Consolas,monospace;fill:var(--sd-edge,#53657a)}}.code-line{{font:500 12.5px/1.35 ui-monospace,'SFMono-Regular',Consolas,monospace;fill:var(--sd-fg,#233347)}}.syn-kw{{fill:var(--sd-syn-kw,#6d28d9);font-weight:600}}.syn-str{{fill:var(--sd-syn-str,#047857)}}.syn-num{{fill:var(--sd-syn-num,#b45309)}}.syn-cmt{{fill:var(--sd-syn-cmt,#526378);font-style:italic}}.syn-type{{fill:var(--sd-syn-type,#0f766e)}}.syn-fn{{fill:var(--sd-syn-fn,#1d4ed8)}}@keyframes sd-marching-ants{{from{{stroke-dashoffset:20}}to{{stroke-dashoffset:0}}}}.marching-ants{{stroke-dasharray:6 4;animation:sd-marching-ants .85s linear infinite}}@media(prefers-reduced-motion:reduce){{.marching-ants{{animation:none}}}}.sd-link{{cursor:pointer}}.sd-link:hover [data-silhouette]{{stroke-width:2.2}}.sd-link:hover path.edge{{stroke-width:2.6}}.sd-link .node-title,.sd-link .connection-label{{text-decoration:underline;text-decoration-thickness:1px;text-underline-offset:3px}}.context-node>:not(text){{opacity:.35}}.context-node text{{fill:var(--sd-edge,#53657a)}}</style>{theme_bg_rect}",
        bounds.left(),
        bounds.top(),
        bounds.w,
        bounds.h,
        bounds.w,
        bounds.h,
        escape(&description(scene))
    );
    for (i, (_, em)) in masks.iter().enumerate() {
        let _ = write!(
            s,
            "<defs><mask id=\"gap-{i}\" maskUnits=\"userSpaceOnUse\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"white\"/>",
            bounds.left(),
            bounds.top(),
            bounds.w,
            bounds.h,
            bounds.left(),
            bounds.top(),
            bounds.w,
            bounds.h
        );
        for p in &em.crossings {
            let _ = write!(
                s,
                "<circle data-crossing=\"true\" cx=\"{}\" cy=\"{}\" r=\"4\" fill=\"black\"/>",
                p.x, p.y
            );
        }
        for r in &em.knockouts {
            let _ = write!(
                s,
                "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"5\" fill=\"black\"/>",
                r.left() - 4.,
                r.top() - 4.,
                r.w + 8.,
                r.h + 8.
            );
        }
        s.push_str("</mask></defs>");
    }
    for n in scene
        .nodes
        .iter()
        .filter(|n| n.container && !crate::layout::is_record_shape(&n.shape))
    {
        let r = l.nodes[&n.id];
        let link = n.link();
        if let Some(href) = &link {
            write_link_open(&mut s, href);
        }
        let is_parallel_region = n
            .attrs
            .get("region")
            .or_else(|| n.attrs.get("parallel"))
            .is_some_and(|v| v.text.trim() == "true")
            || n.annotations
                .iter()
                .any(|a| a.name == "region" || a.name == "parallel");
        let dash = if is_parallel_region {
            " stroke-dasharray=\"6 4\""
        } else {
            ""
        };
        let _ = write!(
            s,
            "<g data-node=\"{}\" data-shape=\"container\" aria-label=\"{}\"><rect data-silhouette=\"true\" x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"12\" fill=\"var(--sd-container,#f0f4f9)\" stroke=\"var(--sd-muted,#73869b)\"{dash}/>",
            escape(&n.id),
            escape(&n.label),
            r.left(),
            r.top(),
            r.w,
            r.h
        );
        s.push_str(&decorate(n, r));
        s.push_str("</g>");
        if link.is_some() {
            s.push_str("</a>");
        }
    }
    for (z, ellipse) in scene.zones.iter().zip(&ellipses) {
        if let Some(e) = ellipse {
            let (tint, alpha) = paint(
                z.attrs
                    .get("tint")
                    .map(|v| v.text.as_str())
                    .unwrap_or("oklch(0.8 0.08 230 / 0.18)"),
            );
            let _ = write!(
                s,
                "<path data-zone=\"{}\" d=\"{}\" fill=\"{}\" fill-opacity=\"{}\"/>",
                escape(&z.id),
                ellipse_svg_path(*e),
                escape(&tint),
                alpha
            );
        }
    }
    let in_flow: BTreeSet<&str> = scene
        .flows
        .iter()
        .flat_map(|f| f.edges.iter().map(String::as_str))
        .collect();
    for e in &scene.edges {
        let r = &l.routes[&e.id];
        let (extra_class, dash) = if !e.attrs.contains_key("phantom") {
            edge_stroke_style(e)
        } else {
            ("", "")
        };
        let phantom = if e.attrs.contains_key("phantom") {
            " stroke=\"#c345ac\" stroke-dasharray=\"5 4\""
        } else {
            ""
        };
        let active = e.attrs.contains_key("active");
        let color = if active {
            "var(--sd-flow,#356bd9)"
        } else if e.attrs.contains_key("phantom") {
            "#c345ac"
        } else {
            "var(--sd-edge,#53657a)"
        };
        let width = if active {
            4.2
        } else if e.kind == EdgeKind::Bold {
            3.0
        } else {
            1.85
        };
        let covered_by_flow = in_flow.contains(e.id.as_str());
        let trim_width = if covered_by_flow && !active {
            3.2
        } else {
            width
        };
        let (shaft, heads) = connector(&r.points, e.kind, trim_width);
        let mask = gap_mask(&e.id, &masks);
        let edge_link = e.link();
        if let Some(href) = &edge_link {
            write_link_open(&mut s, href);
        }
        let _ = write!(
            s,
            "<path data-edge=\"{}\" class=\"edge{extra_class}\" style=\"stroke:{color};stroke-width:{width}\" d=\"{}\"{mask}{dash}{phantom}><title>{}</title></path>",
            escape(&e.id),
            rounded_path(&shaft),
            escape(&format!(
                "{} → {}: {}",
                e.source.node, e.target.node, e.label
            ))
        );
        if !covered_by_flow {
            for (end, points) in heads {
                let _ = write!(
                    s,
                    "<path data-edge=\"{}\" data-arrowhead=\"{end}\" d=\"{}Z\" fill=\"{color}\"/>",
                    escape(&e.id),
                    path(&points)
                );
            }
        }
        if edge_link.is_some() {
            s.push_str("</a>");
        }
    }
    for n in scene
        .nodes
        .iter()
        .filter(|n| !n.container || crate::layout::is_record_shape(&n.shape))
    {
        let r = l.nodes[&n.id];
        let node_link = n.link();
        if let Some(href) = &node_link {
            write_link_open(&mut s, href);
        }
        let _ = write!(
            s,
            "<g data-node=\"{}\" data-shape=\"{}\" aria-label=\"{}\"><title>{}</title>",
            escape(&n.id),
            escape(&n.shape),
            escape(&n.label),
            escape(&n.label)
        );
        match n.shape.as_str() {
            "initial" => {
                let rad = (r.w.min(r.h) / 2.).max(6.);
                let _ = write!(
                    s,
                    "<circle data-silhouette=\"true\" cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" fill=\"var(--sd-ink,#1c2a3a)\" stroke=\"var(--sd-ink,#1c2a3a)\"/>",
                    r.x, r.y, rad
                );
            }
            "final" => {
                let outer_r = (r.w.min(r.h) / 2.).max(8.);
                let inner_r = (outer_r * 0.56).max(4.);
                let _ = write!(
                    s,
                    "<circle data-silhouette=\"true\" cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" fill=\"var(--sd-surface,#ffffff)\" stroke=\"var(--sd-ink,#1c2a3a)\" stroke-width=\"2\"/><circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" fill=\"var(--sd-ink,#1c2a3a)\"/>",
                    r.x, r.y, outer_r, r.x, r.y, inner_r
                );
            }
            "history" | "deep_history" => {
                let rad = (r.w.min(r.h) / 2.).max(8.);
                let _ = write!(
                    s,
                    "<circle data-silhouette=\"true\" cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" fill=\"var(--sd-cache,#fff1d6)\" stroke=\"var(--sd-cache-border,#95723b)\" stroke-width=\"1.5\"/>",
                    r.x, r.y, rad
                );
            }
            "actor" => {
                let _ = write!(
                    s,
                    "<rect data-silhouette=\"true\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"18\" fill=\"var(--sd-actor,#e4f2ea)\" stroke=\"var(--sd-actor-border,#608b75)\"/><circle cx=\"{}\" cy=\"{}\" r=\"5\" fill=\"none\" stroke=\"var(--sd-actor-border,#608b75)\"/><path d=\"M {} {} Q {} {} {} {}\" fill=\"none\" stroke=\"var(--sd-actor-border,#608b75)\"/>",
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
                    ("var(--sd-cache,#fff1d6)", "var(--sd-cache-border,#95723b)")
                } else {
                    (
                        "var(--sd-decision,#fff0d8)",
                        "var(--sd-decision-border,#98702e)",
                    )
                };
                let _ = write!(
                    s,
                    "<path data-silhouette=\"true\" d=\"{}Z\" fill=\"{fill}\" stroke=\"{stroke}\"/>",
                    path(&vertices)
                );
            }
            "database" => {
                let ry = crate::shapes::CYLINDER_RY.min(r.h / 2.);
                let _ = write!(
                    s,
                    "<path data-silhouette=\"true\" d=\"M {} {} A {} {ry} 0 0 1 {} {} L {} {} A {} {ry} 0 0 1 {} {} Z\" fill=\"var(--sd-database,#e1edf9)\" stroke=\"var(--sd-database-border,#6483a2)\"/><ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{ry}\" fill=\"var(--sd-cap,#d1e3f5)\" stroke=\"var(--sd-database-border,#6483a2)\"/>",
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
                    "queue" => "var(--sd-queue,#e7e0fa)",
                    "sql_table" => "var(--sd-database,#e1edf9)",
                    "class" | "uml_class" => "var(--sd-surface,#f6f8fb)",
                    _ => "var(--sd-node,#e5eef6)",
                };
                let rx = match n.shape.as_str() {
                    "sql_table" => 2,
                    "class" | "uml_class" => 4,
                    _ => 8,
                };
                let _ = write!(
                    s,
                    "<rect data-silhouette=\"true\" x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"{rx}\" fill=\"{fill}\" stroke=\"var(--sd-border,#6b8198)\"/>",
                    r.left(),
                    r.top(),
                    r.w,
                    r.h,
                );
                if crate::layout::is_class_shape(&n.shape) {
                    let hdr_h = crate::layout::class_header_height(n).min(r.h);
                    let hb = r.top() + hdr_h;
                    let rad = 4.0f64.min(r.w / 2.).min(hdr_h);
                    let _ = write!(
                        s,
                        "<path class=\"class-hdr\" d=\"M {:.2} {:.2} a {rad:.2} {rad:.2} 0 0 1 {rad:.2} -{rad:.2} H {:.2} a {rad:.2} {rad:.2} 0 0 1 {rad:.2} {rad:.2} V {hb:.2} H {:.2} Z\" stroke=\"var(--sd-border,#6b8198)\"/>",
                        r.left(),
                        r.top() + rad,
                        r.right() - rad,
                        r.left(),
                    );
                }
                if n.shape == "queue" {
                    for k in 1..4 {
                        let x = r.left() + k as f64 * ((r.w - 8.).max(0.) / 4.).min(10.);
                        let _ = write!(
                            s,
                            "<path d=\"M {x} {} V {}\" stroke=\"var(--sd-queue-line,#b4a5d4)\"/>",
                            r.top() + 8.,
                            r.bottom() - 8.
                        );
                    }
                }
            }
        }
        s.push_str(&decorate(n, r));
        if let Some(index) = scene.story.iter().position(|id| id == &n.id) {
            let _ = write!(
                s,
                "<path data-story-outline=\"{}\" d=\"{}\" fill=\"none\" stroke=\"var(--sd-story,#3f6dc5)\" stroke-width=\"2.5\"><title>Story step {}</title></path>",
                escape(&n.id),
                crate::shapes::outline(&n.shape, r),
                index + 1
            );
        }
        if crate::layout::is_record_shape(&n.shape) {
            let is_class = crate::layout::is_class_shape(&n.shape);
            let hdr_h = crate::layout::class_header_height(n).min(r.h);
            if is_class {
                let stereo = n
                    .attrs
                    .get("stereotype")
                    .map(|v| crate::model::unquote(v.text.trim()))
                    .map(|s| s.trim().to_string())
                    .filter(|t| !t.is_empty());
                if let Some(ref st) = stereo {
                    let _ = write!(
                        s,
                        "<text class=\"class-sub\" text-anchor=\"middle\" dominant-baseline=\"central\" x=\"{:.2}\" y=\"{:.2}\">«{}»</text>",
                        r.x,
                        r.top() + 13.,
                        escape(st)
                    );
                }
                let title_y = if stereo.is_some() {
                    r.top() + 16. + (hdr_h - 16.) / 2.
                } else {
                    r.top() + hdr_h / 2.
                };
                let _ = write!(
                    s,
                    "<text class=\"node-title\" text-anchor=\"middle\" dominant-baseline=\"central\" x=\"{:.2}\" y=\"{title_y:.2}\">{}</text>",
                    r.x,
                    text_content(&crate::labels::node_title(n), r.x, 20.)
                );
            }
            let emit_row = |out: &mut String, baseline_y: f64, raw: &str| {
                if !raw.contains('\n')
                    && let Some((lhs, Some(rhs))) = Some(crate::labels::split_record_row(raw))
                {
                    let lhs_w = crate::labels::text_width(lhs) * 13. / 15.;
                    let rhs_w = crate::labels::text_width(rhs) * 13. / 15.;
                    if lhs_w + rhs_w + 36. <= r.w {
                        let _ = write!(
                            out,
                            "<text class=\"port-label\" x=\"{:.2}\" y=\"{baseline_y:.2}\">{}</text><text class=\"port-type\" text-anchor=\"end\" x=\"{:.2}\" y=\"{baseline_y:.2}\">{}</text>",
                            r.left() + 12.,
                            escape(lhs),
                            r.right() - 12.,
                            escape(rhs)
                        );
                        return;
                    }
                }
                let _ = write!(
                    out,
                    "<text class=\"port-label\" x=\"{:.2}\" y=\"{baseline_y:.2}\">{}</text>",
                    r.left() + 12.,
                    text_content(raw, r.left() + 12., 18.)
                );
            };
            let mut fixed_ports: Vec<(&crate::model::Port, f64)> = n
                .ports
                .values()
                .filter_map(|p| {
                    let PortConstraint::FixedPos { offset_grid, .. } = p.constraint else {
                        return None;
                    };
                    let y = l
                        .ports
                        .get(&format!("{}#{}", n.id, p.id))
                        .map(|lp| lp.point.y)
                        .unwrap_or_else(|| {
                            r.top() + crate::layout::record_slot_offset(n, offset_grid, 16.)
                        });
                    Some((p, y))
                })
                .collect();
            fixed_ports.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.id.cmp(&b.0.id)));
            for (pidx, &(p, y)) in fixed_ports.iter().enumerate() {
                if pidx > 0 {
                    let ry = (fixed_ports[pidx - 1].1 + y) / 2.;
                    let _ = write!(
                        s,
                        "<line class=\"row-rule\" x1=\"{:.2}\" y1=\"{ry:.2}\" x2=\"{:.2}\" y2=\"{ry:.2}\"/>",
                        r.left() + 8.,
                        r.right() - 8.
                    );
                }
                let baseline_y = y + 4. + (p.label.split('\n').count() - 1) as f64 * 9.;
                emit_row(&mut s, baseline_y, &p.label);
            }
            if is_class {
                let (extra_fields, extra_methods) = crate::layout::class_extra_rows(n);
                if !extra_fields.is_empty() || !extra_methods.is_empty() {
                    let mut slot_bottom = 0f64;
                    for &(p, _) in &fixed_ports {
                        if let PortConstraint::FixedPos { offset_grid, .. } = p.constraint {
                            slot_bottom = slot_bottom.max(
                                crate::layout::record_slot_offset(n, offset_grid, 16.)
                                    + 16.
                                    + (p.label.split('\n').count() - 1) as f64 * 18.,
                            );
                        }
                    }
                    let mut extra_y = if slot_bottom > 0. {
                        slot_bottom - 4.
                    } else {
                        hdr_h + 6.
                    };
                    for (idx, row) in extra_fields.iter().enumerate() {
                        if idx > 0 || slot_bottom > 0. {
                            let ry = r.top() + extra_y - 2.;
                            let _ = write!(
                                s,
                                "<line class=\"row-rule\" x1=\"{:.2}\" y1=\"{ry:.2}\" x2=\"{:.2}\" y2=\"{ry:.2}\"/>",
                                r.left() + 8.,
                                r.right() - 8.
                            );
                        }
                        emit_row(&mut s, r.top() + extra_y + 12., row);
                        extra_y += 18.;
                    }
                    if !extra_methods.is_empty() && (slot_bottom > 0. || !extra_fields.is_empty()) {
                        let dy = r.top() + extra_y + 2.;
                        let _ = write!(
                            s,
                            "<line class=\"class-rule\" x1=\"{:.2}\" y1=\"{dy:.2}\" x2=\"{:.2}\" y2=\"{dy:.2}\"/>",
                            r.left(),
                            r.right()
                        );
                        extra_y += 8.;
                    }
                    for (idx, row) in extra_methods.iter().enumerate() {
                        if idx > 0 {
                            let ry = r.top() + extra_y - 2.;
                            let _ = write!(
                                s,
                                "<line class=\"row-rule\" x1=\"{:.2}\" y1=\"{ry:.2}\" x2=\"{:.2}\" y2=\"{ry:.2}\"/>",
                                r.left() + 8.,
                                r.right() - 8.
                            );
                        }
                        emit_row(&mut s, r.top() + extra_y + 12., row);
                        extra_y += 18.;
                    }
                }
            }
        } else if let Some(code) = crate::highlight::measure(&n.label, &n.shape) {
            let gutter = crate::shapes::title_gutter(&n.shape);
            let center_x = r.x + gutter / 2.;
            let db_top = if n.shape == "database" {
                crate::shapes::CYLINDER_RY.min(r.h / 2.) * 2. + 4.
            } else {
                0.
            };
            let mut cy = r.top() + code.outer_y + db_top;
            if !code.title.is_empty() {
                let _ = write!(
                    s,
                    "<text class=\"node-title\" text-anchor=\"middle\" dominant-baseline=\"central\" x=\"{:.2}\" y=\"{:.2}\">{}</text>",
                    center_x,
                    cy + code.title_h / 2.,
                    text_content(&code.title, center_x, 20.)
                );
                cy += code.title_h + code.gap_title;
            }
            for (bi, block) in code.blocks.iter().enumerate() {
                if bi > 0 {
                    cy += code.gap_blocks;
                }
                let bx = center_x - block.w / 2.;
                let _ = write!(
                    s,
                    "<rect class=\"code-box\" x=\"{bx:.2}\" y=\"{cy:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"5\"/>",
                    block.w, block.h
                );
                if block.header_h > 0. {
                    let _ = write!(
                        s,
                        "<rect class=\"code-hdr\" x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"4\"/><text class=\"code-lang\" x=\"{:.2}\" y=\"{:.2}\">{}</text>",
                        bx + 1.,
                        cy + 1.,
                        block.w - 2.,
                        block.header_h - 1.,
                        bx + 10.,
                        cy + 13.,
                        escape(&block.lang.to_ascii_uppercase())
                    );
                }
                let mut ly = cy + 9. + block.header_h + 12.;
                for line_tokens in &block.lines {
                    let _ = write!(
                        s,
                        "<text class=\"code-line\" xml:space=\"preserve\" x=\"{:.2}\" y=\"{ly:.2}\">",
                        bx + 12.
                    );
                    for tok in line_tokens {
                        if let Some(cls) = tok.kind.css_class() {
                            let _ =
                                write!(s, "<tspan class=\"{cls}\">{}</tspan>", escape(&tok.text));
                        } else {
                            s.push_str(&escape(&tok.text));
                        }
                    }
                    s.push_str("</text>");
                    ly += 17.;
                }
                cy += block.h;
            }
        } else if matches!(n.shape.as_str(), "initial" | "final") {
            // Initial and final pseudostate markers render only geometry and accessible labels.
        } else {
            let raw_title = crate::labels::node_title(n);
            let default_label = n.label.is_empty()
                || n.label == n.id
                || n.id.rsplit('.').next().is_some_and(|tail| tail == n.label);
            let title_text: &str = if n.shape == "history" && default_label {
                "H"
            } else if n.shape == "deep_history" && default_label {
                "H*"
            } else {
                &raw_title
            };
            let _ = write!(
                s,
                "<text class=\"node-title\" text-anchor=\"middle\" dominant-baseline=\"central\" x=\"{:.2}\" y=\"{:.2}\">{}</text>",
                r.x + crate::shapes::title_gutter(&n.shape) / 2.,
                r.y + if n.shape == "database" {
                    crate::shapes::CYLINDER_RY.min(r.h / 2.)
                        - if n.annotations.iter().any(|a| a.name == "sim") {
                            12.
                        } else {
                            0.
                        }
                } else {
                    0.
                },
                text_content(
                    title_text,
                    r.x + crate::shapes::title_gutter(&n.shape) / 2.,
                    20.
                )
            );
        }
        s.push_str("</g>");
        if node_link.is_some() {
            s.push_str("</a>");
        }
    }
    for p in l.ports.values() {
        let unused = p
            .key
            .rsplit_once('#')
            .and_then(|(id, port)| scene.node(id).and_then(|n| n.ports.get(port)))
            .is_some_and(|port| port.explicit)
            && !scene
                .edges
                .iter()
                .any(|edge| port_key(edge, true) == p.key || port_key(edge, false) == p.key);
        let (radius, paint) = if unused {
            (
                2.5,
                "data-unused-port=\"true\" fill=\"var(--sd-bg,#ffffff)\" stroke=\"var(--sd-edge,#53657a)\" stroke-width=\"1.5\"",
            )
        } else {
            (4., "fill=\"transparent\"")
        };
        let _ = write!(
            s,
            "<circle data-port=\"{}\" cx=\"{:.2}\" cy=\"{:.2}\" r=\"{radius}\" {paint}><title>{}</title></circle>",
            escape(&p.key),
            p.point.x,
            p.point.y,
            escape(&p.key)
        );
    }
    // Keep callout ink behind flow badges so leaders cannot strike through
    // their step numbers when a label anchors at the route midpoint.
    for label in &l.labels {
        let points = crate::labels::leader_path(label);
        if points.len() > 1 {
            let d = points
                .iter()
                .enumerate()
                .map(|(i, p)| format!("{} {} {}", if i == 0 { "M" } else { "L" }, p.x, p.y))
                .collect::<Vec<_>>()
                .join(" ");
            let _ = write!(
                s,
                "<path data-callout=\"true\" data-callout-for=\"{}\" d=\"{d}\" stroke=\"var(--sd-muted,#73869b)\" stroke-width=\"1.2\" stroke-dasharray=\"3 2.5\" stroke-linejoin=\"round\" fill=\"none\"/>",
                escape(&label.feature)
            );
        }
    }
    let mut emitted_flows = BTreeSet::new();
    for flow in &scene.flows {
        for (step, id) in flow.edges.iter().enumerate() {
            if !emitted_flows.insert(id.as_str()) {
                continue;
            }
            if let Some(r) = l.routes.get(id) {
                let edge = scene.edge(id).unwrap();
                let flow_width = if edge.attrs.contains_key("active") {
                    4.2
                } else {
                    3.2
                };
                let (shaft, heads) = connector(&r.points, edge.kind, flow_width);
                let mask = gap_mask(id, &masks);
                let width_attr = if flow_width != 3.2 {
                    format!(" style=\"stroke-width:{flow_width}\"")
                } else {
                    String::new()
                };
                let (extra_class, dash) = edge_stroke_style(edge);
                let edge_link = edge.link();
                if let Some(href) = &edge_link {
                    write_link_open(&mut s, href);
                }
                let _ = write!(
                    s,
                    "<g aria-label=\"{} step {}\"><path data-edge=\"{}\" class=\"flow{extra_class}\"{width_attr}{dash} d=\"{}\"{mask}/></g>",
                    escape(&flow.label),
                    step + 1,
                    escape(id),
                    rounded_path(&shaft)
                );
                for (end, points) in heads {
                    let _ = write!(
                        s,
                        "<path data-edge=\"{}\" data-arrowhead=\"{end}\" d=\"{}Z\" fill=\"var(--sd-flow,#356bd9)\"/>",
                        escape(id),
                        path(&points)
                    );
                }
                if edge_link.is_some() {
                    s.push_str("</a>");
                }
            }
        }
    }
    for n in scene
        .nodes
        .iter()
        .filter(|n| n.container && !crate::layout::is_record_shape(&n.shape))
    {
        let r = l.nodes[&n.id];
        let hx = container_header_rect_with_ports(n, r, &l.ports)
            .map_or(r.left() + 16., |hr| hr.left() + 4.);
        let link = n.link();
        if let Some(href) = &link {
            write_link_open(&mut s, href);
        }
        let _ = write!(
            s,
            "<text data-owner-node=\"{}\" class=\"node-title\" x=\"{}\" y=\"{}\">{}</text>",
            escape(&n.id),
            hx,
            r.top() + 22. + (header_height(n) - 24.) / 2.,
            text_content(&n.label, hx, 20.)
        );
        if link.is_some() {
            s.push_str("</a>");
        }
    }
    for label in &l.labels {
        let edge_link = scene.edge(&label.feature).and_then(|e| e.link());
        if let Some(href) = &edge_link {
            write_link_open(&mut s, href);
        }
        let _ = write!(
            s,
            "<g data-label=\"{}\" data-draggable-label=\"{}\" data-label-x=\"{}\" data-label-y=\"{}\" data-knockout=\"{}\">",
            escape(&label.feature),
            scene.edge(&label.feature).is_some(),
            label.bounds.x,
            label.bounds.y,
            label.knockout && !label.callout
        );
        let r = label.bounds;
        let class = if scene.node(&label.feature).is_some() {
            "node-title"
        } else {
            "connection-label"
        };
        let flow = scene.flows.iter().any(|f| f.edges.contains(&label.feature));
        if flow || label.knockout && !label.callout {
            let inside_container = !flow
                && scene.nodes.iter().any(|n| {
                    n.container
                        && !crate::layout::is_record_shape(&n.shape)
                        && l.nodes.get(&n.id).is_some_and(|nr| nr.contains(r.center()))
                });
            let _ = write!(
                s,
                "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"5\" fill=\"{}\"/>",
                r.left() - 4.,
                r.top() - 4.,
                r.w + 8.,
                r.h + 8.,
                if flow {
                    "var(--sd-flow,#356bd9)"
                } else if inside_container {
                    "var(--sd-container,#f0f4f9)"
                } else {
                    "var(--sd-bg,#ffffff)"
                }
            );
        }
        let _ = write!(
            s,
            "<text class=\"{class}\" style=\"{}\" x=\"{:.2}\" y=\"{:.2}\" text-anchor=\"middle\" dominant-baseline=\"central\">{}</text>",
            if flow {
                "fill:var(--sd-flow-text,#ffffff)"
            } else {
                ""
            },
            r.x,
            r.y,
            text_content(&label.text, r.x, 18.)
        );
        s.push_str("</g>");
        if edge_link.is_some() {
            s.push_str("</a>");
        }
    }
    for (z, r) in titles {
        let _ = write!(
            s,
            "<rect data-zone-title-surface=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"8\" fill=\"var(--sd-bg,#ffffff)\" stroke=\"var(--sd-border,#6b8198)\"/>",
            escape(&z.id),
            r.left(),
            r.top(),
            r.w,
            r.h
        );
        let _ = write!(
            s,
            "<text data-zone-label=\"{}\" x=\"{}\" y=\"{}\" style=\"font-weight:600;fill:var(--sd-zone-text,#29493f);paint-order:stroke;stroke:var(--sd-bg,#ffffff);stroke-width:3;stroke-linejoin:round\">{}</text>",
            escape(&z.id),
            r.left() + 8.,
            r.y + 4.,
            text_content(&z.label, r.left() + 8., 18.)
        );
    }
    s.push_str("</svg>\n");
    namespace_svg(s)
}

type Arrowheads = Vec<(&'static str, [Point; 3])>;

/// Trim the shaft to each arrow's base. Drawing the heads in user units keeps
/// stroke width, color and the exact silhouette attachment independent of SVG
/// marker scaling and avoids a square shaft protruding through the tip.
fn connector(points: &[Point], kind: EdgeKind, width: f64) -> (Vec<Point>, Arrowheads) {
    let mut shaft = crate::routing::coalesced(points);
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
        let available = (tip.x - neighbor.x).hypot(tip.y - neighbor.y);
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
fn ellipse_svg_path(e: crate::zones::ZoneEllipse) -> String {
    let (c, s) = (e.theta.cos(), e.theta.sin());
    let p0 = Point::new(e.cx + e.rx * c, e.cy + e.rx * s);
    let p1 = Point::new(e.cx - e.ry * s, e.cy + e.ry * c);
    let p2 = Point::new(e.cx - e.rx * c, e.cy - e.rx * s);
    let p3 = Point::new(e.cx + e.ry * s, e.cy - e.ry * c);
    let deg = e.theta.to_degrees();
    format!(
        "M {:.2} {:.2} A {:.2} {:.2} {:.2} 0 1 {:.2} {:.2} A {:.2} {:.2} {:.2} 0 1 {:.2} {:.2} A {:.2} {:.2} {:.2} 0 1 {:.2} {:.2} A {:.2} {:.2} {:.2} 0 1 {:.2} {:.2} Z",
        p0.x,
        p0.y,
        e.rx,
        e.ry,
        deg,
        p1.x,
        p1.y,
        e.rx,
        e.ry,
        deg,
        p2.x,
        p2.y,
        e.rx,
        e.ry,
        deg,
        p3.x,
        p3.y,
        e.rx,
        e.ry,
        deg,
        p0.x,
        p0.y,
    )
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
        if tone.opacity >= 1. {
            continue;
        }
        svg = svg.replace(
            &format!("<g data-node=\"{}\"", escape(&id)),
            &format!(
                "<g class=\"context-node\" data-focus-opacity=\"{}\" data-node=\"{}\"",
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
    fn crossing_gaps_include_collinear_stub_vertices_but_not_junctions() {
        let route = |id: &str, points: Vec<Point>| Route {
            edge: id.into(),
            points,
        };
        let mut l = Layout::default();
        l.routes.insert(
            "horizontal".into(),
            route(
                "horizontal",
                vec![
                    Point::new(-40., 0.),
                    Point::new(0., 0.),
                    Point::new(40., 0.),
                ],
            ),
        );
        l.routes.insert(
            "vertical".into(),
            route(
                "vertical",
                vec![
                    Point::new(0., -40.),
                    Point::new(0., 0.),
                    Point::new(0., 40.),
                ],
            ),
        );
        assert_eq!(
            crossing_gaps(&l, |_| true)["vertical"],
            vec![Point::new(0., 0.)]
        );
        assert!(crossing_gaps(&l, |id| id != "horizontal").is_empty());
        l.routes.get_mut("vertical").unwrap().points.remove(0);
        assert!(crossing_gaps(&l, |_| true).is_empty());
    }

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

    #[test]
    fn inline_knockout_emits_mask_cutout_and_container_aware_fill() {
        let root_diag = crate::compile(
            "a @pin(grid: 0, 0)\nb @pin(grid: 20, 0)\na.E -> b.W: payload @label(slot: inline, slide: 0.5)\n",
        )
        .unwrap();
        let root_svg = root_diag.to_svg();
        assert!(root_svg.contains("fill=\"black\"/>"));
        assert!(root_svg.contains("fill=\"var(--sd-bg,#ffffff)\"/>"));
        assert!(root_svg.contains("mask=\"url(#"));

        let cont_diag = crate::compile(
            "box {\n a @pin(grid: 0, 0)\n b @pin(grid: 20, 0)\n a.E -> b.W: payload @label(slot: inline, slide: 0.5)\n}\n",
        )
        .unwrap();
        let cont_svg = cont_diag.to_svg();
        assert!(cont_svg.contains("fill=\"black\"/>"));
        assert!(cont_svg.contains("fill=\"var(--sd-container,#f0f4f9)\"/>"));
    }

    #[test]
    fn overlapping_flows_deduplicate_arrowheads_and_coalesced_connector_handles_collinear_stubs() {
        let diag = crate::compile(
            "a @pin(grid: 0, 0)\nb @pin(grid: 20, 0)\na -> b\nflow f1 { a -> b }\nflow f2 { a -> b }\n",
        )
        .unwrap();
        let out_svg = diag.to_svg();
        assert_eq!(out_svg.matches("class=\"flow\"").count(), 1);
        assert_eq!(out_svg.matches("data-arrowhead=\"end\"").count(), 1);
        assert!(out_svg.contains("fill=\"var(--sd-flow,#356bd9)\""));

        // Collinear vertex only 2px from end must coalesce so arrowhead is not squished to 1.6px.
        let (shaft, heads) = connector(
            &[
                Point::new(0., 0.),
                Point::new(98., 0.),
                Point::new(100., 0.),
            ],
            EdgeKind::Directed,
            1.7,
        );
        assert_eq!(shaft.len(), 2);
        let tip = heads[0].1[0];
        assert!((shaft[1].distance(tip) - 9.7).abs() < 1e-6);
    }

    #[test]
    fn hyperlinks_wrap_nodes_containers_edges_and_labels_safely() {
        let diag = crate::compile(
            "svc: \"Order Service\" [link: \"https://example.com/services/orders\"]\n\
             store: \"Storage\" [link: \"/guide/concepts\"] {\n\
               db: \"Orders DB\" [href: \"https://example.com/db\"]\n\
             }\n\
             svc -> store.db: \"writes\" [link: \"https://example.com/runbooks/db-writes\"]\n",
        )
        .unwrap();
        let out = diag.to_svg();
        assert!(out.contains("class=\"sd-link\" data-link=\"true\" href=\"https://example.com/services/orders\" target=\"_blank\" rel=\"noopener noreferrer\""));
        assert!(out.contains("href=\"/guide/concepts\""));
        assert!(!out.contains("href=\"/guide/concepts\" target=\"_blank\""));
        assert!(out.contains("href=\"https://example.com/runbooks/db-writes\""));
        assert!(crate::compile("bad [link: \"javascript:alert(1)\"]\n").is_err());
    }
}
