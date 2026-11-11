//! Weighted active-set projection and bounded limited-memory BFGS.
use crate::Error;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Weights {
    pub pin: f64,
    pub crossing: f64,
    pub flip: f64,
    pub bend: f64,
    pub pill: f64,
    pub backward: f64,
    pub length: f64,
    pub centeredness: f64,
    pub source_order: f64,
    pub symmetry: f64,
}
impl Default for Weights {
    fn default() -> Self {
        Self {
            pin: 1e5,
            crossing: 2000.,
            flip: 450.,
            bend: 120.,
            pill: 60.,
            backward: 80.,
            length: 1.5,
            centeredness: 4.,
            source_order: 0.25,
            symmetry: 0.1,
        }
    }
}
/// Sparse linear constraint: sum(coefficients[i] * x[i]) >= rhs, or == rhs.
#[derive(Debug, Clone)]
pub struct Constraint {
    pub coefficients: Vec<(usize, f64)>,
    pub rhs: f64,
    pub equality: bool,
}
impl Constraint {
    pub fn separation(left: usize, right: usize, gap: f64) -> Self {
        Self {
            coefficients: vec![(left, -1.), (right, 1.)],
            rhs: gap,
            equality: false,
        }
    }
    pub fn fixed(index: usize, position: f64) -> Self {
        Self {
            coefficients: vec![(index, 1.)],
            rhs: position,
            equality: true,
        }
    }
    pub fn equality(a: usize, b: usize) -> Self {
        Self {
            coefficients: vec![(a, 1.), (b, -1.)],
            rhs: 0.,
            equality: true,
        }
    }
    fn at(&self, x: &[f64]) -> f64 {
        self.coefficients.iter().map(|(i, a)| x[*i] * a).sum()
    }
}
/// Minimize sum w_i (x_i - desired_i)^2 subject to linear constraints.
/// An active-set KKT solve, with rank-revealing elimination for redundant equalities.
pub fn project(
    desired: &[f64],
    weights: &[f64],
    constraints: &[Constraint],
) -> Result<Vec<f64>, Error> {
    let n = desired.len();
    if weights.len() != n
        || desired.iter().any(|x| !x.is_finite())
        || weights.iter().any(|w| !w.is_finite() || *w <= 0.)
        || constraints.iter().any(|c| {
            !c.rhs.is_finite()
                || c.coefficients
                    .iter()
                    .any(|(i, a)| *i >= n || !a.is_finite())
        })
    {
        return Err(Error::Layout("invalid projection input".into()));
    }
    let mut active: Vec<usize> = constraints
        .iter()
        .enumerate()
        .filter(|(_, c)| c.equality)
        .map(|(i, _)| i)
        .collect();
    let mut x = desired.to_vec();
    for _ in 0..(constraints.len() + n + 1) * 20 {
        let m = active.len();
        let mut matrix = vec![vec![0.; m + 1]; m];
        for (i, &ci) in active.iter().enumerate() {
            let c = &constraints[ci];
            matrix[i][m] = c.rhs - c.at(desired);
            for (j, &cj) in active.iter().enumerate() {
                let d = &constraints[cj];
                matrix[i][j] = c
                    .coefficients
                    .iter()
                    .flat_map(|(a, ca)| {
                        d.coefficients
                            .iter()
                            .filter(move |(b, _)| a == b)
                            .map(move |(_, cb)| ca * cb / weights[*a])
                    })
                    .sum();
            }
        }
        let lambda = match linear_solve(matrix) {
            Ok(lambda) => lambda,
            Err(error) => {
                if active.iter().all(|i| constraints[*i].equality) {
                    return Err(error);
                }
                return dual_project(desired, weights, constraints);
            }
        };
        x.copy_from_slice(desired);
        for (i, &ci) in active.iter().enumerate() {
            for &(j, a) in &constraints[ci].coefficients {
                x[j] += lambda[i] * a / weights[j];
            }
        }
        if let Some((remove, _)) = active
            .iter()
            .enumerate()
            .filter(|(i, c)| !constraints[**c].equality && lambda[*i] < -1e-7)
            .min_by(|(i, _), (j, _)| lambda[*i].total_cmp(&lambda[*j]))
        {
            active.remove(remove);
            continue;
        }
        let worst = constraints
            .iter()
            .enumerate()
            .filter(|(i, c)| !c.equality && !active.contains(i))
            .map(|(i, c)| (i, c.at(&x) - c.rhs))
            .filter(|(_, v)| *v < -1e-6)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, _)) = worst {
            active.push(i);
        } else {
            return Ok(x);
        }
    }
    dual_project(desired, weights, constraints)
}
// Hildreth dual-coordinate fallback releases dependent active inequalities. It is essential
// when a transitive separation becomes slack after another active constraint is introduced.
fn dual_project(
    desired: &[f64],
    weights: &[f64],
    constraints: &[Constraint],
) -> Result<Vec<f64>, Error> {
    let mut x = desired.to_vec();
    let mut multipliers = vec![0.; constraints.len()];
    let norms: Vec<f64> = constraints
        .iter()
        .map(|c| {
            c.coefficients
                .iter()
                .map(|(i, a)| a * a / weights[*i])
                .sum()
        })
        .collect();
    for _ in 0..20000 {
        let mut movement = 0.0f64;
        for (i, c) in constraints.iter().enumerate() {
            let residual = c.rhs - c.at(&x);
            if norms[i] < 1e-20 {
                if residual.abs() > 1e-7 {
                    return Err(Error::Layout("inconsistent constant constraint".into()));
                }
                continue;
            }
            let proposed = multipliers[i] + residual / norms[i];
            let next = if c.equality {
                proposed
            } else {
                proposed.max(0.)
            };
            let delta = next - multipliers[i];
            multipliers[i] = next;
            if !next.is_finite() || next.abs() > 1e15 {
                return Err(Error::Layout("infeasible separation constraints".into()));
            }
            for &(j, a) in &c.coefficients {
                let step = delta * a / weights[j];
                x[j] += step;
                movement = movement.max(step.abs());
            }
        }
        if movement < 1e-8
            && constraints.iter().all(|c| {
                let r = c.at(&x) - c.rhs;
                if c.equality {
                    r.abs() < 1e-6
                } else {
                    r >= -1e-6
                }
            })
        {
            for c in constraints
                .iter()
                .filter(|c| c.equality && c.coefficients.len() == 1)
            {
                let (i, a) = c.coefficients[0];
                x[i] = c.rhs / a;
            }
            return Ok(x);
        }
    }
    Err(Error::Layout(
        "infeasible constraints or dual projection convergence budget exceeded".into(),
    ))
}
fn linear_solve(mut a: Vec<Vec<f64>>) -> Result<Vec<f64>, Error> {
    let n = a.len();
    let mut row = 0;
    let mut pivots = Vec::new();
    for col in 0..n {
        let pivot = (row..n).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()));
        let Some(pivot) = pivot else {
            break;
        };
        if a[pivot][col].abs() < 1e-10 {
            continue;
        }
        a.swap(row, pivot);
        let scale = a[row][col];
        for v in &mut a[row][col..=n] {
            *v /= scale;
        }
        for i in 0..n {
            if i == row {
                continue;
            }
            let factor = a[i][col];
            let pivot_values = a[row][col..=n].to_vec();
            for (cell, pivot) in a[i][col..=n].iter_mut().zip(pivot_values) {
                *cell -= factor * pivot;
            }
        }
        pivots.push(col);
        row += 1;
    }
    for r in &a[row..] {
        if r[n].abs() > 1e-5 {
            return Err(Error::Layout("contradictory geometric constraints".into()));
        }
    }
    let mut x = vec![0.; n];
    for (i, &col) in pivots.iter().enumerate() {
        x[col] = a[i][n];
    }
    Ok(x)
}
#[derive(Debug, Clone)]
pub struct Dual {
    pub value: f64,
    pub gradient: Vec<f64>,
}
impl Dual {
    pub fn variable(value: f64, index: usize, n: usize) -> Self {
        let mut gradient = vec![0.; n];
        gradient[index] = 1.;
        Self { value, gradient }
    }
    pub fn constant(value: f64, n: usize) -> Self {
        Self {
            value,
            gradient: vec![0.; n],
        }
    }
    pub fn add(&self, b: &Self) -> Self {
        Self {
            value: self.value + b.value,
            gradient: self
                .gradient
                .iter()
                .zip(&b.gradient)
                .map(|(a, b)| a + b)
                .collect(),
        }
    }
    pub fn sub(&self, b: &Self) -> Self {
        self.add(&b.scale(-1.))
    }
    pub fn scale(&self, a: f64) -> Self {
        Self {
            value: self.value * a,
            gradient: self.gradient.iter().map(|x| x * a).collect(),
        }
    }
    pub fn square(&self) -> Self {
        Self {
            value: self.value * self.value,
            gradient: self.gradient.iter().map(|g| 2. * self.value * g).collect(),
        }
    }
    pub fn sqrt(&self) -> Self {
        let value = self.value.max(1e-20).sqrt();
        Self {
            value,
            gradient: self.gradient.iter().map(|g| g / (2. * value)).collect(),
        }
    }
    pub fn softplus(&self, beta: f64) -> Self {
        let z = beta * self.value;
        let value = (z.max(0.) + (-z.abs()).exp().ln_1p()) / beta;
        let sigmoid = if z >= 0. {
            1. / (1. + (-z).exp())
        } else {
            z.exp() / (1. + z.exp())
        };
        Self {
            value,
            gradient: self.gradient.iter().map(|g| g * sigmoid).collect(),
        }
    }
}
/// Bounded L-BFGS with projected Armijo line search and exact forward-mode gradients.
pub fn lbfgs(
    initial: &[f64],
    bounds: &[(f64, f64)],
    steps: usize,
    energy: impl Fn(&[Dual]) -> Dual,
) -> Vec<f64> {
    let n = initial.len();
    if n == 0 {
        return Vec::new();
    }
    let eval = |x: &[f64]| {
        energy(
            &x.iter()
                .enumerate()
                .map(|(i, v)| Dual::variable(*v, i, n))
                .collect::<Vec<_>>(),
        )
    };
    let mut x = initial.to_vec();
    for (i, v) in x.iter_mut().enumerate() {
        *v = v.clamp(bounds[i].0, bounds[i].1);
    }
    let mut f = eval(&x);
    let mut history: Vec<(Vec<f64>, Vec<f64>, f64)> = Vec::new();
    for _ in 0..steps {
        let mut q = f.gradient.clone();
        let mut alphas = Vec::new();
        for (s, y, rho) in history.iter().rev() {
            let a = rho * dot(s, &q);
            alphas.push(a);
            for i in 0..n {
                q[i] -= a * y[i];
            }
        }
        let scale = history
            .last()
            .map(|(s, y, _)| dot(s, y) / dot(y, y).max(1e-20))
            .unwrap_or(1.);
        for v in &mut q {
            *v *= scale;
        }
        for ((s, y, rho), alpha) in history.iter().zip(alphas.iter().rev()) {
            let b = rho * dot(y, &q);
            for i in 0..n {
                q[i] += s[i] * (alpha - b);
            }
        }
        let mut direction: Vec<_> = q.iter().map(|v| -v).collect();
        if dot(&direction, &f.gradient) >= 0. {
            direction = f.gradient.iter().map(|v| -v).collect();
        }
        let mut step = 1.;
        let mut accepted = None;
        for _ in 0..24 {
            let candidate: Vec<_> = x
                .iter()
                .enumerate()
                .map(|(i, v)| (v + step * direction[i]).clamp(bounds[i].0, bounds[i].1))
                .collect();
            let delta: Vec<_> = candidate.iter().zip(&x).map(|(a, b)| a - b).collect();
            let next = eval(&candidate);
            if next.value.is_finite() && next.value <= f.value + 1e-4 * dot(&f.gradient, &delta) {
                accepted = Some((candidate, next, delta));
                break;
            }
            step *= 0.5;
        }
        let Some((next, nf, s)) = accepted else {
            break;
        };
        if dot(&s, &s) < 1e-16 {
            break;
        }
        let y: Vec<_> = nf
            .gradient
            .iter()
            .zip(&f.gradient)
            .map(|(a, b)| a - b)
            .collect();
        let sy = dot(&s, &y);
        if sy > 1e-10 {
            if history.len() == 5 {
                history.remove(0);
            }
            history.push((s, y, 1. / sy));
        }
        x = next;
        f = nf;
    }
    x
}
fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}

