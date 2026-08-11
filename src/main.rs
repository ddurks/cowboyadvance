//! Cowboy Advance — a cattle-herding demo in real 3D (Bevy; desktop / web / iOS).
//!
//! The engine-agnostic herding simulation lives in `sim`; the rest is Bevy glue:
//! `setup` builds the 3D scene, `input` gathers controls, `systems` runs the
//! per-frame gameplay, `anim` drives the skinned characters. Designed so
//! `PlayerInputs` can hold N players for coop.

use bevy::log::{Level, LogPlugin};
use bevy::prelude::*;

mod anim;
mod cabinet;
mod components;
mod config;
mod fence;
mod grass;
mod hide;
mod input;
mod level;
mod noise;
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
                        // Phone-shaped by default (native); the web canvas overrides
                        // this. The cabinet is a fixed portrait aspect either way.
                        resolution: (430u32, 932u32).into(),
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
        .add_plugins((grass::GrassPlugin, hide::CowHidePlugin))
        .init_resource::<cabinet::CabinetLayout>()
        // Animation graphs before the scene, so the models find them waiting.
        .add_systems(Startup, (anim::load_animations, setup::setup).chain())
        .add_systems(
            Update,
            (
                cabinet::compute_cabinet_layout,
                cabinet::update_game_viewport,
                input::gather_input,
                systems::player_movement,
                systems::cow_boids,
                // Models spawn asynchronously, so these keep watching for parts
                // that have only just appeared.
                anim::mount_rider,
                anim::bind_animation_players,
                anim::apply_model_tint,
                // Gaits read the velocities the movement systems just wrote.
                anim::drive_gaits,
                systems::update_hud,
                cabinet::apply_cabinet_layout,
            )
                .chain(),
        )
        .run();
}
