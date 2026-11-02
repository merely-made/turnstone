// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn workbench_resize_band_precedes_its_document_but_respects_other_appearances() {
    use crate::surface::{Surface, SurfaceId, SurfaceKind};
    let mut app = crate::app::App::test_stub();
    app.update(Action::OpenAddress("https://example.com/left".into()));
    app.update(Action::OpenInWorkbench);
    app.update(Action::OpenAddress("https://example.com/right".into()));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::OpenInWorkbench);
    let pane = app.active_pane.unwrap();
    let plan = vec![Surface {
        id: SurfaceId::pane(pane),
        kind: SurfaceKind::Pane(pane),
        rect: Rect::new(20.0, 40.0, 800.0, 600.0),
    }];
    let hit = crate::surface::HitResult {
        id: SurfaceId::content_appearance(
            node,
            crate::surface::appearance::workbench_appearance_role(pane, node),
        ),
        kind: SurfaceKind::Content(node),
        local: (3.0, 100.0),
    };
    assert!(
        workbench_resize_target(&app, &plan, Some(hit), 424.0, 200.0).is_some(),
        "four pixels into the document still grabs the seam"
    );
    let covered = crate::surface::HitResult {
        id: SurfaceId::content_appearance(node, 99),
        ..hit
    };
    assert!(
        workbench_resize_target(&app, &plan, Some(covered), 424.0, 200.0).is_none(),
        "another appearance covering the seam keeps its input"
    );
    assert!(workbench_resize_target(&app, &plan, Some(hit), 430.0, 200.0).is_none());
}

#[test]
fn web_cursor_shapes_map_to_host_cursor_icons() {
    assert_eq!(
        surface_cursor_icon(inker::CursorShape::Pointer),
        Some(CursorIcon::Pointer)
    );
    assert_eq!(
        surface_cursor_icon(inker::CursorShape::ResizeNesw),
        Some(CursorIcon::NeswResize)
    );
    assert_eq!(surface_cursor_icon(inker::CursorShape::Hidden), None);
    assert_eq!(surface_cursor_label(inker::CursorShape::Pointer), "pointer");
}
