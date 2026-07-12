//! The retro cabinet frame around the game screen.
//!
//! Everything is laid out deterministically from the window size into a
//! `CabinetLayout` (window-pixel rects), so the visuals and the analog-stick
//! touch region share one source of truth. The 3D game is shown as an
//! `ImageNode` in the middle; a marquee sits on top and the controls on the
//! bottom. On wide screens the cabinet is centred with "room" gutters.

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

/// Computed cabinet geometry for the current frame.
#[derive(Resource, Default)]
pub struct CabinetLayout {
    pub cab: RectPx,
    pub bezel: RectPx,
    pub screen: RectPx,
    pub title: RectPx, // strip inside the bezel, below the screen
    pub controls: RectPx,
    pub stick_center: Vec2,
    pub stick_radius: f32,
    pub knob_radius: f32,
    pub btn_a: Vec2,
    pub btn_b: Vec2,
    pub btn_radius: f32,
    pub hud: Vec2,
}

/// Which cabinet element a node is, so one layout query can place them all.
#[derive(Component, Clone, Copy)]
pub enum CabElem {
    Body,
    Bezel,
    Screen,
    Title,
    Controls,
    StickBase,
    Knob,
    ButtonA,
    ButtonB,
    Hud,
}

/// Fit-and-place the cabinet within a `w`×`h` window (logical px).
pub fn compute_layout(w: f32, h: f32) -> CabinetLayout {
    // Contain-fit a portrait cabinet: fill height, but never overflow width.
    let cab_h = h.min(w / CAB_AR);
    let cab_w = cab_h * CAB_AR;
    let cab_x = (w - cab_w) * 0.5;
    let cab_y = (h - cab_h) * 0.5;

    let pad = cab_w * 0.045;
    let gap = cab_h * 0.018;

    // Controls at the bottom; the bezel fills everything above them (no marquee,
    // so the screen gets as much room as possible).
    let controls_h = cab_h * 0.24;
    let controls = RectPx {
        x: cab_x + pad,
        y: cab_y + cab_h - pad - controls_h,
        w: cab_w - 2.0 * pad,
        h: controls_h,
    };
    let bezel = RectPx {
        x: cab_x + pad,
        y: cab_y + pad,
        w: cab_w - 2.0 * pad,
        h: (controls.y - gap) - (cab_y + pad),
    };

    // Inside the bezel: screen on top, "COWBOY ADVANCE" strip below it.
    let bez_pad = cab_w * 0.03;
    let title_h = cab_h * 0.05;
    let inner_w = bezel.w - 2.0 * bez_pad;
    let screen_avail_h = (bezel.h - 2.0 * bez_pad - title_h).max(1.0);
    let sar = SCREEN_RES_W as f32 / SCREEN_RES_H as f32;
    let mut sh = screen_avail_h;
    let mut sw = sh * sar;
    if sw > inner_w {
        sw = inner_w;
        sh = sw / sar;
    }
    let sx = bezel.x + (bezel.w - sw) * 0.5;
    let sy = bezel.y + bez_pad;
    let screen = RectPx { x: sx, y: sy, w: sw, h: sh };

    // Title fills the leftover strip between the screen bottom and the bezel.
    let strip_top = sy + sh;
    let strip_bottom = bezel.y + bezel.h - bez_pad;
    let title = RectPx {
        x: bezel.x,
        y: strip_top,
        w: bezel.w,
        h: (strip_bottom - strip_top).max(1.0),
    };

    let cy = controls.y + controls.h * 0.5;
    let stick_center = Vec2::new(cab_x + cab_w * 0.27, cy);
    let stick_radius = (controls.h * 0.40).min(cab_w * 0.15);
    let knob_radius = stick_radius * 0.52;
    let btn_radius = cab_w * 0.078;
    let btn_a = Vec2::new(cab_x + cab_w * 0.80, cy - controls.h * 0.10);
    let btn_b = Vec2::new(cab_x + cab_w * 0.62, cy + controls.h * 0.10);

    let hud = Vec2::new(sx + sw * 0.05, sy + sh * 0.03);

    CabinetLayout {
        cab: RectPx { x: cab_x, y: cab_y, w: cab_w, h: cab_h },
        bezel,
        screen,
        title,
        controls,
        stick_center,
        stick_radius,
        knob_radius,
        btn_a,
        btn_b,
        btn_radius,
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

/// Position every cabinet node from the computed layout (+ current stick knob).
pub fn apply_cabinet_layout(
    layout: Res<CabinetLayout>,
    joystick: Res<JoystickState>,
    mut q: Query<(&mut Node, &CabElem)>,
) {
    let l = &*layout;
    for (mut node, elem) in &mut q {
        match elem {
            CabElem::Body => place(&mut node, l.cab),
            CabElem::Bezel => place(&mut node, l.bezel),
            CabElem::Screen => place(&mut node, l.screen),
            CabElem::Title => place(&mut node, l.title),
            CabElem::Controls => place(&mut node, l.controls),
            CabElem::StickBase => place_circle(&mut node, l.stick_center, l.stick_radius),
            CabElem::Knob => place_circle(&mut node, l.stick_center + joystick.knob, l.knob_radius),
            CabElem::ButtonA => place_circle(&mut node, l.btn_a, l.btn_radius),
            CabElem::ButtonB => place_circle(&mut node, l.btn_b, l.btn_radius),
            CabElem::Hud => {
                node.left = Val::Px(l.hud.x);
                node.top = Val::Px(l.hud.y);
            }
        }
    }
}

/// Spawn all cabinet UI as children of one root, so stacking is well-defined
/// (later children draw on top). Called from `setup`.
pub fn spawn_cabinet(commands: &mut Commands, ui_camera: Entity, screen_image: Handle<Image>) {
    let abs = || Node { position_type: PositionType::Absolute, ..default() };

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
            // Cabinet body (back).
            c.spawn((
                Node { border_radius: BorderRadius::all(Val::Px(24.0)), ..abs() },
                BackgroundColor(srgb3(BODY_COLOR)),
                CabElem::Body,
            ));

            // Controls panel.
            c.spawn((
                Node { border_radius: BorderRadius::all(Val::Px(16.0)), ..abs() },
                BackgroundColor(srgb3(CONTROLS_COLOR)),
                CabElem::Controls,
            ));

            // Bezel (behind the screen; also holds the title strip).
            c.spawn((
                Node { border_radius: BorderRadius::all(Val::Px(12.0)), ..abs() },
                BackgroundColor(srgb3(BEZEL_COLOR)),
                CabElem::Bezel,
            ));

            // The game screen (render-target image). Dark backing shows if the
            // texture ever fails to bind.
            c.spawn((
                ImageNode::new(screen_image),
                Node { border_radius: BorderRadius::all(Val::Px(6.0)), ..abs() },
                BackgroundColor(srgb3(SKY_COLOR)),
                CabElem::Screen,
            ));

            // "COWBOY ADVANCE" in the bezel strip below the screen.
            c.spawn((
                Node {
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    overflow: Overflow::clip(),
                    ..abs()
                },
                CabElem::Title,
            ))
            .with_children(|p| {
                p.spawn((
                    Text::new("COWBOY  ADVANCE"),
                    TextFont::from_font_size(18.0),
                    TextColor(srgb3(TITLE_COLOR)),
                ));
            });

            // HUD (penned counter) over the screen.
            c.spawn((
                Text::new("Penned: 0 / 24"),
                TextFont::from_font_size(20.0),
                TextColor(Color::WHITE),
                abs(),
                PenCounter,
                CabElem::Hud,
            ));

            // Arcade stick: dark base + red ball-top knob.
            c.spawn((
                Node { border_radius: BorderRadius::MAX, ..abs() },
                BackgroundColor(srgb3(STICK_BASE_COLOR)),
                CabElem::StickBase,
            ));
            c.spawn((
                Node { border_radius: BorderRadius::MAX, ..abs() },
                BackgroundColor(srgb3(STICK_BALL_COLOR)),
                CabElem::Knob,
            ));

            // A / B buttons.
            for (elem, color, label) in [
                (CabElem::ButtonB, BUTTON_B_COLOR, "B"),
                (CabElem::ButtonA, BUTTON_A_COLOR, "A"),
            ] {
                c.spawn((
                    Node {
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        border_radius: BorderRadius::MAX,
                        ..abs()
                    },
                    BackgroundColor(srgb3(color)),
                    elem,
                ))
                .with_children(|p| {
                    p.spawn((
                        Text::new(label),
                        TextFont::from_font_size(22.0),
                        TextColor(Color::srgb(1.0, 1.0, 0.95)),
                    ));
                });
            }
        });
}
