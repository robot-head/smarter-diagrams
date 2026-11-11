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
                );
            }
            for label in svg.split("<g data-label=").skip(1) {
                assert!(!label.split("</g>").next().unwrap().contains("<rect"));
            }
        }
    }
}

#[test]
fn arrowheads_match_active_phantom_and_flow_colors_and_inline_is_explicit() {
    for (attrs, color) in [
        ("", "#53657a"),
        ("[active: true]", "#356bd9"),
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
    assert!(flow_head.contains("fill=\"#356bd9\""));
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
