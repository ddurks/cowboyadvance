//! Building fence out of the hand-modelled post and rail.
//!
//! `blender/build_props.py` exports one post and one rail's worth of geometry;
//! everything the player sees is those two meshes repeated. A run is divided
//! into a whole number of modules and the rails stretched slightly to close the
//! rounding, so a run always ends on a post rather than a sawn-off rail, and the
//! posts stay evenly spaced whatever length the level asks for.
//!
//! Every piece shares one material, so a couple of hundred of them still batch
//! down to a handful of draw calls.

use bevy::prelude::*;

use crate::config::*;
use crate::level::Level;

/// Yaw that swings the rail's local +X onto `dir` (a unit vector on X/Z).
///
/// A Y rotation takes +X to `(cos θ, 0, -sin θ)`, so matching `dir` means
/// `cos θ = dir.x` and `sin θ = -dir.y`.
fn run_yaw(dir: Vec2) -> Quat {
    Quat::from_rotation_y((-dir.y).atan2(dir.x))
}

/// Spawn the pen's fences and the field boundary.
pub fn spawn_fences(
    commands: &mut Commands,
    asset_server: &AssetServer,
    materials: &mut Assets<StandardMaterial>,
    level: &Level,
) {
    let post_mesh: Handle<Mesh> = asset_server.load(
        GltfAssetLabel::Primitive { mesh: FENCE_MESH_POST, primitive: 0 }.from_asset(FENCE_MODEL),
    );
    let rail_mesh: Handle<Mesh> = asset_server.load(
        GltfAssetLabel::Primitive { mesh: FENCE_MESH_RAIL, primitive: 0 }.from_asset(FENCE_MODEL),
    );
    // The UVs are unwrapped inside 0..1, so the default clamping sampler is
    // fine and the texture never needs a repeat mode set on it.
    let wood = materials.add(StandardMaterial {
        base_color_texture: Some(asset_server.load(WOOD_TEXTURE)),
        perceptual_roughness: 1.0,
        ..default()
    });

    // Rails go up as we walk each run; posts are collected first so that a
    // corner shared by two runs only gets one post.
    let mut posts: Vec<Vec2> = Vec::new();

    for f in level.fences.iter().chain(level.boundary.iter()) {
        let delta = f.b - f.a;
        let len = delta.length();
        if len < 1e-3 {
            continue;
        }
        let dir = delta / len;
        let modules = ((len / FENCE_POST_SPACING).round() as usize).max(1);
        let span = len / modules as f32;
        let yaw = run_yaw(dir);

        for i in 0..modules {
            let at = f.a + dir * (i as f32 * span);
            commands.spawn((
                Mesh3d(rail_mesh.clone()),
                MeshMaterial3d(wood.clone()),
                Transform {
                    translation: Vec3::new(at.x, 0.0, at.y),
                    rotation: yaw,
                    // Only the length is adjusted; the rail keeps its section.
                    scale: Vec3::new(span / FENCE_POST_SPACING, 1.0, 1.0),
                },
            ));
        }
        for i in 0..=modules {
            posts.push(f.a + dir * (i as f32 * span));
        }
    }

    // Drop posts that coincide — the pen's corners and the gate ends each turn
    // up twice, and stacking identical geometry z-fights.
    let mut placed: Vec<Vec2> = Vec::with_capacity(posts.len());
    for p in posts {
        if placed.iter().any(|q| q.distance_squared(p) < 0.01) {
            continue;
        }
        placed.push(p);
    }

    for p in placed {
        let gate = level.posts.iter().any(|g| g.distance_squared(p) < 0.01);
        commands.spawn((
            Mesh3d(post_mesh.clone()),
            MeshMaterial3d(wood.clone()),
            Transform {
                translation: Vec3::new(p.x, 0.0, p.y),
                rotation: Quat::IDENTITY,
                scale: if gate {
                    Vec3::new(1.0, GATE_POST_SCALE, 1.0)
                } else {
                    Vec3::ONE
                },
            },
        ));
    }
}
