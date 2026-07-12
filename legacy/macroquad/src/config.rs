//! Central home for every tunable knob in the game.
//!
//! Keeping gameplay + rendering constants in one place makes it easy to feel
//! out the herding behaviour without hunting through the simulation code.

use macroquad::prelude::*;

// ---------------------------------------------------------------------------
// Field / world (all simulation happens on the X/Z plane, measured in "metres")
// ---------------------------------------------------------------------------

/// Half-extents of the playable field. Field spans [-HALF, +HALF] on each axis.
pub const FIELD_HALF_X: f32 = 40.0;
pub const FIELD_HALF_Z: f32 = 28.0;

/// How far from the edge cows start being nudged back toward the centre.
pub const BOUNDARY_MARGIN: f32 = 6.0;

// ---------------------------------------------------------------------------
// Camera (orthographic, angled down so cubes read as 3D)
// ---------------------------------------------------------------------------

/// The point on the ground the camera looks at (X, Z).
pub const CAM_TARGET: Vec2 = vec2(4.0, 0.0);
/// Camera sits this high above the target...
pub const CAM_HEIGHT: f32 = 38.0;
/// ...and this far back along +Z, giving a ~40° downward pitch so cubes show
/// their height and read as 3D blocks.
pub const CAM_BACK: f32 = 44.0;
/// Orthographic "zoom": world-space height visible on screen. Bigger = zoomed out.
pub const CAM_ORTHO_HEIGHT: f32 = 62.0;

// ---------------------------------------------------------------------------
// Player
// ---------------------------------------------------------------------------

pub const PLAYER_SPEED: f32 = 19.0;
/// How quickly velocity chases the input direction (higher = snappier).
pub const PLAYER_ACCEL: f32 = 14.0;
pub const PLAYER_SIZE: Vec3 = vec3(1.8, 2.6, 1.8);
pub const PLAYER_RADIUS: f32 = 0.9;

// ---------------------------------------------------------------------------
// Cows / boids
// ---------------------------------------------------------------------------

pub const COW_COUNT: usize = 24;
pub const COW_MAX_SPEED: f32 = 11.0;
pub const COW_SIZE: Vec3 = vec3(1.4, 1.4, 1.4);
pub const COW_RADIUS: f32 = 0.9;
/// Per-second velocity bleed so an undisturbed herd settles instead of drifting.
pub const COW_DRAG: f32 = 1.6;

/// Cows consider other cows within this range for alignment + cohesion.
pub const NEIGHBOR_RADIUS: f32 = 6.5;
/// Cows actively push apart from other cows closer than this.
pub const SEPARATION_RADIUS: f32 = 2.6;
/// Cows flee any player within this range (the herding force).
pub const FLEE_RADIUS: f32 = 9.5;
/// Cows start steering away from a fence within this distance.
pub const FENCE_AVOID: f32 = 2.6;

// Boid steering weights. These are the main "feel" dials.
pub const W_SEPARATION: f32 = 26.0;
pub const W_ALIGNMENT: f32 = 7.0;
pub const W_COHESION: f32 = 5.0;
pub const W_FLEE: f32 = 55.0;
pub const W_FENCE: f32 = 45.0;
pub const W_BOUNDARY: f32 = 32.0;

// ---------------------------------------------------------------------------
// Level: pen (a rectangle) with a gate gap in its left wall
// ---------------------------------------------------------------------------

pub const PEN_X0: f32 = 14.0;
pub const PEN_X1: f32 = 34.0;
pub const PEN_Z0: f32 = -12.0;
pub const PEN_Z1: f32 = 12.0;
/// Half-width of the gate opening (centred at Z = 0 on the pen's left wall).
pub const GATE_HALF: f32 = 3.75;

pub const FENCE_HEIGHT: f32 = 2.2;
pub const FENCE_THICK: f32 = 0.35;
pub const GATE_POST_H: f32 = 2.8;

// ---------------------------------------------------------------------------
// Virtual joystick (screen-space, in pixels)
// ---------------------------------------------------------------------------

pub const JOYSTICK_RADIUS: f32 = 92.0;
pub const JOYSTICK_MARGIN: f32 = 64.0;

// ---------------------------------------------------------------------------
// Colours
// ---------------------------------------------------------------------------

pub const SKY_COLOR: Color = Color::new(0.55, 0.75, 0.95, 1.0);
pub const GROUND_COLOR: Color = Color::new(0.34, 0.56, 0.28, 1.0);
pub const COW_COLOR: Color = Color::new(0.58, 0.42, 0.30, 1.0);
pub const COW_PENNED_COLOR: Color = Color::new(0.35, 0.80, 0.40, 1.0);
pub const FENCE_COLOR: Color = Color::new(0.45, 0.30, 0.18, 1.0);
pub const POST_COLOR: Color = Color::new(0.30, 0.20, 0.12, 1.0);

/// Distinct colour per player slot (supports the planned 4-player coop).
pub fn player_color(i: usize) -> Color {
    match i % 4 {
        0 => Color::new(0.25, 0.55, 0.95, 1.0), // blue
        1 => Color::new(0.92, 0.32, 0.30, 1.0), // red
        2 => Color::new(0.96, 0.80, 0.22, 1.0), // gold
        _ => Color::new(0.62, 0.36, 0.86, 1.0), // purple
    }
}
