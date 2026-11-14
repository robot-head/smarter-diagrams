use wasm_bindgen::prelude::*;
fn parse_options(options_json: &str) -> Result<crate::LayoutOptions, JsValue> {
    if options_json.trim().is_empty() {
        Ok(crate::LayoutOptions::default())
    } else {
        serde_json::from_str(options_json).map_err(js_error)
    }
}
#[wasm_bindgen]
pub fn render_svg(source: &str) -> Result<String, JsValue> {
    crate::compile(source)
        .map(|d| d.to_svg())
        .map_err(|e| JsValue::from_str(&e.with_source(source)))
}
#[wasm_bindgen]
pub fn available_themes_json() -> Result<String, JsValue> {
    let items: Vec<_> = crate::themes::all_builtin()
        .into_iter()
        .map(|t| serde_json::json!({ "id": t.id, "label": t.label, "dark": t.dark }))
        .collect();
    serde_json::to_string(&items).map_err(js_error)
}
#[wasm_bindgen]
pub fn parse_base16_theme_json(spec: &str) -> Result<String, JsValue> {
    let palette = crate::themes::ThemePalette::from_base16_str(spec).map_err(js_error)?;
    serde_json::to_string(&palette).map_err(js_error)
}
#[wasm_bindgen]
pub fn render_svg_themed(source: &str, theme_or_base16: &str) -> Result<String, JsValue> {
    let diagram = crate::compile(source).map_err(|e| JsValue::from_str(&e.with_source(source)))?;
    if theme_or_base16.trim().is_empty() || theme_or_base16 == "auto" {
        return Ok(diagram.to_svg());
    }
    let palette = if let Some(preset) = crate::themes::by_name(theme_or_base16) {
        preset
    } else if theme_or_base16.trim_start().starts_with('{') || theme_or_base16.contains("base00") {
        crate::themes::ThemePalette::from_base16_str(theme_or_base16).map_err(js_error)?
    } else {
        return Err(JsValue::from_str(&format!(
            "unknown theme {theme_or_base16}"
        )));
    };
    Ok(diagram.to_svg_with_theme(&palette))
}
#[wasm_bindgen]
pub fn render_svg_with_options(source: &str, options_json: &str) -> Result<String, JsValue> {
    let options = parse_options(options_json)?;
    crate::compile_with(source, &options, None)
        .map(|d| d.to_svg())
        .map_err(|e| JsValue::from_str(&e.with_source(source)))
}
#[wasm_bindgen]
pub fn layout_json(source: &str) -> Result<String, JsValue> {
    let d = crate::compile(source).map_err(|e| JsValue::from_str(&e.with_source(source)))?;
    serde_json::to_string(&d.layout).map_err(|e| JsValue::from_str(&e.to_string()))
}
#[wasm_bindgen]
pub fn layout_json_with_options(source: &str, options_json: &str) -> Result<String, JsValue> {
    let options = parse_options(options_json)?;
    let d = crate::compile_with(source, &options, None)
        .map_err(|e| JsValue::from_str(&e.with_source(source)))?;
    serde_json::to_string(&d.layout).map_err(js_error)
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
            inner: crate::Document::new(source).map_err(|e| js_error(e.with_source(source)))?,
        })
    }
    pub fn new_with_options(source: &str, options_json: &str) -> Result<WasmDocument, JsValue> {
        let options = parse_options(options_json)?;
        Ok(Self {
            inner: crate::Document::with_options(source, options)
                .map_err(|e| js_error(e.with_source(source)))?,
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
            return Err(js_error(
                crate::Error::Diagnostics(scene.diagnostics).with_source(source),
            ));
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
/// Browser handle driving an executable Harel statechart optionally coupled to queue simulation.
#[cfg(feature = "wasm-runtime")]
#[wasm_bindgen]
pub struct WasmStatechart {
    chart: crate::statecharts::Statechart,
    sim: Option<crate::simulation::Simulation>,
}
#[cfg(feature = "wasm-runtime")]
#[wasm_bindgen]
impl WasmStatechart {
    #[wasm_bindgen(constructor)]
    pub fn new(source: &str) -> Result<WasmStatechart, JsValue> {
        let chart = crate::statecharts::Statechart::from_source(source)
            .map_err(|e| JsValue::from_str(&e.with_source(source)))?;
        let sim = crate::simulation::Simulation::from_scene(&chart.diagram().scene).ok();
        Ok(Self { chart, sim })
    }

    pub fn dispatch(&mut self, event: &str) -> Result<bool, JsValue> {
        let res = match &mut self.sim {
            Some(sim) => self.chart.dispatch_with_simulation(event, sim),
            None => self.chart.dispatch(event),
        };
        res.map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn step_coupled(&mut self, ticks: usize) -> Result<bool, JsValue> {
        if let Some(sim) = &mut self.sim {
            self.chart
                .step_coupled(sim, ticks)
                .map_err(|e| JsValue::from_str(&e.to_string()))
        } else {
            Ok(false)
        }
    }

    pub fn set_rate(&mut self, node: &str, rate: f64) -> Result<(), JsValue> {
        if let Some(sim) = &mut self.sim {
            sim.set_rate(node, rate)
                .map_err(|e| JsValue::from_str(&e.to_string()))?;
        }
        Ok(())
    }

    pub fn sever(&mut self, edge: &str, severed: bool) -> Result<(), JsValue> {
        if let Some(sim) = &mut self.sim {
            sim.sever(edge, severed)
                .map_err(|e| JsValue::from_str(&e.to_string()))?;
        }
        Ok(())
    }

    pub fn set_context_json(&mut self, json: &str) -> Result<(), JsValue> {
        self.chart
            .set_context_json(json)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn reset(&mut self) {
        self.chart.reset();
        self.sim = crate::simulation::Simulation::from_scene(&self.chart.diagram().scene).ok();
    }

    pub fn events_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&self.chart.inspect_events(self.sim.as_ref()))
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn state_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&self.chart.snapshot(self.sim.as_ref()))
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn svg(&self) -> String {
        self.chart.to_svg_with_simulation(self.sim.as_ref())
    }

    pub fn svg_themed(&self, theme_or_base16: &str) -> Result<String, JsValue> {
        let palette = crate::themes::by_name(theme_or_base16)
            .map(Ok)
            .unwrap_or_else(|| crate::themes::ThemePalette::from_base16_str(theme_or_base16))
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(self.chart.to_svg_with_theme(&palette, self.sim.as_ref()))
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
    /// Render a provisional gesture without changing source, revision or history.
    pub fn preview_drag(&self, drags_json: &str) -> Result<String, JsValue> {
        let mut preview = self.begin_drag()?;
        preview.update(drags_json)?;
        Ok(preview.svg())
    }

    pub fn begin_drag(&self) -> Result<WasmDragPreview, JsValue> {
        Ok(WasmDragPreview {
            inner: self.inner.begin_drag().map_err(js_error)?,
        })
    }
    pub fn commit_drag(&mut self, preview: &WasmDragPreview) -> Result<(), JsValue> {
        self.inner.commit_drag(&preview.inner).map_err(js_error)
    }

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

#[wasm_bindgen]
pub struct WasmDragPreview {
    inner: crate::DragSession,
}
#[wasm_bindgen]
impl WasmDragPreview {
    pub fn update(&mut self, drags_json: &str) -> Result<(), JsValue> {
        let drags: Vec<crate::edit::Drag> = serde_json::from_str(drags_json).map_err(js_error)?;
        self.inner.update(&drags).map_err(js_error)
    }
    pub fn update_label(&mut self, feature: &str, x: f64, y: f64) -> Result<(), JsValue> {
        self.inner
            .update_label(feature, crate::layout::Point::new(x, y))
            .map_err(js_error)
    }
    pub fn reset_label(&mut self, feature: &str) -> Result<(), JsValue> {
        self.inner.reset_label(feature).map_err(js_error)
    }
    pub fn settle(&mut self) -> Result<(), JsValue> {
        self.inner.settle().map_err(js_error)
    }
    pub fn svg(&self) -> String {
        self.inner.snapshot().to_svg()
    }
}
#[cfg(feature = "wasm-runtime")]
#[wasm_bindgen]
pub fn stack_json(source: &str, theta: f64) -> Result<String, JsValue> {
    let scene = crate::model::lower(&crate::syntax::parse(source));
    let stack = crate::spatial::strata(&scene, &Default::default(), theta).map_err(js_error)?;
    serde_json::to_string(&crate::spatial::presentation(&scene, &stack).map_err(js_error)?)
        .map_err(js_error)
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
    pub fn svg_themed(&self, theme_or_base16: &str) -> Result<String, JsValue> {
        let d = self.inner.snapshot();
        if theme_or_base16.trim().is_empty() || theme_or_base16 == "auto" {
            return Ok(d.to_svg());
        }
        let palette = if let Some(preset) = crate::themes::by_name(theme_or_base16) {
            preset
        } else if theme_or_base16.trim_start().starts_with('{')
            || theme_or_base16.contains("base00")
        {
            crate::themes::ThemePalette::from_base16_str(theme_or_base16).map_err(js_error)?
        } else {
            return Err(JsValue::from_str(&format!(
                "unknown theme {theme_or_base16}"
            )));
        };
        Ok(d.to_svg_with_theme(&palette))
    }
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
        self.inner
            .patch(revision, &[patch])
            .map_err(|e| js_error(e.with_source(source)))
    }
}
