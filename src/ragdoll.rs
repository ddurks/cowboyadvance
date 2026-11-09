//! Getting knocked out of the saddle.
//!
//! When `systems::body_collisions` queues a `Throw`, the rider's model (and on a
//! harder hit the horse's too) is detached from the gameplay entity and sent on
//! a ballistic tumble — an arcade ragdoll: the whole skinned model flies, spins,
//! bounces and settles. No physics engine and no jointed skeleton; at this art
//! scale the tumble is what reads, and it keeps the wasm bundle and the
//! deterministic sim untouched.
//!
//! The player sits `Downed` (input ignored) while the ragdoll plays out, then
//! respawns: thrown pieces despawn, the horse model is rebuilt if it flew, and
//! the existing `MountRider` seam re-seats a fresh rider.

use bevy::prelude::*;

use crate::anim::{Animated, MountRider, Rig};
use crate::components::*;
use crate::config::*;
use crate::noise::hash01;
use crate::setup::spawn_horse_model;

/// A model mid-flight. `owner` ties it back to the player it was thrown from so
/// respawn can clean it up.
#[derive(Component)]
pub struct Ragdoll {
    owner: Entity,
    vel: Vec3,
    angvel: Vec3,
}

/// State for a short camera-space impact jolt.
#[derive(Resource, Default)]
pub struct CameraShake {
    elapsed: f32,
    remaining: f32,
}

impl CameraShake {
    fn trigger(&mut self) {
        self.elapsed = 0.0;
        self.remaining = CAMERA_SHAKE_SECS;
    }
}

/// Launch velocity: keep most of the pre-impact motion (the rider sails on over
/// the handlebars) plus an amplified share of the impact's velocity change (a
/// broadside slam batters him sideways), plus the vertical kick.
fn launch(pre: Vec2, post: Vec2, up_scale: f32) -> Vec3 {
    let ground = pre * RAGDOLL_CARRY + (post - pre) * RAGDOLL_SHOVE;
    Vec3::new(ground.x, RAGDOLL_UP * up_scale, ground.y)
}

/// Deterministic tumble axis/rate from a per-throw seed.
fn spin(seed: u64) -> Vec3 {
    let axis = Vec3::new(
        hash01(seed * 3 + 1) - 0.5,
        hash01(seed * 3 + 2) - 0.5,
        hash01(seed * 3 + 3) - 0.5,
    )
    .normalize_or_zero();
    axis * RAGDOLL_SPIN * (0.6 + 0.4 * hash01(seed * 3 + 4))
}

pub fn throw_players(
    mut commands: Commands,
    mut throws: ResMut<PendingThrows>,
    mut counter: Local<u64>,
    models: Query<(&Animated, &GlobalTransform)>,
    children: Query<&Children>,
    mut velocities: Query<&mut Velocity, With<Player>>,
    mut shake: ResMut<CameraShake>,
) {
    for throw in throws.0.drain(..) {
        // Find the model roots hanging under this player. The rider is nested
        // inside the horse's saddle, so walk all descendants.
        let mut rider = None;
        let mut horse = None;
        for e in children.iter_descendants(throw.player) {
            if let Ok((animated, gt)) = models.get(e) {
                match animated.rig {
                    Rig::Rider => rider = Some((e, *gt)),
                    Rig::Horse => horse = Some((e, *gt)),
                    Rig::Cow => {}
                }
            }
        }

        // The glTF scenes stream in over the first moments of play; a hit
        // before the rider exists would punish the player with nothing on
        // screen to show for it, so it simply doesn't count.
        let Some((rider_entity, rider_gt)) = rider else {
            continue;
        };

        let mut eject = |entity: Entity, gt: GlobalTransform, up_scale: f32| {
            *counter += 1;
            commands.entity(entity).remove::<ChildOf>().insert((
                gt.compute_transform(),
                Ragdoll {
                    owner: throw.player,
                    vel: launch(throw.pre_vel, throw.post_vel, up_scale),
                    angvel: spin(*counter),
                },
            ));
        };

        eject(rider_entity, rider_gt, 1.0);
        if throw.horse_too {
            if let Some((entity, gt)) = horse {
                // The horse is heavier: a flatter, lower arc than its rider.
                eject(entity, gt, 0.7);
            }
        }

        commands.entity(throw.player).insert(Downed {
            timer: DOWNED_SECS,
            horse_gone: throw.horse_too,
        });
        shake.trigger();
        // The riderless horse stands where the crash left it (collisions may
        // still shove it around) rather than gliding on under old momentum.
        if let Ok(mut vel) = velocities.get_mut(throw.player) {
            vel.0 = Vec2::ZERO;
        }
    }
}

