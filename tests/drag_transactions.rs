use smarter_diagrams::{
    edit::Drag,
    layout::port_key,
    layout::{LayoutOptions, Point},
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
fn release_commits_the_exact_fractional_preview_and_reopens_identically() {
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
    assert_eq!(compile(doc.source()).unwrap().to_svg(), shown.to_svg());
    assert!(doc.undo());
    assert_eq!(doc.snapshot().to_svg(), original.to_svg());
    assert!(!doc.undo());
    assert!(doc.redo());
    assert_eq!(doc.snapshot().to_svg(), shown.to_svg());
}

#[test]
fn routes_do_not_depend_on_pointer_history_or_idle_time() {
    let mut doc = simulation();
    let baseline = doc.snapshot();
    let mut drag = doc.begin_drag().unwrap();
    let target = at("gateway", 305.3, 137.4);
    drag.update(std::slice::from_ref(&target)).unwrap();
    let direct = drag.snapshot().to_svg();
    for p in [(304., 96.), (100., 200.), (700., -100.), (272., 64.)] {
        drag.update(&[at("gateway", p.0, p.1)]).unwrap();
        drag.settle().unwrap();
    }
    drag.update(&[target]).unwrap();
    assert_eq!(drag.snapshot().to_svg(), direct);
    drag.settle().unwrap();
    assert_eq!(drag.snapshot().to_svg(), direct);
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
