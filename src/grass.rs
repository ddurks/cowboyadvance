//! Grass: sways in the wind, flattens under hooves, springs back, and keeps a
//! slow-fading record of the trails the herd wears into the field.
//!
//! # Why it is built this way
//!
//! The target is WebGL2 on low-end phones, which rules out the usual tricks —
//! no compute shaders, no storage buffers — and punishes the usual grass
//! technique. Three decisions follow:
//!
//! **Opaque triangles, not alpha cards.** Alpha-tested foliage defeats early-Z
//! and the hidden-surface removal tile-based mobile GPUs depend on. Solid
//! triangles cost a fraction as much, and low-poly tufts suit the art style.
//! Each tuft is a handful of narrow solid blades: still cheap and opaque, but
//! clustered closely enough to read as grass rather than scattered confetti.
//!
//! **The fixed orthographic camera is exploited twice.** Under an orthographic
//! projection every tuft sees the camera from the *identical* angle, so the
//! blade orientation is baked at build time and there is no billboarding maths
//! in the shader — not an approximation, exactly right. And since nothing
//! shrinks with distance, there is no LOD, no density falloff and no pop.
//!
//! **Trample state lives in one small texture.** The CPU stamps into it and
//! the vertex shader reads it — the only texture fetch in the whole shader.
//! Its channels are:
//!
//! | channel | meaning | recovery |
//! |---|---|---|
//! | R | flatten | seconds |
//! | G | wear | ~90 s |
//! | B, A | push direction, encoded | persists |
//!
//! Two channels rather than one is what sells it: flatten springs back almost
//! at once, while wear lingers, so driving cattle over the same ground carves
//! semi-permanent trails across the field.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, MeshVertexAttribute, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::mesh::VertexAttributeValues;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
    TextureDimension, TextureFormat, VertexFormat,
};
use bevy::shader::ShaderRef;

use crate::components::{Cow, Player};
use crate::noise::{hash01, lerp, value_noise};
use crate::config::*;
use crate::level::Level;

// ---------------------------------------------------------------------------
// Tuning
// ---------------------------------------------------------------------------

/// The play field, which is the area the trample map covers — tramplers are
/// clamped to it, so nothing outside can ever be walked on.
const FIELD_MIN: Vec2 = Vec2::new(-FIELD_HALF_X, -FIELD_HALF_Z);
const FIELD_SIZE: Vec2 = Vec2::new(FIELD_HALF_X * 2.0, FIELD_Z_NEAR + FIELD_HALF_Z);

/// Grass grows past the play field. `AutoMin` reveals more ground than
/// `CAM_MIN_HEIGHT` on tall viewports, so grass that stopped at the field edge
/// left a visible bald strip along the top of the screen.
const GRASS_MARGIN: f32 = 16.0;
const GRASS_MIN: Vec2 = Vec2::new(FIELD_MIN.x - GRASS_MARGIN, FIELD_MIN.y - GRASS_MARGIN);
const GRASS_SIZE: Vec2 = Vec2::new(
    FIELD_SIZE.x + GRASS_MARGIN * 2.0,
    FIELD_SIZE.y + GRASS_MARGIN * 2.0,
);

/// Tuft centres per square unit. Each centre grows `BLADES_PER_TUFT` blades,
/// so this is deliberately lower than the old one-triangle tuft density.
const TUFTS_PER_UNIT2: f32 = 1.0;

/// A few nearby blades make one clearly readable tuft without increasing the
/// old field's triangle budget much.
const BLADES_PER_TUFT: usize = 3;

/// Fraction of tufts that survive in the thinnest patches. Clumping rejects
/// the rest, so the effective density is roughly 0.75x the number above.
const CLUMP_FLOOR: f32 = 0.30;

/// Chunking is a trade: more chunks cull better, fewer mean fewer draw calls.
/// With grass extending past the field there is real off-screen area to cull,
/// so 15 chunks pays for itself and is still nothing for a mobile GPU.
const CHUNKS_X: usize = 3;
const CHUNKS_Z: usize = 5;

