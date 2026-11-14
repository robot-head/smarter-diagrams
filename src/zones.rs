//! Smooth oriented enclosing ellipses for shared relational zones.
//!
//! Membership never implies ownership: members across containers share one
//! contiguous elliptical blob rather than snaking rectangularly or carving cutouts.
use crate::{layout::*, model::*};
use std::{
    collections::{BTreeMap, BTreeSet},
    f64::consts::{FRAC_PI_2, PI, TAU},
};

/// Oriented ellipse fitted around a zone's member silhouettes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ZoneEllipse {
    pub cx: f64,
    pub cy: f64,
    pub rx: f64,
    pub ry: f64,
    /// Rotation of `rx` axis from `+X` toward `+Y` in radians, in `[0, PI)`.
    pub theta: f64,
}

impl ZoneEllipse {
    /// Tight world-space axis-aligned bounding box of this ellipse.
    pub(crate) fn bounds(self) -> Rect {
        let (c, s) = (self.theta.cos(), self.theta.sin());
        let half_w = (self.rx * c).hypot(self.ry * s);
        let half_h = (self.rx * s).hypot(self.ry * c);
        Rect::new(self.cx, self.cy, 2. * half_w, 2. * half_h)
    }

    /// True if `p` lies inside or on the boundary of this ellipse.
    pub(crate) fn contains(self, p: Point) -> bool {
        let (dx, dy) = (p.x - self.cx, p.y - self.cy);
        let (c, s) = (self.theta.cos(), self.theta.sin());
        let u = dx * c + dy * s;
        let v = -dx * s + dy * c;
        (u / self.rx).powi(2) + (v / self.ry).powi(2) <= 1. + 1e-9
    }

    /// Sample `steps` vertices in clockwise screen order (positive shoelace area).
    pub(crate) fn sample_polygon(self, steps: usize) -> Vec<Point> {
        let steps = steps.max(12);
        let (ct, st) = (self.theta.cos(), self.theta.sin());
        (0..steps)
            .map(|i| {
                let t = TAU * (i as f64) / (steps as f64);
                let (ux, uy) = (self.rx * t.cos(), self.ry * t.sin());
                Point::new(self.cx + ux * ct - uy * st, self.cy + ux * st + uy * ct)
            })
            .collect()
    }
}

/// Fit an oriented enclosing ellipse around `zone`'s members using `layout.nodes`.
pub(crate) fn ellipse(scene: &SceneGraph, layout: &Layout, zone: &Zone) -> Option<ZoneEllipse> {
    ellipse_with_grid(scene, &layout.nodes, zone, GRID)
}

/// Sampled outer contour polygon matching [`ellipse`] for bounds/contour queries.
pub(crate) fn contours(scene: &SceneGraph, layout: &Layout, zone: &Zone) -> Vec<Vec<Point>> {
    ellipse(scene, layout, zone)
        .map(|e| vec![e.sample_polygon(64)])
        .unwrap_or_default()
}

/// Fit an oriented enclosing ellipse around `zone`'s members using a custom grid scale.
pub(crate) fn ellipse_with_grid(
    scene: &SceneGraph,
    nodes: &BTreeMap<String, Rect>,
    zone: &Zone,
    grid: f64,
) -> Option<ZoneEllipse> {
    let members: BTreeSet<String> = scene
        .nodes
        .iter()
        .filter(|n| {
            zone.members.contains(&n.id)
                || ancestors(scene, &n.id)
                    .iter()
                    .any(|id| zone.members.contains(id))
        })
        .map(|n| n.id.clone())
        .collect();
    if members.is_empty() {
        return None;
    }

    let padding = value(&zone.attrs, "padding").unwrap_or(1.5).max(0.) * grid;
    let min_clear = (padding * 0.25).min(6.);
    let face_pad = (padding * 0.65 - min_clear).max(padding.min(2.));

    let mut pts = Vec::new();
    for id in &members {
        let (Some(&r), Some(node)) = (nodes.get(id), scene.node(id)) else {
            continue;
        };
        if r.w <= 0. || r.h <= 0. {
            continue;
        }
        let eff = effective_member_rect(node, r);
        let rx_base = match node.shape.as_str() {
            "actor" => 18.,
            "sql_table" => 2.,
            _ if node.container => 12.,
            _ => 8.,
        };
        sample_rounded_silhouette(eff, min_clear, rx_base, &mut pts);
    }
    if pts.is_empty() {
        return None;
    }

    let owner_box = common_container_interior(scene, nodes, &members);
    let hull = convex_hull(pts.clone());
    let external_rects: Vec<Rect> = scene
        .nodes
        .iter()
        .filter(|n| !members.contains(&n.id) && !(n.container && n.shape != "sql_table"))
        .filter_map(|n| nodes.get(&n.id).map(|&r| effective_member_rect(n, r)))
        .filter(|nr| nr.w > 0. && nr.h > 0. && !point_in_convex_hull(nr.center(), &hull))
        .map(|nr| nr.inflate(4.))
        .collect();

    Some(fit_zone_ellipse(&pts, face_pad, owner_box, &external_rects))
}

