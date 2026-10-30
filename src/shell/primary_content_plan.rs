// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Primary plan shared by presentation, input, and portable host-plan controls.
pub(super) fn primary_content_plan(
    app: &App,
    contents: &NodeDocuments,
    width: u32,
    height: u32,
) -> Vec<crate::surface::Surface> {
    let frame = outer_workspace::frame_body_geometry(
        app,
        crate::action::SpaceRef::Primary,
        (width.max(1), height.max(1)),
    );
    primary_content_plan_with_rects(
        app,
        contents,
        width,
        height,
        frame.as_ref().map(|(b, _)| b.as_slice()),
        frame
            .as_ref()
            .map(|(_, f)| f.as_slice())
            .unwrap_or_default(),
    )
}
