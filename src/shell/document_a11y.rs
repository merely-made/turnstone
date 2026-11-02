// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Controller-owned document semantics and placement-bound assistive actions.
use crate::surface::Surface;
use accesskit::{Action, Node, NodeId, Role, TreeId, TreeUpdate};
use inker::{DocumentA11yAction, DocumentA11yNodeId, DocumentA11yProjection, DocumentA11yRole};
use pelt_core::PeltSessionIdentity;

#[path = "document_a11y/projection_work.rs"]
mod projection_work;
use projection_work::{projection_tree, replace_document};

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(super) struct DocumentRoute {
    pub key: uuid::Uuid,
    pub identity: PeltSessionIdentity,
    pub revision: u64,
    pub local: DocumentA11yNodeId,
    pub placement: Surface,
}

pub(super) type PlacementSnapshot =
    pelt_core::PeltPlacementA11y<super::content_pool::ContentKey, crate::surface::SurfaceId>;

#[derive(Clone, Debug)]
pub(super) struct PoolDocumentRoute {
    pub key: uuid::Uuid,
    pub local: DocumentA11yNodeId,
    pub placement: Surface,
    pub snapshot: std::sync::Arc<PlacementSnapshot>,
}

impl PartialEq for PoolDocumentRoute {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
            && self.local == other.local
            && self.placement == other.placement
            && snapshots_equal(&self.snapshot, &other.snapshot)
    }
}

pub(super) fn snapshots_equal(left: &PlacementSnapshot, right: &PlacementSnapshot) -> bool {
    #[cfg(test)]
    SNAPSHOT_COMPARISONS.with(|count| count.set(count.get() + 1));
    left == right
}

#[cfg(test)]
thread_local! {
    static SNAPSHOT_COMPARISONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn count_snapshot_comparisons(f: impl FnOnce()) -> usize {
    SNAPSHOT_COMPARISONS.with(|count| count.set(0));
    f();
    SNAPSHOT_COMPARISONS.with(std::cell::Cell::get)
}

fn placement_admissions(
    key: uuid::Uuid,
    placement: Surface,
    tree_id: TreeId,
    snapshot: PlacementSnapshot,
) -> std::collections::HashMap<(TreeId, NodeId), super::a11y_bridge::Admission> {
    // One immutable source observation belongs to this placement publication;
    // controls and queued requests share it without copying every semantic node.
    let snapshot = std::sync::Arc::new(snapshot);
    snapshot
        .projection()
        .nodes()
        .iter()
        .filter(|node| !node.actions.is_empty())
        .map(|node| {
            (
                (tree_id, NodeId(node.id.get())),
                super::a11y_bridge::Admission::PoolDocument(PoolDocumentRoute {
                    key,
                    local: node.id,
                    placement,
                    snapshot: snapshot.clone(),
                }),
            )
        })
        .collect()
}

#[cfg(test)]
fn route_is_current<F: 'static>(
    route: &DocumentRoute,
    controller: &pelt_core::PeltController<F>,
    placement: Option<Surface>,
    action: DocumentA11yAction,
) -> bool {
    if controller.session_identity() != route.identity
        || placement != Some(route.placement)
        || route.placement.kind != crate::surface::SurfaceKind::Content(route.key)
        || !valid_rect(route.placement.rect)
    {
        return false;
    }
    controller
        .accessibility_projection()
        .is_some_and(|projection| {
            projection.revision() == route.revision
                && projection.node(route.local).is_some_and(|node| {
                    !node.state.hidden && !node.state.disabled && node.actions.contains(&action)
                })
        })
}

fn valid_rect(rect: crate::surface::Rect) -> bool {
    [rect.x, rect.y, rect.w, rect.h]
        .into_iter()
        .all(f32::is_finite)
        && rect.w > 0.0
        && rect.h > 0.0
}

#[cfg(test)]
fn dispatch_nonpointer<F: 'static>(
    controller: &mut pelt_core::PeltController<F>,
    route: &DocumentRoute,
    placement: Option<Surface>,
    action: DocumentA11yAction,
    data: Option<inker::DocumentA11yActionData>,
) -> bool {
    action != DocumentA11yAction::Click
        && route_is_current(route, controller, placement, action)
        && (action != DocumentA11yAction::SetValue
            || matches!(data, Some(inker::DocumentA11yActionData::Value(_))))
        && controller.dispatch_accessibility_action(&inker::DocumentA11yActionRequest {
            revision: route.revision,
            target: route.local,
            action,
            data,
        })
}

#[cfg(test)]
fn click_inputs(point: inker::DocumentA11yPoint) -> [inker::SessionInput; 2] {
    [
        inker::SessionButtonState::Pressed,
        inker::SessionButtonState::Released,
    ]
    .map(|state| inker::SessionInput::PointerButton {
        x: point.x,
        y: point.y,
        button: inker::SessionPointerButton::Primary,
        state,
        modifiers: inker::SessionModifiers::default(),
    })
}

#[cfg(test)]
#[path = "document_a11y/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "document_a11y/admission_work_tests.rs"]
mod admission_work_tests;

pub(super) struct Projected {
    pub tree: uxtree::UxTree,
    pub routes: crate::a11y::A11yRoutes,
    pub focus: Option<NodeId>,
    pub guests: Vec<TreeUpdate>,
    pub admissions: std::collections::HashMap<(TreeId, NodeId), super::a11y_bridge::Admission>,
}

