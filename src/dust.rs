//! Ephemeral dust clouds kicked up behind moving cows and the horse.
//!
//! A fixed pool of billboarded puffs, all baked into ONE mesh that is rewritten
//! in place each frame and drawn in a single call — no per-particle entities to
//! spawn/despawn, and no compute shaders or storage buffers (WebGL2 has neither).
//!
//! The art is the balls project's smoke sheet, downscaled; only its ALPHA is
//! used, as the puff's shape, and a flat tan tint colours it — a custom material
//! (see `assets/shaders/dust.wgsl`) rather than a textured `StandardMaterial`,
//! whose vertex-colour path rendered the puffs dark and un-faded on WebGL2.
//!
//! The camera only ever translates, never turns, so every billboard shares one
//! constant right/up basis (derived from `CAM_OFFSET`): no per-particle
//! rotation, just centre ± axes.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::mesh::{
    Indices, MeshVertexAttribute, MeshVertexBufferLayoutRef, PrimitiveTopology,
    VertexAttributeValues,
};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError, VertexFormat,
};
use bevy::shader::ShaderRef;

use crate::components::{Cow, Player, Velocity};
use crate::config::*;
use crate::noise::{hash01, smoothstep};

// ---------------------------------------------------------------------------
// Tuning
// ---------------------------------------------------------------------------

/// Puffs alive at once. 4 verts each — even full this is ~1k verts in one draw
/// call. Oldest is recycled first, so a busy stampede degrades to slightly
/// shorter trails rather than a stall. Sized for the dense, short-lived spacing
/// below.
const DUST_POOL: usize = 256;

/// Below this ground speed an animal is ambling and kicks up nothing.
const DUST_MIN_SPEED: f32 = 3.5;
/// World distance between puffs along a trail — spacing, not a timer, so trail
/// density tracks speed and is framerate-independent. Small, so puffs overlap
/// into a cloud instead of dotting a line.
const DUST_SPACING: f32 = 0.85;
/// Cap on puffs spawned per emitter per frame, so a big `dt` hitch can't burst.
const DUST_MAX_PER_STEP: u32 = 4;

/// Drop the puff behind the animal (against its travel), not under its belly.
const DUST_REAR_OFFSET: f32 = 0.9;
/// Scatter radius on the spawn point, so the trail reads as a churned cloud
/// rather than a neat row.
const DUST_JITTER: f32 = 1.1;
/// Puff anchor height — above the ground plane and the pen tint so it never
/// z-fights them.
const DUST_Y: f32 = 0.06;

// Big enough to read as a rolling cloud, not a footprint. A cow is ~1.5 units.
const DUST_SIZE_MIN: f32 = 1.6;
const DUST_SIZE_MAX: f32 = 2.4;
const DUST_LIFE_MIN: f32 = 0.35;
const DUST_LIFE_MAX: f32 = 0.6;
/// Size multiplier across a puff's life: it swells as it disperses.
const DUST_GROW_START: f32 = 0.7;
const DUST_GROW_END: f32 = 1.4;
/// Units/s the puff drifts upward as it ages.
const DUST_RISE: f32 = 0.5;
/// Max units/s a puff drifts outward (random direction) as it ages — spreads the
/// cloud instead of leaving it strung along the path.
const DUST_DRIFT: f32 = 0.9;

/// Overall opacity, on top of the sprite's own alpha and the life fade.
const DUST_OPACITY: f32 = 0.7;
/// Dust is the page/bezel brown (`FRAME_WOOD_COLOR`, #5e4123 — what the web page
/// background and the letterbox bands are painted), brightened so a puff still
/// reads as light dirt kicked up against the green rather than mud.
const DUST_LIGHTEN: f32 = 1.9;
const DUST_TINT: (f32, f32, f32) = (
    FRAME_WOOD_COLOR.0 * DUST_LIGHTEN,
    FRAME_WOOD_COLOR.1 * DUST_LIGHTEN,
    FRAME_WOOD_COLOR.2 * DUST_LIGHTEN,
);

/// Per-vertex payload: atlas UV + life fade, packed as one Float32x3.
const ATTRIBUTE_DUST: MeshVertexAttribute =
    MeshVertexAttribute::new("DustData", 0x4455_5354, VertexFormat::Float32x3);

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct DustPlugin;

impl Plugin for DustPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<DustMaterial>::default())
            .add_systems(Startup, setup_dust)
            .add_systems(Update, (tag_emitters, emit_dust, update_dust).chain());
    }
}

// ---------------------------------------------------------------------------
// Material
// ---------------------------------------------------------------------------

