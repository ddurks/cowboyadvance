//! Web loading-overlay readiness. The HTML sits above every Bevy camera; this
//! system dismisses it only after the streamed character scenes are mounted,
//! animated, textured, and tinted.

use bevy::prelude::*;

use crate::anim::{Animated, GaitState, MountRider};
use crate::components::ModelTint;
use crate::config::COW_COUNT;

pub fn finish_loading(
    mut ready_frames: Local<u8>,
    models: Query<(), With<Animated>>,
    animations: Query<(), With<GaitState>>,
    mounting: Query<(), With<MountRider>>,
    tints: Query<&ModelTint>,
) {
    if *ready_frames >= 2 {
        return; // overlay already dismissed; nothing left to drive
    }

    let expected = COW_COUNT + 2; // cows + horse + mounted rider
    let models_in = models.iter().count();
    let rigs_bound = animations.iter().count();
    // `parts` non-empty means the scene spawned and the tint found its mesh
    // nodes. `applied` can't gate readiness: it only ever flips true for tints
    // that are *on* (the rider's hat) — an unpenned cow stays `applied: false`
    // for the whole session, which would hold the overlay up forever.
    let tints_resolved = tints.iter().filter(|tint| !tint.parts.is_empty()).count();
    let tints_total = tints.iter().count();

    // Every milestone the overlay waits on, as a fraction for the loading bar.
    let done = models_in + rigs_bound + tints_resolved;
    let total = (expected * 2 + tints_total).max(1);
    set_progress(done as f32 / total as f32);

    let ready = mounting.is_empty()
        && models_in == expected
        && rigs_bound == expected
        && tints_total > 0
        && tints_resolved == tints_total;

    if !ready {
        *ready_frames = 0;
        return;
    }
    *ready_frames += 1;
    if *ready_frames == 2 {
        set_progress(1.0);
        dismiss_web_overlay();
    }
}

/// The shell's JS owns the bar's first 75% (download + boot); the engine sweeps
/// the rest — keep the split in sync with web/index.html.
#[cfg(target_arch = "wasm32")]
fn set_progress(engine_fraction: f32) {
    let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("loading-fill"))
    else {
        return;
    };
    let pct = 75.0 + engine_fraction.clamp(0.0, 1.0) * 25.0;
    let _ = element.set_attribute("style", &format!("width:{pct:.0}%"));
}

#[cfg(not(target_arch = "wasm32"))]
fn set_progress(_engine_fraction: f32) {}

#[cfg(target_arch = "wasm32")]
fn dismiss_web_overlay() {
    let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("loading-screen"))
    else {
        return;
    };
    element.set_class_name("ready");
}

#[cfg(not(target_arch = "wasm32"))]
fn dismiss_web_overlay() {}