/// Blade dimensions. Width is narrow relative to height; several blades grow
/// close together so their combined silhouette remains substantial on mobile.
const BLADE_HEIGHT: (f32, f32) = (0.62, 1.18);
const BLADE_WIDTH: (f32, f32) = (0.12, 0.24);
const TUFT_RADIUS: f32 = 0.23;
/// Blades lean toward the camera so more of each one faces it under the 45°
/// view. A fraction of the tuft's height.
const TUFT_LEAN: f32 = 0.22;
/// Random yaw off the camera-facing plane, radians. Keeps the field from
/// looking like it was combed.
const TUFT_YAW_JITTER: f32 = 0.42;

/// Trample map resolution. 128x256 over a 44x100 field is ~0.35 units/texel,
/// so a cow (radius 1.1) covers ~6 texels — enough for smooth trails, small
/// enough that re-uploading it is free.
const TRAMPLE_W: usize = 128;
const TRAMPLE_H: usize = 256;

/// Seconds for flattened grass to stand back up, and for a worn trail to fade.
const FLATTEN_RECOVER_SECS: f32 = 3.2;
const WEAR_RECOVER_SECS: f32 = 90.0;
/// How fast repeated traffic darkens a trail.
const WEAR_GAIN_PER_SEC: f32 = 0.55;

/// The trample map only needs updating as fast as grass visibly moves.
const TRAMPLE_HZ: f32 = 20.0;

/// Radius scale applied to a trampler's collision radius when stamping.
const STAMP_RADIUS_SCALE: f32 = 0.85;

/// Pitched against the ground *as rendered*, not its albedo — the blades are
/// unlit, so they only read by contrast with the lit, textured ground plane.
/// Sits a step BELOW it (tips nearest, base deepest): with the mottled ground
/// texture underneath, darker blades stay visible where lighter ones vanished
/// into the bright field.
const GRASS_BASE_COLOR: (f32, f32, f32) = (0.20, 0.35, 0.16);
const GRASS_TIP_COLOR: (f32, f32, f32) = (0.30, 0.49, 0.23);
const GRASS_DRY_COLOR: (f32, f32, f32) = (0.40, 0.36, 0.19);

const WIND_DIR: Vec2 = Vec2::new(0.82, 0.57);
const WIND_SPEED: f32 = 1.6;
/// Sway amplitude in radians — this is a bend angle, not a distance.
const WIND_AMP: f32 = 0.16;
const GUST_AMP: f32 = 0.28;
const GUST_SPEED: f32 = 0.45;
/// How far a fully trampled blade lies over. ~80° reads as flat without
/// clipping through the ground.
const FLATTEN_ANGLE: f32 = 1.40;

/// Per-vertex packed data: height factor, sway phase, brightness, dryness.
/// `Unorm8x4` keeps a vertex at 16 bytes (12 position + 4), which matters more
/// than vertex count on mobile.
const ATTRIBUTE_GRASS: MeshVertexAttribute =
    MeshVertexAttribute::new("GrassData", 0x6772_6173, VertexFormat::Unorm8x4);

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct GrassPlugin;

impl Plugin for GrassPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<GrassMaterial>::default())
            .add_systems(Startup, setup_grass.after(crate::setup::setup))
            .add_systems(Update, update_trample);
    }
}

// ---------------------------------------------------------------------------
// Material
// ---------------------------------------------------------------------------

#[derive(Clone, ShaderType)]
struct GrassParams {
    /// xy = field origin, zw = field size.
    field: Vec4,
    /// xy = wind direction, z = speed, w = amplitude.
    wind: Vec4,
    /// x = gust amplitude, y = gust speed, z = flatten angle, w = unused.
    gust: Vec4,
    base_color: Vec4,
    tip_color: Vec4,
    dry_color: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct GrassMaterial {
    #[uniform(0)]
    params: GrassParams,
    #[texture(1)]
    #[sampler(2)]
    trample: Handle<Image>,
}

impl Material for GrassMaterial {
    fn vertex_shader() -> ShaderRef {
        "shaders/grass.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "shaders/grass.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }

