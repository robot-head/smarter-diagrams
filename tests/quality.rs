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
fn trust_scope_is_contiguous_across_owners_and_excludes_non_members() {
    let d = compile(TRUST).unwrap();
    valid(&d);
    assert_eq!(d.to_svg().matches("data-zone-label=").count(), 1);
    let contours = render::zone_contours(&d.scene, &d.layout, &d.scene.zones[0]);
    let outer = contours
        .iter()
        .filter(|poly| {
            poly.iter()
                .zip(poly.iter().cycle().skip(1))
                .take(poly.len())
                .map(|(a, b)| a.x * b.y - b.x * a.y)
                .sum::<f64>()
                > 1e-6
        })
        .count();
    assert_eq!(outer, 1);
    for excluded in ["client", "west.worker", "east.worker"] {
        let p = d.layout.nodes[excluded].center();
        let xings = contours
            .iter()
            .flat_map(|poly| (0..poly.len()).map(move |i| (poly[i], poly[(i + 1) % poly.len()])))
            .filter(|(a, b)| {
                (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x
            })
            .count();
        assert_eq!(xings % 2, 0, "{excluded} must lie outside zone contour");
    }
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

fn container_header(node: &smarter_diagrams::model::Node, r: Rect) -> Rect {
    let width = (smarter_diagrams::labels::text_width(&node.label) * 16. / 15. + 8.)
        .min((r.w - 24.).max(0.));
    let h = 24. + (node.label.split('\n').count() - 1) as f64 * 20.;
    Rect::new(
        r.left() + 12. + width / 2.,
        r.top() + 19. + (h - 24.) / 2.,
        width,
        28. + h - 24.,
    )
}

#[test]
fn zones_and_intra_container_captions_clear_headers_and_bridge_same_owner_exclusions() {
    let d = compile(
        "ext: Public Client [shape: actor]\n\
         core: Payment Core {\n\
           api: Checkout API\n\
           fraud: Fraud Engine [shape: diamond]\n\
           vault: Token Vault [shape: database]\n\
           api -> fraud: score\n\
           fraud -> vault: tokenize\n\
         }\n\
         ext -> core.api: HTTPS\n\
         flow checkout \"Tokenized payment\" { ext -> core.api -> core.fraud -> core.vault }\n\
         zone pci \"PCI-DSS cardholder scope\" around [core.api, core.vault] { padding: 1.5g }\n",
    )
    .unwrap();
    valid(&d);
    let contours = render::zone_contours(&d.scene, &d.layout, &d.scene.zones[0]);
    assert_eq!(
        contours.len(),
        1,
        "same-owner zone members should bridge into one contour"
    );
    assert_eq!(d.to_svg().matches("data-zone-label=\"pci\"").count(), 1);
    let core_node = d.scene.node("core").unwrap();
    let header = container_header(core_node, d.layout.nodes["core"]);
    for label in &d.layout.labels {
        assert!(
            !label.bounds.intersects(header),
            "label {} overlapped container header",
            label.feature
        );
    }
}

#[test]
fn hybrid_alignments_and_relative_pins_solve_cleanly_with_unlocked_containers() {
    let aligned = compile(
        "ing: Ingress {\n\
           lb: Edge LB\n\
           waf: WAF [shape: diamond]\n\
           lb -> waf: filter\n\
         }\n\
         ops: Processing {\n\
           s1: Parse\n\
           s2: Enrich\n\
           s3: Validate\n\
           s1 -> s2: step 1\n\
           s2 -> s3: step 2\n\
           align row [s1, s2, s3]\n\
           distribute row [s1, s2, s3]\n\
         }\n\
         ing.waf -> ops.s1: accepted\n",
    )
    .unwrap();
    valid(&aligned);
    assert!((aligned.layout.nodes["ops.s1"].y - aligned.layout.nodes["ops.s2"].y).abs() < 1e-6);
    assert!((aligned.layout.nodes["ops.s2"].y - aligned.layout.nodes["ops.s3"].y).abs() < 1e-6);

    let pinned = compile(
        "anchor_in: Ingress Anchor @pin(grid: 0, 0)\n\
         anchor_mon: Sidecar Monitor @pin(x: anchor_in.x, y: anchor_in.S + 6g) [shape: cache]\n\
         pool: Worker Pool {\n\
           w1: Task Runner A\n\
           w2: Task Runner B\n\
           q: Job Queue [shape: queue]\n\
           q -> w1: pop\n\
           q -> w2: pop\n\
         }\n\
         anchor_in -> pool.q: enqueue\n\
         anchor_mon -> pool.w1: scrape\n",
    )
    .unwrap();
    valid(&pinned);
    let pool_rect = pinned.layout.nodes["pool"];
    let scrape = pinned
        .layout
        .labels
        .iter()
        .find(|l| l.feature == "anchor_mon->pool.w1")
        .unwrap();
    assert!(
        !scrape.bounds.intersects(pool_rect) || pool_rect.inflate(-6.).encloses(scrape.bounds),
        "cross-boundary label must not straddle container border: {:?}",
        scrape.bounds
    );
}

#[test]
fn nested_sql_tables_keep_local_external_titles_clear_of_container_headers() {
    let d = compile(
        "svc: Checkout API\n\
         auth_plane: Identity Store {\n\
           accounts: accounts [shape: sql_table] {\n\
             port id: uuid [side: W, pos: 1]\n\
             port email: text [side: W, pos: 2]\n\
             port org_id: uuid [side: E, pos: 3]\n\
           }\n\
           sessions: active_sessions [shape: sql_table] {\n\
             port token: text [side: W, pos: 1]\n\
             port account_id: uuid [side: E, pos: 2]\n\
           }\n\
         }\n\
         svc -> auth_plane.sessions#token: verify\n\
         auth_plane.sessions#account_id -> auth_plane.accounts#id: FK user\n",
    )
    .unwrap();
    valid(&d);
    let header = container_header(
        d.scene.node("auth_plane").unwrap(),
        d.layout.nodes["auth_plane"],
    );
    for id in ["auth_plane.accounts", "auth_plane.sessions"] {
        let title = d.layout.labels.iter().find(|l| l.feature == id).unwrap();
        let table = d.layout.nodes[id];
        assert!(!title.callout, "sql_table title {id} should stay local");
        assert!(title.bounds.bottom() <= table.top() + 1e-6);
        assert!(!title.bounds.intersects(header));
    }
}

#[test]
fn dense_named_ports_with_satellites_and_shuffled_peers_untangle_planarly() {
    let d = compile(
        "zk: Consensus Coord [shape: database]\n\
         p0: Producer 1\n\
         p1: Producer 2\n\
         p2: Producer 3\n\
         p3: Producer 4\n\
         broker: Partitioned Broker [shape: queue] {\n\
           port in0: ingest 0 [side: W, ratio: 0.2]\n\
           port in1: ingest 1 [side: W, ratio: 0.4]\n\
           port in2: ingest 2 [side: W, ratio: 0.6]\n\
           port in3: ingest 3 [side: W, ratio: 0.8]\n\
           port out0: stream 0 [side: E, ratio: 0.25]\n\
           port out1: stream 1 [side: E, ratio: 0.5]\n\
           port out2: stream 2 [side: E, ratio: 0.75]\n\
           port ctrl: admin [side: N, ratio: 0.5]\n\
           port dlq_p: deadletter [side: S, ratio: 0.5]\n\
         }\n\
         c0: Consumer 1\n\
         c1: Consumer 2\n\
         c2: Consumer 3\n\
         dlq: Dead Letter Store [shape: database]\n\
         zk -> broker#ctrl: lease\n\
         broker#dlq_p -> dlq: poison-pill\n\
         p2 -> broker#in0: b1\n\
         p0 -> broker#in1: b2\n\
         p3 -> broker#in2: b3\n\
         p1 -> broker#in3: b4\n\
         broker#out2 -> c0: s1\n\
         broker#out0 -> c2: s2\n\
         broker#out1 -> c1: s3\n",
    )
    .unwrap();
    valid(&d);
    assert_eq!(crossings(&d), 0);
    let broker = d.layout.nodes["broker"];
    assert!(d.layout.nodes["zk"].bottom() < broker.top());
    assert!(d.layout.nodes["dlq"].top() > broker.bottom());
}

#[test]
fn embedded_fan_inside_cyclic_pipeline_keeps_downstream_sinks_to_the_right() {
    let d = compile(
        "ing: Webhook Ingest\n\
         shard: Sharder [shape: queue]\n\
         w0: Analyzer 1\n\
         w1: Analyzer 2\n\
         w2: Analyzer 3\n\
         w3: Analyzer 4\n\
         agg: Aggregator\n\
         check: Quorum Met? [shape: diamond]\n\
         db: Ledger DB [shape: database]\n\
         alert: PagerDuty Alert\n\
         ing -> shard: payload\n\
         shard -> w0: s1\n\
         shard -> w1: s2\n\
         shard -> w2: s3\n\
         shard -> w3: s4\n\
         w0 -> agg: p1\n\
         w1 -> agg: p2\n\
         w2 -> agg: p3\n\
         w3 -> agg: p4\n\
         agg -> shard: rebalance\n\
         agg -> check: tally\n\
         check -> db: yes\n\
         check -> alert: no\n",
    )
    .unwrap();
    valid(&d);
    let agg_x = d.layout.nodes["agg"].x;
    assert!(d.layout.nodes["check"].x > agg_x);
    assert!(d.layout.nodes["db"].x > d.layout.nodes["check"].x);
    assert!(d.layout.nodes["alert"].x > d.layout.nodes["check"].x);
}

#[test]
fn container_barycenter_and_self_loop_parallel_peers_avoid_spurious_crossings() {
    let bary = compile(
        "fe: Frontend {\n\
           ui_a: Portal\n\
           ui_b: Mobile\n\
           ui_c: Partner\n\
         }\n\
         be: Backend {\n\
           svc_1: Orders\n\
           svc_2: Catalog\n\
           svc_3: Billing\n\
         }\n\
         fe.ui_c -> be.svc_1: create\n\
         fe.ui_a -> be.svc_3: invoice\n\
         fe.ui_b -> be.svc_2: browse\n",
    )
    .unwrap();
    valid(&bary);
    assert_eq!(crossings(&bary), 0);

    let raft = compile(
        "cp: Control Plane {\n\
           leader: Raft Leader\n\
           follower: Raft Follower\n\
           leader -> follower: appendEntries\n\
           leader -> follower: installSnapshot\n\
           follower -> leader: voteGranted\n\
           leader -> leader: leaseRenew\n\
         }\n",
    )
    .unwrap();
    valid(&raft);
    assert_eq!(crossings(&raft), 0);
}

#[test]
fn parallel_raft_containers_route_straight_without_zigzag_or_sprawl() {
    let d = compile(
        "cli: Admin Console [shape: actor]\n\
         cp: Control Plane {\n\
           leader: Raft Leader\n\
           follower: Raft Follower\n\
           store: WAL Store [shape: database]\n\
           leader -> follower: appendEntries\n\
           leader -> follower: installSnapshot\n\
           follower -> leader: voteGranted\n\
           leader -> leader: leaseRenew\n\
           leader -> store: fsync\n\
         }\n\
         dp: Data Plane {\n\
           envoy: Proxy Mesh\n\
           metrics: StatsD [shape: queue]\n\
           envoy <-> metrics: xDS / telemetry\n\
         }\n\
         cli -> cp.leader: apply\n\
         cp.leader -> dp.envoy: push-config\n\
         dp.metrics -> cp.follower: health\n",
    )
    .unwrap();
    valid(&d);
    assert_eq!(crossings(&d), 0);
    let b = render::bounds(&d.scene, &d.layout);
    assert!(
        b.w < 1150. && b.h < 650.,
        "expected compact raft container layout, got {b:?}"
    );
    let bundle: Vec<_> = d
        .scene
        .edges
        .iter()
        .filter(|e| {
            matches!(
                (e.source.node.as_str(), e.target.node.as_str()),
                ("cp.leader", "cp.follower") | ("cp.follower", "cp.leader")
            )
        })
        .collect();
    assert_eq!(bundle.len(), 3);
    for edge in bundle {
        let pts = &d.layout.routes[&edge.id].points;
        assert!(
            pts.len() >= 2 && pts.iter().all(|p| (p.y - pts[0].y).abs() < 1e-6),
            "parallel facing edge {} zigzags: {pts:?}",
            edge.id
        );
    }
}

#[test]
fn cross_boundary_anchors_untangle_entries_and_keep_contiguous_zone_splat() {
    let d = compile(
        "anchor_in: Ingress Anchor @pin(grid: 0, 0)\n\
         anchor_mon: Sidecar Monitor @pin(x: anchor_in.x, y: anchor_in.S + 6g) [shape: cache]\n\
         pool: Worker Pool {\n\
           w1: Task Runner A\n\
           w2: Task Runner B\n\
           q: Job Queue [shape: queue]\n\
           q -> w1: pop\n\
           q -> w2: pop\n\
         }\n\
         store: Persistence {\n\
           db1: Write DB [shape: database]\n\
           db2: Read Replica [shape: database]\n\
           db1 -> db2: wal\n\
         }\n\
         anchor_in -> pool.q: enqueue\n\
         anchor_mon -> pool.w1: scrape\n\
         pool.w1 -> store.db1: commit\n\
         pool.w2 -> store.db1: commit\n\
         zone prod \"Production Cell\" around [pool.w1, pool.w2, store.db1] { padding: 1.5g }\n",
    )
    .unwrap();
    valid(&d);
    assert_eq!(crossings(&d), 0);
    assert_eq!(d.to_svg().matches("data-zone-label=\"prod\"").count(), 1);
    let contours = render::zone_contours(&d.scene, &d.layout, &d.scene.zones[0]);
    let outer = contours
        .iter()
        .filter(|poly| {
            poly.iter()
                .zip(poly.iter().cycle().skip(1))
                .take(poly.len())
                .map(|(a, b)| a.x * b.y - b.x * a.y)
                .sum::<f64>()
                > 1e-6
        })
        .count();
    assert_eq!(outer, 1, "Production Cell should form one contiguous splat");
    for excluded in ["pool.q", "store.db2"] {
        let p = d.layout.nodes[excluded].center();
        let xings = contours
            .iter()
            .flat_map(|poly| (0..poly.len()).map(move |i| (poly[i], poly[(i + 1) % poly.len()])))
            .filter(|(a, b)| {
                (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x
            })
            .count();
        assert_eq!(xings % 2, 0, "{excluded} must lie outside Production Cell");
    }

    let unpinned = compile(
        "anchor_in: Ingress Anchor\n\
         anchor_mon: Sidecar Monitor [shape: cache]\n\
         pool: Worker Pool {\n\
           w1: Task Runner A @pin(grid: 12, 0)\n\
           q: Job Queue @pin(grid: 12, 6) [shape: queue]\n\
         }\n\
         anchor_in -> pool.q: enqueue\n\
         anchor_mon -> pool.w1: scrape\n",
    )
    .unwrap();
    valid(&unpinned);
    assert!(
        unpinned.layout.nodes["anchor_mon"].y < unpinned.layout.nodes["anchor_in"].y,
        "unpinned Sidecar Monitor should order above Ingress Anchor to avoid crossing"
    );
    assert_eq!(crossings(&unpinned), 0);
}
