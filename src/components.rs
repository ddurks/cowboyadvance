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

/// The offscreen texture the 3D game renders into, plus the window camera that
/// draws the cabinet UI. Kept alive here for future use (e.g. resize handling).
#[derive(Resource)]
#[allow(dead_code)]
pub struct GameScreen {
    pub image: Handle<Image>,
    pub ui_camera: Entity,
}

/// Shared material handles we swap between at runtime.
#[derive(Resource)]
pub struct GameAssets {
    pub cow: Handle<StandardMaterial>,
    pub cow_penned: Handle<StandardMaterial>,
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