/// Shake in the camera's own X/Y plane so the effect reads consistently at
/// every window shape. Two mismatched frequencies avoid a mechanical wobble.
///
/// Runs after `systems::follow_camera`, which rewrites the camera's un-shaken
/// position every frame — so the jolt is simply added and never drifts.
pub fn camera_shake(
    time: Res<Time>,
    mut shake: ResMut<CameraShake>,
    mut camera: Query<&mut Transform, With<GameCamera>>,
) {
    let Ok(mut tf) = camera.single_mut() else {
        return;
    };

    if shake.remaining <= 0.0 {
        return;
    }

    let dt = time.delta_secs();
    shake.elapsed += dt;
    shake.remaining = (shake.remaining - dt).max(0.0);
    let decay = (shake.remaining / CAMERA_SHAKE_SECS).powi(2);
    let x = (shake.elapsed * 71.0).sin();
    let y = (shake.elapsed * 53.0 + 1.7).sin();
    let local = Vec3::new(x, y * 0.72, 0.0) * CAMERA_SHAKE_STRENGTH * decay;
    let jolt = tf.rotation * local;
    tf.translation += jolt;
}

pub fn ragdoll_motion(time: Res<Time>, mut q: Query<(&mut Transform, &mut Ragdoll)>) {
    let dt = time.delta_secs();
    for (mut tf, mut rag) in &mut q {
        rag.vel.y -= RAGDOLL_GRAVITY * dt;
        tf.translation += rag.vel * dt;

        // The rig roots at the feet, but a tumbling body pivots anywhere — so
        // treat a little above ground as the floor to keep limbs from clipping.
        if tf.translation.y < 0.0 && rag.vel.y < 0.0 {
            tf.translation.y = 0.0;
            rag.vel.y = -rag.vel.y * RAGDOLL_BOUNCE;
            // Each bounce scrubs speed and spin; below a floor it stops dead so
            // the body settles instead of micro-bouncing forever.
            rag.vel.x *= 0.6;
            rag.vel.z *= 0.6;
            rag.angvel *= 0.5;
            if rag.vel.y < 1.5 {
                rag.vel = Vec3::ZERO;
                rag.angvel = Vec3::ZERO;
            }
        }

        let w = rag.angvel * dt;
        if w != Vec3::ZERO {
            tf.rotation = Quat::from_scaled_axis(w) * tf.rotation;
        }
    }
}

pub fn respawn_players(
    time: Res<Time>,
    mut commands: Commands,
    scenes: Option<Res<ModelScenes>>,
    mut players: Query<
        (Entity, &mut Downed, &mut Transform, &mut Velocity, &mut ModelTint),
        With<Player>,
    >,
    ragdolls: Query<(Entity, &Ragdoll)>,
    models: Query<(Entity, &Animated)>,
    children: Query<&Children>,
) {
    // No one can be thrown before the models exist, so waiting costs nothing.
    let Some(scenes) = scenes else { return };

    let dt = time.delta_secs();
    for (player, mut downed, mut tf, mut vel, mut tint) in &mut players {
        downed.timer -= dt;
        if downed.timer > 0.0 {
            continue;
        }

        for (entity, rag) in &ragdolls {
            if rag.owner == player {
                commands.entity(entity).despawn();
            }
        }

        // Back up where he fell: out on the open range, a trip back to the
        // corral would be a punishment far bigger than the fall.
        tf.translation.y = 0.0;
        tf.rotation = Quat::IDENTITY;
        vel.0 = Vec2::ZERO;

        if downed.horse_gone {
            spawn_horse_model(&mut commands, &scenes, player);
        } else {
            let mut horse = None;
            let mut rider_still_seated = false;
            for e in children.iter_descendants(player) {
                match models.get(e).map(|(_, a)| a.rig) {
                    Ok(Rig::Horse) => horse = Some(e),
                    Ok(Rig::Rider) => rider_still_seated = true,
                    _ => {}
                }
            }
            // Horse survived the crash: ask `anim::mount_rider` to seat a fresh
            // cowboy — unless one is somehow still in the saddle, which would
            // otherwise stack a second rider on top of him.
            if let Some(horse) = horse {
                if !rider_still_seated {
                    commands.entity(horse).insert(MountRider { driver: player });
                }
            }
        }

        // The hat tint's cached mesh entities died with the thrown rider; make
        // `apply_model_tint` re-resolve them on the fresh model.
        tint.parts.clear();
        tint.applied = false;

        commands.entity(player).remove::<Downed>();
    }
}
