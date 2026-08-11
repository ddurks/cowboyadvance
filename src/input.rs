//! Input gathering: keyboard + a stationary virtual joystick (touch on
//! mobile/web, mouse-drag on desktop), collapsed into one `move_dir` per player
//! and written to `PlayerInputs`. The sim never sees where input came from.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::cabinet::CabinetLayout;
use crate::components::*;

fn in_activation(p: Vec2, base: Vec2, radius: f32) -> bool {
    (p - base).length() < radius * 2.4
}

pub fn gather_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    windows: Query<&Window, With<PrimaryWindow>>,
    layout: Res<CabinetLayout>,
    mut joystick: ResMut<JoystickState>,
    mut inputs: ResMut<PlayerInputs>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    // The arcade stick lives in the cabinet's control area.
    let base = layout.stick_center;
    let radius = layout.stick_radius;
    if radius <= 0.0 {
        return; // layout not computed yet (first frame)
    }

    // --- Resolve the current pointer (touch preferred, then mouse) -------
    let mut pressed = false;
    let mut ppos = Vec2::ZERO;

    if let Some(id) = joystick.touch_id {
        if let Some(t) = touches.iter().find(|t| t.id() == id) {
            pressed = true;
            ppos = t.position();
        }
    } else if joystick.mouse_tracking {
        if mouse.pressed(MouseButton::Left) {
            if let Some(c) = window.cursor_position() {
                pressed = true;
                ppos = c;
            }
        }
    } else {
        // Not tracking: look for a fresh press near the base.
        if let Some(t) = touches
            .iter_just_pressed()
            .find(|t| in_activation(t.position(), base, radius))
        {
            joystick.touch_id = Some(t.id());
            pressed = true;
            ppos = t.position();
        } else if mouse.just_pressed(MouseButton::Left) {
            if let Some(c) = window.cursor_position() {
                if in_activation(c, base, radius) {
                    joystick.mouse_tracking = true;
                    pressed = true;
                    ppos = c;
                }
            }
        }
    }

    // --- Joystick vector -------------------------------------------------
    let joy_dir = if pressed {
        joystick.active = true;
        let offset = (ppos - base).clamp_length_max(radius);
        joystick.knob = offset;
        // Screen y is down, so negate to make "up" = forward.
        Vec2::new(offset.x / radius, -offset.y / radius)
    } else {
        joystick.active = false;
        joystick.touch_id = None;
        joystick.mouse_tracking = false;
        joystick.knob = Vec2::ZERO;
        Vec2::ZERO
    };

    // --- Keyboard (wins if pressed) --------------------------------------
    let mut k = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
        k.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
        k.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
        k.x += 1.0;
    }
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        k.x -= 1.0;
    }

    let move_dir = if k.length_squared() > 0.0 {
        k.normalize()
    } else {
        joy_dir
    };

    if inputs.0.is_empty() {
        inputs.0.push(Vec2::ZERO);
    }
    inputs.0[0] = move_dir;
}
