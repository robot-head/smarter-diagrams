//! Harel statecharts engine (hierarchical composite states, orthogonal parallel
//! regions, guard/action expressions, shallow/deep history) and optional coupling
//! to the bounded-queue [`Simulation`].

use crate::model::Node;
use crate::render;
use crate::simulation::{QueueState, SimTraits, Simulation};
use crate::themes::ThemePalette;
use crate::{Diagram, Error};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

/// Runtime variable stored in a statechart's extended-state context.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContextValue {
    Bool(bool),
    Num(f64),
    Str(String),
}

impl ContextValue {
    fn as_bool(&self) -> bool {
        match self {
            Self::Bool(b) => *b,
            Self::Num(n) => *n != 0.0 && !n.is_nan(),
            Self::Str(s) => !s.is_empty() && s != "false" && s != "0",
        }
    }

    fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Num(n) => Some(*n),
            Self::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            Self::Str(s) => s.trim().parse::<f64>().ok(),
        }
    }

    fn as_string(&self) -> String {
        match self {
            Self::Bool(b) => b.to_string(),
            Self::Num(n) => {
                if (n - n.round()).abs() < 1e-9 {
                    format!("{:.0}", n.round())
                } else {
                    format!("{n:.3}")
                }
            }
            Self::Str(s) => s.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PseudostateKind {
    State,
    Initial,
    Final,
    ShallowHistory,
    DeepHistory,
}

#[derive(Debug, Clone)]
struct TransitionSpec {
    edge_id: String,
    source: String,
    target: String,
    events: Vec<String>,
    guard: Option<String>,
    action: Option<String>,
    order: usize,
}

#[derive(Debug, Clone)]
enum SimCommand {
    Sever(String, bool),
    SetRate(String, f64),
    SetWorkers(String, f64),
}

/// Summary of an outgoing transition reachable from the current state configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventInfo {
    pub name: String,
    pub source: String,
    pub target: String,
    pub enabled: bool,
    pub guard: Option<String>,
    pub action: Option<String>,
}

/// Immutable audit record for a fired statechart transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionRecord {
    pub edge_id: String,
    pub event: String,
    pub source: String,
    pub target: String,
    pub exited: Vec<String>,
    pub entered: Vec<String>,
    pub action: Option<String>,
    pub step: usize,
}

/// Serializable queue snapshot embedded inside [`StatechartSnapshot`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationSnapshot {
    pub time: f64,
    pub nodes: BTreeMap<String, QueueState>,
    pub severed: BTreeSet<String>,
    pub traits: BTreeMap<String, SimTraits>,
}

impl SimulationSnapshot {
    fn from_sim(sim: &Simulation) -> Self {
        Self {
            time: sim.time,
            nodes: sim.nodes.clone(),
            severed: sim.severed.clone(),
            traits: sim.traits.clone(),
        }
    }
}

/// Complete observable state of a running [`Statechart`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatechartSnapshot {
    pub active: Vec<String>,
    pub leaf_states: Vec<String>,
    pub shallow_history: BTreeMap<String, String>,
    pub deep_history: BTreeMap<String, Vec<String>>,
    pub context: BTreeMap<String, ContextValue>,
    pub available_events: Vec<String>,
    pub events: Vec<EventInfo>,
    pub history_log: Vec<TransitionRecord>,
    pub step: usize,
    pub last_edge: Option<String>,
    pub simulation: Option<SimulationSnapshot>,
}

/// Executable Harel statechart derived from a compiled [`Diagram`].
pub struct Statechart {
    diagram: Diagram,
    parent_of: BTreeMap<String, Option<String>>,
    children_of: BTreeMap<Option<String>, Vec<String>>,
    kinds: BTreeMap<String, PseudostateKind>,
    parallel_states: BTreeSet<String>,
    initial_targets: BTreeMap<Option<String>, Vec<String>>,
    history_defaults: BTreeMap<String, String>,
    transitions: Vec<TransitionSpec>,
    initial_actions: BTreeMap<String, String>,
    active: BTreeSet<String>,
    shallow_history: BTreeMap<String, String>,
    deep_history: BTreeMap<String, Vec<String>>,
    context: BTreeMap<String, ContextValue>,
    history_log: Vec<TransitionRecord>,
    last_edge: Option<String>,
    step_count: usize,
}

impl Statechart {
    /// Compile source text into a [`Statechart`] initialized to its root initial configuration.
    pub fn from_source(source: &str) -> Result<Self, Error> {
        let diagram = crate::compile(source)?;
        Ok(Self::from_diagram(diagram))
    }

    /// Construct a [`Statechart`] from an already compiled [`Diagram`].
    pub fn from_diagram(diagram: Diagram) -> Self {
        let mut parent_of = BTreeMap::new();
        let mut children_of: BTreeMap<Option<String>, Vec<String>> = BTreeMap::new();
        let mut kinds = BTreeMap::new();
        let mut parallel_annotated = BTreeSet::new();

        let mut ordered_nodes: Vec<&Node> = diagram.scene.nodes.iter().collect();
        ordered_nodes.sort_by_key(|n| n.declaration_index);

        for n in &ordered_nodes {
            parent_of.insert(n.id.clone(), n.parent.clone());
            children_of
                .entry(n.parent.clone())
                .or_default()
                .push(n.id.clone());
            let kind = match n.shape.as_str() {
                "initial" => PseudostateKind::Initial,
                "final" => PseudostateKind::Final,
                "history" => PseudostateKind::ShallowHistory,
                "deep_history" => PseudostateKind::DeepHistory,
                _ => PseudostateKind::State,
            };
            kinds.insert(n.id.clone(), kind);
            if has_flag(n, "region")
                && let Some(p) = &n.parent
            {
                parallel_annotated.insert(p.clone());
            }
            if has_flag(n, "parallel") {
                parallel_annotated.insert(n.id.clone());
            }
        }

        let mut initial_targets: BTreeMap<Option<String>, Vec<String>> = BTreeMap::new();
        let mut initial_actions = BTreeMap::new();
        let mut history_defaults = BTreeMap::new();
        let mut transitions = Vec::new();

        for (idx, edge) in diagram.scene.edges.iter().enumerate() {
            let src_kind = kinds
                .get(&edge.source.node)
                .copied()
                .unwrap_or(PseudostateKind::State);
            let spec = parse_transition(edge, idx);
            match src_kind {
                PseudostateKind::Initial => {
                    let scope = parent_of.get(&edge.source.node).cloned().unwrap_or(None);
                    initial_targets
                        .entry(scope)
                        .or_default()
                        .push(edge.target.node.clone());
                    if let Some(action) = spec.action {
                        initial_actions.insert(edge.source.node.clone(), action);
                    }
                }
                PseudostateKind::ShallowHistory | PseudostateKind::DeepHistory => {
                    history_defaults
                        .entry(edge.source.node.clone())
                        .or_insert_with(|| edge.target.node.clone());
                    if let Some(action) = spec.action {
                        initial_actions.insert(edge.source.node.clone(), action);
                    }
                }
                PseudostateKind::State | PseudostateKind::Final => {
                    transitions.push(spec);
                }
            }
        }

        // Fill default initial targets for every scope (root and composite states).
        let scopes: Vec<Option<String>> = std::iter::once(None)
            .chain(
                ordered_nodes
                    .iter()
                    .filter(|n| {
                        n.container
                            || children_of.get(&Some(n.id.clone())).is_some_and(|c| {
                                c.iter().any(|cid| {
                                    matches!(
                                        kinds.get(cid),
                                        Some(PseudostateKind::State | PseudostateKind::Final)
                                    )
                                })
                            })
                    })
                    .map(|n| Some(n.id.clone())),
            )
            .collect();

        for scope in scopes {
            let is_parallel = scope
                .as_ref()
                .is_some_and(|id| parallel_annotated.contains(id));
            if is_parallel {
                let regions: Vec<String> = children_of
                    .get(&scope)
                    .into_iter()
                    .flatten()
                    .filter(|cid| {
                        matches!(
                            kinds.get(*cid),
                            Some(PseudostateKind::State | PseudostateKind::Final)
                        )
                    })
                    .cloned()
                    .collect();
                if !regions.is_empty() {
                    initial_targets.insert(scope, regions);
                    continue;
                }
            }
            if initial_targets.contains_key(&scope) {
                continue;
            }
            if scope.is_none()
                && let Some(children) = children_of.get(&None)
            {
                let mut picked_initial = Vec::new();
                for cid in children {
                    if let Some(node) = diagram.scene.node(cid)
                        && has_flag(node, "initial")
                        && matches!(
                            kinds.get(cid),
                            Some(PseudostateKind::State | PseudostateKind::Final)
                        )
                    {
                        picked_initial.push(cid.clone());
                    }
                }
                if picked_initial.is_empty() {
                    let has_incoming_root = |cid: &str| {
                        transitions
                            .iter()
                            .any(|t| t.target == cid && parent_of.get(&t.source) == Some(&None))
                    };
                    let machines: Vec<String> = children
                        .iter()
                        .filter(|cid| {
                            !has_incoming_root(cid)
                                && children_of.get(&Some((*cid).clone())).is_some_and(|sub| {
                                    sub.iter()
                                        .any(|s| kinds.get(s) == Some(&PseudostateKind::Initial))
                                        || !diagram.scene.node(cid).is_some_and(|n| {
                                            n.annotations.iter().any(|a| a.name == "sim")
                                        })
                                })
                                && diagram.scene.node(cid).is_some_and(|n| n.container)
                        })
                        .cloned()
                        .collect();
                    if !machines.is_empty() {
                        initial_targets.insert(None, machines);
                        continue;
                    }
                } else {
                    initial_targets.insert(None, picked_initial);
                    continue;
                }
            }
            if let Some(children) = children_of.get(&scope) {
                let mut picked = None;
                for cid in children {
                    if let Some(node) = diagram.scene.node(cid)
                        && has_flag(node, "initial")
                        && matches!(
                            kinds.get(cid),
                            Some(PseudostateKind::State | PseudostateKind::Final)
                        )
                    {
                        picked = Some(cid.clone());
                        break;
                    }
                }
                if picked.is_none() {
                    for cid in children {
                        if kinds.get(cid) == Some(&PseudostateKind::State) {
                            picked = Some(cid.clone());
                            break;
                        }
                    }
                }
                if picked.is_none() {
                    for cid in children {
                        if kinds.get(cid) == Some(&PseudostateKind::Final) {
                            picked = Some(cid.clone());
                            break;
                        }
                    }
                }
                if let Some(target) = picked {
                    initial_targets.insert(scope, vec![target]);
                }
            }
        }

        let mut chart = Self {
            diagram,
            parent_of,
            children_of,
            kinds,
            parallel_states: parallel_annotated,
            initial_targets,
            history_defaults,
            transitions,
            initial_actions,
            active: BTreeSet::new(),
            shallow_history: BTreeMap::new(),
            deep_history: BTreeMap::new(),
            context: BTreeMap::new(),
            history_log: Vec::new(),
            last_edge: None,
            step_count: 0,
        };
        chart.reset();
        chart
    }

