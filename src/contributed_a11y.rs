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

use accesskit::{NodeId, Role, TreeUpdate};
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

pub(crate) struct ContributedA11yProjection {
    pub trees: HashMap<PaneId, UxTree>,
    pub routes: HashMap<NodeId, ContributedA11yRoute>,
    pub focus: Option<NodeId>,
}

/// Project every laid-out contributed session that still has a visible pane.
/// A session has no accessibility tree before its first retained layout; that
/// is an honest absence rather than a second speculative layout.
pub(crate) fn project(
    sessions: &ContributedSurfaceSessions,
    pane_rects: &HashMap<PaneId, Rect>,
    focused_pane: Option<PaneId>,
) -> ContributedA11yProjection {
    let mut trees = HashMap::new();
    let mut routes = HashMap::new();
    let mut focus = None;
    let mut panes: Vec<_> = sessions.iter().collect();
    panes.sort_by_key(|(pane, _)| pane.0);

    for (pane, session) in panes {
        let Some(rect) = pane_rects.get(&pane).copied() else {
            continue;
        };
        let Some((update, action_map)) = session.accessibility_tree() else {
            continue;
        };
        let has_dom_focus = session.accessibility_focus().is_some();
        let label = session.descriptor().label.as_str();
        let (tree, pane_routes, pane_focus) = namespace_tree(
            pane,
            session.generation(),
            label,
            rect,
            update,
            action_map,
            focused_pane == Some(pane) && has_dom_focus,
        );
        trees.insert(pane, tree);
        routes.extend(pane_routes);
        focus = focus.or(pane_focus);
    }

    ContributedA11yProjection {
        trees,
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
    HashMap<NodeId, ContributedA11yRoute>,
    Option<NodeId>,
) {
    let local_root = update
        .tree
        .as_ref()
        .expect("a full retained DOM projection carries its root")
        .root;
    let remap: HashMap<_, _> = update
        .nodes
        .iter()
        .map(|(local, _)| {
            (
                *local,
                node_id_for_path(&format!(
                    "turnstone/contributed/{}/admission/{generation}/dom/{local:?}",
                    pane.0
                )),
            )
        })
        .collect();

    let mut routes = HashMap::new();
    let mut nodes = Vec::with_capacity(update.nodes.len());
    for (local, mut node) in update.nodes {
        let global = remap[&local];
        node.set_children(
            node.children()
                .iter()
                .filter_map(|child| remap.get(child).copied())
                .collect::<Vec<_>>(),
        );
        if let Some(bounds) = node.bounds() {
            node.set_bounds(accesskit::Rect::new(
                bounds.x0 + rect.x as f64,
                bounds.y0 + rect.y as f64,
                bounds.x1 + rect.x as f64,
                bounds.y1 + rect.y as f64,
            ));
        }
        // ScriptedDom's document node represents a standalone host window.
        // Inside Turnstone it is a pane subtree, so it must not claim a second
        // native window. Its semantic document child remains untouched.
        if local == local_root {
            node.set_role(Role::Group);
            node.set_label(label);
            node.set_bounds(accesskit::Rect::new(
                rect.x as f64,
                rect.y as f64,
                (rect.x + rect.w) as f64,
                (rect.y + rect.h) as f64,
            ));
        }
        if let Some(dom_node) = action_map.get(&local).copied() {
            routes.insert(
                global,
                ContributedA11yRoute {
                    pane,
                    generation,
                    node: dom_node,
                },
            );
        }
        nodes.push((global, node));
    }

    let root = remap[&local_root];
    let focus = carries_focus
        .then(|| remap.get(&update.focus).copied())
        .flatten();
    (UxTree { root, nodes }, routes, focus)
}

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::{Action, Node, Tree, TreeId};

    fn local_tree() -> (TreeUpdate, HashMap<NodeId, DomNodeId>) {
        let root = NodeId(1);
        let child = NodeId(2);
        let mut button = Node::new(Role::Button);
        button.set_label("Save");
        button.set_bounds(accesskit::Rect::new(5.0, 7.0, 25.0, 27.0));
        button.add_action(Action::Click);
        button.add_action(Action::Focus);
        let mut group = Node::new(Role::Window);
        group.set_children(vec![child]);
        (
            TreeUpdate {
                nodes: vec![(child, button), (root, group)],
                tree: Some(Tree::new(root)),
                tree_id: TreeId::ROOT,
                focus: child,
            },
            HashMap::from([(child, DomNodeId::from_raw(2))]),
        )
    }

    #[test]
    fn namespacing_places_bounds_and_preserves_distinct_actions() {
        let (tree, routes, focus) = namespace_tree(
            PaneId(9),
            1,
            "Knot document",
            Rect::new(100.0, 40.0, 300.0, 200.0),
            local_tree().0,
            local_tree().1,
            true,
        );
        let focus = focus.expect("the focused pane carries DOM focus");
        let (_, button) = tree
            .nodes
            .iter()
            .find(|(id, _)| *id == focus)
            .expect("focused button");
        assert_eq!(
            button.bounds(),
            Some(accesskit::Rect::new(105.0, 47.0, 125.0, 67.0))
        );
        assert!(button.supports_action(Action::Click));
        assert!(button.supports_action(Action::Focus));
        assert!(routes.contains_key(&focus));
    }

    #[test]
    fn equal_dom_ids_in_two_panes_never_collide() {
        let (left, _, _) = namespace_tree(
            PaneId(1),
            1,
            "left",
            Rect::new(0.0, 0.0, 100.0, 100.0),
            local_tree().0,
            local_tree().1,
            false,
        );
        let (right, _, _) = namespace_tree(
            PaneId(2),
            1,
            "right",
            Rect::new(100.0, 0.0, 100.0, 100.0),
            local_tree().0,
            local_tree().1,
            false,
        );
        assert!(
            left.nodes
                .iter()
                .all(|(left, _)| { right.nodes.iter().all(|(right, _)| left != right) })
        );
    }

    #[test]
    fn a_new_admission_never_reuses_the_old_platform_action_id() {
        let (_, first, _) = namespace_tree(
            PaneId(7),
            1,
            "provider",
            Rect::new(0.0, 0.0, 100.0, 100.0),
            local_tree().0,
            local_tree().1,
            false,
        );
        let (_, second, _) = namespace_tree(
            PaneId(7),
            2,
            "provider",
            Rect::new(0.0, 0.0, 100.0, 100.0),
            local_tree().0,
            local_tree().1,
            false,
        );
        assert!(first.keys().all(|id| !second.contains_key(id)));
        assert!(first.values().all(|route| route.generation == 1));
        assert!(second.values().all(|route| route.generation == 2));
    }

    #[test]
    fn queued_old_admission_cannot_activate_a_current_runner_control() {
        use crate::contributed_surface::SurfaceProviderRegistry;
        use crate::contributed_surface::tests::{provider, root_text, source};
        use crate::panes::{ContextBinding, PaneConfig, PaneKindId};
        let spec = PaneSpec {
            id: PaneId(7),
            kind: PaneKindId::new("fake"),
            source: source("fake.v1"),
            context: ContextBinding::Own,
            config: PaneConfig::empty("test.empty"),
        };
        let mut registry = SurfaceProviderRegistry::new();
        registry
            .register_provider(provider("fake", "fake.v1", "fake.surface"))
            .unwrap();
        let mut sessions = ContributedSurfaceSessions::default();
        let old_generation = sessions.resolve(&spec, &registry).unwrap().generation();
        sessions.remove(spec.id);
        let pane = sessions.resolve(&spec, &registry).unwrap();
        pane.scene(320, 180, 1.0);
        let node = pane.session().root();
        let generation = pane.generation();
        assert_ne!(old_generation, generation);
        // Resolve the old local control to the current arena deliberately:
        // generation, rather than differing NodeId arenas, must reject it.
        let queued = ContributedA11yRoute {
            pane: spec.id,
            generation: old_generation,
            node,
        };
        assert!(!apply_route(
            &mut sessions,
            Some(&spec),
            queued,
            Action::Click
        ));
        let pane = sessions.get(spec.id).unwrap();
        assert!(root_text(&pane.session().dom(), node).contains("count:0"));
        let current = ContributedA11yRoute {
            generation,
            ..queued
        };
        let repinned = PaneSpec {
            source: source("fake.v2"),
            ..spec.clone()
        };
        assert!(!apply_route(
            &mut sessions,
            Some(&repinned),
            current,
            Action::Click
        ));
        assert!(!apply_route(&mut sessions, None, current, Action::Click));
        assert!(apply_route(
            &mut sessions,
            Some(&spec),
            current,
            Action::Click
        ));
        let pane = sessions.get(spec.id).unwrap();
        assert!(root_text(&pane.session().dom(), node).contains("count:1"));
    }

    #[test]
    fn helper_refuses_hidden_disabled_and_unsupported_runner_actions() {
        use crate::contributed_surface::SurfaceProviderRegistry;
        use crate::contributed_surface::tests::{provider, root_text, source};
        use crate::panes::{ContextBinding, PaneConfig, PaneKindId};
        use layout_dom_api::{LayoutDomMut, QualName};
        for attribute in ["aria-hidden", "disabled"] {
            let mut registry = SurfaceProviderRegistry::new();
            registry
                .register_provider(provider("fake", "fake.v1", "fake.surface"))
                .unwrap();
            let spec = PaneSpec {
                id: PaneId(7),
                kind: PaneKindId::new("fake"),
                source: source("fake.v1"),
                context: ContextBinding::Own,
                config: PaneConfig::empty("test.empty"),
            };
            let mut sessions = ContributedSurfaceSessions::default();
            let pane = sessions.resolve(&spec, &registry).unwrap();
            pane.scene(320, 180, 1.0);
            let node = pane.session().root();
            pane.session().dom().borrow_mut().set_attribute(
                node,
                QualName::new(None, "".into(), attribute.into()),
                if attribute == "aria-hidden" {
                    "true"
                } else {
                    ""
                },
            );
            pane.scene(320, 180, 1.0);
            let route = ContributedA11yRoute {
                pane: spec.id,
                generation: pane.generation(),
                node,
            };
            for action in [Action::Click, Action::Focus, Action::Increment] {
                assert!(
                    !apply_route(&mut sessions, Some(&spec), route, action),
                    "{attribute}: {action:?} must be refused by the delivery helper"
                );
            }
            let pane = sessions.get(spec.id).unwrap();
            assert_eq!(pane.session().focus(), None, "{attribute}");
            assert!(
                root_text(&pane.session().dom(), node).contains("count:0"),
                "{attribute}"
            );
        }
    }

    fn focus_admission_fixture(
        suppress_host_effects: bool,
    ) -> (ContributedSurfaceSessions, PaneSpec, ContributedA11yRoute) {
        use crate::contributed_surface::SurfaceProviderRegistry;
        use crate::contributed_surface::tests::{provider, source};
        use crate::panes::{ContextBinding, PaneConfig, PaneKindId};
        let spec = PaneSpec {
            id: PaneId(7),
            kind: PaneKindId::new("fake"),
            source: source("fake.v1"),
            context: ContextBinding::Own,
            config: PaneConfig::empty("test.empty"),
        };
        let mut registry = SurfaceProviderRegistry::new();
        let mut provider = provider("fake", "fake.v1", "fake.surface");
        provider.suppress_host_effects = suppress_host_effects;
        registry.register_provider(provider).unwrap();
        let mut sessions = ContributedSurfaceSessions::default();
        let pane = sessions.resolve(&spec, &registry).unwrap();
        pane.scene(320, 180, 1.0);
        let route = ContributedA11yRoute {
            pane: spec.id,
            generation: pane.generation(),
            node: pane.session().root(),
        };
        (sessions, spec, route)
    }

    #[test]
    fn current_runner_focus_is_admitted_repeatedly_without_activation() {
        use crate::contributed_surface::tests::root_text;
        let (mut sessions, spec, route) = focus_admission_fixture(false);
        assert!(!apply_route(
            &mut sessions,
            Some(&spec),
            route,
            Action::Increment
        ));
        assert_eq!(sessions.get(spec.id).unwrap().session().focus(), None);
        for _ in 0..2 {
            assert!(apply_route(
                &mut sessions,
                Some(&spec),
                route,
                Action::Focus
            ));
            let pane = sessions.get(spec.id).unwrap();
            assert_eq!(pane.session().focus(), Some(route.node));
            assert!(root_text(&pane.session().dom(), route.node).contains("count:0"));
        }
    }

    #[test]
    fn test_provider_without_host_effects_still_admits_current_actions() {
        use crate::contributed_surface::SurfaceRequest;
        use crate::contributed_surface::tests::root_text;
        let (mut sessions, spec, route) = focus_admission_fixture(true);
        let pane = sessions.get_mut(spec.id).unwrap();
        assert_eq!(
            pane.accessibility_action(Action::Focus, route.node),
            Some(SurfaceRequest::None)
        );
        assert!(apply_route(
            &mut sessions,
            Some(&spec),
            route,
            Action::Focus
        ));
        assert_eq!(
            sessions.get(spec.id).unwrap().session().focus(),
            Some(route.node)
        );
        assert!(apply_route(
            &mut sessions,
            Some(&spec),
            route,
            Action::Click
        ));
        let pane = sessions.get(spec.id).unwrap();
        assert!(root_text(&pane.session().dom(), route.node).contains("count:1"));
    }

    #[test]
    fn an_unfocused_dom_does_not_invent_focus_and_its_root_uses_the_pane_rect() {
        let (tree, _, focus) = namespace_tree(
            PaneId(3),
            1,
            "Knot document",
            Rect::new(30.0, 50.0, 400.0, 250.0),
            local_tree().0,
            local_tree().1,
            false,
        );
        assert_eq!(focus, None);
        let (_, root) = tree
            .nodes
            .iter()
            .find(|(id, _)| *id == tree.root)
            .expect("namespaced root");
        assert_eq!(
            root.bounds(),
            Some(accesskit::Rect::new(30.0, 50.0, 430.0, 300.0))
        );
    }
}
