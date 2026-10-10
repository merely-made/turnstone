// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! How the plain viewer's canvas shares the page's gestures (mer3ly site
//! canvas plan, P2 parity with the repository sandbox).
//!
//! An embedded canvas must not trap the page: the wheel scrolls the page
//! unless Ctrl or Meta is held, when it zooms at the pointer. Touch splits by
//! fingers: one finger on empty canvas is the page's scroll, one finger on a
//! node drags it, and two fingers pan and pinch-zoom the camera. Reduced
//! motion moves the camera at once rather than gliding. This module is the
//! policy and the camera arithmetic; the browser listeners are the page's.

use mere::canvas::{CameraView, Canvas};

/// What a wheel notch over the graph is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelUse {
    /// The page scrolls; the canvas leaves the notch alone.
    Page,
    /// The canvas zooms at the pointer.
    Zoom,
}

/// A plain wheel belongs to the page; Ctrl or Meta claims it for zoom. A
/// trackpad pinch arrives as a Ctrl wheel, so it zooms too.
pub fn wheel_use(ctrl: bool, meta: bool) -> WheelUse {
    if ctrl || meta {
        WheelUse::Zoom
    } else {
        WheelUse::Page
    }
}

/// Zoom by a wheel notch `dy` (logical px) about canvas-local `at`, through
/// the canvas's own cursor-anchored zoom.
pub fn wheel_zoom(canvas: &mut Canvas, at: (f32, f32), dy: f32) {
    canvas.cursor_moved(at.0, at.1);
    canvas.set_ctrl(true);
    canvas.wheel(0.0, dy);
    canvas.set_ctrl(false);
}

/// What a touch that starts on the graph is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchUse {
    /// One finger on empty canvas: the page scrolls, the canvas sees nothing.
    Page,
    /// One finger on a node: the canvas drags it.
    Drag,
    /// A second finger: the two pan and pinch.
    TwoFinger,
}

/// Decide a touch from how many fingers are down, this one included, and
/// whether it landed on a node.
pub fn touch_use(fingers: usize, on_node: bool) -> TouchUse {
    match (fingers, on_node) {
        (0 | 1, false) => TouchUse::Page,
        (0 | 1, true) => TouchUse::Drag,
        _ => TouchUse::TwoFinger,
    }
}

/// Two touch points, canvas-local px.
pub type Fingers = [(f32, f32); 2];

fn centre(fingers: Fingers) -> (f32, f32) {
    (
        (fingers[0].0 + fingers[1].0) * 0.5,
        (fingers[0].1 + fingers[1].1) * 0.5,
    )
}

fn spread(fingers: Fingers) -> f32 {
    (fingers[0].0 - fingers[1].0).hypot(fingers[0].1 - fingers[1].1)
}

/// Move the camera as two fingers moved from `from` to `to`: the world point
/// under their old centre goes under their new one, scaled by how far they
/// spread. The canvas clamps the zoom, and the pan follows the clamped zoom.
pub fn two_finger(canvas: &mut Canvas, from: Fingers, to: Fingers) {
    let before = canvas.camera();
    let (old_spread, new_spread) = (spread(from), spread(to));
    let scale = if old_spread > 1.0 && new_spread > 1.0 {
        new_spread / old_spread
    } else {
        1.0
    };
    canvas.set_camera(CameraView {
        zoom: before.zoom * scale,
        ..before
    });
    let zoom = canvas.camera().zoom;
    let (old_centre, new_centre) = (centre(from), centre(to));
    let world = (
        (old_centre.0 - before.offset.0) / before.zoom,
        (old_centre.1 - before.offset.1) / before.zoom,
    );
    canvas.set_camera(CameraView {
        offset: (new_centre.0 - world.0 * zoom, new_centre.1 - world.1 * zoom),
        zoom,
    });
    // A gesture is the reader's camera now, as a wheel pan or zoom is.
    canvas.set_view_follow(false);
}

/// How far one keyboard pan step travels once its glide is done: the canvas
/// adds a step to its pan velocity and decays it by 0.85 a frame
/// (pictograph's `PAN_DECAY`), so the glide covers `step / (1 - 0.85)`.
pub const PAN_GLIDE: f32 = 1.0 / (1.0 - 0.85);

