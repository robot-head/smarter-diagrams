//! Finite-state safety checking via one-hot bounded SAT unrolling (horizons 0 through 12).
use crate::Error;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub name: String,
    pub variables: BTreeMap<String, String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transition {
    pub from: usize,
    pub to: usize,
    pub event: String,
    pub edge: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionSystem {
    pub states: Vec<State>,
    pub initial: Vec<usize>,
    pub transitions: Vec<Transition>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Counterexample {
    pub states: Vec<State>,
    pub events: Vec<String>,
    pub edges: Vec<Option<String>>,
    pub horizon: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CheckResult {
    SafeThrough { horizon: usize },
    Violation(Counterexample),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Invariant {
    Equals {
        variable: String,
        value: String,
    },
    CountAtMost {
        suffix: String,
        value: String,
        count: usize,
    },
    Never {
        variable: String,
        value: String,
    },
}
impl Invariant {
    pub fn holds(&self, state: &State) -> bool {
        match self {
            Self::Equals { variable, value } => state.variables.get(variable) == Some(value),
            Self::Never { variable, value } => state.variables.get(variable) != Some(value),
            Self::CountAtMost {
                suffix,
                value,
                count,
            } => {
                state
                    .variables
                    .iter()
                    .filter(|(k, v)| k.ends_with(suffix) && *v == value)
                    .count()
                    <= *count
            }
        }
    }
}
/// CNF literals use signed, one-based variable IDs; deterministic unit-propagating DPLL.
pub fn satisfy(variables: usize, clauses: &[Vec<i32>]) -> Result<Option<Vec<bool>>, Error> {
    if clauses
        .iter()
        .flatten()
        .any(|v| *v == 0 || v.unsigned_abs() as usize > variables)
    {
        return Err(Error::Edit("invalid SAT literal".into()));
    }
    fn solve(clauses: &[Vec<i32>], mut assignment: Vec<i8>, budget: &mut usize) -> Option<Vec<i8>> {
        if *budget == 0 {
            return None;
        }
        *budget -= 1;
        loop {
            let mut changed = false;
            for clause in clauses {
                let mut unit = 0;
                let mut unknown = 0;
                let mut satisfied = false;
                for &literal in clause {
                    let value = assignment[literal.unsigned_abs() as usize - 1];
                    if value == 0 {
                        unit = literal;
                        unknown += 1;
                    } else if (value > 0) == (literal > 0) {
                        satisfied = true;
                        break;
                    }
                }
                if satisfied {
                    continue;
                }
                if unknown == 0 {
                    return None;
                }
                if unknown == 1 {
                    let i = unit.unsigned_abs() as usize - 1;
                    let v = if unit > 0 { 1 } else { -1 };
                    if assignment[i] != 0 && assignment[i] != v {
                        return None;
                    }
                    if assignment[i] == 0 {
                        assignment[i] = v;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let choice = clauses
            .iter()
            .filter(|c| {
                !c.iter().any(|l| {
                    assignment[l.unsigned_abs() as usize - 1] != 0
                        && (assignment[l.unsigned_abs() as usize - 1] > 0) == (*l > 0)
                })
            })
            .flat_map(|c| c.iter())
            .find(|l| assignment[l.unsigned_abs() as usize - 1] == 0)
            .map(|l| l.unsigned_abs() as usize - 1);
        let Some(i) = choice else {
            return Some(assignment);
        };
        for v in [1, -1] {
            let mut next = assignment.clone();
            next[i] = v;
            if let Some(result) = solve(clauses, next, budget) {
                return Some(result);
            }
        }
        None
    }
    let mut budget = 2_000_000;
    let result = solve(clauses, vec![0; variables], &mut budget);
    if budget == 0 && result.is_none() {
        return Err(Error::Layout(
            "bounded SAT search budget exhausted; result unknown".into(),
        ));
    }
    Ok(result.map(|a| a.into_iter().map(|v| v > 0).collect()))
}
pub fn check(
    system: &TransitionSystem,
    invariant: &Invariant,
    horizon: usize,
) -> Result<CheckResult, Error> {
    let n = system.states.len();
    if horizon > 12
        || n == 0
        || n > 512
        || system.initial.is_empty()
        || system.initial.iter().any(|i| *i >= n)
        || system.transitions.iter().any(|t| t.from >= n || t.to >= n)
    {
        return Err(Error::Edit(
            "finite model requires 1..512 states, valid transitions and horizon <=12".into(),
        ));
    }
    for k in 0..=horizon {
        let var = |t: usize, i: usize| (t * n + i + 1) as i32;
        let mut clauses = Vec::new();
        for t in 0..=k {
            clauses.push((0..n).map(|i| var(t, i)).collect());
            for a in 0..n {
                for b in a + 1..n {
                    clauses.push(vec![-var(t, a), -var(t, b)]);
                }
            }
        }
        clauses.push(system.initial.iter().map(|i| var(0, *i)).collect());
        for t in 0..k {
            for i in 0..n {
                let next: Vec<_> = system
                    .transitions
                    .iter()
                    .filter(|x| x.from == i)
                    .map(|x| x.to)
                    .collect();
                let mut clause = vec![-var(t, i)];
                clause.extend(next.into_iter().map(|j| var(t + 1, j)));
                clauses.push(clause);
            }
        }
        clauses.push(
            system
                .states
                .iter()
                .enumerate()
                .filter(|(_, s)| !invariant.holds(s))
                .map(|(i, _)| var(k, i))
                .collect(),
        );
        if let Some(solution) = satisfy((k + 1) * n, &clauses)? {
            let ids: Vec<_> = (0..=k)
                .map(|t| (0..n).find(|i| solution[t * n + i]).unwrap())
                .collect();
            let mut events = Vec::new();
            let mut edges = Vec::new();
            for pair in ids.windows(2) {
                let transition = system
                    .transitions
                    .iter()
                    .find(|t| t.from == pair[0] && t.to == pair[1])
                    .unwrap();
                events.push(transition.event.clone());
                edges.push(transition.edge.clone());
            }
            return Ok(CheckResult::Violation(Counterexample {
                states: ids.into_iter().map(|i| system.states[i].clone()).collect(),
                events,
                edges,
                horizon: k,
            }));
        }
    }
    Ok(CheckResult::SafeThrough { horizon })
}
/// Parse the RFC's count(role == Leader) <= 1 form into a finite-state predicate.
pub fn parse_invariant(text: &str) -> Result<Invariant, Error> {
    let text = text.trim();
    let Some(rest) = text.strip_prefix("count(") else {
        return Err(Error::Edit(
            "expected count(variable == value) <= integer".into(),
        ));
    };
    let (predicate, limit) = rest
        .split_once(") <=")
        .ok_or_else(|| Error::Edit("invalid count invariant".into()))?;
    let (variable, value) = predicate
        .split_once("==")
        .ok_or_else(|| Error::Edit("invalid equality predicate".into()))?;
    let count = limit
        .trim()
        .parse()
        .map_err(|_| Error::Edit("invalid invariant count".into()))?;
    Ok(Invariant::CountAtMost {
        suffix: variable.trim().into(),
        value: value.trim().into(),
        count,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedCheck {
    pub invariant: String,
    pub result: CheckResult,
}
/// Check the invariants attached to a zone, restricting state variables to its member nodes.
pub fn check_zone(
    scene: &crate::model::SceneGraph,
    zone: &str,
    system: &TransitionSystem,
    horizon: usize,
) -> Result<Vec<NamedCheck>, Error> {
    let zone = scene
        .zones
        .iter()
        .find(|z| z.id == zone)
        .ok_or_else(|| Error::Edit("unknown verification zone".into()))?;
    let mut model = system.clone();
    for state in &mut model.states {
        state.variables.retain(|key, _| {
            !key.contains('.')
                || zone
                    .members
                    .iter()
                    .any(|id| key.starts_with(&(id.clone() + ".")))
        });
    }
    let mut out = Vec::new();
    for (key, value) in &zone.attrs {
        if let Some(name) = key.strip_prefix("invariant:") {
            let invariant = parse_invariant(&value.text)?;
            out.push(NamedCheck {
                invariant: name.into(),
                result: check(&model, &invariant, horizon)?,
            });
        }
    }
    Ok(out)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceFrame {
    pub state: State,
    pub event: Option<String>,
    pub active_edge: Option<String>,
    pub step: usize,
}
pub fn trace_frame(trace: &Counterexample, step: usize) -> Option<TraceFrame> {
    Some(TraceFrame {
        state: trace.states.get(step)?.clone(),
        event: step
            .checked_sub(1)
            .and_then(|i| trace.events.get(i))
            .cloned(),
        active_edge: step
            .checked_sub(1)
            .and_then(|i| trace.edges.get(i))
            .cloned()
            .flatten(),
        step,
    })
}
