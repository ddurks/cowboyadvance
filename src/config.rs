//! Central home for every tunable knob: gameplay, level layout, and rendering.
//!
//! Only depends on `glam` math (via `bevy::math`), so it's engine-agnostic.
//! Colours are stored as plain sRGB tuples and turned into Bevy `Color`s at
//! spawn time to keep this module free of rendering-API churn.

use bevy::math::{Vec2, Vec3};
use core::f32::consts::{FRAC_PI_2, PI};

// ---------------------------------------------------------------------------
// Field / world. The simulation runs on the X/Z ground plane (Y is up).
// ---------------------------------------------------------------------------

// Portrait-oriented field: narrow in X, long in Z (herd "up" the screen, -Z).
//
// Held just inside `CAM_MIN_WIDTH / 2` so the side fences land *inside* the
// view rather than exactly on its edge, where they were being clipped to a
// half-post sliver. The strip of grass beyond them is what makes them read as
// the edge of the field instead of the edge of the screen.
pub const FIELD_HALF_X: f32 = 20.5;
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

/// Above this window aspect the cabinet stops taking the window's shape and
/// falls back to `CABINET_AR`, letterboxed. Landscape and desktop windows are
/// the wrong shape for a portrait cabinet, so they keep the design ratio; every
/// portrait phone and tablet — including the squatter aspect you get once the
/// browser's own chrome has eaten into the viewport — sits below this and fills
/// the screen edge to edge instead.
pub const CABINET_AR_MAX: f32 = 0.80;
/// ...and below this it stops narrowing, so a freakishly tall window letterboxes
/// rather than shrinking the cabinet to a sliver.
pub const CABINET_AR_MIN: f32 = 0.40;

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
/// Collision radius. The horse is ~0.9 wide and ~3.2 long, so this is a
/// compromise between its shoulders and its nose.
pub const PLAYER_RADIUS: f32 = 1.1;

// ---------------------------------------------------------------------------
// Facing: entities rotate (yaw) to point their local -Z toward their travel
// direction — the glTF/Bevy character convention, so this carries straight over
// to real character models.
// ---------------------------------------------------------------------------

/// How fast heading chases the travel direction (higher = snappier turns).
pub const TURN_RATE: f32 = 12.0;
/// Below this speed, keep the current heading instead of snapping around.
pub const FACE_MIN_SPEED: f32 = 0.2;

// ---------------------------------------------------------------------------
// Cows / boids
// ---------------------------------------------------------------------------

pub const COW_COUNT: usize = 24;
pub const COW_MAX_SPEED: f32 = 11.0;
pub const COW_RADIUS: f32 = 1.1;
pub const COW_DRAG: f32 = 1.6;

pub const NEIGHBOR_RADIUS: f32 = 6.5;
/// Roughly a cow-length, so the herd packs without the models interpenetrating.
pub const SEPARATION_RADIUS: f32 = 3.0;
pub const FLEE_RADIUS: f32 = 9.5;
pub const FENCE_AVOID: f32 = 2.6;

pub const W_SEPARATION: f32 = 26.0;
pub const W_ALIGNMENT: f32 = 7.0;
pub const W_COHESION: f32 = 5.0;
pub const W_FLEE: f32 = 55.0;
pub const W_FENCE: f32 = 45.0;
pub const W_BOUNDARY: f32 = 32.0;

// ---------------------------------------------------------------------------
// Character models
//
// Built and exported by blender/build_rig.py (one .glb per rig, skinned, with
// baked clips). Everything below that looks like a magic number is read off
// that build: clip order, stride lengths, clip durations.
// ---------------------------------------------------------------------------

pub const HORSE_MODEL: &str = "models/horse.glb";
pub const COW_MODEL: &str = "models/cow.glb";
pub const COWBOY_MODEL: &str = "models/cowboy.glb";

/// Clip index inside each .glb. The exporter writes actions in alphabetical
/// order, and `GltfAssetLabel::Animation(i)` is the only way to grab one clip
/// without first waiting on the whole `Gltf` asset — so the order lives here.
/// Re-check these if a clip is ever added or renamed in ca_anims.py.
pub const HORSE_CLIP_IDLE: usize = 0;
pub const HORSE_CLIP_RUN: usize = 1;
pub const HORSE_CLIP_STOP: usize = 2;
pub const HORSE_CLIP_WALK: usize = 3;