    /// Borrow the underlying compiled [`Diagram`].
    pub fn diagram(&self) -> &Diagram {
        &self.diagram
    }

    /// Currently active states (includes active composite ancestors as well as leaves).
    pub fn active_states(&self) -> &BTreeSet<String> {
        &self.active
    }

    /// Active leaf states ordered deterministically by declaration order.
    pub fn leaf_states(&self) -> Vec<String> {
        let mut leaves: Vec<String> = self
            .active
            .iter()
            .filter(|id| {
                !self
                    .children_of
                    .get(&Some((*id).clone()))
                    .is_some_and(|children| children.iter().any(|c| self.active.contains(c)))
            })
            .cloned()
            .collect();
        leaves.sort_by_key(|id| {
            self.diagram
                .scene
                .node(id)
                .map(|n| n.declaration_index)
                .unwrap_or(usize::MAX)
        });
        leaves
    }

    /// Reset context, clear recorded history, and enter initial state(s).
    pub fn reset(&mut self) {
        self.active.clear();
        self.shallow_history.clear();
        self.deep_history.clear();
        self.context.clear();
        self.history_log.clear();
        self.last_edge = None;
        self.step_count = 0;

        // Seed context variables declared via `[context: "k=v; ..."]` or `[init: "..."]`.
        let mut nodes: Vec<Node> = self.diagram.scene.nodes.clone();
        nodes.sort_by_key(|n| n.declaration_index);
        for n in &nodes {
            if let Some(raw) = attr_text(n, "context").or_else(|| attr_text(n, "init")) {
                let _ = self.exec_actions(&raw, None);
            }
        }

        let mut pending_sim = Vec::new();
        let mut internal_events = Vec::new();
        let targets = self.initial_targets.get(&None).cloned().unwrap_or_default();
        for target in targets {
            let mut entered = Vec::new();
            self.enter_branch(
                &target,
                None,
                &mut entered,
                &mut pending_sim,
                &mut internal_events,
                None,
            );
        }
        // Fire initial-edge actions on any initial pseudostates at root scope.
        for n in &nodes {
            if n.parent.is_none()
                && self.kinds.get(&n.id) == Some(&PseudostateKind::Initial)
                && let Some(action) = self.initial_actions.get(&n.id).cloned()
            {
                let (cmds, evs) = self.exec_actions(&action, None);
                pending_sim.extend(cmds);
                internal_events.extend(evs);
            }
        }
        let _ = self.drain_microsteps(None, internal_events);
    }

    /// Assign a variable inside the statechart context.
    pub fn set_context(&mut self, key: impl Into<String>, value: ContextValue) {
        self.context.insert(key.into(), value);
    }

    /// Merge JSON object keys into the statechart context.
    pub fn set_context_json(&mut self, json: &str) -> Result<(), Error> {
        let map: BTreeMap<String, ContextValue> =
            serde_json::from_str(json).map_err(|e| Error::Layout(e.to_string()))?;
        for (k, v) in map {
            self.context.insert(k, v);
        }
        Ok(())
    }

    /// Read a copy of the statechart context variables.
    pub fn context(&self) -> &BTreeMap<String, ContextValue> {
        &self.context
    }

    /// Inspect all outgoing named transitions from currently active states.
    pub fn inspect_events(&self, sim: Option<&Simulation>) -> Vec<EventInfo> {
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for t in &self.transitions {
            if !self.active.contains(&t.source) {
                continue;
            }
            let enabled = t.guard.as_deref().is_none_or(|g| self.eval_guard(g, sim));
            for ev in &t.events {
                if seen.insert((ev.clone(), t.source.clone(), t.target.clone())) {
                    out.push(EventInfo {
                        name: ev.clone(),
                        source: t.source.clone(),
                        target: t.target.clone(),
                        enabled,
                        guard: t.guard.clone(),
                        action: t.action.clone(),
                    });
                }
            }
        }
        out
    }

    /// Unique enabled event names triggerable from the current active states.
    pub fn available_events(&self) -> Vec<String> {
        self.available_events_with_sim(None)
    }

    /// Unique enabled event names triggerable from active states under optional simulation telemetry.
    pub fn available_events_with_sim(&self, sim: Option<&Simulation>) -> Vec<String> {
        let mut names = Vec::new();
        for info in self.inspect_events(sim) {
            if info.enabled && !names.contains(&info.name) {
                names.push(info.name);
            }
        }
        names
    }

    /// Evaluate a guard expression against current statechart context & optional queue telemetry.
    pub fn eval_guard(&self, expr: &str, sim: Option<&Simulation>) -> bool {
        let trimmed = expr.trim();
        if trimmed.is_empty() {
            return true;
        }
        let tokens = tokenize_expr(trimmed);
        let mut p = ExprParser {
            tokens: &tokens,
            pos: 0,
            ctx: &self.context,
            active: &self.active,
            sim,
        };
        p.parse_or().is_some_and(|v| v.as_bool())
    }

    /// Dispatch a named event without queue simulation coupling.
    pub fn dispatch(&mut self, event: &str) -> Result<bool, Error> {
        self.dispatch_internal(Some(event.trim()), None)
    }

    /// Dispatch a named event while sharing telemetry and action mutations with [`Simulation`].
    pub fn dispatch_with_simulation(
        &mut self,
        event: &str,
        sim: &mut Simulation,
    ) -> Result<bool, Error> {
        self.dispatch_internal(Some(event.trim()), Some(sim))
    }

