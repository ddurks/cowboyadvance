//! Per-frame gameplay systems: player movement, the boid herd, pen scoring, the
//! follow camera, and the HUD. What the characters *look* like doing it lives in `anim`.

use bevy::prelude::*;

use crate::components::*;
use crate::config::*;
use crate::sim::{
    boid_acceleration, claim_owners, clamp_range, confine_to_corral, herd_max_speed, herd_sizes,
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
    players: Query<(&Player, &Transform, Option<&Downed>), Without<Cow>>,
    mut cows: Query<(&mut Transform, &mut Velocity, &mut Cow), Without<Player>>,
) {
    let dt = time.delta_secs();

    // A downed player whose horse also went flying leaves nothing standing to
    // avoid; a riderless horse still spooks cows.
    let player_positions: Vec<Vec2> = players
        .iter()
        .filter(|(_, _, downed)| !downed.is_some_and(|d| d.horse_gone))
        .map(|(_, t, _)| Vec2::new(t.translation.x, t.translation.z))
        .collect();
    // Only a rider in the saddle gathers cattle.
    let claimers: Vec<(usize, Vec2)> = players
        .iter()
        .filter(|(_, _, downed)| downed.is_none())
        .map(|(p, t, _)| (p.index, Vec2::new(t.translation.x, t.translation.z)))
        .collect();

    // Pass 1: snapshot (ends the immutable borrow before we mutate). The penned
    // flag rides along so the corral herd and the loose cattle flock apart.
    let snapshot: Vec<Boid> = cows
        .iter()
        .map(|(t, v, c)| Boid {
            pos: Vec2::new(t.translation.x, t.translation.z),
            vel: v.0,
            penned: c.penned,
            owner: c.owner,
            drive: c.drive,
        })
        .collect();
    let herd_size = herd_sizes(&snapshot);
    let owners = claim_owners(&snapshot, &claimers);

    // Pass 2: steer each cow against the snapshot (same iteration order).
    for (i, (mut tf, mut vel, mut cow)) in cows.iter_mut().enumerate() {
        let (acc, drive) = boid_acceleration(i, &snapshot, &player_positions, &level.0, dt);
        cow.drive = drive;
        cow.owner = owners[i];
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

/// Riding into the corral ends the run: the rider's penned cows are banked and
/// come back as wild cattle elsewhere on the range, and whatever of the herd
/// is still outside goes wild where it stands. Riding in with no herd at all
/// is just riding, not a run.
#[allow(clippy::type_complexity)]
pub fn cash_out(
    time: Res<Time>,
    local: Res<LocalPlayer>,
    level: Res<GameLevel>,
    mut runs: ResMut<Runs>,
    players: Query<(&Player, &Transform), (Without<Downed>, Without<Cow>)>,
    mut cows: Query<(&mut Transform, &mut Velocity, &mut Cow), Without<Player>>,
) {
    runs.banner = (runs.banner - time.delta_secs()).max(0.0);
    let riders: Vec<Vec2> =
        players.iter().map(|(_, t)| Vec2::new(t.translation.x, t.translation.z)).collect();

    for (player, tf) in &players {
        let inside = level.0.penned(Vec2::new(tf.translation.x, tf.translation.z));
        if runs.in_corral.len() <= player.index {
            runs.in_corral.resize(player.index + 1, false);
        }
        let entered = inside && !runs.in_corral[player.index];
        runs.in_corral[player.index] = inside;
        let mine = Some(player.index);
        if !entered || !cows.iter().any(|(_, _, c)| c.owner == mine) {
            continue;
        }

        let mut banked = 0;
        for (mut tf, mut vel, mut cow) in &mut cows {
            // Wild strays that wandered into the corral clear out with the
            // rider's herd rather than milling there forever.
            if cow.penned && (cow.owner == mine || cow.owner.is_none()) {
                banked += usize::from(cow.owner == mine);
                let spot = crate::range::replacement_spawn(runs.replaced, &riders);
                runs.replaced += 1;
                tf.translation.x = spot.x;
                tf.translation.z = spot.y;
                vel.0 = Vec2::ZERO;
                *cow = Cow::default();
            } else if cow.owner == mine {
                cow.owner = None;
            }
        }

        if player.index == local.0 {
            runs.last_banked = banked;
            runs.best = runs.best.max(banked);
            runs.banner = CASH_OUT_BANNER_SECS;
        }
    }
}

/// Show each herded cow's brand in its rider's colour; hide it on wild cows.
pub fn show_herd_brands(
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut palette: ResMut<BrandMaterials>,
    cows: Query<(&Cow, &Children)>,
    mut brands: Query<(&mut Visibility, &mut MeshMaterial3d<StandardMaterial>), With<HerdBrand>>,
) {
    for (cow, children) in &cows {
        for &child in children {
            let Ok((mut vis, mut mat)) = brands.get_mut(child) else {
                continue;
            };
            let Some(owner) = cow.owner else {
                vis.set_if_neq(Visibility::Hidden);
                continue;
            };
            while palette.0.len() <= owner {
                let color = srgb3(player_color(palette.0.len()));
                // Unlit so the colour reads the same in the cow's shadow.
                palette.0.push(materials.add(StandardMaterial { base_color: color, unlit: true, ..default() }));
            }
            vis.set_if_neq(Visibility::Inherited);
            if mat.0 != palette.0[owner] {
                mat.0 = palette.0[owner].clone();
            }
        }
    }
}

pub fn update_hud(
    local: Res<LocalPlayer>,
    runs: Res<Runs>,
    cows: Query<&Cow>,
    mut text_q: Query<&mut Text, With<PenCounter>>,
) {
    let mine = cows.iter().filter(|c| c.owner == Some(local.0));
    let (penned, herd) = mine.fold((0, 0), |(p, h), c| if c.penned { (p + 1, h) } else { (p, h + 1) });
    for mut text in &mut text_q {
        **text = if runs.banner > 0.0 {
            format!("Banked {}!  Best {}", runs.last_banked, runs.best)
        } else if penned > 0 {
            format!("Herd {herd}  Penned {penned}")
        } else {
            format!("Herd {herd}")
        };
    }
}

