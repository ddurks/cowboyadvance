//! The game world: entities, the boid herding simulation, and rendering.
//!
//! The whole simulation lives on the X/Z plane and is expressed with `Vec2`
//! (`.x` = world X, `.y` = world Z). Rendering lifts those into 3D. The update
//! step takes an array of `PlayerInput` — one per player — so single-player
//! today and 4-player coop later run through the exact same code path.

use macroquad::prelude::*;
use macroquad::rand::gen_range;

use crate::camera::make_camera;
use crate::config::*;
use crate::input::PlayerInput;
use crate::level::{Fence, Level};

pub struct Player {
    pub pos: Vec2,
    pub vel: Vec2,
    pub color: Color,
}

pub struct Cow {
    pub pos: Vec2,
    pub vel: Vec2,
    pub penned: bool,
}

pub struct World {
    pub players: Vec<Player>,
    pub cows: Vec<Cow>,
    pub level: Level,
    pub penned_count: usize,
}

impl World {
    pub fn new() -> Self {
        let level = Level::build();
        let players = vec![Player {
            pos: vec2(-25.0, 6.0),
            vel: Vec2::ZERO,
            color: player_color(0),
        }];
        let cows = spawn_cows();
        Self {
            players,
            cows,
            level,
            penned_count: 0,
        }
    }

    /// Add another cowboy (local coop / testing; networked players slot in the
    /// same way). Colour is chosen from the player index.
    #[allow(dead_code)] // wired up when multiplayer lands
    pub fn add_player(&mut self, spawn: Vec2) {
        let color = player_color(self.players.len());
        self.players.push(Player {
            pos: spawn,
            vel: Vec2::ZERO,
            color,
        });
    }

    pub fn camera(&self) -> Camera3D {
        make_camera(CAM_TARGET)
    }

    /// Advance the simulation by `dt` seconds. `inputs[i]` drives `players[i]`.
    pub fn update(&mut self, dt: f32, inputs: &[PlayerInput]) {
        // --- Players -------------------------------------------------------
        for (i, p) in self.players.iter_mut().enumerate() {
            let inp = inputs.get(i).copied().unwrap_or_default();
            // Input space (x=right, y=forward) -> world space (x, z). Forward is -Z.
            let target = vec2(inp.move_dir.x, -inp.move_dir.y) * PLAYER_SPEED;
            p.vel = p.vel.lerp(target, (PLAYER_ACCEL * dt).min(1.0));
            p.pos += p.vel * dt;
            resolve_fences(&mut p.pos, &mut p.vel, PLAYER_RADIUS, &self.level.fences);
            clamp_field(&mut p.pos, PLAYER_RADIUS);
        }

        // --- Cows (boids) --------------------------------------------------
        // Snapshot current state so every cow steers off the same frame.
        let snapshot: Vec<(Vec2, Vec2)> = self.cows.iter().map(|c| (c.pos, c.vel)).collect();

        for i in 0..self.cows.len() {
            let acc = boid_acceleration(i, &snapshot, &self.players, &self.level);
            let c = &mut self.cows[i];
            c.vel += acc * dt;
            c.vel *= 1.0 - (COW_DRAG * dt).min(1.0);
            c.vel = c.vel.clamp_length_max(COW_MAX_SPEED);
            c.pos += c.vel * dt;
            resolve_fences(&mut c.pos, &mut c.vel, COW_RADIUS, &self.level.fences);
            clamp_field(&mut c.pos, COW_RADIUS);
            c.penned = self.level.in_pen(c.pos);
        }

        self.penned_count = self.cows.iter().filter(|c| c.penned).count();
    }

    // -- Rendering ----------------------------------------------------------