#[derive(Clone, ShaderType)]
struct DustParams {
    tint: Vec4, // rgb linear, a = opacity
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
struct DustMaterial {
    #[uniform(0)]
    params: DustParams,
    #[texture(1)]
    #[sampler(2)]
    atlas: Handle<Image>,
}

impl Material for DustMaterial {
    fn vertex_shader() -> ShaderRef {
        "shaders/dust.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "shaders/dust.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    fn enable_shadows() -> bool {
        false
    }

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
            ATTRIBUTE_DUST.at_shader_location(8),
        ])?];
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Pool
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Puff {
    pos: Vec3,   // ground spawn point (bottom-centre of the billboard)
    drift: Vec2, // world-XZ outward drift (units/s), so the cloud spreads
    rot: f32,    // billboard-plane spin, so overlapping puffs don't align
    age: f32,
    life: f32,
    size: f32,
    alive: bool,
}

/// Per-emitter distance accumulator: a puff drops every `DUST_SPACING` travelled.
#[derive(Component, Default)]
struct DustEmitter {
    accum: f32,
}

#[derive(Resource)]
struct DustField {
    puffs: Vec<Puff>,
    next: usize,      // ring cursor for recycling the oldest slot
    spawn_count: u64, // seeds the deterministic per-puff jitter
    mesh: Handle<Mesh>,
    right: Vec3, // constant billboard basis (camera never turns)
    up: Vec3,
    // Scratch buffers, kept the exact mesh length so each frame is an in-place
    // copy — no per-frame allocation.
    positions: Vec<[f32; 3]>,
    data: Vec<[f32; 3]>, // (u, v, fade) per vertex
}

/// Screen-facing billboard axes for the fixed camera. `right` is world +X;
/// `up` tilts with the camera's ~45° pitch.
fn billboard_basis() -> (Vec3, Vec3) {
    let view_dir = (-CAM_OFFSET).normalize();
    let right = view_dir.cross(Vec3::Y).normalize();
    let up = right.cross(view_dir).normalize();
    (right, up)
}

/// The atlas cell rect (u0, v0, u1, v1) for a frame. Cells carry a transparent
/// gutter, so the full cell rect samples cleanly under linear filtering.
fn cell_uv(frame: usize) -> (f32, f32, f32, f32) {
    let cols = DUST_SHEET_COLS;
    let f = frame.min(DUST_SHEET_FRAMES - 1);
    let (c, r) = ((f % cols) as f32, (f / cols) as f32);
    let s = 1.0 / cols as f32;
    (c * s, r * s, (c + 1.0) * s, (r + 1.0) * s)
}

/// Quick fade in, hold, smooth fade out — so a puff dissolves instead of popping.
fn fade(t: f32) -> f32 {
    smoothstep(0.0, 0.12, t) * (1.0 - smoothstep(0.55, 1.0, t))
}

fn setup_dust(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<DustMaterial>>,
    asset_server: Res<AssetServer>,
) {
    let (right, up) = billboard_basis();
    let verts = DUST_POOL * 4;

    // Static index buffer: two triangles per quad, [tl, tr, br, bl].
    let mut indices: Vec<u16> = Vec::with_capacity(DUST_POOL * 6);
    for q in 0..DUST_POOL {
        let b = (q * 4) as u16;
        indices.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        // Editable each frame from the main world, and rendered — so keep both.
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; verts]);
    mesh.insert_attribute(ATTRIBUTE_DUST, vec![[0.0f32; 3]; verts]);
    mesh.insert_indices(Indices::U16(indices));
    let mesh = meshes.add(mesh);

    let lin = Color::srgb(DUST_TINT.0, DUST_TINT.1, DUST_TINT.2).to_linear();
    let material = materials.add(DustMaterial {
        params: DustParams {
            tint: Vec4::new(lin.red, lin.green, lin.blue, DUST_OPACITY),
        },
        atlas: asset_server.load(DUST_TEXTURE),
    });

    commands.spawn((
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material),
        Transform::default(),
        Visibility::default(),
        // The mesh AABB is stale the instant particles move; skip culling rather
        // than recompute it — it is a single object.
        NoFrustumCulling,
    ));

    commands.insert_resource(DustField {
        puffs: vec![
            Puff {
                pos: Vec3::ZERO,
                drift: Vec2::ZERO,
                rot: 0.0,
                age: 0.0,
                life: 1.0,
                size: 0.0,
                alive: false,
            };
            DUST_POOL
        ],
        next: 0,
        spawn_count: 0,
        mesh,
        right,
        up,
        positions: vec![[0.0; 3]; verts],
        data: vec![[0.0; 3]; verts],
    });
}

