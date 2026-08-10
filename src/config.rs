//! Central home for every tunable knob: gameplay, level layout, and rendering.
//!
//! Only depends on `glam` math (via `bevy::math`), so it's engine-agnostic.
//! Colours are stored as plain sRGB tuples and turned into Bevy `Color`s at
//! spawn time to keep this module free of rendering-API churn.

use bevy::math::{Vec2, Vec3};

// ---------------------------------------------------------------------------
// Field / world. The simulation runs on the X/Z ground plane (Y is up).
// ---------------------------------------------------------------------------

// Portrait-oriented field: narrow in X, long in Z (herd "up" the screen, -Z).
pub const FIELD_HALF_X: f32 = 22.0;
pub const FIELD_HALF_Z: f32 = 40.0;
pub const BOUNDARY_MARGIN: f32 = 6.0;

// Near (bottom, +Z) edge. Straight, and pushed down to fill the full visible view
// (the scene extends behind the bottom frame piece); entities may pass under the
// hump — that's fine, it just hides them briefly.
pub const FIELD_Z_NEAR: f32 = 60.0;

// ---------------------------------------------------------------------------
// Camera (angled orthographic — real 3D, no perspective foreshortening)
// ---------------------------------------------------------------------------

/// Ground point the camera looks at (X, Z). Centred on the vertical play area.
pub const CAM_TARGET: Vec2 = Vec2::new(0.0, -2.0);
/// Camera offset from its target: high up and pulled back along +Z for a ~45°
/// downward pitch so cubes show their height. Screen-up maps to world -Z.
pub const CAM_OFFSET: Vec3 = Vec3::new(0.0, 40.0, 40.0);
/// Orthographic play box for `ScalingMode::AutoMin` (portrait-shaped): this
/// area is always fully visible; wider/taller screens just reveal more ground.
pub const CAM_MIN_WIDTH: f32 = 44.0;
pub const CAM_MIN_HEIGHT: f32 = 64.0;

// ---------------------------------------------------------------------------
// Frame HUD. The bezel art is split into two pieces so it never distorts: a top
// wood strip (assets/ui/frame_top.png) anchored to the top and a hump + bottom
// wood (assets/ui/frame_bottom.png) anchored to the bottom, each drawn full
// cabinet width at its own aspect. The game renders straight into the flexible
// middle via a camera viewport (AutoMin fits any height), so resizing only
// changes the scene, never the frame. The joystick (assets/ui/joystick.png,
// trimmed to content) rides on the hump in the bottom piece.
// ---------------------------------------------------------------------------

/// Fixed mobile-portrait aspect (width / height) for the whole cabinet. The
/// cabinet is contain-fit into the window at this ratio and centred, so the
/// viewport is identical on phone and desktop (black gutters fill the rest on
/// wide screens). Set to a modern iPhone portrait ratio (~9:19.5).
pub const CABINET_AR: f32 = 0.462;

/// Height of each frame piece as a fraction of cabinet width (= its source
/// height / width, so the art keeps its shape). The scene fills the gap between.
pub const FRAME_TOP_H_FRAC: f32 = 0.0917; // frame_top.png is 2400×220
pub const FRAME_BOTTOM_H_FRAC: f32 = 0.3158; // frame_bottom.png is 2400×758
/// Joystick rest centre within the bottom piece: x is centred, y is where the
/// knob sat in the source art (0.889 of the full frame, mapped into the piece).
pub const STICK_Y_IN_BOTTOM_FRAC: f32 = 0.531;
/// Knob (ball-top) diameter as a fraction of the cabinet width. Sits on — not
/// filling — the frame's hump.
pub const KNOB_DIAM_FRAC: f32 = 0.24;
/// How far the knob can travel from centre, as a fraction of cabinet width. Kept
/// small so the knob + its range ring stay inside the hump.
pub const STICK_RADIUS_FRAC: f32 = 0.06;

// ---------------------------------------------------------------------------
// Player
// ---------------------------------------------------------------------------

