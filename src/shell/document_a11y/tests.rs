// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;
use pelt_core::{PeltController, PeltControllerConfig};
fn page(address: &str, engine: &str, body: &str) -> PeltController<netrender::Scene> {
    let request = inker::SessionSpawnRequest::new(address)
        .with_body(body)
        .with_viewport(640, 480);
    let mut controller = PeltController::new(
        super::super::standard_content_engines(),
        inker::SurfaceEngineRegistry::new(),
        PeltControllerConfig::from_request(engine, request).with_host_history(),
        super::super::ShellClock(std::time::Instant::now()),
    )
    .unwrap();
    controller.frame(640, 480);
    controller
}
#[test]
fn real_field_accepts_value_once_then_refuses_the_stale_admission() {
    let mut controller = page(
        "file:///form.html",
        inker::routing::ENGINE_GENET_LIVERY,
        "<html><body><input name='name' value='seed'></body></html>",
    );
    let projection = controller.accessibility_projection().unwrap();
    let local = projection
        .nodes()
        .iter()
        .find(|n| n.role == DocumentA11yRole::TextField)
        .unwrap()
        .id;
    let key = uuid::Uuid::from_u128(8);
    let placement = Surface {
        id: crate::surface::SurfaceId(9),
        kind: crate::surface::SurfaceKind::Content(key),
        rect: crate::surface::Rect::new(0.0, 0.0, 640.0, 480.0),
    };
    let route = DocumentRoute {
        key,
        identity: controller.session_identity(),
        revision: projection.revision(),
        local,
        placement,
    };
    assert!(!dispatch_nonpointer(
        &mut controller,
        &route,
        Some(placement),
        DocumentA11yAction::SetValue,
        None
    ));
    assert!(dispatch_nonpointer(
        &mut controller,
        &route,
        Some(placement),
        DocumentA11yAction::SetValue,
        Some(inker::DocumentA11yActionData::Value("typed".into()))
    ));
    assert_eq!(
        controller
            .accessibility_projection()
            .unwrap()
            .node(local)
            .unwrap()
            .value
            .as_deref(),
        Some("typed")
    );
    assert!(!dispatch_nonpointer(
        &mut controller,
        &route,
        Some(placement),
        DocumentA11yAction::SetValue,
        Some(inker::DocumentA11yActionData::Value("stale".into()))
    ));
    assert_eq!(controller.address(), "file:///form.html");
}

#[test]
fn source_click_uses_neutral_pointer_and_hands_navigation_to_the_host_once() {
    let mut controller = page(
        "gemini://fixture.test/",
        inker::routing::ENGINE_NEMATIC_GEMTEXT,
        "=> /next Follow next\n",
    );
    let projection = controller.accessibility_projection().unwrap();
    let link = projection
        .nodes()
        .iter()
        .find(|n| n.role == DocumentA11yRole::Link)
        .unwrap();
    let target = controller.accessibility_click_target(link.id).unwrap();
    assert_eq!(target.revision, projection.revision());
    let navigations: Vec<_> = click_inputs(target.point)
        .into_iter()
        .filter_map(|input| controller.input(input).navigation)
        .collect();
    assert_eq!(navigations.len(), 1);
    assert_eq!(navigations[0].request.address, "gemini://fixture.test/next");
    assert_eq!(controller.address(), "gemini://fixture.test/");
}

#[test]
fn document_namespaces_separate_keys_instances_generations_and_placements() {
    let key = uuid::Uuid::from_u128(1);
    let identity = PeltSessionIdentity {
        instance_id: 2,
        generation: 3,
    };
    let placement = crate::surface::SurfaceId(4);
    let original = guest_id(key, identity, placement);
    for other in [
        guest_id(uuid::Uuid::from_u128(2), identity, placement),
        guest_id(
            key,
            PeltSessionIdentity {
                instance_id: 4,
                ..identity
            },
            placement,
        ),
        guest_id(
            key,
            PeltSessionIdentity {
                generation: 4,
                ..identity
            },
            placement,
        ),
        guest_id(key, identity, crate::surface::SurfaceId(5)),
    ] {
        assert_ne!(original, other);
    }
    assert_ne!(original, TreeId::ROOT);
    assert_eq!(original, guest_id(key, identity, placement));
}