    /// Advance a coupled queue simulation by `ticks`, syncing telemetry into guard evaluation
    /// and firing any enabled eventless/guarded or completion transitions (e.g. circuit breakers).
    pub fn step_coupled(&mut self, sim: &mut Simulation, ticks: usize) -> Result<bool, Error> {
        let mut any_fired = false;
        for _ in 0..ticks.max(1) {
            sim.tick()?;
            if self.dispatch_internal(None, Some(sim))? {
                any_fired = true;
            }
        }
        Ok(any_fired)
    }

    fn dispatch_internal(
        &mut self,
        event: Option<&str>,
        mut sim: Option<&mut Simulation>,
    ) -> Result<bool, Error> {
        let sim_ref = sim.as_deref();
        let chosen = self.select_transitions(event, sim_ref);
        if chosen.is_empty() {
            return Ok(false);
        }

        let mut internal_events = Vec::new();
        for spec in chosen {
            let ev_label = event.unwrap_or("[auto]").to_string();
            let cmds =
                self.execute_transition(&spec, &ev_label, sim.as_deref(), &mut internal_events);
            if let Some(sim_mut) = sim.as_deref_mut() {
                self.apply_sim_commands(sim_mut, &cmds)?;
            }
        }

        let extra_cmds = self.drain_microsteps(sim.as_deref(), internal_events);
        if let Some(sim_mut) = sim {
            self.apply_sim_commands(sim_mut, &extra_cmds)?;
        }
        Ok(true)
    }

    fn drain_microsteps(
        &mut self,
        sim: Option<&Simulation>,
        mut queue: Vec<String>,
    ) -> Vec<SimCommand> {
        let mut all_cmds = Vec::new();
        for _ in 0..16 {
            let next_ev = if queue.is_empty() {
                None
            } else {
                Some(queue.remove(0))
            };
            let chosen = self.select_transitions(next_ev.as_deref(), sim);
            if chosen.is_empty() {
                if next_ev.is_none() {
                    break;
                }
                continue;
            }
            for spec in chosen {
                let ev_label = next_ev.as_deref().unwrap_or("[auto]").to_string();
                let mut raised = Vec::new();
                let cmds = self.execute_transition(&spec, &ev_label, sim, &mut raised);
                all_cmds.extend(cmds);
                queue.extend(raised);
            }
        }
        all_cmds
    }

    fn select_transitions(
        &self,
        event: Option<&str>,
        sim: Option<&Simulation>,
    ) -> Vec<TransitionSpec> {
        let mut candidates: Vec<&TransitionSpec> = self
            .transitions
            .iter()
            .filter(|t| {
                if !self.active.contains(&t.source) {
                    return false;
                }
                let ev_matches = match event {
                    Some(name) => t.events.iter().any(|e| e == name),
                    None => {
                        t.events.is_empty()
                            && (t.guard.is_some()
                                || self.kinds.get(&t.source) == Some(&PseudostateKind::Final))
                    }
                };
                ev_matches && t.guard.as_deref().is_none_or(|g| self.eval_guard(g, sim))
            })
            .collect();

        // Harel priority: deeper active descendant source beats ancestor source.
        candidates.sort_by(|a, b| {
            let da = self.depth_of(&a.source);
            let db = self.depth_of(&b.source);
            db.cmp(&da).then_with(|| a.order.cmp(&b.order))
        });

        let mut selected: Vec<TransitionSpec> = Vec::new();
        for cand in candidates {
            let conflicts = selected.iter().any(|prev| {
                self.is_ancestor_or_same(&prev.source, &cand.source)
                    || self.is_ancestor_or_same(&cand.source, &prev.source)
            });
            if !conflicts {
                selected.push(cand.clone());
            }
        }
        selected
    }

    fn execute_transition(
        &mut self,
        spec: &TransitionSpec,
        event_label: &str,
        sim: Option<&Simulation>,
        internal_events: &mut Vec<String>,
    ) -> Vec<SimCommand> {
        let mut cmds = Vec::new();
        let resolved_target = self.resolve_effective_target(&spec.target);
        let lca = if spec.source == resolved_target {
            self.parent_of.get(&spec.source).cloned().unwrap_or(None)
        } else {
            self.lowest_common_strict_ancestor(&spec.source, &resolved_target)
        };

        // 1. Identify all active states strictly below LCA to exit, deepest first.
        let mut to_exit: Vec<String> = self
            .active
            .iter()
            .filter(|id| match &lca {
                Some(anc) => *id != anc && self.is_strict_descendant(id, anc),
                None => true,
            })
            .filter(|id| {
                // Exit only branches intersecting source's exit scope below LCA.
                let src_branch = self.child_directly_under(&spec.source, lca.as_deref());
                src_branch
                    .as_ref()
                    .is_some_and(|root| *id == root || self.is_strict_descendant(id, root))
            })
            .cloned()
            .collect();
        to_exit.sort_by(|a, b| {
            self.depth_of(b)
                .cmp(&self.depth_of(a))
                .then_with(|| a.cmp(b))
        });

        // 2. Snapshot shallow & deep history before clearing exited active states.
        for state_id in &to_exit {
            let direct_children = self
                .children_of
                .get(&Some(state_id.clone()))
                .cloned()
                .unwrap_or_default();
            if !direct_children.is_empty()
                && !self.parallel_states.contains(state_id)
                && let Some(active_child) = direct_children.iter().find(|cid| {
                    self.active.contains(*cid)
                        && matches!(
                            self.kinds.get(*cid),
                            Some(PseudostateKind::State | PseudostateKind::Final)
                        )
                })
            {
                self.shallow_history
                    .insert(state_id.clone(), active_child.clone());
            }
            if !direct_children.is_empty() {
                let mut saved: Vec<String> = self
                    .active
                    .iter()
                    .filter(|id| self.is_strict_descendant(id, state_id))
                    .cloned()
                    .collect();
                saved.sort_by_key(|id| (self.depth_of(id), id.clone()));
                if !saved.is_empty() {
                    self.deep_history.insert(state_id.clone(), saved);
                }
            }
        }

        // 3. Exit states and run [exit] actions.
        for state_id in &to_exit {
            if let Some(exit_action) = self
                .diagram
                .scene
                .node(state_id)
                .and_then(|n| attr_text(n, "exit"))
            {
                let (c, e) = self.exec_actions(&exit_action, sim);
                cmds.extend(c);
                internal_events.extend(e);
            }
            self.active.remove(state_id);
        }

        // 4. Transition action.
        if let Some(action) = &spec.action {
            let (c, e) = self.exec_actions(action, sim);
            cmds.extend(c);
            internal_events.extend(e);
        }

        // 5. Enter target (resolving history/deep_history & orthogonal siblings).
        let mut entered = Vec::new();
        self.enter_target(
            &spec.target,
            lca.as_deref(),
            &mut entered,
            &mut cmds,
            internal_events,
            sim,
        );

        self.step_count += 1;
        self.last_edge = Some(spec.edge_id.clone());
        self.history_log.push(TransitionRecord {
            edge_id: spec.edge_id.clone(),
            event: event_label.to_string(),
            source: spec.source.clone(),
            target: resolved_target,
            exited: to_exit,
            entered,
            action: spec.action.clone(),
            step: self.step_count,
        });
        cmds
    }

    fn resolve_effective_target(&self, target: &str) -> String {
        match self
            .kinds
            .get(target)
            .copied()
            .unwrap_or(PseudostateKind::State)
        {
            PseudostateKind::ShallowHistory | PseudostateKind::DeepHistory => self
                .parent_of
                .get(target)
                .cloned()
                .flatten()
                .unwrap_or_else(|| target.to_string()),
            _ => target.to_string(),
        }
    }