pub const COW_CLIP_IDLE: usize = 0;
pub const COW_CLIP_RUN: usize = 1;
pub const COW_CLIP_STOP: usize = 2;
pub const COW_CLIP_WALK: usize = 3;

// The rider never dismounts here, so cowboy_idle (0), cowboy_run (3) and
// cowboy_walk (4) go unused: `ride` is the gallop seat, `ride_walk` the relaxed
// one, and those two carry him at every speed.
pub const COWBOY_CLIP_RIDE: usize = 1;
pub const COWBOY_CLIP_RIDE_WALK: usize = 2;

/// The rigs are modelled facing +Z (Blender's -Y forward, exported Y-up), but
/// the game turns entities so their local -Z leads. Each model child carries
/// this yaw to reconcile the two.
pub const MODEL_YAW: f32 = PI;

/// glTF joint the rider is parented to, so he rides the horse's actual back
/// rather than a guessed offset. Kept out of the deform-bone strip in
/// build_rig.py (`export_def_bones=False`) precisely so it survives export.
pub const SADDLE_JOINT: &str = "saddle";
/// A Blender bone points down its own +Y, which the Y-up export turns into a
/// -90° tilt about X on the joint node. The rider cancels that out and is then
/// upright in the horse's space, bouncing with whatever the saddle does.
pub const SADDLE_FIX: f32 = FRAC_PI_2;

/// glTF nodes making up the cow's hide — everything sharing the `Cow` material.
/// These get swapped to the penned colour instead of tinting horns and hooves.
pub const COW_HIDE_NODES: &[&str] = &["cow_body", "cow_head", "cow_ears", "cow_tail"];
/// The rider's hat, dyed with the player's colour so coop players stay apart.
pub const RIDER_HAT_NODES: &[&str] = &["cowboy_hat"];

// ---------------------------------------------------------------------------
// Gaits
//
// Each locomotion clip has a stride (world units the feet cover per cycle) and
// a duration, so the ground speed at which its feet do not skate is
// `stride / duration` — the numbers below, straight from build_rig.py's report.
// Play a clip at `entity_speed / clip_speed` and the contact foot stays welded
// to the ground.
//
// That is the principle; `GAIT_SPEED_SCALE` is the concession. The field is
// arcade-scaled — a 1.5-unit-tall cow crosses seven of its own lengths every
// second — which is about 20x the pace these gaits were animated for. Taken
// literally the formula would run every cycle at 20x and the legs would be a
// blur, so every reference speed is scaled up by one shared factor. The
// authored walk:run relationship, and the relationship between the animals,
// both survive untouched; only the absolute pace is dialled back.
// ---------------------------------------------------------------------------

pub const HORSE_WALK_CLIP_SPEED: f32 = 0.44 / 1.0000; // 0.44 u stride, 24f @ 24fps
pub const HORSE_RUN_CLIP_SPEED: f32 = 0.62 / 0.6667; // 0.62 u stride, 16f
pub const COW_WALK_CLIP_SPEED: f32 = 0.30 / 1.1667; // 0.30 u stride, 28f
pub const COW_RUN_CLIP_SPEED: f32 = 0.40 / 0.7500; // 0.40 u stride, 18f

/// Raise for lazier legs at speed, lower for faster ones. At 14.5 the horse
/// hits 1.0x playback (~1.5 gallop strides/second) around three quarters of
/// `PLAYER_SPEED`, and walks a stride a second at a believable amble.
pub const GAIT_SPEED_SCALE: f32 = 14.5;

/// Gait thresholds, as fractions of that gait's reference speed. Enter and exit
/// differ so an entity hovering on a boundary does not flicker between clips.
pub const GAIT_WALK_ENTER: f32 = 0.30;
pub const GAIT_WALK_EXIT: f32 = 0.18;
pub const GAIT_RUN_ENTER: f32 = 0.62;
pub const GAIT_RUN_EXIT: f32 = 0.50;

