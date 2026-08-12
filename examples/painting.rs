//! A still "oil painting" of the cattle drive, rendered with the game's own
//! engine, models and grass — `cargo run --example painting`.
//!
//! The horses, cattle and riders are the same skinned glTF rigs the game spawns,
//! walking under the same gait system; the field is the game's grass. The example
//! adds a landscape camera, a procedural sky, rolling hills, aerial haze, and a
//! painterly filter over the finished frame.
//!
//! The scene is composed for a *narrow* lens (20° vertical) — the single most
//! important number in the file: the game's grass covers a fixed strip of world,
//! and a long lens keeps its edges outside the frame while compressing the
//! distance the way the reference painting does. The camera renders to a fixed
//! 1920x1080 image, not the window, so the output never depends on the monitor
//! or its DPI. Once the models have walked for a couple of seconds the frame is
//! captured and run through a generalised Kuwahara filter (flatten into brush
//! strokes while keeping edges), plus a warm grade and a canvas grain.
//!
//! Sky, hills, scrub and dust are all generated from one deterministic value
//! noise, so the picture is byte-identical every run.

#![allow(dead_code)]

#[path = "../src/anim.rs"]
mod anim;
#[path = "../src/cabinet.rs"]
mod cabinet;
#[path = "../src/components.rs"]
mod components;
#[path = "../src/config.rs"]
mod config;
#[path = "../src/fence.rs"]
mod fence;
#[path = "../src/foam.rs"]
mod foam;
#[path = "../src/grass.rs"]
mod grass;
#[path = "../src/hide.rs"]
mod hide;
#[path = "../src/level.rs"]
mod level;
#[path = "../src/noise.rs"]
mod noise;
#[path = "../src/setup.rs"]
mod setup;
#[path = "../src/sim.rs"]
mod sim;

use std::f32::consts::{PI, TAU};

use bevy::asset::RenderAssetUsages;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::log::{Level as LogLevel, LogPlugin};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured};

use crate::components::*;
use crate::config::*;
use crate::level::Level;
use crate::noise::{hash01, lerp, smoothstep, value_noise};

const OUT_W: u32 = 1920;
const OUT_H: u32 = 1080;
const RAW_PATH: &str = "renders/painting_raw.png";
const ART_PATH: &str = "renders/painting.png";

/// Vertical field of view. Deliberately long — see the module notes. At 25° the
/// game's grass strip (x = +/-36.5) only starts showing its side edges 96 units
/// out, by which point they are up against the horizon and read as a hedgerow.
const FOV_DEG: f32 = 22.5;

/// Eye height and station point: mounted, on a slight rise. Low enough that the
/// riders keep some height against the horizon, high enough to look *over* the
/// grass rather than through it — a good metre of tuft at eye level swallows
/// the horses' legs and the herd stops reading as a line going away.
///
/// The pitch is what sets the framing. At -3.7° the horizon lands a third of
/// the way down, and the ground comes into frame about 28 units out — which is
/// why nothing important is placed closer than thirty.
const EYE: Vec3 = Vec3::new(0.0, 7.5, 60.0);
const LOOK_AT: Vec3 = Vec3::new(1.3, 1.2, -37.0);

// ---------------------------------------------------------------------------
// Palette. sRGB triples, in the reference painting's key: warm low sun, dusty
// blue-green distance, a sky that runs from deep blue overhead to warm haze.
// ---------------------------------------------------------------------------

const SKY_ZENITH: (f32, f32, f32) = (0.20, 0.41, 0.77);
const SKY_HAZE: (f32, f32, f32) = (0.78, 0.82, 0.84);
const CLOUD_TOP: (f32, f32, f32) = (0.99, 0.97, 0.93);
const CLOUD_UNDER: (f32, f32, f32) = (0.58, 0.60, 0.67);

// Kept close together on purpose. Strong dry/green variation reads as real
// country; a cartoon background wants a hill to be one green shape with just
// enough shading to turn the form.
const HILL_GREEN: (f32, f32, f32) = (0.30, 0.49, 0.24);
const HILL_DRY: (f32, f32, f32) = (0.39, 0.53, 0.26);
const PLAIN: (f32, f32, f32) = (0.32, 0.50, 0.25);
const SCRUB: (f32, f32, f32) = (0.19, 0.26, 0.15);

/// Greened off from the sky's own haze on purpose: whatever bare ground is left
/// at the horizon fades into this, and a blue-grey there reads as a strip of
/// water under the hills rather than as distance.
const HAZE_FOG: (f32, f32, f32) = (0.66, 0.75, 0.74);
const SUN_TINT: (f32, f32, f32) = (1.0, 0.94, 0.82);
const DUST: (f32, f32, f32) = (0.84, 0.78, 0.63);

/// Sun position (it only matters as a direction). Low and to the right, so the
/// riders are rim-lit down one side and throw long shadows across the field.
const SUN_POS: Vec3 = Vec3::new(92.0, 40.0, 26.0);
const SUN_LUX: f32 = 1900.0;

/// A second light just off the camera's shoulder, casting no shadows. A low sun
/// alone turns everything facing the lens into a silhouette — which is dramatic
/// but hides the tack, the hats and the horses' faces. This is the portrait
/// photographer's fill: enough to open the shadows, too soft to cast its own.
const FILL_POS: Vec3 = Vec3::new(26.0, 30.0, 96.0);
const FILL_LUX: f32 = 1500.0;
const FILL_TINT: (f32, f32, f32) = (0.94, 0.96, 1.0);

