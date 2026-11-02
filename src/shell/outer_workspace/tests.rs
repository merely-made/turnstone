// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn unscoped_selector_uses_a_standalone_panes_retained_route() {
    let pane = PaneId(7);
    let mut roster = crate::cambium_pane::RosterGrid::new();
    roster.sync(&crate::app::App::test_stub(), pane, 512.0, 600.0);
    roster.scene(512, 600, 1.0);
    assert!(
        roster.inline_mount().is_none(),
        "fixture is not mounted inline"
    );
    let mut renderers = super::super::renderers::PaneRenderers::default();
    renderers.roster.insert(pane, roster);
    let selector = taproot::Selector::class("tab").containing("Nodes");
    let family = pane_selector_family(Some(&crate::panes::PaneContent::Roster));
    assert!(selector.matches_surface(family));
    assert!(
        ordinary_pane_selector(&renderers, pane, family, &selector)
            .expect("current retained geometry")
            .is_some(),
        "the Shell's unscoped route must see standalone pane paint"
    );
    assert_eq!(
        pane_selector_family(Some(&crate::panes::PaneContent::Workbench)),
        "workbench"
    );
    assert_eq!(
        pane_selector_family(Some(&crate::panes::PaneContent::Inspector)),
        "inspector"
    );
}

#[test]
fn outer_capture_retirement_is_window_scoped_and_terminal() {
    let mut tab = Some(OuterCapture::Tab {
        slot: 2,
        id: TileId(7),
        start: (20.0, 30.0),
        session: uuid::Uuid::nil(),
        appearance: Default::default(),
    });
    assert!(!retire_outer_capture(&mut tab, 1));
    assert!(tab.is_some(), "another window retains its gesture");
    assert!(retire_outer_capture(&mut tab, 2));
    assert!(
        tab.take().is_none(),
        "a late Up cannot revive withdrawn custody"
    );
    assert!(!retire_outer_capture(&mut tab, 2));

    let mut divider = Some(OuterCapture::Divider {
        slot: 0,
        path: TilePath(Vec::new()),
        index: 0,
        area: Rect::new(0.0, 0.0, 200.0, 100.0),
        axis: workbench::SplitAxis::Row,
        shares: vec![0.5, 0.5],
        gaps: vec![10.0],
        session: uuid::Uuid::nil(),
        appearance: Default::default(),
    });
    assert!(!retire_outer_capture(&mut divider, 2));
    assert!(retire_outer_capture(&mut divider, 0));
    assert!(divider.is_none());

    let mut floating = Some(OuterCapture::Float {
        slot: 3,
        id: TileId(9),
        start: (20.0, 30.0),
        rect: RelativeRect {
            x: 0.1,
            y: 0.1,
            width: 0.5,
            height: 0.5,
        },
        size: (400, 300),
        session: uuid::Uuid::nil(),
        appearance: Default::default(),
    });
    assert!(!retire_outer_capture(&mut floating, 2));
    assert!(
        matches!(
            floating.as_ref(),
            Some(OuterCapture::Float {
                slot: 3,
                id: TileId(9),
                ..
            })
        ),
        "another window retains its floating gesture"
    );
    assert!(retire_outer_capture(&mut floating, 3));
    assert!(
        floating.take().is_none(),
        "a late Up cannot revive withdrawn floating custody"
    );
    assert!(!retire_outer_capture(&mut floating, 3));
}