fn effective_member_rect(node: &Node, r: Rect) -> Rect {
    if let Some(tr) = sql_table_title_rect(node, r) {
        let top = r.top().min(tr.top());
        Rect::new(r.x, (top + r.bottom()) / 2., r.w, r.bottom() - top)
    } else {
        r
    }
}

fn sample_rounded_silhouette(r: Rect, min_clear: f64, rx_base: f64, out: &mut Vec<Point>) {
    let hw = r.w / 2. + min_clear;
    let hh = r.h / 2. + min_clear;
    let cr = hw.min(hh).min(rx_base + min_clear).max(0.);
    let ix = (hw - cr).max(0.);
    let iy = (hh - cr).max(0.);
    const SEGMENTS_PER_QUADRANT: usize = 6;
    // Scale circular arc radius by sec(half-step) so the inscribed polygon covers the exact arcs.
    let rc = if cr > 0. {
        cr / (PI / (4. * SEGMENTS_PER_QUADRANT as f64)).cos()
    } else {
        0.
    };
    let corners = [
        (1., 1., 0.),
        (-1., 1., FRAC_PI_2),
        (-1., -1., PI),
        (1., -1., 3. * FRAC_PI_2),
    ];
    for (sx, sy, a0) in corners {
        for j in 0..=SEGMENTS_PER_QUADRANT {
            let a = a0 + FRAC_PI_2 * (j as f64) / (SEGMENTS_PER_QUADRANT as f64);
            out.push(Point::new(
                r.x + sx * ix + rc * a.cos(),
                r.y + sy * iy + rc * a.sin(),
            ));
        }
    }
}

fn common_container_interior(
    scene: &SceneGraph,
    nodes: &BTreeMap<String, Rect>,
    members: &BTreeSet<String>,
) -> Option<Rect> {
    let mut common: Option<BTreeSet<String>> = None;
    for id in members {
        let ancs: BTreeSet<String> = ancestors(scene, id)
            .into_iter()
            .filter(|a| !members.contains(a))
            .collect();
        common = Some(match common {
            None => ancs,
            Some(prev) => prev.intersection(&ancs).cloned().collect(),
        });
    }
    let best_id = common?
        .into_iter()
        .max_by_key(|cid| (ancestors(scene, cid).len(), cid.clone()))?;
    let c_node = scene.node(&best_id)?;
    let cr = *nodes.get(&best_id)?;
    let left = cr.left() + 6.;
    let right = cr.right() - 6.;
    let top = cr.top() + header_height(c_node).max(28.);
    let bottom = cr.bottom() - 6.;
    (right > left && bottom > top).then(|| {
        Rect::new(
            (left + right) / 2.,
            (top + bottom) / 2.,
            right - left,
            bottom - top,
        )
    })
}

fn point_in_convex_hull(p: Point, hull: &[Point]) -> bool {
    if hull.len() < 3 {
        return false;
    }
    hull.iter().enumerate().all(|(i, &a)| {
        let b = hull[(i + 1) % hull.len()];
        (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x) >= -1e-6
    })
}

#[derive(Clone, Copy)]
struct FitCandidate {
    cost: f64,
    ellipse: ZoneEllipse,
}

