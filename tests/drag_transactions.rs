use smarter_diagrams::{
    edit::Drag,
    layout::port_key,
    layout::{LayoutOptions, Point, Rect},
    *,
};

fn simulation() -> Document {
    Document::new(include_str!("../examples/simulation.diagram")).unwrap()
}
fn at(node: &str, x: f64, y: f64) -> Drag {
    Drag {
        node: node.into(),
        position: Point::new(x, y),
    }
}

#[test]
fn release_commits_exact_preview_and_reopens_valid_geometry() {
    let mut doc = simulation();
    let original = doc.snapshot();
    let mut drag = doc.begin_drag().unwrap();
    drag.update(&[at("gateway", 305.3, 37.4)]).unwrap();
    let shown = drag.snapshot();
    assert_eq!(doc.revision(), 0);
    assert_eq!(doc.source(), original.parse.source.as_ref());
    assert_eq!(
        shown.layout.nodes["gateway"].center(),
        Point::new(305.3, 37.4)
    );
    for id in ["client", "worker", "db"] {
        assert_eq!(shown.layout.nodes[id], original.layout.nodes[id]);
    }
    doc.commit_drag(&drag).unwrap();
    assert_eq!(doc.snapshot().to_svg(), shown.to_svg());
    assert_eq!(doc.revision(), 1);
    let reopened = compile(doc.source()).unwrap();
    assert_eq!(reopened.layout.nodes, shown.layout.nodes);
    layout::validate(&reopened.scene, &reopened.layout, &LayoutOptions::default()).unwrap();
    assert!(doc.undo());
    assert_eq!(doc.snapshot().to_svg(), original.to_svg());
    assert!(!doc.undo());
    assert!(doc.redo());
    assert_eq!(doc.snapshot().to_svg(), shown.to_svg());
}

#[test]
fn route_updates_stay_valid_at_idle_and_restore_the_exact_origin() {
    let mut doc = simulation();
    let baseline = doc.snapshot();
    let mut drag = doc.begin_drag().unwrap();
    let target = at("gateway", 305.3, 137.4);
    drag.update(std::slice::from_ref(&target)).unwrap();
    for p in [(304., 96.), (100., 200.), (700., -100.), (272., 64.)] {
        drag.update(&[at("gateway", p.0, p.1)]).unwrap();
        drag.settle().unwrap();
    }
    drag.update(&[target]).unwrap();
    drag.settle().unwrap();
    let stable = drag.snapshot().to_svg();
    drag.update(&[at("gateway", 305.3, 137.4)]).unwrap();
    assert_eq!(drag.snapshot().to_svg(), stable);
    drag.update(&[at("gateway", 240., 0.)]).unwrap();
    assert_eq!(drag.snapshot().to_svg(), baseline.to_svg());
    doc.commit_drag(&drag).unwrap();
    assert_eq!(doc.revision(), 0);
    assert!(!doc.undo());
}

#[test]
fn invalid_positions_cannot_commit_and_the_same_gesture_recovers() {
    let mut doc = simulation();
    let baseline = doc.snapshot().to_svg();
    let mut drag = doc.begin_drag().unwrap();
    drag.update(&[at("gateway", 305.3, 137.4)]).unwrap();
    let good = drag.snapshot().to_svg();
    for positions in [
        vec![at("gateway", 480., 0.)],
        vec![at("gateway", f64::NAN, 0.)],
        vec![at("missing", 0., 0.)],
        vec![at("gateway", 0., 0.), at("gateway", 1., 1.)],
    ] {
        assert!(drag.update(&positions).is_err());
        assert_eq!(drag.snapshot().to_svg(), good);
        assert!(doc.commit_drag(&drag).is_err());
        assert_eq!(doc.snapshot().to_svg(), baseline);
        assert_eq!(doc.revision(), 0);
    }
    drag.update(&[at("gateway", 304.25, 3.)]).unwrap();
    doc.commit_drag(&drag).unwrap();
    assert_eq!(
        doc.snapshot().layout.nodes["gateway"].center(),
        Point::new(304.25, 3.)
    );
}

#[test]
fn stale_or_foreign_snapshots_cannot_replace_a_document() {
    let mut doc = simulation();
    let mut drag = doc.begin_drag().unwrap();
    drag.update(&[at("gateway", 305.3, 137.4)]).unwrap();
    let mut other = simulation();
    assert!(other.commit_drag(&drag).is_err());
    doc.edit(
        doc.revision(),
        &[edit::DiagramEditOp::PinNode {
            node: "client".into(),
            pin: edit::PinExpr::AbsoluteGrid { x: 0, y: 4 },
            size_lock: None,
        }],
        edit::Placement::Auto,
    )
    .unwrap();
    let changed = doc.source().to_owned();
    assert!(doc.commit_drag(&drag).is_err());
    assert_eq!(doc.source(), changed);
}

