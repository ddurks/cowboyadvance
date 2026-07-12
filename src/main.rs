//! Cowboy Advance — a cattle-herding demo in real 3D (Bevy; desktop / web / iOS).
//!
//! The engine-agnostic herding simulation lives in `sim`; the rest is Bevy glue:
//! `setup` builds the 3D scene, `input` gathers controls, `systems` runs the
//! per-frame gameplay. Designed so `PlayerInputs` can hold N players for coop.

use bevy::log::{Level, LogPlugin};
use bevy::prelude::*;

mod cabinet;
mod components;
mod config;
mod input;
mod level;
mod setup;
mod sim;
mod systems;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Cowboy Advance".into(),
                        canvas: Some("#bevy-canvas".into()),
                        fit_canvas_to_parent: true,
                        prevent_default_event_handling: true,
                        ..default()
                    }),
                    ..default()
                })
                .set(LogPlugin {
                    // Hide Bevy's INFO chatter, plus the benign "plugin not
                    // loaded" WARNs for GPU features WebGL2 doesn't support
                    // (OIT, SSAO, atmosphere, motion vectors…).
                    level: Level::WARN,
                    filter: "wgpu=error,naga=warn,bevy_core_pipeline=error,bevy_pbr=error".into(),
                    ..default()
                }),
        )
        .init_resource::<cabinet::CabinetLayout>()
        .add_systems(Startup, setup::setup)
        .add_systems(
            Update,
            (
                cabinet::compute_cabinet_layout,
                input::gather_input,
                systems::player_movement,
                systems::cow_boids,
                systems::update_hud,
                cabinet::apply_cabinet_layout,
            )
                .chain(),
        )
        .run();
}