    fn enter_target(
        &mut self,
        target: &str,
        lca: Option<&str>,
        entered: &mut Vec<String>,
        cmds: &mut Vec<SimCommand>,
        internal_events: &mut Vec<String>,
        sim: Option<&Simulation>,
    ) {
        let kind = self
            .kinds
            .get(target)
            .copied()
            .unwrap_or(PseudostateKind::State);
        match kind {
            PseudostateKind::ShallowHistory => {
                if let Some(action) = self.initial_actions.get(target).cloned() {
                    let (c, e) = self.exec_actions(&action, sim);
                    cmds.extend(c);
                    internal_events.extend(e);
                }
                let parent = self.parent_of.get(target).cloned().flatten();
                if let Some(parent_id) = parent {
                    if let Some(remembered) = self.shallow_history.get(&parent_id).cloned() {
                        self.enter_branch(&remembered, lca, entered, cmds, internal_events, sim);
                    } else if let Some(default_t) = self.history_defaults.get(target).cloned() {
                        self.enter_branch(&default_t, lca, entered, cmds, internal_events, sim);
                    } else {
                        self.enter_branch(&parent_id, lca, entered, cmds, internal_events, sim);
                    }
                }
            }
            PseudostateKind::DeepHistory => {
                if let Some(action) = self.initial_actions.get(target).cloned() {
                    let (c, e) = self.exec_actions(&action, sim);
                    cmds.extend(c);
                    internal_events.extend(e);
                }
                let parent = self.parent_of.get(target).cloned().flatten();
                if let Some(parent_id) = parent {
                    self.activate_chain_to(&parent_id, lca, entered, cmds, internal_events, sim);
                    if let Some(saved) = self.deep_history.get(&parent_id).cloned()
                        && !saved.is_empty()
                    {
                        for id in saved {
                            self.activate_single(&id, entered, cmds, internal_events, sim);
                        }
                        self.complete_missing_regions(entered, cmds, internal_events, sim);
                    } else if let Some(default_t) = self.history_defaults.get(target).cloned() {
                        self.enter_branch(
                            &default_t,
                            Some(&parent_id),
                            entered,
                            cmds,
                            internal_events,
                            sim,
                        );
                    } else {
                        let targets = self
                            .initial_targets
                            .get(&Some(parent_id.clone()))
                            .cloned()
                            .unwrap_or_default();
                        for child_t in targets {
                            self.enter_branch(
                                &child_t,
                                Some(&parent_id),
                                entered,
                                cmds,
                                internal_events,
                                sim,
                            );
                        }
                    }
                }
            }
            _ => {
                self.enter_branch(target, lca, entered, cmds, internal_events, sim);
            }
        }
    }

    fn enter_branch(
        &mut self,
        target: &str,
        lca: Option<&str>,
        entered: &mut Vec<String>,
        cmds: &mut Vec<SimCommand>,
        internal_events: &mut Vec<String>,
        sim: Option<&Simulation>,
    ) {
        self.activate_chain_to(target, lca, entered, cmds, internal_events, sim);

        // Descend into initial child / orthogonal regions if target is composite.
        let targets = self
            .initial_targets
            .get(&Some(target.to_string()))
            .cloned()
            .unwrap_or_default();
        if !targets.is_empty() {
            // Execute any initial pseudostate action inside target.
            if let Some(children) = self.children_of.get(&Some(target.to_string())).cloned() {
                for cid in children {
                    if self.kinds.get(&cid) == Some(&PseudostateKind::Initial)
                        && let Some(action) = self.initial_actions.get(&cid).cloned()
                    {
                        let (c, e) = self.exec_actions(&action, sim);
                        cmds.extend(c);
                        internal_events.extend(e);
                    }
                }
            }
            for child_t in targets {
                self.enter_branch(&child_t, Some(target), entered, cmds, internal_events, sim);
            }
        }
        self.complete_missing_regions(entered, cmds, internal_events, sim);
    }

    fn activate_chain_to(
        &mut self,
        target: &str,
        lca: Option<&str>,
        entered: &mut Vec<String>,
        cmds: &mut Vec<SimCommand>,
        internal_events: &mut Vec<String>,
        sim: Option<&Simulation>,
    ) {
        let mut chain = self.ancestors_inclusive(target);
        chain.reverse(); // root -> ... -> target
        let start_idx = match lca {
            Some(anc) => chain
                .iter()
                .position(|id| id == anc)
                .map(|i| i + 1)
                .unwrap_or(0),
            None => 0,
        };
        for state_id in chain.into_iter().skip(start_idx) {
            self.activate_single(&state_id, entered, cmds, internal_events, sim);
        }
    }

    fn complete_missing_regions(
        &mut self,
        entered: &mut Vec<String>,
        cmds: &mut Vec<SimCommand>,
        internal_events: &mut Vec<String>,
        sim: Option<&Simulation>,
    ) {
        // Ensure every active parallel state has ALL of its child regions activated.
        let active_parallels: Vec<String> = self
            .active
            .iter()
            .filter(|id| self.parallel_states.contains(*id))
            .cloned()
            .collect();
        for p in active_parallels {
            let regions = self
                .children_of
                .get(&Some(p.clone()))
                .cloned()
                .unwrap_or_default();
            for r in regions {
                if matches!(
                    self.kinds.get(&r),
                    Some(PseudostateKind::State | PseudostateKind::Final)
                ) && !self.active.contains(&r)
                {
                    self.enter_branch(&r, Some(&p), entered, cmds, internal_events, sim);
                }
            }
        }
    }

    fn activate_single(
        &mut self,
        state_id: &str,
        entered: &mut Vec<String>,
        cmds: &mut Vec<SimCommand>,
        internal_events: &mut Vec<String>,
        sim: Option<&Simulation>,
    ) {
        if self.active.insert(state_id.to_string()) {
            entered.push(state_id.to_string());
            if let Some(entry_action) = self
                .diagram
                .scene
                .node(state_id)
                .and_then(|n| attr_text(n, "entry"))
            {
                let (c, e) = self.exec_actions(&entry_action, sim);
                cmds.extend(c);
                internal_events.extend(e);
            }
        }
    }

    fn exec_actions(
        &mut self,
        script: &str,
        sim: Option<&Simulation>,
    ) -> (Vec<SimCommand>, Vec<String>) {
        let mut cmds = Vec::new();
        let mut events = Vec::new();
        for stmt in split_top_level(script, ';') {
            let s = stmt.trim();
            if s.is_empty() {
                continue;
            }
            if let Some(prefix) = s.strip_suffix("++") {
                let var = prefix.trim().to_string();
                let cur = self
                    .context
                    .get(&var)
                    .and_then(ContextValue::as_f64)
                    .unwrap_or(0.0);
                self.context.insert(var, ContextValue::Num(cur + 1.0));
                continue;
            }
            if let Some(prefix) = s.strip_suffix("--") {
                let var = prefix.trim().to_string();
                let cur = self
                    .context
                    .get(&var)
                    .and_then(ContextValue::as_f64)
                    .unwrap_or(0.0);
                self.context.insert(var, ContextValue::Num(cur - 1.0));
                continue;
            }
            if let Some((call, args_raw)) = parse_call(s) {
                let args = split_top_level(args_raw, ',');
                match call {
                    "sever" if !args.is_empty() => {
                        let edge = self.resolve_edge_ref(&args);
                        if let Some(id) = edge {
                            cmds.push(SimCommand::Sever(id, true));
                        }
                    }
                    "restore" if !args.is_empty() => {
                        let edge = self.resolve_edge_ref(&args);
                        if let Some(id) = edge {
                            cmds.push(SimCommand::Sever(id, false));
                        }
                    }
                    "set_rate" if args.len() >= 2 => {
                        let node = self.resolve_node_ref(unquote_arg(&args[0]));
                        if let Some(v) = self.eval_expr_val(&args[1], sim).and_then(|v| v.as_f64())
                        {
                            cmds.push(SimCommand::SetRate(node, v));
                        }
                    }
                    "set_workers" | "scale" if args.len() >= 2 => {
                        let node = self.resolve_node_ref(unquote_arg(&args[0]));
                        if let Some(v) = self.eval_expr_val(&args[1], sim).and_then(|v| v.as_f64())
                        {
                            cmds.push(SimCommand::SetWorkers(node, v));
                        }
                    }
                    "emit" | "raise" if !args.is_empty() => {
                        events.push(unquote_arg(&args[0]).to_string());
                    }
                    _ => {}
                }
                continue;
            }
            if let Some((lhs, rhs)) = split_assign(s, "+=") {
                if let Some(v) = self.eval_expr_val(rhs, sim).and_then(|v| v.as_f64()) {
                    let cur = self
                        .context
                        .get(lhs)
                        .and_then(ContextValue::as_f64)
                        .unwrap_or(0.0);
                    self.context
                        .insert(lhs.to_string(), ContextValue::Num(cur + v));
                }
            } else if let Some((lhs, rhs)) = split_assign(s, "-=") {
                if let Some(v) = self.eval_expr_val(rhs, sim).and_then(|v| v.as_f64()) {
                    let cur = self
                        .context
                        .get(lhs)
                        .and_then(ContextValue::as_f64)
                        .unwrap_or(0.0);
                    self.context
                        .insert(lhs.to_string(), ContextValue::Num(cur - v));
                }
            } else if let Some((lhs, rhs)) = split_assign(s, "=")
                && let Some(val) = self.eval_expr_val(rhs, sim)
            {
                self.context.insert(lhs.to_string(), val);
            }
        }
        (cmds, events)
    }

