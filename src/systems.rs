//! Per-frame gameplay systems: player movement, the boid herd, pen scoring, the
//! follow camera, and the HUD. What the characters *look* like doing it lives in `anim`.

use bevy::prelude::*;

use crate::components::*;
use crate::config::*;
use crate::sim::{
    boid_acceleration, clamp_range, confine_to_corral, herd_max_speed, herd_sizes,
    resolve_body_collisions, resolve_fences, shape_ride_input, Body, Boid,
};

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
    mut q: Query<(&Player, &mut Transform, &mut Velocity), Without<Downed>>,
) {
    let dt = time.delta_secs();
    for (player, mut tf, mut vel) in &mut q {
        let inp = shape_ride_input(inputs.0.get(player.index).copied().unwrap_or(Vec2::ZERO));
        // Input (x = right, y = forward) -> sim (x, z). Forward is -Z.
        let target = Vec2::new(inp.x, -inp.y) * PLAYER_SPEED;
        let mut v = vel.0.lerp(target, (PLAYER_ACCEL * dt).min(1.0));
        let mut pos = Vec2::new(tf.translation.x, tf.translation.z) + v * dt;
        resolve_fences(&mut pos, &mut v, PLAYER_RADIUS, &level.0.fences);
        clamp_range(&mut pos, PLAYER_RADIUS);
        vel.0 = v;
        tf.translation.x = pos.x;
        tf.translation.z = pos.y;
        if let Some(target) = travel_rotation(v) {
            tf.rotation = tf.rotation.slerp(target, (TURN_RATE * dt).min(1.0));
        }
    }
}

// The query type is a normal Bevy filter combination, not real complexity.
#[allow(clippy::type_complexity)]
pub fn cow_boids(
    time: Res<Time>,
    level: Res<GameLevel>,
    players: Query<(&Transform, Option<&Downed>), (With<Player>, Without<Cow>)>,
    mut cows: Query<(&mut Transform, &mut Velocity, &mut Cow), Without<Player>>,
) {
    let dt = time.delta_secs();

    // A downed player whose horse also went flying leaves nothing standing to
    // avoid; a riderless horse still spooks cows.
    let player_positions: Vec<Vec2> = players
        .iter()
        .filter(|(_, downed)| !downed.is_some_and(|d| d.horse_gone))
        .map(|(t, _)| Vec2::new(t.translation.x, t.translation.z))
        .collect();

    // Pass 1: snapshot (ends the immutable borrow before we mutate). The penned
    // flag rides along so the corral herd and the loose cattle flock apart.
    let snapshot: Vec<Boid> = cows
        .iter()
        .map(|(t, v, c)| Boid {
            pos: Vec2::new(t.translation.x, t.translation.z),
            vel: v.0,
            penned: c.penned,
            drive: c.drive,
        })
        .collect();
    let herd_size = herd_sizes(&snapshot);

    // Pass 2: steer each cow against the snapshot (same iteration order).
    for (i, (mut tf, mut vel, mut cow)) in cows.iter_mut().enumerate() {
        let (acc, drive) = boid_acceleration(i, &snapshot, &player_positions, &level.0, dt);
        cow.drive = drive;
        let mut v = vel.0 + acc * dt;
        v *= 1.0 - (COW_DRAG * dt).min(1.0);
        v = v.clamp_length_max(herd_max_speed(herd_size[i]));
        let mut pos = Vec2::new(tf.translation.x, tf.translation.z) + v * dt;
        resolve_fences(&mut pos, &mut v, COW_RADIUS, &level.0.fences);
        keep_in_bounds(&mut pos, COW_RADIUS, cow.penned);
        vel.0 = v;
        tf.translation.x = pos.x;
        tf.translation.z = pos.y;
        if let Some(target) = travel_rotation(v) {
            tf.rotation = tf.rotation.slerp(target, (TURN_RATE * dt).min(1.0));
        }

        if level.0.penned(pos) {
            cow.penned = true;
        }
    }
}

/// Penned cows stay in the corral; everything else stays on the range.
fn keep_in_bounds(pos: &mut Vec2, radius: f32, penned: bool) {
    if penned {
        confine_to_corral(pos, radius);
    } else {
        clamp_range(pos, radius);
    }
}