#[test]
fn container_children_ports_and_comments_survive_one_transaction() {
    let source = "# keep 😀\r\nprod {\r\n api { port out [side: E] }\r\n db\r\n api#out -> db\r\n}\r\nother\r\n";
    let mut doc = Document::new(source).unwrap();
    let before = doc.snapshot();
    let mut drag = doc.begin_drag().unwrap();
    let position = before.layout.nodes["prod"].center().offset(320.25, -160.5);
    drag.update(&[Drag {
        node: "prod".into(),
        position,
    }])
    .unwrap();
    let after = drag.snapshot();
    for id in ["prod", "prod.api", "prod.db"] {
        assert!(
            after.layout.nodes[id]
                .center()
                .distance(before.layout.nodes[id].center().offset(320.25, -160.5))
                < 1e-6
        );
    }
    for (key, port) in &before.layout.ports {
        assert!(
            after.layout.ports[key]
                .point
                .distance(port.point.offset(320.25, -160.5))
                < 1e-6
        );
    }
    assert_eq!(after.layout.nodes["other"], before.layout.nodes["other"]);
    doc.commit_drag(&drag).unwrap();
    assert!(doc.source().starts_with("# keep 😀\r\nprod {\r\n"));
    assert_eq!(compile(doc.source()).unwrap().to_svg(), after.to_svg());
}

#[test]
fn every_port_direction_keeps_valid_paths_through_reversals() {
    for source in ["N", "E", "S", "W"] {
        for target in ["N", "E", "S", "W"] {
            let doc = Document::new(&format!(
                "a @pin(grid: 0,0)\nb @pin(grid: 20,10)\na.{source} -> b.{target}\n"
            ))
            .unwrap();
            let mut drag = doc.begin_drag().unwrap();
            for p in [
                (13.25, 17.75),
                (64., -32.),
                (-128., 240.),
                (420., 320.),
                (13.25, 17.75),
            ] {
                drag.update(&[at("a", p.0, p.1)])
                    .unwrap_or_else(|e| panic!("{source} -> {target} at {p:?}: {e}"));
                drag.settle().unwrap();
                let shown = drag.snapshot();
                let edge = &shown.scene.edges[0];
                assert_eq!(
                    shown.layout.routes[&edge.id].points[0],
                    shown.layout.ports[&port_key(edge, true)].point
                );
                assert_eq!(
                    *shown.layout.routes[&edge.id].points.last().unwrap(),
                    shown.layout.ports[&port_key(edge, false)].point
                );
            }
        }
    }
}

#[test]
fn custom_grid_and_alignment_have_no_hidden_drop_snap() {
    let mut doc = Document::with_options(
        "a -> b\nalign row [a,b]\n",
        LayoutOptions {
            grid: 10.,
            ..Default::default()
        },
    )
    .unwrap();
    let mut drag = doc.begin_drag().unwrap();
    drag.update(&[at("b", 217.375, 3.125)]).unwrap();
    doc.commit_drag(&drag).unwrap();
    assert_eq!(
        doc.snapshot().layout.nodes["b"].center(),
        Point::new(217.375, 3.125)
    );
    assert!(doc.snapshot().scene.relations.is_empty());
}

#[test]
fn corrupt_cached_routes_are_repaired_instead_of_reused() {
    let d = compile("a @pin(grid: 0,0)\nb @pin(grid: 20,0)\na.E -> b.W\n").unwrap();
    let mut bad = d.layout.clone();
    let edge = &d.scene.edges[0];
    let a = &bad.ports[&port_key(edge, true)];
    let b = &bad.ports[&port_key(edge, false)];
    bad.routes.get_mut(&edge.id).unwrap().points = vec![
        a.point,
        a.stub,
        a.stub.offset(0., 64.),
        a.stub.offset(-8., 64.),
        a.stub.offset(-8., 0.),
        b.stub,
        b.point,
    ];
    assert!(layout::validate(&d.scene, &bad, &Default::default()).is_err());
    let repaired = layout::layout(&d.scene, &Default::default(), Some(&bad)).unwrap();
    layout::validate(&d.scene, &repaired, &Default::default()).unwrap();
    assert_eq!(repaired.routes, d.layout.routes);
}

#[test]
fn facing_overlapping_stubs_form_a_straight_shaft_without_a_hairpin() {
    let d = compile("a.E -> b.W: request\nb.S -> c.N: response\nc.W -> a.S: retry\n").unwrap();
    let r = &d.layout.routes["a->b"];
    let length = r
        .points
        .windows(2)
        .map(|p| p[0].distance(p[1]))
        .sum::<f64>();
    assert!((length - r.points[0].distance(*r.points.last().unwrap())).abs() < 1e-6);
}