/// Pan by a keyboard step `(dx, dy)` at once, covering the distance the glide
/// would: the reduced-motion pan.
pub fn pan_at_once(canvas: &mut Canvas, dx: f32, dy: f32) {
    let camera = canvas.camera();
    canvas.set_camera(CameraView {
        offset: (
            camera.offset.0 + dx * PAN_GLIDE,
            camera.offset.1 + dy * PAN_GLIDE,
        ),
        ..camera
    });
    canvas.set_view_follow(false);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_at(canvas: &Canvas, at: (f32, f32)) -> (f32, f32) {
        let view = canvas.camera();
        (
            (at.0 - view.offset.0) / view.zoom,
            (at.1 - view.offset.1) / view.zoom,
        )
    }

    fn close(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3
    }

    #[test]
    fn a_plain_wheel_is_the_pages_and_a_modified_one_zooms() {
        assert_eq!(wheel_use(false, false), WheelUse::Page);
        assert_eq!(wheel_use(true, false), WheelUse::Zoom);
        assert_eq!(wheel_use(false, true), WheelUse::Zoom);
    }

    #[test]
    fn a_wheel_zoom_keeps_the_point_under_the_pointer() {
        let mut canvas = Canvas::new();
        canvas.resize(800, 600);
        let at = (200.0, 150.0);
        let before = world_at(&canvas, at);
        let zoom = canvas.camera().zoom;
        wheel_zoom(&mut canvas, at, 40.0);
        assert!(canvas.camera().zoom > zoom, "wheeling up enlarges");
        assert!(close(world_at(&canvas, at), before));
    }

    #[test]
    fn one_finger_is_the_pages_unless_it_holds_a_node() {
        assert_eq!(touch_use(1, false), TouchUse::Page);
        assert_eq!(touch_use(1, true), TouchUse::Drag);
        assert_eq!(touch_use(2, false), TouchUse::TwoFinger);
        assert_eq!(touch_use(2, true), TouchUse::TwoFinger);
    }

    #[test]
    fn two_fingers_pan_and_pinch_about_their_centre() {
        let mut canvas = Canvas::new();
        canvas.set_camera(CameraView {
            offset: (130.0, -40.0),
            zoom: 1.0,
        });
        let from = [(300.0, 300.0), (400.0, 300.0)];
        let held = world_at(&canvas, (350.0, 300.0));
        // Spread to twice the distance and move the centre 30 px right.
        let to = [(280.0, 300.0), (480.0, 300.0)];
        two_finger(&mut canvas, from, to);
        assert!((canvas.camera().zoom - 2.0).abs() < 1e-4);
        assert!(close(world_at(&canvas, (380.0, 300.0)), held));
        // A drag without spreading pans only.
        let zoom = canvas.camera().zoom;
        let offset = canvas.camera().offset;
        two_finger(
            &mut canvas,
            [(0.0, 0.0), (100.0, 0.0)],
            [(10.0, 20.0), (110.0, 20.0)],
        );
        assert_eq!(canvas.camera().zoom, zoom);
        assert!(close(
            canvas.camera().offset,
            (offset.0 + 10.0, offset.1 + 20.0)
        ));
    }

    #[test]
    fn a_pinch_past_the_range_keeps_the_held_point() {
        let mut canvas = Canvas::new();
        let from = [(100.0, 100.0), (110.0, 100.0)];
        let held = world_at(&canvas, (105.0, 100.0));
        two_finger(&mut canvas, from, [(0.0, 100.0), (1000.0, 100.0)]);
        let zoom = canvas.camera().zoom;
        assert!(zoom < 100.0, "the canvas clamps the zoom");
        assert!(close(world_at(&canvas, (500.0, 100.0)), held));
    }

    #[test]
    fn a_pan_at_once_covers_the_glide() {
        let mut canvas = Canvas::new();
        let before = canvas.camera().offset;
        pan_at_once(&mut canvas, -42.0, 0.0);
        let after = canvas.camera().offset;
        assert!((after.0 - (before.0 - 42.0 * PAN_GLIDE)).abs() < 1e-3);
        assert_eq!(after.1, before.1);
    }
}
