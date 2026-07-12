//! Input abstraction.
//!
//! Both control schemes collapse to the same thing: a `move_dir` vector where
//! `+x` is right and `+y` is "forward" (up the screen). The simulation never
//! learns whether that came from WASD, a touch joystick, or (eventually) the
//! network — which is exactly what makes 4-player coop a drop-in later.

use macroquad::prelude::*;

use crate::config::*;

/// One player's intent for a single frame.
#[derive(Clone, Copy, Default)]
pub struct PlayerInput {
    /// Movement direction. `+x` = right, `+y` = forward (up-screen). Length 0..=1.
    pub move_dir: Vec2,
}

/// A fixed (stationary) on-screen thumbstick anchored bottom-left.
///
/// Driven by touch on mobile/web, or by mouse-drag on desktop for testing.
/// The base never moves; the knob tracks the pointer, clamped to the ring.
pub struct Joystick {
    knob: Vec2,             // knob offset from base, in pixels
    active: bool,
    touch_id: Option<u64>,  // the touch we're currently tracking, if any
    mouse_tracking: bool,   // whether the mouse grabbed the stick
}

impl Joystick {
    pub fn new() -> Self {
        Self {
            knob: Vec2::ZERO,
            active: false,
            touch_id: None,
            mouse_tracking: false,
        }
    }

    /// Fixed anchor point of the stick, recomputed each frame in case the
    /// window (or device orientation) changed size.
    fn base(&self) -> Vec2 {
        vec2(
            JOYSTICK_MARGIN + JOYSTICK_RADIUS,
            screen_height() - JOYSTICK_MARGIN - JOYSTICK_RADIUS,
        )
    }

    /// You have to grab the stick near where it lives — keeps a stray tap on
    /// the far side of the screen from yanking the player around.
    fn in_activation(&self, p: Vec2) -> bool {
        (p - self.base()).length() < JOYSTICK_RADIUS * 2.6
    }

    /// Poll the pointer and return this frame's stick vector (length 0..=1).
    pub fn update(&mut self) -> Vec2 {
        let ts = touches();
        let mut pressed = false;
        let mut ppos = Vec2::ZERO;

        if let Some(id) = self.touch_id {
            // Continue tracking an in-progress touch.
            if let Some(t) = ts.iter().find(|t| t.id == id) {
                match t.phase {
                    TouchPhase::Ended | TouchPhase::Cancelled => pressed = false,
                    _ => {
                        pressed = true;
                        ppos = t.position;
                    }
                }
            }
        } else if self.mouse_tracking {
            if is_mouse_button_down(MouseButton::Left) {
                pressed = true;
                ppos = mouse_position().into();
            }
        } else {
            // Not tracking anything yet: look for a fresh press near the stick.
            if let Some(t) = ts
                .iter()
                .find(|t| matches!(t.phase, TouchPhase::Started) && self.in_activation(t.position))
            {
                self.touch_id = Some(t.id);
                pressed = true;
                ppos = t.position;
            } else if is_mouse_button_pressed(MouseButton::Left) {
                let mp: Vec2 = mouse_position().into();
                if self.in_activation(mp) {
                    self.mouse_tracking = true;
                    pressed = true;
                    ppos = mp;
                }
            }
        }

        if pressed {
            self.active = true;
            let offset = (ppos - self.base()).clamp_length_max(JOYSTICK_RADIUS);
            self.knob = offset;
            // Screen y grows downward, so negate to make "up" = forward.
            vec2(offset.x / JOYSTICK_RADIUS, -offset.y / JOYSTICK_RADIUS)
        } else {
            self.active = false;
            self.touch_id = None;
            self.mouse_tracking = false;
            self.knob = Vec2::ZERO;
            Vec2::ZERO
        }
    }

    /// Draw the stick as a translucent 2D overlay (call under the default camera).
    pub fn draw(&self) {
        let base = self.base();
        let ring = if self.active { 0.45 } else { 0.30 };
        draw_circle(base.x, base.y, JOYSTICK_RADIUS, Color::new(1.0, 1.0, 1.0, 0.08));
        draw_circle_lines(base.x, base.y, JOYSTICK_RADIUS, 3.0, Color::new(1.0, 1.0, 1.0, ring));
        let knob = base + self.knob;
        draw_circle(knob.x, knob.y, JOYSTICK_RADIUS * 0.45, Color::new(1.0, 1.0, 1.0, 0.55));
    }
}

/// Gather this frame's local player input: keyboard wins if pressed, otherwise
/// fall back to the joystick.
pub fn gather_input(joystick: &mut Joystick) -> PlayerInput {
    let mut k = Vec2::ZERO;
    if is_key_down(KeyCode::W) || is_key_down(KeyCode::Up) {
        k.y += 1.0;
    }
    if is_key_down(KeyCode::S) || is_key_down(KeyCode::Down) {
        k.y -= 1.0;
    }
    if is_key_down(KeyCode::D) || is_key_down(KeyCode::Right) {
        k.x += 1.0;
    }
    if is_key_down(KeyCode::A) || is_key_down(KeyCode::Left) {
        k.x -= 1.0;
    }

    // Always poll the stick so its knob animates even when keyboard is active.
    let joy = joystick.update();
    let move_dir = if k.length_squared() > 0.0 { k.normalize() } else { joy };

    PlayerInput { move_dir }
}