    /// Draw the 3D scene. Call after `set_camera(&world.camera())`.
    pub fn draw_3d(&self) {
        // Ground.
        draw_plane(
            vec3(0.0, 0.0, 0.0),
            vec2(FIELD_HALF_X, FIELD_HALF_Z),
            None,
            GROUND_COLOR,
        );

        // Goal-zone tint over the pen floor.
        let center = (self.level.pen_min + self.level.pen_max) * 0.5;
        let half = (self.level.pen_max - self.level.pen_min) * 0.5;
        draw_plane(
            vec3(center.x, 0.02, center.y),
            half,
            None,
            Color::new(0.20, 0.80, 0.35, 0.22),
        );

        // Fences + gate posts.
        for f in &self.level.fences {
            draw_fence(f);
        }
        for &post in &self.level.posts {
            draw_cube(
                vec3(post.x, GATE_POST_H * 0.5, post.y),
                vec3(0.6, GATE_POST_H, 0.6),
                None,
                POST_COLOR,
            );
        }

        // Cows.
        for c in &self.cows {
            let color = if c.penned { COW_PENNED_COLOR } else { COW_COLOR };
            let p = vec3(c.pos.x, COW_SIZE.y * 0.5, c.pos.y);
            draw_cube(p, COW_SIZE, None, color);
            draw_cube_wires(p, COW_SIZE, BLACK);
        }

        // Players.
        for pl in &self.players {
            let p = vec3(pl.pos.x, PLAYER_SIZE.y * 0.5, pl.pos.y);
            draw_cube(p, PLAYER_SIZE, None, pl.color);
            draw_cube_wires(p, PLAYER_SIZE, BLACK);
        }
    }

