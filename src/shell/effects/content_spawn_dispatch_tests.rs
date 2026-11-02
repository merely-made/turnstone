// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::{ContentSpawnDispatch, dispatch_content_spawn};

#[test]
fn graft_spawn_waits_for_host_in_browser_surface_queue() {
    let node = uuid::Uuid::new_v4();
    let url = "file:///browser-servo-dispatch.html";
    let decision = super::super::standard_route_policy().route(&inker::EngineRouteRequest {
        workspace_id: inker::WorkspaceRouteId::new("turnstone-test"),
        view: None,
        node: None,
        address: url.to_string(),
        content_type: Some("text/html".to_string()),
        pinned_engine: Some(inker::routing::ENGINE_GRAFT_SERVO.to_string()),
    });
    assert_eq!(decision.engine_id, inker::routing::ENGINE_GRAFT_SERVO);
    assert!(!super::super::standard_content_engines().contains(&decision.engine_id));

    let mut pending = vec![(uuid::Uuid::new_v4(), "https://earlier.test/".to_string())];
    let earlier = pending[0].clone();
    assert_eq!(
        dispatch_content_spawn(&decision.engine_id, false, &mut pending, node, url),
        ContentSpawnDispatch::PendingBrowser,
        "a pinned browser must not fall into the retained-session registry"
    );
    assert_eq!(pending, vec![earlier, (node, url.to_string())]);

    // Retry the held node/address once host initialization has completed.
    let (queued_node, queued_url) = pending.pop().unwrap();
    assert_eq!(
        dispatch_content_spawn(
            &decision.engine_id,
            true,
            &mut pending,
            queued_node,
            &queued_url,
        ),
        ContentSpawnDispatch::Browser
    );
    assert_eq!(
        pending.len(),
        1,
        "a ready host does not enqueue another retry"
    );
}

#[test]
fn browser_dispatch_preserves_other_surfaces_and_retained_sessions() {
    let node = uuid::Uuid::new_v4();
    let mut pending = Vec::new();
    for engine in [
        inker::routing::ENGINE_WELD_CHROMIUM,
        inker::routing::ENGINE_SCRYING_WEB,
    ] {
        assert_eq!(
            dispatch_content_spawn(engine, true, &mut pending, node, "https://browser.test/",),
            ContentSpawnDispatch::Browser
        );
    }
    assert_eq!(
        dispatch_content_spawn(
            inker::routing::ENGINE_GENET_LIVERY,
            false,
            &mut pending,
            node,
            "https://reader.test/",
        ),
        ContentSpawnDispatch::Session
    );
    assert!(pending.is_empty());
}
