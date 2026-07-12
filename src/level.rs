//! The level: a fenced pen with a gate gap to herd cows through.
//!
//! Pure data + geometry helpers on the X/Z plane — no engine dependency beyond
//! glam math. Consumed both by the boid sim (avoidance) and the spawn system
//! (rendering the fences as 3D blocks).

use bevy::math::Vec2;

use crate::config::*;

/// A straight wall segment on the ground plane.
#[derive(Clone, Copy)]
pub struct Fence {
    pub a: Vec2,
    pub b: Vec2,
}

impl Fence {
    /// Closest point on the segment to `p` (for avoidance + collision).
    pub fn closest_point(&self, p: Vec2) -> Vec2 {
        let ab = self.b - self.a;
        let len_sq = ab.length_squared();
        let t = if len_sq > 1e-6 {
            ((p - self.a).dot(ab) / len_sq).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.a + ab * t
    }
}

pub struct Level {
    pub fences: Vec<Fence>,
    pub posts: Vec<Vec2>, // decorative gate posts
    pub pen_min: Vec2,
    pub pen_max: Vec2,
}

impl Level {
    pub fn build() -> Self {
        let (x0, x1, z0, z1) = (PEN_X0, PEN_X1, PEN_Z0, PEN_Z1);
        let g = GATE_HALF;

        let fences = vec![
            // Near wall (z1, nearest the herd), split by the gate gap around X = 0.
            Fence { a: Vec2::new(x0, z1), b: Vec2::new(-g, z1) },
            Fence { a: Vec2::new(g, z1), b: Vec2::new(x1, z1) },
            // Far wall (z0).
            Fence { a: Vec2::new(x0, z0), b: Vec2::new(x1, z0) },
            // Left + right walls.
            Fence { a: Vec2::new(x0, z0), b: Vec2::new(x0, z1) },
            Fence { a: Vec2::new(x1, z0), b: Vec2::new(x1, z1) },
        ];

        let posts = vec![Vec2::new(-g, z1), Vec2::new(g, z1)];

        Self {
            fences,
            posts,
            pen_min: Vec2::new(x0, z0),
            pen_max: Vec2::new(x1, z1),
        }
    }

    /// Is a ground position inside the pen rectangle?
    pub fn in_pen(&self, p: Vec2) -> bool {
        p.x >= self.pen_min.x
            && p.x <= self.pen_max.x
            && p.y >= self.pen_min.y
            && p.y <= self.pen_max.y
    }
}
