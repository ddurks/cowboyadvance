//! Per-frame gameplay systems: player movement, the boid herd, pen scoring, and
//! the HUD. What the characters *look* like doing it lives in `anim`.

use bevy::prelude::*;

use crate::components::*;
use crate::config::*;
use crate::sim::{boid_acceleration, clamp_field, resolve_fences};

/// Target yaw so the entity's local -Z (forward) points along its travel
/// direction. `None` when too slow to bother turning. `vel` is (X, Z).
fn travel_rotation(vel: Vec2) -> Option<Quat> {
    if vel.length_squared() > FACE_MIN_SPEED * FACE_MIN_SPEED {
        // -Z maps to (vel.x, vel.z) at yaw = atan2(-x, -z).
        Some(Quat::from_rotation_y((-vel.x).atan2(-vel.y)))
    } else {
        None
    }
}

pub fn player_movement(
    time: Res<Time>,
    inputs: Res<PlayerInputs>,
    level: Res<GameLevel>,
    mut q: Query<(&Player, &mut Transform, &mut Velocity)>,
) {
    let dt = time.delta_secs();
    for (player, mut tf, mut vel) in &mut q {
        let inp = inputs.0.get(player.index).copied().unwrap_or(Vec2::ZERO);
        // Input (x = right, y = forward) -> sim (x, z). Forward is -Z.
        let target = Vec2::new(inp.x, -inp.y) * PLAYER_SPEED;
        let mut v = vel.0.lerp(target, (PLAYER_ACCEL * dt).min(1.0));
        let mut pos = Vec2::new(tf.translation.x, tf.translation.z) + v * dt;
        resolve_fences(&mut pos, &mut v, PLAYER_RADIUS, &level.0.fences);
        clamp_field(&mut pos, PLAYER_RADIUS);
        vel.0 = v;
        tf.translation.x = pos.x;
        tf.translation.z = pos.y;
        if let Some(target) = travel_rotation(v) {
            tf.rotation = tf.rotation.slerp(target, (TURN_RATE * dt).min(1.0));
        }
    }
}

pub fn cow_boids(
    time: Res<Time>,
    level: Res<GameLevel>,
    players: Query<&Transform, (With<Player>, Without<Cow>)>,
    mut cows: Query<(&mut Transform, &mut Velocity, &mut Cow, &mut ModelTint), Without<Player>>,
) {
    let dt = time.delta_secs();

    let player_positions: Vec<Vec2> = players
        .iter()
        .map(|t| Vec2::new(t.translation.x, t.translation.z))
        .collect();

    // Pass 1: snapshot (ends the immutable borrow before we mutate).
    let snapshot: Vec<(Vec2, Vec2)> = cows
        .iter()
        .map(|(t, v, _, _)| (Vec2::new(t.translation.x, t.translation.z), v.0))
        .collect();

    // Pass 2: steer each cow against the snapshot (same iteration order).
    for (i, (mut tf, mut vel, mut cow, mut tint)) in cows.iter_mut().enumerate() {
        let acc = boid_acceleration(i, &snapshot, &player_positions, &level.0);
        let mut v = vel.0 + acc * dt;
        v *= 1.0 - (COW_DRAG * dt).min(1.0);
        v = v.clamp_length_max(COW_MAX_SPEED);
        let mut pos = Vec2::new(tf.translation.x, tf.translation.z) + v * dt;
        resolve_fences(&mut pos, &mut v, COW_RADIUS, &level.0.fences);
        clamp_field(&mut pos, COW_RADIUS);
        vel.0 = v;
        tf.translation.x = pos.x;
        tf.translation.z = pos.y;
        if let Some(target) = travel_rotation(v) {
            tf.rotation = tf.rotation.slerp(target, (TURN_RATE * dt).min(1.0));
        }

        let penned = level.0.in_pen(pos);
        if penned != cow.penned {
            cow.penned = penned;
            // `anim::apply_model_tint` repaints the hide next time it runs.
            tint.on = penned;
        }
    }
}

pub fn update_hud(cows: Query<&Cow>, mut text_q: Query<&mut Text, With<PenCounter>>) {
    let total = cows.iter().count();
    let penned = cows.iter().filter(|c| c.penned).count();
    for mut text in &mut text_q {
        if total > 0 && penned == total {
            **text = format!("Penned: {penned} / {total}  -  ALL PENNED!");
        } else {
            **text = format!("Penned: {penned} / {total}");
        }
    }
}

