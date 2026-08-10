//! The herding simulation — pure functions, no ECS, no engine types.
//!
//! Everything here operates on plain `Vec2` data on the X/Z plane. Bevy systems
//! collect entity state into slices, call these, and write results back. Keeping
//! the sim engine-agnostic is what lets it run unchanged when we add networked
//! coop (feed remote player positions into `players`).

use bevy::math::Vec2;

use crate::config::*;
use crate::level::{Fence, Level};

/// Steering acceleration for cow `i`, given a snapshot of all cows (position +
/// velocity), all player positions, and the level geometry.
///
/// Classic Reynolds boids (separation / alignment / cohesion) plus two
/// game-specific forces: fleeing players (the herding push) and avoiding
/// fences + the field edge.
pub fn boid_acceleration(
    i: usize,
    cows: &[(Vec2, Vec2)],
    players: &[Vec2],
    level: &Level,
) -> Vec2 {
    let (pos, _vel) = cows[i];

    let mut separation = Vec2::ZERO;
    let mut avg_vel = Vec2::ZERO;
    let mut avg_pos = Vec2::ZERO;
    let mut neighbors = 0.0f32;

    for (j, &(other_pos, other_vel)) in cows.iter().enumerate() {
        if j == i {
            continue;
        }
        let d = pos - other_pos;
        let dist = d.length();
        if dist < NEIGHBOR_RADIUS && dist > 1e-4 {
            avg_vel += other_vel;
            avg_pos += other_pos;
            neighbors += 1.0;
            if dist < SEPARATION_RADIUS {
                separation += (d / dist) * (1.0 - dist / SEPARATION_RADIUS);
            }
        }
    }

    let mut acc = separation * W_SEPARATION;

    if neighbors > 0.0 {
        // Alignment: match the herd's heading.
        acc += (avg_vel / neighbors).normalize_or_zero() * W_ALIGNMENT;
        // Cohesion: drift toward the herd's centre.
        let center = avg_pos / neighbors;
        acc += (center - pos).normalize_or_zero() * W_COHESION;
    }

    // Flee from players — this is what lets the cowboy steer the herd.
    for &pp in players {
        let d = pos - pp;
        let dist = d.length();
        if dist < FLEE_RADIUS && dist > 1e-4 {
            acc += (d / dist) * (1.0 - dist / FLEE_RADIUS) * W_FLEE;
        }
    }

    // Steer away from fences before hitting them.
    for f in &level.fences {
        let cp = f.closest_point(pos);
        let d = pos - cp;
        let dist = d.length();
        if dist < FENCE_AVOID && dist > 1e-4 {
            acc += (d / dist) * (1.0 - dist / FENCE_AVOID) * W_FENCE;
        }
    }

    acc + boundary_force(pos)
}

/// Soft inward push when a position strays near the field edge.
pub fn boundary_force(pos: Vec2) -> Vec2 {
    let m = BOUNDARY_MARGIN;
    let mut f = Vec2::ZERO;
    if pos.x > FIELD_HALF_X - m {
        f.x -= (pos.x - (FIELD_HALF_X - m)) / m;
    }
    if pos.x < -FIELD_HALF_X + m {
        f.x += ((-FIELD_HALF_X + m) - pos.x) / m;
    }
    if pos.y > FIELD_Z_NEAR - m {
        f.y -= (pos.y - (FIELD_Z_NEAR - m)) / m;
    }
    if pos.y < -FIELD_HALF_Z + m {
        f.y += ((-FIELD_HALF_Z + m) - pos.y) / m;
    }
    f * W_BOUNDARY
}

/// Push a circle of `radius` out of any fence it overlaps and kill the velocity
/// component driving it into the wall. Shared by players and cows.
pub fn resolve_fences(pos: &mut Vec2, vel: &mut Vec2, radius: f32, fences: &[Fence]) {
    let r = radius + FENCE_THICK * 0.5;
    for f in fences {
        let cp = f.closest_point(*pos);
        let d = *pos - cp;
        let dist = d.length();
        if dist < r {
            if dist > 1e-4 {
                let n = d / dist;
                *pos = cp + n * r;
                let into = vel.dot(n);
                if into < 0.0 {
                    *vel -= n * into;
                }
            } else {
                *pos += Vec2::new(r, 0.0);
            }
        }
    }
}

/// Keep a position inside the field bounds.
pub fn clamp_field(pos: &mut Vec2, radius: f32) {
    pos.x = pos.x.clamp(-FIELD_HALF_X + radius, FIELD_HALF_X - radius);
    pos.y = pos.y.clamp(-FIELD_HALF_Z + radius, FIELD_Z_NEAR - radius);
}
