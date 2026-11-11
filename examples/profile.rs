use std::time::Instant;
fn main() {
    let source = (0..499)
        .map(|i| format!("n{i} -> n{}\n", i + 1))
        .collect::<String>();
    let start = Instant::now();
    let parse = smarter_diagrams::syntax::parse(&source);
    let a = start.elapsed();
    let start = Instant::now();
    let scene = smarter_diagrams::model::lower(&parse);
    let b = start.elapsed();
    let start = Instant::now();
    let layout = smarter_diagrams::layout::layout(&scene, &Default::default(), None).unwrap();
    let c = start.elapsed();
    let start = Instant::now();
    let _ = smarter_diagrams::layout::layout(&scene, &Default::default(), Some(&layout)).unwrap();
    let d = start.elapsed();
    println!("parse={a:?} lower={b:?} layout={c:?} cache={d:?}");

    let mut source = String::from("cluster {\n inner4: Short\n");
    for i in 0..8 {
        source.push_str(&format!(" inner{i} -> inner{}\n", i + 1));
    }
    source.push_str("}\n");
    for i in 0..539 {
        source.push_str(&format!("outside{i} -> outside{}\n", i + 1));
    }
    let base = smarter_diagrams::compile(&source).unwrap();
    let edited = source.replace("Short", "A longer service");
    let start = Instant::now();
    let parse = smarter_diagrams::syntax::parse(&edited);
    let parse_time = start.elapsed();
    let start = Instant::now();
    let scene = smarter_diagrams::model::lower(&parse);
    let lower_time = start.elapsed();
    let start = Instant::now();
    let layout =
        smarter_diagrams::layout::layout(&scene, &Default::default(), Some(&base.layout)).unwrap();
    let query_time = start.elapsed();
    let max_shift = layout
        .nodes
        .iter()
        .map(|(id, r)| r.center().distance(base.layout.nodes[id].center()))
        .reduce(f64::max)
        .unwrap_or(0.);
    println!(
        "container parse={parse_time:?} lower={lower_time:?} query={query_time:?} max_centroid_shift={max_shift:e}px"
    );
}
