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
//
// This width (and the herd/player spawns and camera target/offset) was scaled
// down by `CAM_ZOOM` in lockstep with the camera's play box, so pulling the
// camera in leaves the fenced area the same size on screen while the characters
// — which keep their modelled size — grow by `CAM_ZOOM`.
pub const FIELD_HALF_X: f32 = 13.67;
// Deep enough that its top edge (the clamp ceiling) sits just past `PEN_LINE_Z`,
// so a herded cow can only get over the line by threading the gap. Not scaled as
// hard as the width: the far edge is off-screen anyway, and this keeps the pen
// line up under the top frame with a little grass showing above it.
pub const FIELD_HALF_Z: f32 = 34.0;
pub const BOUNDARY_MARGIN: f32 = 4.0;

// Near (bottom, +Z) edge. On screen: the visible scene bottom (where the
// bottom frame piece takes over) sits at z ≈ 28.8 on the guaranteed play box,
// so the fence line here shows just above it, flanking the frame's hump —
// entities now stop at a fence the player can see instead of wandering under
// the frame.
pub const FIELD_Z_NEAR: f32 = 27.5;

// ---------------------------------------------------------------------------
// Camera (angled orthographic — real 3D, no perspective foreshortening)
// ---------------------------------------------------------------------------

/// How much closer the camera is pulled than the original framing. The play box
/// below is divided by this and the whole level (above) multiplied by its
/// reciprocal, so the fenced field keeps its on-screen size while the characters
/// — modelled at a fixed size — render this much larger.
pub const CAM_ZOOM: f32 = 1.5;

/// Ground point the camera looks at (X, Z). Centred on the vertical play area,
/// plus a fixed -Z (up-screen) nudge: the viewport spans the whole cabinet (the
/// scene renders behind the top wood strip too), which lifts AutoMin's centre by
/// half the strip's height — `FRAME_TOP_H_FRAC / 2 * CAM_MIN_WIDTH / cos 45° ≈
/// 1.9` z-units at any window size. Aiming that much further up-screen drops
/// the play area back where it was, leaving the extra revealed world tucked
/// under the strip.
pub const CAM_TARGET: Vec2 = Vec2::new(0.0, -3.23);
/// Camera offset from its target: high up and pulled back along +Z for a ~45°
/// downward pitch so cubes show their height. Screen-up maps to world -Z. The
/// magnitude only frames the near/far planes (orthographic size comes from the
/// play box below), so it rides down with the rest of the shrunk layout.
pub const CAM_OFFSET: Vec3 = Vec3::new(0.0, 26.67, 26.67);
/// Orthographic play box for `ScalingMode::AutoMin` (portrait-shaped): this
/// area is always fully visible; wider/taller screens just reveal more ground.
/// The original 44×64 framing, pulled in by `CAM_ZOOM`. The level literals above
/// were scaled by the reciprocal by hand — re-zoom here and they must move too.
pub const CAM_MIN_WIDTH: f32 = 44.0 / CAM_ZOOM;
pub const CAM_MIN_HEIGHT: f32 = 64.0 / CAM_ZOOM;

// ---------------------------------------------------------------------------
// Lighting — phase 1 of the "crisp" pass (Mario vs. DK-style toy look): a
// strong sun with tight shadows and real contrast, no washed-out ambient.
// ---------------------------------------------------------------------------

/// Sun position (it looks at the origin). On the *camera's* side of the sky
/// (+Z, offset left): the camera then sees the characters' lit faces, not
/// their shade side — with the sun behind the field they only read via ambient
/// and washed out. Shadows fall up-right, peeking around each body. Elevation
/// ~53°.
pub const SUN_POSITION: Vec3 = Vec3::new(-18.0, 42.0, 26.0);
/// Bright enough to model form at default camera exposure. Walked down from 9k
/// in two rounds of "still a bit bright"; 12k read as washed-out pastel.
pub const SUN_ILLUMINANCE: f32 = 6_800.0;
/// Fill: enough that shadows read as soft shade rather than hard black, but
/// under the old 240, which flattened the whole scene into its albedo.
pub const AMBIENT_BRIGHTNESS: f32 = 185.0;

/// Single shadow cascade clamped to what the fixed camera can actually see
/// (nearest visible ground is ~29 units from the camera, farthest ~70). The
/// default 4 cascades spread over ~1000 units left a handful of shadow texels
/// on the play field; this puts the whole 2048 map on it — sharper shadows AND
/// less GPU work.
pub const SHADOW_MIN_DISTANCE: f32 = 22.0;
pub const SHADOW_MAX_DISTANCE: f32 = 78.0;

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
// Body collisions & ragdolls
//
// Hand-rolled circle physics rather than a physics engine: rapier/avian would
// add megabytes to the size-tuned wasm bundle, aren't reliably deterministic
// across platforms (which the lockstep-netcode goal needs), and a jointed
// skeletal ragdoll is invisible fidelity at this art scale. A ballistic tumble
// of the whole model reads as "ragdoll" here and costs nothing.
// ---------------------------------------------------------------------------

