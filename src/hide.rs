//! Randomised splotches on each cow's hide, so a herd reads as individuals.
//!
//! Every cow gets its own dark-brown patches, generated once when its model
//! finishes spawning and then left alone — there is no per-frame cost at all.
//!
//! # Why vertex colours
//!
//! The obvious approach is a texture, but the hide meshes carry **no UVs**: the
//! `Cow` material is untextured, so the glTF export strips their UV layers to
//! save space. Re-adding a UV unwrap plus 24 generated textures would cost real
//! memory and real art time.
//!
//! A shader could do it procedurally, but splotches have to stay welded to the
//! hide while the cow animates, which means sampling the *rest* position — and
//! that is not available in a fragment shader on a skinned mesh without extra
//! plumbing.
//!
//! Vertex colours sidestep both. The splotch is baked per vertex from the rest
//! position, so it skins along with the mesh for free, `StandardMaterial`
//! already multiplies vertex colour into base colour, and it composes with the
//! penned-green tint: a penned cow keeps its patches, in green.
//!
//! The usual objection to vertex colours is blockiness, and it does not apply
//! here. `cow_body` is 1664 triangles on something ~50 px tall on screen, so
//! triangles are around a pixel; a splotch edge that fades across one triangle
//! is a hard edge as far as the player is concerned.
//!
//! Cost: one cloned mesh per cow per splotched part. Body and head only —
//! ~1060 vertices each, so ~1.6 MB across 24 cows, one-time, at startup.

use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;

use crate::components::Cow;
use crate::noise::{hash01, lerp, smoothstep, value_noise3};

/// Which glTF nodes get patches. The ears and tail are left plain: they are
/// small on screen and would cost another ~1100 vertices per cow to clone.
const SPLOTCH_NODES: &[&str] = &["cow_body", "cow_head"];

/// Noise cell size in model units. The cow's barrel is ~1.5 units, so this
/// gives a couple of cells across it — a few big patches rather than spots.
/// Cows are only ~25 px tall on screen, so fine detail is wasted.
const SPLOTCH_CELL: f32 = 0.60;
/// Finer second octave, to break up the edges so they are not obviously blobs.
const SPLOTCH_CELL_FINE: f32 = 0.26;
const FINE_WEIGHT: f32 = 0.22;

/// Half-width of the threshold band. Small = hard-edged patches, which is what
/// a real hide looks like; a wide band gives a washed-out dirty look.
const EDGE_SOFTNESS: f32 = 0.028;

/// Coverage range. Lower threshold = more of the cow covered. Trilinear value
/// noise clusters around 0.5, so these sit just above it: roughly 35% coverage
/// at the low end down to a few percent at the high end. Going below ~0.48
/// turns cows almost entirely dark rather than patched.
const THRESHOLD: (f32, f32) = (0.50, 0.63);
/// How dark the patches go, as a multiplier on the hide's brown.
const DARKNESS: (f32, f32) = (0.30, 0.50);
/// Fraction of the herd that comes out solid-coloured, as real herds do.
const SOLID_CHANCE: f32 = 0.18;

pub struct CowHidePlugin;

impl Plugin for CowHidePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, splotch_cow_hides);
    }
}

/// Set once a cow's hide has been given its own meshes, so the work happens
/// exactly once per cow.
#[derive(Component)]
struct HideSplotched;

/// The per-cow look, derived entirely from one seed.
struct HidePattern {
    offset: Vec3,
    threshold: f32,
    dark: Vec3,
}

impl HidePattern {
    fn from_seed(seed: u64) -> Self {
        // Each cow samples a different region of the same noise field.
        let offset = Vec3::new(
            hash01(seed ^ 0x11) * 64.0,
            hash01(seed ^ 0x22) * 64.0,
            hash01(seed ^ 0x33) * 64.0,
        );
        let solid = hash01(seed ^ 0x44) < SOLID_CHANCE;
        let threshold = if solid {
            // Above anything the noise can reach: no patches at all.
            2.0
        } else {
            lerp(THRESHOLD.0, THRESHOLD.1, hash01(seed ^ 0x55))
        };
        let d = lerp(DARKNESS.0, DARKNESS.1, hash01(seed ^ 0x66));
        // Warmer in the dark: pull blue down harder than red so the patch reads
        // as a deeper brown rather than a grey shadow.
        let dark = Vec3::new(d * 1.10, d, d * 0.82);
        Self { offset, threshold, dark }
    }

    /// Vertex tint at a rest-pose model-space position. 1.0 = the hide's own
    /// colour, `dark` = fully inside a patch.
    fn tint_at(&self, p: Vec3, seed: u64) -> [f32; 4] {
        let q = p + self.offset;
        let n = value_noise3(q, SPLOTCH_CELL, seed) * (1.0 - FINE_WEIGHT)
            + value_noise3(q, SPLOTCH_CELL_FINE, seed ^ 0x77) * FINE_WEIGHT;
        let m = smoothstep(
            self.threshold - EDGE_SOFTNESS,
            self.threshold + EDGE_SOFTNESS,
            n,
        );
        [
            lerp(1.0, self.dark.x, m),
            lerp(1.0, self.dark.y, m),
            lerp(1.0, self.dark.z, m),
            1.0,
        ]
    }
}

/// Give every newly spawned cow its own splotched copy of the hide meshes.
///
/// Models load asynchronously, so this keeps looking each frame until the
/// cow's parts actually exist — the same pattern the rest of the model wiring
/// uses.
fn splotch_cow_hides(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    cows: Query<Entity, (With<Cow>, Without<HideSplotched>)>,
    children: Query<&Children>,
    names: Query<&Name>,
    mut mesh_handles: Query<&mut Mesh3d>,
) {
    for cow in &cows {
        // Purely cosmetic, so keying off the entity id is fine — it needs to be
        // stable for this run, not across runs.
        let seed = cow.to_bits();
        let pattern = HidePattern::from_seed(seed);

        let nodes: Vec<Entity> = children
            .iter_descendants(cow)
            .filter(|&e| names.get(e).is_ok_and(|n| SPLOTCH_NODES.contains(&n.as_str())))
            .collect();
        if nodes.len() < SPLOTCH_NODES.len() {
            continue; // scene still loading
        }

        // The loader may put the mesh on the named node or on a child per
        // primitive, so check both — same as `apply_model_tint`.
        let mut painted = 0;
        for node in nodes {
            for part in core::iter::once(node).chain(children.iter_descendants(node)) {
                let Ok(mut handle) = mesh_handles.get_mut(part) else {
                    continue;
                };
                let Some(source) = meshes.get(&handle.0) else {
                    continue;
                };
                let Some(VertexAttributeValues::Float32x3(positions)) =
                    source.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    continue;
                };

                // Rest-pose positions: the patch is fixed to the hide and gets
                // skinned along with everything else.
                let colors: Vec<[f32; 4]> = positions
                    .iter()
                    .map(|p| pattern.tint_at(Vec3::from_array(*p), seed))
                    .collect();

                let mut owned = source.clone();
                owned.insert_attribute(
                    Mesh::ATTRIBUTE_COLOR,
                    VertexAttributeValues::Float32x4(colors),
                );
                handle.0 = meshes.add(owned);
                painted += 1;
            }
        }

        if painted > 0 {
            commands.entity(cow).insert(HideSplotched);
        }
    }
}