    /// Grass neither casts nor receives shadows. Casting would run every tuft
    /// through the shadow pass as well, roughly doubling its vertex cost, for
    /// a shadow nobody can see at this scale.
    fn enable_shadows() -> bool {
        false
    }

    /// The prepass would rasterise depth with the *default* vertex shader,
    /// which knows nothing about the sway — every blade would be depth-tested
    /// against its unbent position. Opt out.
    fn enable_prepass() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.vertex.buffers = vec![layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            ATTRIBUTE_GRASS.at_shader_location(8),
        ])?];
        // Two-sided: a tuft is a single triangle, so drawing both faces costs
        // no extra fill and removes any winding-order hazard in the builder.
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Trample map
// ---------------------------------------------------------------------------

#[derive(Resource)]
struct TrampleMap {
    image: Handle<Image>,
    /// Texel bounds of everything currently non-zero, so decay touches a small
    /// rect instead of all 32k texels. Cattle cluster; this is usually tiny.
    dirty: Option<[usize; 4]>,
    /// Fractional decay carried between ticks. Without this, an 8-bit channel
    /// decaying by at least 1 per tick could never fade as slowly as `wear`
    /// wants to.
    flatten_debt: f32,
    wear_debt: f32,
    accum: f32,
    prev: HashMap<Entity, Vec2>,
}

impl TrampleMap {
    fn mark(&mut self, x: usize, y: usize) {
        match &mut self.dirty {
            Some(r) => {
                r[0] = r[0].min(x);
                r[1] = r[1].min(y);
                r[2] = r[2].max(x);
                r[3] = r[3].max(y);
            }
            None => self.dirty = Some([x, y, x, y]),
        }
    }
}

/// World XZ -> trample texel coordinates (fractional).
fn world_to_texel(p: Vec2) -> Vec2 {
    let uv = (p - FIELD_MIN) / FIELD_SIZE;
    Vec2::new(uv.x * TRAMPLE_W as f32, uv.y * TRAMPLE_H as f32)
}

fn make_trample_image() -> Image {
    let mut image = Image::new_fill(
        Extent3d {
            width: TRAMPLE_W as u32,
            height: TRAMPLE_H as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        // flatten 0, wear 0, direction encoded as (0.5, 0.5) == zero vector
        &[0, 0, 128, 128],
        TextureFormat::Rgba8Unorm,
        // Kept in the main world so the CPU can keep stamping into it.
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        // Bilinear so trails have soft edges despite the coarse grid.
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        ..default()
    });
    image
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

fn setup_grass(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<GrassMaterial>>,
    mut images: ResMut<Assets<Image>>,
    level: Res<crate::components::GameLevel>,
) {
    let trample = images.add(make_trample_image());

    let material = materials.add(GrassMaterial {
        params: GrassParams {
            field: Vec4::new(FIELD_MIN.x, FIELD_MIN.y, FIELD_SIZE.x, FIELD_SIZE.y),
            wind: Vec4::new(
                WIND_DIR.x,
                WIND_DIR.y,
                WIND_SPEED,
                WIND_AMP,
            ),
            gust: Vec4::new(GUST_AMP, GUST_SPEED, FLATTEN_ANGLE, 0.0),
            base_color: color_vec4(GRASS_BASE_COLOR),
            tip_color: color_vec4(GRASS_TIP_COLOR),
            dry_color: color_vec4(GRASS_DRY_COLOR),
        },
        trample: trample.clone(),
    });

    let chunk = Vec2::new(
        GRASS_SIZE.x / CHUNKS_X as f32,
        GRASS_SIZE.y / CHUNKS_Z as f32,
    );
    let mut total = 0usize;
    for cz in 0..CHUNKS_Z {
        for cx in 0..CHUNKS_X {
            let min = GRASS_MIN + Vec2::new(cx as f32 * chunk.x, cz as f32 * chunk.y);
            let centre = min + chunk * 0.5;
            let seed = (cz * CHUNKS_X + cx) as u64 * 0x9E37_79B9;
            let Some((mesh, count)) = build_chunk(centre, chunk, seed, &level.0) else {
                continue;
            };
            total += count;
            commands.spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material.clone()),
                Transform::from_xyz(centre.x, 0.0, centre.y),
            ));
        }
    }

    commands.insert_resource(TrampleMap {
        image: trample,
        dirty: None,
        flatten_debt: 0.0,
        wear_debt: 0.0,
        accum: 0.0,
        prev: HashMap::default(),
    });

    info!("grass: {total} tufts across {} chunks", CHUNKS_X * CHUNKS_Z);
}

