//! The level: a fence across the top of the field with a gap to herd cows
//! through — get a cow over the line and it's captured.
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
    /// The field edge. Drawn, but not simulated — `clamp_field` already stops
    /// anything walking off, so these exist to show the player where the world
    /// ends rather than to push anyone back.
    pub boundary: Vec<Fence>,
}

impl Level {
    pub fn build() -> Self {
        let g = GATE_HALF;
        let hx = FIELD_HALF_X;
        let z = PEN_LINE_Z;

        // The pen is one fence across the top of the field, split by the gap
        // around X = 0. Real collision (these live in `fences`), so a cow can
        // only get past the line by threading the gap.
        let fences = vec![
            Fence { a: Vec2::new(-hx, z), b: Vec2::new(-g, z) },
            Fence { a: Vec2::new(g, z), b: Vec2::new(hx, z) },
        ];

        // Gate posts flank the gap, built taller than the run so the opening the
        // cows have to be driven through reads at a glance.
        let posts = vec![Vec2::new(-g, z), Vec2::new(g, z)];

        // The field edge: the two long sides running from the pen line down to
        // the near edge, closed by a bottom run that shows beside the cabinet's
        // hump (the hump art hides its middle). Only the strip above the pen
        // line is open — that's where captured cows walk off to.
        let boundary = vec![
            Fence { a: Vec2::new(-hx, z), b: Vec2::new(-hx, FIELD_Z_NEAR) },
            Fence { a: Vec2::new(hx, z), b: Vec2::new(hx, FIELD_Z_NEAR) },
            Fence { a: Vec2::new(-hx, FIELD_Z_NEAR), b: Vec2::new(hx, FIELD_Z_NEAR) },
        ];

        Self { fences, posts, boundary }
    }

    /// A cow is captured once it's past the pen line — which, given the fence,
    /// means it made it through the gap. `p` is (X, Z) on the ground plane.
    pub fn penned(&self, p: Vec2) -> bool {
        p.y < PEN_LINE_Z
    }
}
