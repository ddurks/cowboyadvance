//! Character models and their animation.
//!
//! Graphs are built at startup from clip handles taken by index off the .glb, so
//! they exist before any model has loaded. The rest of the file waits on spawned
//! scenes: parenting the rider to the horse's saddle joint, handing each
//! `AnimationPlayer` its graph and driver, driving gaits from ground speed, and
//! recolouring model parts — all via the same "wait for the scene, then find
//! things by node name" pattern.

use core::time::Duration;

use bevy::animation::graph::AnimationNodeIndex;
use bevy::gltf::Gltf;
use bevy::prelude::*;

use crate::components::*;
use crate::config::*;

/// Which character a model is, and therefore which clips drive it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Rig {
    Horse,
    Cow,
    Rider,
}

/// Locomotion state. `Stop` is the one-shot skid between running and standing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Gait {
    Idle,
    Walk,
    Run,
    Stop,
}

/// One rig's clips, all hung off a single graph, plus the speeds they were
/// animated for.
pub struct RigClips {
    pub graph: Handle<AnimationGraph>,
    pub idle: AnimationNodeIndex,
    pub walk: AnimationNodeIndex,
    pub run: AnimationNodeIndex,
    /// The skid. The rider has no equivalent — he just keeps his seat.
    pub stop: Option<AnimationNodeIndex>,
    /// Ground speed (game units/second) at which each clip plays at 1.0x.
    pub walk_speed: f32,
    pub run_speed: f32,
    /// How long the skid lasts, seconds.
    pub stop_len: f32,
    /// Playback rate for the idle clip, which has no ground speed to match.
    pub idle_rate: f32,
}

#[derive(Resource)]
pub struct RigAnimations {
    pub horse: RigClips,
    pub cow: RigClips,
    pub rider: RigClips,
}

impl RigAnimations {
    pub fn clips(&self, rig: Rig) -> &RigClips {
        match rig {
            Rig::Horse => &self.horse,
            Rig::Cow => &self.cow,
            Rig::Rider => &self.rider,
        }
    }
}

/// Marks the glTF model root we spawn under a gameplay entity, and remembers
/// which entity's velocity its gait should follow. The rider's driver is the
/// player, not the horse model, so horse and rider stay in step by construction.
#[derive(Component, Clone, Copy)]
pub struct Animated {
    pub rig: Rig,
    pub driver: Entity,
}

/// Put on a horse model root until its rider has been parented to the saddle.
#[derive(Component, Clone, Copy)]
pub struct MountRider {
    pub driver: Entity,
}

/// The live gait, on the entity Bevy put the `AnimationPlayer` on.
#[derive(Component)]
pub struct GaitState {
    rig: Rig,
    driver: Entity,
    gait: Gait,
    /// Seconds of skid left to play.
    stop_left: f32,
    /// Seconds of credit for having been running recently, so a halt reads as a
    /// skid rather than a stroll.
    ran_recently: f32,
}

// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------

/// An animal's four clips, in the order `RigClips` wants them: idle, walk, run,
/// stop. `walk`/`run` are the speeds those clips were animated for, before the
/// arcade scaling in `config`.
fn quadruped(
    graphs: &mut Assets<AnimationGraph>,
    gltf: &Gltf,
    clips: [usize; 4],
    walk: f32,
    run: f32,
    stop_len: f32,
) -> RigClips {
    let (graph, nodes) = AnimationGraph::from_clips(clips.map(|i| gltf.animations[i].clone()));
    RigClips {
        graph: graphs.add(graph),
        idle: nodes[0],
        walk: nodes[1],
        run: nodes[2],
        stop: Some(nodes[3]),
        walk_speed: walk * GAIT_SPEED_SCALE,
        run_speed: run * GAIT_SPEED_SCALE,
        stop_len,
        idle_rate: 1.0,
    }
}

/// Request each character .glb exactly once, as its root `Gltf` asset. See
/// `PendingModels` for why sub-assets must never be loaded by label.
pub fn load_models(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(PendingModels {
        horse: asset_server.load(HORSE_MODEL),
        cow: asset_server.load(COW_MODEL),
        cowboy: asset_server.load(COWBOY_MODEL),
    });
}

