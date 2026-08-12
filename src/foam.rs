//! The soft-foam "muppet" surface on every character.
//!
//! Two ingredients, both WebGL2-safe and free per frame:
//!
//! * **Vertex-colour grain.** The character meshes carry no UVs (stripped at
//!   export — see `hide`), so surface texture is baked per vertex, exactly like
//!   the cow splotches: triangles are around a pixel on screen, which makes
//!   per-vertex noise per-pixel speckle at display size. Baked from rest-pose
//!   positions, so it skins along with the model.
//! * **A fully matte response.** Foam has no specular highlight, so every
//!   character material is flattened to roughness 1.0 with (almost) no
//!   reflectance; form then comes entirely from the phase-1 sun and the grain.
//!
//! Cow bodies and heads get their grain inside `hide`'s splotch bake (one clone,
//! colours written once); everything else — horse, rider, and the cow's small
//! parts — is grained here. Clones are cached by source mesh so a respawned
//! horse or rider reuses the grained copy instead of leaking a new one.

use bevy::mesh::VertexAttributeValues;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::anim::Animated;
use crate::noise::value_noise3;

/// Grain cells in model units. Fine cell sits near the vertex spacing, so it
/// resolves as random per-vertex speckle — which is the foam look; the mid cell
/// adds soft clumping so the speckle doesn't read as uniform static.
const GRAIN_CELL_FINE: f32 = 0.05;
const GRAIN_CELL_MID: f32 = 0.14;
/// Brightness swing of each octave, around 1.0. Strong enough that the nap
/// still reads through the sun's shading gradients at gameplay zoom.
const GRAIN_FINE_AMP: f32 = 0.26;
const GRAIN_MID_AMP: f32 = 0.10;

/// One shared seed: the grain is surface texture, not identity, so every
/// character sampling the same field is fine (and lets clones be cached).
const GRAIN_SEED: u64 = 0xF0A;

/// Foam's matte response, applied to every character material.
pub const FOAM_ROUGHNESS: f32 = 1.0;
pub const FOAM_REFLECTANCE: f32 = 0.04;

/// Brightness multiplier of the foam grain at a rest-pose position.
pub fn grain_at(p: Vec3) -> f32 {
    1.0 + (value_noise3(p, GRAIN_CELL_FINE, GRAIN_SEED) - 0.5) * GRAIN_FINE_AMP
        + (value_noise3(p, GRAIN_CELL_MID, GRAIN_SEED ^ 0x99) - 0.5) * GRAIN_MID_AMP
}

pub struct FoamPlugin;

impl Plugin for FoamPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GrainCache>()
            .add_systems(Update, apply_foam);
    }
}

/// Source mesh -> its grained clone, so respawned models reuse instead of leak.
#[derive(Resource, Default)]
struct GrainCache(HashMap<AssetId<Mesh>, Handle<Mesh>>);

/// On the model root once its whole subtree has been foamed.
#[derive(Component)]
struct Foamed;

fn apply_foam(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cache: ResMut<GrainCache>,
    models: Query<Entity, (With<Animated>, Without<Foamed>)>,
    children: Query<&Children>,
    mut mesh_handles: Query<(&mut Mesh3d, Option<&MeshMaterial3d<StandardMaterial>>)>,
) {
    for root in &models {
        let mut touched = 0;
        for part in children.iter_descendants(root) {
            let Ok((mut handle, material)) = mesh_handles.get_mut(part) else {
                continue;
            };

            // Matte the material (shared assets: idempotent, settles once).
            if let Some(material) = material {
                if let Some(mut mat) = materials.get_mut(&material.0) {
                    mat.perceptual_roughness = FOAM_ROUGHNESS;
                    mat.reflectance = FOAM_REFLECTANCE;
                }
            }

            let id = handle.0.id();
            if let Some(grained) = cache.0.get(&id) {
                handle.0 = grained.clone();
                touched += 1;
                continue;
            }

            let Some(source) = meshes.get(&handle.0) else {
                continue; // mesh asset still loading
            };
            // Splotched cow parts already carry grain via `hide`'s bake.
            if source.attribute(Mesh::ATTRIBUTE_COLOR).is_some() {
                touched += 1;
                continue;
            }
            let Some(VertexAttributeValues::Float32x3(positions)) =
                source.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                continue;
            };

            let colors: Vec<[f32; 4]> = positions
                .iter()
                .map(|p| {
                    let g = grain_at(Vec3::from_array(*p));
                    [g, g, g, 1.0]
                })
                .collect();

            let mut owned = source.clone();
            owned.insert_attribute(Mesh::ATTRIBUTE_COLOR, VertexAttributeValues::Float32x4(colors));
            let grained = meshes.add(owned);
            cache.0.insert(id, grained.clone());
            handle.0 = grained;
            touched += 1;
        }

        if touched > 0 {
            commands.entity(root).insert(Foamed);
        }
    }
}