/// Playback rate is clamped: below the floor a walk reads as slow motion, above
/// the ceiling a run reads as a sewing machine. The floor deliberately matches
/// `GAIT_WALK_ENTER`, so a walk starts exactly at the slowest rate it is allowed
/// and speeds up from there rather than opening on a clamp.
pub const GAIT_RATE_MIN: f32 = 0.30;
pub const GAIT_RATE_MAX: f32 = 2.20;
/// Cross-fade between clips, in seconds.
pub const GAIT_BLEND: f32 = 0.18;

/// Length of the one-shot skid clips (frames / 24 fps). Timed from here rather
/// than polled off the player, so the state machine never waits on an asset.
pub const HORSE_STOP_LEN: f32 = 20.0 / 24.0;
pub const COW_STOP_LEN: f32 = 22.0 / 24.0;
/// How long after running a halt still counts as a skid rather than a stroll to
/// a stop. Without this the clip would only fire on a same-frame drop from a
/// full gallop, which essentially never happens.
pub const GAIT_STOP_WINDOW: f32 = 0.40;

/// The rider's seat is the same loop whether the horse idles or walks, so idling
/// plays it slowed down — a horse standing still still shifts under its rider.
pub const RIDER_IDLE_RATE: f32 = 0.45;

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

/// Rail and post cross-section, read straight off the exported model. The sim
/// widens fence collision by half of this, and grass keeps clear of it.
pub const FENCE_THICK: f32 = 0.32;

// ---------------------------------------------------------------------------
// Fence props
//
// Hand-modelled in cowboy_advance.blend's `Fence` collection and cut into a
// repeatable post + a stretchable rail by blender/build_props.py, which prints
// the dimensions below. The wood texture is generated by blender/gen_wood.py
// from the cabinet frame's own palette, so fence and bezel are the same timber.
// ---------------------------------------------------------------------------

pub const FENCE_MODEL: &str = "models/fence.glb";
pub const WOOD_TEXTURE: &str = "textures/wood.png";

/// Mesh index inside fence.glb. The exporter writes the post first, the rail
/// second; re-check if build_props.py ever exports more pieces.
pub const FENCE_MESH_POST: usize = 0;
pub const FENCE_MESH_RAIL: usize = 1;

/// Distance between post centres in the exported module. A run is split into a
/// whole number of these and its rails stretched to absorb the rounding, so
/// every run ends on a post instead of a stub.
pub const FENCE_POST_SPACING: f32 = 3.4974;

/// Gate posts stand taller than the run they interrupt, so the opening the cows
/// have to be driven through reads at a glance.
pub const GATE_POST_SCALE: f32 = 1.45;

// ---------------------------------------------------------------------------
// Colours (linear-ish sRGB triples; converted with Color::srgb at spawn)
// ---------------------------------------------------------------------------

pub const GROUND_COLOR: (f32, f32, f32) = (0.34, 0.56, 0.28);
pub const COW_PENNED_COLOR: (f32, f32, f32) = (0.35, 0.80, 0.40);

/// The bezel's base brown, `#5e4123` — the exact flat fill the frame art is
/// drawn with, and the same value `blender/gen_wood.py` builds the fence
/// texture from. Everything around the cabinet is painted with it: the
/// letterbox bands on a window the wrong shape, and the safe-area strips a
/// phone leaves above and below. Black there made the frame look like it was
/// floating; this makes the whole screen read as one piece of timber.
pub const FRAME_WOOD_COLOR: (f32, f32, f32) = (0.3686, 0.2549, 0.1373);
pub const PEN_TINT_COLOR: (f32, f32, f32) = (0.30, 0.72, 0.38);

/// Distinct colour per player slot (supports the planned 4-player coop). Dyed
/// onto the rider's hat, which is the one part of him a top-down camera always
/// sees.
pub fn player_color(i: usize) -> (f32, f32, f32) {
    match i % 4 {
        0 => (0.25, 0.55, 0.95), // blue
        1 => (0.92, 0.32, 0.30), // red
        2 => (0.96, 0.80, 0.22), // gold
        _ => (0.62, 0.36, 0.86), // purple
    }
}
