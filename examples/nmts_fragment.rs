//! Demonstrate graphing an Outernet Council NMTS (nmts.v1.Fragment) entity-relationship model.
//!
//! Lowers `RK_CONTAINS` hierarchies into scoped containers, separates `logical` and `physical`
//! `EK_*` entity kinds into strata, and renders both 2D canonical SVG and 2.5D strata vias.
//!
//! Run with:
//!   cargo run --example nmts_fragment
use serde::Deserialize;
use smarter_diagrams::{LayoutOptions, compile, spatial};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Deserialize)]
struct Fragment {
    entity: Vec<Entity>,
    relationship: Vec<Relationship>,
}

#[derive(Debug, Deserialize)]
struct Entity {
    id: String,
    kind: String,
    #[serde(default)]
    labels: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct Relationship {
    kind: String,
    a: String,
    z: String,
}

fn sanitize_id(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn entity_stratum(kind: &str) -> &'static str {
    match kind {
        "EK_NETWORK_NODE"
        | "EK_INTERFACE"
        | "EK_ROUTE_FN"
        | "EK_SWITCH_FN"
        | "EK_SDN_AGENT"
        | "EK_LOGICAL_PACKET_LINK"
        | "EK_UNI" => "logical",
        _ => "physical",
    }
}

fn entity_shape(kind: &str) -> Option<&'static str> {
    match kind {
        "EK_PHYSICAL_MEDIUM_LINK" | "EK_LOGICAL_PACKET_LINK" => Some("queue"),
        "EK_ROUTE_FN" => Some("sql_table"),
        _ => None,
    }
}

fn emit_entity(
    id: &str,
    entities: &BTreeMap<String, &Entity>,
    children: &BTreeMap<String, Vec<String>>,
    contained_edges: &[(String, String, String)],
    indent: usize,
    out: &mut String,
) {
    let ent = entities[id];
    let pad = "  ".repeat(indent);
    let local = sanitize_id(id);
    let display = ent
        .labels
        .get("display_name")
        .cloned()
        .unwrap_or_else(|| id.to_string());
    let kids = children.get(id).cloned().unwrap_or_default();
    if kids.is_empty() {
        let mut attrs = vec![format!("stratum: {}", entity_stratum(&ent.kind))];
        if let Some(shape) = entity_shape(&ent.kind) {
            attrs.insert(0, format!("shape: {shape}"));
        }
        let caption = format!("{display}\\n({})", ent.kind);
        out.push_str(&format!(
            "{pad}{local}: \"{caption}\" [{}]\n",
            attrs.join(", ")
        ));
    } else {
        out.push_str(&format!("{pad}{local}: \"{display} ({})\" {{\n", ent.kind));
        for child in &kids {
            emit_entity(child, entities, children, contained_edges, indent + 1, out);
        }
        let kid_set: BTreeSet<&str> = kids.iter().map(String::as_str).collect();
        for (a, z, rk) in contained_edges {
            if kid_set.contains(a.as_str()) && kid_set.contains(z.as_str()) {
                out.push_str(&format!(
                    "{pad}  {} -> {}: \"{rk}\"\n",
                    sanitize_id(a),
                    sanitize_id(z)
                ));
            }
        }
        out.push_str(&format!("{pad}}}\n"));
    }
}

fn fragment_to_diagram(fragment: &Fragment) -> String {
    let entities: BTreeMap<String, &Entity> =
        fragment.entity.iter().map(|e| (e.id.clone(), e)).collect();
    let mut children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut parent_of: BTreeMap<String, String> = BTreeMap::new();
    let mut non_contains = Vec::new();

    for rel in &fragment.relationship {
        if rel.kind == "RK_CONTAINS" {
            children
                .entry(rel.a.clone())
                .or_default()
                .push(rel.z.clone());
            parent_of.insert(rel.z.clone(), rel.a.clone());
        } else {
            non_contains.push((rel.a.clone(), rel.z.clone(), rel.kind.clone()));
        }
    }

    let qualified = |id: &str| -> String {
        let mut chain = vec![sanitize_id(id)];
        let mut cur = id;
        while let Some(p) = parent_of.get(cur) {
            chain.push(sanitize_id(p));
            cur = p;
        }
        chain.reverse();
        chain.join(".")
    };

    let mut out = String::from("strata [logical, physical]\n\n");
    for ent in &fragment.entity {
        if !parent_of.contains_key(&ent.id) {
            emit_entity(&ent.id, &entities, &children, &non_contains, 0, &mut out);
        }
    }
    out.push('\n');
    for (a, z, rk) in &non_contains {
        if parent_of.get(a) != parent_of.get(z) || !parent_of.contains_key(a) {
            out.push_str(&format!("{} -> {}: \"{rk}\"\n", qualified(a), qualified(z)));
        }
    }
    out
}

