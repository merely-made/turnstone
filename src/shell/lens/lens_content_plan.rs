// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(in super::super) fn lens_content_plan(
    app: &crate::app::App,
    contents: &super::super::NodeDocuments,
    ordinal: usize,
    w: u32,
    h: u32,
) -> Vec<crate::surface::Surface> {
    let frame = super::super::outer_workspace::frame_body_geometry(
        app,
        crate::action::SpaceRef::Lens(ordinal),
        (w.max(1), h.max(1)),
    );
    lens_content_plan_with_rects(
        app,
        contents,
        ordinal,
        w,
        h,
        frame.as_ref().map(|(b, _)| b.as_slice()),
        frame
            .as_ref()
            .map(|(_, f)| f.as_slice())
            .unwrap_or_default(),
    )
}
