//! Orthographic camera setup.
//!
//! The camera looks down at the field at an angle so the placeholder cubes
//! read as 3D volumes rather than flat squares, but uses an *orthographic*
//! projection (no perspective foreshortening) for that clean, readable,
//! toy-diorama look.

use macroquad::prelude::*;

use crate::config::*;

/// Build the game camera aimed at `target_xz` (a point on the ground plane).
pub fn make_camera(target_xz: Vec2) -> Camera3D {
    let target = vec3(target_xz.x, 0.0, target_xz.y);
    Camera3D {
        position: target + vec3(0.0, CAM_HEIGHT, CAM_BACK),
        target,
        up: vec3(0.0, 1.0, 0.0),
        // For an orthographic projection macroquad uses `fovy` as the world-space
        // height of the view volume — our "zoom".
        fovy: CAM_ORTHO_HEIGHT,
        aspect: None, // derive from window aspect ratio
        projection: Projection::Orthographics,
        render_target: None,
        viewport: None,
        z_near: 0.1,
        z_far: 1000.0,
    }
}
