use layout::{LayoutOptions, Point};
use smarter_diagrams::{Document, compile, edit, layout, render, routing};

fn crossings(diagram: &smarter_diagrams::Diagram) -> usize {
    let routes: Vec<_> = diagram.layout.routes.values().collect();
    let mut crossings = 0;
    for (i, a) in routes.iter().enumerate() {
        for b in &routes[i + 1..] {
            for p in a.points.windows(2) {
                for q in b.points.windows(2) {
                    crossings += usize::from(routing::proper_crossing(p[0], p[1], q[0], q[1]));
                    assert!(routing::collinear_overlap(p[0], p[1], q[0], q[1]) < 1e-6);
                }
            }
        }
    }
    crossings
}

fn mesh_source() -> String {
    let mut source = "a: Service A\nb: Service B\nc: Service C\nx: Store X [shape: database]\ny: Store Y [shape: database]\nz: Store Z [shape: database]\n".to_owned();
    for a in ["a", "b", "c"] {
        for b in ["x", "y", "z"] {
            source.push_str(&format!("{a} -> {b}: access\n"));
        }
    }
    source
}

#[test]
fn nonplanar_mesh_uses_only_one_crossing_and_keeps_it_after_a_drag() {
    let source = mesh_source();
    let mut doc = Document::new(&source).unwrap();
    let before = doc.snapshot();
    assert_eq!(
        crossings(&before),
        1,
        "K3,3 needs one crossing, not a bundle"
    );
    assert_eq!(before.layout.labels.len(), 9);
    assert!(
        before
            .layout
            .nodes
            .values()
            .all(|r| r.w < 200. && r.h < 140.)
    );
    let body = before.layout.nodes["c"];
    let mut drag = doc.begin_drag().unwrap();
    drag.update(&[edit::Drag {
        node: "c".into(),
        position: Point::new(body.x, body.y + 4.),
    }])
    .unwrap();
    doc.commit_drag(&drag).unwrap();
    for d in [
        drag.snapshot(),
        std::sync::Arc::new(compile(doc.source()).unwrap()),
    ] {
        assert_eq!(crossings(&d), 1);
        for (id, old) in &before.layout.nodes {
            let now = d.layout.nodes[id];
            assert!((now.x - old.x).abs() < 0.01);
            assert!((now.y - old.y - if id == "c" { 4. } else { 0. }).abs() < 0.01);
            assert!(
                (now.w - old.w).abs() < 0.01 && (now.h - old.h).abs() < 0.01,
                "{id}: {old:?} -> {now:?}"
            );
        }
        layout::validate(&d.scene, &d.layout, &LayoutOptions::default()).unwrap();
    }
}

#[test]
fn trees_keep_depths_subtrees_and_captions_in_order() {
    let source = "root -> left: choose left\nroot -> right: choose right\nleft -> a: first\nleft -> b: second\nright -> c: third\nright -> d: fourth\na -> e: next\n";
    let d = compile(source).unwrap();
    let nodes = &d.layout.nodes;
    assert!((nodes["left"].y - nodes["right"].y).abs() < 0.01);
    for pair in [
        ["root", "left"],
        ["root", "right"],
        ["left", "a"],
        ["left", "b"],
        ["right", "c"],
        ["right", "d"],
        ["a", "e"],
    ] {
        assert!(nodes[pair[0]].bottom() < nodes[pair[1]].top());
    }
    assert!(
        nodes["a"].x < nodes["b"].x && nodes["b"].x < nodes["c"].x && nodes["c"].x < nodes["d"].x
    );
    assert_eq!(crossings(&d), 0);
    assert!(d.layout.labels.iter().all(|l| !l.callout));
    let explicit = compile(&format!("{source}align row [root,left,right,a,b,c,d,e]\n")).unwrap();
    let y = explicit.layout.nodes["root"].y;
    assert!(
        explicit
            .layout
            .nodes
            .values()
            .all(|r| (r.y - y).abs() < 0.01),
        "explicit layout wins over presentation"
    );
}

#[test]
fn main_chains_with_shortcuts_and_parallel_sends_fit_a_compact_frame() {
    for extra in [
        "s0 -> s6: fast\ns1 -> s7: shortcut\n",
        "s0 -> s7: fast\ns0 -> s7: retry\ns6 -> s7: again\n",
    ] {
        let source = format!("s0 -> s1 -> s2 -> s3 -> s4 -> s5 -> s6 -> s7: data\n{extra}");
        let d = compile(&source).unwrap();
        let bounds = render::bounds(&d.scene, &d.layout);
        assert!(
            bounds.w < 1200. && bounds.h < 850.,
            "compact frame: {bounds:?}"
        );
        assert!(
            d.layout
                .nodes
                .values()
                .any(|r| r.y > d.layout.nodes["s0"].y + 20.)
        );
        assert_eq!(d.to_svg(), compile(&source).unwrap().to_svg());
        layout::validate(&d.scene, &d.layout, &LayoutOptions::default()).unwrap();
    }
}

