//! Machine-readable semantic checks for the visual audit corpora.
use std::io::Read;
fn main() {
    let mut source = String::new();
    std::io::stdin().read_to_string(&mut source).unwrap();
    let diagram = smarter_diagrams::compile(&source).unwrap();
    let missing: Vec<_> = diagram
        .scene
        .edges
        .iter()
        .filter(|edge| {
            !edge.label.is_empty() && !diagram.layout.labels.iter().any(|l| l.feature == edge.id)
        })
        .map(|e| format!("Missing caption for {}: {}", e.id, e.label))
        .collect();
    println!(
        "{}",
        serde_json::json!({"layout":diagram.layout,"semanticFailures":missing,"edgeCaptions":diagram.scene.edges.iter().map(|e|(&e.id,&e.label)).collect::<Vec<_>>()})
    );
}
