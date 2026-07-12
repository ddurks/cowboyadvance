//! Cowboy Advance — a cattle-herding demo (macroquad; desktop / web / iOS).
//!
//! The frame loop is deliberately thin: gather local input, step the sim,
//! draw. Everything interesting lives in `world` (simulation) and `input`
//! (control schemes).

use macroquad::prelude::*;

mod camera;
mod config;
mod input;
mod level;
mod world;

use config::SKY_COLOR;
use input::{gather_input, Joystick};
use world::World;

fn window_conf() -> Conf {
    Conf {
        window_title: "Cowboy Advance".to_owned(),
        window_width: 1280,
        window_height: 720,
        high_dpi: true,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let mut world = World::new();
    let mut joystick = Joystick::new();

    loop {
        // Clamp dt so a hitch (or a backgrounded tab) can't teleport entities.
        let dt = get_frame_time().min(0.05);

        let local_input = gather_input(&mut joystick);
        world.update(dt, &[local_input]);

        clear_background(SKY_COLOR);

        set_camera(&world.camera());
        world.draw_3d();

        set_default_camera();
        world.draw_hud();
        joystick.draw();

        next_frame().await;
    }
}
