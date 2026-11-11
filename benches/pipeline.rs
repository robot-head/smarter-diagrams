use smarter_diagrams::{
    Document, compile, compile_with,
    edit::TextPatch,
    layout::LayoutOptions,
    syntax::{self, Span},
};
use std::time::Instant;
fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}
fn main() {
    println!("case,nodes,initial_ms,incremental_median_ms,reparse_median_ms");
    for n in [10, 30, 100, 500] {
        let source = (0..n - 1)
            .map(|i| format!("n{i} -> n{}\n", i + 1))
            .collect::<String>();
        let start = Instant::now();
        let d = compile(&source).unwrap();
        let initial = start.elapsed().as_secs_f64() * 1000.;
        let edited = format!("{source}# local comment\n");
        let parse = syntax::parse(&source);
        let mut times = Vec::new();
        let mut reparses = Vec::new();
        for _ in 0..9 {
            let start = Instant::now();
            let _ = compile_with(&edited, &LayoutOptions::default(), Some(&d.layout)).unwrap();
            times.push(start.elapsed().as_secs_f64() * 1000.);
            let start = Instant::now();
            let _ = parse.reparse(Span::new(0, 2), "m0").unwrap();
            reparses.push(start.elapsed().as_secs_f64() * 1000.);
        }
        println!(
            "chain,{n},{initial:.3},{:.3},{:.3}",
            median(times),
            median(reparses)
        );
    }
    let mut source = String::from("cluster {\n inner4: Short\n");
    for i in 0..8 {
        source.push_str(&format!(" inner{i} -> inner{}\n", i + 1));
    }
    source.push_str("}\n");
    for i in 0..539 {
        source.push_str(&format!("outside{i} -> outside{}\n", i + 1));
    }
    let start = Instant::now();
    let d = compile(&source).unwrap();
    let initial = start.elapsed().as_secs_f64() * 1000.;
    let edited = source.replace("Short", "A longer service");
    let mut times = Vec::new();
    for _ in 0..9 {
        let start = Instant::now();
        let next = compile_with(&edited, &LayoutOptions::default(), Some(&d.layout)).unwrap();
        assert!(
            next.layout.solved_nodes <= 10,
            "expected a local query, solved {} nodes",
            next.layout.solved_nodes
        );
        assert_eq!(
            next.layout.nodes["outside100"],
            d.layout.nodes["outside100"]
        );
        times.push(start.elapsed().as_secs_f64() * 1000.);
    }
    println!(
        "container-local,{},{initial:.3},{:.3},n/a",
        d.scene.nodes.len(),
        median(times)
    );
    let start = Instant::now();
    let mut document = Document::new(&source).unwrap();
    let initial = start.elapsed().as_secs_f64() * 1000.;
    let start = source.find("Short").unwrap();
    let patch = TextPatch::replace(&source, Span::new(start, start + 5), "A longer service");
    let mut times = Vec::new();
    for _ in 0..9 {
        let start = Instant::now();
        document
            .patch(document.revision(), std::slice::from_ref(&patch))
            .unwrap();
        times.push(start.elapsed().as_secs_f64() * 1000.);
        assert!(document.snapshot().layout.solved_nodes <= 10);
        assert!(document.undo());
    }
    println!("document-local,550,{initial:.3},{:.3},n/a", median(times));
}
