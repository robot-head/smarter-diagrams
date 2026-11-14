use smarter_diagrams::layout::{GRID, Point};
use smarter_diagrams::{drift::*, edit::*, formal::*, simulation::*, spatial::*, views::*, *};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::process::{Command, Stdio};
#[test]
fn views_focus_story_and_zoom() {
    let p = syntax::parse(
        "a -> b\nb -> c\nx\nview trace { include a -> b -> * }\n@story [a -> b -> c]\n",
    );
    let s = model::lower(&p);
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    assert_eq!(s.views.len(), 1);
    assert_eq!(s.story, vec!["a", "b", "c"]);
    let v = project(&s, "trace").unwrap();
    assert_eq!(v.nodes.len(), 3);
    let cone = causal_cone(&s, "a", 1, Direction::Forward);
    assert_eq!(cone, BTreeSet::from(["a".into(), "b".into()]));
    let tones = attention(&s, Some("a"), 1);
    assert!(tones["x"].opacity < tones["a"].opacity);
    assert_eq!(smoothstep(0.), 0.);
    assert_eq!(smoothstep(1.), 1.);
}
#[test]
fn queue_overload_retry_and_sever() {
    let s = model::lower(&syntax::parse(
        "a @sim(rate: 120, service_rate: 200)\nb @sim(service_rate: 40, queue_cap: 20, retry: exp_backoff(0.1, 0.5), timeout: 0.05)\na -> b\n",
    ));
    let mut sim = Simulation::from_scene(&s).unwrap();
    sim.advance(2.).unwrap();
    assert_eq!(sim.nodes["b"].queued, 20.);
    assert!(sim.nodes["b"].dropped > 0.);
    assert!(sim.nodes["b"].retry_rate > 0.);
    sim.sever("a->b", true).unwrap();
    sim.traits.get_mut("b").unwrap().retry_fraction = 0.;
    sim.advance(3.).unwrap();
    assert!(sim.nodes["b"].queued < 1e-5);
    assert!(sim.set_rate("a", f64::NAN).is_err());
}
#[test]
fn fluid_mass_is_conserved() {
    let s = model::lower(&syntax::parse(
        "a @sim(rate: 150, service_rate: 100, queue_cap: 1000)\n",
    ));
    let mut sim = Simulation::from_scene(&s).unwrap();
    sim.advance(1.).unwrap();
    let q = &sim.nodes["a"];
    assert!((q.queued + q.processed + q.dropped - 150.).abs() < 1e-6);
}
#[test]
fn bounded_sat_finds_exact_trace() {
    let state = |name: &str, a: &str, b: &str| State {
        name: name.into(),
        variables: BTreeMap::from([("a.role".into(), a.into()), ("b.role".into(), b.into())]),
    };
    let system = TransitionSystem {
        states: vec![
            state("none", "Follower", "Follower"),
            state("a leader", "Leader", "Follower"),
            state("split", "Leader", "Leader"),
        ],
        initial: vec![0],
        transitions: vec![
            Transition {
                from: 0,
                to: 1,
                event: "elect a".into(),
                edge: Some("a->b".into()),
            },
            Transition {
                from: 1,
                to: 2,
                event: "drop heartbeat; elect b".into(),
                edge: Some("b->a".into()),
            },
        ],
    };
    let invariant = parse_invariant("count(role == Leader) <= 1").unwrap();
    assert!(matches!(
        check(&system, &invariant, 1).unwrap(),
        CheckResult::SafeThrough { horizon: 1 }
    ));
    let CheckResult::Violation(c) = check(&system, &invariant, 12).unwrap() else {
        panic!()
    };
    assert_eq!(c.horizon, 2);
    assert_eq!(c.states[2].name, "split");
    assert_eq!(c.events.len(), 2);
    assert!(check(&system, &invariant, 13).is_err());
    assert!(satisfy(1, &[vec![1], vec![-1]]).unwrap().is_none());
}
#[test]
fn drift_differential_and_promotion() {
    let source = "a @bind(otel: \"service.name=api\", rust: \"crate::Api\")\nb @bind(otel: \"service.name=db\")\n";
    let s = model::lower(&syntax::parse(source));
    let spans = vec![
        SpanRecord {
            trace_id: "t".into(),
            span_id: "1".into(),
            parent_span_id: None,
            service: "api".into(),
        },
        SpanRecord {
            trace_id: "t".into(),
            span_id: "2".into(),
            parent_span_id: Some("1".into()),
            service: "db".into(),
        },
    ];
    let obs = observed_from_spans(&s, &spans);
    let r = reconcile(&s, &obs, Some(&BTreeSet::new()));
    assert_eq!(r.phantom.len(), 1);
    assert_eq!(r.missing_symbols, vec!["a"]);
    let patch = promote(source, &r.phantom[0]).unwrap();
    let next = edit::apply_patches(source, &[patch]).unwrap();
    let graph = model::lower(&syntax::parse(&next));
    assert!(reconcile(&graph, &obs, None).phantom.is_empty());
}
#[cfg(feature = "code-bindings")]
#[test]
fn tree_sitter_symbols() {
    let s = rust_symbols("mod broker { pub struct Partition; fn send() {} }", "crate").unwrap();
    assert!(s.contains("crate::broker::Partition"));
    assert!(s.contains("crate::broker::send"));
}
#[test]
fn stratigraphic_vias() {
    let s = model::lower(&syntax::parse(
        "strata [app, hardware]\na [stratum: app]\nb [stratum: hardware]\na -> b [relation: runs_on]\n",
    ));
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    let stack = strata(&s, &LayoutOptions::default(), 0.5).unwrap();
    assert_eq!(stack.planes.len(), 2);
    assert_eq!(stack.vias.len(), 1);
    assert_ne!(stack.vias[0].source.y, stack.vias[0].target.y);
}
#[test]
fn spatial_union_locks_infrastructure() {
    let a = compile("a -> b\n").unwrap();
    let b = model::lower(&syntax::parse("a -> b\nb -> c\n"));
    let diff = compare(&a.scene, &b, &a.layout, &LayoutOptions::default()).unwrap();
    assert_eq!(diff.changes["c"], Change::Added);
    assert_eq!(diff.layout.nodes["a"].x, a.layout.nodes["a"].x);
    let f = morph(
        &diff,
        0.,
        &BTreeMap::from([("q".into(), 1.)]),
        &BTreeMap::from([("q".into(), 3.)]),
    )
    .unwrap();
    assert_eq!(f.opacity["c"], 0.);
    assert_eq!(f.metrics["q"], 1.);
    assert!(diff_svg(&diff, 1.).unwrap().contains("#29966f"));
}
#[test]
fn mcp_revision_checked_edits() {
    let mut server = mcp::Server::default();
    let response=server.handle(serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"diagram_open","arguments":{"source":"a\nb\n"}}})).unwrap();
    assert!(response.get("result").is_some(), "{response}");
    let response=server.handle(serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"diagram_edit","arguments":{"revision":0,"operations":[{"PinNode":{"node":"a","pin":{"AbsoluteGrid":{"x":0,"y":0}},"size_lock":null}}]}}})).unwrap();
    assert!(response.get("result").is_some(), "{response}");
    let stale=server.handle(serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"diagram_edit","arguments":{"revision":0,"operations":[]}}})).unwrap();
    assert!(stale.get("error").is_some());
}
#[cfg(feature = "export")]
#[test]
fn vector_and_raster_exports() {
    let svg = compile("a -> b: hello\n").unwrap().to_svg();
    let png = export::png(&svg, 1.).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let pdf = export::pdf(&svg).unwrap();
    assert!(pdf.starts_with(b"%PDF"));

    // Signatures alone do not catch exports silently omitting all typography.
    let text = r#"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="40"><text x="4" y="24" font-family="sans-serif">Hello</text></svg>"#;
    let raster = export::png(text, 1.).unwrap();
    let pixels = resvg::tiny_skia::Pixmap::decode_png(&raster).unwrap();
    assert!(
        pixels.pixels().iter().any(|p| p.alpha() > 0),
        "missing exported glyphs"
    );
}
#[test]
fn zone_invariants_use_only_zone_members() {
    let scene = model::lower(&syntax::parse(
        "a\nb\nx\nzone quorum around [a,b] {\n invariant split_brain_free: count(role == Leader) <= 1\n}\n",
    ));
    assert!(scene.diagnostics.is_empty(), "{:?}", scene.diagnostics);
    let system = TransitionSystem {
        states: vec![State {
            name: "safe quorum".into(),
            variables: BTreeMap::from([
                ("a.role".into(), "Leader".into()),
                ("b.role".into(), "Follower".into()),
                ("x.role".into(), "Leader".into()),
            ]),
        }],
        initial: vec![0],
        transitions: vec![],
    };
    let result = check_zone(&scene, "quorum", &system, 0).unwrap();
    assert!(matches!(
        result[0].result,
        CheckResult::SafeThrough { horizon: 0 }
    ));
}
#[test]
fn phantom_hydration_preserves_pins_and_marks_observed_edges() {
    let source = "a @pin(grid: 0,0)\nb @pin(grid: 16,0)\n";
    let d = compile(source).unwrap();
    let observed = vec![ObservedEdge {
        source: "a".into(),
        target: "b".into(),
        evidence: "runtime".into(),
    }];
    let view = hydrate(source, &observed, &LayoutOptions::default(), &d.layout).unwrap();
    assert_eq!(view.layout.nodes["a"], d.layout.nodes["a"]);
    assert!(view.scene.edges[0].attrs.contains_key("phantom"));
    assert!(enforce_boundary(&view.report).is_err());
}
#[cfg(feature = "verification")]
#[test]
fn verification_reruns_on_source_edits_and_undo() {
    use smarter_diagrams::document::VerificationPolicy;
    let source =
        "a\nb\nzone quorum around [a,b] {\n invariant safe: count(role == Leader) <= 1\n}\n";
    let system = TransitionSystem {
        states: vec![State {
            name: "two leaders".into(),
            variables: BTreeMap::from([
                ("a.role".into(), "Leader".into()),
                ("b.role".into(), "Leader".into()),
            ]),
        }],
        initial: vec![0],
        transitions: vec![],
    };
    let mut d = Document::new(source).unwrap();
    d.verify_continuously(VerificationPolicy {
        zone: "quorum".into(),
        system,
        horizon: 0,
    })
    .unwrap();
    assert!(matches!(
        d.verification_results()[0].result,
        CheckResult::Violation(_)
    ));
    let at = source.find("<= 1").unwrap() + 3;
    d.patch(
        0,
        &[edit::TextPatch::replace(
            source,
            syntax::Span::new(at, at + 1),
            "2",
        )],
    )
    .unwrap();
    assert!(matches!(
        d.verification_results()[0].result,
        CheckResult::SafeThrough { .. }
    ));
    d.undo();
    assert!(matches!(
        d.verification_results()[0].result,
        CheckResult::Violation(_)
    ));
}
#[test]
fn branch_morph_interpolates_changed_geometry() {
    let a = compile("a @pin(grid: 0,0)\nb @pin(grid: 16,0)\na -> b\n").unwrap();
    let b = model::lower(&syntax::parse(
        "a @pin(grid: 0,0)\nb @pin(grid: 20,8)\na -> b\n",
    ));
    let diff = compare(&a.scene, &b, &a.layout, &LayoutOptions::default()).unwrap();
    let mid = morph(&diff, 0.5, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    assert!((mid.nodes["b"].x - 288.).abs() < 1e-6);
    assert!((mid.nodes["b"].y - 64.).abs() < 1e-6);
}
#[test]
fn lenses_intersect_and_fisheye_reprojects_collisions() {
    let d = compile("a -> b\nb -> c\nx\n").unwrap();
    let members = BTreeSet::from(["a".into(), "b".into(), "c".into()]);
    let lens = Lens {
        center: Point::new(0., 0.),
        radius: 2000.,
        strength: 2.,
        kind: LensKind::Members(members.clone()),
    };
    let other = Lens {
        kind: LensKind::Members(BTreeSet::from(["b".into(), "x".into()])),
        ..lens.clone()
    };
    assert_eq!(
        lens_intersection(&d.scene, &d.layout, &[lens.clone(), other]),
        BTreeSet::from(["b".into()])
    );
    let l = fisheye(&d.scene, &d.layout, &[lens], &Default::default()).unwrap();
    layout::validate(&d.scene, &l, &Default::default()).unwrap();
}
#[test]
fn optical_tints_and_trace_playback_frames() {
    assert_eq!(
        render::paint("oklch(1 0 0 / 0.12)"),
        ("#ffffff".into(), 0.12)
    );
    let d = compile("a -> b: first\nb -> c: second\nflow request { a -> b -> c }\n").unwrap();
    let first = render::flow_frame_svg(&d.scene, &d.layout, "request", 0).unwrap();
    assert!(first.contains("stroke-width:4"));
    assert_eq!(first.matches("class=\"flow\"").count(), 1);
    let second = render::flow_frame_svg(&d.scene, &d.layout, "request", 1).unwrap();
    assert_eq!(second.matches("class=\"flow\"").count(), 2);
}

#[test]
fn synthesize_drag_attaches_all_four_container_borders() {
    let source = "box: Cluster {\n  a -> b\n}\nsat: Satellite\n";
    let d = compile(source).unwrap();
    let box_r = d.layout.nodes["box"];
    let sat_r = d.layout.nodes["sat"];
    let hw = sat_r.w * 0.5;
    let hh = sat_r.h * 0.5;
    let gap = 2. * GRID;

    let cases = [
        ("N", Point::new(box_r.x, box_r.top() - hh - gap), "box.N - "),
        (
            "E",
            Point::new(box_r.right() + hw + gap, box_r.y),
            "box.E + ",
        ),
        (
            "S",
            Point::new(box_r.x, box_r.bottom() + hh + gap),
            "box.S + ",
        ),
        (
            "W",
            Point::new(box_r.left() - hw - gap, box_r.y),
            "box.W - ",
        ),
    ];
    for (side, pos, expected_prefix) in cases {
        let ops = synthesize_drag(
            &d.scene,
            &d.layout,
            &[Drag {
                node: "sat".into(),
                position: pos,
            }],
        )
        .unwrap();
        let pin = ops.iter().find_map(|op| match op {
            DiagramEditOp::PinNode {
                node,
                pin: PinExpr::Relative { x, y },
                ..
            } if node == "sat" => Some((x.as_str(), y.as_str())),
            _ => None,
        });
        let (x_expr, y_expr) = pin.unwrap_or_else(|| {
            panic!("expected relative border pin on side {side}, got ops={ops:?}")
        });
        let expr = if matches!(side, "N" | "S") {
            y_expr
        } else {
            x_expr
        };
        assert!(
            expr.starts_with(expected_prefix),
            "side {side}: expected prefix {expected_prefix:?}, got {expr:?}"
        );
        let edited = apply_ops(source, &ops, Placement::Auto).unwrap();
        let recompiled = compile(&edited.source).unwrap();
        layout::validate(
            &recompiled.scene,
            &recompiled.layout,
            &LayoutOptions::default(),
        )
        .unwrap();
    }

    // Wide target node within 6*GRID edge-to-wall still attaches using actual half-width.
    let wide_source = "box: Cluster {\n  a -> b\n}\nsat: Very Wide Satellite Service Name\n";
    let wide_d = compile(wide_source).unwrap();
    let wide_box = wide_d.layout.nodes["box"];
    let wide_sat = wide_d.layout.nodes["sat"];
    assert!(wide_sat.w * 0.5 + 2. * GRID > 6. * GRID);
    let wide_ops = synthesize_drag(
        &wide_d.scene,
        &wide_d.layout,
        &[Drag {
            node: "sat".into(),
            position: Point::new(wide_box.right() + wide_sat.w * 0.5 + 2. * GRID, wide_box.y),
        }],
    )
    .unwrap();
    assert!(wide_ops.iter().any(|op| matches!(
        op,
        DiagramEditOp::PinNode {
            pin: PinExpr::Relative { x, .. },
            ..
        } if x.starts_with("box.E + ")
    )));

    // Far away or lacking orthogonal overlap falls back to AbsoluteGrid.
    for outside in [
        Point::new(box_r.right() + hw + 8. * GRID, box_r.y),
        Point::new(box_r.right() + hw + 2. * GRID, box_r.top() - hh - 2. * GRID),
    ] {
        let ops = synthesize_drag(
            &d.scene,
            &d.layout,
            &[Drag {
                node: "sat".into(),
                position: outside,
            }],
        )
        .unwrap();
        assert!(ops.iter().any(|op| matches!(
            op,
            DiagramEditOp::PinNode {
                pin: PinExpr::AbsoluteGrid { .. },
                ..
            }
        )));
    }

    // Closest border wins when adjacent to multiple containers.
    let two_src =
        "left_box {\n  a @pin(grid: 0, 0)\n}\nright_box {\n  b @pin(grid: 40, 0)\n}\nsat\n";
    let two = compile(two_src).unwrap();
    let lr = two.layout.nodes["left_box"];
    let rr = two.layout.nodes["right_box"];
    let sr = two.layout.nodes["sat"];
    let near_left = synthesize_drag(
        &two.scene,
        &two.layout,
        &[Drag {
            node: "sat".into(),
            position: Point::new(lr.right() + sr.w * 0.5 + GRID, lr.y),
        }],
    )
    .unwrap();
    assert!(
        near_left.iter().any(|op| matches!(
            op,
            DiagramEditOp::PinNode {
                pin: PinExpr::Relative { x, .. },
                ..
            } if x.starts_with("left_box.E + ")
        )),
        "expected left_box.E + ..., got {near_left:?}, lr={lr:?}, rr={rr:?}, sr={sr:?}"
    );
    let near_right = synthesize_drag(
        &two.scene,
        &two.layout,
        &[Drag {
            node: "sat".into(),
            position: Point::new(rr.left() - sr.w * 0.5 - GRID, rr.y),
        }],
    )
    .unwrap();
    assert!(
        near_right.iter().any(|op| matches!(
            op,
            DiagramEditOp::PinNode {
                pin: PinExpr::Relative { x, .. },
                ..
            } if x.starts_with("right_box.W - ")
        )),
        "expected right_box.W - ..., got {near_right:?}, lr={lr:?}, rr={rr:?}, sr={sr:?}"
    );

    // synthesize_drag relaxes distribute row/col along the constrained axis only.
    let dist_row = compile("a -> b -> c\ndistribute row [a, b, c]\n").unwrap();
    let bc = dist_row.layout.nodes["b"].center();
    let keep_row = synthesize_drag(
        &dist_row.scene,
        &dist_row.layout,
        &[Drag {
            node: "b".into(),
            position: Point::new(bc.x, bc.y + 48.),
        }],
    )
    .unwrap();
    assert!(
        !keep_row
            .iter()
            .any(|o| matches!(o, DiagramEditOp::BreakAlignment { .. }))
    );
    let break_row = synthesize_drag(
        &dist_row.scene,
        &dist_row.layout,
        &[Drag {
            node: "b".into(),
            position: Point::new(bc.x + 48., bc.y),
        }],
    )
    .unwrap();
    assert!(break_row.iter().any(|o| matches!(
        o,
        DiagramEditOp::BreakAlignment {
            node,
            axis: model::Axis::Row
        } if node == "b"
    )));
}

#[test]
fn candidate_exploration_toggle_is_deterministic_and_cli_accepts_no_explore_candidates() {
    let source = "u0 -> v0\nu0 -> v1\nu0 -> v2\nu1 -> v0\nu1 -> v1\nu1 -> v2\nu2 -> v0\nu2 -> v1\nu2 -> v2\n";
    let explore_opts = LayoutOptions {
        explore_candidates: true,
        ..LayoutOptions::default()
    };
    let no_explore_opts = LayoutOptions {
        explore_candidates: false,
        ..LayoutOptions::default()
    };

    let e1 = compile_with(source, &explore_opts, None).unwrap();
    let e2 = compile_with(source, &explore_opts, None).unwrap();
    assert_eq!(e1.layout.nodes, e2.layout.nodes);
    assert_eq!(e1.to_svg(), e2.to_svg());
    layout::validate(&e1.scene, &e1.layout, &explore_opts).unwrap();

    let n1 = compile_with(source, &no_explore_opts, None).unwrap();
    let n2 = compile_with(source, &no_explore_opts, None).unwrap();
    assert_eq!(n1.layout.nodes, n2.layout.nodes);
    assert_eq!(n1.to_svg(), n2.to_svg());
    layout::validate(&n1.scene, &n1.layout, &no_explore_opts).unwrap();
    assert_ne!(
        e1.layout.nodes, n1.layout.nodes,
        "K3,3 candidate exploration should choose a distinct layout from canonical LTR"
    );

    let bin = env!("CARGO_BIN_EXE_smarter-diagrams");
    let help = Command::new(bin).arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(
        String::from_utf8_lossy(&help.stdout).contains("--no-explore-candidates"),
        "help should document --no-explore-candidates"
    );

    let mut child = Command::new(bin)
        .args(["-", "--format", "json", "--no-explore-candidates"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cli_layout: Layout = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(cli_layout.nodes, n1.layout.nodes);
    assert_eq!(cli_layout.routes, n1.layout.routes);
}
