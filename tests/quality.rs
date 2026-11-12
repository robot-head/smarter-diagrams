use smarter_diagrams::{
    Diagram, Document, compile, edit,
    layout::{self, LayoutOptions, Point, Rect},
    render, routing,
};
const RECORDS: &str = include_str!("fixtures/quality/09-many-record-slots.diagram");
const DUPLEX: &str = include_str!("fixtures/quality/10-many-named-ports.diagram");
const GATHER: &str = include_str!("fixtures/quality/01-dispatch-collect.diagram");
const K24: &str = include_str!("fixtures/quality/02-planar-mesh.diagram");
const K44: &str = include_str!("fixtures/quality/03-dense-mesh.diagram");
const SCOPE: &str = include_str!("fixtures/quality/c0ffee01-scoped.diagram");
const TRUST: &str = include_str!("fixtures/quality/08-cross-ownership.diagram");
const SERIES: &str = include_str!("fixtures/quality/c0ffee01-branch-join.diagram");
fn valid(d: &Diagram) {
    layout::validate(&d.scene, &d.layout, &LayoutOptions::default()).unwrap();
    for e in &d.scene.edges {
        if !e.label.is_empty() {
            assert!(
                d.layout.labels.iter().any(|l| l.feature == e.id),
                "missing {}: {}",
                e.id,
                e.label
            );
        }
    }
    for n in &d.scene.nodes {
        for p in n.ports.values() {
            if n.shape == "sql_table"
                && matches!(
                    p.constraint,
                    smarter_diagrams::model::PortConstraint::FixedPos { .. }
                )
            {
                continue;
            }
            let uses: Vec<_> = d
                .scene
                .edges
                .iter()
                .filter(|e| {
                    e.source.node == n.id && e.source.port.as_ref() == Some(&p.id)
                        || e.target.node == n.id && e.target.port.as_ref() == Some(&p.id)
                })
                .collect();
            if !uses.is_empty() && uses.iter().all(|e| e.label.trim() == p.label.trim()) {
                continue;
            }
            assert!(
                d.layout
                    .labels
                    .iter()
                    .any(|l| l.feature == format!("{}#{}", n.id, p.id)),
                "missing port caption {}#{}",
                n.id,
                p.id
            );
        }
    }
}
#[test]
fn port_captions_preserve_unquoted_words_and_numbers() {
    let d = compile("broker: Event log [shape: queue] {\n port unquoted: input 1 [side: W, ratio: 0.2]\n port quoted: \"input 2\" [side: W, ratio: 0.5]\n port words: Customer account [side: W, ratio: 0.8]\n}\n").unwrap();
    valid(&d);
    for (id, text) in [
        ("unquoted", "unquoted: input 1"),
        ("quoted", "quoted: input 2"),
        ("words", "words: Customer account"),
    ] {
        assert_eq!(d.scene.node("broker").unwrap().ports[id].label, text);
        assert!(
            d.layout
                .labels
                .iter()
                .any(|l| l.feature == format!("broker#{id}") && l.text == text)
        );
    }
}
fn crossings(d: &Diagram) -> usize {
    let routes: Vec<_> = d.layout.routes.values().collect();
    let mut count = 0;
    for (i, a) in routes.iter().enumerate() {
        for b in &routes[i + 1..] {
            for p in a.points.windows(2) {
                for q in b.points.windows(2) {
                    count += usize::from(routing::proper_crossing(p[0], p[1], q[0], q[1]));
                    assert!(
                        routing::collinear_overlap(p[0], p[1], q[0], q[1]) < 1e-6,
                        "coincident {} / {}",
                        a.edge,
                        b.edge
                    );
                }
            }
        }
    }
    count
}
#[test]
fn caption_search_never_returns_a_partial_assignment() {
    for source in [
        RECORDS,
        DUPLEX,
        include_str!("fixtures/quality/18-cross-scope.diagram"),
        include_str!("fixtures/quality/c0ffee77-parallel.diagram"),
        include_str!("fixtures/quality/a11ce002-parallel.diagram"),
        include_str!("fixtures/quality/a11ce002-sparse-mesh.diagram"),
        include_str!("fixtures/quality/5eed1203-sparse-mesh.diagram"),
    ] {
        let d = compile(source).unwrap();
        valid(&d);
        for l in &d.layout.labels {
            if l.callout {
                assert!(
                    l.bounds.center().distance(l.anchor) <= 160.01,
                    "detached {}",
                    l.feature
                );
            }
        }
    }
}
#[test]
fn ownership_feedback_is_feasible_and_retains_hard_pins() {
    for source in [
        SCOPE,
        include_str!("fixtures/quality/control-scoped-no-captions.diagram"),
        include_str!("fixtures/quality/control-scoped-pinned.diagram"),
    ] {
        let d = compile(source).unwrap();
        valid(&d);
        assert!(!d.layout.nodes["left"].intersects(d.layout.nodes["right"]));
        if source.contains("lock:") {
            assert_eq!(
                d.layout.nodes["left.worker"].center(),
                Point::new(24. * 16., 0.)
            );
            assert_eq!(
                d.layout.nodes["right.worker"].center(),
                Point::new(124. * 16., 0.)
            );
        } else {
            let b = render::bounds(&d.scene, &d.layout);
            assert_eq!(crossings(&d), 0);
            assert!(
                b.w / b.h < 2.5,
                "scoped feedback must not form an unreadable strip: {b:?}"
            );
        }
    }
}
#[test]
fn bipartite_routes_achieve_small_graph_crossing_minima() {
    for (source, expected) in [(K24, 0), (K44, 4)] {
        let d = compile(source).unwrap();
        valid(&d);
        assert_eq!(crossings(&d), expected);
        assert!(d.layout.labels.iter().all(|l| !l.callout));
    }
}
#[test]
fn composed_fan_keeps_all_workers_in_one_column() {
    let d = compile(GATHER).unwrap();
    valid(&d);
    for i in 0..5 {
        let worker = d.layout.nodes[&format!("w{i}")];
        assert!((worker.x - d.layout.nodes["w0"].x).abs() < 1e-6);
        if i > 0 {
            assert!(worker.y > d.layout.nodes[&format!("w{}", i - 1)].y);
        }
    }
    assert_eq!(crossings(&d), 0);
}
#[test]
fn multi_source_record_slots_preserve_rows_without_crossings() {
    let d = compile(RECORDS).unwrap();
    valid(&d);
    assert_eq!(crossings(&d), 0);
    assert!(d.layout.labels.iter().all(|l| !l.callout));
    let table = d.layout.nodes["orders"];
    for (row, id) in [
        "id",
        "customer_id",
        "status",
        "created_at",
        "total",
        "audit_id",
    ]
    .iter()
    .enumerate()
    {
        let p = &d.layout.ports[&format!("orders#{id}")];
        assert!((p.point.y - table.top() - (row + 1) as f64 * 16.).abs() < 1e-6);
    }
}
#[test]
fn branch_join_regions_fold_as_whole_stages() {
    for source in [
        SERIES,
        include_str!("fixtures/quality/a11ce002-branch-join.diagram"),
        include_str!("fixtures/quality/5eed1203-branch-join.diagram"),
    ] {
        let d = compile(source).unwrap();
        valid(&d);
        assert_eq!(crossings(&d), 0);
        let b = render::bounds(&d.scene, &d.layout);
        let fitted = 15. * (730. / b.w).min(450. / b.h).min(1.);
        assert!(fitted >= 8.4, "fitted type {fitted}, bounds {b:?}");
        assert!(d.layout.nodes["d2"].y > d.layout.nodes["d0"].y + 100.);
    }
}
#[test]
fn named_duplex_ports_keep_leaf_proportions_and_local_captions() {
    let d = compile(DUPLEX).unwrap();
    valid(&d);
    let widths: Vec<_> = (0..4).map(|i| d.layout.nodes[&format!("p{i}")].w).collect();
    assert!(
        widths.iter().copied().fold(0., f64::max)
            / widths.iter().copied().fold(f64::INFINITY, f64::min)
            < 1.15
    );
    assert_eq!(crossings(&d), 0);
    assert!(d.layout.labels.iter().all(|l| !l.callout));
}
#[test]
fn trust_scope_is_contiguous_per_owner_and_identified_on_each_island() {
    let d = compile(TRUST).unwrap();
    valid(&d);
    assert_eq!(d.to_svg().matches("data-zone-label=").count(), 3);
    assert_eq!(
        render::zone_contours(&d.scene, &d.layout, &d.scene.zones[0]).len(),
        3
    );
}
#[test]
fn independent_systems_pack_before_the_standalone_observer() {
    let d = compile(include_str!(
        "fixtures/quality/12-independent-systems.diagram"
    ))
    .unwrap();
    valid(&d);
    let bounds = |prefix: &str| {
        d.layout
            .nodes
            .iter()
            .filter(|(id, _)| id.starts_with(prefix))
            .map(|(_, r)| *r)
            .reduce(Rect::union)
            .unwrap()
    };
    assert!(bounds("a").bottom() < bounds("b").top());
    assert!(bounds("b").bottom() < d.layout.nodes["observer"].top());
}
#[test]
fn new_presentations_preserve_other_nodes_and_reopen_small_drags() {
    for source in [K24, K44, GATHER, DUPLEX, SERIES, SCOPE, TRUST, RECORDS] {
        let mut doc = Document::new(source).unwrap();
        let before = doc.snapshot();
        let node = before
            .scene
            .nodes
            .iter()
            .find(|n| !n.container)
            .unwrap()
            .id
            .clone();
        let p = before.layout.nodes[&node].center();
        let mut drag = doc.begin_drag().unwrap();
        for dx in [0.1, -0.1, 1., 4.] {
            drag.update(&[edit::Drag {
                node: node.clone(),
                position: Point::new(p.x + dx, p.y + 4.),
            }])
            .unwrap();
            valid(&drag.snapshot());
        }
        doc.commit_drag(&drag).unwrap();
        let reopened = compile(doc.source()).unwrap();
        valid(&reopened);
        assert!((reopened.layout.nodes[&node].x - p.x - 4.).abs() < 0.01);
        assert!((reopened.layout.nodes[&node].y - p.y - 4.).abs() < 0.01);
        for (id, old) in &before.layout.nodes {
            if id != &node && !before.scene.node(id).unwrap().container {
                let r = reopened.layout.nodes[id];
                assert!(
                    (r.x - old.x).abs() < 0.01 && (r.y - old.y).abs() < 0.01,
                    "moved neighbor {id}"
                );
                assert!(
                    (r.w - old.w).abs() < 0.01 && (r.h - old.h).abs() < 0.01,
                    "resized neighbor {id}: {old:?} -> {r:?}; source {}",
                    source.lines().next().unwrap()
                );
            }
        }
    }
}

