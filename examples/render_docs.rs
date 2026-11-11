//! Build-time bridge for visual documentation, using the actual library API.
use serde::Deserialize;
use smarter_diagrams::{LayoutOptions, compile_with, layout, views};

#[derive(Deserialize)]
struct Request {
    source: String,
    view: Option<String>,
    variant: Option<String>,
    focus: Option<String>,
    label: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let requests: Vec<Request> = serde_json::from_reader(std::io::stdin().lock())?;
    let mut output = Vec::with_capacity(requests.len());
    for request in requests {
        let options = LayoutOptions {
            variant: request.variant,
            ..Default::default()
        };
        let render = || -> Result<String, smarter_diagrams::Error> {
            let mut diagram = compile_with(&request.source, &options, None)?;
            if let Some(name) = &request.view {
                diagram.scene = views::project(&diagram.scene, name)?;
                diagram.layout = layout::layout(&diagram.scene, &options, None)?;
            }
            if let Some(focus) = &request.focus {
                if diagram.scene.node(focus).is_none() {
                    return Err(smarter_diagrams::Error::Edit(format!(
                        "unknown focus node {focus}"
                    )));
                }
                Ok(smarter_diagrams::render::focused_svg(
                    &diagram.scene,
                    &diagram.layout,
                    focus,
                    1,
                ))
            } else {
                Ok(diagram.to_svg())
            }
        };
        output.push(render().map_err(|error| format!("{}: {error}", request.label))?);
    }
    serde_json::to_writer(std::io::stdout().lock(), &output)?;
    Ok(())
}
