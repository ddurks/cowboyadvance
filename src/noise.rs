//! Deterministic hashing and smooth value noise, on the CPU.
//!
//! Shared by the grass field (density and height clumping) and the cow hides
//! (splotches). No RNG crate, so no wasm `getrandom` dance, and everything is
//! reproducible from a seed — which keeps spawns identical run to run, the same
//! property `setup` already relies on.

use bevy::math::{Vec2, Vec3};

/// Deterministic pseudo-random in [0, 1) from an integer seed (splitmix-ish).
pub fn hash01(n: u64) -> f32 {
    let mut z = n
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    ((z >> 40) as f32) / (1u64 << 24) as f32
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Hermite smoothstep of `x` between `edge0` and `edge1`.
pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn corner2(x: f32, y: f32, seed: u64) -> f32 {
    hash01(
        (x as i64 as u64).wrapping_mul(0x9E37_79B9)
            ^ (y as i64 as u64).wrapping_mul(0x85EB_CA6B)
            ^ seed,
    )
}

fn corner3(x: f32, y: f32, z: f32, seed: u64) -> f32 {
    hash01(
        (x as i64 as u64).wrapping_mul(0x9E37_79B9)
            ^ (y as i64 as u64).wrapping_mul(0x85EB_CA6B)
            ^ (z as i64 as u64).wrapping_mul(0xC2B2_AE35)
            ^ seed,
    )
}

/// Smooth 2D value noise in [0, 1] over a grid of `cell` world units.
pub fn value_noise(p: Vec2, cell: f32, seed: u64) -> f32 {
    let g = p / cell;
    let (ix, iy) = (g.x.floor(), g.y.floor());
    let f = Vec2::new(g.x - ix, g.y - iy);
    // Smoothstep the interpolant so cells blend without visible seams.
    let s = Vec2::new(f.x * f.x * (3.0 - 2.0 * f.x), f.y * f.y * (3.0 - 2.0 * f.y));
    let (a, b) = (corner2(ix, iy, seed), corner2(ix + 1.0, iy, seed));
    let (c, d) = (corner2(ix, iy + 1.0, seed), corner2(ix + 1.0, iy + 1.0, seed));
    lerp(lerp(a, b, s.x), lerp(c, d, s.x), s.y)
}

/// Smooth 3D value noise in [0, 1] over a grid of `cell` units. Trilinear over
/// the eight surrounding lattice corners.
pub fn value_noise3(p: Vec3, cell: f32, seed: u64) -> f32 {
    let g = p / cell;
    let (ix, iy, iz) = (g.x.floor(), g.y.floor(), g.z.floor());
    let f = Vec3::new(g.x - ix, g.y - iy, g.z - iz);
    let s = Vec3::new(
        f.x * f.x * (3.0 - 2.0 * f.x),
        f.y * f.y * (3.0 - 2.0 * f.y),
        f.z * f.z * (3.0 - 2.0 * f.z),
    );
    let face = |dz: f32| {
        let z = iz + dz;
        let a = corner3(ix, iy, z, seed);
        let b = corner3(ix + 1.0, iy, z, seed);
        let c = corner3(ix, iy + 1.0, z, seed);
        let d = corner3(ix + 1.0, iy + 1.0, z, seed);
        lerp(lerp(a, b, s.x), lerp(c, d, s.x), s.y)
    };
    lerp(face(0.0), face(1.0), s.z)
}
