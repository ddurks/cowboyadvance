//! Render the game's real mounted gallop into PNG frames for the loading GIF.
//! Run with `cargo run --example render_loading_gif`, then assemble with the
//! `loading-gif` just recipe.

use core::time::Duration;
use std::f32::consts::{FRAC_PI_2, PI};

use bevy::animation::graph::AnimationNodeIndex;
use bevy::camera::ScalingMode;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured};

const HORSE_MODEL: &str = "models/horse.glb";
const RIDER_MODEL: &str = "models/cowboy.glb";
const HORSE_RUN: usize = 1;
const RIDER_RUN: usize = 1;
const FRAMES: usize = 12;
const CYCLE: f32 = 16.0 / 24.0;
const BROWN: Color = Color::srgb(
    0x5e as f32 / 255.0,
    0x41 as f32 / 255.0,
    0x23 as f32 / 255.0,
);

#[derive(Component, Clone, Copy)]
enum Rig {
    Horse,
    Rider,
}

#[derive(Resource)]
struct Clips {
    horse_graph: Handle<AnimationGraph>,
    horse: AnimationNodeIndex,
    rider_graph: Handle<AnimationGraph>,
    rider: AnimationNodeIndex,
    rider_scene: Handle<WorldAsset>,
}

#[derive(Resource, Default)]
struct Capture {
    next: usize,
    settling: u8,
    waiting: bool,
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Loading GIF renderer".into(),
                // Square: the loading overlay shows the GIF 1:1, horseman centred.
                resolution: (512u32, 512u32).into(),
                resizable: false,
                ..default()
            }),
            ..default()
        }))
        .init_resource::<Capture>()
        .add_systems(Startup, setup)
        .add_systems(Update, (mount_rider, bind_players, capture_frames).chain())
        .run();
}

fn setup(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let (horse_graph, horse) = AnimationGraph::from_clip(
        assets.load::<AnimationClip>(GltfAssetLabel::Animation(HORSE_RUN).from_asset(HORSE_MODEL)),
    );
    let (rider_graph, rider) = AnimationGraph::from_clip(
        assets.load::<AnimationClip>(GltfAssetLabel::Animation(RIDER_RUN).from_asset(RIDER_MODEL)),
    );
    commands.insert_resource(Clips {
        horse_graph: graphs.add(horse_graph),
        horse,
        rider_graph: graphs.add(rider_graph),
        rider,
        rider_scene: assets.load(GltfAssetLabel::Scene(0).from_asset(RIDER_MODEL)),
    });

    commands.spawn((
        WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(HORSE_MODEL))),
        Transform::from_rotation(Quat::from_rotation_y(PI)),
        Rig::Horse,
    ));

    // The ground shares the loading-screen matte; only the overhead shadow
    // distinguishes it, so the finished GIF has no visible rectangular edge.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(30.0, 30.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            // Measured, not styled: the albedo that the sun + camera fill +
            // ambient budget lifts into exactly the clear-color brown, so the
            // ground and backdrop read as one matte surface with only the
            // contact shadow breaking it.
            base_color: Color::srgb(
                0x25 as f32 / 255.0,
                0x17 as f32 / 255.0,
                0x08 as f32 / 255.0,
            ),
            perceptual_roughness: 1.0,
            // No specular at all: the default 0.5 reflectance bounces the white
            // ambient back and washes the match blue-grey.
            reflectance: 0.0,
            ..default()
        })),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
        CascadeShadowConfigBuilder {
            num_cascades: 1,
            maximum_distance: 30.0,
            ..default()
        }
        .build(),
    ));
    // Camera-side fill matching the favicon portrait — without it the gallop
    // reads as a top-lit silhouette. The overhead sun stays for the contact
    // shadow (and the shadow pass `Screenshot` needs to not capture black).
    commands.spawn((
        PointLight {
            intensity: 500_000.0, // half the 1_000_000 lm default
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(8.0, 2.6, 0.0),
    ));
    commands.spawn(AmbientLight {
        color: Color::WHITE,
        brightness: 1_600.0,
        affects_lightmapped_meshes: true,
    });
    commands.spawn((
        Camera3d::default(),
        Camera {
            clear_color: ClearColorConfig::Custom(BROWN),
            ..default()
        },
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: 4.8,
            },
            ..OrthographicProjection::default_3d()
        }),
        // Trucked -Z to the gallop cycle's average visual centre, measured off
        // the rendered frames, so the horseman sits centred in the square.
        Transform::from_xyz(8.0, 2.6, -0.45).looking_at(Vec3::new(0.0, 1.7, -0.45), Vec3::Y),
    ));
}

fn mount_rider(
    mut commands: Commands,
    clips: Res<Clips>,
    horses: Query<Entity, With<Rig>>,
    children: Query<&Children>,
    names: Query<&Name>,
    mut mounted: Local<bool>,
) {
    if *mounted {
        return;
    }
    let Some(saddle) = horses.iter().find_map(|horse| {
        children
            .iter_descendants(horse)
            .find(|&e| names.get(e).is_ok_and(|n| n.as_str() == "saddle"))
    }) else {
        return;
    };
    commands.entity(saddle).with_children(|c| {
        c.spawn((
            WorldAssetRoot(clips.rider_scene.clone()),
            Transform::from_rotation(Quat::from_rotation_x(FRAC_PI_2)),
            Rig::Rider,
        ));
    });
    *mounted = true;
}

fn bind_players(
    mut commands: Commands,
    clips: Res<Clips>,
    rigs: Query<&Rig>,
    parents: Query<&ChildOf>,
    mut players: Query<(Entity, &mut AnimationPlayer), Added<AnimationPlayer>>,
) {
    for (entity, mut player) in &mut players {
        let mut ancestor = entity;
        let rig = loop {
            if let Ok(rig) = rigs.get(ancestor) {
                break *rig;
            }
            let Ok(parent) = parents.get(ancestor) else {
                break Rig::Horse;
            };
            ancestor = parent.parent();
        };
        let (graph, node) = match rig {
            Rig::Horse => (&clips.horse_graph, clips.horse),
            Rig::Rider => (&clips.rider_graph, clips.rider),
        };
        let mut transitions = AnimationTransitions::new();
        transitions
            .play(&mut player, node, Duration::ZERO)
            .repeat()
            .pause();
        commands
            .entity(entity)
            .insert((AnimationGraphHandle(graph.clone()), transitions));
    }
}

fn capture_frames(
    mut commands: Commands,
    mut state: ResMut<Capture>,
    mut players: Query<&mut AnimationPlayer>,
) {
    if state.next >= FRAMES || state.waiting || players.iter().count() != 2 {
        return;
    }
    if state.settling == 0 {
        let phase = state.next as f32 / FRAMES as f32 * CYCLE;
        for mut player in &mut players {
            for (_, animation) in player.playing_animations_mut() {
                animation.set_seek_time(phase);
            }
        }
        state.settling = 2;
        return;
    }
    state.settling -= 1;
    if state.settling != 0 {
        return;
    }

    let _ = std::fs::create_dir_all("renders/loading");
    let path = format!("renders/loading/frame-{:02}.png", state.next);
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path))
        .observe(frame_saved);
    state.waiting = true;
}

fn frame_saved(
    _shot: On<ScreenshotCaptured>,
    mut state: ResMut<Capture>,
    mut exit: MessageWriter<AppExit>,
) {
    state.next += 1;
    state.waiting = false;
    if state.next == FRAMES {
        exit.write(AppExit::Success);
    }
}