fn eval_candidate(
    pts: &[Point],
    face_pad: f64,
    owner_box: Option<Rect>,
    external_rects: &[Rect],
    theta: f64,
    du_shift: f64,
    dv_shift: f64,
) -> FitCandidate {
    let (cos_t, sin_t) = (theta.cos(), theta.sin());
    let (mut u_min, mut u_max) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut v_min, mut v_max) = (f64::INFINITY, f64::NEG_INFINITY);
    for &p in pts {
        let u = cos_t * p.x + sin_t * p.y;
        let v = -sin_t * p.x + cos_t * p.y;
        u_min = u_min.min(u);
        u_max = u_max.max(u);
        v_min = v_min.min(v);
        v_max = v_max.max(v);
    }
    let half_u = (u_max - u_min) / 2.;
    let half_v = (v_max - v_min) / 2.;
    let cu = (u_min + u_max) / 2. + du_shift * half_u;
    let cv = (v_min + v_max) / 2. + dv_shift * half_v;
    let cx = cos_t * cu - sin_t * cv;
    let cy = sin_t * cu + cos_t * cv;
    let min_rx = ((u_max - cu).max(cu - u_min) + face_pad).max(4.);
    let min_ry = ((v_max - cv).max(cv - v_min) + face_pad).max(4.);

    let sq_offsets: Vec<(f64, f64)> = pts
        .iter()
        .map(|&p| {
            let du = cos_t * p.x + sin_t * p.y - cu;
            let dv = -sin_t * p.x + cos_t * p.y - cv;
            (du * du, dv * dv)
        })
        .collect();

    let mut ext_pts = Vec::with_capacity(external_rects.len() * 5);
    for r in external_rects {
        let probes = [
            Point::new(r.left(), r.top()),
            Point::new(r.right(), r.top()),
            Point::new(r.right(), r.bottom()),
            Point::new(r.left(), r.bottom()),
            r.center(),
        ];
        for p in probes {
            ext_pts.push((
                cos_t * p.x + sin_t * p.y - cu,
                -sin_t * p.x + cos_t * p.y - cv,
            ));
        }
    }

    let center_shift_pen = 0.25 * ((du_shift * half_u).powi(2) + (dv_shift * half_v).powi(2));
    let min_rx_sq = min_rx * min_rx;
    let eval_s = |s: f64| -> (f64, f64, f64) {
        let mut req_rx_sq = min_rx_sq.max((min_ry * s).powi(2));
        let s_sq = s * s;
        for &(du2, dv2) in &sq_offsets {
            req_rx_sq = req_rx_sq.max(du2 + s_sq * dv2);
        }
        let rx = req_rx_sq.sqrt();
        let ry = rx / s;
        let half_w = (rx * cos_t).hypot(ry * sin_t);
        let half_h = (rx * sin_t).hypot(ry * cos_t);
        let left = cx - half_w;
        let right = cx + half_w;
        let top = cy - half_h;
        let bottom = cy + half_h;
        let mut cost = rx * ry + 20. * (rx + ry) + center_shift_pen;
        if let Some(ob) = owner_box {
            let spill = (ob.left() - left).max(0.)
                + (right - ob.right()).max(0.)
                + (ob.top() - top).max(0.)
                + (bottom - ob.bottom()).max(0.);
            cost += 200. * spill * spill;
        }
        let min_r = rx.min(ry);
        for &(eu, ev) in &ext_pts {
            let d = (eu / rx).hypot(ev / ry);
            if d < 1. {
                let pen = (1. - d) * min_r;
                cost += 400. * pen * pen;
            }
        }
        (cost, rx, ry)
    };

    // 1D golden-section search on log(rx / ry) in [ln(0.05), ln(20)].
    let phi = (5_f64.sqrt() - 1.) / 2.;
    let mut a = 0.05_f64.ln();
    let mut b = 20.0_f64.ln();
    let mut x1 = b - phi * (b - a);
    let mut x2 = a + phi * (b - a);
    let mut f1 = eval_s(x1.exp()).0;
    let mut f2 = eval_s(x2.exp()).0;
    for _ in 0..45 {
        if f1 < f2 {
            b = x2;
            x2 = x1;
            f2 = f1;
            x1 = b - phi * (b - a);
            f1 = eval_s(x1.exp()).0;
        } else {
            a = x1;
            x1 = x2;
            f1 = f2;
            x2 = a + phi * (b - a);
            f2 = eval_s(x2.exp()).0;
        }
    }
    let (cost, rx, ry) = eval_s(((a + b) / 2.).exp());
    FitCandidate {
        cost,
        ellipse: ZoneEllipse {
            cx,
            cy,
            rx,
            ry,
            theta,
        },
    }
}

