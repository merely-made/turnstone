// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Canvas operations shared by Graphshell's browser presentation paths.
//!
//! Controls and keyboard bindings choose their own distances. The canvas owns
//! camera and physics state; this seam does not read a DOM or duplicate it.

use mere::canvas::{CameraView, Canvas};

/// A presentation command that does not change graph authority.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CanvasCommand {
    Pan {
        dx: f32,
        dy: f32,
    },
    /// Zoom about the centre of the canvas's logical viewport.
    Zoom {
        delta: f32,
    },
    /// Set the zoom exactly, about the same centre: what a receipt uses to
    /// visit chosen zooms. The canvas clamps it to its range.
    SetZoom {
        zoom: f32,
    },
    Fit,
    /// One-shot framing, without enabling layout following or restoring placement.
    FitVisible,
    FitSelection,
    RestoreArrangement,
    SetPhysicsPaused(bool),
    TogglePhysics,
}

impl CanvasCommand {
    pub fn apply(self, canvas: &mut Canvas, viewport: (u32, u32)) {
        match self {
            Self::Pan { dx, dy } => {
                canvas.wheel(dx, dy);
            },
            Self::Zoom { delta } => {
                canvas.cursor_moved(viewport.0 as f32 * 0.5, viewport.1 as f32 * 0.5);
                canvas.set_ctrl(true);
                canvas.wheel(0.0, delta);
                canvas.set_ctrl(false);
            },
            Self::SetZoom { zoom } => {
                // Both pages' cameras are top-down: screen = world * zoom + offset.
                let before = canvas.camera();
                canvas.set_camera(CameraView { zoom, ..before });
                let scale = canvas.camera().zoom / before.zoom;
                let centre = (viewport.0 as f32 * 0.5, viewport.1 as f32 * 0.5);
                canvas.set_camera(CameraView {
                    offset: (
                        centre.0 - (centre.0 - before.offset.0) * scale,
                        centre.1 - (centre.1 - before.offset.1) * scale,
                    ),
                    zoom: canvas.camera().zoom,
                });
            },
            // Fitting resumes following the layout; a pan or zoom (both reach
            // the canvas's wheel) or a node drag stops it.
            Self::Fit => {
                canvas.fit_to_content();
                canvas.set_view_follow(true);
            },
            Self::FitVisible => {
                canvas.fit_visible();
            },
            Self::FitSelection => {
                canvas.fit_selection();
            },
            Self::RestoreArrangement => {
                canvas.restore_arrangement();
            },
            Self::SetPhysicsPaused(paused) => canvas.set_physics_paused(paused),
            // An arrangement can pause the canvas independently of the UI.
            Self::TogglePhysics => canvas.set_physics_paused(!canvas.physics_paused()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_zoom_lands_exactly_and_keeps_the_centre_world_point() {
        let mut canvas = Canvas::new();
        canvas.set_camera(CameraView {
            offset: (130.0, -40.0),
            zoom: 1.3,
        });
        let viewport = (800, 600);
        let world = |c: &Canvas| {
            let v = c.camera();
            ((400.0 - v.offset.0) / v.zoom, (300.0 - v.offset.1) / v.zoom)
        };
        let before = world(&canvas);
        for zoom in [1.0, 0.75, 0.5, 0.25, 0.1, 2.0] {
            CanvasCommand::SetZoom { zoom }.apply(&mut canvas, viewport);
            assert_eq!(canvas.camera().zoom, zoom);
            let after = world(&canvas);
            assert!((after.0 - before.0).abs() < 1e-3 && (after.1 - before.1).abs() < 1e-3);
        }
        CanvasCommand::SetZoom { zoom: 0.01 }.apply(&mut canvas, viewport);
        assert_eq!(canvas.camera().zoom, 0.1, "the canvas's range clamps it");
    }
}
