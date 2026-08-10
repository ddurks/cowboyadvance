//! The HUD frame around the game screen.
//!
//! The bezel art is split into two pieces so it never distorts: a top wood strip
//! (`frame_top.png`) anchored to the top and a hump + bottom wood
//! (`frame_bottom.png`) anchored to the bottom, each drawn full cabinet width at
//! its own aspect. The 3D game renders straight into the flexible middle via a
//! camera viewport (see [`update_game_viewport`]), so resizing the cabinet only
//! stretches the scene, never the frame. The joystick knob (`joystick.png`,
//! trimmed to content) rides on the hump in the bottom piece.
//!
//! Everything is laid out deterministically from the window size into a
//! `CabinetLayout` (window-pixel rects), so the visuals and the analog-stick
//! touch region share one source of truth.

use bevy::camera::Viewport;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::components::*;
use crate::config::*;

/// A window-space rectangle in logical pixels.
#[derive(Clone, Copy, Default)]
pub struct RectPx {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Computed HUD geometry for the current frame.
#[derive(Resource, Default)]
pub struct CabinetLayout {
    pub frame_top: RectPx,
    pub frame_bottom: RectPx,
    /// The scene rect — the game camera's window viewport.
    pub screen: RectPx,
    pub stick_center: Vec2,
    pub stick_radius: f32,
    pub knob_radius: f32,
    pub shadow_radius: f32,
    pub hud: Vec2,
}

/// Which HUD element a node is, so one layout query can place them all.
#[derive(Component, Clone, Copy)]
pub enum CabElem {
    FrameTop,
    FrameBottom,
    StickShadow,
    Knob,
    Hud,
}

/// Fit-and-place the HUD within a `w`×`h` window (logical px).
pub fn compute_layout(w: f32, h: f32) -> CabinetLayout {
    // Fixed mobile-portrait cabinet: contain-fit `CABINET_AR` into the window and
    // centre it, so the viewport is identical on phone and desktop (black gutters
    // fill the rest on wide screens).
    let (cw, ch) = if w / h > CABINET_AR {
        (h * CABINET_AR, h) // window wider than the cabinet: limited by height
    } else {
        (w, w / CABINET_AR) // window narrower: limited by width
    };
    let cx = (w - cw) * 0.5;
    let cy = (h - ch) * 0.5;

    // Frame pieces keep their own shape (height = fraction of width), anchored to
    // the top and bottom edges of the cabinet.
    let top_h = cw * FRAME_TOP_H_FRAC;
    let bot_h = cw * FRAME_BOTTOM_H_FRAC;
    let frame_top = RectPx { x: cx, y: cy, w: cw, h: top_h };
    let bot_y = cy + ch - bot_h;
    let frame_bottom = RectPx { x: cx, y: bot_y, w: cw, h: bot_h };

    // The scene (the game viewport) fills from under the top wood strip all the
    // way to the cabinet bottom, so the game renders *behind* the bottom piece —
    // its transparent flanks reveal the field in the bottom corners, and only the
    // hump/wood cover it. The play boundary is contoured to match (see sim).
    let screen = RectPx { x: cx, y: cy + top_h, w: cw, h: (ch - top_h).max(1.0) };

    // Joystick on the hump, inside the bottom piece.
    let stick_center = Vec2::new(cx + cw * 0.5, bot_y + bot_h * STICK_Y_IN_BOTTOM_FRAC);
    let knob_radius = cw * KNOB_DIAM_FRAC * 0.5;
    let stick_radius = cw * STICK_RADIUS_FRAC;
    // Range ring under the knob: the disc the ball sweeps (travel + ball radius).
    let shadow_radius = stick_radius + knob_radius;

    // Penned counter: top-left of the scene, just under the top wood strip.
    let hud = Vec2::new(cx + cw * 0.04, cy + top_h + 8.0);

    CabinetLayout {
        frame_top,
        frame_bottom,
        screen,
        stick_center,
        stick_radius,
        knob_radius,
        shadow_radius,
        hud,
    }
}

/// Recompute the layout from the window each frame (runs before input + apply).
pub fn compute_cabinet_layout(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut layout: ResMut<CabinetLayout>,
) {
    let Ok(win) = windows.single() else {
        return;
    };
    *layout = compute_layout(win.width(), win.height());
}

/// Point the game camera at the cabinet's scene rect (in physical pixels).
pub fn update_game_viewport(
    layout: Res<CabinetLayout>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cam: Query<&mut Camera, With<GameCamera>>,
) {
    let Ok(win) = windows.single() else {
        return;
    };
    let Ok(mut camera) = cam.single_mut() else {
        return;
    };
    let s = win.scale_factor();
    let r = layout.screen;
    if r.w <= 0.0 || r.h <= 0.0 {
        return;
    }
    camera.viewport = Some(Viewport {
        physical_position: UVec2::new((r.x * s).max(0.0) as u32, (r.y * s).max(0.0) as u32),
        physical_size: UVec2::new((r.w * s) as u32, (r.h * s) as u32),
        ..default()
    });
}

fn place(node: &mut Node, r: RectPx) {
    node.left = Val::Px(r.x);
    node.top = Val::Px(r.y);
    node.width = Val::Px(r.w);
    node.height = Val::Px(r.h);
}

fn place_circle(node: &mut Node, center: Vec2, radius: f32) {
    place(
        node,
        RectPx { x: center.x - radius, y: center.y - radius, w: radius * 2.0, h: radius * 2.0 },
    );
}

/// Position every HUD node from the computed layout (+ current stick knob).
pub fn apply_cabinet_layout(
    layout: Res<CabinetLayout>,
    joystick: Res<JoystickState>,
    mut q: Query<(&mut Node, &CabElem)>,
) {
    let l = &*layout;
    for (mut node, elem) in &mut q {
        match elem {
            CabElem::FrameTop => place(&mut node, l.frame_top),
            CabElem::FrameBottom => place(&mut node, l.frame_bottom),
            // Range ring stays put at the stick centre; the knob rides on top.
            CabElem::StickShadow => place_circle(&mut node, l.stick_center, l.shadow_radius),
            CabElem::Knob => place_circle(&mut node, l.stick_center + joystick.knob, l.knob_radius),
            CabElem::Hud => {
                node.left = Val::Px(l.hud.x);
                node.top = Val::Px(l.hud.y);
            }
        }
    }
}

/// Spawn all HUD nodes as children of one root, so stacking is well-defined
/// (later children draw on top). Called from `setup`.
///
/// Draw order (back → front): frame pieces, penned counter, range ring, knob.
/// (The 3D game is drawn behind all of this by its own camera.)
pub fn spawn_cabinet(
    commands: &mut Commands,
    ui_camera: Entity,
    frame_top: Handle<Image>,
    frame_bottom: Handle<Image>,
    joystick_image: Handle<Image>,
) {
    let abs = || Node { position_type: PositionType::Absolute, ..default() };
    // Stretch the frame pieces to their (aspect-matched) nodes so they keep shape.
    let piece = |img: Handle<Image>, elem: CabElem| {
        (ImageNode { image_mode: NodeImageMode::Stretch, ..ImageNode::new(img) }, abs(), elem)
    };

    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            UiTargetCamera(ui_camera),
        ))
        .with_children(|c| {
            // Frame bezel pieces (transparent middles reveal the game viewport).
            c.spawn(piece(frame_top, CabElem::FrameTop));
            c.spawn(piece(frame_bottom, CabElem::FrameBottom));

            // HUD (penned counter) over the scene.
            c.spawn((
                Text::new("Penned: 0 / 24"),
                TextFont::from_font_size(20.0),
                TextColor(Color::WHITE),
                abs(),
                PenCounter,
                CabElem::Hud,
            ));

            // Range ring: a soft dark disc under the knob showing how far it
            // travels. Fixed at the stick centre; the knob rides on top of it.
            c.spawn((
                Node { border_radius: BorderRadius::MAX, ..abs() },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.28)),
                CabElem::StickShadow,
            ));

            // Joystick knob (ball-top art) — sits on the hump, drags to steer.
            c.spawn((ImageNode::new(joystick_image), abs(), CabElem::Knob));
        });
}