#[test]
fn unconstrained_bypasses_never_merge_into_a_shared_shaft() {
    for source in [
        include_str!("fixtures/quality/5eed1203-dag.diagram"),
        include_str!("fixtures/quality/abc12345-dag.diagram"),
    ] {
        let d = compile(source).unwrap();
        valid(&d);
        let _ = crossings(&d);
    }
}
#[test]
fn parallel_transformations_keep_their_captions_near_the_same_track() {
    let d = compile(include_str!("fixtures/quality/c0ffee77-parallel.diagram")).unwrap();
    valid(&d);
    assert!((d.layout.nodes["n8"].y - d.layout.nodes["n9"].y).abs() < 0.01);
    let labels: Vec<_> = d
        .layout
        .labels
        .iter()
        .filter(|l| l.feature.starts_with("n8->n9"))
        .collect();
    assert_eq!(labels.len(), 3);
    assert!(
        labels
            .iter()
            .all(|l| l.bounds.center().distance(l.anchor) <= 50.)
    );
}

#[test]
fn long_caption_short_chains_use_a_readable_forward_track() {
    let d = compile(include_str!("fixtures/quality/14-long-labels.diagram")).unwrap();
    valid(&d);
    let b = render::bounds(&d.scene, &d.layout);
    assert!(15. * (730. / b.w).min(450. / b.h).min(1.) >= 12.5);
    for edge in &d.scene.edges {
        assert_eq!(
            d.layout.ports[&layout::port_key(edge, true)].side,
            smarter_diagrams::model::Side::S
        );
        assert_eq!(
            d.layout.ports[&layout::port_key(edge, false)].side,
            smarter_diagrams::model::Side::N
        );
        let points = &d.layout.routes[&edge.id].points;
        assert!(points.iter().all(|p| (p.x - points[0].x).abs() < 1e-6));
    }
}

