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
    let expected = COW_COUNT + 2; // cows + horse + mounted rider
    // `parts` non-empty means the scene spawned and the tint found its mesh
    // nodes. `applied` can't gate readiness: it only ever flips true for tints
    // that are *on* (the rider's hat) — an unpenned cow stays `applied: false`
    // for the whole session, which would hold the overlay up forever.
    let ready = mounting.is_empty()
        && models.iter().count() == expected
        && animations.iter().count() == expected
        && tints.iter().all(|tint| !tint.parts.is_empty());

    if !ready {
        *ready_frames = 0;
        return;
    }
    *ready_frames = ready_frames.saturating_add(1);
    if *ready_frames == 2 {
        dismiss_web_overlay();
    }
}

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
