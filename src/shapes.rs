//! Shared silhouette geometry for drawing nodes and attaching orthogonal routes.
use crate::{layout::*, model::Side};

pub(crate) const CYLINDER_RY: f64 = 8.;

pub(crate) fn title_gutter(shape: &str) -> f64 {
    match shape {
        "queue" => 32.,
        "actor" => 20.,
        _ => 0.,
    }
}
pub(crate) fn ray_attachment(shape: &str, r: Rect, toward: Point) -> Point {
    let (dx, dy) = (toward.x - r.x, toward.y - r.y);
    let length = dx.hypot(dy);
    if length < 1e-9 {
        return Point::new(r.x, r.bottom());
    }
    let inside = |p: Point| {
        if let Some(v) = polygon(shape, r) {
            return (0..4).all(|i| {
                let a = v[i];
                let b = v[(i + 1) % 4];
                (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x) >= -1e-9
            });
        }
        if shape == "database" {
            if p.x < r.left() || p.x > r.right() || p.y < r.top() || p.y > r.bottom() {
                return false;
            }
            let ry = CYLINDER_RY.min(r.h / 2.);
            let cy = p.y.clamp(r.top() + ry, r.bottom() - ry);
            return ((p.x - r.x) / (r.w / 2.)).powi(2) + ((p.y - cy) / ry).powi(2) <= 1.;
        }
        let radius: f64 = match shape {
            "actor" => 18.,
            "container" => 12.,
            "sql_table" => 2.,
            _ => 8.,
        };
        let radius = radius.min(r.w / 2.).min(r.h / 2.);
        let x = p.x.clamp(r.left() + radius, r.right() - radius);
        let y = p.y.clamp(r.top() + radius, r.bottom() - radius);
        (p.x - x).hypot(p.y - y) <= radius
    };
    let (mut low, mut high) = (0., r.w.hypot(r.h));
    for _ in 0..48 {
        let mid = (low + high) / 2.;
        if inside(r.center().offset(dx / length * mid, dy / length * mid)) {
            low = mid
        } else {
            high = mid
        }
    }
    r.center().offset(dx / length * low, dy / length * low)
}

pub(crate) fn outline(shape: &str, r: Rect) -> String {
    if let Some(points) = polygon(shape, r) {
        return format!("{}Z", crate::render::path(&points));
    }
    if shape == "database" {
        let ry = CYLINDER_RY.min(r.h / 2.);
        return format!(
            "M {} {} A {} {ry} 0 0 1 {} {} L {} {} A {} {ry} 0 0 1 {} {} Z",
            r.left(),
            r.top() + ry,
            r.w / 2.,
            r.right(),
            r.top() + ry,
            r.right(),
            r.bottom() - ry,
            r.w / 2.,
            r.left(),
            r.bottom() - ry
        );
    }
    let radius: f64 = match shape {
        "actor" => 18.,
        "container" => 12.,
        "sql_table" => 2.,
        _ => 8.,
    };
    let radius = radius.min(r.w / 2.).min(r.h / 2.);
    format!(
        "M {} {} H {} A {radius} {radius} 0 0 1 {} {} V {} A {radius} {radius} 0 0 1 {} {} H {} A {radius} {radius} 0 0 1 {} {} V {} A {radius} {radius} 0 0 1 {} {} Z",
        r.left() + radius,
        r.top(),
        r.right() - radius,
        r.right(),
        r.top() + radius,
        r.bottom() - radius,
        r.right() - radius,
        r.bottom(),
        r.left() + radius,
        r.left(),
        r.bottom() - radius,
        r.top() + radius,
        r.left() + radius,
        r.top()
    )
}

pub(crate) fn polygon(shape: &str, r: Rect) -> Option<[Point; 4]> {
    Some(match shape {
        "diamond" | "decision" => [
            Point::new(r.x, r.top()),
            Point::new(r.right(), r.y),
            Point::new(r.x, r.bottom()),
            Point::new(r.left(), r.y),
        ],
        "cache" => [
            Point::new(r.left() + 10., r.top()),
            Point::new(r.right(), r.top()),
            Point::new(r.right() - 10., r.bottom()),
            Point::new(r.left(), r.bottom()),
        ],
        _ => return None,
    })
}

/// Intersect a cardinal ray with the drawn outline, retaining the requested
/// coordinate along that side. The router's stub still lies outside the box.
pub(crate) fn attachment(shape: &str, r: Rect, side: Side, offset: f64) -> Point {
    let horizontal = matches!(side, Side::E | Side::W);
    let mut p = match side {
        Side::N => Point::new(r.left() + offset, r.top()),
        Side::E => Point::new(r.right(), r.top() + offset),
        Side::S => Point::new(r.left() + offset, r.bottom()),
        Side::W => Point::new(r.left(), r.top() + offset),
    };
    if let Some(vertices) = polygon(shape, r) {
        let mut hits = Vec::new();
        for i in 0..vertices.len() {
            let (a, b) = (vertices[i], vertices[(i + 1) % vertices.len()]);
            let (start, end, at) = if horizontal {
                (a.y, b.y, p.y)
            } else {
                (a.x, b.x, p.x)
            };
            if (end - start).abs() < 1e-9
                || at < start.min(end) - 1e-9
                || at > start.max(end) + 1e-9
            {
                continue;
            }
            let t = (at - start) / (end - start);
            hits.push(if horizontal {
                a.x + (b.x - a.x) * t
            } else {
                a.y + (b.y - a.y) * t
            });
        }
        let coordinate = if matches!(side, Side::E | Side::S) {
            hits.into_iter().reduce(f64::max)
        } else {
            hits.into_iter().reduce(f64::min)
        };
        if let Some(value) = coordinate {
            if horizontal {
                p.x = value;
            } else {
                p.y = value;
            }
        }
    } else if shape == "database" {
        let ry = CYLINDER_RY.min(r.h / 2.);
        if horizontal {
            let dy = (p.y - p.y.clamp(r.top() + ry, r.bottom() - ry)) / ry;
            let dx = r.w / 2. * (1. - dy * dy).max(0.).sqrt();
            p.x = r.x + if side == Side::E { dx } else { -dx };
        } else {
            let dx = (p.x - r.x) / (r.w / 2.);
            let dy = ry * (1. - dx * dx).max(0.).sqrt();
            p.y = if side == Side::N {
                r.top() + ry - dy
            } else {
                r.bottom() - ry + dy
            };
        }
    } else {
        let radius: f64 = match shape {
            "actor" => 18.,
            "container" => 12.,
            "sql_table" => 2.,
            _ => 8.,
        };
        let radius = radius.min(r.w / 2.).min(r.h / 2.);
        if horizontal {
            let dy = p.y - p.y.clamp(r.top() + radius, r.bottom() - radius);
            let inset = radius - (radius * radius - dy * dy).max(0.).sqrt();
            p.x += if side == Side::E { -inset } else { inset };
        } else {
            let dx = p.x - p.x.clamp(r.left() + radius, r.right() - radius);
            let inset = radius - (radius * radius - dx * dx).max(0.).sqrt();
            p.y += if side == Side::N { inset } else { -inset };
        }
    }
    p
}