    fn apply_sim_commands(&self, sim: &mut Simulation, cmds: &[SimCommand]) -> Result<(), Error> {
        for cmd in cmds {
            match cmd {
                SimCommand::Sever(edge, severed) => {
                    if sim.sever(edge, *severed).is_err() {
                        // Ignore unknown optional edge refs cleanly if user specified label.
                    }
                }
                SimCommand::SetRate(node, rate) => {
                    if sim.set_rate(node, *rate).is_err() {
                        // Ignore unknown node refs cleanly.
                    }
                }
                SimCommand::SetWorkers(node, workers) => {
                    if let Some(t) = sim.traits.get_mut(node) {
                        t.workers = workers.max(0.0);
                    }
                }
            }
        }
        Ok(())
    }

    fn eval_expr_val(&self, expr: &str, sim: Option<&Simulation>) -> Option<ContextValue> {
        let tokens = tokenize_expr(expr.trim());
        let mut p = ExprParser {
            tokens: &tokens,
            pos: 0,
            ctx: &self.context,
            active: &self.active,
            sim,
        };
        p.parse_or()
    }

    fn resolve_node_ref(&self, name: &str) -> String {
        if self.diagram.scene.node(name).is_some() {
            return name.to_string();
        }
        self.diagram
            .scene
            .nodes
            .iter()
            .find(|n| n.id.rsplit('.').next() == Some(name))
            .map(|n| n.id.clone())
            .unwrap_or_else(|| name.to_string())
    }

    fn resolve_edge_ref(&self, args: &[String]) -> Option<String> {
        let raw0 = unquote_arg(&args[0]);
        if args.len() == 1 {
            if self.diagram.scene.edge(raw0).is_some() {
                return Some(raw0.to_string());
            }
            if let Some((src, dst)) = raw0.split_once("->") {
                return self.find_edge_between(src.trim(), dst.trim());
            }
            return self
                .diagram
                .scene
                .edges
                .iter()
                .find(|e| e.label == raw0)
                .map(|e| e.id.clone());
        }
        self.find_edge_between(unquote_arg(&args[0]), unquote_arg(&args[1]))
    }

    fn find_edge_between(&self, src: &str, dst: &str) -> Option<String> {
        self.diagram
            .scene
            .edges
            .iter()
            .find(|e| {
                (e.source.node == src || e.source.node.rsplit('.').next() == Some(src))
                    && (e.target.node == dst || e.target.node.rsplit('.').next() == Some(dst))
            })
            .map(|e| e.id.clone())
    }

    fn depth_of(&self, id: &str) -> usize {
        let mut d = 0;
        let mut cur = id;
        while let Some(Some(p)) = self.parent_of.get(cur) {
            d += 1;
            cur = p;
        }
        d
    }

    fn ancestors_inclusive(&self, id: &str) -> Vec<String> {
        let mut chain = vec![id.to_string()];
        let mut cur = id;
        while let Some(Some(p)) = self.parent_of.get(cur) {
            chain.push(p.clone());
            cur = p;
        }
        chain
    }

    fn is_ancestor_or_same(&self, anc: &str, node: &str) -> bool {
        self.ancestors_inclusive(node).iter().any(|x| x == anc)
    }

    fn is_strict_descendant(&self, node: &str, anc: &str) -> bool {
        node != anc && self.is_ancestor_or_same(anc, node)
    }

    fn lowest_common_strict_ancestor(&self, a: &str, b: &str) -> Option<String> {
        let a_ancs = self.ancestors_inclusive(a);
        let b_ancs = self.ancestors_inclusive(b);
        a_ancs
            .into_iter()
            .skip(1)
            .find(|candidate| b_ancs.iter().skip(1).any(|x| x == candidate))
    }

    fn child_directly_under(&self, node: &str, anc: Option<&str>) -> Option<String> {
        let mut cur = node;
        loop {
            match self.parent_of.get(cur) {
                Some(Some(p)) => {
                    if anc.is_some_and(|a| p == a) {
                        return Some(cur.to_string());
                    }
                    cur = p;
                }
                Some(None) | None => {
                    if anc.is_none() {
                        return Some(cur.to_string());
                    }
                    return None;
                }
            }
        }
    }

    /// Capture complete observable snapshot of active configuration, history, context, and events.
    pub fn snapshot(&self, sim: Option<&Simulation>) -> StatechartSnapshot {
        StatechartSnapshot {
            active: self.active.iter().cloned().collect(),
            leaf_states: self.leaf_states(),
            shallow_history: self.shallow_history.clone(),
            deep_history: self.deep_history.clone(),
            context: self.context.clone(),
            available_events: self.available_events_with_sim(sim),
            events: self.inspect_events(sim),
            history_log: self.history_log.clone(),
            step: self.step_count,
            last_edge: self.last_edge.clone(),
            simulation: sim.map(SimulationSnapshot::from_sim),
        }
    }

    /// Render the statechart with active states highlighted using the default palette.
    pub fn to_svg(&self) -> String {
        self.to_svg_with_simulation(None)
    }

    /// Render the statechart with active state highlights and optional live queue gauges.
    pub fn to_svg_with_simulation(&self, sim: Option<&Simulation>) -> String {
        let mut svg = render::svg_with_decorations(
            &self.diagram.scene,
            &self.diagram.layout,
            |_| true,
            |n, r| self.decorate_node(n, r, sim),
        );
        self.inject_edge_overlays(&mut svg, sim);
        svg
    }

    /// Render the statechart using an explicit [`ThemePalette`].
    pub fn to_svg_with_theme(&self, theme: &ThemePalette, sim: Option<&Simulation>) -> String {
        let mut svg = render::svg_themed_with_decorations(
            &self.diagram.scene,
            &self.diagram.layout,
            Some(theme),
            |_| true,
            |n, r| self.decorate_node(n, r, sim),
        );
        self.inject_edge_overlays(&mut svg, sim);
        svg
    }

    fn decorate_node(&self, n: &Node, r: crate::layout::Rect, sim: Option<&Simulation>) -> String {
        let mut out = String::new();
        if self.active.contains(&n.id) {
            if n.container {
                let dash = if has_flag(n, "region") || has_flag(n, "parallel") {
                    " stroke-dasharray=\"6 4\""
                } else {
                    ""
                };
                let _ = write!(
                    out,
                    "<rect class=\"sc-active-container\" data-state-active=\"true\" x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"12\" fill=\"none\" stroke=\"var(--sd-status,#16a34a)\" stroke-width=\"2.5\"{dash}/><circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"4.5\" fill=\"var(--sd-status,#16a34a)\"/>",
                    r.left(),
                    r.top(),
                    r.w,
                    r.h,
                    r.right() - 12.,
                    r.top() + 12.
                );
            } else if n.shape == "final" {
                let rad = (r.w.min(r.h) / 2. + 3.).max(10.);
                let _ = write!(
                    out,
                    "<circle class=\"sc-active-state\" data-state-active=\"true\" cx=\"{:.2}\" cy=\"{:.2}\" r=\"{:.2}\" fill=\"none\" stroke=\"var(--sd-status,#16a34a)\" stroke-width=\"2.5\"/>",
                    r.x, r.y, rad
                );
            } else {
                let _ = write!(
                    out,
                    "<rect class=\"sc-active-state\" data-state-active=\"true\" x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"10\" fill=\"var(--sd-status,#16a34a)\" fill-opacity=\"0.12\" stroke=\"var(--sd-status,#16a34a)\" stroke-width=\"3\"/><circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"4\" fill=\"var(--sd-status,#16a34a)\"/>",
                    r.left(),
                    r.top(),
                    r.w,
                    r.h,
                    r.right() - 10.,
                    r.top() + 10.
                );
            }
        }
        if let Some(sim) = sim {
            append_queue_decoration(&mut out, n, r, sim);
        }
        out
    }

