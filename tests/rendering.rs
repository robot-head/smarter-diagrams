use smarter_diagrams::{compile, layout::*, model::*};

#[test]
fn orthogonal_routes_touch_each_silhouette_and_keep_fixed_port_coordinates() {
    for shape in [
        "rect",
        "actor",
        "cache",
        "diamond",
        "decision",
        "database",
        "queue",
        "sql_table",
    ] {
        for (side, x, y) in [
            (Side::N, 0, -24),
            (Side::E, 24, 0),
            (Side::S, 0, 24),
            (Side::W, -24, 0),
        ] {
            for ratio in [0.125, 0.25, 0.5, 0.75, 0.875] {
                let source = format!(
                    "node [shape: {shape}] @pin(grid: 0,0, lock: [w: 12g, h: 6g])\nother @pin(grid: {x},{y})\nnode.{side:?}@{ratio} -> other\n"
                );
                let d = compile(&source).unwrap_or_else(|error| panic!("{source}: {error}"));
                let edge = &d.scene.edges[0];
                let port = &d.layout.ports[&port_key(edge, true)];
                let r = d.layout.nodes["node"];
                let p = port.point;
                let horizontal = matches!(side, Side::E | Side::W);
                let expected = if horizontal {
                    r.top() + ratio * r.h
                } else {
                    r.left() + ratio * r.w
                };
                assert!(
                    (if horizontal { p.y } else { p.x } - expected).abs() < 1e-6,
                    "{source}"
                );
                let (nx, ny) = side.normal();
                assert!(
                    (port.stub.x * nx + port.stub.y * ny)
                        >= (if horizontal { r.w } else { r.h }) / 2. + 16. - 1e-6
                );
                let on_outline = match shape {
                    "diamond" | "decision" => {
                        ((p.x - r.x).abs() / (r.w / 2.) + (p.y - r.y).abs() / (r.h / 2.) - 1.).abs()
                            < 1e-6
                    }
                    "cache" if horizontal => {
                        let inset = 10. * (p.y - r.top()) / r.h;
                        (p.x - if side == Side::E {
                            r.right() - inset
                        } else {
                            r.left() + 10. - inset
                        })
                        .abs()
                            < 1e-6
                    }
                    "database" if !horizontal => {
                        let cy = if side == Side::N {
                            r.top() + 8.
                        } else {
                            r.bottom() - 8.
                        };
                        (((p.x - r.x) / (r.w / 2.)).powi(2) + ((p.y - cy) / 8.).powi(2) - 1.).abs()
                            < 1e-6
                    }
                    "actor" if horizontal && (p.y - r.y).abs() > r.h / 2. - 18. => {
                        let cx = if side == Side::E {
                            r.right() - 18.
                        } else {
                            r.left() + 18.
                        };
                        let cy = if p.y < r.y {
                            r.top() + 18.
                        } else {
                            r.bottom() - 18.
                        };
                        ((p.x - cx).powi(2) + (p.y - cy).powi(2) - 18_f64.powi(2)).abs() < 1e-6
                    }
                    _ => {
                        (if horizontal {
                            (p.x - r.x).abs() - r.w / 2.
                        } else {
                            (p.y - r.y).abs() - r.h / 2.
                        })
                        .abs()
                            < 1e-6
                    }
                };
                assert!(
                    on_outline,
                    "detached endpoint: {shape} {side:?} {ratio}: {p:?}"
                );
                assert_eq!(d.layout.routes[&edge.id].points[0], p);
                validate(&d.scene, &d.layout, &Default::default()).unwrap();
            }
        }
    }
}

#[test]
fn documented_connector_artifacts_stay_connected_and_labels_leave_the_ink_clear() {
    for (page, indices) in [
        (include_str!("../site/language/basics.md"), vec![0, 1, 3]),
        (include_str!("../site/language/ports.md"), vec![3, 4]),
    ] {
        let page = page.replace("\r\n", "\n");
        let sources: Vec<_> = page
            .split("```diagram\n")
            .skip(1)
            .map(|s| s.split("```").next().unwrap())
            .collect();
        for index in indices {
            let d = compile(sources[index]).unwrap();
            for label in &d.layout.labels {
                assert!(
                    !label.knockout && !label.callout,
                    "unexpected connector cutout: {label:?}"
                );
                for route in d.layout.routes.values() {
                    assert!(
                        route
                            .points
                            .windows(2)
                            .all(|p| !label.bounds.inflate(2.).blocks(p[0], p[1])),
                        "label erases connector: {label:?}"
                    );
                }
            }
            let svg = d.to_svg();
            assert!(!svg.contains("<marker"));
            for port in svg.split("<circle data-port=").skip(1) {
                assert!(
                    port.split('>')
                        .next()
                        .unwrap()
                        .contains("fill=\"transparent\"")
                        || port
                            .split('>')
                            .next()
                            .unwrap()
                            .contains("data-unused-port=\"true\"")
                );
            }
            for label in svg.split("<g data-label=").skip(1) {
                assert!(!label.split("</g>").next().unwrap().contains("<rect"));
            }
        }
    }
}