fn fit_zone_ellipse(
    pts: &[Point],
    face_pad: f64,
    owner_box: Option<Rect>,
    external_rects: &[Rect],
) -> ZoneEllipse {
    const SHIFTS: [(f64, f64); 9] = [
        (0., 0.),
        (0.08, 0.),
        (-0.08, 0.),
        (0., 0.08),
        (0., -0.08),
        (0.16, 0.),
        (-0.16, 0.),
        (0., 0.16),
        (0., -0.16),
    ];
    let best_for_theta = |theta: f64| -> FitCandidate {
        let mut best = eval_candidate(pts, face_pad, owner_box, external_rects, theta, 0., 0.);
        for &(du, dv) in &SHIFTS[1..] {
            let cand = eval_candidate(pts, face_pad, owner_box, external_rects, theta, du, dv);
            if cand.cost < best.cost {
                best = cand;
            }
        }
        best
    };

    let axis0 = best_for_theta(0.);
    let mut best = axis0;
    const STEPS: usize = 60;
    for i in 1..STEPS {
        let theta = PI * (i as f64) / (STEPS as f64);
        let cand = best_for_theta(theta);
        if cand.cost < best.cost {
            best = cand;
        }
    }
    let coarse_theta = best.ellipse.theta;
    for step in -5..=5 {
        if step == 0 {
            continue;
        }
        let theta = (coarse_theta + (step as f64) * 0.5_f64.to_radians()).rem_euclid(PI);
        let cand = best_for_theta(theta);
        if cand.cost < best.cost {
            best = cand;
        }
    }

    let mut chosen = if axis0.cost <= best.cost * 1.05 {
        axis0.ellipse
    } else {
        best.ellipse
    };
    if chosen.ry > chosen.rx {
        std::mem::swap(&mut chosen.rx, &mut chosen.ry);
        chosen.theta = (chosen.theta + FRAC_PI_2).rem_euclid(PI);
    } else {
        chosen.theta = chosen.theta.rem_euclid(PI);
    }
    if chosen.theta < 1e-6 || (PI - chosen.theta).abs() < 1e-6 {
        chosen.theta = 0.;
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_strictly_convex(poly: &[Point]) -> bool {
        if poly.len() < 3 {
            return false;
        }
        let mut sign = 0.0_f64;
        for i in 0..poly.len() {
            let a = poly[i];
            let b = poly[(i + 1) % poly.len()];
            let c = poly[(i + 2) % poly.len()];
            let cross = (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x);
            if cross.abs() <= 1e-6 {
                return false;
            }
            if sign == 0. {
                sign = cross;
            } else if cross.signum() != sign.signum() {
                return false;
            }
        }
        true
    }

    #[test]
    fn intervening_non_members_stay_inside_one_solid_elliptical_contour() {
        for source in [
            "a @pin(grid: 0,0)\nb @pin(grid: 20,0)\nx @pin(grid: 10,0)\nzone scope around [a,b] { padding: 20 }\n",
            "a @pin(grid: 0, 0)\nblocker @pin(grid: 36, 0)\nb @pin(grid: 72, 0)\nzone wide around [a, b] { padding: 20 }\n",
        ] {
            let d = crate::compile(source).unwrap();
            let polygons = contours(&d.scene, &d.layout, &d.scene.zones[0]);
            assert_eq!(
                polygons.len(),
                1,
                "zone must form one solid elliptical contour"
            );
            assert!(
                is_strictly_convex(&polygons[0]),
                "elliptical contour must be strictly convex with no cutouts or snaking concavities"
            );
            let e = ellipse(&d.scene, &d.layout, &d.scene.zones[0]).unwrap();
            for id in d.scene.nodes.iter().map(|n| &n.id) {
                assert!(
                    e.contains(d.layout.nodes[id].center()),
                    "member or intervening node {id} must be inside the solid ellipse"
                );
            }
        }
    }

    #[test]
    fn diagonal_members_tilt_ellipse_and_exclude_off_axis_nodes() {
        let d = crate::compile(
            "a @pin(grid: 0, 0)\nb @pin(grid: 24, 24)\noff_hi @pin(grid: 24, 0)\noff_lo @pin(grid: 0, 24)\nzone diag around [a, b] { padding: 1.5g }\n",
        )
        .unwrap();
        let e = ellipse(&d.scene, &d.layout, &d.scene.zones[0]).unwrap();
        assert!(
            e.theta.abs() > 0.2 && (PI - e.theta).abs() > 0.2,
            "expected tilted ellipse for diagonal members, got theta={}",
            e.theta
        );
        assert!(e.contains(d.layout.nodes["a"].center()));
        assert!(e.contains(d.layout.nodes["b"].center()));
        assert!(
            !e.contains(d.layout.nodes["off_hi"].center()),
            "off_hi should remain outside tilted ellipse"
        );
        assert!(
            !e.contains(d.layout.nodes["off_lo"].center()),
            "off_lo should remain outside tilted ellipse"
        );
    }
}