pub const COW_MASS: f32 = 1.0;
/// Horse + rider together. Heavier than a cow so routine herding contact shoves
/// cows aside rather than rocking the player.
pub const PLAYER_MASS: f32 = 2.4;
/// Bounciness of body-body impacts (0 = dead thud, 1 = elastic).
pub const BODY_RESTITUTION: f32 = 0.35;
/// Fraction of a pair's overlap closed per frame. Under 1 so the tightly packed
/// spawn cluster eases apart over a few frames instead of popping.
pub const BODY_RELAX: f32 = 0.45;

/// Closing speed along the contact normal that unseats the rider. Chasing a
/// fleeing cow tops out around `PLAYER_SPEED - COW_MAX_SPEED` = 8 (safe, that's
/// herding); plowing full gallop into a cow with nowhere to run measures ~10-19.
/// The line sits just above the first regime.
pub const RIDER_THROW_SPEED: f32 = 10.0;
/// Closing speed that sends the horse flying too — takes a cow coming *at* you
/// at speed while you gallop into it.
pub const HORSE_THROW_SPEED: f32 = 16.0;

// A thrown body's launch velocity = pre-impact velocity * CARRY (the rider keeps
// going over the handlebars) + the impact's velocity change * SHOVE (a broadside
// hit batters him sideways), plus the vertical kick.
pub const RAGDOLL_CARRY: f32 = 0.9;
pub const RAGDOLL_SHOVE: f32 = 2.2;
pub const RAGDOLL_UP: f32 = 8.5;
/// Heavier than earth gravity — arcade scale; a realistic 9.8 floats.
pub const RAGDOLL_GRAVITY: f32 = 30.0;
pub const RAGDOLL_BOUNCE: f32 = 0.4;
/// Max tumble rate, rad/s.
pub const RAGDOLL_SPIN: f32 = 9.0;
/// Brief screen-space jolt when a rider is thrown. Kept small so the cabinet
/// remains visually stable while the playfield takes the hit.
pub const CAMERA_SHAKE_SECS: f32 = 0.32;
pub const CAMERA_SHAKE_STRENGTH: f32 = 0.34;
/// How long a player stays down (ragdoll plays out) before respawning.
pub const DOWNED_SECS: f32 = 2.4;
/// Where a respawned player reappears (also the startup spawn).
pub const PLAYER_SPAWN: Vec2 = Vec2::new(0.0, 22.0);

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
// Level: a fence across the top of the field with a gap to herd cows through
// ---------------------------------------------------------------------------

// There is no pen box — just a fence line spanning the field near the top (far,
// -Z), split by a gap at the centre. Driving a cow through the gap captures it;
// a captured cow then walks off the top and vanishes rather than filling a pen.
// Kept high so the line sits just below the top frame and barely eats the play
// area.
pub const PEN_LINE_Z: f32 = -32.0;
pub const GATE_HALF: f32 = 3.0; // half-width of the gap

/// Once through the gap, a captured cow marches straight off the top at this
/// speed (units/s) — a steady amble, not the boid herd's dash — and is hidden
/// past `PEN_HIDE_Z`, so it leaves the play area for good instead of leaking back.
pub const PEN_EXIT_SPEED: f32 = 8.0;
/// World Z at which an escaped cow is fully hidden. Past the top of the view
/// (which sits around `-FIELD_HALF_Z`) so it disappears off-screen, not on it.
pub const PEN_HIDE_Z: f32 = -44.0;

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

/// Dust-puff sprite sheet: the balls project's smoke sheet, downscaled and cut
/// with a transparent gutter per cell. A 7×7 grid of 64px cells, frames 0..45
/// (the last three cells are blank); one frame plays over each puff's short life.
pub const DUST_TEXTURE: &str = "textures/dust.png";
pub const DUST_SHEET_COLS: usize = 7;
pub const DUST_SHEET_FRAMES: usize = 46;

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

// ---------------------------------------------------------------------------
// Ground texture: the foam grain extended to the field, plus a short
// grass/dirt mottle. Generated at startup (setup::ground_image) as a tileable
// tint map multiplied over `GROUND_COLOR`, so that stays the colour knob.
// ---------------------------------------------------------------------------

/// Texture edge in pixels. With `GROUND_TEX_REPEAT` tiles over the 400-unit
/// ground plane a texel lands at ~0.05 world units — about one screen pixel at
/// gameplay zoom, matching the characters' per-vertex foam speckle.
pub const GROUND_TEX_SIZE: usize = 256;
/// Times the texture repeats across the ground plane (via `uv_transform`).
pub const GROUND_TEX_REPEAT: f32 = 32.0;
/// Brightness swing of each octave, around 1.0: coarse grass patchiness, mid
/// mottle, fine foam grain. Kept low so the field reads as texture, not noise.
pub const GROUND_PATCH_AMP: f32 = 0.045;
pub const GROUND_MOTTLE_AMP: f32 = 0.05;
pub const GROUND_GRAIN_AMP: f32 = 0.06;
/// Sparse dirt speckles: the value-noise threshold that leaves a few percent of
/// the field speckled (noise clusters around 0.5, so 0.72 is already rare), and
/// the warm multiplier the speckles pull toward.
pub const GROUND_DIRT_THRESH: f32 = 0.72;
pub const GROUND_DIRT_TINT: (f32, f32, f32) = (0.92, 0.80, 0.62);
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