pub const PLAYER_SPEED: f32 = 19.0;
pub const PLAYER_ACCEL: f32 = 14.0;
pub const PLAYER_SIZE: Vec3 = Vec3::new(1.8, 2.6, 1.8);
pub const PLAYER_RADIUS: f32 = 0.9;

// ---------------------------------------------------------------------------
// Facing: entities rotate (yaw) to point their local -Z toward their travel
// direction — the glTF/Bevy character convention, so this carries straight over
// to real character models.
// ---------------------------------------------------------------------------

/// How fast heading chases the travel direction (higher = snappier turns).
pub const TURN_RATE: f32 = 12.0;
/// Below this speed, keep the current heading instead of snapping around.
pub const FACE_MIN_SPEED: f32 = 0.2;

/// Front "face" marker colours (a stand-in head, so heading is visible now).
pub const MARKER_DARK: (f32, f32, f32) = (0.12, 0.10, 0.08);
pub const MARKER_LIGHT: (f32, f32, f32) = (0.96, 0.95, 0.90);

// ---------------------------------------------------------------------------
// Cows / boids
// ---------------------------------------------------------------------------

pub const COW_COUNT: usize = 24;
pub const COW_MAX_SPEED: f32 = 11.0;
pub const COW_SIZE: Vec3 = Vec3::new(1.4, 1.4, 1.4);
pub const COW_RADIUS: f32 = 0.9;
pub const COW_DRAG: f32 = 1.6;

pub const NEIGHBOR_RADIUS: f32 = 6.5;
pub const SEPARATION_RADIUS: f32 = 2.6;
pub const FLEE_RADIUS: f32 = 9.5;
pub const FENCE_AVOID: f32 = 2.6;

pub const W_SEPARATION: f32 = 26.0;
pub const W_ALIGNMENT: f32 = 7.0;
pub const W_COHESION: f32 = 5.0;
pub const W_FLEE: f32 = 55.0;
pub const W_FENCE: f32 = 45.0;
pub const W_BOUNDARY: f32 = 32.0;

// ---------------------------------------------------------------------------
// Level: pen (a rectangle) with a gate gap in its left wall
// ---------------------------------------------------------------------------

// Pen sits at the top (far, -Z); the gate is a gap in its bottom wall (the wall
// nearest the herd), so you drive the cows up through it.
pub const PEN_X0: f32 = -15.0;
pub const PEN_X1: f32 = 15.0;
pub const PEN_Z0: f32 = -38.0; // far wall (top of screen)
pub const PEN_Z1: f32 = -18.0; // near wall, holds the gate
pub const GATE_HALF: f32 = 4.5;

pub const FENCE_HEIGHT: f32 = 2.2;
pub const FENCE_THICK: f32 = 0.35;
pub const GATE_POST_H: f32 = 2.8;

// ---------------------------------------------------------------------------
// Colours (linear-ish sRGB triples; converted with Color::srgb at spawn)
// ---------------------------------------------------------------------------

pub const GROUND_COLOR: (f32, f32, f32) = (0.34, 0.56, 0.28);
pub const COW_COLOR: (f32, f32, f32) = (0.58, 0.42, 0.30);
pub const COW_PENNED_COLOR: (f32, f32, f32) = (0.35, 0.80, 0.40);
pub const FENCE_COLOR: (f32, f32, f32) = (0.45, 0.30, 0.18);
pub const POST_COLOR: (f32, f32, f32) = (0.30, 0.20, 0.12);
pub const PEN_TINT_COLOR: (f32, f32, f32) = (0.30, 0.72, 0.38);

/// Distinct colour per player slot (supports the planned 4-player coop).
pub fn player_color(i: usize) -> (f32, f32, f32) {
    match i % 4 {
        0 => (0.25, 0.55, 0.95), // blue
        1 => (0.92, 0.32, 0.30), // red
        2 => (0.96, 0.80, 0.22), // gold
        _ => (0.62, 0.36, 0.86), // purple
    }
}
