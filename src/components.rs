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

/// HUD marker for the "penned" counter text.
#[derive(Component)]
pub struct PenCounter;

/// The level geometry, shared with systems as a resource.
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
    /// glTF nodes whose meshes the tint covers.
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

/// Convenience: sRGB triple -> Bevy `Color`.
pub fn srgb3(c: (f32, f32, f32)) -> Color {
    Color::srgb(c.0, c.1, c.2)
}
