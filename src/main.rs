//! Cowboy Advance — a cattle-herding demo in real 3D (Bevy; desktop / web / iOS).
//!
//! The engine-agnostic herding simulation lives in `sim`; the rest is Bevy glue:
//! `setup` builds the 3D scene, `input` gathers controls, `systems` runs the
//! per-frame gameplay, `anim` drives the skinned characters. Designed so
//! `PlayerInputs` can hold N players for coop.

use bevy::asset::{AssetMetaCheck, AssetPlugin};
use bevy::log::{Level, LogPlugin};
use bevy::prelude::*;

mod anim;
mod cabinet;
mod components;
mod config;
mod dust;
mod fence;
mod foam;
mod grass;
mod hide;
mod input;
mod level;
mod loading;
mod noise;
mod ragdoll;
mod range;
mod setup;
mod sim;
mod systems;

fn main() {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(AssetPlugin {
                // No asset ships a .meta, so the default probe turns every web
                // load into an extra 404 round trip before the real fetch.
                meta_check: AssetMetaCheck::Never,
                ..default()
            })
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
        .add_plugins((
            grass::GrassPlugin,
            hide::CowHidePlugin,
            dust::DustPlugin,
            foam::FoamPlugin,
        ))
        .init_resource::<cabinet::CabinetLayout>()
        .init_resource::<components::PendingThrows>()
        .init_resource::<ragdoll::CameraShake>()
        .add_systems(Startup, (anim::load_models, setup::setup).chain())
        .add_systems(
            Update,
            (
                cabinet::compute_cabinet_layout,
                cabinet::update_game_viewport,
                input::gather_input,
                systems::player_movement,
                systems::cow_boids,
                // Hard contacts after both movers have written their positions;
                // a hit over the throw threshold ejects the rider (and maybe
                // the horse) into a ballistic tumble.
                systems::body_collisions,
                ragdoll::throw_players,
                systems::follow_camera,
                ragdoll::camera_shake,
                ragdoll::ragdoll_motion,
                ragdoll::respawn_players,
                // The .glbs parse asynchronously; these finish construction as
                // the pieces arrive, then keep watching for spawned parts.
                anim::finish_models,
                fence::spawn_fences_when_ready,
                anim::mount_rider,
                anim::bind_animation_players,
                anim::apply_model_tint,
                // The web overlay stays above every camera until all streamed
                // character scenes have completed their runtime setup.
                loading::finish_loading,
                // Gaits read the velocities the movement systems just wrote.
                anim::drive_gaits,
                systems::update_hud,
                cabinet::apply_cabinet_layout,
            )
                .chain(),
        );

    #[cfg(target_arch = "wasm32")]
    app.insert_resource(bevy::light::DirectionalLightShadowMap {
        size: config::WEB_SHADOW_MAP_SIZE,
    });

    app.run();
}
