use wasm_bindgen::prelude::*;
#[wasm_bindgen]
pub fn render_svg(source: &str) -> Result<String, JsValue> {
    crate::compile(source)
        .map(|d| d.to_svg())
        .map_err(|e| JsValue::from_str(&e.to_string()))
}
#[wasm_bindgen]
pub fn layout_json(source: &str) -> Result<String, JsValue> {
    let d = crate::compile(source).map_err(|e| JsValue::from_str(&e.to_string()))?;
    serde_json::to_string(&d.layout).map_err(|e| JsValue::from_str(&e.to_string()))
}
#[wasm_bindgen]
pub fn edit_source(source: &str, operations_json: &str) -> Result<String, JsValue> {
    let ops: Vec<crate::edit::DiagramEditOp> =
        serde_json::from_str(operations_json).map_err(|e| JsValue::from_str(&format!("{e}")))?;
    crate::edit::apply_ops(source, &ops, crate::edit::Placement::Auto)
        .map(|r| r.source)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen]
pub struct WasmDocument {
    inner: crate::Document,
}
#[wasm_bindgen]
impl WasmDocument {
    #[wasm_bindgen(constructor)]
    pub fn new(source: &str) -> Result<WasmDocument, JsValue> {
        Ok(Self {
            inner: crate::Document::new(source).map_err(js_error)?,
        })
    }
    pub fn source(&self) -> String {
        self.inner.source().into()
    }
    pub fn revision(&self) -> u64 {
        self.inner.revision()
    }
    pub fn svg(&self) -> String {
        self.inner.snapshot().to_svg()
    }
    pub fn edit(&mut self, revision: u64, operations_json: &str) -> Result<(), JsValue> {
        let ops: Vec<crate::edit::DiagramEditOp> =
            serde_json::from_str(operations_json).map_err(js_error)?;
        self.inner
            .edit(revision, &ops, Default::default())
            .map_err(js_error)
    }
    pub fn undo(&mut self) -> bool {
        self.inner.undo()
    }
    pub fn redo(&mut self) -> bool {
        self.inner.redo()
    }
}
fn js_error(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}
#[cfg(feature = "wasm-runtime")]
#[wasm_bindgen]
pub struct WasmSimulation {
    inner: crate::simulation::Simulation,
}
#[cfg(feature = "wasm-runtime")]
#[wasm_bindgen]
impl WasmSimulation {
    #[wasm_bindgen(constructor)]
    pub fn new(source: &str) -> Result<WasmSimulation, JsValue> {
        let scene = crate::model::lower(&crate::syntax::parse(source));
        if !scene.diagnostics.is_empty() {
            return Err(js_error(format!("{:?}", scene.diagnostics)));
        }
        Ok(Self {
            inner: crate::simulation::Simulation::from_scene(&scene).map_err(js_error)?,
        })
    }
    pub fn tick(&mut self) -> Result<(), JsValue> {
        self.inner.tick().map_err(js_error)
    }
    pub fn set_rate(&mut self, node: &str, rate: f64) -> Result<(), JsValue> {
        self.inner.set_rate(node, rate).map_err(js_error)
    }
    pub fn sever(&mut self, edge: &str, severed: bool) -> Result<(), JsValue> {
        self.inner.sever(edge, severed).map_err(js_error)
    }
    pub fn state(&self) -> Result<String, JsValue> {
        serde_json::to_string(&self.inner).map_err(js_error)
    }
}
#[cfg(feature = "wasm-runtime")]
#[wasm_bindgen]
pub fn check_safety(
    model_json: &str,
    invariant_json: &str,
    horizon: usize,
) -> Result<String, JsValue> {
    let model = serde_json::from_str(model_json).map_err(js_error)?;
    let invariant = serde_json::from_str(invariant_json).map_err(js_error)?;
    let result = crate::formal::check(&model, &invariant, horizon).map_err(js_error)?;
    serde_json::to_string(&result).map_err(js_error)
}
#[wasm_bindgen]
impl WasmDocument {
    pub fn drag(&mut self, drags_json: &str) -> Result<(), JsValue> {
        let drags: Vec<crate::edit::Drag> = serde_json::from_str(drags_json).map_err(js_error)?;
        let snapshot = self.inner.snapshot();
        let operations = crate::edit::synthesize_drag(&snapshot.scene, &snapshot.layout, &drags)
            .map_err(js_error)?;
        self.inner
            .edit(self.inner.revision(), &operations, Default::default())
            .map_err(js_error)
    }
}
#[cfg(feature = "wasm-runtime")]
#[wasm_bindgen]
pub fn stack_json(source: &str, theta: f64) -> Result<String, JsValue> {
    let scene = crate::model::lower(&crate::syntax::parse(source));
    let stack = crate::spatial::strata(&scene, &Default::default(), theta).map_err(js_error)?;
    serde_json::to_string(&stack).map_err(js_error)
}
#[cfg(feature = "wasm-runtime")]
#[wasm_bindgen]
pub fn branch_diff_svg(base: &str, proposal: &str, tau: f64) -> Result<String, JsValue> {
    let a = crate::compile(base).map_err(js_error)?;
    let b = crate::compile(proposal).map_err(js_error)?;
    let diff = crate::spatial::compare(&a.scene, &b.scene, &a.layout, &Default::default())
        .map_err(js_error)?;
    crate::spatial::diff_svg(&diff, tau).map_err(js_error)
}
#[cfg(feature = "verification")]
#[wasm_bindgen]
impl WasmDocument {
    pub fn verify(
        &mut self,
        zone: &str,
        system_json: &str,
        horizon: usize,
    ) -> Result<String, JsValue> {
        let system = serde_json::from_str(system_json).map_err(js_error)?;
        let results = self
            .inner
            .verify_continuously(crate::document::VerificationPolicy {
                zone: zone.into(),
                system,
                horizon,
            })
            .map_err(js_error)?;
        serde_json::to_string(results).map_err(js_error)
    }
    pub fn verification_results(&self) -> Result<String, JsValue> {
        serde_json::to_string(self.inner.verification_results()).map_err(js_error)
    }
}
#[wasm_bindgen]
impl WasmDocument {
    pub fn focus_svg(&self, node: &str, hops: usize) -> String {
        let d = self.inner.snapshot();
        crate::render::focused_svg(&d.scene, &d.layout, node, hops)
    }
}
#[wasm_bindgen]
impl WasmDocument {
    pub fn flows_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&self.inner.snapshot().scene.flows).map_err(js_error)
    }
    pub fn flow_frame_svg(&self, id: &str, step: usize) -> Result<String, JsValue> {
        let d = self.inner.snapshot();
        crate::render::flow_frame_svg(&d.scene, &d.layout, id, step).map_err(js_error)
    }
}
#[wasm_bindgen]
impl WasmDocument {
    /// Replace editor text through one minimal UTF-8 splice, retaining layout and undo history.
    pub fn replace_source(&mut self, revision: u64, source: &str) -> Result<(), JsValue> {
        let old = self.inner.source();
        let mut prefix = old
            .bytes()
            .zip(source.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        while !old.is_char_boundary(prefix) || !source.is_char_boundary(prefix) {
            prefix -= 1;
        }
        let mut suffix = old[prefix..]
            .bytes()
            .rev()
            .zip(source[prefix..].bytes().rev())
            .take_while(|(a, b)| a == b)
            .count();
        while !old.is_char_boundary(old.len() - suffix)
            || !source.is_char_boundary(source.len() - suffix)
        {
            suffix -= 1;
        }
        let patch = crate::edit::TextPatch::replace(
            old,
            crate::syntax::Span::new(prefix, old.len() - suffix),
            &source[prefix..source.len() - suffix],
        );
        self.inner.patch(revision, &[patch]).map_err(js_error)
    }
}