const AMBIENT: (f32, f32, f32) = (0.64, 0.75, 0.95);
const AMBIENT_BRIGHTNESS: f32 = 300.0;

const HERD: usize = 64;

/// The three riders: ground position, the way they are looking, how fast they
/// are moving (which is what picks their gait), and the player slot whose
/// colour dyes their hat.
const RIDERS: [(Vec2, Vec2, f32, usize); 3] = [
    // Foreground left, closest to the eye — the painting's lone horseman. Reined
    // round to three-quarters, so the lead rider shows a face and a shoulder
    // instead of the back of a hat. Held out at 34 units: any nearer and the
    // horse's legs fall out of the bottom of the frame.
    (Vec2::new(-6.5, 26.0), Vec2::new(0.78, 0.63), 4.2, 0),
    // Riding the right flank of the herd, mid-distance.
    (Vec2::new(12.5, -14.0), Vec2::new(0.16, -0.99), 5.2, 1),
    // Drag rider, left and further back, small in the haze.
    (Vec2::new(-9.0, -52.0), Vec2::new(0.30, -0.95), 4.6, 2),
];

/// Cattle that have stopped and turned to look at the viewer, by index into the
/// herd, with the way each is facing. A column where every animal points the
/// same way reads as texture; two heads up and turned out is what makes it read
/// as animals. Both are early in the line, so they are near enough for a face,
/// horns and hide markings to carry.
const TURNED_COWS: [(usize, Vec2); 2] =
    [(1, Vec2::new(-0.40, 0.92)), (7, Vec2::new(0.58, 0.81))];
/// Slow enough that the gait system idles them — stopped and staring, rather
/// than walking at the camera.
const TURNED_SPEED: f32 = 0.8;

/// Centreline of the column of cattle, as a function of how far along it is.
fn herd_point(s: f32) -> Vec2 {
    Vec2::new(1.0 + 16.0 * s + 7.0 * (s * 3.4).sin(), 22.0 - 190.0 * s)
}

/// How wide the column is at `s`. It tightens as it recedes, which is most of
/// what makes the line read as going away rather than fanning out.
fn herd_spread(s: f32) -> f32 {
    lerp(7.0, 2.0, s)
}

fn main() {
    App::new()
        .add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Cowboy Advance — painting".into(),
                    // The canvas *is* the window: an explicit scale factor
                    // pins it to exactly OUT_W x OUT_H physical pixels, so the
                    // picture never depends on the monitor's DPI.
                    resolution: bevy::window::WindowResolution::new(OUT_W, OUT_H)
                        .with_scale_factor_override(1.0),
                    resizable: false,
                    ..default()
                }),
                ..default()
            })
            .set(LogPlugin {
                level: LogLevel::INFO,
                filter: "wgpu=error,naga=warn,bevy=warn,bevy_core_pipeline=error,bevy_pbr=error"
                    .into(),
                ..default()
            }),
    )
    .add_plugins(grass::GrassPlugin)
    // Grass builds itself at `Startup` and reads the level, so the resource has
    // to exist before any startup system runs — not be inserted by one.
    .insert_resource(GameLevel(Level::build()))
    .init_resource::<Shoot>()
    .add_systems(Startup, (anim::load_models, build_scene).chain())
    // After `Startup`, so the grass plugin's own chunks already exist.
    .add_systems(PostStartup, extend_grass)
    .add_systems(
        Update,
        (
            anim::finish_models,
            anim::mount_rider,
            anim::bind_animation_players,
            anim::apply_model_tint,
            anim::drive_gaits,
            capture,
            finish,
        )
            .chain(),
    )
    .run();
}

/// Drives the one thing this example does over time: wait for the models, let
/// them walk, take the shot, quit.
#[derive(Resource, Default)]
struct Shoot {
    /// Frames since every rig finished loading.
    warmed: u32,
    fired: bool,
    saved: bool,
}

/// Rigs to wait for: one animation player per horse, per rider, per cow.
const EXPECTED_RIGS: usize = RIDERS.len() * 2 + HERD;
/// Frames of walking after the last rig loads. Long enough for the herd's
/// strides to fall out of lockstep and for hooves to wear the grass down.
const WARMUP_FRAMES: u32 = 150;

fn capture(
    mut commands: Commands,
    mut shoot: ResMut<Shoot>,
    rigs: Query<(), With<anim::GaitState>>,
    drawn: Query<(), With<Mesh3d>>,
) {
    if shoot.fired {
        return;
    }
    if rigs.iter().count() < EXPECTED_RIGS {
        return; // rigs still streaming in
    }
    shoot.warmed += 1;
    if shoot.warmed < WARMUP_FRAMES {
        return;
    }

    info!("capturing: {} rigs, {} meshes", rigs.iter().count(), drawn.iter().count());
    let _ = std::fs::create_dir_all("renders");
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(RAW_PATH))
        .observe(develop);
    shoot.fired = true;
}

fn finish(shoot: Res<Shoot>, mut exit: MessageWriter<AppExit>) {
    if shoot.saved {
        exit.write(AppExit::Success);
    }
}

