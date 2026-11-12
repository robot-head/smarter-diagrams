//! Machine-readable semantic checks for the visual audit corpora.
use std::io::Read;
fn main() {
    let mut source = String::new();
    std::io::stdin().read_to_string(&mut source).unwrap();
    let diagram = smarter_diagrams::compile(&source).unwrap();
    let mut missing: Vec<_> = diagram
        .scene
        .edges
        .iter()
        .filter(|edge| {
            !edge.label.is_empty() && !diagram.layout.labels.iter().any(|l| l.feature == edge.id)
        })
        .map(|e| format!("Missing caption for {}: {}", e.id, e.label))
        .collect();
    for node in &diagram.scene.nodes {
        if node.shape == "sql_table" && !diagram.layout.labels.iter().any(|l| l.feature == node.id)
        {
            missing.push(format!("Missing record title for {}", node.id));
        }
        for port in node.ports.values() {
            if node.shape == "sql_table"
                && matches!(
                    port.constraint,
                    smarter_diagrams::model::PortConstraint::FixedPos { .. }
                )
            {
                continue;
            }
            let uses: Vec<_> = diagram
                .scene
                .edges
                .iter()
                .filter(|e| {
                    e.source.node == node.id && e.source.port.as_ref() == Some(&port.id)
                        || e.target.node == node.id && e.target.port.as_ref() == Some(&port.id)
                })
                .collect();
            if !uses.is_empty() && uses.iter().all(|e| e.label.trim() == port.label.trim()) {
                continue;
            }
            let feature = format!("{}#{}", node.id, port.id);
            if !diagram.layout.labels.iter().any(|l| l.feature == feature) {
                missing.push(format!(
                    "Missing port caption for {feature}: {}",
                    port.label
                ));
            }
        }
    }
    println!(
        "{}",
        serde_json::json!({"layout":diagram.layout,"semanticFailures":missing,"edgeCaptions":diagram.scene.edges.iter().map(|e|(&e.id,&e.label)).collect::<Vec<_>>()})
    );
}