    /// Draw the 2D HUD. Call after `set_default_camera()`.
    pub fn draw_hud(&self) {
        let total = self.cows.len();
        draw_text(
            &format!("Penned: {} / {}", self.penned_count, total),
            20.0,
            44.0,
            40.0,
            WHITE,
        );
        draw_text(
            "WASD / arrows or drag the joystick to herd the cows through the gate",
            20.0,
            72.0,
            24.0,
            Color::new(0.9, 0.9, 0.9, 0.9),
        );

        if total > 0 && self.penned_count == total {
            let msg = "ALL PENNED!";
            let dims = measure_text(msg, None, 64, 1.0);
            draw_text(
                msg,
                screen_width() * 0.5 - dims.width * 0.5,
                screen_height() * 0.45,
                64.0,
                GOLD,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Boid steering
// ---------------------------------------------------------------------------

/// Compute the steering acceleration for cow `i` from a frame snapshot.
///
/// Classic Reynolds boids (separation / alignment / cohesion) plus two
/// game-specific forces: fleeing from players (the herding push) and avoiding
/// fences + field edges.
fn boid_acceleration(
    i: usize,
    snapshot: &[(Vec2, Vec2)],
    players: &[Player],
    level: &Level,
) -> Vec2 {
    let (pos, _) = snapshot[i];

    let mut separation = Vec2::ZERO;
    let mut avg_vel = Vec2::ZERO;
    let mut avg_pos = Vec2::ZERO;
    let mut neighbors = 0.0;

    for (j, &(other_pos, other_vel)) in snapshot.iter().enumerate() {
        if j == i {
            continue;
        }
        let d = pos - other_pos;
        let dist = d.length();
        if dist < NEIGHBOR_RADIUS && dist > 1e-4 {
            avg_vel += other_vel;
            avg_pos += other_pos;
            neighbors += 1.0;
            if dist < SEPARATION_RADIUS {
                // Push apart, hardest when closest.
                separation += (d / dist) * (1.0 - dist / SEPARATION_RADIUS);
            }
        }
    }

    let mut acc = separation * W_SEPARATION;

    if neighbors > 0.0 {
        // Alignment: match the herd's heading.
        acc += (avg_vel / neighbors).normalize_or_zero() * W_ALIGNMENT;
        // Cohesion: drift toward the herd's centre.
        let center = avg_pos / neighbors;
        acc += (center - pos).normalize_or_zero() * W_COHESION;
    }

    // Flee from players — this is what lets the cowboy steer the herd.
    for p in players {
        let d = pos - p.pos;
        let dist = d.length();
        if dist < FLEE_RADIUS && dist > 1e-4 {
            acc += (d / dist) * (1.0 - dist / FLEE_RADIUS) * W_FLEE;
        }
    }

    // Steer away from fences before actually hitting them.
    for f in &level.fences {
        let cp = f.closest_point(pos);
        let d = pos - cp;
        let dist = d.length();
        if dist < FENCE_AVOID && dist > 1e-4 {
            acc += (d / dist) * (1.0 - dist / FENCE_AVOID) * W_FENCE;
        }
    }

    // Keep cows on the field.
    acc += boundary_force(pos);

    acc
}

/// Soft inward push when a cow strays near the field edge.
fn boundary_force(pos: Vec2) -> Vec2 {
    let m = BOUNDARY_MARGIN;
    let mut f = Vec2::ZERO;
    if pos.x > FIELD_HALF_X - m {
        f.x -= (pos.x - (FIELD_HALF_X - m)) / m;
    }
    if pos.x < -FIELD_HALF_X + m {
        f.x += ((-FIELD_HALF_X + m) - pos.x) / m;
    }
    if pos.y > FIELD_HALF_Z - m {
        f.y -= (pos.y - (FIELD_HALF_Z - m)) / m;
    }
    if pos.y < -FIELD_HALF_Z + m {
        f.y += ((-FIELD_HALF_Z + m) - pos.y) / m;
    }
    f * W_BOUNDARY
}

// ---------------------------------------------------------------------------
// Shared physics helpers (used by both players and cows)
// ---------------------------------------------------------------------------

/// Push a circle of `radius` out of any fence it overlaps and kill the velocity
/// component driving it into the wall.
fn resolve_fences(pos: &mut Vec2, vel: &mut Vec2, radius: f32, fences: &[Fence]) {
    let r = radius + FENCE_THICK * 0.5;
    for f in fences {
        let cp = f.closest_point(*pos);
        let d = *pos - cp;
        let dist = d.length();
        if dist < r {
            if dist > 1e-4 {
                let n = d / dist;
                *pos = cp + n * r;
                let into = vel.dot(n);
                if into < 0.0 {
                    *vel -= n * into;
                }
            } else {
                // Sitting exactly on the line: nudge off in an arbitrary dir.
                *pos += vec2(r, 0.0);
            }
        }
    }
}

/// Keep a position inside the field bounds.
fn clamp_field(pos: &mut Vec2, radius: f32) {
    pos.x = pos.x.clamp(-FIELD_HALF_X + radius, FIELD_HALF_X - radius);
    pos.y = pos.y.clamp(-FIELD_HALF_Z + radius, FIELD_HALF_Z - radius);
}

// ---------------------------------------------------------------------------
// Spawning + fence rendering
// ---------------------------------------------------------------------------

fn spawn_cows() -> Vec<Cow> {
    let mut cows = Vec::with_capacity(COW_COUNT);
    for _ in 0..COW_COUNT {
        cows.push(Cow {
            pos: vec2(gen_range(-32.0, -12.0), gen_range(-12.0, 12.0)),
            vel: Vec2::ZERO,
            penned: false,
        });
    }
    cows
}

/// Draw an axis-aligned fence segment as a thin, tall cube.
fn draw_fence(f: &Fence) {
    let mid = (f.a + f.b) * 0.5;
    let d = f.b - f.a;
    let size = if d.x.abs() >= d.y.abs() {
        vec3(d.x.abs() + FENCE_THICK, FENCE_HEIGHT, FENCE_THICK)
    } else {
        vec3(FENCE_THICK, FENCE_HEIGHT, d.y.abs() + FENCE_THICK)
    };
    draw_cube(vec3(mid.x, FENCE_HEIGHT * 0.5, mid.y), size, None, FENCE_COLOR);
}
