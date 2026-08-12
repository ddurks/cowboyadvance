//! Scene construction: camera, lighting, ground, pen, the herd, the mounted
//! player, and the 2D HUD / joystick overlay. Runs once at `Startup`.
//!
//! The characters are only *requested* here — glTF scenes load asynchronously,
//! so `anim` finishes assembling them (rider onto saddle, clips onto players)
//! whenever they actually turn up.

use bevy::asset::RenderAssetUsages;
use bevy::camera::ScalingMode;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::CascadeShadowConfigBuilder;
use bevy::math::Affine2;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::anim::{Animated, MountRider, Rig};
use crate::components::*;
use crate::config::*;
use crate::level::Level;
use crate::noise::{hash01, lerp, smoothstep};

/// Lift a ground position (X, Z) to a world translation at height `y`.
fn world(pos: Vec2, y: f32) -> Vec3 {
    Vec3::new(pos.x, y, pos.y)
}

/// Tileable 2D value noise: the lattice wraps at `cells`, so opposite texture
/// edges meet seamlessly and the ground can repeat without seams —
/// `noise::value_noise` can't do this, its lattice is unbounded.
fn tile_noise(u: f32, v: f32, cells: usize, seed: u64) -> f32 {
    let p = cells as f32;
    let (x, y) = (u * p, v * p);
    let (xi, yi) = (x.floor() as usize, y.floor() as usize);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let corner = |dx: usize, dy: usize| {
        let cx = ((xi + dx) % cells) as u64;
        let cy = ((yi + dy) % cells) as u64;
        hash01((seed << 40) ^ (cx << 20) ^ cy)
    };
    let sx = smoothstep(0.0, 1.0, fx);
    let sy = smoothstep(0.0, 1.0, fy);
    lerp(
        lerp(corner(0, 0), corner(1, 0), sx),
        lerp(corner(0, 1), corner(1, 1), sx),
        sy,
    )
}

/// The field's tiling texture: `GROUND_COLOR` carrying the characters' foam
/// grain plus a short-grass mottle and sparse dirt speckles. Baked here rather
/// than shipped as an asset so `config` constants stay the tuning knobs.
fn ground_image(images: &mut Assets<Image>) -> Handle<Image> {
    let n = GROUND_TEX_SIZE;
    let base = GROUND_COLOR;
    let dirt = GROUND_DIRT_TINT;
    let mut data = Vec::with_capacity(n * n * 4);
    for y in 0..n {
        for x in 0..n {
            let (u, v) = (x as f32 / n as f32, y as f32 / n as f32);
            // Coarse grass patchiness, mid mottle, and the foam-scale grain.
            let b = 1.0
                + (tile_noise(u, v, 6, 1) - 0.5) * 2.0 * GROUND_PATCH_AMP
                + (tile_noise(u, v, 18, 2) - 0.5) * 2.0 * GROUND_MOTTLE_AMP
                + (tile_noise(u, v, 64, 3) - 0.5) * 2.0 * GROUND_GRAIN_AMP;
            // Value noise clusters around 0.5, so this threshold — not the
            // naive `1 - cover` — is what leaves a few percent as dirt.
            let m = smoothstep(
                GROUND_DIRT_THRESH,
                GROUND_DIRT_THRESH + 0.04,
                tile_noise(u, v, 90, 4),
            );
            let texel = [
                base.0 * b * lerp(1.0, dirt.0, m),
                base.1 * b * lerp(1.0, dirt.1, m),
                base.2 * b * lerp(1.0, dirt.2, m),
            ];
            for c in texel {
                data.push((c.clamp(0.0, 1.0) * 255.0) as u8);
            }
            data.push(255);
        }
    }

    let mut image = Image::new(
        Extent3d { width: n as u32, height: n as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..default()
    });
    images.add(image)
}

