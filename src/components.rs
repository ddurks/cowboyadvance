//! ECS components and resources.
//!
//! The gameplay logic itself lives in the engine-agnostic `sim` module; these
//! types are just the Bevy-side wiring that feeds it.

use bevy::prelude::*;

use crate::level::Level;

/// A cowboy. `index` selects the colour and (later) which input stream drives it.
#[derive(Component)]
pub struct Player {
    pub index: usize,
}

/// A cow. `penned` is cached so we only swap its material when it changes.
#[derive(Component)]
pub struct Cow {
    pub penned: bool,
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

/// Marks the 3D game camera, whose window viewport is resized each frame to the
/// cabinet's scene rect.
#[derive(Component)]
pub struct GameCamera;

/// A runtime material override for part of a spawned glTF model: the cow's hide
/// turning green once it is penned, and the rider's hat carrying the player's
/// colour.
///
/// glTF materials are shared by every instance of a model, so recolouring one
/// cow means swapping the handle on that cow's own mesh entities. Those live
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
