//! The herding simulation — pure functions, no ECS, no engine types.
//!
//! Everything here operates on plain `Vec2` data on the X/Z plane. Bevy systems
//! collect entity state into slices, call these, and write results back. Keeping
//! the sim engine-agnostic is what lets it run unchanged when we add networked
//! coop (feed remote player positions into `players`).

use bevy::math::Vec2;

use crate::config::*;
use crate::level::{Fence, Level};

/// One cow as the flocking pass sees it.
#[derive(Clone, Copy)]
pub struct Boid {
    pub pos: Vec2,
    pub vel: Vec2,
    pub penned: bool,
    /// Rider index whose herd this is; `None` for wild cattle.
    pub owner: Option<usize>,
    /// Herding pressure: direction the cow is being pushed, length 0..=1. Set
    /// by a nearby rider, then relayed cow to cow so pushing the back of a herd
    /// moves the whole herd instead of crushing its near edge.
    pub drive: Vec2,
}

impl Boid {
    /// Whether two cows flock as one herd. Penned and loose cows never do: the
    /// penned herd mills in the corral and shouldn't tug loose cows against
    /// the fence. Out on the range each rider's herd, and the wild cattle, keep
    /// to themselves so herds stay distinct when they brush past each other.
    fn same_flock(&self, other: &Boid) -> bool {
        self.penned == other.penned && (self.penned || self.owner == other.owner)
    }
}