/// Hang a fresh horse model off a player entity; `MountRider` gets the rider
/// seated once the scene loads. Used at startup and again by `ragdoll` when a
/// thrown horse respawns.
pub fn spawn_horse_model(commands: &mut Commands, scenes: &ModelScenes, player: Entity) {
    commands.entity(player).with_children(|c| {
        c.spawn((
            WorldAssetRoot(scenes.horse.clone()),
            Transform::from_rotation(Quat::from_rotation_y(MODEL_YAW)),
            Animated { rig: Rig::Horse, driver: player },
            MountRider { driver: player },
        ));
    });
}

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
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
        // Punchy contrast without the `tonemapping_luts` feature (our trimmed
        // build lacks it, so Bevy's default LUT tonemapper would fall back).
        // KhronosPbrNeutral over AcesFitted: ACES hue-shifted the green field
        // to pastel mint and the brown hides to beige; this one keeps the
        // authored colours while still adding contrast.
        Tonemapping::KhronosPbrNeutral,
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

    let frame_top = asset_server.load("ui/frame_top.png");
    let frame_bottom = asset_server.load("ui/frame_bottom.png");
    let joystick_image = asset_server.load("ui/joystick.png");
    crate::cabinet::spawn_cabinet(&mut commands, ui_camera, frame_top, frame_bottom, joystick_image);

    // --- Lighting --------------------------------------------------------
    commands.spawn((
        DirectionalLight {
            illuminance: SUN_ILLUMINANCE,
            shadow_maps_enabled: true,
            ..default()
        },
        CascadeShadowConfigBuilder {
            num_cascades: 1,
            minimum_distance: SHADOW_MIN_DISTANCE,
            maximum_distance: SHADOW_MAX_DISTANCE,
            ..default()
        }
        .build(),
        Transform::from_translation(SUN_POSITION).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: AMBIENT_BRIGHTNESS,
        ..default()
    });
    // (Clear colors are set per-camera above: sky for the game, room for the UI.)

    // --- Ground ----------------------------------------------------------
    // Oversized so AutoMin's "extra" space fills with grass (not sky) on any
    // aspect ratio. The play area is bounded separately by FIELD_HALF_*.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(400.0, 400.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            // The colour lives in the baked texture (from GROUND_COLOR).
            base_color_texture: Some(ground_image(&mut images)),
            uv_transform: Affine2::from_scale(Vec2::splat(GROUND_TEX_REPEAT)),
            perceptual_roughness: crate::foam::FOAM_ROUGHNESS,
            reflectance: crate::foam::FOAM_REFLECTANCE,
            ..default()
        })),
    ));

    // Goal-zone tint: a short chute straddling the gap, so the target the cows
    // have to be driven through reads at a glance without a whole pen floor to
    // paint. Its width matches the opening; it runs a little either side of the
    // line, into the strip where captured cows walk off.
    let chute = Vec2::new(GATE_HALF * 2.0, 5.33);
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(chute.x, chute.y))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(PEN_TINT_COLOR.0, PEN_TINT_COLOR.1, PEN_TINT_COLOR.2, 0.5),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Transform::from_translation(world(Vec2::new(0.0, PEN_LINE_Z), 0.02)),
    ));

    // --- Fences ----------------------------------------------------------
    // The pen, plus the field edge, built from the modelled post and rail once
    // its .glb parses (`fence::spawn_fences_when_ready`).
    commands.insert_resource(crate::fence::PendingFences(asset_server.load(FENCE_MODEL)));

    // --- Characters ------------------------------------------------------
    // Gameplay lives on a bare transform; the model hangs off it as a child so
    // the sim never has to care that the art is a skinned glTF spun 180° to
    // face the way the game thinks is forward. Models stand on y = 0 — their
    // rigs are rooted at the feet. The models themselves arrive later:
    // `anim::finish_models` hangs them off these roots once the .glbs parse.

    // Swapped-in tints share the foam response so a penned cow or a dyed hat
    // doesn't suddenly turn glossy.
    let cow_penned_mat = materials.add(StandardMaterial {
        base_color: srgb3(COW_PENNED_COLOR),
        perceptual_roughness: crate::foam::FOAM_ROUGHNESS,
        reflectance: crate::foam::FOAM_REFLECTANCE,
        ..default()
    });
    for i in 0..COW_COUNT {
        // Herd starts clustered near the bottom of the field (+Z).
        let x = -9.33 + hash01(i as u64 * 2 + 1) * 18.67;
        let z = 10.67 + hash01(i as u64 * 2 + 2) * 12.0;
        commands.spawn((
            Transform::from_translation(world(Vec2::new(x, z), 0.0)),
            Visibility::default(),
            Cow { penned: false },
            Velocity(Vec2::ZERO),
            ModelTint::new(COW_HIDE_NODES, cow_penned_mat.clone(), false),
        ));
    }

    // The player is a horse; the rider is parented to its saddle joint once the
    // horse's scene exists (see `anim::mount_rider`).
    let hat_mat = materials.add(StandardMaterial {
        base_color: srgb3(player_color(0)),
        perceptual_roughness: crate::foam::FOAM_ROUGHNESS,
        reflectance: crate::foam::FOAM_REFLECTANCE,
        ..default()
    });
    commands.spawn((
        Transform::from_translation(world(PLAYER_SPAWN, 0.0)),
        Visibility::default(),
        Player { index: 0 },
        Velocity(Vec2::ZERO),
        ModelTint::new(RIDER_HAT_NODES, hat_mat, true),
    ));

    // --- Resources -------------------------------------------------------
    commands.insert_resource(GameLevel(level));
    commands.insert_resource(PlayerInputs(vec![Vec2::ZERO])); // one local player

    commands.init_resource::<JoystickState>();
}
