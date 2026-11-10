//! The open range's starting layout: where the wild cattle are.
//!
//! Pure and hash-based — no engine types, no RNG — so every machine that builds
//! the range gets the same one.

use bevy::math::Vec2;
use core::f32::consts::{SQRT_2, TAU};

use crate::config::*;
use crate::noise::{hash01, lerp};

/// Starting positions for all `COW_COUNT` wild cows: small groups near the
/// corral growing into big herds out by the river.
pub fn wild_cow_spawns() -> Vec<Vec2> {
    let inner = CORRAL_HALF * SQRT_2 + WILD_SPAWN_CLEARANCE;
    let max_spread = WILD_GROUP_SPREAD * (WILD_GROUP_MAX as f32).sqrt();
    let outer = RANGE_RADIUS - BOUNDARY_MARGIN - max_spread;

    let mut cows = Vec::with_capacity(COW_COUNT);
    let mut group = 0u64;
    while cows.len() < COW_COUNT {
        let seed = (group + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        // Uniform in radius rather than area: area-uniform would put nearly
        // every group out by the river and leave the ride out from the corral
        // empty.
        let reach = hash01(seed ^ 0x01);
        let centre = Vec2::from_angle(hash01(seed ^ 0x02) * TAU) * lerp(inner, outer, reach);
        let size = lerp(WILD_GROUP_MIN as f32, WILD_GROUP_MAX as f32, reach).round() as usize;
        let size = size.min(COW_COUNT - cows.len());
        let spread = WILD_GROUP_SPREAD * (size as f32).sqrt();
        for k in 0..size {
            let ks = seed ^ (k as u64 + 1).wrapping_mul(0xD1B5_4A32_D192_ED03);
            let offset = Vec2::from_angle(hash01(ks ^ 0x03) * TAU) * spread * hash01(ks ^ 0x04).sqrt();
            cows.push(centre + offset);
        }
        group += 1;
    }
    cows
}

/// Where the `n`th banked cow comes back as a wild one: uniform over the
/// range's area (the range refills evenly, not in its starting herds) and at
/// least `REPLACE_CLEARANCE` from every rider, so nobody sees it pop in.
pub fn replacement_spawn(n: u64, riders: &[Vec2]) -> Vec2 {
    let inner = CORRAL_HALF * SQRT_2 + WILD_SPAWN_CLEARANCE;
    let outer = RANGE_RADIUS - BOUNDARY_MARGIN - COW_RADIUS;
    let at = |attempt: u64| {
        let seed = (n + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (attempt + 1).wrapping_mul(0xD1B5_4A32_D192_ED03);
        let r = lerp(inner * inner, outer * outer, hash01(seed ^ 0x05)).sqrt();
        Vec2::from_angle(hash01(seed ^ 0x06) * TAU) * r
    };
    let clear = |p: &Vec2| riders.iter().all(|r| r.distance(*p) >= REPLACE_CLEARANCE);
    // A handful of tries always finds room on a range this size; the fallback
    // only matters if riders ever blanket it.
    (0..REPLACE_TRIES).map(at).find(clear).unwrap_or_else(|| at(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawns_every_cow_on_the_range_and_clear_of_the_corral() {
        let cows = wild_cow_spawns();
        assert_eq!(cows.len(), COW_COUNT);
        for p in &cows {
            assert!(p.length() < RANGE_RADIUS - BOUNDARY_MARGIN, "{p} past the bank");
            let corral = CORRAL_HALF + COW_RADIUS;
            assert!(p.x.abs() > corral || p.y.abs() > corral, "{p} inside the corral");
        }
    }

    #[test]
    fn replacements_land_on_the_range_away_from_riders() {
        let riders = [Vec2::new(60.0, 0.0), Vec2::new(-30.0, 90.0)];
        for n in 0..200 {
            let p = replacement_spawn(n, &riders);
            assert!(p.length() < RANGE_RADIUS - BOUNDARY_MARGIN, "{p} past the bank");
            assert!(riders.iter().all(|r| r.distance(p) >= REPLACE_CLEARANCE), "{p} near a rider");
        }
        assert_eq!(replacement_spawn(7, &riders), replacement_spawn(7, &riders));
    }

    #[test]
    fn is_deterministic() {
        assert_eq!(wild_cow_spawns(), wild_cow_spawns());
    }

    #[test]
    fn herds_grow_toward_the_river() {
        let cows = wild_cow_spawns();
        let mid = RANGE_RADIUS * 0.5;
        let near = cows.iter().filter(|p| p.length() < mid).count();
        let far = cows.len() - near;
        assert!(far > near, "near {near}, far {far}");
        assert!(near > 0, "nothing to gather on the way out");
    }
}