    fn inject_edge_overlays(&self, svg: &mut String, sim: Option<&Simulation>) {
        let mut extra = String::new();
        if let Some(edge_id) = &self.last_edge
            && let Some(route) = self.diagram.layout.routes.get(edge_id)
            && route.points.len() >= 2
        {
            let _ = write!(
                extra,
                "<path class=\"sc-last-transition\" data-transition-active=\"{}\" d=\"{}\" fill=\"none\" stroke=\"var(--sd-status,#16a34a)\" stroke-width=\"3.2\" stroke-opacity=\"0.78\" pointer-events=\"none\"/>",
                render::escape(edge_id),
                render::path(&route.points)
            );
        }
        if let Some(sim) = sim {
            append_severed_markers(&mut extra, &self.diagram.layout, sim);
        }
        if !extra.is_empty()
            && let Some(pos) = svg.rfind("</svg>")
        {
            svg.insert_str(pos, &format!("<g class=\"sc-overlay\">{extra}</g>"));
        }
    }
}

/// Render a standalone queue [`Simulation`] frame directly into SVG (for headless GIF/APNG export).
pub fn simulation_frame_svg(diagram: &Diagram, sim: &Simulation) -> String {
    let mut svg = render::svg_with_decorations(
        &diagram.scene,
        &diagram.layout,
        |_| true,
        |n, r| {
            let mut s = String::new();
            append_queue_decoration(&mut s, n, r, sim);
            s
        },
    );
    let mut extra = String::new();
    append_severed_markers(&mut extra, &diagram.layout, sim);
    if !extra.is_empty()
        && let Some(pos) = svg.rfind("</svg>")
    {
        svg.insert_str(pos, &format!("<g class=\"sim-overlay\">{extra}</g>"));
    }
    svg
}

fn append_queue_decoration(out: &mut String, n: &Node, r: crate::layout::Rect, sim: &Simulation) {
    if n.container {
        return;
    }
    let Some(q) = sim.nodes.get(&n.id) else {
        return;
    };
    let has_traffic = q.arrival_rate > 0.0
        || q.queued > 0.0
        || q.dropped > 0.0
        || sim.traits.get(&n.id).is_some_and(|t| t.rate > 0.0);
    if !has_traffic {
        return;
    }
    let sat = q.saturation.clamp(0.0, 1.0);
    let color = if sat >= 0.90 || q.dropped > 0.05 {
        "var(--sd-overload,#dc2626)"
    } else if sat >= 0.65 {
        "#d97706"
    } else {
        "var(--sd-status,#16a34a)"
    };
    let bar_w = (r.w - 16.0).max(16.0);
    let fill_w = (bar_w * sat).max(2.0);
    let bx = r.left() + 8.0;
    let by = r.bottom() - 8.0;
    let _ = write!(
        out,
        "<rect x=\"{bx:.2}\" y=\"{by:.2}\" width=\"{bar_w:.2}\" height=\"4\" rx=\"2\" fill=\"var(--sd-container,#e2e8f0)\"/><rect x=\"{bx:.2}\" y=\"{by:.2}\" width=\"{fill_w:.2}\" height=\"4\" rx=\"2\" fill=\"{color}\"/>"
    );
    if sat >= 0.90 || q.dropped > 0.05 {
        let _ = write!(
            out,
            "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"8\" fill=\"none\" stroke=\"var(--sd-overload,#dc2626)\" stroke-width=\"2\"/>",
            r.left(),
            r.top(),
            r.w,
            r.h
        );
    }
}

fn append_severed_markers(out: &mut String, layout: &crate::layout::Layout, sim: &Simulation) {
    for edge_id in &sim.severed {
        if let Some(route) = layout.routes.get(edge_id)
            && route.points.len() >= 2
        {
            let mid = render::at_arc(&route.points, 0.5);
            let _ = write!(
                out,
                "<g transform=\"translate({:.2},{:.2})\"><circle r=\"8\" fill=\"var(--sd-surface,#ffffff)\" stroke=\"var(--sd-cut,#dc2626)\" stroke-width=\"2\"/><path d=\"M -3.5 -3.5 L 3.5 3.5 M 3.5 -3.5 L -3.5 3.5\" stroke=\"var(--sd-cut,#dc2626)\" stroke-width=\"2\" stroke-linecap=\"round\"/></g>",
                mid.x, mid.y
            );
        }
    }
}

fn has_flag(node: &Node, name: &str) -> bool {
    node.attrs
        .get(name)
        .is_some_and(|v| v.text.trim().eq_ignore_ascii_case("true"))
        || node.annotations.iter().any(|a| a.name == name)
}

fn attr_text(node: &Node, key: &str) -> Option<String> {
    if let Some(v) = node.attrs.get(key) {
        return Some(unquote_arg(&v.text).to_string());
    }
    node.annotations
        .iter()
        .find(|a| a.name == key)
        .and_then(|a| {
            a.attrs
                .values()
                .next()
                .map(|v| unquote_arg(&v.text).to_string())
        })
}

fn unquote_arg(raw: &str) -> &str {
    let t = raw.trim();
    if t.len() >= 2
        && ((t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\'')))
    {
        &t[1..t.len() - 1]
    } else {
        t
    }
}