/// Once all three .glb parses land: build the clip graphs, pin the scene
/// handles in `ModelScenes`, and hang the models off the gameplay roots that
/// have been herding invisibly in the meantime.
pub fn finish_models(
    mut commands: Commands,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    pending: Option<Res<PendingModels>>,
    gltfs: Res<Assets<Gltf>>,
    cows: Query<Entity, With<Cow>>,
    players: Query<Entity, With<Player>>,
) {
    let Some(pending) = pending else { return };
    let (Some(horse_gltf), Some(cow_gltf), Some(cowboy_gltf)) = (
        gltfs.get(&pending.horse),
        gltfs.get(&pending.cow),
        gltfs.get(&pending.cowboy),
    ) else {
        return;
    };

    let horse = quadruped(
        &mut graphs,
        horse_gltf,
        [HORSE_CLIP_IDLE, HORSE_CLIP_WALK, HORSE_CLIP_RUN, HORSE_CLIP_STOP],
        HORSE_WALK_CLIP_SPEED,
        HORSE_RUN_CLIP_SPEED,
        HORSE_STOP_LEN,
    );
    let cow = quadruped(
        &mut graphs,
        cow_gltf,
        [COW_CLIP_IDLE, COW_CLIP_WALK, COW_CLIP_RUN, COW_CLIP_STOP],
        COW_WALK_CLIP_SPEED,
        COW_RUN_CLIP_SPEED,
        COW_STOP_LEN,
    );

    // The rider's two seats were animated on the same cycle lengths as the
    // horse's walk and gallop, so feeding him the horse's reference speeds locks
    // his bounce to whatever the horse is doing underneath him. Idling reuses
    // the walking seat, slowed: `cowboy_idle` is a standing pose and would put
    // his boots through the barrel.
    let (rider_graph, rider_nodes) = AnimationGraph::from_clips(
        [COWBOY_CLIP_RIDE_WALK, COWBOY_CLIP_RIDE].map(|i| cowboy_gltf.animations[i].clone()),
    );
    let rider = RigClips {
        graph: graphs.add(rider_graph),
        // Idle and walk are deliberately the same node: crossing the walk
        // threshold then changes the seat's tempo without restarting it.
        idle: rider_nodes[0],
        walk: rider_nodes[0],
        run: rider_nodes[1],
        stop: None,
        walk_speed: horse.walk_speed,
        run_speed: horse.run_speed,
        stop_len: 0.0,
        idle_rate: RIDER_IDLE_RATE,
    };

    commands.insert_resource(RigAnimations { horse, cow, rider });

    let scenes = ModelScenes {
        horse: horse_gltf.scenes[0].clone(),
        cow: cow_gltf.scenes[0].clone(),
        rider: cowboy_gltf.scenes[0].clone(),
        _roots: [pending.horse.clone(), pending.cow.clone(), pending.cowboy.clone()],
    };

    let model_yaw = Quat::from_rotation_y(MODEL_YAW);
    for cow in &cows {
        commands.entity(cow).with_children(|c| {
            c.spawn((
                WorldAssetRoot(scenes.cow.clone()),
                Transform::from_rotation(model_yaw),
                Animated { rig: Rig::Cow, driver: cow },
            ));
        });
    }
    for player in &players {
        crate::setup::spawn_horse_model(&mut commands, &scenes, player);
    }

    commands.insert_resource(scenes);
    commands.remove_resource::<PendingModels>();
}

// ---------------------------------------------------------------------------
// Wiring up spawned models
// ---------------------------------------------------------------------------

/// Parent the rider to the horse's `saddle` joint once the horse's scene has
/// spawned. The joint is animated, so the rider inherits the gait's bounce for
/// free — the same setup blender/preview_ride.py renders to check it.
pub fn mount_rider(
    mut commands: Commands,
    scenes: Option<Res<ModelScenes>>,
    pending: Query<(Entity, &MountRider)>,
    children: Query<&Children>,
    names: Query<&Name>,
) {
    let Some(scenes) = scenes else { return };
    for (horse, mount) in &pending {
        let Some(saddle) = children
            .iter_descendants(horse)
            .find(|&e| names.get(e).is_ok_and(|n| n.as_str() == SADDLE_JOINT))
        else {
            continue; // scene still loading
        };

        let driver = mount.driver;
        commands.entity(saddle).with_children(|c| {
            c.spawn((
                WorldAssetRoot(scenes.rider.clone()),
                Transform::from_rotation(Quat::from_rotation_x(SADDLE_FIX)),
                Animated { rig: Rig::Rider, driver },
            ));
        });
        commands.entity(horse).remove::<MountRider>();
    }
}

/// Hand every freshly spawned `AnimationPlayer` its graph and its gait state.
///
/// Bevy puts the player on whichever node inside the scene roots the animated
/// hierarchy, so we walk back up the parents to the model root we spawned — the
/// one carrying `Animated`. The rider is nested inside the horse, and stopping
/// at the *first* `Animated` ancestor is what keeps the two apart.
pub fn bind_animation_players(
    mut commands: Commands,
    rigs: Option<Res<RigAnimations>>,
    models: Query<&Animated>,
    parents: Query<&ChildOf>,
    mut players: Query<(Entity, &mut AnimationPlayer), Added<AnimationPlayer>>,
) {
    let Some(rigs) = rigs else { return };
    for (entity, mut player) in &mut players {
        let mut node = entity;
        let model = loop {
            if let Ok(model) = models.get(node) {
                break Some(*model);
            }
            match parents.get(node) {
                Ok(parent) => node = parent.parent(),
                Err(_) => break None,
            }
        };
        let Some(model) = model else { continue };

        let clips = rigs.clips(model.rig);
        let mut transitions = AnimationTransitions::new();
        transitions.play(&mut player, clips.idle, Duration::ZERO).repeat();

        commands.entity(entity).insert((
            AnimationGraphHandle(clips.graph.clone()),
            transitions,
            GaitState {
                rig: model.rig,
                driver: model.driver,
                gait: Gait::Idle,
                stop_left: 0.0,
                ran_recently: 0.0,
            },
        ));
    }
}

