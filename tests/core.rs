use smarter_diagrams::{
    edit::*,
    layout::*,
    model::*,
    syntax::{self, Span},
    *,
};
#[test]
fn lossless_unicode_crlf_and_recovery() {
    for source in [
        "# café 😀\r\n  α.E -> β.W: λ   # comment\r\n",
        "a {\n b ->\n",
        "a [shape: \"rect\"] @pin(grid: 2, 3)\n",
        "a -> b; b <-> c; c -- d; d ==> a",
    ] {
        let p = syntax::parse(source);
        assert_eq!(p.syntax().text().to_string(), source);
    }
}
#[test]
fn incremental_reparse_preserves_unaffected_green() {
    let source = "a: One\nb: Two\n";
    let p = syntax::parse(source);
    let changed = p.reparse(Span::new(3, 6), "First").unwrap();
    assert_eq!(&*changed.source, "a: First\nb: Two\n");
    assert_eq!(changed.syntax().text().to_string(), changed.source.as_ref());
    assert_eq!(
        p.syntax().children().nth(1).unwrap().green(),
        changed.syntax().children().nth(1).unwrap().green()
    );
}
#[test]
fn scoped_nodes_ports_and_overlays() {
    let source = r#"
prod: VPC {
  api.E -> db#id: query
  db: SQL [shape: sql_table] {
    port id: uuid [side: W, pos: 1]
  }
}
kms: Key manager
zone pci "PCI" around [prod.api, prod.db, kms] {
  padding: 1.5g
  tint: oklch(0.72 0.14 35 / 0.12)
}
align row [prod.api, kms]
flow path { prod.api -> prod.db#id }
@layout(wide) { kms @pin(grid: 40, 4) }
"#;
    let p = syntax::parse(source);
    assert!(p.errors.is_empty(), "{:?}", p.errors);
    let s = model::lower(&p);
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    assert_eq!(s.node("prod.api").unwrap().parent.as_deref(), Some("prod"));
    assert_eq!(s.zones[0].members.len(), 3);
    assert!(s.node("kms").unwrap().parent.is_none());
    assert!(matches!(
        s.edges[0].target.constraint,
        PortConstraint::FixedPos {
            offset_grid: 1.,
            ..
        }
    ));
    assert_eq!(s.flows[0].edges.len(), 1);
    assert_eq!(s.layout_rules[0].variant.as_deref(), Some("wide"));
}
#[test]
fn all_port_tiers() {
    let p = syntax::parse(
        "a -> b\na.E -> b.W\na.S(order: 2) -> b.N@0.25\na#row -> b\na { port row [side: E, pos: 2] }\n",
    );
    let s = model::lower(&p);
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    assert!(matches!(s.edges[0].source.constraint, PortConstraint::Free));
    assert!(matches!(
        s.edges[1].source.constraint,
        PortConstraint::FixedSide { .. }
    ));
    assert!(matches!(
        s.edges[2].source.constraint,
        PortConstraint::FixedOrder { order: 2, .. }
    ));
    assert!(matches!(
        s.edges[2].target.constraint,
        PortConstraint::FixedRatio { ratio: 0.25, .. }
    ));
    assert!(matches!(
        s.edges[3].source.constraint,
        PortConstraint::FixedPos { .. }
    ));
}
#[test]
fn simple_pipeline_is_deterministic() {
    let source = "client -> gateway: HTTPS\ngateway -> auth: JWT\nauth -> db: SQL\n";
    let a = compile(source).unwrap();
    let b = compile(source).unwrap();
    assert_eq!(a.to_svg(), b.to_svg());
    assert_eq!(a.layout.labels.len(), 3);
    layout::validate(&a.scene, &a.layout, &LayoutOptions::default()).unwrap();
}
#[test]
fn ports_routes_loops_cycles_and_collision_free_labels() {
    for source in [
        "a.E -> b.W: request\nb.S -> c.N: response\nc.W -> a.S: retry\n",
        "a.E -> a.N: loop\n",
        "a.E -> b.W@0.25: first\na.S(order: 1) -> c.N: second\na.S(order: 2) -> d.N: third\n",
        "a { port id [side: E, pos: 2] }\na#id -> b.W: key\n",
    ] {
        let d = compile(source).unwrap_or_else(|e| panic!("{source}\n{e}"));
        for e in &d.scene.edges {
            let r = &d.layout.routes[&e.id];
            assert_eq!(r.points[0], d.layout.ports[&port_key(e, true)].point);
            assert_eq!(
                *r.points.last().unwrap(),
                d.layout.ports[&port_key(e, false)].point
            );
        }
        for (i, a) in d.layout.labels.iter().enumerate() {
            for b in &d.layout.labels[i + 1..] {
                assert!(!a.bounds.intersects(b.bounds));
            }
        }
    }
}
#[test]
fn hard_pins_and_relations() {
    let d = compile("a @pin(grid: 0, 0)\nb @pin(grid: 16, 0)\na.E -> b.W: call\nalign row [a,b]\n")
        .unwrap();
    assert_eq!(d.layout.nodes["a"].x, 0.);
    assert_eq!(d.layout.nodes["b"].x, 256.);
    assert_eq!(d.layout.nodes["a"].y, d.layout.nodes["b"].y);
    assert!(compile("a @pin(grid: 0,0)\nb @pin(grid: 0,0)\n").is_err());
}
#[test]
fn nested_containment() {
    let d = compile("cluster {\n inner {\n  a -> b: RPC\n }\n c\n}\nexternal\n").unwrap();
    assert!(d.layout.nodes["cluster"].encloses(d.layout.nodes["cluster.inner"]));
    assert!(d.layout.nodes["cluster.inner"].encloses(d.layout.nodes["cluster.inner.a"]));
}
#[test]
fn pin_patches_preserve_every_unrelated_byte() {
    let s = "# title\r\n  a: Café  @pin(grid: 1, 2)    # 😀 note\r\nb\r\n";
    let r = apply_ops(
        s,
        &[DiagramEditOp::PinNode {
            node: "a".into(),
            pin: PinExpr::AbsoluteGrid { x: 3, y: 4 },
            size_lock: None,
        }],
        Placement::Inline,
    )
    .unwrap();
    assert_eq!(r.source, s.replace("1, 2", "3, 4"));
    let s = "a: Café    # note\nb\n";
    let r = apply_ops(
        s,
        &[DiagramEditOp::PinNode {
            node: "a".into(),
            pin: PinExpr::AbsoluteGrid { x: 2, y: 3 },
            size_lock: None,
        }],
        Placement::Inline,
    )
    .unwrap();
    assert_eq!(r.source, "a: Café @pin(grid: 2, 3)    # note\nb\n");
}
#[test]
fn multiselect_and_existing_layout_upsert() {
    let s = "a\nb\nc\n";
    let ops: Vec<_> = ["a", "b", "c"]
        .into_iter()
        .enumerate()
        .map(|(i, n)| DiagramEditOp::PinNode {
            node: n.into(),
            pin: PinExpr::AbsoluteGrid {
                x: i as i32 * 15,
                y: 0,
            },
            size_lock: None,
        })
        .collect();
    let r = apply_ops(s, &ops, Placement::Auto).unwrap();
    assert!(r.source.starts_with(s));
    assert_eq!(r.source.matches("@layout").count(), 1);
    let scene = model::lower(&syntax::parse(&r.source));
    assert_eq!(scene.layout_rules.len(), 3);
    assert!(scene.diagnostics.is_empty(), "{:?}", scene.diagnostics);
}
#[test]
fn endpoint_port_lock_is_surgical() {
    let s = "  a.S    -> b:   call      # preserve\n";
    let r = apply_ops(
        s,
        &[DiagramEditOp::LockPort {
            node: "a".into(),
            port: "a->b:source".into(),
            spec: PortConstraint::FixedOrder {
                side: Side::S,
                order: 1,
            },
        }],
        Placement::Auto,
    )
    .unwrap();
    assert_eq!(r.source, s.replace("a.S", "a.S(order: 1)"));
}
#[test]
fn breakaway_and_undo_are_atomic() {
    let source = "a @pin(grid: 0, 0)\nb @pin(grid: 16, 0)\nalign row [a,b]\n";
    let mut d = Document::new(source).unwrap();
    let snap = d.snapshot();
    let ops = vec![
        DiagramEditOp::BreakAlignment {
            node: "b".into(),
            axis: Axis::Row,
        },
        DiagramEditOp::PinNode {
            node: "b".into(),
            pin: PinExpr::AbsoluteGrid { x: 16, y: 10 },
            size_lock: None,
        },
    ];
    d.edit(0, &ops, Placement::Auto).unwrap();
    assert!(!d.source().contains("align"));
    assert_eq!(snap.parse.source.as_ref(), source);
    assert!(d.undo());
    assert_eq!(d.source(), source);
    assert!(d.redo());
    assert!(d.source().contains("16, 10"));
    assert!(d.edit(0, &[], Placement::Auto).is_err());
}
#[test]
fn invalid_and_stale_patches_are_rejected() {
    let s = "😀abc";
    assert!(apply_patches(s, &[TextPatch::insert(1, "x")]).is_err());
    let mut p = TextPatch::replace(s, Span::new(4, 5), "b");
    p.expected_text = Some("z".into());
    assert!(apply_patches(s, &[p]).is_err());
    assert!(apply_patches(s, &[TextPatch::insert(4, "x"), TextPatch::insert(4, "y")]).is_err());
}
#[test]
fn projection_and_forward_autodiff() {
    use solver::*;
    let x = project(&[0., 0.], &[1., 1.], &[Constraint::separation(0, 1, 10.)]).unwrap();
    assert!((x[0] + 5.).abs() < 1e-6);
    assert!((x[1] - 5.).abs() < 1e-6);
    assert!(
        project(
            &[0.],
            &[1.],
            &[Constraint::fixed(0, 1.), Constraint::fixed(0, 2.)]
        )
        .is_err()
    );
    let z = lbfgs(&[5.], &[(-10., 10.)], 20, |x| x[0].square());
    assert!(z[0].abs() < 1e-6);
    let a = Dual::variable(3., 0, 1).square();
    assert_eq!(a.gradient, vec![6.]);
    assert!(Dual::variable(1000., 0, 1).softplus(2.).value.is_finite());
}
#[test]
fn escaped_svg() {
    let d = compile("a: \"<script>& hi\"\na -> b: \"<bad>\"\n").unwrap();
    let s = d.to_svg();
    assert!(!s.contains("<script>"));
    assert!(s.contains("&lt;script&gt;&amp; hi"));
    assert!(s.contains("role=\"img\""));
}
#[test]
fn incremental_stability() {
    let a = compile("a -> b\nb -> c\n").unwrap();
    let b = compile_with(
        "a -> b\nb -> c\nc -> d\n",
        &LayoutOptions::default(),
        Some(&a.layout),
    )
    .unwrap();
    assert_eq!(b.layout.stability.order_flips, 0);
    assert!(b.layout.stability.squared_drift < 10000.);
}

#[test]
fn malformed_input_never_panics_and_roundtrips() {
    let alphabet = [
        'a', 'b', ' ', ':', '.', '#', '@', '(', ')', '[', ']', '{', '}', '-', '>', ';', '\n', '"',
        'é', '😀', '0', ',',
    ];
    let mut seed = 1u64;
    for length in 0..70 {
        for _ in 0..30 {
            let mut source = String::new();
            for _ in 0..length {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                source.push(alphabet[(seed >> 32) as usize % alphabet.len()]);
            }
            let p = syntax::parse(&source);
            assert_eq!(p.syntax().text().to_string(), source);
            let _ = model::lower(&p);
        }
    }
    // A fresh parse is the oracle for incremental token boundaries, especially
    // comments/quotes next to semicolons and nested closing delimiters.
    for source in [
        "a: Short; b: Next\na -> b\n",
        "group {\n a: Short\n a -> b\n}\nx -> y\n",
        "α: \"quoted λ\"; β\r\nα -> β\r\n",
    ] {
        let parsed = syntax::parse(source);
        for position in source.char_indices().map(|(i, _)| i).chain([source.len()]) {
            for inserted in [" ", "#", "x", "\"", "\\", ";", "[", "}", "\n"] {
                let reparsed = parsed
                    .reparse(Span::new(position, position), inserted)
                    .unwrap();
                let mut edited = source.to_owned();
                edited.insert_str(position, inserted);
                let full = syntax::parse(&edited);
                assert_eq!(
                    format!("{:#?}", reparsed.syntax()),
                    format!("{:#?}", full.syntax()),
                    "{edited:?}"
                );
                assert_eq!(reparsed.errors, full.errors);
            }
        }
    }
}
#[test]
fn typed_cst_endpoint_and_key_value_views() {
    use syntax::ast::{AstNode, Endpoint, KeyVal, PortAnchor};
    let p = syntax::parse("a.S(order: 2) -> b.W@0.25: hello @label(slide: 0.5)\n");
    assert_eq!(
        p.syntax().descendants().filter_map(Endpoint::cast).count(),
        2
    );
    assert_eq!(
        p.syntax()
            .descendants()
            .filter_map(PortAnchor::cast)
            .count(),
        2
    );
    assert_eq!(p.syntax().descendants().filter_map(KeyVal::cast).count(), 2);
}
#[test]
fn switching_pin_forms_preserves_other_attribute_trivia() {
    let source = "a @pin( grid: 1, 2,   lock : [w: 10g, h: 6g] ) # keep\nb\n";
    let r = apply_ops(
        source,
        &[DiagramEditOp::PinNode {
            node: "a".into(),
            pin: PinExpr::Relative {
                x: "b.E + 3g".into(),
                y: "b.y".into(),
            },
            size_lock: None,
        }],
        Placement::Auto,
    )
    .unwrap();
    assert!(r.source.contains("lock : [w: 10g, h: 6g]"));
    assert!(r.source.contains("# keep"));
    assert!(!r.source.contains("grid:"));
    let model = model::lower(&syntax::parse(&r.source));
    assert!(model.diagnostics.is_empty(), "{:?}", model.diagnostics);
}
#[test]
fn relative_pins_remain_equalities_after_compaction() {
    let d = compile("a: Container {\n child\n}\nb @pin(x: a.E + 4g, y: a.child.y)\nc\n").unwrap();
    assert!((d.layout.nodes["b"].x - d.layout.nodes["a"].right() - 64.).abs() < 1e-6);
    assert!((d.layout.nodes["b"].y - d.layout.nodes["a.child"].y).abs() < 1e-6);
}
#[test]
fn port_order_is_clockwise_on_all_sides() {
    for side in [Side::N, Side::E, Side::S, Side::W] {
        let source =
            format!("a.{side:?}(order: 1) -> b\na.{side:?}(order: 2) -> c\na.{side:?} -> d\n");
        let d = compile(&source).unwrap();
        let a = &d.layout.ports["a->b:source"];
        let b = &d.layout.ports["a->c:source"];
        let rank = |p: Point| match side {
            Side::N => p.x,
            Side::E => p.y,
            Side::S => -p.x,
            Side::W => -p.y,
        };
        assert!(rank(a.point) < rank(b.point));
    }
}
#[test]
fn cache_is_sensitive_to_layout_options() {
    let a = compile("a -> b\n").unwrap();
    let options = LayoutOptions {
        stub: 32.,
        ..Default::default()
    };
    let b = compile_with("a -> b\n", &options, Some(&a.layout)).unwrap();
    let p = b.layout.ports.values().next().unwrap();
    assert_eq!(p.point.distance(p.stub), 32.);
}
#[test]
fn cached_equal_size_labels_refresh_text() {
    let a = compile("a: Start\na -> b: first\n").unwrap();
    let b = compile_with(
        "a: Other\na -> b: tiffs\n",
        &LayoutOptions::default(),
        Some(&a.layout),
    )
    .unwrap();
    assert_eq!(b.layout.reused_routes, 1);
    assert!(b.to_svg().contains("Other"));
    assert!(b.to_svg().contains("tiffs"));
    assert!(!b.to_svg().contains("first"));
}
#[test]
fn channel_vpsc_separates_shared_tracks() {
    let scene = model::lower(&syntax::parse("a.E -> c.W\nb.E -> d.W\n"));
    let mut l = Layout::default();
    for (id, x, y) in [
        ("a", -200., -100.),
        ("b", -200., 100.),
        ("c", 200., -100.),
        ("d", 200., 100.),
    ] {
        l.nodes.insert(id.into(), Rect::new(x, y, 112., 48.));
    }
    for (edge, y) in [(&scene.edges[0], -100.), (&scene.edges[1], 100.)] {
        let a = PortPlacement {
            point: Point::new(-144., y),
            stub: Point::new(-128., y),
            side: Side::E,
            key: port_key(edge, true),
        };
        let b = PortPlacement {
            point: Point::new(144., y),
            stub: Point::new(128., y),
            side: Side::W,
            key: port_key(edge, false),
        };
        l.ports.insert(a.key.clone(), a.clone());
        l.ports.insert(b.key.clone(), b.clone());
        l.routes.insert(
            edge.id.clone(),
            Route {
                edge: edge.id.clone(),
                points: vec![
                    a.point,
                    a.stub,
                    Point::new(-100., y),
                    Point::new(-100., 0.),
                    Point::new(100., 0.),
                    Point::new(100., y),
                    b.stub,
                    b.point,
                ],
            },
        );
    }
    routing::nudge_channels(&scene, &LayoutOptions::default(), &mut l);
    let a = l.routes["a->c"].points[3].y;
    let b = l.routes["b->d"].points[3].y;
    assert!((a + b).abs() < 1e-6);
    assert!((a - b).abs() >= 8. - 1e-6);
    layout::validate(&scene, &l, &LayoutOptions::default()).unwrap();
}
#[test]
fn semantic_gesture_promotion_and_relaxation() {
    let d = compile("a @pin(grid: 0,0)\nb @pin(grid: 16,0)\na -> b\nalign row [a,b]\n").unwrap();
    let ops = synthesize_drag(
        &d.scene,
        &d.layout,
        &[Drag {
            node: "b".into(),
            position: Point::new(320., 3.),
        }],
    )
    .unwrap();
    assert!(ops.iter().any(|o| matches!(
        o,
        DiagramEditOp::SnapAlign {
            axis: Axis::Row,
            ..
        }
    )));
    let moved = apply_ops(&d.parse.source, &ops, Placement::Auto).unwrap();
    let moved = compile_with(&moved.source, &LayoutOptions::default(), Some(&d.layout)).unwrap();
    assert_eq!(moved.layout.nodes["b"].x, 320.);
    assert_eq!(moved.layout.nodes["b"].y, moved.layout.nodes["a"].y);
    assert_eq!(moved.layout.nodes["a"], d.layout.nodes["a"]);
    let ops = synthesize_drag(
        &d.scene,
        &d.layout,
        &[Drag {
            node: "b".into(),
            position: Point::new(256., 64.),
        }],
    )
    .unwrap();
    assert!(matches!(ops[0], DiagramEditOp::BreakAlignment { .. }));
    assert!(ops.iter().any(|o| matches!(
        o,
        DiagramEditOp::PinNode {
            pin: PinExpr::AbsoluteGrid { x: 16, y: 4 },
            ..
        }
    )));
}
#[test]
fn source_injection_is_rejected_by_typed_coordinate_edits() {
    let result = apply_ops(
        "a\nb\n",
        &[DiagramEditOp::PinNode {
            node: "a".into(),
            pin: PinExpr::Relative {
                x: "b.E)\nc @pin(x: 1g".into(),
                y: "b.y".into(),
            },
            size_lock: None,
        }],
        Placement::Auto,
    );
    assert!(result.is_err());
}
#[test]
fn local_container_query_reuses_unrelated_geometry() {
    let source = "cluster {\n a -> b -> c\n b: Short\n}\nx -> y\n";
    let a = compile(source).unwrap();
    let b = compile_with(
        &source.replace("Short", "A longer service"),
        &LayoutOptions::default(),
        Some(&a.layout),
    )
    .unwrap();
    assert_eq!(a.layout.nodes["x"], b.layout.nodes["x"]);
    assert_eq!(a.layout.nodes["y"], b.layout.nodes["y"]);
    assert_eq!(a.layout.routes["x->y"], b.layout.routes["x->y"]);
    layout::validate(&b.scene, &b.layout, &LayoutOptions::default()).unwrap();
}
#[test]
fn absolute_child_pin_expands_unlocked_container() {
    let d = compile("cluster {\n child @pin(grid: 40, 40)\n}\n").unwrap();
    assert_eq!(
        d.layout.nodes["cluster.child"].center(),
        Point::new(640., 640.)
    );
    assert!(d.layout.nodes["cluster"].encloses(d.layout.nodes["cluster.child"]));
}
#[test]
fn malformed_reserved_statements_report_errors_without_panics() {
    for source in [
        "@layout { @pin(grid: 1, 2) }",
        "@layout { (a ->) @pin(grid: 1,2) }",
        "zone pci around [] {}",
        "flow f { a -> }",
        "a [",
        "a @pin(",
        "a: \"unterminated\n",
    ] {
        let p = syntax::parse(source);
        assert_eq!(p.syntax().text().to_string(), source);
        let _ = model::lower(&p);
    }
}
#[test]
fn scoped_relative_pin_and_anchored_layout_edge() {
    let source = "prod {\n a @pin(grid: 0,0)\n b @pin(x: a.E + 6g, y: a.y)\n a.E -> b.W@0.5: RPC\n}\n@layout { (prod.a.E -> prod.b.W@0.5) @label(slot: above, slide: 0.5) }\n";
    let d = compile(source).unwrap();
    assert_eq!(d.scene.layout_rules[0].target, "prod.a->prod.b");
    assert!((d.layout.nodes["prod.b"].x - d.layout.nodes["prod.a"].right() - 96.).abs() < 1e-6);
}
#[test]
fn rigid_translation_is_discounted_from_mental_map() {
    let a = compile("a @pin(grid: 0,0)\na -> b -> c\n").unwrap();
    let b = compile_with(
        "a @pin(grid: 4,3)\nb @pin(grid: 19,3)\nc @pin(grid: 34,3)\na -> b -> c\n",
        &LayoutOptions::default(),
        Some(&a.layout),
    )
    .unwrap();
    for id in ["a", "b", "c"] {
        assert!((b.layout.nodes[id].x - a.layout.nodes[id].x - 64.).abs() < 0.01);
        assert!((b.layout.nodes[id].y - a.layout.nodes[id].y - 48.).abs() < 0.01);
    }
    assert!(b.layout.stability.squared_drift < 0.01);
}
#[test]
fn elastic_order_can_relax_when_pins_require_it() {
    let x = solver::project_stable(
        &[0., 10.],
        &[1., 1.],
        &[
            solver::Constraint::fixed(0, 20.),
            solver::Constraint::fixed(1, 0.),
        ],
        &[Some(0.), Some(10.)],
        450.,
    )
    .unwrap();
    assert!((x[0] - 20.).abs() < 1e-4);
    assert!(x[1].abs() < 1e-4);
}
#[test]
fn bounded_cyclic_shape_search_is_feasible_and_deterministic() {
    let source = "a -> b\nb -> c\nc -> d\nd -> a\n";
    let d = compile(source).unwrap();
    assert!(d.layout.shape_search_states > 0);
    assert!(d.layout.shape_search_states <= LayoutOptions::default().shape_budget);
    let again = compile(source).unwrap();
    assert_eq!(d.to_svg(), again.to_svg());
    layout::validate(&d.scene, &d.layout, &Default::default()).unwrap();
    let clauses = topology::hard_clauses(3);
    let mut impossible = clauses.clone();
    impossible.push(vec![1]);
    impossible.push(vec![4]);
    assert!(formal::satisfy(9, &impossible).unwrap().is_none());
}
#[test]
fn cyclic_shape_objective_never_worsens_incumbent() {
    let scene = model::lower(&syntax::parse("a -> b\nb -> c\nc -> d\nd -> a\n"));
    let mut rects: Vec<_> = (0..4)
        .map(|i| Rect::new(i as f64 * 240., 0., 112., 48.))
        .collect();
    let stats = topology::optimize(&scene, &mut rects, &Default::default());
    assert!(stats.after <= stats.before);
    assert_eq!(stats.optimized_components, 1);
    assert!(rects.iter().any(|r| r.y != 0.));
}
#[test]
fn transitive_active_constraints_can_be_released() {
    let x = solver::project(
        &[0., 0., 0.],
        &[1., 1., 1.],
        &[
            solver::Constraint::separation(0, 2, 15.),
            solver::Constraint::separation(0, 1, 10.),
            solver::Constraint::separation(1, 2, 10.),
        ],
    )
    .unwrap();
    assert!((x[0] + 10.).abs() < 1e-5);
    assert!(x[1].abs() < 1e-5);
    assert!((x[2] - 10.).abs() < 1e-5);
}
#[test]
fn dense_cyclic_graphs_remain_geometrically_valid() {
    for n in 3..=8 {
        let mut source = String::new();
        for i in 0..n {
            source.push_str(&format!("n{i} -> n{}\n", (i + 1) % n));
            if n > 3 {
                source.push_str(&format!("n{i} -> n{}\n", (i + 2) % n));
            }
        }
        let options = LayoutOptions {
            shape_budget: 256,
            ..Default::default()
        };
        let d = compile_with(&source, &options, None).unwrap_or_else(|e| panic!("{n} nodes: {e}"));
        layout::validate(&d.scene, &d.layout, &options).unwrap();
    }
}
#[test]
fn coffman_graham_bounds_dag_layer_width() {
    let mut source = String::new();
    for i in 0..20 {
        source.push_str(&format!("n{i} -> hub\n"));
    }
    let scene = model::lower(&syntax::parse(&source));
    let ranks = topology::coffman_graham(&scene, 4);
    let mut counts = std::collections::BTreeMap::new();
    for r in &ranks {
        *counts.entry(*r).or_insert(0) += 1;
    }
    assert!(counts.values().all(|count| *count <= 4));
    for e in &scene.edges {
        assert!(
            ranks[scene.index(&e.source.node).unwrap()]
                < ranks[scene.index(&e.target.node).unwrap()]
        );
    }
}
#[test]
fn invalid_numeric_ports_are_diagnosed() {
    for source in [
        "a { port p [side: Q] }\n",
        "a.S(order: -1) -> b\n",
        "a.S(order: 1.5) -> b\n",
        "a { port p [side: N, ratio: bogus] }\n",
    ] {
        let scene = model::lower(&syntax::parse(source));
        assert!(
            scene
                .diagnostics
                .iter()
                .any(|d| d.severity == Severity::Error),
            "{source}"
        );
    }
}
#[test]
fn virtual_named_ports_are_inferred_and_promoted_for_edits() {
    let source = "prod {\n broker.E#orders -> worker.W\n}\n";
    let scene = model::lower(&syntax::parse(source));
    assert!(scene.diagnostics.is_empty(), "{:?}", scene.diagnostics);
    assert!(!scene.node("prod.broker").unwrap().ports["orders"].explicit);
    let next = apply_ops(
        source,
        &[DiagramEditOp::LockPort {
            node: "prod.broker".into(),
            port: "orders".into(),
            spec: PortConstraint::FixedRatio {
                side: Side::E,
                ratio: 0.5,
            },
        }],
        Placement::Auto,
    )
    .unwrap();
    let d = compile(&next.source).unwrap();
    assert_eq!(
        d.scene.node("prod.broker").unwrap().parent.as_deref(),
        Some("prod")
    );
    assert!(d.scene.node("prod.broker").unwrap().ports["orders"].explicit);
}
#[test]
fn bare_cardinal_identifiers_are_nodes() {
    let d = compile("N -> E\nE -> S\nS -> W\n").unwrap();
    for id in ["N", "E", "S", "W"] {
        assert!(d.scene.node(id).is_some());
    }
}
#[test]
fn alignment_extension_retains_trailing_comma_syntax() {
    let source = "a\nb\nc\nalign row [a, b, ]\n";
    let next = apply_ops(
        source,
        &[DiagramEditOp::SnapAlign {
            axis: Axis::Row,
            members: vec!["a".into(), "c".into()],
        }],
        Placement::Auto,
    )
    .unwrap();
    assert!(!next.source.contains(",,"));
    let scene = model::lower(&syntax::parse(&next.source));
    assert_eq!(scene.relations[0].members, vec!["a", "b", "c"]);
}
#[test]
fn named_port_tiers_merge_and_can_be_unlocked() {
    let source = "broker.E#orders -> worker.W\n";
    let next = apply_ops(
        source,
        &[DiagramEditOp::LockPort {
            node: "broker".into(),
            port: "orders".into(),
            spec: PortConstraint::FixedRatio {
                side: Side::E,
                ratio: 0.5,
            },
        }],
        Placement::Auto,
    )
    .unwrap();
    let d = compile(&next.source).unwrap();
    assert!(matches!(
        d.scene.edges[0].source.constraint,
        PortConstraint::FixedRatio { ratio: 0.5, .. }
    ));
    let free = apply_ops(
        &next.source,
        &[DiagramEditOp::LockPort {
            node: "broker".into(),
            port: "orders".into(),
            spec: PortConstraint::Free,
        }],
        Placement::Auto,
    )
    .unwrap();
    let d = compile(&free.source).unwrap();
    assert!(matches!(
        d.scene.edges[0].source.constraint,
        PortConstraint::Free
    ));
    assert!(!d.scene.node("broker").unwrap().container);
}
#[test]
fn dragging_one_node_keeps_unrelated_world_positions() {
    let a = compile("a -> b -> c\n").unwrap();
    let b = compile_with(
        "b @pin(grid: 15,6)\na -> b -> c\n",
        &Default::default(),
        Some(&a.layout),
    )
    .unwrap();
    assert!((b.layout.nodes["b"].y - 96.).abs() < 1e-5);
    for id in ["a", "c"] {
        assert!((b.layout.nodes[id].y - a.layout.nodes[id].y).abs() < 0.01);
    }
}

#[test]
fn complete_architecture_fixture_has_readable_local_callouts() {
    let diagram = compile(include_str!("../examples/architecture.diagram")).unwrap();
    assert_eq!(diagram.scene.nodes.len(), 8);
    assert_eq!(diagram.scene.edges.len(), 6);
    assert!(
        diagram.layout.labels.iter().all(|label| {
            !label.callout || label.bounds.center().distance(label.anchor) <= 200.
        })
    );
    assert!(diagram.to_svg().contains("Request path step 3"));
}

#[test]
fn incremental_leaf_lowering_matches_fresh_origins_and_structural_fallback() {
    let source = "cluster {\r\n a: Short\r\n a -> b: RPC\r\n b -> db#id\r\n db [shape: sql_table] {\r\n port id: uuid [side: W, pos: 1]\r\n }\r\n}\r\nzone z around [cluster.a, cluster.db] { padding: 1.5g }\r\nalign row [cluster.a, cluster.b]\r\nflow f { cluster.a -> cluster.b -> cluster.db }\r\nview v { include cluster.a -> * }\r\n@layout(alternate) { cluster.b @pin(grid: 40,0) }\r\n";
    let mut document = Document::new(source).unwrap();
    for (old, new) in [
        ("Short", "Longer λ service"),
        ("Longer λ service", "Tiny"),
        ("Tiny", "Named [shape: cache]"),
        ("Named", "Renamed"),
    ] {
        let start = document.source().find(old).unwrap();
        document
            .patch(
                document.revision(),
                &[TextPatch {
                    delete_range: Span::new(start, start + old.len()),
                    insert_text: new.into(),
                    expected_text: Some(old.into()),
                }],
            )
            .unwrap();
        let full = compile(document.source()).unwrap();
        assert_eq!(
            serde_json::to_value(&document.snapshot().scene).unwrap(),
            serde_json::to_value(&full.scene).unwrap()
        );
    }
    // A comment introduced at a semicolon boundary can hide the next explicit
    // declaration. It must take the full parse/lower path.
    let mut document = Document::new("a: Short; b: Explicit\na -> b\n").unwrap();
    document
        .patch(
            0,
            &[TextPatch {
                delete_range: Span::new(3, 8),
                insert_text: "Long # comment".into(),
                expected_text: Some("Short".into()),
            }],
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(&document.snapshot().scene).unwrap(),
        serde_json::to_value(&compile(document.source()).unwrap().scene).unwrap()
    );
    let mut document = Document::new("a: label#p; b: Explicit\na -> b\n").unwrap();
    document
        .patch(
            0,
            &[TextPatch::replace(document.source(), Span::new(8, 8), " ")],
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(&document.snapshot().scene).unwrap(),
        serde_json::to_value(&compile(document.source()).unwrap().scene).unwrap()
    );
}

#[test]
fn fresh_fans_keep_siblings_and_captions_in_readable_columns() {
    for fan_out in [true, false] {
        for leaf_count in 3..=8 {
            let mut source = "hub: Event log [shape: queue]\n".to_owned();
            for i in 0..leaf_count {
                source.push_str(&format!("leaf{i}: Consumer {i}\n"));
                source.push_str(&if fan_out {
                    format!("hub -> leaf{i}: event\n")
                } else {
                    format!("leaf{i} -> hub: upload\n")
                });
            }
            let diagram = compile(&source).unwrap();
            let first = diagram.layout.nodes["leaf0"];
            for i in 0..leaf_count {
                let leaf = diagram.layout.nodes[&format!("leaf{i}")];
                assert!((leaf.x - first.x).abs() < 0.01);
                let id = if fan_out {
                    format!("hub->leaf{i}")
                } else {
                    format!("leaf{i}->hub")
                };
                let caption = diagram
                    .layout
                    .labels
                    .iter()
                    .find(|label| label.feature == id)
                    .unwrap();
                assert!(!caption.callout);
                assert!(
                    (caption.anchor.y - leaf.y).abs() < 0.01,
                    "caption must stay on its leaf branch"
                );
            }
            let routes: Vec<_> = diagram.layout.routes.values().collect();
            for (i, a) in routes.iter().enumerate() {
                for b in &routes[i + 1..] {
                    for p in a.points.windows(2) {
                        for q in b.points.windows(2) {
                            assert!(
                                !routing::proper_crossing(p[0], p[1], q[0], q[1]),
                                "a fan must not cross its branches"
                            );
                            assert!(
                                routing::collinear_overlap(p[0], p[1], q[0], q[1]) < 1e-6,
                                "fan branches must remain distinct"
                            );
                        }
                    }
                }
            }
            assert_eq!(diagram.to_svg(), compile(&source).unwrap().to_svg());
            layout::validate(&diagram.scene, &diagram.layout, &LayoutOptions::default()).unwrap();
        }
    }
}

#[test]
fn long_unconstrained_pipeline_folds_but_explicit_rows_and_drags_stay_put() {
    let source = "s0 -> s1 -> s2 -> s3 -> s4 -> s5 -> s6 -> s7: data\n";
    let diagram = compile(source).unwrap();
    let bounds = render::bounds(&diagram.scene, &diagram.layout);
    assert!(
        bounds.w / bounds.h < 2.5,
        "long chain should fit a readable frame"
    );
    assert_eq!(diagram.layout.routes.len(), 7);
    let source = format!("{source}align row [s0,s1,s2,s3,s4,s5,s6,s7]\n");
    let row = compile(&source).unwrap();
    let y = row.layout.nodes["s0"].y;
    assert!(row.layout.nodes.values().all(|r| (r.y - y).abs() < 0.01));
    let doc = Document::new(&source).unwrap();
    let before = doc.snapshot();
    let session = doc.begin_drag().unwrap();
    assert_eq!(before.to_svg(), session.snapshot().to_svg());
}

#[test]
fn parallel_protocols_have_straight_separated_lanes_and_local_captions() {
    let source = "client: Client\nserver: Server\nclient -> server: HTTPS\nclient -> server: WebSocket\nclient -> server: telemetry\n";
    let diagram = compile(source).unwrap();
    for edge in &diagram.scene.edges {
        let points = &diagram.layout.routes[&edge.id].points;
        assert!(
            points.iter().all(|p| (p.y - points[0].y).abs() < 0.01),
            "parallel lanes must not exchange order"
        );
        let label = diagram
            .layout
            .labels
            .iter()
            .find(|l| l.feature == edge.id)
            .unwrap();
        assert!(
            !label.callout,
            "each protocol must label its own lane locally"
        );
        assert!((label.anchor.y - points[0].y).abs() < 0.01);
    }
    layout::validate(&diagram.scene, &diagram.layout, &LayoutOptions::default()).unwrap();
    let mut doc = Document::new(source).unwrap();
    let mut drag = doc.begin_drag().unwrap();
    let before = drag.snapshot();
    let center = before.layout.nodes["client"].center();
    drag.update(&[edit::Drag {
        node: "client".into(),
        position: Point::new(center.x, center.y + 4.),
    }])
    .unwrap();
    assert_eq!(
        drag.snapshot().layout.nodes["client"].h,
        before.layout.nodes["client"].h,
        "dragging must keep caption lane clearance"
    );
    doc.commit_drag(&drag).unwrap();
    let reopened = compile(doc.source()).unwrap();
    assert_eq!(
        reopened.layout.nodes["client"].h, before.layout.nodes["client"].h,
        "saved pins must preserve caption clearance"
    );
}

#[test]
fn retry_returns_and_planar_fallbacks_use_clear_outer_rails() {
    let retry=compile("api -> queue: enqueue\nqueue -> worker: consume\nworker -> db: persist\nworker -> queue: retry\n").unwrap();
    let points = &retry.layout.routes["worker->queue"].points;
    let top = retry
        .layout
        .nodes
        .values()
        .map(|r| r.top())
        .fold(f64::INFINITY, f64::min);
    assert!(
        points.iter().any(|p| p.y < top - 40.),
        "retry must form a visible outer loop"
    );
    let fallback=compile("a: Frontend A\nb: Frontend B\nc: Backend C\nd: Backend D\na -> c: primary\na -> d: fallback\nb -> c: fallback\nb -> d: primary\n").unwrap();
    let routes: Vec<_> = fallback.layout.routes.values().collect();
    for (i, a) in routes.iter().enumerate() {
        for b in &routes[i + 1..] {
            for p in a.points.windows(2) {
                for q in b.points.windows(2) {
                    assert!(!routing::proper_crossing(p[0], p[1], q[0], q[1]));
                    assert!(routing::collinear_overlap(p[0], p[1], q[0], q[1]) < 1e-6);
                }
            }
        }
    }
}

#[test]
fn record_slot_order_and_local_captions_follow_the_actual_rows() {
    let d=compile("api: API @pin(grid: 0,0)\ndb: Orders [shape: sql_table] @pin(grid: 24,0) {\n port id: uuid [side: W,pos: 1]\n port customer_id: uuid [side: W,pos: 2]\n}\napi.E -> db#id: insert\napi.E -> db#customer_id: lookup\n").unwrap();
    let insert = &d.scene.edges[0];
    let lookup = &d.scene.edges[1];
    let a = &d.layout.ports[&layout::port_key(insert, true)];
    let b = &d.layout.ports[&layout::port_key(lookup, true)];
    assert!(
        a.point.y < b.point.y,
        "source order must match record row order"
    );
    for edge in &d.scene.edges {
        let label = d
            .layout
            .labels
            .iter()
            .find(|l| l.feature == edge.id)
            .unwrap();
        assert!(!label.callout);
        assert!(
            (label.anchor.y - d.layout.ports[&layout::port_key(edge, true)].point.y).abs() < 0.01
        );
    }
    for p in d.layout.routes[&insert.id].points.windows(2) {
        for q in d.layout.routes[&lookup.id].points.windows(2) {
            assert!(!routing::proper_crossing(p[0], p[1], q[0], q[1]));
        }
    }
}

#[test]
fn wrapping_keeps_source_text_but_uses_taller_readable_shapes() {
    let source = "client: Customer account management console\napi: Regional authorization and policy enforcement service\nclient -> api: Authenticate using a short-lived access token\n";
    let d = compile(source).unwrap();
    assert!(d.layout.nodes["api"].w < 300. && d.layout.nodes["api"].h >= 68.);
    assert!(d.layout.labels[0].text.contains('\n'));
    assert!(!d.scene.nodes[1].label.contains('\n'));
    assert!(d.to_svg().contains("<tspan"));
    assert_eq!(Document::new(source).unwrap().source(), source);
    let caption = "Authenticate requests using a short-lived organization access token";
    let source = format!("hub -> a: {caption}\nhub -> b: {caption}\nhub -> c: {caption}\n");
    let fan = compile(&source).unwrap();
    assert!(
        render::bounds(&fan.scene, &fan.layout).w < 1000.,
        "spacing must use the wrapped caption width"
    );
    assert!(fan.layout.labels.iter().all(|l| l.text.contains('\n')));
    let scope = compile(
        "client -> api\napi -> db\napi -> kms\nzone secure around [api,db,kms] { padding: 1.5g }\n",
    )
    .unwrap();
    let rings = render::zone_contours(&scope.scene, &scope.layout, &scope.scene.zones[0]);
    assert_eq!(
        rings.len(),
        1,
        "root trust scope should have one contiguous enclosure"
    );
    let p = scope.layout.nodes["client"].center();
    let crossings = rings[0]
        .iter()
        .zip(rings[0].iter().cycle().skip(1))
        .filter(|(a, b)| {
            (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x
        })
        .count();
    assert_eq!(
        crossings % 2,
        0,
        "Browser must stay outside the trusted scope"
    );
}

#[test]
fn unconstrained_caption_failure_recovers_by_reserving_more_space() {
    let source = include_str!("fixtures/label-reflow.diagram");
    let d = compile(source).unwrap();
    assert_eq!(
        d.layout.labels.len(),
        d.scene.edges.iter().filter(|e| !e.label.is_empty()).count()
    );
    layout::validate(&d.scene, &d.layout, &LayoutOptions::default()).unwrap();
    assert_eq!(d.to_svg(), compile(source).unwrap().to_svg());
}
