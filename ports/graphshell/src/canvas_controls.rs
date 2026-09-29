// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Canvas operations shared by Graphshell's browser presentation paths.
//!
//! Controls and keyboard bindings choose their own distances. The canvas owns
//! camera and physics state; this seam does not read a DOM or duplicate it.

use mere::canvas::Canvas;

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
    Fit,
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
            Self::Fit => canvas.fit_to_content(),
            Self::RestoreArrangement => {
                canvas.restore_arrangement();
            },
            Self::SetPhysicsPaused(paused) => canvas.set_physics_paused(paused),
            // An arrangement can pause the canvas independently of the UI.
            Self::TogglePhysics => canvas.set_physics_paused(!canvas.physics_paused()),
        }
    }
}