#[test]
fn free_record_attachment_keeps_clearance_from_unused_fixed_slot() {
    let base = compile("client -> db\ndb: Ledger [shape: database]\n").unwrap();
    let proposal = compile(
        "client -> db\ndb: Accounts [shape: sql_table] { port id: uuid [side: W, pos: 1] }\n",
    )
    .unwrap();
    valid(&proposal);
    let free = &proposal.layout.ports["client->db:target"];
    let fixed = &proposal.layout.ports["db#id"];
    assert!(free.point.distance(fixed.point) >= LayoutOptions::default().port_spacing - 1e-6);
    for (a, b) in [(&base, &proposal), (&proposal, &base)] {
        let diff = smarter_diagrams::spatial::compare(
            &a.scene,
            &b.scene,
            &a.layout,
            &LayoutOptions::default(),
        )
        .unwrap();
        for tau in [0., 0.25, 0.5, 0.75, 1.] {
            smarter_diagrams::spatial::diff_svg(&diff, tau).unwrap();
        }
    }
}

#[test]
fn adjacent_bypass_uses_the_clear_interior_channel() {
    let d = compile(include_str!("fixtures/quality/local-bypass.diagram")).unwrap();
    valid(&d);
    let edge = d.scene.edge("n1->n4").unwrap();
    let source = &d.layout.ports[&layout::port_key(edge, true)];
    let target = &d.layout.ports[&layout::port_key(edge, false)];
    assert_eq!(source.side, smarter_diagrams::model::Side::S);
    assert_eq!(target.side, smarter_diagrams::model::Side::N);
    assert!(
        d.layout.routes[&edge.id]
            .points
            .iter()
            .all(|p| (p.x - source.point.x).abs() < 1e-6)
    );
    assert_eq!(crossings(&d), 0);
}