#[test]
fn locked_channels_join_with_their_actual_vertical_direction() {
    for y in [8, -8] {
        let d=compile(&format!("api: API @pin(grid: 0,0)\ndb: Database @pin(grid: 20,{y})\napi.E -> db.W: request @route(channel_x: 10g)\n")).unwrap();
        layout::validate(&d.scene, &d.layout, &Default::default()).unwrap();
        assert!(
            d.layout.routes["api->db"]
                .points
                .windows(2)
                .any(|p| (p[0].x - 160.).abs() < 1e-6
                    && (p[1].x - 160.).abs() < 1e-6
                    && (p[0].y - p[1].y).abs() > 64.)
        );
    }
}

#[test]
fn client_micro_motion_keeps_the_same_elbow_and_caption_rail() {
    let doc = simulation();
    let mut gesture = doc.begin_drag().unwrap();
    let mut checked = 0;
    for x in [0., 24., 48., 72.] {
        for y in [-120., -96., -72., -48., 48., 72., 96., 120.] {
            if gesture.update(&[at("client", x, y)]).is_err() {
                continue;
            }
            let first = gesture.snapshot();
            if gesture.update(&[at("client", x + 0.1, y + 0.1)]).is_err() {
                continue;
            }
            let next = gesture.snapshot();
            let a = &first.layout.routes["client->gateway"].points;
            let edge = first.scene.edge("client->gateway").unwrap();
            let source = &first.layout.ports[&port_key(edge, true)];
            let target = &first.layout.ports[&port_key(edge, false)];
            let channel = (source.stub.x + target.stub.x) / 2.;
            assert!(
                a.windows(2).any(|p| (p[0].x - channel).abs() < 1e-6
                    && (p[1].x - channel).abs() < 1e-6
                    && (p[0].y - p[1].y).abs() > 1.),
                "Client must use the continuous midpoint corridor at {x},{y}"
            );

            let b = &next.layout.routes["client->gateway"].points;
            assert_eq!(a.len(), b.len(), "elbow topology changed at {x},{y}");
            for (a, b) in a.iter().zip(b) {
                assert!(a.distance(*b) < 1., "elbow jumped at {x},{y}");
            }
            let label = |d: &Diagram| {
                d.layout
                    .labels
                    .iter()
                    .find(|l| l.feature == "client->gateway")
                    .unwrap()
                    .clone()
            };
            let (a, b) = (label(&first), label(&next));
            assert_eq!((a.callout, a.knockout), (b.callout, b.knockout));
            assert!(
                a.bounds.center().distance(b.bounds.center()) < 1.,
                "caption jumped at {x},{y}: {a:?} -> {b:?}"
            );
            checked += 1;
        }
    }
    assert!(checked >= 24);
}

#[test]
fn database_backtracking_detour_survives_two_axis_micro_motion() {
    let source = format!(
        "{}\n@layout {{\nclient @pin(grid: 0,0)\ngateway @pin(grid: 15,0)\nworker @pin(grid: 30,0)\ndb @pin(grid: 10,25)\n}}\n",
        include_str!("../examples/simulation.diagram")
    );
    let doc = Document::new(&source).unwrap();
    let mut drag = doc.begin_drag().unwrap();
    let mut first = None;
    for i in 0..80 {
        let delta = if i % 2 == 0 { 0.1 } else { -0.1 };
        drag.update(&[at("db", 160. + delta, 400. + delta)])
            .unwrap();
        let shown = drag.snapshot();
        let points = &shown.layout.routes["worker->db"].points;
        let label = shown
            .layout
            .labels
            .iter()
            .find(|l| l.feature == "worker->db")
            .unwrap();
        let bounds = points
            .iter()
            .copied()
            .fold(Rect::new(points[0].x, points[0].y, 0., 0.), |r, p| {
                r.union(Rect::new(p.x, p.y, 0., 0.))
            });
        if let Some((old, label_center)) = first {
            let old: Rect = old;
            let label_center: Point = label_center;
            assert!(
                old.center().distance(bounds.center()) < 1.,
                "detour corridor jumped"
            );
            assert!(
                label_center.distance(label.bounds.center()) < 1.,
                "detour caption jumped at {i}: {label_center:?} -> {label:?}; points={points:?}"
            );
        } else {
            first = Some((bounds, label.bounds.center()));
        }
        drag.settle().unwrap();
    }
}

