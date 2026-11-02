// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

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
fn native_graft_composes_nested_transforms_and_keeps_dom_focus() {
    use accesskit_consumer::{NodeRef as ConsumerNode, Tree as ConsumerTree, TreeChangeHandler};
    struct Changes;
    impl TreeChangeHandler for Changes {
        fn node_added(&mut self, _: &ConsumerNode<'_>) {}
        fn node_updated(&mut self, _: &ConsumerNode<'_>, _: &ConsumerNode<'_>) {}
        fn node_removed(&mut self, _: &ConsumerNode<'_>) {}
        fn focus_moved(&mut self, _: Option<&ConsumerNode<'_>>, _: Option<&ConsumerNode<'_>>) {}
    }
    let (mut update, actions) = local_tree();
    update.nodes[0]
        .1
        .set_transform(accesskit::Affine::translate((2.0, 1.0)));
    update.nodes[1]
        .1
        .set_transform(accesskit::Affine::translate((3.0, 4.0)));
    let (tree, guest, _, focus) = namespace_tree(
        PaneId(9),
        1,
        "pane",
        Rect::new(100.0, 40.0, 300.0, 200.0),
        update,
        actions,
        true,
    );
    let mut root = accesskit::Node::new(Role::Window);
    root.set_children(vec![tree.root]);
    let mut nodes = tree.nodes;
    nodes.push((NodeId(99), root));
    let host = TreeUpdate {
        nodes,
        tree: Some(Tree::new(NodeId(99))),
        tree_id: TreeId::ROOT,
        focus: focus.unwrap(),
    };
    let composition = uxtree::graft::Composition::new(host, vec![(guest.tree_id, guest)]).unwrap();
    let mut consumer = ConsumerTree::new(composition.initial_host(), true);
    for update in uxtree::graft::Grafts::new().frame(composition) {
        consumer.update_and_process_changes(update, &mut Changes);
    }
    let focused = consumer.state().focus().unwrap();
    assert_eq!(focused.label().as_deref(), Some("Save"));
    assert_eq!(
        focused.bounding_box(),
        Some(accesskit::Rect::new(110.0, 52.0, 130.0, 72.0))
    );
}

#[test]
fn contributed_relations_resolve_inside_the_published_tree() {
    let (mut update, actions) = local_tree();
    update.nodes[0].1.set_labelled_by(vec![NodeId(1)]);
    let (_, tree, _, _) = namespace_tree(
        PaneId(9),
        1,
        "document",
        Rect::new(100.0, 40.0, 300.0, 200.0),
        update,
        actions,
        false,
    );
    let button = tree
        .nodes
        .iter()
        .find(|(_, n)| n.role() == Role::Button)
        .unwrap();
    assert!(
        button
            .1
            .labelled_by()
            .iter()
            .all(|id| tree.nodes.iter().any(|(n, _)| n == id))
    );
}

#[test]
fn namespacing_places_bounds_and_preserves_distinct_actions() {
    let (tree, guest, routes, focus) = namespace_tree(
        PaneId(9),
        1,
        "Knot document",
        Rect::new(100.0, 40.0, 300.0, 200.0),
        local_tree().0,
        local_tree().1,
        true,
    );
    assert_eq!(focus, Some(tree.root));
    let focus = guest.focus;
    let (_, button) = guest
        .nodes
        .iter()
        .find(|(id, _)| *id == focus)
        .expect("focused button");
    assert_eq!(
        button.bounds(),
        Some(accesskit::Rect::new(5.0, 7.0, 25.0, 27.0))
    );
    assert!(button.supports_action(Action::Click));
    assert!(button.supports_action(Action::Focus));
    assert!(routes.contains_key(&focus));
}

#[test]
fn equal_dom_ids_in_two_panes_never_collide() {
    let (left, left_guest, _, _) = namespace_tree(
        PaneId(1),
        1,
        "left",
        Rect::new(0.0, 0.0, 100.0, 100.0),
        local_tree().0,
        local_tree().1,
        false,
    );
    let (right, right_guest, _, _) = namespace_tree(
        PaneId(2),
        1,
        "right",
        Rect::new(100.0, 0.0, 100.0, 100.0),
        local_tree().0,
        local_tree().1,
        false,
    );
    assert_ne!(left_guest.tree_id, right_guest.tree_id);
    assert_eq!(left_guest.nodes[0].0, right_guest.nodes[0].0);
    assert!(
        left.nodes
            .iter()
            .all(|(left, _)| { right.nodes.iter().all(|(right, _)| left != right) })
    );
}

#[test]
fn a_new_admission_never_reuses_the_old_platform_action_id() {
    let (_, first_guest, first, _) = namespace_tree(
        PaneId(7),
        1,
        "provider",
        Rect::new(0.0, 0.0, 100.0, 100.0),
        local_tree().0,
        local_tree().1,
        false,
    );
    let (_, second_guest, second, _) = namespace_tree(
        PaneId(7),
        2,
        "provider",
        Rect::new(0.0, 0.0, 100.0, 100.0),
        local_tree().0,
        local_tree().1,
        false,
    );
    assert_ne!(first_guest.tree_id, second_guest.tree_id);
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
    let (tree, _, _, focus) = namespace_tree(
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
        Some(accesskit::Rect::new(0.0, 0.0, 400.0, 250.0))
    );
}