fn build_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    commands.spawn((
        Camera3d::default(),
        Camera { clear_color: ClearColorConfig::Custom(srgb3(SKY_HAZE)), ..default() },
        Projection::from(PerspectiveProjection {
            fov: FOV_DEG.to_radians(),
            near: 0.4,
            far: 6000.0,
            ..default()
        }),
        Transform::from_translation(EYE).looking_at(LOOK_AT, Vec3::Y),
        // Aerial perspective. Linear rather than exponential because the whole
        // point is to place the hills at a chosen strength by hand: barely any
        // haze over the grass, half-veiled hills, a horizon that dissolves.
        DistanceFog {
            color: srgb3(HAZE_FOG),
            directional_light_color: srgb3(SUN_TINT),
            directional_light_exponent: 24.0,
            // Held right back. Everything from the far grass to infinity is
            // squeezed into the last degree below the horizon, so haze that
            // bites before a kilometre turns that sliver into a pale band —
            // and a pale band under the hills is exactly what makes them look
            // like they are floating rather than standing on the ground. The
            // grass shader ignores fog too, so anything stronger would veil
            // the hills while the field in front of them stayed saturated.
            falloff: FogFalloff::Linear { start: 400.0, end: 5000.0 },
        },
        Msaa::Sample4,
    ));

    // Sky dome, unlit and fog-exempt: it *is* the far distance, so nothing
    // should veil it.
    let sky_tex = images.add(build_sky_texture());
    commands.spawn((
        Mesh3d(meshes.add(build_sky_dome())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(sky_tex),
            unlit: true,
            fog_enabled: false,
            cull_mode: None,
            ..default()
        })),
        Transform::from_translation(EYE),
    ));

    commands.spawn((
        DirectionalLight {
            color: srgb3(SUN_TINT),
            illuminance: SUN_LUX,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_translation(SUN_POS).looking_at(Vec3::ZERO, Vec3::Y),
        // The riders are the only things close enough for shadow detail to
        // matter; past a couple of hundred units the haze has taken over.
        CascadeShadowConfigBuilder {
            num_cascades: 4,
            minimum_distance: 0.4,
            first_cascade_far_bound: 28.0,
            maximum_distance: 260.0,
            ..default()
        }
        .build(),
    ));
    commands.spawn((
        DirectionalLight {
            color: srgb3(FILL_TINT),
            illuminance: FILL_LUX,
            // No shadows: a fill that casts is a second sun, and two sets of
            // shadows crossing each other is the fastest way to look synthetic.
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_translation(FILL_POS).looking_at(Vec3::new(0.0, 0.0, -20.0), Vec3::Y),
    ));
    commands.insert_resource(GlobalAmbientLight {
        color: srgb3(AMBIENT),
        brightness: AMBIENT_BRIGHTNESS,
        ..default()
    });

    // The hills mesh is dead flat under the grass and only starts swelling a
    // couple of hundred units out, so there is no seam anywhere the camera can
    // see. This plane behind it is just a backstop for the corners.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(14000.0, 14000.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: srgb3(PLAIN),
            perceptual_roughness: 1.0,
            ..default()
        })),
        Transform::from_xyz(0.0, -0.05, 0.0),
    ));
    commands.spawn((
        Mesh3d(meshes.add(build_hills())),
        MeshMaterial3d(materials.add(StandardMaterial {
            // White, because the hill colours ride on the mesh's vertex colours
            // — that is what lets ridge tops go dry and hollows stay green.
            base_color: Color::WHITE,
            perceptual_roughness: 1.0,
            ..default()
        })),
    ));

    // Sage clumps, thickest around the line where the game's grass stops, which
    // is exactly the edge that would otherwise read as a seam.
    let scrub_mesh = meshes.add(build_shrub());
    let scrub_mat = materials.add(StandardMaterial {
        base_color: srgb3(SCRUB),
        perceptual_roughness: 1.0,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    for i in 0..170u64 {
        let x = (hash01(i * 7 + 1) - 0.5) * 190.0;
        let z = lerp(56.0, -180.0, hash01(i * 7 + 2));
        // Thin them out where the herd walks, so the column stays readable.
        let near_herd = (x - herd_point(((22.0 - z) / 190.0).clamp(0.0, 1.0)).x).abs();
        if near_herd < 9.0 && hash01(i * 7 + 3) < 0.75 {
            continue;
        }
        let s = lerp(0.7, 2.1, hash01(i * 7 + 4));
        commands.spawn((
            Mesh3d(scrub_mesh.clone()),
            MeshMaterial3d(scrub_mat.clone()),
            Transform::from_xyz(x, 0.0, z)
                .with_scale(Vec3::new(s, s * lerp(0.6, 1.1, hash01(i * 7 + 5)), s))
                .with_rotation(Quat::from_rotation_y(hash01(i * 7 + 6) * TAU)),
        ));
    }

    // Soft warm cards hanging over the column: the one thing that makes a line
    // of cattle read as *moving* in a still image.
    let dust_tex = images.add(build_dust_texture());
    let dust_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(DUST.0, DUST.1, DUST.2, 0.26),
        base_color_texture: Some(dust_tex),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        fog_enabled: false,
        cull_mode: None,
        ..default()
    });
    for i in 0..14u64 {
        let s = lerp(0.18, 0.95, hash01(i * 11 + 1));
        let p = herd_point(s);
        let w = lerp(16.0, 52.0, s) * lerp(0.7, 1.3, hash01(i * 11 + 2));
        let h = w * lerp(0.35, 0.6, hash01(i * 11 + 3));
        let pos = Vec3::new(
            p.x + (hash01(i * 11 + 4) - 0.5) * w * 0.5,
            h * 0.36,
            p.y + (hash01(i * 11 + 5) - 0.5) * 14.0,
        );
        commands.spawn((
            Mesh3d(meshes.add(Mesh::from(Rectangle::new(w, h)))),
            MeshMaterial3d(dust_mat.clone()),
            Transform::from_translation(pos).looking_at(EYE, Vec3::Y),
        ));
    }

    // The models themselves hang off these roots once `anim::finish_models`
    // sees the .glbs parsed.
    let penned_mat = materials.add(StandardMaterial {
        base_color: srgb3(COW_PENNED_COLOR),
        perceptual_roughness: 0.8,
        ..default()
    });

    for i in 0..HERD {
        let s = (i as f32 + hash01(i as u64 * 3 + 1) * 0.7) / HERD as f32;
        let mid = herd_point(s);
        let across = herd_spread(s) * (hash01(i as u64 * 3 + 2) - 0.5) * 2.0;
        let pos = mid + Vec2::new(across, (hash01(i as u64 * 3 + 3) - 0.5) * 5.0);

        // Head the way the column is going, with a little wander so the herd
        // does not look like a marching band — unless this is one of the two
        // that have turned out of the line to look at us.
        //
        // Speed drives the gait *and* the playback rate, so spreading it is
        // what stops all sixty-four cows stepping on the same beat.
        let (dir, speed) = match TURNED_COWS.iter().find(|(idx, _)| *idx == i) {
            Some((_, look)) => (look.normalize(), TURNED_SPEED),
            None => {
                let along = (herd_point((s + 0.02).min(1.0)) - mid).normalize_or_zero();
                let wander = (hash01(i as u64 * 3 + 4) - 0.5) * 0.5;
                (
                    Vec2::from_angle(wander).rotate(along),
                    lerp(2.6, 4.4, hash01(i as u64 * 3 + 5)),
                )
            }
        };

        commands.spawn((
            Transform::from_translation(ground(pos)).with_rotation(face(dir)),
            Visibility::default(),
            Cow { penned: false },
            Velocity(dir * speed),
            ModelTint::new(COW_HIDE_NODES, penned_mat.clone(), false),
        ));
    }

    for (pos, dir, speed, slot) in RIDERS {
        let hat = materials.add(StandardMaterial {
            base_color: srgb3(player_color(slot)),
            perceptual_roughness: 0.7,
            ..default()
        });
        commands.spawn((
            Transform::from_translation(ground(pos)).with_rotation(face(dir)),
            Visibility::default(),
            Player { index: slot },
            Velocity(dir * speed),
            ModelTint::new(RIDER_HAT_NODES, hat, true),
        ));
    }
}

