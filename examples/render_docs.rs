//! Build-time bridge for visual documentation, using the actual library API.
use serde::Deserialize;
use serde_json::json;
use smarter_diagrams::{LayoutOptions, compile_with, layout, views};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    source: String,
    view: Option<String>,
    variant: Option<String>,
    focus: Option<String>,
    label: String,
    #[serde(default)]
    expect_error: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let requests: Vec<Request> = serde_json::from_reader(std::io::stdin().lock())?;
    let mut output = Vec::with_capacity(requests.len());
    for request in requests {
        let options = LayoutOptions {
            variant: request.variant,
            ..Default::default()
        };
        let render = || -> Result<serde_json::Value, smarter_diagrams::Error> {
            let mut diagram = compile_with(&request.source, &options, None)?;
            if let Some(name) = &request.view {
                diagram.scene = views::project(&diagram.scene, name)?;
                diagram.layout = layout::layout(&diagram.scene, &options, None)?;
            }
            let svg = if let Some(focus) = &request.focus {
                if diagram.scene.node(focus).is_none() {
                    return Err(smarter_diagrams::Error::Edit(format!(
                        "unknown focus node {focus}"
                    )));
                }
                smarter_diagrams::render::focused_svg(&diagram.scene, &diagram.layout, focus, 1)
            } else {
                diagram.to_svg()
            };
            let bounds = smarter_diagrams::render::bounds(&diagram.scene, &diagram.layout);
            Ok(json!({
                "svg":svg,"viewBox":[bounds.left(),bounds.top(),bounds.w,bounds.h],"grid":options.grid,
                "description":smarter_diagrams::render::description(&diagram.scene),
                "nodes":diagram.scene.nodes.iter().map(|n|json!({"id":n.id,"label":n.label,"shape":n.shape,"parent":n.parent,"bounds":diagram.layout.nodes[&n.id],"ports":n.ports.values().map(|p|json!({"id":p.id,"label":p.label,"constraint":p.constraint})).collect::<Vec<_>>()})).collect::<Vec<_>>(),
                "edges":diagram.scene.edges.iter().map(|e|{
                    let name=|ep:&smarter_diagrams::model::Endpoint|format!("{}{}",ep.node,ep.port.as_ref().map(|p|format!("#{p}")).unwrap_or_else(||ep.constraint.side().map(|side|format!(".{side:?}")).unwrap_or_default()));
                    json!({"id":e.id,"source":name(&e.source),"target":name(&e.target),"label":e.label,"kind":e.kind})
                }).collect::<Vec<_>>(),
                "zones":diagram.scene.zones,"flows":diagram.scene.flows
            }))
        };
        match (render(), request.expect_error) {
            (Ok(_), true) => {
                return Err(format!("{}: expected an infeasible layout", request.label).into());
            }
            (Err(error), true) => output.push(json!({"diagnostic":error.to_string()})),
            (Ok(result), false) => output.push(result),
            (Err(error), false) => return Err(format!("{}: {error}", request.label).into()),
        }
    }
    serde_json::to_writer(std::io::stdout().lock(), &output)?;
    Ok(())
}