/// Give every cow and the horse a distance accumulator once they exist.
#[allow(clippy::type_complexity)]
fn tag_emitters(
    mut commands: Commands,
    q: Query<Entity, (Or<(With<Cow>, With<Player>)>, Without<DustEmitter>)>,
) {
    for e in &q {
        commands.entity(e).insert(DustEmitter::default());
    }
}

fn emit_dust(
    time: Res<Time>,
    mut field: ResMut<DustField>,
    mut movers: Query<(&Transform, &Velocity, &mut DustEmitter)>,
) {
    let dt = time.delta_secs();
    let f = &mut *field;

    for (tf, vel, mut em) in &mut movers {
        let speed = vel.0.length();
        if speed < DUST_MIN_SPEED {
            em.accum = 0.0; // reset so a fresh start doesn't puff on step one
            continue;
        }
        let dir = vel.0 / speed;
        em.accum += speed * dt;

        let mut steps = 0;
        while em.accum >= DUST_SPACING && steps < DUST_MAX_PER_STEP {
            em.accum -= DUST_SPACING;
            steps += 1;

            let s = f.spawn_count;
            f.spawn_count += 1;
            let ground = Vec2::new(tf.translation.x, tf.translation.z) - dir * DUST_REAR_OFFSET;
            let jx = (hash01(s * 8 + 1) - 0.5) * DUST_JITTER;
            let jz = (hash01(s * 8 + 2) - 0.5) * DUST_JITTER;
            let size = DUST_SIZE_MIN + hash01(s * 8 + 3) * (DUST_SIZE_MAX - DUST_SIZE_MIN);
            let life = DUST_LIFE_MIN + hash01(s * 8 + 4) * (DUST_LIFE_MAX - DUST_LIFE_MIN);
            let (dsin, dcos) = (hash01(s * 8 + 5) * std::f32::consts::TAU).sin_cos();
            let dmag = hash01(s * 8 + 6) * DUST_DRIFT;
            let rot = hash01(s * 8 + 7) * std::f32::consts::TAU;

            let idx = f.next;
            f.next = (idx + 1) % DUST_POOL;
            f.puffs[idx] = Puff {
                pos: Vec3::new(ground.x + jx, DUST_Y, ground.y + jz),
                drift: Vec2::new(dcos, dsin) * dmag,
                rot,
                age: 0.0,
                life,
                size,
                alive: true,
            };
        }
    }
}

fn update_dust(time: Res<Time>, mut field: ResMut<DustField>, mut meshes: ResMut<Assets<Mesh>>) {
    let dt = time.delta_secs();
    let f = &mut *field;
    let (right, up) = (f.right, f.up);

    for i in 0..f.puffs.len() {
        let base = i * 4;
        let mut clear = !f.puffs[i].alive;
        if !clear {
            f.puffs[i].age += dt;
            let p = f.puffs[i];
            let t = p.age / p.life;
            if t >= 1.0 {
                f.puffs[i].alive = false;
                clear = true;
            } else {
                let size = p.size * (DUST_GROW_START + t * (DUST_GROW_END - DUST_GROW_START));
                let h = size * 0.5;
                let center = p.pos
                    + up * (h + DUST_RISE * p.age)
                    + Vec3::new(p.drift.x, 0.0, p.drift.y) * p.age;
                let (u0, v0, u1, v1) = cell_uv((t * DUST_SHEET_FRAMES as f32) as usize);
                let a = fade(t);

                // Spin the quad in the billboard plane so overlapping puffs read
                // as a churn, not stacked squares.
                let (sr, cr) = p.rot.sin_cos();
                let rx = (right * cr + up * sr) * h;
                let uy = (up * cr - right * sr) * h;
                // [tl, tr, br, bl] — matches the static index buffer.
                f.positions[base] = (center - rx + uy).to_array();
                f.positions[base + 1] = (center + rx + uy).to_array();
                f.positions[base + 2] = (center + rx - uy).to_array();
                f.positions[base + 3] = (center - rx - uy).to_array();
                f.data[base] = [u0, v0, a];
                f.data[base + 1] = [u1, v0, a];
                f.data[base + 2] = [u1, v1, a];
                f.data[base + 3] = [u0, v1, a];
            }
        }
        if clear {
            for k in 0..4 {
                f.positions[base + k] = [0.0; 3];
                f.data[base + k] = [0.0; 3];
            }
        }
    }

    if let Some(mut mesh) = meshes.get_mut(&f.mesh) {
        if let Some(VertexAttributeValues::Float32x3(v)) =
            mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
        {
            v.copy_from_slice(&f.positions);
        }
        if let Some(VertexAttributeValues::Float32x3(v)) = mesh.attribute_mut(ATTRIBUTE_DUST) {
            v.copy_from_slice(&f.data);
        }
    }
}