fn parse_transition(edge: &crate::model::Edge, order: usize) -> TransitionSpec {
    let (mut events, mut guard, mut action) = parse_transition_label(&edge.label);
    if let Some(v) = edge.attrs.get("event") {
        let ev_text = unquote_arg(&v.text);
        events = ev_text
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    if let Some(v) = edge.attrs.get("guard").or_else(|| edge.attrs.get("when")) {
        let g = unquote_arg(&v.text).trim();
        if !g.is_empty() {
            guard = Some(g.to_string());
        }
    }
    if let Some(v) = edge.attrs.get("action").or_else(|| edge.attrs.get("do")) {
        let a = unquote_arg(&v.text).trim();
        if !a.is_empty() {
            action = Some(a.to_string());
        }
    }
    TransitionSpec {
        edge_id: edge.id.clone(),
        source: edge.source.node.clone(),
        target: edge.target.node.clone(),
        events,
        guard,
        action,
        order,
    }
}

fn parse_transition_label(raw: &str) -> (Vec<String>, Option<String>, Option<String>) {
    let text = raw.trim();
    if text.is_empty() {
        return (Vec::new(), None, None);
    }
    let bytes = text.as_bytes();
    let mut quote: Option<u8> = None;
    let mut bracket_depth: usize = 0;
    let mut guard_start: Option<usize> = None;
    let mut guard_end: Option<usize> = None;
    let mut slash_pos: Option<usize> = None;

    for (i, &b) in bytes.iter().enumerate() {
        if let Some(q) = quote {
            if b == q {
                quote = None;
            }
            continue;
        }
        match b {
            b'"' | b'\'' => quote = Some(b),
            b'[' if slash_pos.is_none() => {
                if bracket_depth == 0 && guard_start.is_none() {
                    guard_start = Some(i);
                }
                bracket_depth += 1;
            }
            b']' if slash_pos.is_none() && bracket_depth > 0 => {
                bracket_depth -= 1;
                if bracket_depth == 0 && guard_end.is_none() {
                    guard_end = Some(i);
                }
            }
            b'/' if bracket_depth == 0 => {
                slash_pos = Some(i);
                break;
            }
            _ => {}
        }
    }

    let action = slash_pos.and_then(|pos| {
        let rest = text[pos + 1..].trim();
        (!rest.is_empty()).then(|| rest.to_string())
    });

    let before_slash = match slash_pos {
        Some(pos) => &text[..pos],
        None => text,
    };

    let (ev_part, guard) = match (guard_start, guard_end) {
        (Some(gs), Some(ge)) if ge > gs => {
            let g = before_slash[gs + 1..ge].trim();
            let ev = before_slash[..gs].trim();
            (ev, (!g.is_empty()).then(|| g.to_string()))
        }
        _ => (before_slash.trim(), None),
    };

    let events = ev_part
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    (events, guard, action)
}

fn split_top_level(text: &str, delim: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut quote: Option<char> = None;
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut start = 0usize;
    for (idx, ch) in text.char_indices() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '(' => paren += 1,
            ')' => paren = paren.saturating_sub(1),
            '[' => bracket += 1,
            ']' => bracket = bracket.saturating_sub(1),
            c if c == delim && paren == 0 && bracket == 0 => {
                out.push(text[start..idx].to_string());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    out.push(text[start..].to_string());
    out
}

fn split_assign<'a>(stmt: &'a str, op: &str) -> Option<(&'a str, &'a str)> {
    let pos = stmt.find(op)?;
    let lhs = stmt[..pos].trim();
    let rhs = stmt[pos + op.len()..].trim();
    if lhs.is_empty() || rhs.is_empty() {
        None
    } else {
        Some((lhs, rhs))
    }
}

fn parse_call(stmt: &str) -> Option<(&str, &str)> {
    let trimmed = stmt.trim();
    if !trimmed.ends_with(')') {
        return None;
    }
    let open = trimmed.find('(')?;
    let name = trimmed[..open].trim();
    if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !name.is_empty() {
        Some((name, &trimmed[open + 1..trimmed.len() - 1]))
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ExprTok {
    Ident(String),
    Num(f64),
    Str(String),
    Bool(bool),
    Op(String),
    LParen,
    RParen,
    Comma,
}

fn tokenize_expr(input: &str) -> Vec<ExprTok> {
    let mut tokens = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if b == b'(' {
            tokens.push(ExprTok::LParen);
            i += 1;
            continue;
        }
        if b == b')' {
            tokens.push(ExprTok::RParen);
            i += 1;
            continue;
        }
        if b == b',' {
            tokens.push(ExprTok::Comma);
            i += 1;
            continue;
        }
        if b == b'"' || b == b'\'' {
            let q = b;
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i] != q {
                i += 1;
            }
            let s = String::from_utf8_lossy(&bytes[start..i]).into_owned();
            if i < bytes.len() {
                i += 1;
            }
            tokens.push(ExprTok::Str(s));
            continue;
        }
        let two = bytes.get(i..i + 2);
        if let Some(op) = two.and_then(|pair| match pair {
            b"==" => Some("=="),
            b"!=" => Some("!="),
            b"<=" => Some("<="),
            b">=" => Some(">="),
            b"&&" => Some("&&"),
            b"||" => Some("||"),
            _ => None,
        }) {
            tokens.push(ExprTok::Op(op.to_string()));
            i += 2;
            continue;
        }
        if matches!(
            b,
            b'<' | b'>' | b'+' | b'-' | b'*' | b'/' | b'%' | b'!' | b'='
        ) {
            tokens.push(ExprTok::Op((b as char).to_string()));
            i += 1;
            continue;
        }
        if b.is_ascii_digit() || (b == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            if let Ok(n) = input[start..i].parse::<f64>() {
                tokens.push(ExprTok::Num(n));
            }
            continue;
        }
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'.')
            {
                i += 1;
            }
            let ident = &input[start..i];
            match ident {
                "true" => tokens.push(ExprTok::Bool(true)),
                "false" => tokens.push(ExprTok::Bool(false)),
                "and" => tokens.push(ExprTok::Op("&&".to_string())),
                "or" => tokens.push(ExprTok::Op("||".to_string())),
                "not" => tokens.push(ExprTok::Op("!".to_string())),
                _ => tokens.push(ExprTok::Ident(ident.to_string())),
            }
            continue;
        }
        i += 1;
    }
    tokens
}

struct ExprParser<'a> {
    tokens: &'a [ExprTok],
    pos: usize,
    ctx: &'a BTreeMap<String, ContextValue>,
    active: &'a BTreeSet<String>,
    sim: Option<&'a Simulation>,
}