/// Translation-adjusted anchoring with elastic prior-order hinges. A common translation
/// variable removes rigid motion from the anchor cost; order slacks carry a linear penalty.
pub fn project_stable(
    desired: &[f64],
    weights: &[f64],
    constraints: &[Constraint],
    previous: &[Option<f64>],
    flip_penalty: f64,
) -> Result<Vec<f64>, Error> {
    let n = desired.len();
    if previous.len() != n {
        return Err(Error::Layout("invalid stability anchors".into()));
    }
    let mut ranked: Vec<_> = previous
        .iter()
        .enumerate()
        .filter_map(|(i, v)| v.map(|v| (i, v)))
        .collect();
    ranked.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    let orders: Vec<_> = ranked
        .windows(2)
        .filter(|p| p[1].1 - p[0].1 >= 1.)
        .map(|p| (p[0].0, p[1].0))
        .collect();
    let mut hinges = Vec::<(usize, usize)>::new();
    let solve = |hinges: &[(usize, usize)]| -> Result<Vec<f64>, Error> {
        let mut d = desired.to_vec();
        let mut w = weights.to_vec();
        d.push(0.);
        w.push(1e-6);
        let mut cs = constraints.to_vec();
        for c in &mut cs {
            let sum: f64 = c.coefficients.iter().map(|(_, a)| *a).sum();
            if sum.abs() > 1e-10 {
                c.coefficients.push((n, sum));
            }
        }
        for &(a, b) in hinges {
            let slack = d.len();
            let epsilon = 1e-4;
            d.push(-flip_penalty / (2. * epsilon));
            w.push(epsilon);
            cs.push(Constraint {
                coefficients: vec![(a, -1.), (b, 1.), (slack, 1.)],
                rhs: 0.,
                equality: false,
            });
            cs.push(Constraint {
                coefficients: vec![(slack, 1.)],
                rhs: 0.,
                equality: false,
            });
        }
        let solution = project(&d, &w, &cs)?;
        let result: Vec<_> = solution[..n].iter().map(|v| v + solution[n]).collect();
        project(&result, weights, constraints)
    };
    let mut current = solve(&[])?;
    for _ in 0..orders.len() + 1 {
        let additions: Vec<_> = orders
            .iter()
            .filter(|(a, b)| current[*a] > current[*b] + 1e-6 && !hinges.contains(&(*a, *b)))
            .copied()
            .collect();
        if additions.is_empty() {
            return Ok(current);
        }
        hinges.extend(additions);
        current = solve(&hinges)?;
    }
    Err(Error::Layout(
        "elastic order projection did not converge".into(),
    ))
}
