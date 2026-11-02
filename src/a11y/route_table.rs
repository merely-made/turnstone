// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

#[test]
fn every_graph_member_is_reachable_by_route_when_an_orrery_exists() {
    let app = App::projection_fixture();
    let (_, routes) = project_app_with_routes(&app);
    let members: Vec<_> = app
        .graph_runtimes
        .graph()
        .nodes()
        .map(|(_, node)| node.id)
        .collect();
    assert!(!members.is_empty(), "the stub graph has members");
    for member in members {
        let id = node_id_for_path(&format!("turnstone/frozen-projection/instance/{member}"));
        match routes.get(&id) {
            Some(A11yRoute::SelectMember { member: routed, .. }) => {
                assert_eq!(*routed, member)
            },
            other => panic!("member {member} routed to {other:?}"),
        }
    }
    assert!(matches!(
        routes.get(&node_id_for_path("turnstone/chrome/omnibar")),
        Some(A11yRoute::OpenOmnibar)
    ));
}

#[test]
fn a_routed_selection_changes_the_app_and_a_miss_says_so() {
    let mut app = App::projection_fixture();
    let (_, routes) = project_app_with_routes(&app);
    let member = app
        .graph_runtimes
        .graph()
        .nodes()
        .map(|(_, node)| node.id)
        .next()
        .expect("a member");
    let id = node_id_for_path(&format!("turnstone/frozen-projection/instance/{member}"));

    let effects = apply_route(&mut app, routes.get(&id), id);
    assert!(!effects.is_empty(), "a landed selection redraws");

    // The miss is loud and attributable, exactly like a pointer miss.
    let ghost = node_id_for_path("turnstone/frozen-projection/instance/ghost");
    let effects = apply_route(&mut app, None, ghost);
    assert!(effects.is_empty());
    let described: Vec<String> = app
        .take_events()
        .iter()
        .map(crate::observe::AppEvent::describe)
        .collect();
    assert!(
        described
            .iter()
            .any(|line| line.starts_with("interaction-missed a11y-action")),
        "the miss vanished: {described:?}"
    );
}

#[test]
fn an_omnibar_route_opens_the_omnibar_through_the_spine() {
    let mut app = App::test_stub();
    assert!(!app.omnibar.open);
    apply_route(
        &mut app,
        Some(&A11yRoute::OpenOmnibar),
        node_id_for_path("turnstone/chrome/omnibar"),
    );
    assert!(app.omnibar.open, "the same spine a keypress uses");
}
