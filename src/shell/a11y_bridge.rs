// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The OS AccessKit bridge: app projection and retained foreign subtrees,
//! composed on the UI thread and pushed to the platform assistive stack.
//!
//! Turnstone has carried a complete in-process accessibility projection since
//! the a11y module landed — `project_app` stitches chrome, panes, and the
//! frozen projection into one `UxTree`, and the scenario lane asserts against
//! it. What was missing was the last foot: nothing handed that tree to the OS,
//! so a screen reader saw a bare window while the harness saw everything. The
//! projection grammar plan's manual screen-reader pass fails its own preflight
//! ("a11y_bridge: installed") without this file.
//!
//! The adapter's activation handler runs on whatever thread the platform
//! calls in from, so it takes the latest tree out of a shared slot rather
//! than reaching into `App`, which lives on the main thread and must stay
//! there. The shell retains a safe host-only activation snapshot in the slot;
//! activation wakes the UI thread to replay nested subtrees in parent order.
//! Foreign semantic changes publish on their own wake through
//! `update_if_active`, independently of pixel acquisition. Each event frame
//! publishes the composed host and live document forest, including the frame
//! that settles into idle. Composed snapshots do not prove screen-reader use.
//!
//! Actions are routed, not dropped, as of the day after the first pass: the
//! platform hands an `ActionRequest` to whatever thread it likes, so the
//! handler queues it and wakes the event loop, and the shell drains the
//! queue on the main thread, resolves each target through the route table
//! built beside the projection, and lowers it through the same update spine
//! a keypress uses. A request whose node has no route lands in the event
//! stream as `interaction-missed a11y-action`, because a miss a receipt
//! cannot see is the failure this whole lane exists to prevent.

use accesskit::{ActionRequest, TreeUpdate};
use std::sync::{Arc, Mutex};
use winit::window::Window;

/// Queued assistive actions, drained by the shell on the main thread.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Admission {
    #[cfg(test)]
    Document(super::document_a11y::DocumentRoute),
    PoolDocument(super::document_a11y::PoolDocumentRoute),
    Contributed(crate::contributed_a11y::PublishedRoute),
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PublishedAdmission {
    pub generation: u64,
    pub admission: Admission,
}

pub(super) struct QueuedAction {
    pub request: ActionRequest,
    pub admission: Option<PublishedAdmission>,
}

#[derive(Default)]
pub(crate) struct ActionQueueState {
    pub(super) requests: Vec<QueuedAction>,
    pub(super) published:
        std::collections::HashMap<(accesskit::TreeId, accesskit::NodeId), PublishedAdmission>,
    next_generation: u64,
}

impl ActionQueueState {
    pub(super) fn publish(
        &mut self,
        admissions: std::collections::HashMap<(accesskit::TreeId, accesskit::NodeId), Admission>,
    ) {
        self.published = admissions
            .into_iter()
            .map(|(key, admission)| {
                let generation = self
                    .published
                    .get(&key)
                    .filter(|old| old.admission == admission)
                    .map(|old| old.generation)
                    .unwrap_or_else(|| {
                        self.next_generation = self
                            .next_generation
                            .checked_add(1)
                            .expect("a11y admission generations exhausted");
                        self.next_generation
                    });
                (
                    key,
                    PublishedAdmission {
                        generation,
                        admission,
                    },
                )
            })
            .collect();
    }

    pub(super) fn is_current(
        &self,
        request: &ActionRequest,
        captured: &PublishedAdmission,
    ) -> bool {
        self.published
            .get(&(request.target_tree, request.target_node))
            .is_some_and(|current| current.generation == captured.generation)
    }

    fn enqueue(&mut self, request: ActionRequest) {
        let admission = self
            .published
            .get(&(request.target_tree, request.target_node))
            .cloned();
        self.requests.push(QueuedAction { request, admission });
    }
}

pub(crate) type ActionQueue = Arc<Mutex<ActionQueueState>>;

/// Initial native activation must never enter a guest before the ordered
/// complete forest has installed its subtrees.
pub(super) fn safe_initial(update: &TreeUpdate) -> TreeUpdate {
    let mut initial = update.clone();
    let root = initial.tree.as_ref().expect("complete host tree").root;
    initial.focus = root;
    initial.nodes.retain(|(id, _)| *id == root);
    for (_, node) in &mut initial.nodes {
        node.clear_children();
        node.clear_tree_id();
    }
    initial
}

