//! Versioned immutable compiler snapshots. Failed transactions never modify history.
use crate::{
    Diagram, Error, compile_with,
    edit::{self, DiagramEditOp, Placement, TextPatch},
    layout::LayoutOptions,
    model, syntax,
};
use std::sync::Arc;
#[derive(Debug, Clone)]
pub struct Document {
    current: Arc<Diagram>,
    undo: Vec<Arc<Diagram>>,
    redo: Vec<Arc<Diagram>>,
    revision: u64,
    options: LayoutOptions,
    #[cfg(feature = "verification")]
    policies: Vec<VerificationPolicy>,
    #[cfg(feature = "verification")]
    checks: Vec<crate::formal::NamedCheck>,
}
impl Document {
    pub fn new(source: &str) -> Result<Self, Error> {
        Self::with_options(source, LayoutOptions::default())
    }
    pub fn with_options(source: &str, options: LayoutOptions) -> Result<Self, Error> {
        let current = Arc::new(compile_with(source, &options, None)?);
        Ok(Self {
            current,
            undo: Vec::new(),
            redo: Vec::new(),
            revision: 0,
            options,
            #[cfg(feature = "verification")]
            policies: Vec::new(),
            #[cfg(feature = "verification")]
            checks: Vec::new(),
        })
    }
    pub fn snapshot(&self) -> Arc<Diagram> {
        self.current.clone()
    }
    pub fn source(&self) -> &str {
        &self.current.parse.source
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// Begin a source-preserving canvas gesture. Only valid candidates can commit.
    pub fn begin_drag(&self) -> Result<crate::DragSession, Error> {
        crate::DragSession::new(self.current.clone(), self.revision, self.options.clone())
    }
    /// Save precisely the candidate shown during the gesture, with one undo step.
    pub fn commit_drag(&mut self, gesture: &crate::DragSession) -> Result<(), Error> {
        self.check_revision(gesture.revision)?;
        if !Arc::ptr_eq(&self.current, &gesture.baseline) {
            return Err(Error::Edit(
                "drag belongs to a different document snapshot".into(),
            ));
        }
        gesture.settle()?;
        if gesture.prepared.parse.source == self.current.parse.source {
            return Ok(());
        }
        self.commit((*gesture.prepared).clone())
    }
    fn commit(&mut self, next: Diagram) -> Result<(), Error> {
        #[cfg(feature = "verification")]
        let checks = self.run_verifiers(&next.scene)?;
        self.undo.push(self.current.clone());
        self.current = Arc::new(next);
        self.redo.clear();
        self.revision += 1;
        #[cfg(feature = "verification")]
        {
            self.checks = checks;
        }
        Ok(())
    }
    pub fn edit(
        &mut self,
        expected_revision: u64,
        ops: &[DiagramEditOp],
        placement: Placement,
    ) -> Result<(), Error> {
        self.check_revision(expected_revision)?;
        let next = edit::apply_ops(self.source(), ops, placement)?;
        if next.source == self.source() {
            return Ok(());
        }
        let diagram = compile_with(&next.source, &self.options, Some(&self.current.layout))?;
        self.commit(diagram)
    }
    pub fn patch(&mut self, expected_revision: u64, patches: &[TextPatch]) -> Result<(), Error> {
        self.check_revision(expected_revision)?;
        let source = edit::apply_patches(self.source(), patches)?;
        if source == self.source() {
            return Ok(());
        }
        let parse = if patches.len() == 1 {
            let p = &patches[0];
            self.current.parse.reparse(p.delete_range, &p.insert_text)?
        } else {
            syntax::parse(&source)
        };
        let scene = if let [patch] = patches {
            model::lower_incremental(
                &parse,
                &self.current.parse,
                &self.current.scene,
                patch.delete_range,
                &patch.insert_text,
            )
        } else {
            model::lower(&parse)
        };
        let errors: Vec<_> = scene
            .diagnostics
            .iter()
            .filter(|d| d.severity == model::Severity::Error)
            .cloned()
            .collect();
        if !errors.is_empty() {
            return Err(Error::Diagnostics(errors));
        }
        let layout = crate::layout::layout(&scene, &self.options, Some(&self.current.layout))?;
        self.commit(Diagram {
            parse,
            scene,
            layout,
        })
    }
    fn check_revision(&self, revision: u64) -> Result<(), Error> {
        if revision != self.revision {
            Err(Error::Edit(format!(
                "stale revision {revision}; current revision is {}",
                self.revision
            )))
        } else {
            Ok(())
        }
    }
    pub fn undo(&mut self) -> bool {
        if let Some(previous) = self.undo.pop() {
            self.redo.push(self.current.clone());
            self.current = previous;
            self.revision += 1;
            #[cfg(feature = "verification")]
            {
                self.checks = self.run_verifiers(&self.current.scene).unwrap_or_default();
            }
            true
        } else {
            false
        }
    }
    pub fn redo(&mut self) -> bool {
        if let Some(next) = self.redo.pop() {
            self.undo.push(self.current.clone());
            self.current = next;
            self.revision += 1;
            #[cfg(feature = "verification")]
            {
                self.checks = self.run_verifiers(&self.current.scene).unwrap_or_default();
            }
            true
        } else {
            false
        }
    }
}

#[cfg(feature = "verification")]
#[derive(Debug, Clone)]
pub struct VerificationPolicy {
    pub zone: String,
    pub system: crate::formal::TransitionSystem,
    pub horizon: usize,
}
#[cfg(feature = "verification")]
impl Document {
    pub fn verify_continuously(
        &mut self,
        policy: VerificationPolicy,
    ) -> Result<&[crate::formal::NamedCheck], Error> {
        let checks = crate::formal::check_zone(
            &self.current.scene,
            &policy.zone,
            &policy.system,
            policy.horizon,
        )?;
        self.policies.retain(|p| p.zone != policy.zone);
        self.policies.push(policy);
        self.checks = checks;
        Ok(&self.checks)
    }
    pub fn verification_results(&self) -> &[crate::formal::NamedCheck] {
        &self.checks
    }
    fn run_verifiers(
        &self,
        scene: &crate::model::SceneGraph,
    ) -> Result<Vec<crate::formal::NamedCheck>, Error> {
        let mut results = Vec::new();
        for policy in &self.policies {
            if !scene.zones.iter().any(|z| z.id == policy.zone) {
                continue;
            }
            let mut system = policy.system.clone();
            system
                .transitions
                .retain(|t| t.edge.as_ref().is_none_or(|id| scene.edge(id).is_some()));
            results.extend(crate::formal::check_zone(
                scene,
                &policy.zone,
                &system,
                policy.horizon,
            )?);
        }
        Ok(results)
    }
}
