//! Text-authored diagrams with lossless syntax, transactional editing and deterministic layout.
//!
//! ```
//! let diagram = smarter_diagrams::compile("client -> gateway: HTTPS\n")?;
//! assert_eq!(diagram.scene.nodes.len(), 2);
//! let svg = diagram.to_svg();
//! assert!(svg.contains("<svg"));
//! # Ok::<(), smarter_diagrams::Error>(())
//! ```
pub mod document;
pub mod drift;
pub mod edit;
#[cfg(feature = "export")]
pub mod export;
pub mod formal;
pub mod labels;
pub mod layout;
pub mod lint;
pub mod mcp;
pub mod model;
pub mod render;
pub mod routing;
pub mod simulation;
pub mod solver;
pub mod spatial;
pub mod syntax;
pub mod topology;
pub mod views;
#[cfg(feature = "wasm")]
mod wasm;

pub use document::Document;
pub use layout::{Layout, LayoutOptions};
pub use model::{Diagnostic, SceneGraph};
pub use syntax::Parse;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid diagram: {0:?}")]
    Diagnostics(Vec<Diagnostic>),
    #[error("invalid edit: {0}")]
    Edit(String),
    #[error("infeasible layout: {0}")]
    Layout(String),
}

#[derive(Debug, Clone)]
pub struct Diagram {
    pub parse: Parse,
    pub scene: SceneGraph,
    pub layout: Layout,
}
impl Diagram {
    pub fn to_svg(&self) -> String {
        render::svg(&self.scene, &self.layout)
    }
}
pub fn compile(source: &str) -> Result<Diagram, Error> {
    compile_with(source, &LayoutOptions::default(), None)
}
pub fn compile_with(
    source: &str,
    options: &LayoutOptions,
    previous: Option<&Layout>,
) -> Result<Diagram, Error> {
    let parse = syntax::parse(source);
    let scene = model::lower(&parse);
    let errors: Vec<_> = scene
        .diagnostics
        .iter()
        .filter(|d| d.severity == model::Severity::Error)
        .cloned()
        .collect();
    if !errors.is_empty() {
        return Err(Error::Diagnostics(errors));
    }
    let layout = layout::layout(&scene, options, previous)?;
    Ok(Diagram {
        parse,
        scene,
        layout,
    })
}
