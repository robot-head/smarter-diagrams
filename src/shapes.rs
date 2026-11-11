//! Shared silhouette geometry for drawing nodes and attaching orthogonal routes.
use crate::{layout::*, model::Side};

pub(crate) const CYLINDER_RY: f64 = 8.;

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