/// Steering acceleration and next `drive` for cow `i`, given a snapshot of all
/// cows, all player positions, and the level geometry.
///
/// Classic Reynolds boids (separation / alignment / cohesion) plus
/// game-specific forces: fleeing players (the herding push), the relayed drive,
/// and avoiding fences + the river bank.
pub fn boid_acceleration(
    i: usize,
    cows: &[Boid],
    players: &[Vec2],
    level: &Level,
    dt: f32,
) -> (Vec2, Vec2) {
    let me = cows[i];
    let pos = me.pos;

    let mut separation = Vec2::ZERO;
    let mut avg_vel = Vec2::ZERO;
    let mut avg_pos = Vec2::ZERO;
    let mut neighbors = 0.0f32;
    let mut relay_dir = Vec2::ZERO;
    let mut relay_max = 0.0f32;

    for (j, other) in cows.iter().enumerate() {
        if j == i || other.penned != me.penned {
            continue;
        }
        let d = pos - other.pos;
        let dist = d.length();
        // Every cow keeps its distance, whoever's herd it is in.
        if dist < SEPARATION_RADIUS && dist > 1e-4 {
            separation += (d / dist) * (1.0 - dist / SEPARATION_RADIUS);
        }
        if dist < NEIGHBOR_RADIUS && dist > 1e-4 && me.same_flock(other) {
            avg_vel += other.vel;
            avg_pos += other.pos;
            neighbors += 1.0;
            // Strength from the most-pushed neighbour, heading from the blend of
            // all of them, so a herd pushed from one flank still moves as one.
            relay_dir += other.drive;
            relay_max = relay_max.max(other.drive.length());
        }
    }

    // Flee from players — this is what lets the cowboy steer the herd.
    let mut flee = Vec2::ZERO;
    let mut pushed = Vec2::ZERO;
    for &pp in players {
        let d = pos - pp;
        let dist = d.length();
        if dist < FLEE_RADIUS && dist > 1e-4 {
            let falloff = 1.0 - dist / FLEE_RADIUS;
            flee += (d / dist) * falloff * W_FLEE;
            pushed += (d / dist) * (falloff * DRIVE_FROM_RIDER).min(1.0);
        }
    }
    let relayed = relay_dir.normalize_or_zero() * relay_max * DRIVE_RELAY;
    let target = if pushed.length() >= relayed.length() {
        pushed.clamp_length_max(1.0)
    } else {
        relayed
    };
    let rate = if target.length() > me.drive.length() { DRIVE_RISE } else { DRIVE_FALL };
    let drive = me.drive.lerp(target, 1.0 - (-rate * dt).exp());
    let pressure = drive.length();

    let mut acc = separation * W_SEPARATION + flee + drive * W_DRIVE;

    if neighbors > 0.0 {
        acc += (avg_vel / neighbors).normalize_or_zero() * W_ALIGNMENT;
        // Pressed cattle bunch up: the harder the push, the tighter the herd
        // holds, so a big herd doesn't spill around the rider's flanks.
        let center = avg_pos / neighbors;
        acc += (center - pos).normalize_or_zero() * (W_COHESION + pressure * W_DRIVE_COHESION);
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

    (acc + boundary_force(pos), drive)
}

/// Size of the herd each cow belongs to: cows of one flock chained together
/// by `NEIGHBOR_RADIUS` count as one herd.
pub fn herd_sizes(cows: &[Boid]) -> Vec<usize> {
    let mut parent: Vec<usize> = (0..cows.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for i in 0..cows.len() {
        for j in i + 1..cows.len() {
            if cows[i].same_flock(&cows[j])
                && cows[i].pos.distance_squared(cows[j].pos) < NEIGHBOR_RADIUS * NEIGHBOR_RADIUS
            {
                let (a, b) = (root(&mut parent, i), root(&mut parent, j));
                parent[a] = b;
            }
        }
    }
    let roots: Vec<usize> = (0..cows.len()).map(|i| root(&mut parent, i)).collect();
    let mut count = vec![0; cows.len()];
    for &r in &roots {
        count[r] += 1;
    }
    roots.iter().map(|&r| count[r]).collect()
}

/// Owner of each cow after this frame's claims. A loose wild cow joins the
/// herd of a rider within `JOIN_RADIUS_RIDER`, or of a loose cow already in
/// that rider's herd within `JOIN_RADIUS_HERD`; the nearest claim wins. Claims
/// read the snapshot, so a touched wild group joins a cow-hop per frame.
/// `riders` is (player index, position) for riders able to claim.
pub fn claim_owners(cows: &[Boid], riders: &[(usize, Vec2)]) -> Vec<Option<usize>> {
    cows.iter()
        .map(|me| {
            if me.penned || me.owner.is_some() {
                return me.owner;
            }
            let by_rider = riders
                .iter()
                .map(|&(index, p)| (index, p.distance(me.pos), JOIN_RADIUS_RIDER));
            let by_herd = cows
                .iter()
                .filter(|c| !c.penned)
                .filter_map(|c| Some((c.owner?, c.pos.distance(me.pos), JOIN_RADIUS_HERD)));
            by_rider
                .chain(by_herd)
                .filter(|&(_, dist, reach)| dist < reach)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(index, _, _)| index)
        })
        .collect()
}

/// Top speed for a cow in a herd of `size`: big herds lumber, which keeps them
/// together under the rider and is the herd's weight in the spec.
pub fn herd_max_speed(size: usize) -> f32 {
    let scale = 1.0 / (1.0 + (size.max(1) - 1) as f32 / HERD_SLOW_SIZE);
    COW_MAX_SPEED * scale.max(HERD_MIN_SPEED_FRAC)
}

/// Soft inward push when a position strays near the river bank.
pub fn boundary_force(pos: Vec2) -> Vec2 {
    let edge = RANGE_RADIUS - BOUNDARY_MARGIN;
    let r = pos.length();
    if r <= edge {
        return Vec2::ZERO;
    }
    -pos / r * ((r - edge) / BOUNDARY_MARGIN) * W_BOUNDARY
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

/// Keep a circle of `radius` on the near side of the river.
pub fn clamp_range(pos: &mut Vec2, radius: f32) {
    let max = RANGE_RADIUS - radius;
    if pos.length_squared() > max * max {
        *pos = pos.normalize() * max;
    }
}

/// Keep a penned cow inside the corral — gates included, so the herd you
/// brought home stays home.
pub fn confine_to_corral(pos: &mut Vec2, radius: f32) {
    let inside = CORRAL_HALF - radius - FENCE_THICK * 0.5;
    pos.x = pos.x.clamp(-inside, inside);
    pos.y = pos.y.clamp(-inside, inside);
}

/// Turn a raw stick vector into a riding intent: still inside the deadzone,
/// otherwise a full gallop in the stick's direction (see `STICK_DEADZONE`).
pub fn shape_ride_input(stick: Vec2) -> Vec2 {
    if stick.length() <= STICK_DEADZONE {
        return Vec2::ZERO;
    }
    stick.normalize()
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
    fn stick_is_stop_or_gallop_like_the_keyboard() {
        assert_eq!(shape_ride_input(Vec2::new(STICK_DEADZONE * 0.5, 0.0)), Vec2::ZERO);
        assert_eq!(shape_ride_input(Vec2::new(0.0, 0.3)), Vec2::Y);
        assert_eq!(shape_ride_input(Vec2::new(-1.0, 0.0)), Vec2::NEG_X);
        let keyboard_diagonal = Vec2::new(1.0, 1.0).normalize();
        assert!(shape_ride_input(keyboard_diagonal).abs_diff_eq(keyboard_diagonal, 1e-6));
    }

    #[test]
    fn drive_relays_through_the_herd_and_fades() {
        let level = Level { fences: Vec::new(), posts: Vec::new() };
        let cow = |x: f32, drive: Vec2| Boid { pos: Vec2::new(x, 0.0), vel: Vec2::ZERO, penned: false, owner: None, drive };
        let mut herd = vec![cow(0.0, Vec2::X), cow(4.0, Vec2::ZERO)];
        for _ in 0..60 {
            herd[1].drive = boid_acceleration(1, &herd, &[], &level, 1.0 / 60.0).1;
        }
        assert!(herd[1].drive.x > DRIVE_RELAY * 0.9, "far cow picks up its neighbour's push");
        assert!(herd[1].drive.y.abs() < 1e-4);

        herd[0].drive = Vec2::ZERO;
        let faded = boid_acceleration(1, &herd, &[], &level, 1.0).1;
        assert!(faded.length() < herd[1].drive.length(), "drive fades once the push stops");
    }

    #[test]
    fn bigger_herds_are_slower() {
        let cow = |x: f32| Boid { pos: Vec2::new(x, 0.0), vel: Vec2::ZERO, penned: false, owner: None, drive: Vec2::ZERO };
        let cows = [cow(0.0), cow(4.0), cow(8.0), cow(50.0)];
        assert_eq!(herd_sizes(&cows), vec![3, 3, 3, 1]);
        assert_eq!(herd_max_speed(1), COW_MAX_SPEED);
        assert!(herd_max_speed(30) < herd_max_speed(10));
        assert_eq!(herd_max_speed(10_000), COW_MAX_SPEED * HERD_MIN_SPEED_FRAC);
    }

    #[test]
    fn wild_cows_join_by_touch_and_chain_through_the_herd() {
        let cow = |x: f32, owner: Option<usize>| Boid {
            pos: Vec2::new(x, 0.0),
            vel: Vec2::ZERO,
            penned: false,
            owner,
            drive: Vec2::ZERO,
        };
        let mut cows = vec![cow(2.0, None), cow(4.5, None), cow(30.0, None), cow(33.0, Some(1))];
        let riders = [(0, Vec2::ZERO), (1, Vec2::new(40.0, 0.0))];
        let owners = claim_owners(&cows, &riders);
        assert_eq!(owners, vec![Some(0), None, Some(1), Some(1)], "rider and herd touches claim");
        for (c, o) in cows.iter_mut().zip(owners) {
            c.owner = o;
        }
        assert_eq!(claim_owners(&cows, &riders)[1], Some(0), "next frame the chain reaches on");
    }

    #[test]
    fn range_clamp_and_push_point_inward() {
        let mut p = Vec2::new(RANGE_RADIUS + 5.0, 0.0);
        clamp_range(&mut p, 1.0);
        assert!((p.length() - (RANGE_RADIUS - 1.0)).abs() < 1e-3);
        assert!(boundary_force(Vec2::new(0.0, -(RANGE_RADIUS - 1.0))).y > 0.0);
        assert_eq!(boundary_force(Vec2::new(10.0, 10.0)), Vec2::ZERO);
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