/// Hard circle-circle collisions between every mover (players + unpenned cows),
/// run after both movement systems so it resolves the positions they just wrote.
///
/// Impacts harder than the throw thresholds queue a `Throw` for `ragdoll` to
/// act on. Skipped: downed players whose horse is gone (nothing is standing
/// there).
#[allow(clippy::type_complexity)]
pub fn body_collisions(
    level: Res<GameLevel>,
    mut throws: ResMut<PendingThrows>,
    mut players: Query<
        (Entity, &mut Transform, &mut Velocity, Option<&Downed>),
        (With<Player>, Without<Cow>),
    >,
    mut cows: Query<(&mut Transform, &mut Velocity, &Cow), Without<Player>>,
) {
    let alive_player = |downed: &Option<&Downed>| !downed.as_ref().is_some_and(|d| d.horse_gone);

    let mut bodies: Vec<Body> = Vec::new();
    let mut player_meta: Vec<(Entity, Vec2, bool)> = Vec::new(); // (entity, pre_vel, downed)
    for (entity, tf, vel, downed) in players.iter() {
        if !alive_player(&downed) {
            continue;
        }
        player_meta.push((entity, vel.0, downed.is_some()));
        bodies.push(Body {
            pos: Vec2::new(tf.translation.x, tf.translation.z),
            vel: vel.0,
            radius: PLAYER_RADIUS,
            mass: PLAYER_MASS,
        });
    }
    let player_count = bodies.len();
    for (tf, vel, _) in cows.iter() {
        bodies.push(Body {
            pos: Vec2::new(tf.translation.x, tf.translation.z),
            vel: vel.0,
            radius: COW_RADIUS,
            mass: COW_MASS,
        });
    }

    let impacts = resolve_body_collisions(&mut bodies);

    // Write back in the same iteration order the snapshot was taken in, keeping
    // resolved bodies out of the fences a shove may have pushed them into.
    let write = |tf: &mut Transform, vel: &mut Velocity, body: &Body, penned: bool| {
        let (mut pos, mut v) = (body.pos, body.vel);
        resolve_fences(&mut pos, &mut v, body.radius, &level.0.fences);
        keep_in_bounds(&mut pos, body.radius, penned);
        tf.translation.x = pos.x;
        tf.translation.z = pos.y;
        vel.0 = v;
    };

    let mut i = 0;
    for (_, mut tf, mut vel, downed) in players.iter_mut() {
        if !alive_player(&downed) {
            continue;
        }
        write(&mut tf, &mut vel, &bodies[i], false);
        i += 1;
    }
    for (i, (mut tf, mut vel, cow)) in cows.iter_mut().enumerate() {
        write(&mut tf, &mut vel, &bodies[player_count + i], cow.penned);
    }

    for (slot, &(entity, pre_vel, already_downed)) in player_meta.iter().enumerate() {
        if already_downed || impacts[slot] < RIDER_THROW_SPEED {
            continue;
        }
        throws.0.push(Throw {
            player: entity,
            pre_vel,
            post_vel: bodies[slot].vel,
            horse_too: impacts[slot] >= HORSE_THROW_SPEED,
        });
    }
}

/// Ease the camera after this device's rider. Writes the un-shaken position;
/// `ragdoll::camera_shake` adds its jolt on top afterwards.
pub fn follow_camera(
    time: Res<Time>,
    local: Res<LocalPlayer>,
    players: Query<(&Player, &Transform), Without<GameCamera>>,
    mut focus: ResMut<CameraFocus>,
    mut camera: Query<&mut Transform, With<GameCamera>>,
) {
    let Some((_, rider)) = players.iter().find(|(p, _)| p.index == local.0) else {
        return;
    };
    let Ok(mut tf) = camera.single_mut() else {
        return;
    };
    let target = Vec2::new(rider.translation.x, rider.translation.z);
    let catch_up = 1.0 - (-CAM_FOLLOW_RATE * time.delta_secs()).exp();
    focus.0 = focus.0.lerp(target, catch_up);
    tf.translation = Vec3::new(focus.0.x, 0.0, focus.0.y) + CAM_OFFSET;
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

