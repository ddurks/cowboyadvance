//! Per-frame gameplay systems: player movement, the boid herd, pen scoring, and
//! the HUD. What the characters *look* like doing it lives in `anim`.

use bevy::prelude::*;

use crate::components::*;
use crate::config::*;
use crate::sim::{boid_acceleration, clamp_field, resolve_body_collisions, resolve_fences, Body};

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

// The query type is a normal Bevy filter combination, not real complexity.
#[allow(clippy::type_complexity)]
pub fn cow_boids(
    time: Res<Time>,
    level: Res<GameLevel>,
    players: Query<(&Transform, Option<&Downed>), (With<Player>, Without<Cow>)>,
    mut cows: Query<
        (&mut Transform, &mut Velocity, &mut Cow, &mut ModelTint, &mut Visibility),
        Without<Player>,
    >,
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
    // flag rides along so captured cows drop out of everyone's neighbourhood.
    let snapshot: Vec<(Vec2, Vec2, bool)> = cows
        .iter()
        .map(|(t, v, c, _, _)| (Vec2::new(t.translation.x, t.translation.z), v.0, c.penned))
        .collect();

    // Pass 2: steer each cow against the snapshot (same iteration order).
    for (i, (mut tf, mut vel, mut cow, mut tint, mut vis)) in cows.iter_mut().enumerate() {
        let mut pos = Vec2::new(tf.translation.x, tf.translation.z);

        // Captured: leave the herd behind and march straight off the top of the
        // field. No fence or boundary handling, so it walks clean through the top
        // edge and out of view instead of leaking back into the play area.
        if cow.penned {
            let mut v = Vec2::new(0.0, -PEN_EXIT_SPEED);
            if pos.y <= PEN_HIDE_Z {
                v = Vec2::ZERO; // fully off-screen: stop and disappear for good.
                *vis = Visibility::Hidden;
            }
            pos += v * dt;
            vel.0 = v;
            tf.translation.x = pos.x;
            tf.translation.z = pos.y;
            if let Some(target) = travel_rotation(v) {
                tf.rotation = tf.rotation.slerp(target, (TURN_RATE * dt).min(1.0));
            }
            continue;
        }

        let acc = boid_acceleration(i, &snapshot, &player_positions, &level.0);
        let mut v = vel.0 + acc * dt;
        v *= 1.0 - (COW_DRAG * dt).min(1.0);
        v = v.clamp_length_max(COW_MAX_SPEED);
        pos += v * dt;
        resolve_fences(&mut pos, &mut v, COW_RADIUS, &level.0.fences);
        clamp_field(&mut pos, COW_RADIUS);
        vel.0 = v;
        tf.translation.x = pos.x;
        tf.translation.z = pos.y;
        if let Some(target) = travel_rotation(v) {
            tf.rotation = tf.rotation.slerp(target, (TURN_RATE * dt).min(1.0));
        }

        if level.0.penned(pos) {
            cow.penned = true;
            // `anim::apply_model_tint` repaints the hide green before it exits.
            tint.on = true;
        }
    }
}

/// Hard circle-circle collisions between every mover (players + unpenned cows),
/// run after both movement systems so it resolves the positions they just wrote.
///
/// Impacts harder than the throw thresholds queue a `Throw` for `ragdoll` to
/// act on. Skipped bodies: penned cows (marching off-field) and downed players
/// whose horse is gone (nothing is standing there).
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
    for (tf, vel, cow) in cows.iter() {
        if cow.penned {
            continue;
        }
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
    let write = |tf: &mut Transform, vel: &mut Velocity, body: &Body| {
        let (mut pos, mut v) = (body.pos, body.vel);
        resolve_fences(&mut pos, &mut v, body.radius, &level.0.fences);
        clamp_field(&mut pos, body.radius);
        tf.translation.x = pos.x;
        tf.translation.z = pos.y;
        vel.0 = v;
    };

    let mut i = 0;
    for (_, mut tf, mut vel, downed) in players.iter_mut() {
        if !alive_player(&downed) {
            continue;
        }
        write(&mut tf, &mut vel, &bodies[i]);
        i += 1;
    }
    let mut i = player_count;
    for (mut tf, mut vel, cow) in cows.iter_mut() {
        if cow.penned {
            continue;
        }
        write(&mut tf, &mut vel, &bodies[i]);
        i += 1;
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

