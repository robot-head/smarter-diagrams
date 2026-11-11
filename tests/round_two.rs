use smarter_diagrams::{edit::*, layout::*, solver::*, *};

#[test]
fn svg_namespaces_preserve_literal_reference_text_and_accessible_names() {
    let label = "url(#diagram-title) var(--sd-bg,#ffffff)";
    let d = compile(&format!("a: \"{label}\"\n")).unwrap();
    let svg = d.to_svg();
    assert!(svg.contains(&format!("aria-label=\"{label}\"")));
    assert!(svg.contains(&format!(">{label}</text>")));
    assert!(svg.contains("aria-labelledby=\"sd-"));
}

#[test]
fn diagonal_collision_and_distance_have_geometric_meaning() {
    let r = Rect::new(0., 0., 10., 10.);
    assert!(!r.blocks(Point::new(10., 10.), Point::new(20., 20.)));
    assert!(r.blocks(Point::new(-10., -10.), Point::new(10., 10.)));
    assert!(!r.blocks(Point::new(-10., 0.), Point::new(0., -10.)));
    assert_eq!(Point::default().distance(Point::new(3., 4.)), 5.);
    assert_eq!(Point::default().manhattan_distance(Point::new(3., 4.)), 7.);
}
#[test]
fn source_errors_report_unicode_columns_and_missing_endpoints() {
    let source = "α\na ->";
    let error = compile(source).unwrap_err().with_source(source);
    assert!(
        error.contains("line 2:5 — error: edge requires complete endpoints"),
        "{error}"
    );
    assert!(!error.contains("Diagnostic {"));
}
#[test]
fn multiline_labels_have_height_and_scoped_vector_ids() {
    let a = compile("a: \"Hello\\nworld\"\nb\na -> b: \"line one\\nline two\"\n").unwrap();
    assert!(a.layout.nodes["a"].h >= 68.);
    assert!(a.layout.labels[0].bounds.h >= 42.);
    assert_eq!(
        labels::text_width("Hello\nworld"),
        labels::text_width("Hello").max(labels::text_width("world"))
    );
    let svg = a.to_svg();
    assert!(svg.contains("<tspan"));
    let b = compile("x -> y\n").unwrap().to_svg();
    let ids = |s: &str| {
        s.split(" id=\"")
            .skip(1)
            .map(|s| s.split('"').next().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    let ai = ids(&svg);
    let bi = ids(&b);
    assert!(ai.iter().all(|id| !bi.contains(id)));
    assert_eq!(svg, a.to_svg());
}
#[test]
fn named_views_contract_hidden_paths_and_stop_at_visible_nodes() {
    let d=compile("a -> hidden\nhidden -> other\nother -> hidden\nother -> b\nb -> c\nview compact { include a -> b -> c }\n").unwrap();
    let projected = views::project(&d.scene, "compact").unwrap();
    let summary = projected
        .edges
        .iter()
        .find(|e| e.attrs.contains_key("summary"))
        .unwrap();
    assert_eq!((&*summary.source.node, &*summary.target.node), ("a", "b"));
    assert!(summary.label.contains("hidden"));
    assert!(summary.label.contains("other"));
    assert!(
        !projected
            .edges
            .iter()
            .any(|e| e.source.node == "a" && e.target.node == "c")
    );
    layout::layout(&projected, &Default::default(), None).unwrap();
    assert_eq!(d.scene.edges.len(), 5);
}
#[test]
fn dependent_constraints_with_tiny_coefficients_keep_hard_equalities() {
    let rows = [
        Constraint {
            coefficients: vec![(0, 1e-8)],
            rhs: 3e-8,
            equality: true,
        },
        Constraint::fixed(0, 3.),
        Constraint::equality(0, 1),
        Constraint::equality(1, 0),
    ];
    let x = project(&[0., 9.], &[1., 1.], &rows).unwrap();
    assert!((x[0] - 3.).abs() < 1e-7);
    assert!((x[1] - 3.).abs() < 1e-7);
    let bad = [Constraint::fixed(0, 3.), Constraint::fixed(0, 4.)];
    assert!(project(&[0.], &[1.], &bad).is_err());
}
#[test]
fn row_snap_preserves_pin_trivia_and_releases_old_axis() {
    let source = "\t a @pin( grid : 0, 0, lock : [w: 10g, h: 4g] ) # café keep\r\n b @pin(grid: 20,0)\r\n a -> b\r\n";
    let d = compile(source).unwrap();
    let ops = synthesize_drag(
        &d.scene,
        &d.layout,
        &[Drag {
            node: "a".into(),
            position: Point::new(64., 3.),
        }],
    )
    .unwrap();
    let edited = apply_ops(source, &ops, Placement::Auto).unwrap().source;
    assert!(edited.starts_with("\t a @pin("));
    assert!(edited.contains("lock : [w: 10g, h: 4g]"));
    assert!(edited.contains("# café keep\r\n"));
    let result = compile_with(&edited, &Default::default(), Some(&d.layout)).unwrap();
    assert_eq!(result.layout.nodes["a"].x, 64.);
    assert_eq!(result.layout.nodes["a"].y, result.layout.nodes["b"].y);
}
#[test]
fn flow_captions_and_bounded_badges_are_explicit() {
    let d = compile(include_str!("../examples/simulation.diagram")).unwrap();
    assert!(d.layout.labels.iter().any(|l| l.text == "1 · HTTPS"));
    assert_eq!(
        formal::CheckResult::SafeThrough { horizon: 12 }.label(),
        "Safe ≤ 12 steps"
    );
    let svg = d.to_svg();
    assert!(svg.contains(" Q "));
    assert!(!svg.contains("r=\"10\" fill=\"#356bd9\""));
}

#[test]
fn precision_ports_and_multiline_decisions_keep_valid_geometry() {
    for ratio in [0.1234567890123, 0.3333333333333, 0.5000000000001] {
        let d=compile(&format!("a @pin(grid: 0,0)\nb @pin(grid: 20,10)\nobstacle @pin(grid: 10,5)\na.E@{ratio} -> b.W@0.6666666666667\n")).unwrap();
        validate(&d.scene, &d.layout, &Default::default()).unwrap();
    }
    let d =
        compile("decision: \"Approve payment?\\nCheck account first\" [shape: diamond]\n").unwrap();
    assert!(d.layout.nodes["decision"].h >= 126.);
    assert!(d.to_svg().contains("<tspan"));
    let source = include_str!("fixtures/multiline.diagram");
    let d = compile(source).unwrap();
    let container = d.layout.nodes["prod"];
    let child = d.layout.nodes["prod.child"];
    assert!(child.top() > container.top() + 60.);
    assert!(d.to_svg().matches("<tspan").count() >= 12);
    assert!(
        compile(&source.replace("pos: 4", "pos: 2")).is_err(),
        "overlapping multiline record rows need an actionable diagnostic"
    );
}

#[test]
fn spatial_badges_and_drift_counts_stay_semantic_and_unchanged_nodes_stay_fixed() {
    let base = compile("client -> api\napi -> db\n").unwrap();
    let proposal = model::lower(&syntax::parse("client -> api\napi -> worker\n"));
    let diff = spatial::compare(&base.scene, &proposal, &base.layout, &Default::default()).unwrap();
    for i in 0..=10 {
        let frame = spatial::morph(
            &diff,
            i as f64 / 10.,
            &Default::default(),
            &Default::default(),
        )
        .unwrap();
        for id in ["client", "api"] {
            assert_eq!(frame.nodes[id], base.layout.nodes[id]);
        }
    }
    let svg = spatial::diff_svg(&diff, 0.5).unwrap();
    assert!(svg.contains("data-change-badge"));
    assert!(svg.contains("+ Added · − Removed · ~ Modified"));
    let observed = [drift::ObservedEdge {
        source: "client".into(),
        target: "db".into(),
        evidence: "trace-1".into(),
    }];
    let view = drift::hydrate(
        base.parse.source.as_ref(),
        &observed,
        &Default::default(),
        &base.layout,
    )
    .unwrap();
    assert!(view.to_svg().contains("data-drift-badge=\"client\""));
}

#[test]
fn removing_multiline_pins_keeps_comment_tokens() {
    let source = "a @pin(\n  grid: 0,0, # keep coordinates note\n  lock: [w: 10g, h: 4g]\n) # keep declaration note\nb\n";
    let edited = apply_ops(
        source,
        &[DiagramEditOp::UnpinEntities {
            targets: vec!["a".into()],
        }],
        Placement::Auto,
    )
    .unwrap()
    .source;
    assert!(edited.contains("# keep coordinates note"));
    assert!(edited.contains("# keep declaration note"));
    compile(&edited).unwrap();
}