// ---------------------------------------------------------------------------
// Grass, extended
//
// The game grows grass over one fixed strip — the play field plus a margin —
// because that is all its top-down camera can ever see. A landscape camera can
// see to the horizon, and the strip's far edge cuts a hard line across the
// middle distance with bare ground beyond it.
//
// Rather than grow more (the tuft meshes take real time to build, and the
// module's extent is the game's business), this re-uses the chunks that already
// exist: same mesh handles, same material, just more transforms. The wind phase
// is a function of world position inside the shader, so tiled copies do not
// sway in lockstep, and by the time the pattern would repeat visibly it is a
// few hundred units away.
// ---------------------------------------------------------------------------

/// Footprint of one grass tile. Mirrors `grass.rs`, which grows the field plus
/// `GRASS_MARGIN` on every side — if that margin changes, change it here.
const GRASS_MARGIN: f32 = 16.0;
const GRASS_TILE: Vec2 = Vec2::new(
    FIELD_HALF_X * 2.0 + GRASS_MARGIN * 2.0,
    FIELD_Z_NEAR + FIELD_HALF_Z + GRASS_MARGIN * 2.0,
);
/// Tiles either side, and beyond. This has to reach much further than it looks
/// like it should: a flat plain past ~500 units occupies less than a degree
/// below the horizon, so the strip of *bare* ground between the last grass and
/// the hills is thin, bright, and reads as a hard seam. Tiling to roughly 1200
/// units shrinks it to nothing. `in_frame` throws away the tiles this generous
/// bound puts outside the lens.
const GRASS_TILES_X: i32 = 6;
const GRASS_TILES_Z: i32 = 13;

/// Two rejections. The camera looks down -Z with a half-width of ~0.36 per unit
/// of depth, so anything well outside that cone is wasted; and tiles are flat at
/// y = 0, so one sitting on rising ground would bury its tufts in a hillside and
/// leave them poking through the slope. The slack in both covers a tile's own
/// size and keeps the test cheap and obviously safe.
fn in_frame(centre: Vec3) -> bool {
    let depth = (EYE.z - centre.z).max(0.0);
    centre.x.abs() - GRASS_TILE.x <= depth * 0.40 + 40.0
        && hill_height(centre.x, centre.z) < 14.0
}

