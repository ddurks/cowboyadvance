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
    cows: &[(Vec2, Vec2, bool)],
    players: &[Vec2],
    level: &Level,
) -> Vec2 {
    let (pos, _vel, _penned) = cows[i];

    let mut separation = Vec2::ZERO;
    let mut avg_vel = Vec2::ZERO;
    let mut avg_pos = Vec2::ZERO;
    let mut neighbors = 0.0f32;

    for (j, &(other_pos, other_vel, other_penned)) in cows.iter().enumerate() {
        // Captured cows have left the herd — skip them so they stop tugging the
        // ones still in play toward the exit.
        if j == i || other_penned {
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
        acc += (avg_vel / neighbors).normalize_or_zero() * W_ALIGNMENT;
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
    // Only the near (+Z) edge pushes back. The far (-Z) edge is the pen line the
    // cows are meant to cross, so there is deliberately no push there.
    if pos.y > FIELD_Z_NEAR - m {
        f.y -= (pos.y - (FIELD_Z_NEAR - m)) / m;
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

pub fn clamp_field(pos: &mut Vec2, radius: f32) {
    pos.x = pos.x.clamp(-FIELD_HALF_X + radius, FIELD_HALF_X - radius);
    pos.y = pos.y.clamp(-FIELD_HALF_Z + radius, FIELD_Z_NEAR - radius);
}

/// A colliding mover on the ground plane (cow, or horse + rider).
pub struct Body {
    pub pos: Vec2,
    pub vel: Vec2,
    pub radius: f32,
    pub mass: f32,
}

/// Circle-circle collisions between every pair of bodies: ease overlaps apart
/// (split by inverse mass) and exchange an impulse along the contact normal.
///
/// Returns each body's hardest closing speed this frame (0 if untouched) — the
/// number gameplay compares against the throw thresholds. Pairs resolve in
/// index order, sequentially, so the result is deterministic.
pub fn resolve_body_collisions(bodies: &mut [Body]) -> Vec<f32> {
    let n = bodies.len();
    let mut impact = vec![0.0f32; n];
    for i in 0..n {
        for j in (i + 1)..n {
            let d = bodies[j].pos - bodies[i].pos;
            let r = bodies[i].radius + bodies[j].radius;
            let dist_sq = d.length_squared();
            if dist_sq >= r * r || dist_sq < 1e-8 {
                continue;
            }
            let dist = dist_sq.sqrt();
            let normal = d / dist; // i -> j
            let inv_i = 1.0 / bodies[i].mass;
            let inv_j = 1.0 / bodies[j].mass;
            let inv_sum = inv_i + inv_j;

            let push = (r - dist) * BODY_RELAX / inv_sum;
            bodies[i].pos -= normal * push * inv_i;
            bodies[j].pos += normal * push * inv_j;

            // Impulse only while actually closing, so resting contact doesn't
            // jitter.
            let closing = (bodies[i].vel - bodies[j].vel).dot(normal);
            if closing <= 0.0 {
                continue;
            }
            impact[i] = impact[i].max(closing);
            impact[j] = impact[j].max(closing);
            let magnitude = (1.0 + BODY_RESTITUTION) * closing / inv_sum;
            bodies[i].vel -= normal * magnitude * inv_i;
            bodies[j].vel += normal * magnitude * inv_j;
        }
    }
    impact
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(pos: Vec2, vel: Vec2, mass: f32) -> Body {
        Body { pos, vel, radius: 1.0, mass }
    }

    #[test]
    fn head_on_impact_reports_closing_speed_and_reverses() {
        let mut bodies = vec![
            body(Vec2::new(-0.9, 0.0), Vec2::new(10.0, 0.0), 1.0),
            body(Vec2::new(0.9, 0.0), Vec2::new(-10.0, 0.0), 1.0),
        ];
        let impact = resolve_body_collisions(&mut bodies);
        assert!((impact[0] - 20.0).abs() < 1e-3);
        assert!((impact[1] - 20.0).abs() < 1e-3);
        // Equal masses swap momentum, scaled by restitution.
        assert!(bodies[0].vel.x < 0.0 && bodies[1].vel.x > 0.0);
        assert!((bodies[0].vel.x + bodies[1].vel.x).abs() < 1e-3, "momentum conserved");
    }

    #[test]
    fn resting_overlap_separates_without_impulse() {
        let mut bodies = vec![
            body(Vec2::new(-0.5, 0.0), Vec2::ZERO, 1.0),
            body(Vec2::new(0.5, 0.0), Vec2::ZERO, 1.0),
        ];
        let impact = resolve_body_collisions(&mut bodies);
        assert_eq!(impact, vec![0.0, 0.0]);
        assert!(bodies[1].pos.x - bodies[0].pos.x > 1.0, "overlap eased apart");
        assert_eq!(bodies[0].vel, Vec2::ZERO);
    }

    #[test]
    fn heavier_body_budges_less() {
        let mut bodies = vec![
            body(Vec2::new(-0.9, 0.0), Vec2::new(10.0, 0.0), 1.0),
            body(Vec2::new(0.9, 0.0), Vec2::ZERO, PLAYER_MASS),
        ];
        resolve_body_collisions(&mut bodies);
        assert!(bodies[1].vel.x > 0.0 && bodies[1].vel.x < bodies[0].vel.x.abs() + 10.0);
        assert!(
            bodies[1].vel.x < 10.0 / PLAYER_MASS * (1.0 + BODY_RESTITUTION),
            "light cow can't fling the heavy horse at its own speed"
        );
    }
}