fn color_vec4(c: (f32, f32, f32)) -> Vec4 {
    let lin = Color::srgb(c.0, c.1, c.2).to_linear();
    Vec4::new(lin.red, lin.green, lin.blue, 1.0)
}

/// Build one chunk's mesh. Vertices are chunk-local so the entity transform
/// carries the world position and Bevy's computed AABB stays tight for culling.
fn build_chunk(centre: Vec2, size: Vec2, seed: u64, level: &Level) -> Option<(Mesh, usize)> {
    let count = (size.x * size.y * TUFTS_PER_UNIT2) as usize;
    let half = size * 0.5;

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(count * BLADES_PER_TUFT * 3);
    let mut data: Vec<[u8; 4]> = Vec::with_capacity(count * BLADES_PER_TUFT * 3);
    let mut indices: Vec<u16> = Vec::with_capacity(count * BLADES_PER_TUFT * 3);

    let mut tufts = 0usize;
    for i in 0..count {
        let s = seed.wrapping_add(i as u64 * 0x2545_F491);
        let px = lerp(-half.x, half.x, hash01(s));
        let pz = lerp(-half.y, half.y, hash01(s ^ 0xA1));
        let world = centre + Vec2::new(px, pz);
        if blocked(world, level) {
            continue;
        }

        // Two octaves: broad thick/thin patches, plus finer break-up.
        let clump = value_noise(world, 9.0, 0x51) * 0.7 + value_noise(world, 3.5, 0x77) * 0.3;
        if hash01(s ^ 0x99) > CLUMP_FLOOR + (1.0 - CLUMP_FLOOR) * clump * 1.35 {
            continue;
        }

        // All blades share a little colour and motion character, while their
        // sizes, positions, and lean vary enough to avoid a fork-like shape.
        let tuft_phase = hash01(s ^ 0xE5);
        let tuft_tint = hash01(s ^ 0xF6);
        let tuft_dry = hash01(s ^ 0x17);
        for blade in 0..BLADES_PER_TUFT {
            let bs = s ^ (blade as u64 + 1).wrapping_mul(0xD1B5_4A32_D192_ED03);
            let angle = hash01(bs ^ 0x31) * std::f32::consts::TAU;
            let radius = TUFT_RADIUS * hash01(bs ^ 0x42).sqrt();
            let offset = Vec2::new(angle.cos(), angle.sin()) * radius;
            let bx = px + offset.x;
            let bz = pz + offset.y;

            let height = lerp(
                BLADE_HEIGHT.0,
                BLADE_HEIGHT.1,
                hash01(bs ^ 0xB2) * 0.55 + clump * 0.45,
            );
            let width = lerp(BLADE_WIDTH.0, BLADE_WIDTH.1, hash01(bs ^ 0xC3));
            let yaw = (hash01(bs ^ 0xD4) - 0.5) * 2.0 * TUFT_YAW_JITTER;

            // The base edge runs mostly along X, putting the blade's plane
            // toward the fixed camera while retaining some natural disorder.
            let (sin_y, cos_y) = yaw.sin_cos();
            let ex = Vec2::new(cos_y, sin_y) * (width * 0.5);
            let lean = height * TUFT_LEAN;
            let lean_x = (hash01(bs ^ 0x28) - 0.5) * height * 0.24;

            let base = positions.len() as u16;
            positions.push([bx - ex.x, 0.0, bz - ex.y]);
            positions.push([bx + ex.x, 0.0, bz + ex.y]);
            positions.push([bx + lean_x, height, bz + lean]);

            let phase = (tuft_phase + (hash01(bs ^ 0x53) - 0.5) * 0.08).rem_euclid(1.0);
            let tint = (tuft_tint + (hash01(bs ^ 0x64) - 0.5) * 0.18).clamp(0.0, 1.0);
            let phase_b = (phase * 255.0) as u8;
            let tint_b = (tint * 255.0) as u8;
            let dry_b = (tuft_dry * 255.0) as u8;
            data.push([0, phase_b, tint_b, dry_b]);
            data.push([0, phase_b, tint_b, dry_b]);
            data.push([255, phase_b, tint_b, dry_b]);

            indices.extend_from_slice(&[base, base + 1, base + 2]);
        }
        tufts += 1;
    }

    if tufts == 0 {
        return None;
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(ATTRIBUTE_GRASS, VertexAttributeValues::Unorm8x4(data));
    mesh.insert_indices(Indices::U16(indices));
    Some((mesh, tufts))
}

/// True where a tuft would grow through the fence rails or a gate post.
/// `p` is in world space.
fn blocked(p: Vec2, level: &Level) -> bool {
    let clearance = FENCE_THICK * 0.5 + 0.28;
    level
        .fences
        .iter()
        .chain(level.boundary.iter())
        .any(|f| f.closest_point(p).distance_squared(p) < clearance * clearance)
        || level.posts.iter().any(|q| q.distance_squared(p) < 0.49)
}

// ---------------------------------------------------------------------------
// Per-frame: stamp tramplers, then recover
// ---------------------------------------------------------------------------

// The query type is a normal Bevy filter combination, not real complexity.
#[allow(clippy::type_complexity)]
fn update_trample(
    time: Res<Time>,
    mut map: ResMut<TrampleMap>,
    mut images: ResMut<Assets<Image>>,
    movers: Query<(Entity, &Transform), Or<(With<Player>, With<Cow>)>>,
) {
    map.accum += time.delta_secs();
    let step = map.accum;
    if step < 1.0 / TRAMPLE_HZ {
        return;
    }
    map.accum = 0.0;

    // Gather first so the borrow of `map.prev` ends before we mutate the image.
    let mut strokes: Vec<Stroke> = Vec::new();
    let mut seen: Vec<Entity> = Vec::new();
    for (entity, tf) in &movers {
        let cur = Vec2::new(tf.translation.x, tf.translation.z);
        let prev = map.prev.get(&entity).copied().unwrap_or(cur);
        strokes.push(Stroke { from: prev, to: cur, radius: COW_RADIUS * STAMP_RADIUS_SCALE });
        seen.push(entity);
        map.prev.insert(entity, cur);
    }
    map.prev.retain(|e, _| seen.contains(e));

    let handle = map.image.clone();
    // `AssetMut` is a change-detection wrapper: touching it is what schedules
    // the texture's re-upload to the GPU.
    let Some(mut image) = images.get_mut(&handle) else {
        return;
    };
    let Some(pixels) = image.data.as_mut() else {
        return;
    };

    for s in strokes {
        stamp_stroke(&mut map, pixels, s, step);
    }
    recover(&mut map, pixels, step);
}

/// One entity's movement over a tick: the segment of ground it swept.
struct Stroke {
    from: Vec2,
    to: Vec2,
    radius: f32,
}

/// Flatten grass along the segment the entity actually travelled this tick.
///
/// Stamping a disc at the current position is not enough: the player moves at
/// `PLAYER_SPEED` (19 u/s), so at 20 Hz they cover ~0.95 units — nearly three
/// texels — and would leave a dotted trail. Sweeping the segment fixes it and
/// costs the same handful of texel tests.
fn stamp_stroke(map: &mut TrampleMap, pixels: &mut [u8], stroke: Stroke, step: f32) {
    let Stroke { from, to, radius } = stroke;
    let a = world_to_texel(from);
    let b = world_to_texel(to);
    let r = Vec2::new(
        radius / FIELD_SIZE.x * TRAMPLE_W as f32,
        radius / FIELD_SIZE.y * TRAMPLE_H as f32,
    );
    let rad = r.x.max(r.y);

    let min_x = (a.x.min(b.x) - rad).floor().max(0.0) as usize;
    let max_x = (a.x.max(b.x) + rad).ceil().min(TRAMPLE_W as f32 - 1.0) as usize;
    let min_y = (a.y.min(b.y) - rad).floor().max(0.0) as usize;
    let max_y = (a.y.max(b.y) + rad).ceil().min(TRAMPLE_H as f32 - 1.0) as usize;
    if min_x > max_x || min_y > max_y {
        return;
    }

    // Grass is pushed the way the traveller is going.
    let travel = to - from;
    let dir = travel.normalize_or_zero();
    let (dx, dy) = if dir == Vec2::ZERO {
        (128u8, 128u8)
    } else {
        (
            ((dir.x * 0.5 + 0.5) * 255.0) as u8,
            ((dir.y * 0.5 + 0.5) * 255.0) as u8,
        )
    };
    let wear_gain = (WEAR_GAIN_PER_SEC * step * 255.0).max(1.0) as u8;

    let ab = b - a;
    let len_sq = ab.length_squared();
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let t = if len_sq > 1e-6 {
                ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let d = (p - (a + ab * t)).length();
            if d > rad {
                continue;
            }
            // Soft edge so trails do not have a hard rim.
            let f = (1.0 - (d / rad).powi(2)).clamp(0.0, 1.0);
            let i = (y * TRAMPLE_W + x) * 4;
            let flatten = (f * 255.0) as u8;
            if flatten > pixels[i] {
                pixels[i] = flatten;
                pixels[i + 2] = dx;
                pixels[i + 3] = dy;
            }
            pixels[i + 1] = pixels[i + 1].saturating_add((f * wear_gain as f32) as u8);
            map.mark(x, y);
        }
    }
}

