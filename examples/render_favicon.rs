//! Render the cowboy's head-and-shoulders portrait for the favicons.
//! Run with `cargo run --example render_favicon`, then cut the sizes with the
//! `favicon` just recipe.

use core::time::Duration;

use bevy::asset::RenderAssetUsages;
use bevy::camera::{RenderTarget, ScalingMode};
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureUsages,
};
use bevy::render::view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured};

const RIDER_MODEL: &str = "models/cowboy.glb";
/// `cowboy_idle` — the standing pose; the ride seats would fold his legs.
const RIDER_IDLE: usize = 0;
/// Mid-face of the standing rig; the ortho window is centred here.
const HEAD_HEIGHT: f32 = 2.23;
/// World-units of height in frame: clear air over the hat crown down to the
/// shoulder tips.
const PORTRAIT_HEIGHT: f32 = 2.36;

#[derive(Resource)]
struct Clips {
    graph: Handle<AnimationGraph>,
    idle: bevy::animation::graph::AnimationNodeIndex,
}

/// The offscreen target the portrait renders into. The window surface drops
/// the alpha channel, so a transparent background only survives in a texture
/// we own.
#[derive(Resource)]
struct Portrait(Handle<Image>);

#[derive(Resource, Default)]
struct Capture {
    settling: u8,
    fired: bool,
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Favicon renderer".into(),
                resolution: (512u32, 512u32).into(),
                resizable: false,
                ..default()
            }),
            ..default()
        }))
        .init_resource::<Capture>()
        .add_systems(Startup, setup)
        .add_systems(Update, (bind_players, capture_portrait).chain())
        .run();
}

fn setup(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut images: ResMut<Assets<Image>>,
) {
    let (graph, idle) = AnimationGraph::from_clip(
        assets.load::<AnimationClip>(GltfAssetLabel::Animation(RIDER_IDLE).from_asset(RIDER_MODEL)),
    );
    commands.insert_resource(Clips { graph: graphs.add(graph), idle });

    // The rig already faces +Z — straight at the +Z camera.
    commands.spawn((
        WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(RIDER_MODEL))),
        Transform::default(),
    ));

    // Ambient plus a half-intensity point light sitting on the camera, so the
    // face reads flat and bright at 16 px. Its shadow maps stay ON even though
    // every shadow falls invisibly behind him: a scene with no shadow pass
    // blacks out `Screenshot` captures in this Bevy build.
    commands.spawn((
        PointLight {
            intensity: 500_000.0, // half the 1_000_000 lm default
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(0.0, HEAD_HEIGHT, 6.0),
    ));
    commands.spawn(AmbientLight {
        color: Color::WHITE,
        brightness: 1_600.0,
        affects_lightmapped_meshes: true,
    });

    let mut target = Image::new_fill(
        Extent3d { width: 512, height: 512, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    target.texture_descriptor.usage =
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC;
    let target = images.add(target);
    commands.insert_resource(Portrait(target.clone()));

    commands.spawn((
        Camera3d::default(),
        RenderTarget::Image(target.into()),
        // `save_to_disk` writes RGB — a transparent clear cannot survive into
        // the PNG. Chroma instead: clear to lime (nothing on the cowboy is
        // green) and let the `favicon` recipe key the background out. MSAA off
        // so edge pixels are either subject or pure backdrop, never a blend.
        Msaa::Off,
        Camera {
            clear_color: ClearColorConfig::Custom(Color::srgb(0.0, 1.0, 0.0)),
            ..default()
        },
        Projection::from(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: PORTRAIT_HEIGHT,
            },
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_xyz(0.0, HEAD_HEIGHT, 6.0)
            .looking_at(Vec3::new(0.0, HEAD_HEIGHT, 0.0), Vec3::Y),
    ));
}

fn bind_players(
    mut commands: Commands,
    clips: Res<Clips>,
    mut players: Query<(Entity, &mut AnimationPlayer), Added<AnimationPlayer>>,
) {
    for (entity, mut player) in &mut players {
        let mut transitions = AnimationTransitions::new();
        transitions
            .play(&mut player, clips.idle, Duration::ZERO)
            .repeat()
            .pause();
        commands
            .entity(entity)
            .insert((AnimationGraphHandle(clips.graph.clone()), transitions));
    }
}

fn capture_portrait(
    mut commands: Commands,
    mut state: ResMut<Capture>,
    portrait: Res<Portrait>,
    players: Query<(), With<AnimationPlayer>>,
) {
    if state.fired || players.is_empty() {
        return;
    }
    state.settling += 1;
    if state.settling < 4 {
        return; // let the paused pose and the lighting settle
    }
    let _ = std::fs::create_dir_all("renders");
    commands
        .spawn(Screenshot::image(portrait.0.clone()))
        .observe(save_to_disk("renders/favicon.png"))
        .observe(portrait_saved);
    state.fired = true;
}

fn portrait_saved(_shot: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>) {
    exit.write(AppExit::Success);
}
