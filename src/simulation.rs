//! Fixed-step fluid queueing with bounded queues, delayed retries and counterfactual edge cuts.
use crate::{Error, model::*};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimTraits {
    pub rate: f64,
    pub workers: f64,
    pub service_rate: f64,
    pub queue_cap: f64,
    pub timeout: f64,
    pub retry_fraction: f64,
    pub retry_backoff: f64,
}
impl Default for SimTraits {
    fn default() -> Self {
        Self {
            rate: 0.,
            workers: 1.,
            service_rate: 100.,
            queue_cap: 1000.,
            timeout: 1.,
            retry_fraction: 0.,
            retry_backoff: 1.,
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueueState {
    pub queued: f64,
    pub arrival_rate: f64,
    pub throughput: f64,
    pub dropped: f64,
    pub saturation: f64,
    pub retry_rate: f64,
    pub processed: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Simulation {
    pub time: f64,
    pub nodes: BTreeMap<NodeId, QueueState>,
    pub traits: BTreeMap<NodeId, SimTraits>,
    pub severed: BTreeSet<EdgeId>,
    edges: Vec<(EdgeId, NodeId, NodeId)>,
    retries: VecDeque<(f64, NodeId, f64)>,
}
impl Simulation {
    pub fn from_scene(scene: &SceneGraph) -> Result<Self, Error> {
        let mut traits = BTreeMap::new();
        for n in &scene.nodes {
            if n.container && n.shape != "sql_table" {
                continue;
            }
            let mut t = SimTraits::default();
            if let Some(a) = n.annotations.iter().find(|a| a.name == "sim") {
                for (key, target) in [
                    ("rate", &mut t.rate),
                    ("workers", &mut t.workers),
                    ("queue_cap", &mut t.queue_cap),
                    ("timeout", &mut t.timeout),
                    ("retry_fraction", &mut t.retry_fraction),
                    ("retry_backoff", &mut t.retry_backoff),
                    ("service_rate", &mut t.service_rate),
                ] {
                    if let Some(v) = a.attrs.get(key) {
                        *target = number(&v.text)
                            .ok_or_else(|| Error::Layout(format!("invalid @sim {key}")))?;
                    }
                }
                if let Some(v) = a.attrs.get("service") {
                    if let Some(args) = v
                        .text
                        .strip_prefix("lognormal(")
                        .and_then(|s| s.strip_suffix(')'))
                    {
                        let p: Vec<_> = args.split(',').filter_map(number).collect();
                        if p.len() != 2 {
                            return Err(Error::Layout("lognormal requires mu and sigma".into()));
                        }
                        t.service_rate = 1. / (p[0] + p[1] * p[1] / 2.).exp();
                    } else {
                        t.service_rate = number(&v.text)
                            .ok_or_else(|| Error::Layout("invalid service distribution".into()))?;
                    }
                }
                if let Some(v) = a.attrs.get("retry")
                    && let Some(args) = v
                        .text
                        .strip_prefix("exp_backoff(")
                        .and_then(|s| s.strip_suffix(')'))
                {
                    let p: Vec<_> = args.split(',').filter_map(number).collect();
                    if p.is_empty() {
                        return Err(Error::Layout("exp_backoff requires a delay".into()));
                    }
                    t.retry_backoff = p[0];
                    t.retry_fraction = p.get(1).copied().unwrap_or(1.);
                }
            }
            if [
                t.rate,
                t.workers,
                t.service_rate,
                t.queue_cap,
                t.timeout,
                t.retry_fraction,
                t.retry_backoff,
            ]
            .iter()
            .any(|x| !x.is_finite() || *x < 0.)
                || t.queue_cap == 0.
                || t.timeout == 0.
                || t.retry_backoff == 0.
                || t.retry_fraction > 1.
            {
                return Err(Error::Layout(format!(
                    "invalid simulation parameters for {}",
                    n.id
                )));
            }
            traits.insert(n.id.clone(), t);
        }
        let nodes = traits
            .keys()
            .map(|id| (id.clone(), QueueState::default()))
            .collect();
        let edges = scene
            .edges
            .iter()
            .filter(|e| traits.contains_key(&e.source.node) && traits.contains_key(&e.target.node))
            .map(|e| (e.id.clone(), e.source.node.clone(), e.target.node.clone()))
            .collect();
        Ok(Self {
            time: 0.,
            nodes,
            traits,
            severed: BTreeSet::new(),
            edges,
            retries: VecDeque::new(),
        })
    }
    pub fn set_rate(&mut self, node: &str, rate: f64) -> Result<(), Error> {
        if !rate.is_finite() || rate < 0. {
            return Err(Error::Edit(
                "ingress rate must be nonnegative and finite".into(),
            ));
        }
        self.traits
            .get_mut(node)
            .ok_or_else(|| Error::Edit("unknown simulation node".into()))?
            .rate = rate;
        Ok(())
    }
    pub fn sever(&mut self, edge: &str, severed: bool) -> Result<(), Error> {
        if !self.edges.iter().any(|(id, _, _)| id == edge) {
            return Err(Error::Edit("unknown simulation edge".into()));
        }
        if severed {
            self.severed.insert(edge.into());
        } else {
            self.severed.remove(edge);
        }
        Ok(())
    }
    pub fn tick(&mut self) -> Result<(), Error> {
        self.advance(1. / 60.)
    }
    pub fn advance(&mut self, seconds: f64) -> Result<(), Error> {
        if !seconds.is_finite() || !(0. ..=3600.).contains(&seconds) {
            return Err(Error::Edit(
                "simulation duration must be in [0,3600]".into(),
            ));
        }
        let steps = (seconds * 60.).ceil() as usize;
        if steps == 0 {
            return Ok(());
        }
        let dt = seconds / steps as f64;
        for _ in 0..steps {
            self.step(dt);
        }
        Ok(())
    }
    fn step(&mut self, dt: f64) {
        let mut arrival: BTreeMap<_, f64> = self
            .traits
            .iter()
            .map(|(id, t)| (id.clone(), t.rate))
            .collect();
        let mut retry = BTreeMap::<String, f64>::new();
        while self
            .retries
            .front()
            .is_some_and(|(time, _, _)| *time <= self.time + 1e-9)
        {
            let (_, id, amount) = self.retries.pop_front().unwrap();
            *arrival.entry(id.clone()).or_default() += amount / dt;
            *retry.entry(id).or_default() += amount / dt;
        }
        // Simultaneous fluid rates use prior-step service output, avoiding declaration-order bias.
        for (_, from, to) in self
            .edges
            .iter()
            .filter(|(id, _, _)| !self.severed.contains(id))
        {
            let count = self
                .edges
                .iter()
                .filter(|(id, a, _)| a == from && !self.severed.contains(id))
                .count()
                .max(1) as f64;
            *arrival.get_mut(to).unwrap() += self.nodes[from].throughput / count;
        }
        let mut pending = Vec::new();
        for (id, state) in &mut self.nodes {
            let t = &self.traits[id];
            let rate = arrival[id];
            let capacity = t.workers * t.service_rate;
            let throughput = capacity.min(rate + state.queued / dt);
            let next = (state.queued + (rate - throughput) * dt).max(0.);
            let dropped = (next - t.queue_cap).max(0.);
            state.queued = next.min(t.queue_cap);
            state.arrival_rate = rate;
            state.throughput = throughput;
            state.processed += throughput * dt;
            state.dropped += dropped;
            state.saturation = state.queued / t.queue_cap;
            state.retry_rate = retry.get(id).copied().unwrap_or(0.);
            let wait = state.queued / capacity.max(1e-9);
            let probability = if wait > t.timeout {
                1. - (-wait / t.timeout).exp()
            } else {
                0.
            };
            let amount = (rate * probability * dt + dropped) * t.retry_fraction;
            if amount > 0. {
                pending.push((self.time + t.retry_backoff, id.clone(), amount));
            }
        }
        self.retries.extend(pending);
        self.retries
            .make_contiguous()
            .sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        self.time += dt;
    }
}
