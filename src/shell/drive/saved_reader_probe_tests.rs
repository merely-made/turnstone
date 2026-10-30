// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
#[test]
fn saved_reader_probe_follows_the_focused_workbenchs_own_member() {
    let mut app = crate::app::App::test_stub();
    app.update(Action::OpenAddress("mere://reader".into()));
    let node = app.graph_runtimes.focused_member().unwrap();
    app.update(Action::OpenInWorkbench);
    let crate::surface::FocusTarget::Pane(pane) = app.focus else {
        panic!("Workbench has pane focus")
    };
    assert_eq!(
        focused_saved_reader_node_for(&app, |member| member == node),
        Some(node)
    );
    app.publish_member_context(pane, None);
    assert_eq!(
        focused_saved_reader_node_for(&app, |member| member == node),
        None,
        "a reader elsewhere is not focused"
    );
    app.publish_member_context(pane, Some(uuid::Uuid::new_v4()));
    assert_eq!(
        focused_saved_reader_node_for(&app, |member| member == node),
        None,
        "another member is not a saved reader"
    );
    app.focus = crate::surface::FocusTarget::Chrome;
    assert_eq!(
        focused_saved_reader_node_for(&app, |member| member == node),
        None
    );
}