/// Let flattened grass stand back up and worn trails fade, over the dirty rect
/// only, and shrink the rect to whatever is still non-zero.
fn recover(map: &mut TrampleMap, pixels: &mut [u8], step: f32) {
    let Some([x0, y0, x1, y1]) = map.dirty else {
        return;
    };

    map.flatten_debt += step / FLATTEN_RECOVER_SECS * 255.0;
    map.wear_debt += step / WEAR_RECOVER_SECS * 255.0;
    let d_flat = map.flatten_debt.floor();
    let d_wear = map.wear_debt.floor();
    map.flatten_debt -= d_flat;
    map.wear_debt -= d_wear;
    let (d_flat, d_wear) = (d_flat as u8, d_wear as u8);
    if d_flat == 0 && d_wear == 0 {
        return;
    }

    let mut live: Option<[usize; 4]> = None;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let i = (y * TRAMPLE_W + x) * 4;
            pixels[i] = pixels[i].saturating_sub(d_flat);
            pixels[i + 1] = pixels[i + 1].saturating_sub(d_wear);
            if pixels[i] > 0 || pixels[i + 1] > 0 {
                live = Some(match live {
                    Some(r) => [r[0].min(x), r[1].min(y), r[2].max(x), r[3].max(y)],
                    None => [x, y, x, y],
                });
            }
        }
    }
    map.dirty = live;
}
