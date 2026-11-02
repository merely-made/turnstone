// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::shell::{a11y_bridge::ActionQueueState, content_pool::ContentKey};
use crate::surface::{Rect, SurfaceId, SurfaceKind};
use pelt_core::{PeltContent, PeltController, PeltControllerConfig, PeltControllerPool};

const CONTROLS: usize = 32;
type Pool = PeltControllerPool<ContentKey, SurfaceId, netrender::Scene>;

fn fixture() -> (Pool, uuid::Uuid, Surface) {
    let key = uuid::Uuid::from_u128(99);
    let content = ContentKey::Node(key);
    let surface = Surface {
        id: SurfaceId(71),
        kind: SurfaceKind::Content(key),
        rect: Rect::new(0.0, 0.0, 640.0, 480.0),
    };
    let body = (0..CONTROLS)
        .map(|id| format!("=> /{id} Control {id}\n"))
        .collect::<String>();
    let address = "gemini://admission.fixture/";
    let engine = inker::routing::ENGINE_NEMATIC_GEMTEXT;
    let controller = PeltController::new(
        crate::shell::standard_content_engines(),
        inker::SurfaceEngineRegistry::new(),
        PeltControllerConfig::from_request(
            engine,
            inker::SessionSpawnRequest::new(address)
                .with_body(body)
                .with_viewport(640, 480),
        )
        .with_host_history(),
        crate::shell::ShellClock(std::time::Instant::now()),
    )
    .unwrap();
    let decision = crate::shell::standard_route_policy().route(&inker::EngineRouteRequest {
        workspace_id: inker::WorkspaceRouteId::new("admission-work"),
        view: None,
        node: None,
        address: address.into(),
        content_type: None,
        pinned_engine: Some(engine.into()),
    });
    let mut pool = Pool::new();
    pool.insert(
        content,
        PeltContent::from_controller(
            controller,
            pelt_core::PeltRoute {
                decision,
                source: pelt_core::PeltRouteSource::Automatic,
                state: pelt_core::PeltRouteState::Document,
            },
        ),
    );
    pool.set_placements([(
        surface.id,
        pelt_core::PeltPlacement {
            content,
            window: 0,
            rect: pelt_core::WorkspaceRect::new(0.0, 0.0, 640.0, 480.0),
            visible: true,
            focused: true,
            scale_factor: 1.0,
        },
    )]);
    (pool, key, surface)
}

fn admissions(
    key: uuid::Uuid,
    surface: Surface,
    snapshot: PlacementSnapshot,
) -> std::collections::HashMap<(TreeId, NodeId), super::super::a11y_bridge::Admission> {
    let tree = guest_id(key, snapshot.identity(), surface.id);
    placement_admissions(key, surface, tree, snapshot)
}

fn request(target: &(TreeId, NodeId)) -> accesskit::ActionRequest {
    accesskit::ActionRequest {
        target_tree: target.0,
        target_node: target.1,
        action: Action::Focus,
        data: None,
    }
}

#[test]
fn pool_document_admission_shares_one_full_snapshot_per_placement() {
    let (mut pool, key, surface) = fixture();
    let snapshot = pool.accessibility_projection(&surface.id).unwrap();
    let expected = snapshot
        .projection()
        .nodes()
        .iter()
        .filter(|node| !node.actions.is_empty())
        .count();
    let routes = admissions(key, surface, snapshot);
    assert_eq!(
        routes.len(),
        expected,
        "every actual actionable source node"
    );
    assert!(expected >= CONTROLS, "source links remain actionable");
    let allocations: std::collections::HashSet<_> = routes
        .values()
        .map(|route| {
            let super::super::a11y_bridge::Admission::PoolDocument(route) = route else {
                unreachable!()
            };
            route.snapshot.projection().nodes().as_ptr()
        })
        .collect();
    assert_eq!(
        allocations.len(),
        1,
        "action routes must share the immutable full projection"
    );
}

#[test]
fn pool_document_admission_compares_full_snapshot_once_per_placement() {
    let (mut pool, key, surface) = fixture();
    let snapshot = pool.accessibility_projection(&surface.id).unwrap();
    let mut queue = ActionQueueState::default();
    queue.publish(admissions(key, surface, snapshot.clone()));
    let before = queue.published.clone();
    let comparisons = count_snapshot_comparisons(|| {
        queue.publish(admissions(key, surface, snapshot));
    });
    assert_eq!(
        comparisons, 1,
        "one full comparison for all source controls"
    );
    for (target, captured) in before {
        assert!(queue.is_current(&request(&target), &captured));
    }
}

#[test]
fn pool_document_admission_keeps_live_focus_freshness_for_every_control() {
    let (mut pool, key, surface) = fixture();
    let snapshot = pool.accessibility_projection(&surface.id).unwrap();
    let target = snapshot
        .projection()
        .nodes()
        .iter()
        .find(|node| node.actions.contains(&DocumentA11yAction::Focus))
        .unwrap()
        .id;
    let mut queue = ActionQueueState::default();
    queue.publish(admissions(key, surface, snapshot.clone()));
    let before = queue.published.clone();
    let action = inker::DocumentA11yActionRequest {
        revision: snapshot.projection().revision(),
        target,
        action: DocumentA11yAction::Focus,
        data: None,
    };
    assert!(
        pool.dispatch_accessibility_action(&snapshot, &action)
            .handled
    );
    assert!(
        !pool
            .dispatch_accessibility_action(&snapshot, &action)
            .handled,
        "an earlier action in the drain retires the old exact source admission"
    );
    let changed = pool.accessibility_projection(&surface.id).unwrap();
    let comparisons = count_snapshot_comparisons(|| {
        queue.publish(admissions(key, surface, changed));
    });
    assert_eq!(comparisons, 1, "one exact changed-source comparison");
    for (target, captured) in before {
        assert!(!queue.is_current(&request(&target), &captured));
    }
}
