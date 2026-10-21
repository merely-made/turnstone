// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The local viewer's synchronous command receiver, shared by Mesquite and CPU
//! receiving tests. Current offers and canvas ownership remain host decisions.

use crate::viewer_commands::{self, Action};
use cambium::{CommandSet, Invocation, Outcome, catalogue::ids};
use mere::canvas::{Canvas, camera_host::CameraHost, controls::CanvasCommand};

/// This host's zero-argument viewer schema. Other hosts own their schemas.
pub const SCHEMA: &str = "graphshell.viewer/v1";

/// Invoke the supported synchronous subset against freshly composed offers.
///
/// A retired offer is stale. Invalid envelopes, disabled offers and operations
/// without an observed synchronous completion path are refused before dispatch.
/// This receiver never forwards commands to a resident or remote owner; callers
/// must first establish that this is the currently presented local canvas.
pub fn invoke(
    canvas: &mut Canvas,
    viewport: (u32, u32),
    declarations: &CommandSet,
    invocation: &Invocation,
) -> Outcome {
    let refused = |reason: &str| Outcome::Refused {
        reason: reason.into(),
    };
    let Some(declaration) = declarations.get(&invocation.id) else {
        return Outcome::Stale;
    };
    if invocation.schema_id != SCHEMA
        || invocation.effect != declaration.effect
        || !invocation.arguments.is_empty()
    {
        return refused("Viewer command does not match its current contract.");
    }
    if let Some(reason) = &declaration.disabled_reason {
        return refused(reason);
    }
    if !matches!(
        invocation.id.as_str(),
        ids::VIEW_ZOOM_IN | ids::VIEW_ZOOM_OUT | ids::PHYSICS_TOGGLE
    ) {
        return refused("This viewer command has no synchronous invocation receiver.");
    }
    let Some(Action::Canvas(command)) = viewer_commands::action(&invocation.id, false) else {
        return refused("This viewer command has no canvas operation.");
    };
    let before = (canvas.camera(), canvas.physics_paused());
    if let CanvasCommand::Zoom { delta } = command {
        let (low, high) = canvas.zoom_range();
        if (delta > 0.0 && before.0.zoom >= high) || (delta < 0.0 && before.0.zoom <= low) {
            // Do not stop layout following for an operation that cannot zoom.
            return refused("The viewer is already at its zoom limit.");
        }
    }
    command.apply(canvas, viewport);
    let completed = match command {
        CanvasCommand::Zoom { .. } => before.0.zoom != canvas.camera().zoom,
        CanvasCommand::TogglePhysics => before.1 != canvas.physics_paused(),
        _ => false,
    };
    if completed {
        Outcome::Done
    } else {
        refused("The canvas did not establish a completed viewer change.")
    }
}

#[cfg(test)]
mod tests;
