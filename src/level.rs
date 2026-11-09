//! The level: a corral at the centre of the range with a gate in each side —
//! get a cow through one and it's penned.
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
    pub posts: Vec<Vec2>, // gate posts, built taller than the run
}

impl Level {
    pub fn build() -> Self {
        let g = GATE_HALF;
        let h = CORRAL_HALF;

        // Each side of the corral is two runs split by a gate at its middle.
        // Real collision, so a cow only gets in by threading a gap.
        let fences = vec![
            Fence { a: Vec2::new(-h, -h), b: Vec2::new(-g, -h) },
            Fence { a: Vec2::new(g, -h), b: Vec2::new(h, -h) },
            Fence { a: Vec2::new(-h, h), b: Vec2::new(-g, h) },
            Fence { a: Vec2::new(g, h), b: Vec2::new(h, h) },
            Fence { a: Vec2::new(-h, -h), b: Vec2::new(-h, -g) },
            Fence { a: Vec2::new(-h, g), b: Vec2::new(-h, h) },
            Fence { a: Vec2::new(h, -h), b: Vec2::new(h, -g) },
            Fence { a: Vec2::new(h, g), b: Vec2::new(h, h) },
        ];

        let posts = vec![
            Vec2::new(-g, -h),
            Vec2::new(g, -h),
            Vec2::new(-g, h),
            Vec2::new(g, h),
            Vec2::new(-h, -g),
            Vec2::new(-h, g),
            Vec2::new(h, -g),
            Vec2::new(h, g),
        ];

        Self { fences, posts }
    }

    /// A cow is penned once it is well inside the corral — which, given the
    /// fences, means it came through a gate. `p` is (X, Z) on the ground plane.
    pub fn penned(&self, p: Vec2) -> bool {
        let inside = CORRAL_HALF - PEN_COMMIT;
        p.x.abs() < inside && p.y.abs() < inside
    }
}
