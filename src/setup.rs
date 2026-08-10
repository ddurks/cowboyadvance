//! Scene construction: camera, lighting, ground, pen, cows, player, and the 2D
//! HUD / joystick overlay. Runs once at `Startup`.

use bevy::camera::ScalingMode;
use bevy::prelude::*;

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
    //  0  background — clears the whole window black (the gutters around the
    //     fixed-aspect cabinet, and behind the frame's transparent areas).
    //  1  game — angled orthographic 3D, confined to a viewport (the cabinet's
    //     scene rect, resized each frame). `clear_color: None` so it draws over
    //     the black without wiping the gutters; the ground fills the viewport.
    //  2  UI — the frame pieces, joystick and HUD, on top.
    commands.spawn((
        Camera2d::default(),
        Camera { order: 0, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
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

    // --- Fences + gate posts --------------------------------------------
    let fence_mat = materials.add(StandardMaterial {
        base_color: srgb3(FENCE_COLOR),
        perceptual_roughness: 0.9,
        ..default()
    });
    for f in &level.fences {
        let mid = (f.a + f.b) * 0.5;
        let d = f.b - f.a;
        let size = if d.x.abs() >= d.y.abs() {
            Vec3::new(d.x.abs() + FENCE_THICK, FENCE_HEIGHT, FENCE_THICK)
        } else {
            Vec3::new(FENCE_THICK, FENCE_HEIGHT, d.y.abs() + FENCE_THICK)
        };
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
            MeshMaterial3d(fence_mat.clone()),
            Transform::from_translation(world(mid, FENCE_HEIGHT * 0.5)),
        ));
    }

    let post_mesh = meshes.add(Cuboid::new(0.6, GATE_POST_H, 0.6));
    let post_mat = materials.add(StandardMaterial {
        base_color: srgb3(POST_COLOR),
        ..default()
    });
    for &post in &level.posts {
        commands.spawn((
            Mesh3d(post_mesh.clone()),
            MeshMaterial3d(post_mat.clone()),
            Transform::from_translation(world(post, GATE_POST_H * 0.5)),
        ));
    }

    // --- Cows ------------------------------------------------------------
    let cow_mesh = meshes.add(Cuboid::new(COW_SIZE.x, COW_SIZE.y, COW_SIZE.z));
    let cow_mat = materials.add(StandardMaterial {
        base_color: srgb3(COW_COLOR),
        perceptual_roughness: 0.8,
        ..default()
    });
    let cow_penned_mat = materials.add(StandardMaterial {
        base_color: srgb3(COW_PENNED_COLOR),
        perceptual_roughness: 0.8,
        ..default()
    });
    // Front "face" marker — makes heading visible and stands in for a head.
    let marker_dark = materials.add(StandardMaterial {
        base_color: srgb3(MARKER_DARK),
        ..default()
    });
    let cow_marker_mesh = meshes.add(Cuboid::new(0.55, 0.5, 0.4));
    for i in 0..COW_COUNT {
        // Herd starts clustered near the bottom of the field (+Z).
        let x = -14.0 + hash01(i as u64 * 2 + 1) * 28.0;
        let z = 16.0 + hash01(i as u64 * 2 + 2) * 18.0;
        commands
            .spawn((
                Mesh3d(cow_mesh.clone()),
                MeshMaterial3d(cow_mat.clone()),
                Transform::from_translation(world(Vec2::new(x, z), COW_SIZE.y * 0.5)),
                Cow { penned: false },
                Velocity(Vec2::ZERO),
            ))
            .with_children(|c| {
                c.spawn((
                    Mesh3d(cow_marker_mesh.clone()),
                    MeshMaterial3d(marker_dark.clone()),
                    // Local -Z is forward, so the marker leads the way.
                    Transform::from_xyz(0.0, 0.15, -COW_SIZE.z * 0.5),
                ));
            });
    }

    // --- Player ----------------------------------------------------------
    let player_mesh = meshes.add(Cuboid::new(PLAYER_SIZE.x, PLAYER_SIZE.y, PLAYER_SIZE.z));
    let marker_light = materials.add(StandardMaterial {
        base_color: srgb3(MARKER_LIGHT),
        ..default()
    });
    let player_marker_mesh = meshes.add(Cuboid::new(0.75, 0.6, 0.5));
    commands
        .spawn((
            Mesh3d(player_mesh),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: srgb3(player_color(0)),
                perceptual_roughness: 0.6,
                ..default()
            })),
            Transform::from_translation(world(Vec2::new(0.0, 33.0), PLAYER_SIZE.y * 0.5)),
            Player { index: 0 },
            Velocity(Vec2::ZERO),
        ))
        .with_children(|c| {
            c.spawn((
                Mesh3d(player_marker_mesh),
                MeshMaterial3d(marker_light),
                Transform::from_xyz(0.0, PLAYER_SIZE.y * 0.2, -PLAYER_SIZE.z * 0.5),
            ));
        });

    // --- Resources -------------------------------------------------------
    commands.insert_resource(GameLevel(level));
    commands.insert_resource(GameAssets {
        cow: cow_mat,
        cow_penned: cow_penned_mat,
    });
    commands.insert_resource(PlayerInputs(vec![Vec2::ZERO])); // one local player

    commands.init_resource::<JoystickState>();
}