fn guest_id(
    key: uuid::Uuid,
    identity: PeltSessionIdentity,
    placement: crate::surface::SurfaceId,
) -> TreeId {
    uxtree::graft::tree_id_for_path(&format!(
        "turnstone/document/{key}/instance/{}/generation/{}/placement/{}",
        identity.instance_id, identity.generation, placement.0
    ))
}

impl super::Shell {
    pub(super) fn project_document_accessibility(
        &mut self,
        frame: &mut Projected,
        plan: &[Surface],
    ) {
        use crate::surface::{FocusTarget, SurfaceKind};
        let mut documents: Vec<_> = self
            .content_sessions
            .iter()
            .filter_map(|(&key, content)| {
                content.document().map(|controller| {
                    (
                        key,
                        controller.address().to_owned(),
                        controller.a11y_capability(),
                    )
                })
            })
            .collect();
        documents.sort_by_key(|(key, _, _)| *key);
        for (key, address, capability) in documents {
            let root = uxtree::node_id_for_path(&format!("turnstone/doc/{key}"));
            let placements: Vec<_> = plan
                .iter()
                .filter(|p| p.kind == SurfaceKind::Content(key) && valid_rect(p.rect))
                .copied()
                .collect();
            if placements.is_empty() {
                replace_document(&mut frame.tree, root, None);
                continue;
            }
            let label = frame
                .tree
                .nodes
                .iter()
                .find(|(id, _)| *id == root)
                .and_then(|(_, n)| n.label())
                .unwrap_or(&address)
                .to_owned();
            let mut parent = Node::new(Role::Document);
            parent.set_label(label);
            let mut children = Vec::new();
            let mut nodes = Vec::new();
            for placement in placements {
                let Some(snapshot) = self
                    .content_sessions
                    .pool
                    .accessibility_projection(&placement.id)
                else {
                    continue;
                };
                let projection = snapshot.projection();
                let Some(source_tree) = projection_tree(projection) else {
                    continue;
                };
                let tree_id = guest_id(key, snapshot.identity(), placement.id);
                let graft_id = uxtree::node_id_for_path(&format!(
                    "turnstone/doc/{key}/placement/{}",
                    placement.id.0
                ));
                // The projection's bounds are in the document's layout pixels;
                // the plan rect and the accessibility tree are physical.
                let scale = self
                    .content_sessions
                    .pool
                    .placement(&placement.id)
                    .map_or(1.0, |pool| f64::from(pool.scale_factor));
                let origin = (placement.rect.x as f64, placement.rect.y as f64);
                let mut graft = uxtree::graft::graft_node(
                    Role::Group,
                    tree_id,
                    origin,
                    (
                        placement.rect.w as f64 / scale,
                        placement.rect.h as f64 / scale,
                    ),
                );
                graft.set_transform(
                    accesskit::Affine::translate(origin) * accesskit::Affine::scale(scale),
                );
                frame
                    .admissions
                    .extend(placement_admissions(key, placement, tree_id, snapshot));
                if self.app.focus
                    == (FocusTarget::Content {
                        node: key,
                        appearance: placement.id,
                    })
                {
                    frame.focus = Some(graft_id);
                }
                children.push(graft_id);
                nodes.push((graft_id, graft));
                frame
                    .guests
                    .push(uxtree::graft::as_subtree(source_tree, tree_id));
            }
            if children.is_empty() {
                parent.set_description(format!(
                    "{capability:?}: no live accessibility projection is available"
                ));
            }
            parent.set_children(children);
            nodes.push((root, parent));
            replace_document(&mut frame.tree, root, Some(uxtree::UxTree { root, nodes }));
        }
    }

    pub(super) fn dispatch_pool_document_accessibility(
        &mut self,
        route: &PoolDocumentRoute,
        request: &accesskit::ActionRequest,
    ) -> bool {
        let action = match request.action {
            Action::Click => DocumentA11yAction::Click,
            Action::Focus => DocumentA11yAction::Focus,
            Action::SetValue => DocumentA11yAction::SetValue,
            Action::ScrollIntoView => DocumentA11yAction::ScrollIntoView,
            Action::Increment => DocumentA11yAction::Increment,
            Action::Decrement => DocumentA11yAction::Decrement,
            _ => return false,
        };
        if action == DocumentA11yAction::Click
            && (self.pointer_capture.is_some() || self.surface_pointer_buttons.0 != 0)
        {
            return false;
        }
        self.sync_content_placements();
        let data = match (&request.data, action) {
            (Some(accesskit::ActionData::Value(value)), DocumentA11yAction::SetValue) => {
                Some(inker::DocumentA11yActionData::Value(value.to_string()))
            },
            (_, DocumentA11yAction::SetValue) => return false,
            _ => None,
        };
        let effect = self.content_sessions.pool.dispatch_accessibility_action(
            &route.snapshot,
            &inker::DocumentA11yActionRequest {
                revision: route.snapshot.projection().revision(),
                target: route.local,
                action,
                data,
            },
        );
        let handled = effect.handled;
        if handled
            && matches!(
                action,
                DocumentA11yAction::Focus | DocumentA11yAction::Click
            )
        {
            self.app.focus = crate::surface::FocusTarget::Content {
                node: route.key,
                appearance: route.placement.id,
            };
        }
        self.apply_content_effect(route.key, effect);
        handled
    }
}