/// Each native window owns the same shared Genet bridge and captures the
/// currently served admission synchronously in its platform action callback.
pub(crate) fn install(
    window: &Window,
    queue: ActionQueue,
    wake: winit::event_loop::EventLoopProxy<()>,
    name: &str,
) -> genet_winit_host::AccessKitBridge {
    let mut bridge = genet_winit_host::AccessKitBridge::with_action_handler(
        move || {
            let _ = wake.send_event(());
        },
        move |request| {
            queue
                .lock()
                .expect("a11y action queue poisoned")
                .enqueue(ActionRequest {
                    action: request.action,
                    target_tree: request.target_tree,
                    target_node: request.target_node,
                    data: request.data,
                })
        },
    );
    let root = accesskit::NodeId(0);
    let mut node = accesskit::Node::new(accesskit::Role::Window);
    node.set_label(name);
    let initial = TreeUpdate {
        tree_id: accesskit::TreeId::ROOT,
        tree: Some(accesskit::Tree::new(root)),
        nodes: vec![(root, node)],
        focus: root,
    };
    if let Err(error) = bridge.install(window, initial) {
        tracing::warn!(%error, "a11y bridge installation failed");
    } else {
        tracing::info!("a11y_bridge: installed");
    }
    bridge
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn content_pool_activation_starts_on_guest_free_root_before_window_replay() {
        let root = accesskit::NodeId(1);
        let child = accesskit::NodeId(2);
        let mut window = accesskit::Node::new(accesskit::Role::Window);
        window.set_label("Turnstone lens 0");
        window.set_children(vec![child]);
        let mut graft = accesskit::Node::new(accesskit::Role::Group);
        graft.set_tree_id(uxtree::graft::tree_id_for_path("lens/guest"));
        let update = TreeUpdate {
            tree_id: accesskit::TreeId::ROOT,
            nodes: vec![(root, window), (child, graft)],
            tree: Some(accesskit::Tree::new(root)),
            focus: child,
        };
        let initial = safe_initial(&update);
        assert_eq!(
            initial.focus, root,
            "activation never focuses an uninstalled guest"
        );
        assert_eq!(
            initial.nodes.len(),
            1,
            "only the complete host root is served initially"
        );
        assert!(initial.nodes[0].1.children().is_empty());
        assert!(
            initial
                .nodes
                .iter()
                .all(|(_, node)| node.tree_id().is_none())
        );
        assert_eq!(initial.nodes[0].1.label(), Some("Turnstone lens 0"));
        assert_eq!(update.focus, child, "ordered final focus remains unchanged");
    }

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
        let mut queue = ActionQueueState::default();
        queue.publish(std::collections::HashMap::from([(
            (tree, node),
            Admission::Document(route.clone()),
        )]));
        let request = || accesskit::ActionRequest {
            target_tree: tree,
            target_node: node,
            action: accesskit::Action::Click,
            data: None,
        };
        queue.enqueue(request());
        let mut newer = route.clone();
        newer.revision = 5;
        newer.placement.rect.x = 100.0;
        queue.publish(std::collections::HashMap::from([(
            (tree, node),
            Admission::Document(newer.clone()),
        )]));
        queue.enqueue(request());
        assert_eq!(
            queue.requests[0]
                .admission
                .as_ref()
                .map(|a| a.admission.clone()),
            Some(Admission::Document(route))
        );
        assert_eq!(
            queue.requests[1]
                .admission
                .as_ref()
                .map(|a| a.admission.clone()),
            Some(Admission::Document(newer.clone()))
        );
        assert!(!queue.is_current(&request(), queue.requests[0].admission.as_ref().unwrap()));
        assert!(queue.is_current(&request(), queue.requests[1].admission.as_ref().unwrap()));
        queue.publish(std::collections::HashMap::new());
        queue.publish(std::collections::HashMap::from([(
            (tree, node),
            Admission::Document(newer),
        )]));
        assert!(
            !queue.is_current(&request(), queue.requests[1].admission.as_ref().unwrap()),
            "hidden and reopened publication retires queued admission"
        );
    }
}