#[test]
fn automatic_attachments_consider_obstacles_and_explicit_sides_stay_fixed() {
    for fixed in [false, true] {
        let edge = if fixed {
            "worker.E -> db.W"
        } else {
            "worker -> db"
        };
        let d = compile(&format!(
            "gateway @pin(grid: 15,0)\nworker @pin(grid: 30,0)\ndb @pin(grid: 10,25)\n{edge}\n"
        ))
        .unwrap();
        let e = &d.scene.edges[0];
        let a = &d.layout.ports[&port_key(e, true)];
        let b = &d.layout.ports[&port_key(e, false)];
        if fixed {
            assert_eq!(a.side, model::Side::E);
            assert_eq!(b.side, model::Side::W);
        } else {
            assert_eq!(a.side, model::Side::S);
            assert_eq!(b.side, model::Side::E);
        }
    }
}

#[test]
fn free_attachment_sides_have_a_switching_cost_during_micro_motion() {
    let doc = Document::new("a @pin(grid: 0,0)\nb @pin(grid: 20,20)\na -> b\n").unwrap();
    let mut drag = doc.begin_drag().unwrap();
    let mut pair = None;
    for i in 0..60 {
        drag.update(&[at("a", 0., if i % 2 == 0 { 0.1 } else { -0.1 })])
            .unwrap();
        let d = drag.snapshot();
        let e = &d.scene.edges[0];
        let now = (
            d.layout.ports[&port_key(e, true)].side,
            d.layout.ports[&port_key(e, false)].side,
        );
        if let Some(pair) = pair {
            assert_eq!(
                now, pair,
                "automatic ports must not switch at the diagonal tie"
            );
        } else {
            pair = Some(now);
        }
    }
    drag.update(&[at("a", 640., 320.)]).unwrap();
    let d = drag.snapshot();
    let e = &d.scene.edges[0];
    assert_eq!(d.layout.ports[&port_key(e, true)].side, model::Side::W);
    assert_eq!(d.layout.ports[&port_key(e, false)].side, model::Side::E);
}

fn recorded_playground() -> Document {
    let html = include_str!("../web/index.html");
    Document::new(
        html.split("spellcheck=\"false\">")
            .nth(1)
            .unwrap()
            .split("</textarea>")
            .next()
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn recorded_client_sweep_keeps_clear_captions_continuous() {
    let doc = recorded_playground();
    let mut drag = doc.begin_drag().unwrap();
    let mut previous = None;
    for step in [4, 8] {
        for y in [120, 124, 128, 132, 136, 140, 160, 200, 240, 280] {
            for x in (0..=240).step_by(step) {
                drag.update(&[at("client", x as f64, y as f64)]).unwrap();
                let d = drag.snapshot();
                let label = d
                    .layout
                    .labels
                    .iter()
                    .find(|l| l.feature == "client->gateway")
                    .unwrap();
                if let Some((oldx, oldy, center)) = previous
                    && oldy == y
                    && oldx + step == x
                {
                    assert!(
                        label.bounds.center().distance(center) < 40.,
                        "caption jumped on a {step}px move at {x},{y}: {center:?} -> {label:?}"
                    );
                }
                previous = Some((x, y, label.bounds.center()));
                drag.settle().unwrap();
            }
        }
    }
    assert_eq!(doc.revision(), 0);
}

#[test]
fn separated_near_box_drag_positions_are_not_rejected_by_stub_envelopes() {
    let doc = recorded_playground();
    let mut drag = doc.begin_drag().unwrap();
    for (x, y) in [(88., 80.), (100., 96.), (120., 96.), (160., 104.)] {
        drag.update(&[at("client", x, y)])
            .unwrap_or_else(|e| panic!("visible clear position {x},{y} rejected: {e}"));
        drag.settle().unwrap();
    }
    let valid = drag.snapshot().to_svg();
    assert!(drag.update(&[at("client", 240., 0.)]).is_err());
    assert_eq!(
        drag.snapshot().to_svg(),
        valid,
        "actual overlaps must retain the valid image"
    );
}

#[test]
fn retained_caption_leader_segments_clear_the_nodes_and_old_json_still_loads() {
    let doc = recorded_playground();
    let mut drag = doc.begin_drag().unwrap();
    let mut orthogonal = false;
    for (x, y) in [
        (0., 120.),
        (168., 120.),
        (176., 120.),
        (240., 120.),
        (100., 96.),
    ] {
        drag.update(&[at("client", x, y)]).unwrap();
        let d = drag.snapshot();
        for label in &d.layout.labels {
            orthogonal |= label.orthogonal_leader;
            for segment in labels::leader_path(label).windows(2) {
                for (id, rect) in &d.layout.nodes {
                    assert!(
                        !rect.blocks(segment[0], segment[1]),
                        "caption leader crosses {id} at {x},{y}"
                    );
                }
            }
        }
    }
    assert!(orthogonal);
    let mut json = serde_json::to_value(&doc.snapshot().layout.labels[0]).unwrap();
    json.as_object_mut().unwrap().remove("orthogonal_leader");
    let restored: labels::LabelPlacement = serde_json::from_value(json).unwrap();
    assert!(!restored.orthogonal_leader);
}