/// Tufts are authored for a camera looking down at 45°, where a metre of grass
/// is a few pixels. From near ground level the same tuft stands up to a horse's
/// knee and buries the herd, so they are pressed to half height here. Scaling Y
/// alone is the whole trick — scaling X or Z as well would drag every tuft
/// toward its chunk's centre and open gaps along the chunk seams.
const GRASS_HEIGHT_SCALE: f32 = 0.5;

fn extend_grass(
    mut commands: Commands,
    mut chunks: Query<(&Mesh3d, &MeshMaterial3d<grass::GrassMaterial>, &mut Transform)>,
) {
    let mut copies = Vec::new();
    for (mesh, material, mut transform) in &mut chunks {
        transform.scale.y = GRASS_HEIGHT_SCALE;
        for ix in -GRASS_TILES_X..=GRASS_TILES_X {
            for iz in 0..=GRASS_TILES_Z {
                if ix == 0 && iz == 0 {
                    continue; // the original chunk, already placed
                }
                let mut tiled = *transform;
                tiled.translation.x += ix as f32 * GRASS_TILE.x;
                tiled.translation.z -= iz as f32 * GRASS_TILE.y;
                if !in_frame(tiled.translation) {
                    continue;
                }
                copies.push((Mesh3d(mesh.0.clone()), MeshMaterial3d(material.0.clone()), tiled));
            }
        }
    }
    info!("grass: {} tiled copies out to the horizon", copies.len());
    commands.spawn_batch(copies);
}

/// Lift a ground position to a world translation (the sim is on X/Z).
fn ground(p: Vec2) -> Vec3 {
    Vec3::new(p.x, 0.0, p.y)
}

/// Yaw that points an entity's local -Z along `dir` — the game's convention
/// (see `systems::player_movement`).
fn face(dir: Vec2) -> Quat {
    Quat::from_rotation_y((-dir.x).atan2(-dir.y))
}

const SKY_SEGS: usize = 128;
const SKY_RINGS: usize = 48;
const SKY_RADIUS: f32 = 2400.0;
/// A little below level, so the dome's rim is safely under the horizon line.
const SKY_ELEV_MIN: f32 = -0.14;
/// Rings bunch toward the horizon, where all the interesting compression is.
const SKY_RING_BIAS: f32 = 1.9;

/// Elevation/azimuth for a dome UV. Shared by the mesh and the texture so the
/// two cannot drift apart.
fn sky_dir(u: f32, v: f32) -> Vec3 {
    let elev = SKY_ELEV_MIN + (PI * 0.5 - SKY_ELEV_MIN) * v.powf(SKY_RING_BIAS);
    let az = u * TAU;
    Vec3::new(elev.cos() * az.sin(), elev.sin(), elev.cos() * az.cos())
}

