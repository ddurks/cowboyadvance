//! ECS components and resources.
//!
//! The gameplay logic itself lives in the engine-agnostic `sim` module; these
//! types are just the Bevy-side wiring that feeds it.

use bevy::gltf::Gltf;
use bevy::prelude::*;

use crate::level::Level;

/// A cowboy. `index` selects the colour and (later) which input stream drives it.
#[derive(Component)]
pub struct Player {
    pub index: usize,
}

/// A cow. `penned` once it has been driven into the corral, where it stays.
#[derive(Component)]
pub struct Cow {
    pub penned: bool,
    /// Herding pressure carried frame to frame; see `sim::Boid::drive`.
    pub drive: Vec2,
}

/// 2D velocity on the X/Z ground plane.
#[derive(Component)]
pub struct Velocity(pub Vec2);

/// A player knocked out of the saddle: input is ignored and the ragdoll plays
/// out until `timer` runs down, then `ragdoll::respawn_players` rebuilds them.
#[derive(Component)]
pub struct Downed {
    pub timer: f32,
    /// The horse went flying too, so there is nothing standing at the player's
    /// position — skip it in collisions and cow avoidance until respawn.
    pub horse_gone: bool,
}

/// A hit hard enough to unseat somebody. Queued by `systems::body_collisions`,
/// consumed by `ragdoll::throw_players` in the same frame — a resource rather
/// than a message so the ordering stays explicit in the system chain.
pub struct Throw {
    pub player: Entity,
    /// Velocity going into the impact — the "over the handlebars" component.
    pub pre_vel: Vec2,
    /// Velocity after the impulse — carries the striker's momentum.
    pub post_vel: Vec2,
    pub horse_too: bool,
}

#[derive(Resource, Default)]
pub struct PendingThrows(pub Vec<Throw>);

#[derive(Component)]
pub struct PenCounter;

#[derive(Resource)]
pub struct GameLevel(pub Level);

/// The character .glb roots, requested once by `anim::load_models` and consumed
/// by `anim::finish_models` when all three have parsed.
///
/// Loading a *labeled* sub-asset (`Scene(0)`, `Animation(2)`, …) triggers its
/// own full fetch and parse of the source file, and every parse re-inserts all
/// of the file's sub-assets. On desktop the duplicate parses land within the
/// world-asset spawner's debounce window; over HTTP they finish seconds apart,
/// so each late parse fires `Modified` on an already-spawned scene, which the
/// spawner answers by despawning and rebuilding every instance — wiping the
/// runtime vertex bakes (hide splotches, foam grain) and despawning the rider
/// with the saddle joint he was parented under. One root load per file means
/// one parse, one `Added`, no storms — and one download instead of five.
#[derive(Resource)]
pub struct PendingModels {
    pub horse: Handle<Gltf>,
    pub cow: Handle<Gltf>,
    pub cowboy: Handle<Gltf>,
}

/// Session-lifetime strong handles to the character scenes. Model entities are
/// despawned and rebuilt across ragdoll respawns; if one of them held the last
/// handle, the despawn would unload the scene and the respawn would re-fetch it
/// — a whole HTTP round trip on web — and the unload/reload churn can catch the
/// world-asset spawner between its Modified-event read and the instance update,
/// where a missing asset is a panic (`NonExistentWorldAsset`).
///
/// Inserted by `anim::finish_models` once the .glb parses land, so consumers
/// take it as `Option<Res>` and wait.
#[derive(Resource)]
pub struct ModelScenes {
    pub horse: Handle<WorldAsset>,
    pub cow: Handle<WorldAsset>,
    pub rider: Handle<WorldAsset>,
    /// The parsed .glb roots, never read — held only so no sub-asset unloads.
    pub _roots: [Handle<Gltf>; 3],
}

/// Which player slot this device controls — the one the camera follows. Other
/// slots are other riders (bots now, remote players later), never "not player 0".
#[derive(Resource, Default)]
pub struct LocalPlayer(pub usize);

/// Ground point (X, Z) the game camera is centred on; it trails the local
/// rider. Grass streams and the trample map scrolls around it.
#[derive(Resource)]
pub struct CameraFocus(pub Vec2);

/// Marks the 3D game camera, whose window viewport is resized each frame to the
/// cabinet's scene rect.
#[derive(Component)]
pub struct GameCamera;

/// A runtime material override for part of a spawned glTF model: the rider's
/// hat carrying the player's colour.
///
/// glTF materials are shared by every instance of a model, so recolouring one
/// rider means swapping the handle on that cow's own mesh entities. Those live
/// inside the scene the asset server spawns a few frames later, so the parts are
/// resolved lazily, by glTF node name, and cached here.
#[derive(Component)]
pub struct ModelTint {
    pub nodes: &'static [&'static str],
    pub tint: Handle<StandardMaterial>,
    /// Whether the tint should be showing. Gameplay writes this; the tint
    /// system notices and repaints.
    pub on: bool,
    /// (mesh entity, the material it was modelled with). Empty until the model
    /// finishes spawning.
    pub parts: Vec<(Entity, Handle<StandardMaterial>)>,
    /// What the meshes are actually painted with right now.
    pub applied: bool,
}

impl ModelTint {
    pub fn new(nodes: &'static [&'static str], tint: Handle<StandardMaterial>, on: bool) -> Self {
        Self { nodes, tint, on, parts: Vec::new(), applied: false }
    }
}

/// Per-player movement intent (input space: x = right, y = forward). Indexed by
/// `Player::index`, so networked/local players later just fill more slots.
#[derive(Resource, Default)]
pub struct PlayerInputs(pub Vec<Vec2>);

/// Stationary virtual-joystick state (screen pixels).
#[derive(Resource, Default)]
pub struct JoystickState {
    /// Knob offset from the base centre, in screen pixels (x right, y down).
    pub knob: Vec2,
    pub active: bool,
    pub touch_id: Option<u64>,
    pub mouse_tracking: bool,
}

pub fn srgb3(c: (f32, f32, f32)) -> Color {
    Color::srgb(c.0, c.1, c.2)
}