impl<'a> ExprParser<'a> {
    fn peek(&self) -> Option<&ExprTok> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<&ExprTok> {
        let tok = self.tokens.get(self.pos);
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn eat_op(&mut self, op: &str) -> bool {
        if matches!(self.peek(), Some(ExprTok::Op(s)) if s == op) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn parse_or(&mut self) -> Option<ContextValue> {
        let mut lhs = self.parse_and()?;
        while self.eat_op("||") {
            let rhs = self.parse_and()?;
            lhs = ContextValue::Bool(lhs.as_bool() || rhs.as_bool());
        }
        Some(lhs)
    }

    fn parse_and(&mut self) -> Option<ContextValue> {
        let mut lhs = self.parse_eq()?;
        while self.eat_op("&&") {
            let rhs = self.parse_eq()?;
            lhs = ContextValue::Bool(lhs.as_bool() && rhs.as_bool());
        }
        Some(lhs)
    }

    fn parse_eq(&mut self) -> Option<ContextValue> {
        let mut lhs = self.parse_cmp()?;
        loop {
            if self.eat_op("==") || self.eat_op("=") {
                let rhs = self.parse_cmp()?;
                lhs = ContextValue::Bool(values_equal(&lhs, &rhs));
            } else if self.eat_op("!=") {
                let rhs = self.parse_cmp()?;
                lhs = ContextValue::Bool(!values_equal(&lhs, &rhs));
            } else {
                break;
            }
        }
        Some(lhs)
    }

    fn parse_cmp(&mut self) -> Option<ContextValue> {
        let mut lhs = self.parse_add()?;
        loop {
            let op = if self.eat_op("<") {
                "<"
            } else if self.eat_op("<=") {
                "<="
            } else if self.eat_op(">") {
                ">"
            } else if self.eat_op(">=") {
                ">="
            } else {
                break;
            };
            let rhs = self.parse_add()?;
            let a = lhs.as_f64()?;
            let b = rhs.as_f64()?;
            let res = match op {
                "<" => a < b,
                "<=" => a <= b,
                ">" => a > b,
                _ => a >= b,
            };
            lhs = ContextValue::Bool(res);
        }
        Some(lhs)
    }

    fn parse_add(&mut self) -> Option<ContextValue> {
        let mut lhs = self.parse_mul()?;
        loop {
            if self.eat_op("+") {
                let rhs = self.parse_mul()?;
                lhs = match (lhs.as_f64(), rhs.as_f64()) {
                    (Some(a), Some(b)) => ContextValue::Num(a + b),
                    _ => ContextValue::Str(format!("{}{}", lhs.as_string(), rhs.as_string())),
                };
            } else if self.eat_op("-") {
                let rhs = self.parse_mul()?;
                lhs = ContextValue::Num(lhs.as_f64()? - rhs.as_f64()?);
            } else {
                break;
            }
        }
        Some(lhs)
    }

    fn parse_mul(&mut self) -> Option<ContextValue> {
        let mut lhs = self.parse_unary()?;
        loop {
            if self.eat_op("*") {
                let rhs = self.parse_unary()?;
                lhs = ContextValue::Num(lhs.as_f64()? * rhs.as_f64()?);
            } else if self.eat_op("/") {
                let rhs = self.parse_unary()?;
                let denom = rhs.as_f64()?;
                lhs = ContextValue::Num(if denom.abs() < 1e-12 {
                    0.0
                } else {
                    lhs.as_f64()? / denom
                });
            } else if self.eat_op("%") {
                let rhs = self.parse_unary()?;
                let denom = rhs.as_f64()?;
                lhs = ContextValue::Num(if denom.abs() < 1e-12 {
                    0.0
                } else {
                    lhs.as_f64()? % denom
                });
            } else {
                break;
            }
        }
        Some(lhs)
    }

    fn parse_unary(&mut self) -> Option<ContextValue> {
        if self.eat_op("!") {
            let v = self.parse_unary()?;
            return Some(ContextValue::Bool(!v.as_bool()));
        }
        if self.eat_op("-") {
            let v = self.parse_unary()?;
            return Some(ContextValue::Num(-v.as_f64()?));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Option<ContextValue> {
        match self.next()?.clone() {
            ExprTok::Num(n) => Some(ContextValue::Num(n)),
            ExprTok::Bool(b) => Some(ContextValue::Bool(b)),
            ExprTok::Str(s) => Some(ContextValue::Str(s)),
            ExprTok::LParen => {
                let inner = self.parse_or()?;
                if matches!(self.peek(), Some(ExprTok::RParen)) {
                    self.pos += 1;
                }
                Some(inner)
            }
            ExprTok::Ident(name) => {
                if matches!(self.peek(), Some(ExprTok::LParen)) {
                    self.pos += 1;
                    let mut args = Vec::new();
                    while !matches!(self.peek(), Some(ExprTok::RParen) | None) {
                        args.push(self.parse_or()?);
                        if matches!(self.peek(), Some(ExprTok::Comma)) {
                            self.pos += 1;
                        }
                    }
                    if matches!(self.peek(), Some(ExprTok::RParen)) {
                        self.pos += 1;
                    }
                    match name.as_str() {
                        "in" | "in_state" | "active" => {
                            let target = args
                                .first()
                                .map(ContextValue::as_string)
                                .unwrap_or_default();
                            Some(ContextValue::Bool(self.active.iter().any(|s| {
                                s == &target || s.rsplit('.').next() == Some(target.as_str())
                            })))
                        }
                        "min" => {
                            let a = args.first().and_then(ContextValue::as_f64)?;
                            let b = args.get(1).and_then(ContextValue::as_f64)?;
                            Some(ContextValue::Num(a.min(b)))
                        }
                        "max" => {
                            let a = args.first().and_then(ContextValue::as_f64)?;
                            let b = args.get(1).and_then(ContextValue::as_f64)?;
                            Some(ContextValue::Num(a.max(b)))
                        }
                        _ => None,
                    }
                } else {
                    Some(self.lookup_ident(&name))
                }
            }
            _ => None,
        }
    }

    fn lookup_ident(&self, name: &str) -> ContextValue {
        if let Some(v) = self.ctx.get(name) {
            return v.clone();
        }
        if let Some((node_ref, field)) = name.rsplit_once('.')
            && let Some(sim) = self.sim
        {
            let q_opt = sim.nodes.get(node_ref).or_else(|| {
                sim.nodes
                    .iter()
                    .find(|(k, _)| k.rsplit('.').next() == Some(node_ref))
                    .map(|(_, v)| v)
            });
            let t_opt = sim.traits.get(node_ref).or_else(|| {
                sim.traits
                    .iter()
                    .find(|(k, _)| k.rsplit('.').next() == Some(node_ref))
                    .map(|(_, v)| v)
            });
            if let Some(q) = q_opt {
                match field {
                    "queued" => return ContextValue::Num(q.queued),
                    "arrival_rate" => return ContextValue::Num(q.arrival_rate),
                    "throughput" => return ContextValue::Num(q.throughput),
                    "dropped" => return ContextValue::Num(q.dropped),
                    "saturation" => return ContextValue::Num(q.saturation),
                    "retry_rate" => return ContextValue::Num(q.retry_rate),
                    "processed" => return ContextValue::Num(q.processed),
                    _ => {}
                }
            }
            if let Some(t) = t_opt {
                match field {
                    "workers" => return ContextValue::Num(t.workers),
                    "rate" => return ContextValue::Num(t.rate),
                    "service_rate" => return ContextValue::Num(t.service_rate),
                    "queue_cap" => return ContextValue::Num(t.queue_cap),
                    _ => {}
                }
            }
        }
        ContextValue::Num(0.0)
    }
}

fn values_equal(a: &ContextValue, b: &ContextValue) -> bool {
    match (a, b) {
        (ContextValue::Bool(x), ContextValue::Bool(y)) => x == y,
        (ContextValue::Str(x), ContextValue::Str(y)) => x == y,
        _ => match (a.as_f64(), b.as_f64()) {
            (Some(x), Some(y)) => (x - y).abs() < 1e-9,
            _ => a.as_string() == b.as_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hierarchical_states_and_guards() {
        let src = r#"
order [context: "retries=0; max_retries=2"] {
  start [shape: initial]
  draft
  review
  failed
  start -> draft
  draft -> review: "submit"
  review -> review: "retry [retries < max_retries] / retries += 1"
  review -> failed: "[retries >= max_retries]"
}
"#;
        let mut sc = Statechart::from_source(src).unwrap();
        assert!(sc.active_states().contains("order"));
        assert!(sc.active_states().contains("order.draft"));

        assert!(sc.dispatch("submit").unwrap());
        assert!(sc.active_states().contains("order.review"));

        assert!(sc.dispatch("retry").unwrap());
        assert_eq!(sc.context().get("retries"), Some(&ContextValue::Num(1.0)));
        assert!(sc.active_states().contains("order.review"));

        // Second retry reaches max_retries=2 and automatically trips guarded [retries >= max_retries] -> failed.
        assert!(sc.dispatch("retry").unwrap());
        assert_eq!(sc.context().get("retries"), Some(&ContextValue::Num(2.0)));
        assert!(sc.active_states().contains("order.failed"));
    }

    #[test]
    fn parallel_orthogonal_regions_run_concurrently() {
        let src = r#"
fulfillment [parallel: true] {
  payment [region: true] {
    p_init [shape: initial]
    pending
    authorized
    p_init -> pending
    pending -> authorized: "pay"
  }
  shipping [region: true] {
    s_init [shape: initial]
    packing
    dispatched
    s_init -> packing
    packing -> dispatched: "ship"
  }
}
"#;
        let mut sc = Statechart::from_source(src).unwrap();
        assert!(sc.active_states().contains("fulfillment.payment.pending"));
        assert!(sc.active_states().contains("fulfillment.shipping.packing"));

        assert!(sc.dispatch("pay").unwrap());
        assert!(
            sc.active_states()
                .contains("fulfillment.payment.authorized")
        );
        assert!(sc.active_states().contains("fulfillment.shipping.packing"));

        assert!(sc.dispatch("ship").unwrap());
        assert!(
            sc.active_states()
                .contains("fulfillment.payment.authorized")
        );
        assert!(
            sc.active_states()
                .contains("fulfillment.shipping.dispatched")
        );
    }

    #[test]
    fn shallow_and_deep_history_restore_distinctly() {
        let src = r#"
player {
  init [shape: initial]
  shallow_h [shape: history]
  deep_h [shape: deep_history]
  mode_a {
    ma_init [shape: initial]
    step1
    step2
    ma_init -> step1
    step1 -> step2: "advance"
  }
  mode_b
  init -> mode_a
  mode_a -> mode_b: "switch"
  mode_b -> mode_a: "reset_to_a"
}
paused
player -> paused: "pause"
paused -> player.shallow_h: "resume_shallow"
paused -> player.deep_h: "resume_deep"
"#;
        let mut sc = Statechart::from_source(src).unwrap();
        assert!(sc.active_states().contains("player.mode_a.step1"));
        assert!(sc.dispatch("advance").unwrap());
        assert!(sc.active_states().contains("player.mode_a.step2"));

        // Leave player cleanly; captures shallow(player)=mode_a AND deep(player)=[mode_a, step2].
        assert!(sc.dispatch("pause").unwrap());
        assert!(sc.active_states().contains("paused"));

        // Shallow history restores mode_a via its initial (step1), NOT step2!
        assert!(sc.dispatch("resume_shallow").unwrap());
        assert!(sc.active_states().contains("player.mode_a.step1"));

        // Advance back to step2, pause again, then resume via deep history -> step2!
        assert!(sc.dispatch("advance").unwrap());
        assert!(sc.active_states().contains("player.mode_a.step2"));
        assert!(sc.dispatch("pause").unwrap());
        assert!(sc.dispatch("resume_deep").unwrap());
        assert!(sc.active_states().contains("player.mode_a.step2"));
    }

    #[test]
    fn coupled_simulation_trips_circuit_breaker() {
        let src = r#"
ingress [label: "Ingress"] @sim(rate: 80)
workers [label: "Workers"] @sim(workers: 2, service_rate: 12, queue_cap: 20)
ingress -> workers

breaker {
  b_init [shape: initial]
  closed
  open
  b_init -> closed
  closed -> open: "[workers.saturation >= 0.85] / sever(ingress -> workers)"
  open -> closed: "reset_breaker / restore(ingress -> workers)"
}
"#;
        let mut sc = Statechart::from_source(src).unwrap();
        let mut sim = Simulation::from_scene(&sc.diagram().scene).unwrap();
        assert!(sc.active_states().contains("breaker.closed"));

        assert!(sc.step_coupled(&mut sim, 30).unwrap());
        assert!(sc.active_states().contains("breaker.open"));
        assert!(!sim.severed.is_empty());

        assert!(
            sc.dispatch_with_simulation("reset_breaker", &mut sim)
                .unwrap()
        );
        assert!(sc.active_states().contains("breaker.closed"));
        assert!(sim.severed.is_empty());
    }
}
