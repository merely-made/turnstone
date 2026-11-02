// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Portable owner validation uses the same retained frame aperture calculation
/// as the live Shell, without booting a native host or another render core.
pub(in super::super) fn frame_body_geometry(
    app: &crate::app::App,
    space: SpaceRef,
    size: (u32, u32),
) -> Option<(Vec<(PaneId, Rect)>, Vec<(PaneId, Rect, Rect)>)> {
    if app.blueprint_space(space).is_some() {
        return None;
    }
    let layout = app.space(space)?;
    let mut frame = crate::outer_frame::OuterFrame::default();
    let slot = match space {
        SpaceRef::Primary => 0,
        SpaceRef::Lens(ordinal) => ordinal + 1,
    };
    frame.set_main_view_switch(space == SpaceRef::Primary);
    frame.set_layout_scale(app.shell_chrome_config().layout_scale(slot));
    frame.set_workspace_area(crate::chrome_view::workspace_area_for_window(
        app.shell_chrome_config(),
        slot,
        size.0,
        size.1,
    ));
    let docked = frame
        .prepare(
            layout,
            app.active_pane,
            if space == SpaceRef::Primary {
                app.maximized
            } else {
                None
            },
            size,
            &app.shell_chrome_config().appearance,
            app.session_id.as_uuid().as_u128(),
        )
        .unwrap_or_default();
    let floats = frame.floats();
    Some((docked, floats))
}