#[test]
fn replacing_an_outline_removes_retired_descendants_and_hidden_document() {
    let mut root = Node::new(Role::Window);
    root.set_children(vec![NodeId(2)]);
    let mut old = Node::new(Role::Document);
    old.set_children(vec![NodeId(3)]);
    let mut tree = uxtree::UxTree {
        root: NodeId(1),
        nodes: vec![
            (NodeId(1), root),
            (NodeId(2), old),
            (NodeId(3), Node::new(Role::Link)),
        ],
    };
    let mut fallback = Node::new(Role::Document);
    fallback.set_description("Partial: no live projection");
    replace_document(
        &mut tree,
        NodeId(2),
        Some(uxtree::UxTree {
            root: NodeId(2),
            nodes: vec![(NodeId(2), fallback)],
        }),
    );
    assert!(!tree.nodes.iter().any(|(id, _)| *id == NodeId(3)));
    assert_eq!(tree.nodes.len(), 2);
    replace_document(&mut tree, NodeId(2), None);
    assert!(tree.nodes[0].1.children().is_empty());
    assert_eq!(tree.nodes.len(), 1);
}

#[test]
fn actual_field_semantics_keep_value_and_supported_actions() {
    let controller = page(
        "file:///form.html",
        inker::routing::ENGINE_GENET_LIVERY,
        "<html><body><label for='name'>Name</label><input id='name' name='name' value='seed'></body></html>",
    );
    let projection = controller.accessibility_projection().unwrap();
    let field = projection
        .nodes()
        .iter()
        .find(|n| n.role == inker::DocumentA11yRole::TextField)
        .expect("source field");
    let update = projection_tree(&projection).expect("published document");
    let node = &update
        .nodes
        .iter()
        .find(|(id, _)| id.0 == field.id.get())
        .unwrap()
        .1;
    assert_eq!(node.role(), accesskit::Role::TextInput);
    assert_eq!(node.value(), Some("seed"));
    assert!(node.supports_action(accesskit::Action::Focus));
    assert!(node.supports_action(accesskit::Action::SetValue));
}

#[test]
fn micron_fields_remain_partial_inert_source_text() {
    let controller = page(
        "file:///fixture.mu",
        crate::nomadnet::ENGINE_ID,
        "Name: `<name`seed>\n`[Next`0123456789abcdef0123456789abcdef:/page/next]\n",
    );
    let projection = controller.accessibility_projection().unwrap();
    assert_eq!(
        projection.support().capability(),
        inker::A11yCapability::Partial
    );
    let update = projection_tree(&projection).expect("published document");
    assert!(
        !update
            .nodes
            .iter()
            .any(|(_, n)| n.role() == accesskit::Role::TextInput)
    );
    assert!(
        !update
            .nodes
            .iter()
            .any(|(_, n)| n.supports_action(accesskit::Action::SetValue))
    );
    let root = update.tree.unwrap().root;
    let description = update
        .nodes
        .iter()
        .find(|(id, _)| *id == root)
        .unwrap()
        .1
        .description()
        .unwrap();
    assert!(description.contains("Partial"));
    for limitation in projection.support().limitations() {
        assert!(description.contains(limitation));
    }
}