#[test]
fn visual_qa_fixtures_have_clear_titles_and_leaders() {
    for source in [
        "broker: Event log [shape: queue] { port orders [side: E, ratio: 0.5] }\nbroker#orders -> worker.W: consume\n",
        "α: \"Public API\"; β: \"Ledger λ\"\nα -> β: \"read / write\"\n",
        include_str!("../examples/architecture.diagram"),
    ] {
        let d = compile(source).unwrap();
        for (i, label) in d.layout.labels.iter().enumerate() {
            if let Some((a, b)) = smarter_diagrams::labels::leader(label) {
                // Leaders terminate outside their own box and cannot enter a
                // different selected label, in either horizontal or diagonal form.
                for (j, other) in d.layout.labels.iter().enumerate() {
                    for t in 0..=100 {
                        let p = Point::new(
                            a.x + (b.x - a.x) * t as f64 / 100.,
                            a.y + (b.y - a.y) * t as f64 / 100.,
                        );
                        assert!(
                            !other.bounds.contains(p),
                            "leader {i} enters label {j}: {source}"
                        );
                    }
                }
            }
        }
        for node in &d.scene.nodes {
            if node.shape == "queue" {
                assert!(
                    d.layout.nodes[&node.id].w
                        >= smarter_diagrams::labels::text_width(&node.label) * 16. / 15. + 64.
                            - 1e-6
                );
            }
        }
    }
}

#[test]
fn records_keep_their_fixed_slots_below_a_separate_title() {
    let d = compile(
        "api -> orders#id\norders: Orders [shape: sql_table] { port id: uuid [side: W, pos: 1] }\n",
    )
    .unwrap();
    let body = d.layout.nodes["orders"];
    let slot = &d.layout.ports["orders#id"];
    assert!((slot.point.y - body.top() - 16.).abs() < 1e-6);
    let title = d
        .layout
        .labels
        .iter()
        .find(|l| l.feature == "orders")
        .unwrap();
    assert!(title.bounds.bottom() < body.top());
    assert!(d.to_svg().contains("Orders"));
}

#[test]
fn unused_explicit_ports_are_visible_without_covering_connected_tips() {
    let d=compile("a -> broker#input\nbroker { port input [side: W, ratio: 0.25]; port ack [side: W, ratio: 0.75] }\n").unwrap();
    let svg = d.to_svg();
    for (id, unused) in [("broker#input", false), ("broker#ack", true)] {
        let tag = svg
            .split(&format!("<circle data-port=\"{id}\""))
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert_eq!(tag.contains("data-unused-port"), unused);
    }
}

#[test]
fn fresh_unpinned_containers_fit_their_children() {
    let d = compile("prod { api -> db }\nclient -> prod.api\n").unwrap();
    let children = d.layout.nodes["prod.api"].union(d.layout.nodes["prod.db"]);
    assert!((d.layout.nodes["prod"].h - children.h - 88.).abs() < 1e-6);
    assert!(d.layout.nodes["prod"].encloses(children));
}

#[test]
fn arrowheads_match_active_phantom_and_flow_colors_and_inline_is_explicit() {
    for (attrs, color) in [
        ("", "var(--sd-edge,#53657a)"),
        ("[active: true]", "var(--sd-flow,#356bd9)"),
        ("[phantom: true]", "#c345ac"),
    ] {
        let svg = compile(&format!("a -> b {attrs}\n")).unwrap().to_svg();
        let head = svg
            .split("data-arrowhead=\"end\"")
            .nth(1)
            .unwrap()
            .split('>')
            .next()
            .unwrap();
        assert!(head.contains(&format!("fill=\"{color}\"")));
    }
    let svg = compile("a -> b\nflow f { a -> b }\n").unwrap().to_svg();
    let flow_head = svg
        .split("data-arrowhead=\"end\"")
        .last()
        .unwrap()
        .split('>')
        .next()
        .unwrap();
    assert!(flow_head.contains("fill=\"var(--sd-flow,#356bd9)\""));
    let d = compile("a -> b: inline @label(slot: inline, slide: 0.5)\n").unwrap();
    assert!(d.layout.labels[0].knockout);
    assert!(
        d.to_svg()
            .split("<g data-label=")
            .nth(1)
            .unwrap()
            .contains("<rect")
    );
}

#[test]
fn long_decisions_reserve_an_interior_title_rectangle() {
    let source = "node: \"Approve international payment request?\" [shape: diamond]\n";
    let diagram = compile(source).unwrap();
    let r = diagram.layout.nodes["node"];
    let svg = diagram.to_svg();
    let lines: Vec<_> = svg
        .split("<tspan ")
        .skip(1)
        .map(|span| span.split_once('>').unwrap().1.split_once('<').unwrap().0)
        .collect();
    assert!(lines.len() > 1, "long titles should wrap");
    assert_eq!(lines.join(" "), diagram.scene.nodes[0].label);
    let width = lines
        .iter()
        .map(|line| smarter_diagrams::labels::text_width(line) * 16. / 15.)
        .fold(0., f64::max);
    assert!((width + 16.) / r.w + (lines.len() as f64 * 20.) / r.h <= 1.);
    let locked = format!("{} @pin(grid: 0,0, lock: [w: 8g, h: 3g])\n", source.trim());
    assert!(
        compile(&locked)
            .unwrap_err()
            .to_string()
            .contains("decision title")
    );
}

#[test]
fn database_title_is_centered_beneath_the_top_cap() {
    let d = compile("db: Database\n").unwrap();
    let r = d.layout.nodes["db"];
    let y = r.y + 8.;
    assert!(
        d.to_svg()
            .contains(&format!("y=\"{y:.2}\">Database</text>"))
    );
}

#[test]
fn simulation_titles_reserve_space_for_the_occupancy_strip() {
    let d = compile("db: Database @sim(rate: 10, service_rate: 20)\n").unwrap();
    assert!(d.to_svg().contains("y=\"-4.00\">Database</text>"));
    let d =
        compile("db: Organization membership history service @sim(rate: 10, service_rate: 20)\n")
            .unwrap();
    assert!(d.layout.nodes["db"].h >= 116.);
}