const SAMPLE_NMTS_JSON: &str = r#"{
  "entity": [
    { "id": "gs-arl", "kind": "EK_PLATFORM", "labels": { "display_name": "GS Arlington" } },
    { "id": "gs-router", "kind": "EK_NETWORK_NODE", "labels": { "display_name": "Gateway Router" } },
    { "id": "gse0", "kind": "EK_INTERFACE", "labels": { "display_name": "gse0" } },
    { "id": "mod-ka", "kind": "EK_MODULATOR", "labels": { "display_name": "DVB-S2X Mod" } },
    { "id": "dish-ka", "kind": "EK_ANTENNA", "labels": { "display_name": "4.5m Ka Dish" } },
    { "id": "ka-uplink", "kind": "EK_PHYSICAL_MEDIUM_LINK", "labels": { "display_name": "Ka 29.5 GHz" } },
    { "id": "leo-142", "kind": "EK_PLATFORM", "labels": { "display_name": "LEO-142" } },
    { "id": "array-ka", "kind": "EK_ANTENNA", "labels": { "display_name": "Phased Array" } },
    { "id": "demod-ka", "kind": "EK_DEMODULATOR", "labels": { "display_name": "Ka Demod" } },
    { "id": "sat-sw", "kind": "EK_NETWORK_NODE", "labels": { "display_name": "Onboard Switch" } },
    { "id": "ts-sdn", "kind": "EK_SDN_AGENT", "labels": { "display_name": "Spacetime SDN" } }
  ],
  "relationship": [
    { "kind": "RK_CONTAINS", "a": "gs-arl", "z": "ts-sdn" },
    { "kind": "RK_CONTAINS", "a": "gs-arl", "z": "gs-router" },
    { "kind": "RK_CONTAINS", "a": "gs-arl", "z": "gse0" },
    { "kind": "RK_CONTAINS", "a": "gs-arl", "z": "mod-ka" },
    { "kind": "RK_CONTAINS", "a": "gs-arl", "z": "dish-ka" },
    { "kind": "RK_CONTAINS", "a": "leo-142", "z": "array-ka" },
    { "kind": "RK_CONTAINS", "a": "leo-142", "z": "demod-ka" },
    { "kind": "RK_CONTAINS", "a": "leo-142", "z": "sat-sw" },
    { "kind": "RK_ORIGINATES", "a": "gs-router", "z": "gse0" },
    { "kind": "RK_TRAVERSES", "a": "gse0", "z": "mod-ka" },
    { "kind": "RK_SIGNAL_TRANSITS", "a": "mod-ka", "z": "dish-ka" },
    { "kind": "RK_SIGNAL_TRANSITS", "a": "dish-ka", "z": "ka-uplink" },
    { "kind": "RK_SIGNAL_TRANSITS", "a": "ka-uplink", "z": "array-ka" },
    { "kind": "RK_SIGNAL_TRANSITS", "a": "array-ka", "z": "demod-ka" },
    { "kind": "RK_TRAVERSES", "a": "demod-ka", "z": "sat-sw" },
    { "kind": "RK_CONTROLS", "a": "ts-sdn", "z": "gs-router" },
    { "kind": "RK_CONTROLS", "a": "ts-sdn", "z": "mod-ka" }
  ]
}"#;

fn fragment_to_strata_diagram(fragment: &Fragment) -> String {
    let mut out = String::from("strata [logical, physical]\n\n");
    for ent in &fragment.entity {
        if ent.kind == "EK_PLATFORM" {
            continue;
        }
        let id = sanitize_id(&ent.id);
        let display = ent
            .labels
            .get("display_name")
            .cloned()
            .unwrap_or_else(|| ent.id.clone());
        out.push_str(&format!(
            "{id}: \"{display}\" [stratum: {}]\n",
            entity_stratum(&ent.kind)
        ));
    }
    out.push('\n');
    for rel in &fragment.relationship {
        if rel.kind != "RK_CONTAINS" {
            out.push_str(&format!(
                "{} -> {}: \"{}\"\n",
                sanitize_id(&rel.a),
                sanitize_id(&rel.z),
                rel.kind
            ));
        }
    }
    out
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fragment: Fragment = serde_json::from_str(SAMPLE_NMTS_JSON)?;
    let generated_source = fragment_to_diagram(&fragment);
    let diagram = compile(&generated_source)?;
    let svg_2d = diagram.to_svg();

    let strata_scene = smarter_diagrams::model::lower(&smarter_diagrams::syntax::parse(
        &fragment_to_strata_diagram(&fragment),
    ));
    let stack = spatial::strata(&strata_scene, &LayoutOptions::default(), 0.36)?;
    let stack_json = serde_json::to_string_pretty(&stack)?;

    // Temporospatial contact-window update: reroute bearer onto backup Ku-band link.
    let handoff_json = SAMPLE_NMTS_JSON
        .replace("ka-uplink", "ku-backup")
        .replace("Ka 29.5 GHz", "Ku Backup 14.2 GHz");
    let next_fragment: Fragment = serde_json::from_str(&handoff_json)?;
    let next_diagram = compile(&fragment_to_diagram(&next_fragment))?;
    let handoff_diff = spatial::compare(
        &diagram.scene,
        &next_diagram.scene,
        &diagram.layout,
        &LayoutOptions::default(),
    )?;
    let handoff_svg = spatial::diff_svg(&handoff_diff, 1.0)?;

    let curated = compile(&std::fs::read_to_string("examples/nmts-fragment.diagram")?)?;
    assert!(svg_2d.contains("RK_SIGNAL_TRANSITS"));
    assert_eq!(stack.planes.len(), 2);
    assert!(!stack.vias.is_empty());

    if let Some(dir) = std::env::args().nth(1) {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(format!("{dir}/nmts-generated.diagram"), &generated_source)?;
        std::fs::write(format!("{dir}/nmts-generated.svg"), &svg_2d)?;
        std::fs::write(format!("{dir}/nmts-strata.json"), &stack_json)?;
        std::fs::write(format!("{dir}/nmts-handoff-diff.svg"), &handoff_svg)?;
        std::fs::write(format!("{dir}/nmts-curated.svg"), curated.to_svg())?;
    }

    println!(
        "NMTS fragment: {} entities, {} relationships -> {} planes ({} cross-stratum vias) | contact-window diff rendered",
        fragment.entity.len(),
        fragment.relationship.len(),
        stack.planes.len(),
        stack.vias.len(),
    );
    Ok(())
}
