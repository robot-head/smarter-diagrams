//! Text-authored diagrams with lossless syntax, transactional editing and deterministic layout.
//!
//! The [online guide](https://robot-head.github.io/smarter-diagrams/) covers the
//! diagram language, CLI, Wasm bindings, integrations and executable extensions.
//! Start with [`compile`] for static export, or [`Document`] for a live editor.
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
/// A synchronized syntax, semantic graph and geometry compilation result.
pub struct Diagram {
    /// Immutable lossless source syntax, including trivia and source ranges.
    pub parse: Parse,
    /// Resolved entities, ownership, ports and shared semantic relations.
    pub scene: SceneGraph,
    /// Validated node boxes, connector routes, ports, labels and zones.
    pub layout: Layout,
}
impl Diagram {
    /// Render the existing scene and geometry as escaped, accessible SVG.
    pub fn to_svg(&self) -> String {
        render::svg(&self.scene, &self.layout)
    }
}
/// Compile UTF-8 source with default options and no prior geometry.
///
/// Returns source diagnostics or a layout error for inconsistent hard constraints.
pub fn compile(source: &str) -> Result<Diagram, Error> {
    compile_with(source, &LayoutOptions::default(), None)
}
/// Compile with explicit layout options and optional previous geometry.
///
/// Surviving entities anchor to the prior layout. Compatible semantic geometry
/// and independent container queries can be reused; structural changes may
/// require a global solve. See [`LayoutOptions`] and [`Layout`].
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
