//! Scene construction: camera, lighting, ground, pen, the herd, the mounted
//! player, and the 2D HUD / joystick overlay. Runs once at `Startup`.
//!
//! The characters are only *requested* here — glTF scenes load asynchronously,
//! so `anim` finishes assembling them (rider onto saddle, clips onto players)
//! whenever they actually turn up.

use bevy::camera::ScalingMode;
use bevy::prelude::*;

use crate::anim::{Animated, MountRider, Rig};
use crate::components::*;
use crate::config::*;
use crate::level::Level;

/// Deterministic pseudo-random in [0, 1) from an integer seed (splitmix-ish).
///
/// Avoids pulling in an RNG crate (and the wasm `getrandom` dance), and keeps
/// cow spawns reproducible — handy for future deterministic multiplayer.
fn hash01(n: u64) -> f32 {
    let mut z = n.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^= z >> 31;
    // top 24 bits -> [0,1)
    ((z >> 40) as f32) / (1u64 << 24) as f32
}

/// Lift a ground position (X, Z) to a world translation at height `y`.
fn world(pos: Vec2, y: f32) -> Vec3 {
    Vec3::new(pos.x, y, pos.y)
}

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    let level = Level::build();

    // --- Cameras ---------------------------------------------------------
    // Three passes over the window (ascending `order`):
    //  0  background — clears the whole window to the bezel's own brown, so the
    //     bands a phone leaves above and below (and any letterbox on a window
    //     the cabinet cannot fill) read as more of the frame's timber rather
    //     than as dead space. Also sits behind the frame's transparent areas.
    //  1  game — angled orthographic 3D, confined to a viewport (the cabinet's
    //     scene rect, resized each frame). `clear_color: None` so it draws over
    //     the black without wiping the gutters; the ground fills the viewport.
    //  2  UI — the frame pieces, joystick and HUD, on top.
    commands.spawn((
        Camera2d::default(),
        Camera {
            order: 0,
            clear_color: ClearColorConfig::Custom(srgb3(FRAME_WOOD_COLOR)),
            ..default()
        },
    ));

    let cam_target = world(CAM_TARGET, 0.0);
    commands.spawn((
        Camera3d::default(),
        Camera { order: 1, clear_color: ClearColorConfig::None, ..default() },
        Projection::from(OrthographicProjection {
            // AutoMin keeps the whole play box visible on any aspect (uniform
            // scale, no distortion); wider/taller viewports reveal more ground.
            scaling_mode: ScalingMode::AutoMin {
                min_width: CAM_MIN_WIDTH,
                min_height: CAM_MIN_HEIGHT,
            },
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_translation(cam_target + CAM_OFFSET).looking_at(cam_target, Vec3::Y),
        GameCamera,
    ));

    let ui_camera = commands
        .spawn((
            Camera2d::default(),
            Camera { order: 2, clear_color: ClearColorConfig::None, ..default() },
        ))
        .id();

    // Build the HUD: split frame pieces + the joystick knob.
    let frame_top = asset_server.load("ui/frame_top.png");
    let frame_bottom = asset_server.load("ui/frame_bottom.png");
    let joystick_image = asset_server.load("ui/joystick.png");
    crate::cabinet::spawn_cabinet(&mut commands, ui_camera, frame_top, frame_bottom, joystick_image);

    // --- Lighting --------------------------------------------------------
    commands.spawn((
        DirectionalLight {
            illuminance: light_consts::lux::OVERCAST_DAY,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(20.0, 40.0, 20.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: 240.0,
        ..default()
    });
    // (Clear colors are set per-camera above: sky for the game, room for the UI.)

    // --- Ground ----------------------------------------------------------
    // Oversized so AutoMin's "extra" space fills with grass (not sky) on any
    // aspect ratio. The play area is bounded separately by FIELD_HALF_*.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(400.0, 400.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: srgb3(GROUND_COLOR),
            perceptual_roughness: 1.0,
            ..default()
        })),
    ));

    // Goal-zone tint over the pen floor.
    let pen_center = (level.pen_min + level.pen_max) * 0.5;
    let pen_size = level.pen_max - level.pen_min;
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(pen_size.x, pen_size.y))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(PEN_TINT_COLOR.0, PEN_TINT_COLOR.1, PEN_TINT_COLOR.2, 0.5),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Transform::from_translation(world(pen_center, 0.02)),
    ));

    // --- Fences ----------------------------------------------------------
    // The pen, plus the field edge, built from the modelled post and rail.
    crate::fence::spawn_fences(&mut commands, &asset_server, &mut materials, &level);

    // --- Characters ------------------------------------------------------
    // Gameplay lives on a bare transform; the model hangs off it as a child so
    // the sim never has to care that the art is a skinned glTF spun 180° to
    // face the way the game thinks is forward. Models stand on y = 0 — their
    // rigs are rooted at the feet.
    let model_yaw = Quat::from_rotation_y(MODEL_YAW);

    let cow_penned_mat = materials.add(StandardMaterial {
        base_color: srgb3(COW_PENNED_COLOR),
        perceptual_roughness: 0.8,
        ..default()
    });
    for i in 0..COW_COUNT {
        // Herd starts clustered near the bottom of the field (+Z).
        let x = -14.0 + hash01(i as u64 * 2 + 1) * 28.0;
        let z = 16.0 + hash01(i as u64 * 2 + 2) * 18.0;
        let cow = commands
            .spawn((
                Transform::from_translation(world(Vec2::new(x, z), 0.0)),
                Visibility::default(),
                Cow { penned: false },
                Velocity(Vec2::ZERO),
                ModelTint::new(COW_HIDE_NODES, cow_penned_mat.clone(), false),
            ))
            .id();
        commands.entity(cow).with_children(|c| {
            c.spawn((
                WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(COW_MODEL))),
                Transform::from_rotation(model_yaw),
                Animated { rig: Rig::Cow, driver: cow },
            ));
        });
    }

    // The player is a horse; the rider is parented to its saddle joint once the
    // horse's scene exists (see `anim::mount_rider`).
    let hat_mat = materials.add(StandardMaterial {
        base_color: srgb3(player_color(0)),
        perceptual_roughness: 0.7,
        ..default()
    });
    let player = commands
        .spawn((
            Transform::from_translation(world(Vec2::new(0.0, 33.0), 0.0)),
            Visibility::default(),
            Player { index: 0 },
            Velocity(Vec2::ZERO),
            ModelTint::new(RIDER_HAT_NODES, hat_mat, true),
        ))
        .id();
    commands.entity(player).with_children(|c| {
        c.spawn((
            WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(HORSE_MODEL))),
            Transform::from_rotation(model_yaw),
            Animated { rig: Rig::Horse, driver: player },
            MountRider { driver: player },
        ));
    });

    // --- Resources -------------------------------------------------------
    commands.insert_resource(GameLevel(level));
    commands.insert_resource(PlayerInputs(vec![Vec2::ZERO])); // one local player

    commands.init_resource::<JoystickState>();
}
