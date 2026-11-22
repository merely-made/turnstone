// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Turnstone route admission for Cambium's shared native publisher.
//!
//! Each native window has its own queue. Turnstone supplies route equality
//! and rechecks the source and placement when dispatching an observed action.

use std::collections::HashMap;
use std::sync::Arc;

use accesskit::TreeId;
use cambium_winit_a11y::guest::{ForeignA11y, GuestAdmission};

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Admission {
    #[cfg(test)]
    Document(super::document_a11y::DocumentRoute),
    PoolDocument(super::document_a11y::PoolDocumentRoute),
    Host(crate::a11y::ActionEnvelope),
    Contributed(crate::contributed_a11y::PublishedRoute),
    Guest(GuestAdmission<uuid::Uuid>),
}

pub(super) type PublishedAdmission = uxtree::generation::Published<Admission>;
pub(super) type AdmissionKey = uxtree::envelope::TreeAddress;
pub(super) type ActionQueue = cambium_winit_a11y::ActionQueue<Admission>;
pub(super) type ForestPublisher = cambium_winit_a11y::ForestPublisher<Admission>;
#[cfg(test)]
pub(super) type QueuedAction = cambium_winit_a11y::QueuedAction<Admission>;

pub(super) fn admission_equivalence() -> impl FnMut(&Admission, &Admission) -> bool {
    // Controls from one pane or placement share an immutable source. Compare
    // each snapshot pair once for that publication. Allocation addresses key
    // the memo only while the compared snapshots are both alive.
    let mut compared_sources = HashMap::new();
    let mut compared_snapshots = HashMap::new();
    move |before, after| match (before, after) {
        (Admission::PoolDocument(before), Admission::PoolDocument(after)) => {
            before.key == after.key
                && before.local == after.local
                && before.placement == after.placement
                && *compared_snapshots
                    .entry((Arc::as_ptr(&before.snapshot), Arc::as_ptr(&after.snapshot)))
                    .or_insert_with(|| {
                        super::document_a11y::snapshots_equal(&before.snapshot, &after.snapshot)
                    })
        },
        (Admission::Contributed(before), Admission::Contributed(after)) => {
            before.address == after.address
                && before.route == after.route
                && before.rect == after.rect
                && *compared_sources
                    .entry((Arc::as_ptr(&before.source), Arc::as_ptr(&after.source)))
                    .or_insert_with(|| {
                        crate::contributed_a11y::source_trees_equal(&before.source, &after.source)
                    })
        },
        _ => before == after,
    }
}

fn with_host_routes(
    mut admissions: HashMap<AdmissionKey, Admission>,
    envelopes: &crate::a11y::ActionEnvelopes,
) -> HashMap<AdmissionKey, Admission> {
    for (&id, envelope) in envelopes {
        admissions
            .entry((TreeId::ROOT, id))
            .or_insert_with(|| Admission::Host(envelope.clone()));
    }
    admissions
}

pub(super) fn window_admissions(
    admissions: HashMap<AdmissionKey, Admission>,
    envelopes: &crate::a11y::ActionEnvelopes,
    forest: &ForeignA11y<uuid::Uuid>,
) -> HashMap<AdmissionKey, Admission> {
    let mut admissions = with_host_routes(admissions, envelopes);
    for (address, guest) in forest.guest_admissions() {
        admissions.entry(address).or_insert(Admission::Guest(guest));
    }
    admissions
}

pub(super) fn with_captured_generation(mut captured: PublishedAdmission) -> Admission {
    if let Admission::Host(envelope) = &mut captured.admission {
        envelope.stability = uxtree::envelope::Stability::Derived {
            generation: Some(captured.generation),
        };
    }
    captured.admission
}

/// Connect a native window to the common publisher and wake the existing UI
/// event loop when the platform queues an action or activates the adapter.
pub(super) fn install(
    window: &dyn winit::window::Window,
    queue: ActionQueue,
    wake: crate::shell::ShellEventProxy,
    name: &str,
) -> ForestPublisher {
    let publisher = ForestPublisher::install(
        window,
        queue,
        move || {
            let _ = wake.send_event(crate::shell::ShellEvent::ActorWake);
        },
        name,
    );
    match publisher.status() {
        genet_winit_host::BridgeStatus::Installed => tracing::info!("a11y_bridge: installed"),
        genet_winit_host::BridgeStatus::Unavailable => {
            tracing::warn!("a11y bridge installation failed")
        },
    }
    publisher
}

#[cfg(test)]
pub(super) fn publish_for_test(
    queue: &ActionQueue,
    admissions: HashMap<AdmissionKey, Admission>,
) -> HashMap<AdmissionKey, PublishedAdmission> {
    let keys: Vec<_> = admissions.keys().copied().collect();
    queue.publish_source(admissions, admission_equivalence());
    keys.into_iter()
        .map(|(target_tree, target_node)| {
            let captured = queue
                .admission(&accesskit::ActionRequest {
                    action: accesskit::Action::Click,
                    target_tree,
                    target_node,
                    data: None,
                })
                .expect("the supplied admission was published");
            ((target_tree, target_node), captured)
        })
        .collect()
}

#[cfg(test)]
#[path = "a11y_bridge/admission_tests.rs"]
mod admission_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::ActionRequest;

    #[test]
    fn queued_admission_stays_on_its_published_revision_and_placement() {
        let key = uuid::Uuid::from_u128(1);
        let tree = uxtree::graft::tree_id_for_path("queue-test");
        let node = accesskit::NodeId(2);
        let route = super::super::document_a11y::DocumentRoute {
            key,
            identity: pelt_core::PeltSessionIdentity {
                instance_id: 1,
                generation: 1,
            },
            revision: 4,
            local: inker::DocumentA11yNodeId::new(2),
            placement: crate::surface::Surface {
                id: crate::surface::SurfaceId(3),
                kind: crate::surface::SurfaceKind::Content(key),
                rect: crate::surface::Rect::new(10.0, 20.0, 30.0, 40.0),
            },
        };
        let queue = ActionQueue::default();
        publish_for_test(
            &queue,
            std::collections::HashMap::from([((tree, node), Admission::Document(route.clone()))]),
        );
        let request = || accesskit::ActionRequest {
            target_tree: tree,
            target_node: node,
            action: accesskit::Action::Click,
            data: None,
        };
        queue.enqueue_source(request());
        let mut newer = route.clone();
        newer.revision = 5;
        newer.placement.rect.x = 100.0;
        publish_for_test(
            &queue,
            std::collections::HashMap::from([((tree, node), Admission::Document(newer.clone()))]),
        );
        queue.enqueue_source(request());
        let queued = queue.take();
        assert_eq!(
            queued[0].admission.as_ref().map(|a| a.admission.clone()),
            Some(Admission::Document(route))
        );
        assert_eq!(
            queued[1].admission.as_ref().map(|a| a.admission.clone()),
            Some(Admission::Document(newer.clone()))
        );
        assert!(!queue.is_current(&request(), queued[0].admission.as_ref().unwrap()));
        assert!(queue.is_current(&request(), queued[1].admission.as_ref().unwrap()));
        publish_for_test(&queue, std::collections::HashMap::new());
        publish_for_test(
            &queue,
            std::collections::HashMap::from([((tree, node), Admission::Document(newer))]),
        );
        assert!(
            !queue.is_current(&request(), queued[1].admission.as_ref().unwrap()),
            "hidden and reopened publication retires queued admission"
        );
    }
}