#[test]
fn redundant_port_caption_is_suppressed_without_removing_port_identity() {
    let source = "producer: Publisher @pin(grid: 0,0)\nbroker: Event log [shape: queue] @pin(grid: 24,0) {\n port input [side: W,ratio: 0.25]\n port ack [side: W,ratio: 0.75]\n}\nproducer.E -> broker#input: publish\nbroker#ack -> producer.E: ack\n";
    let d = compile(source).unwrap();
    assert!(!d.layout.labels.iter().any(|l| l.feature == "broker#ack"));
    assert!(d.layout.labels.iter().any(|l| l.feature == "broker#input"));
    assert!(d.layout.ports.contains_key("broker#ack"));
    assert!(d.to_svg().contains("data-port=\"broker#ack\""));
    assert!(render::description(&d.scene).contains("ack"));
    let distinct = compile(&source.replace("producer.E: ack", "producer.E: receipt")).unwrap();
    assert!(
        distinct
            .layout
            .labels
            .iter()
            .any(|l| l.feature == "broker#ack")
    );
}

#[test]
fn compound_presentations_keep_body_sizes_and_other_nodes_fixed_after_reopening() {
    for source in [
        "root -> a: first\nroot -> b: second\nroot -> c: third\na -> d: next\n",
        "s0 -> s1 -> s2 -> s3 -> s4 -> s5 -> s6 -> s7: data\ns0 -> s7: shortcut\ns0 -> s6: skip\ns0 -> s5: jump\n",
    ] {
        let mut doc = Document::new(source).unwrap();
        let before = doc.snapshot();
        let node = &before.scene.nodes[0].id;
        let body = before.layout.nodes[node];
        let mut drag = doc.begin_drag().unwrap();
        drag.update(&[edit::Drag {
            node: node.clone(),
            position: Point::new(body.x, body.y + 4.),
        }])
        .unwrap();
        doc.commit_drag(&drag).unwrap();
        for d in [
            drag.snapshot(),
            std::sync::Arc::new(compile(doc.source()).unwrap()),
        ] {
            for (id, old) in &before.layout.nodes {
                let now = d.layout.nodes[id];
                assert!(
                    (now.x - old.x).abs() < 0.01
                        && (now.y - old.y - if id == node { 4. } else { 0. }).abs() < 0.01
                );
                assert!(
                    (now.w - old.w).abs() < 0.01 && (now.h - old.h).abs() < 0.01,
                    "{id}: {old:?} -> {now:?}"
                );
            }
            layout::validate(&d.scene, &d.layout, &LayoutOptions::default()).unwrap();
        }
    }
}

#[test]
fn trust_title_stays_at_its_boundary_instead_of_above_an_unrelated_client() {
    let d=compile("client: Browser\napi: API\ndb: Ledger\nkms: Key service\nclient -> api: HTTPS\napi -> db: SQL\napi -> kms: key\nzone secure \"Trusted boundary\" around [api,db,kms] { padding: 1.5g }\n").unwrap();
    let top = render::zone_contours(&d.scene, &d.layout, &d.scene.zones[0])
        .into_iter()
        .flatten()
        .map(|p| p.y)
        .fold(f64::INFINITY, f64::min);
    let svg = d.to_svg();
    let surface = svg.split("data-zone-title-surface=").nth(1).unwrap();
    let y: f64 = surface
        .split(" y=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(top - y < 64., "title must remain beside its own boundary");
}

#[test]
fn replacing_all_nodes_in_an_editor_uses_the_same_fresh_presentation() {
    let mut doc = Document::new("old_client -> old_server\n").unwrap();
    let source = mesh_source();
    doc.patch(
        doc.revision(),
        &[edit::TextPatch::replace(
            doc.source(),
            smarter_diagrams::syntax::Span::new(0, doc.source().len()),
            &source,
        )],
    )
    .unwrap();
    assert_eq!(crossings(&doc.snapshot()), 1);
    assert_eq!(doc.snapshot().to_svg(), compile(&source).unwrap().to_svg());
}

#[test]
fn shortcut_backbones_are_recognized_independently_of_node_declaration_order() {
    let d=compile("s7\ns2\ns5\ns0\ns3\ns1\ns6\ns4\ns0 -> s1 -> s2 -> s3 -> s4 -> s5 -> s6 -> s7: data\ns0 -> s6: skip\ns1 -> s7: cache\n").unwrap();
    let b = render::bounds(&d.scene, &d.layout);
    assert!(b.w < 1200. && b.h < 850.);
    assert!(
        d.layout
            .nodes
            .values()
            .any(|r| r.y > d.layout.nodes["s0"].y + 20.)
    );
    layout::validate(&d.scene, &d.layout, &LayoutOptions::default()).unwrap();
}