fn build_sky_dome() -> Mesh {
    let mut pos = Vec::new();
    let mut nrm = Vec::new();
    let mut uv = Vec::new();
    let mut idx = Vec::new();

    for ring in 0..=SKY_RINGS {
        let v = ring as f32 / SKY_RINGS as f32;
        for seg in 0..=SKY_SEGS {
            let u = seg as f32 / SKY_SEGS as f32;
            let d = sky_dir(u, v);
            pos.push((d * SKY_RADIUS).to_array());
            nrm.push((-d).to_array()); // inward, though the dome is unlit
            uv.push([u, v]);
        }
    }
    let stride = SKY_SEGS + 1;
    for ring in 0..SKY_RINGS {
        for seg in 0..SKY_SEGS {
            let a = (ring * stride + seg) as u32;
            let (b, c, d) = (a + 1, a + stride as u32, a + stride as u32 + 1);
            idx.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }

    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_indices(Indices::U32(idx))
}

const SKY_TEX_W: u32 = 2048;
const SKY_TEX_H: u32 = 640;

fn build_sky_texture() -> Image {
    let mut data = Vec::with_capacity((SKY_TEX_W * SKY_TEX_H * 4) as usize);
    for y in 0..SKY_TEX_H {
        let v = (y as f32 + 0.5) / SKY_TEX_H as f32;
        for x in 0..SKY_TEX_W {
            let u = (x as f32 + 0.5) / SKY_TEX_W as f32;
            let c = sky_color(sky_dir(u, v));
            data.extend_from_slice(&[
                (c.x.clamp(0.0, 1.0) * 255.0) as u8,
                (c.y.clamp(0.0, 1.0) * 255.0) as u8,
                (c.z.clamp(0.0, 1.0) * 255.0) as u8,
                255,
            ]);
        }
    }
    Image::new(
        Extent3d { width: SKY_TEX_W, height: SKY_TEX_H, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Sky and cloud colour for a view direction.
///
/// The clouds live on a flat plane overhead, sampled by projecting the view ray
/// onto it — which is why they compress toward the horizon on their own, with
/// no fudge factor. Lighting comes from comparing the cloud density here with
/// the density a short step toward the sun: more cloud that way means this face
/// is in shadow, which is enough to model cumulus tops and grey undersides.
fn sky_color(d: Vec3) -> Vec3 {
    let up = d.y.max(0.0);
    let base = vlerp(v3(SKY_ZENITH), v3(SKY_HAZE), (1.0 - up).powf(4.5));
    if d.y <= 0.004 {
        return base;
    }

    // A softening term in the denominator. Projecting straight onto the cloud
    // plane is the honest thing to do, and it is unusable: on a long lens the
    // whole visible sky sits below 10 degrees, where true perspective squashes
    // every cloud into a horizontal smear. Landscape painters cheat exactly
    // here, drawing near-horizon clouds far larger than perspective allows, and
    // `+ 0.28` is that cheat — the compression is kept, its blow-up is not.
    let q = Vec2::new(d.x, d.z) / (d.y + 0.28) * 1.6;
    let cover = fbm(q, 5);
    let cluster = fbm(q * 0.21 + Vec2::new(31.7, 11.3), 3);
    let thresh = 0.51 - 0.24 * smoothstep(0.36, 0.74, cluster);

    let mut amount = smoothstep(thresh, thresh + 0.10, cover);
    // Dissolve into haze at the horizon rather than stacking to a hard band.
    amount *= smoothstep(0.004, 0.05, d.y);
    if amount <= 0.0 {
        return base;
    }

    let toward_sun = Vec2::new(SUN_POS.x, SUN_POS.z).normalize_or_zero() * 0.42;
    let shadowed = fbm(q + toward_sun, 5) - cover;
    let lit = smoothstep(-0.14, 0.09, -shadowed);
    let cloud = vlerp(v3(CLOUD_UNDER), v3(CLOUD_TOP), lit);
    vlerp(base, cloud, amount)
}

const HILL_NX: usize = 260;
const HILL_NZ: usize = 190;
const HILL_X: f32 = 4200.0;
const HILL_Z_FAR: f32 = -3200.0;
/// Starts behind the eye so the mesh's near edge is never in frame.
const HILL_Z_NEAR: f32 = 90.0;

/// The hills, placed by hand: centre x, centre z, radius, height.
///
/// Noise makes convincing terrain and *unconvincing* cartoon hills — it gives
/// you crumpled, busy country with no silhouette you could draw from memory.
/// These are a handful of big smooth domes instead, laid out so their outlines
/// overlap into a readable near/far stack. Their feet run well forward of where
/// the grass gives out, so the range grows out of the field rather than
/// floating behind a band of haze.
/// A hill only shows the part of itself that clears the eye, so what lands on
/// screen is roughly `(height - eye) / distance` radians against a 0.39 rad
/// frame. These sit a kilometre-plus out at 60-105 high, which is an eighth of
/// the frame each — near enough and they stop being background and start being
/// a wall.
const HILLS: [(f32, f32, f32, f32); 7] = [
    (-450.0, -950.0, 420.0, 62.0),
    (380.0, -1150.0, 520.0, 78.0),
    (1100.0, -1000.0, 430.0, 56.0),
    (-1250.0, -1250.0, 560.0, 72.0),
    (-100.0, -1650.0, 700.0, 96.0),
    (1500.0, -1800.0, 650.0, 88.0),
    (-1750.0, -2000.0, 750.0, 104.0),
];

/// Height of the ground at (x, z): dead flat across the play field, a whisper
/// of a swell after that, then the domes.
///
/// The domes combine with `max` rather than by adding, which is what keeps them
/// reading as separate hills — summing them would melt the lot into one lump,
/// and the creases where two silhouettes cross are exactly the lines a cartoon
/// background is drawn with.
fn hill_height(x: f32, z: f32) -> f32 {
    let d = (-z - 40.0).max(0.0);
    let mut h = smoothstep(90.0, 320.0, d) * 5.0 * fbm(Vec2::new(x, z) * 0.0042, 3);
    for (hx, hz, radius, height) in HILLS {
        let t = Vec2::new(x - hx, z - hz).length() / radius;
        // Smoothstep from foot to crown: flat where it meets the ground, flat
        // over the top, all the curvature in between. A round hill, drawn.
        h = h.max(height * smoothstep(1.0, 0.05, t));
    }
    h
}

fn build_hills() -> Mesh {
    let mut pos = Vec::new();
    let mut nrm = Vec::new();
    let mut col = Vec::new();
    let mut idx = Vec::new();

    let eps = 2.5;
    for iz in 0..=HILL_NZ {
        let z = lerp(HILL_Z_NEAR, HILL_Z_FAR, iz as f32 / HILL_NZ as f32);
        for ix in 0..=HILL_NX {
            let x = lerp(-HILL_X, HILL_X, ix as f32 / HILL_NX as f32);
            let h = hill_height(x, z);
            pos.push([x, h, z]);

            let dx = hill_height(x + eps, z) - hill_height(x - eps, z);
            let dz = hill_height(x, z + eps) - hill_height(x, z - eps);
            let n = Vec3::new(-dx, 2.0 * eps, -dz).normalize();
            nrm.push(n.to_array());

            // Crowns and steep flanks lighten a little; the flat field stays as
            // it is. Deliberately understated — see the palette note.
            let dry = (smoothstep(20.0, 70.0, h) * 0.5
                + (1.0 - n.y) * 0.9
                + fbm(Vec2::new(x, z) * 0.02, 3) * 0.2
                - 0.10)
                .clamp(0.0, 1.0);
            let base = vlerp(vlerp(v3(PLAIN), v3(HILL_GREEN), smoothstep(1.0, 14.0, h)), v3(HILL_DRY), dry);
            let lin = Color::srgb(base.x, base.y, base.z).to_linear();
            col.push([lin.red, lin.green, lin.blue, 1.0]);
        }
    }

    let stride = HILL_NX + 1;
    for iz in 0..HILL_NZ {
        for ix in 0..HILL_NX {
            let a = (iz * stride + ix) as u32;
            let (b, c, d) = (a + 1, a + stride as u32, a + stride as u32 + 1);
            idx.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }

    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32(idx))
}

/// A sage clump: a ring of leaning triangles, in the same spirit as the game's
/// grass tufts — chunky, opaque, and two-sided so winding never matters.
fn build_shrub() -> Mesh {
    let mut pos = Vec::new();
    let mut nrm = Vec::new();
    let mut idx = Vec::new();

    let blades = 7;
    for k in 0..blades {
        let a = k as f32 / blades as f32 * TAU;
        let (s, c) = a.sin_cos();
        let out = Vec3::new(s, 0.0, c);
        let side = Vec3::new(c, 0.0, -s) * 0.34;
        let base = out * 0.55;
        let apex = out * 0.22 + Vec3::Y * lerp(0.7, 1.0, hash01(k as u64 + 5));

        let n = (apex - base).cross(side * 2.0).normalize_or_zero();
        let start = pos.len() as u32;
        for p in [base - side, base + side, apex] {
            pos.push(p.to_array());
            nrm.push(n.to_array());
        }
        idx.extend_from_slice(&[start, start + 1, start + 2]);
    }

    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_indices(Indices::U32(idx))
}

const DUST_TEX: u32 = 128;

/// A soft blob: radial falloff broken up with noise so the cards do not read as
/// circles when they overlap.
fn build_dust_texture() -> Image {
    let mut data = Vec::with_capacity((DUST_TEX * DUST_TEX * 4) as usize);
    for y in 0..DUST_TEX {
        for x in 0..DUST_TEX {
            let p = Vec2::new(x as f32, y as f32) / DUST_TEX as f32 * 2.0 - Vec2::ONE;
            let r = p.length();
            let n = fbm(p * 3.4 + Vec2::new(7.0, 3.0), 4);
            let a = (1.0 - smoothstep(0.15, 1.0, r)) * lerp(0.45, 1.0, n);
            data.extend_from_slice(&[255, 255, 255, (a.clamp(0.0, 1.0) * 255.0) as u8]);
        }
    }
    Image::new(
        Extent3d { width: DUST_TEX, height: DUST_TEX, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

/// Seed for everything generated here, so the picture is identical every run.
/// Each octave gets its own derived seed — reusing one would make the octaves
/// correlate and the noise band.
const SEED: u64 = 0x9A17_C0DE;

/// Fractal sum of the game's own value noise. Octaves step by 2.03 rather than
/// exactly 2 so lattice corners from different octaves never line up.
fn fbm(mut p: Vec2, octaves: u32) -> f32 {
    let (mut amp, mut sum, mut norm) = (0.5, 0.0, 0.0);
    for i in 0..octaves {
        sum += amp * value_noise(p, 1.0, SEED ^ (i as u64 * 0x2545_F491));
        norm += amp;
        p *= 2.03;
        amp *= 0.5;
    }
    sum / norm
}

/// Ridged variant: folds the noise at its midpoint, which turns rounded blobs
/// into the creased ridge lines a range of hills actually has.
fn ridged(mut p: Vec2, octaves: u32) -> f32 {
    let (mut amp, mut sum, mut norm) = (0.5, 0.0, 0.0);
    for i in 0..octaves {
        let n = value_noise(p, 1.0, SEED ^ (i as u64 * 0x9E37_79B1));
        sum += amp * (1.0 - (n * 2.0 - 1.0).abs());
        norm += amp;
        p *= 2.11;
        amp *= 0.5;
    }
    sum / norm
}

fn v3(c: (f32, f32, f32)) -> Vec3 {
    Vec3::new(c.0, c.1, c.2)
}

fn vlerp(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// The painterly pass
//
// Runs on the CPU over the captured frame. A generalised Kuwahara filter is the
// whole trick: for every pixel it looks at eight wedges of a disc around it and
// takes the mean of whichever wedge has the flattest colour. Flat areas get
// pooled into strokes, but an edge always has one wedge lying entirely on one
// side of it, so edges survive — which is exactly the property that makes it
// read as paint rather than as blur.
//
// Radius 4 is deliberately restrained: big enough to see brushwork, small
// enough that a cow at two hundred units is still a cow.
// ---------------------------------------------------------------------------

const BRUSH_RADIUS: i32 = 4;
const SECTORS: usize = 8;
/// How far to pull toward the filtered result. Holding a little of the original
/// back keeps fine detail (grass, tack, distant cattle) from going to mush.
const BRUSH_MIX: f32 = 0.85;

fn develop(shot: On<ScreenshotCaptured>, mut state: ResMut<Shoot>) {
    let img = &shot.image;
    let w = img.texture_descriptor.size.width as usize;
    let h = img.texture_descriptor.size.height as usize;
    let Some(src) = img.data.as_ref() else {
        error!("screenshot had no data");
        state.saved = true;
        return;
    };

    let start = std::time::Instant::now();
    let out = paint(src, w, h);
    match image::RgbImage::from_raw(w as u32, h as u32, out) {
        Some(buf) => match buf.save(ART_PATH) {
            Ok(()) => info!("painted {ART_PATH} in {:.1}s", start.elapsed().as_secs_f32()),
            Err(e) => error!("could not write {ART_PATH}: {e}"),
        },
        None => error!("painted buffer was the wrong size"),
    }
    state.saved = true;
}

/// Kuwahara, then grade, then canvas. Returns tightly packed RGB8.
fn paint(src: &[u8], w: usize, h: usize) -> Vec<u8> {
    let sectors = sector_offsets();

    // One band per core. The filter is a pure function of the source, so the
    // bands never need to see each other.
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let band = h.div_ceil(threads);
    let mut out = vec![0u8; w * h * 3];

    std::thread::scope(|scope| {
        for (i, chunk) in out.chunks_mut(band * w * 3).enumerate() {
            let sectors = &sectors;
            let y0 = i * band;
            scope.spawn(move || {
                for row in 0..chunk.len() / (w * 3) {
                    let y = y0 + row;
                    for x in 0..w {
                        let c = brush(src, w, h, x, y, sectors);
                        let c = grade(c, x, y, w, h);
                        let o = (row * w + x) * 3;
                        chunk[o] = (c.x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                        chunk[o + 1] = (c.y.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                        chunk[o + 2] = (c.z.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                    }
                }
            });
        }
    });

    out
}

/// The disc, split into eight wedges. Precomputed once: working out which wedge
/// a sample belongs to with `atan2` inside the pixel loop would dominate the
/// cost of the whole filter.
fn sector_offsets() -> Vec<Vec<(i32, i32)>> {
    let mut sectors = vec![Vec::new(); SECTORS];
    let r2 = (BRUSH_RADIUS * BRUSH_RADIUS) as f32;
    for dy in -BRUSH_RADIUS..=BRUSH_RADIUS {
        for dx in -BRUSH_RADIUS..=BRUSH_RADIUS {
            if (dx * dx + dy * dy) as f32 > r2 {
                continue;
            }
            if dx == 0 && dy == 0 {
                // The centre belongs to every wedge, which stops small wedges
                // from ever being empty.
                for s in &mut sectors {
                    s.push((0, 0));
                }
                continue;
            }
            let a = (dy as f32).atan2(dx as f32).rem_euclid(TAU);
            let s = ((a / TAU * SECTORS as f32) as usize).min(SECTORS - 1);
            sectors[s].push((dx, dy));
        }
    }
    sectors
}

fn texel(src: &[u8], w: usize, h: usize, x: i32, y: i32) -> Vec3 {
    let x = x.clamp(0, w as i32 - 1) as usize;
    let y = y.clamp(0, h as i32 - 1) as usize;
    let o = (y * w + x) * 4;
    Vec3::new(src[o] as f32, src[o + 1] as f32, src[o + 2] as f32) / 255.0
}

fn luma(c: Vec3) -> f32 {
    c.x * 0.2126 + c.y * 0.7152 + c.z * 0.0722
}

fn brush(src: &[u8], w: usize, h: usize, x: usize, y: usize, sectors: &[Vec<(i32, i32)>]) -> Vec3 {
    let original = texel(src, w, h, x as i32, y as i32);
    let mut best = original;
    let mut best_var = f32::INFINITY;

    for sector in sectors {
        let (mut sum, mut l1, mut l2) = (Vec3::ZERO, 0.0f32, 0.0f32);
        for &(dx, dy) in sector {
            let c = texel(src, w, h, x as i32 + dx, y as i32 + dy);
            let l = luma(c);
            sum += c;
            l1 += l;
            l2 += l * l;
        }
        let n = sector.len() as f32;
        let mean_l = l1 / n;
        let var = (l2 / n - mean_l * mean_l).max(0.0);
        if var < best_var {
            best_var = var;
            best = sum / n;
        }
    }

    original + (best - original) * BRUSH_MIX
}

/// Warm oil-paint grade: a gentle S-curve for contrast, a warm bias with a
/// slightly lifted, brown-tinted black (canvas is never truly black), a faint
/// weave, and a vignette to seat the image in its frame.
fn grade(c: Vec3, x: usize, y: usize, w: usize, h: usize) -> Vec3 {
    let s = Vec3::new(scurve(c.x), scurve(c.y), scurve(c.z));
    let mut out = s * Vec3::new(1.035, 1.0, 0.955);
    out = out * 0.962 + Vec3::new(0.034, 0.030, 0.024);

    // Canvas weave: two-pixel noise, barely there but it kills the digital
    // smoothness that survives the brush pass in flat sky.
    let weave = value_noise(Vec2::new(x as f32, y as f32), 2.0, SEED) - 0.5;
    out *= 1.0 + weave * 0.035;

    let p = Vec2::new(x as f32 / w as f32, y as f32 / h as f32) * 2.0 - Vec2::ONE;
    out *= 1.0 - 0.20 * p.length_squared().powf(1.5);
    out
}

fn scurve(x: f32) -> f32 {
    let s = x * x * (3.0 - 2.0 * x);
    x + (s - x) * 0.32
}