// ---------------------------------------------------------------------------
// Per-frame gait
// ---------------------------------------------------------------------------

/// Pick a clip from how fast the entity is travelling, and play it at the rate
/// that keeps its feet on the ground (see the gait notes in `config`).
pub fn drive_gaits(
    time: Res<Time>,
    rigs: Option<Res<RigAnimations>>,
    movers: Query<&Velocity>,
    mut players: Query<(&mut AnimationPlayer, &mut AnimationTransitions, &mut GaitState)>,
) {
    let Some(rigs) = rigs else { return };
    let dt = time.delta_secs();

    for (mut player, mut transitions, mut state) in &mut players {
        let clips = rigs.clips(state.rig);
        let speed = movers.get(state.driver).map(|v| v.0.length()).unwrap_or(0.0);

        state.stop_left = (state.stop_left - dt).max(0.0);
        state.ran_recently = if state.gait == Gait::Run {
            GAIT_STOP_WINDOW
        } else {
            (state.ran_recently - dt).max(0.0)
        };

        let running = speed > clips.run_speed * GAIT_RUN_ENTER;
        let moving = speed > clips.walk_speed * GAIT_WALK_ENTER;
        // A halt is a skid only if there was speed to lose and a clip to show it.
        let halt = if clips.stop.is_some() && state.ran_recently > 0.0 {
            Gait::Stop
        } else {
            Gait::Idle
        };

        let next = match state.gait {
            Gait::Run => {
                if speed > clips.run_speed * GAIT_RUN_EXIT {
                    Gait::Run
                } else if moving {
                    Gait::Walk
                } else {
                    halt
                }
            }
            Gait::Walk => {
                if running {
                    Gait::Run
                } else if speed > clips.walk_speed * GAIT_WALK_EXIT {
                    Gait::Walk
                } else {
                    halt
                }
            }
            // The skid plays out unless the entity gets going again.
            Gait::Stop => {
                if running {
                    Gait::Run
                } else if moving {
                    Gait::Walk
                } else if state.stop_left > 0.0 {
                    Gait::Stop
                } else {
                    Gait::Idle
                }
            }
            Gait::Idle => {
                if running {
                    Gait::Run
                } else if moving {
                    Gait::Walk
                } else {
                    Gait::Idle
                }
            }
        };

        if next == Gait::Stop && state.gait != Gait::Stop {
            state.stop_left = clips.stop_len;
        }
        state.gait = next;

        let (node, rate, looping) = match next {
            Gait::Idle => (clips.idle, clips.idle_rate, true),
            Gait::Walk => (clips.walk, speed / clips.walk_speed, true),
            Gait::Run => (clips.run, speed / clips.run_speed, true),
            // `halt` only yields Stop when the clip exists, so the fallback is
            // unreachable; it just keeps the match total.
            Gait::Stop => (clips.stop.unwrap_or(clips.idle), 1.0, false),
        };
        let rate = rate.clamp(GAIT_RATE_MIN, GAIT_RATE_MAX);

        // Compare clips, not gaits: the rider plays one seat for both idling and
        // walking, and restarting it on every threshold crossing would hitch.
        if transitions.get_main_animation() != Some(node) {
            let active = transitions.play(&mut player, node, Duration::from_secs_f32(GAIT_BLEND));
            if looping {
                active.repeat();
            }
            active.set_speed(rate);
        } else if let Some(active) = player.animation_mut(node) {
            active.set_speed(rate);
        }
    }
}

// ---------------------------------------------------------------------------
// Material overrides
// ---------------------------------------------------------------------------

/// Resolve each `ModelTint`'s mesh entities once its model has spawned, then
/// keep those meshes painted to match `ModelTint::on`.
///
/// The loader may put a node's material on the node entity itself or on a child
/// per primitive, so both are checked.
pub fn apply_model_tint(
    mut tinted: Query<(Entity, &mut ModelTint)>,
    children: Query<&Children>,
    names: Query<&Name>,
    mut materials: Query<&mut MeshMaterial3d<StandardMaterial>>,
) {
    for (root, mut tint) in &mut tinted {
        if tint.parts.is_empty() {
            let wanted = tint.nodes;
            let nodes: Vec<Entity> = children
                .iter_descendants(root)
                .filter(|&e| names.get(e).is_ok_and(|n| wanted.iter().any(|&w| w == n.as_str())))
                .collect();
            for node in nodes {
                for mesh in core::iter::once(node).chain(children.iter_descendants(node)) {
                    if let Ok(material) = materials.get(mesh) {
                        tint.parts.push((mesh, material.0.clone()));
                    }
                }
            }
            if tint.parts.is_empty() {
                continue; // scene still loading
            }
        }

        if tint.applied == tint.on {
            continue;
        }
        for (mesh, modelled) in &tint.parts {
            if let Ok(mut material) = materials.get_mut(*mesh) {
                material.0 = if tint.on { tint.tint.clone() } else { modelled.clone() };
            }
        }
        tint.applied = tint.on;
    }
}