#[test]
fn document_action_admission_checks_identity_revision_and_visible_placement() {
    let mut controller = page(
        "gemini://fixture.test/",
        inker::routing::ENGINE_NEMATIC_GEMTEXT,
        "=> /next Follow this link\n",
    );
    let projection = controller.accessibility_projection().unwrap();
    let local = projection
        .nodes()
        .iter()
        .find(|n| n.role == inker::DocumentA11yRole::Link)
        .unwrap()
        .id;
    let key = uuid::Uuid::from_u128(7);
    let placement = Surface {
        id: crate::surface::SurfaceId(9),
        kind: crate::surface::SurfaceKind::Content(key),
        rect: crate::surface::Rect::new(10.0, 20.0, 640.0, 480.0),
    };
    let route = DocumentRoute {
        key,
        identity: controller.session_identity(),
        revision: projection.revision(),
        local,
        placement,
    };
    assert!(route_is_current(
        &route,
        &controller,
        Some(placement),
        DocumentA11yAction::Click
    ));
    assert!(!route_is_current(
        &route,
        &controller,
        None,
        DocumentA11yAction::Click
    ));
    let moved = Surface {
        rect: crate::surface::Rect::new(11.0, 20.0, 640.0, 480.0),
        ..placement
    };
    assert!(!route_is_current(
        &route,
        &controller,
        Some(moved),
        DocumentA11yAction::Click
    ));
    assert!(!route_is_current(
        &route,
        &controller,
        Some(placement),
        DocumentA11yAction::SetValue
    ));
    for identity in [
        PeltSessionIdentity {
            instance_id: route.identity.instance_id + 1,
            ..route.identity
        },
        PeltSessionIdentity {
            generation: route.identity.generation + 1,
            ..route.identity
        },
    ] {
        assert!(!route_is_current(
            &DocumentRoute {
                identity,
                ..route.clone()
            },
            &controller,
            Some(placement),
            DocumentA11yAction::Click
        ));
    }
    controller.dispatch_accessibility_action(&inker::DocumentA11yActionRequest {
        revision: route.revision,
        target: local,
        action: DocumentA11yAction::Focus,
        data: None,
    });
    assert!(!route_is_current(
        &route,
        &controller,
        Some(placement),
        DocumentA11yAction::Click
    ));
}

#[test]
fn projected_focus_never_performs_a_link_click() {
    let mut controller = page(
        "gemini://fixture.test/",
        inker::routing::ENGINE_NEMATIC_GEMTEXT,
        "=> /next Follow this link\n",
    );
    let projection = controller.accessibility_projection().unwrap();
    let update = projection_tree(&projection).expect("published document");
    let link = projection
        .nodes()
        .iter()
        .find(|n| n.role == inker::DocumentA11yRole::Link)
        .unwrap();
    assert!(
        update
            .nodes
            .iter()
            .any(|(id, n)| id.0 == link.id.get() && n.supports_action(accesskit::Action::Focus))
    );
    assert!(
        controller.dispatch_accessibility_action(&inker::DocumentA11yActionRequest {
            revision: projection.revision(),
            target: link.id,
            action: DocumentA11yAction::Focus,
            data: None,
        })
    );
    assert_eq!(controller.address(), "gemini://fixture.test/");
    let focused = controller.accessibility_projection().unwrap();
    assert!(focused.node(link.id).unwrap().state.focused);
}

#[test]
fn real_protocol_projections_keep_link_names_in_document_order() {
    for (address, engine, body, names) in [
        (
            "gemini://fixture.test/",
            inker::routing::ENGINE_NEMATIC_GEMTEXT,
            "# Capsule\n=> /first First Gemini\n=> /second Second Gemini\n",
            ["First Gemini", "Second Gemini"],
        ),
        (
            "gopher://fixture.test/1/",
            inker::routing::ENGINE_NEMATIC_GOPHER,
            "1First Gopher\t/first\tfixture.test\t70\r\n1Second Gopher\t/second\tfixture.test\t70\r\n.\r\n",
            ["First Gopher", "Second Gopher"],
        ),
        (
            "file:///fixture.mu",
            crate::nomadnet::ENGINE_ID,
            ">Page\n`[First Micron`0123456789abcdef0123456789abcdef:/page/first]\n`[Second Micron`0123456789abcdef0123456789abcdef:/page/second]\n",
            ["First Micron", "Second Micron"],
        ),
    ] {
        let controller = page(address, engine, body);
        let projection = controller
            .accessibility_projection()
            .expect("laid-out source");
        let update = projection_tree(&projection).expect("host publishes source projection");
        let tree = uxtree::UxTree {
            root: update.tree.unwrap().root,
            nodes: update.nodes,
        };
        let links: Vec<_> = crate::a11y::tree_lines(&tree)
            .into_iter()
            .filter(|line| line.starts_with("link:"))
            .collect();
        assert_eq!(
            links,
            names.map(|name| format!("link: {name}")),
            "{address}"
        );
    }
}
