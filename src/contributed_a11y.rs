// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! AccessKit composition for product-contributed retained pane surfaces.
//!
//! Genet owns DOM semantics and Livery geometry. This module performs the two
//! host duties that cannot live there: namespace each independent DOM inside
//! Turnstone's one platform tree, and translate pane-local bounds into window
//! coordinates.

use std::collections::HashMap;

use accesskit::{NodeId, Role, TreeId, TreeUpdate};
use genet_scripted_dom::NodeId as DomNodeId;
use uxtree::{UxTree, node_id_for_path};

use crate::contributed_surface::ContributedSurfaceSessions;
use crate::panes::{PaneId, PaneSpec};
use crate::surface::Rect;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ContributedA11yRoute {
    pub pane: PaneId,
    pub generation: u64,
    pub node: DomNodeId,
}

/// Admit a queued platform route against the current source and session before
/// the provider can receive it. Shell's drain and regression controls share this
/// delivery seam; a reused pane/local control cannot revive an old admission.
pub(crate) fn apply_route(
    sessions: &mut ContributedSurfaceSessions,
    current_spec: Option<&PaneSpec>,
    route: ContributedA11yRoute,
    action: accesskit::Action,
) -> bool {
    let Some(spec) = current_spec.filter(|spec| spec.id == route.pane) else {
        return false;
    };
    let Some(surface) = sessions.get_mut(route.pane) else {
        return false;
    };
    if surface.generation() != route.generation || !surface.matches(&spec.kind, &spec.source) {
        return false;
    }
    surface.accessibility_action(action, route.node).is_some()
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PublishedRoute {
    pub route: ContributedA11yRoute,
    pub rect: Rect,
    pub source: std::sync::Arc<TreeUpdate>,
}

pub(crate) struct ContributedA11yProjection {
    pub trees: HashMap<PaneId, UxTree>,
    pub guests: Vec<TreeUpdate>,
    pub routes: HashMap<(TreeId, NodeId), PublishedRoute>,
    pub focus: Option<NodeId>,
}

/// Preserve guest-local references, focus and nested transforms. Only the
/// graft carries the pane's window placement.
pub(crate) fn project(
    sessions: &ContributedSurfaceSessions,
    pane_rects: &HashMap<PaneId, Rect>,
    focused_pane: Option<PaneId>,
) -> ContributedA11yProjection {
    let mut trees = HashMap::new();
    let mut guests = Vec::new();
    let mut routes = HashMap::new();
    let mut focus = None;
    let mut panes: Vec<_> = sessions.iter().collect();
    panes.sort_by_key(|(pane, _)| pane.0);
    for (pane, session) in panes {
        let Some(rect) = pane_rects.get(&pane).copied().filter(|r| {
            [r.x, r.y, r.w, r.h].into_iter().all(f32::is_finite) && r.w > 0.0 && r.h > 0.0
        }) else {
            continue;
        };
        let Some((update, action_map)) = session.accessibility_tree() else {
            continue;
        };
        let source = std::sync::Arc::new(update.clone());
        let (tree, guest, pane_routes, pane_focus) = namespace_tree(
            pane,
            session.generation(),
            session.descriptor().label.as_str(),
            rect,
            update,
            action_map,
            focused_pane == Some(pane) && session.accessibility_focus().is_some(),
        );
        routes.extend(pane_routes.into_iter().map(|(id, route)| {
            (
                (guest.tree_id, id),
                PublishedRoute {
                    route,
                    rect,
                    source: source.clone(),
                },
            )
        }));
        trees.insert(pane, tree);
        guests.push(guest);
        focus = focus.or(pane_focus);
    }
    ContributedA11yProjection {
        trees,
        guests,
        routes,
        focus,
    }
}

fn namespace_tree(
    pane: PaneId,
    generation: u64,
    label: &str,
    rect: Rect,
    update: TreeUpdate,
    action_map: HashMap<NodeId, DomNodeId>,
    carries_focus: bool,
) -> (
    UxTree,
    TreeUpdate,
    HashMap<NodeId, ContributedA11yRoute>,
    Option<NodeId>,
) {
    let path = format!("turnstone/contributed/{}/admission/{generation}", pane.0);
    let tree_id = uxtree::graft::tree_id_for_path(&path);
    let root = node_id_for_path(&path);
    let mut graft = uxtree::graft::graft_node(
        Role::Group,
        tree_id,
        (rect.x as f64, rect.y as f64),
        (rect.w as f64, rect.h as f64),
    );
    graft.set_label(label);
    let mut guest = uxtree::graft::as_subtree(update, tree_id);
    if let Some(root) = guest.tree.as_ref().map(|tree| tree.root)
        && let Some((_, node)) = guest.nodes.iter_mut().find(|(id, _)| *id == root)
        && node.role() == Role::Window
    {
        node.set_role(Role::Group);
    }
    let routes = action_map
        .into_iter()
        .map(|(id, node)| {
            (
                id,
                ContributedA11yRoute {
                    pane,
                    generation,
                    node,
                },
            )
        })
        .collect();
    (
        UxTree {
            root,
            nodes: vec![(root, graft)],
        },
        guest,
        routes,
        carries_focus.then_some(root),
    )
}

/// Check the captured publication before the retained surface's own action
/// validation. A moved pane or changed source cannot receive an old request.
pub(crate) fn published_route_is_current(
    sessions: &ContributedSurfaceSessions,
    route: &PublishedRoute,
    current_rect: Option<Rect>,
    target: NodeId,
    action: accesskit::Action,
) -> bool {
    current_rect == Some(route.rect)
        && sessions.get(route.route.pane).is_some_and(|session| {
            session.generation() == route.route.generation
                && session
                    .accessibility_tree()
                    .is_some_and(|(source, actions)| {
                        source == *route.source
                            && actions.get(&target) == Some(&route.route.node)
                            && source.nodes.iter().any(|(id, n)| {
                                *id == target
                                    && !n.is_hidden()
                                    && !n.is_disabled()
                                    && n.supports_action(action)
                            })
                    })
        })
}

#[cfg(test)]
#[path = "contributed_a11y/tests.rs"]
mod tests;
