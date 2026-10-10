// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Controller-owned document semantics and placement-bound assistive actions.
use crate::surface::Surface;
use accesskit::{
    Action, HasPopup, Live, Node, NodeId, Orientation, Role, Toggled, TreeId, TreeUpdate,
};
use inker::{DocumentA11yAction, DocumentA11yNodeId, DocumentA11yProjection, DocumentA11yRole};
use pelt_core::PeltSessionIdentity;

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(super) struct DocumentRoute {
    pub key: uuid::Uuid,
    pub identity: PeltSessionIdentity,
    pub revision: u64,
    pub local: DocumentA11yNodeId,
    pub placement: Surface,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PoolDocumentRoute {
    pub key: uuid::Uuid,
    pub local: DocumentA11yNodeId,
    pub placement: Surface,
    pub snapshot:
        pelt_core::PeltPlacementA11y<super::content_pool::ContentKey, crate::surface::SurfaceId>,
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

fn projection_tree(projection: &DocumentA11yProjection) -> Option<TreeUpdate> {
    let mut seen = std::collections::HashSet::new();
    let mut pending = vec![projection.root()];
    let mut nodes = Vec::new();
    let mut focus = NodeId(projection.root().get());
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            return None;
        }
        let semantic = projection.node(id)?;
        let mut node = Node::new(
            if semantic.role == DocumentA11yRole::TextField && semantic.state.multiline {
                Role::MultilineTextInput
            } else {
                accesskit_role(semantic.role)
            },
        );
        if let DocumentA11yRole::Heading { level } = semantic.role
            && level > 0
        {
            node.set_level(usize::from(level));
        }
        if let Some(name) = &semantic.name {
            node.set_label(name.clone());
        }
        if let Some(description) = &semantic.description {
            node.set_description(description.clone());
        }
        if let Some(value) = &semantic.value {
            node.set_value(value.clone());
        }
        if semantic.state.disabled {
            node.set_disabled();
        }
        if semantic.state.hidden {
            node.set_hidden();
        }
        if semantic.state.read_only {
            node.set_read_only();
        }
        if semantic.state.required {
            node.set_required();
        }
        if let Some(selected) = semantic.state.selected {
            node.set_selected(selected);
        }
        if let Some(expanded) = semantic.state.expanded {
            node.set_expanded(expanded);
        }
        if let Some(toggled) = semantic.state.toggled {
            node.set_toggled(match toggled {
                inker::DocumentA11yToggled::On => Toggled::True,
                inker::DocumentA11yToggled::Off => Toggled::False,
                inker::DocumentA11yToggled::Mixed => Toggled::Mixed,
            });
        } else if let Some(checked) = semantic.state.checked {
            node.set_toggled(if checked {
                Toggled::True
            } else {
                Toggled::False
            });
        }
        if let Some(live) = semantic.state.live {
            node.set_live(match live {
                inker::DocumentA11yLive::Off => Live::Off,
                inker::DocumentA11yLive::Polite => Live::Polite,
                inker::DocumentA11yLive::Assertive => Live::Assertive,
            });
        }
        if let Some(orientation) = semantic.state.orientation {
            node.set_orientation(match orientation {
                inker::DocumentA11yOrientation::Horizontal => Orientation::Horizontal,
                inker::DocumentA11yOrientation::Vertical => Orientation::Vertical,
            });
        }
        if let Some(has_popup) = semantic.state.has_popup {
            node.set_has_popup(match has_popup {
                inker::DocumentA11yHasPopup::Menu => HasPopup::Menu,
                inker::DocumentA11yHasPopup::ListBox => HasPopup::Listbox,
                inker::DocumentA11yHasPopup::Tree => HasPopup::Tree,
                inker::DocumentA11yHasPopup::Grid => HasPopup::Grid,
                inker::DocumentA11yHasPopup::Dialog => HasPopup::Dialog,
            });
        }
        if let Some(value) = semantic.numeric_value {
            node.set_numeric_value(value);
        }
        if let Some(value) = semantic.numeric_minimum {
            node.set_min_numeric_value(value);
        }
        if let Some(value) = semantic.numeric_maximum {
            node.set_max_numeric_value(value);
        }
        if let Some(bounds) = semantic.bounds {
            node.set_bounds(accesskit::Rect::new(
                f64::from(bounds.x),
                f64::from(bounds.y),
                f64::from(bounds.x + bounds.width),
                f64::from(bounds.y + bounds.height),
            ));
        }

        node.set_children(
            semantic
                .children
                .iter()
                .map(|id| NodeId(id.get()))
                .collect::<Vec<_>>(),
        );
        accesskit_actions(&semantic.actions, &mut node);
        if semantic.state.focused && !semantic.state.hidden {
            focus = NodeId(id.get());
        }
        if id == projection.root() {
            let support = projection.support();
            let description = format!(
                "{:?}: {}",
                support.capability(),
                support.limitations().join(" ")
            );
            node.set_description(match node.description() {
                Some(existing) => format!("{existing}; {description}"),
                None => description,
            });
        }
        nodes.push((NodeId(id.get()), node));
        pending.extend(semantic.children.iter().rev().copied());
    }
    Some(TreeUpdate {
        nodes,
        tree: Some(accesskit::Tree::new(NodeId(projection.root().get()))),
        tree_id: TreeId::ROOT,
        focus,
    })
}

fn accesskit_role(role: DocumentA11yRole) -> Role {
    match role {
        DocumentA11yRole::Window => Role::Window,
        DocumentA11yRole::Document => Role::Document,
        DocumentA11yRole::Article => Role::Article,
        DocumentA11yRole::Region => Role::Region,
        DocumentA11yRole::Group => Role::Group,
        DocumentA11yRole::Navigation => Role::Navigation,
        DocumentA11yRole::Main => Role::Main,
        DocumentA11yRole::Heading { .. } => Role::Heading,
        DocumentA11yRole::Paragraph => Role::Paragraph,
        DocumentA11yRole::StaticText => Role::TextRun,
        DocumentA11yRole::Link => Role::Link,
        DocumentA11yRole::Button => Role::Button,
        DocumentA11yRole::TextField => Role::TextInput,
        DocumentA11yRole::CheckBox => Role::CheckBox,
        DocumentA11yRole::RadioButton => Role::RadioButton,
        DocumentA11yRole::RadioGroup => Role::RadioGroup,
        DocumentA11yRole::Switch => Role::Switch,
        DocumentA11yRole::ComboBox => Role::ComboBox,
        DocumentA11yRole::List => Role::List,
        DocumentA11yRole::ListItem => Role::ListItem,
        DocumentA11yRole::ListBox => Role::ListBox,
        DocumentA11yRole::ListBoxOption => Role::ListBoxOption,
        DocumentA11yRole::Table => Role::Table,
        DocumentA11yRole::Row => Role::Row,
        DocumentA11yRole::Cell => Role::Cell,
        DocumentA11yRole::Image => Role::Image,
        DocumentA11yRole::Form => Role::Form,
        DocumentA11yRole::Dialog => Role::Dialog,
        DocumentA11yRole::Alert => Role::Alert,
        DocumentA11yRole::Menu => Role::Menu,
        DocumentA11yRole::MenuItem => Role::MenuItem,
        DocumentA11yRole::MenuItemCheckBox => Role::MenuItemCheckBox,
        DocumentA11yRole::MenuItemRadio => Role::MenuItemRadio,
        DocumentA11yRole::TabList => Role::TabList,
        DocumentA11yRole::Tab => Role::Tab,
        DocumentA11yRole::TabPanel => Role::TabPanel,
        DocumentA11yRole::Tree => Role::Tree,
        DocumentA11yRole::TreeItem => Role::TreeItem,
        DocumentA11yRole::Slider => Role::Slider,
        DocumentA11yRole::SpinButton => Role::SpinButton,
        DocumentA11yRole::Splitter => Role::Splitter,
        DocumentA11yRole::Toolbar => Role::Toolbar,
        DocumentA11yRole::ProgressIndicator => Role::ProgressIndicator,
        DocumentA11yRole::Label => Role::Label,
        DocumentA11yRole::Status => Role::Status,
        DocumentA11yRole::Log => Role::Log,
        DocumentA11yRole::Note => Role::Note,
        DocumentA11yRole::Unknown => Role::GenericContainer,
    }
}

fn accesskit_actions(actions: &[DocumentA11yAction], node: &mut Node) {
    for action in actions {
        node.add_action(match action {
            DocumentA11yAction::Click => Action::Click,
            DocumentA11yAction::Focus => Action::Focus,
            DocumentA11yAction::SetValue => Action::SetValue,
            DocumentA11yAction::ScrollIntoView => Action::ScrollIntoView,
            DocumentA11yAction::Increment => Action::Increment,
            DocumentA11yAction::Decrement => Action::Decrement,
        });
    }
}

#[cfg(test)]
mod tests {
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
        assert!(update.nodes.iter().any(|(id, n)| id.0 == link.id.get() && n.supports_action(accesskit::Action::Focus)));
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
}

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

/// Replace the spawn outline, including its now-unreachable descendants.
fn replace_document(tree: &mut uxtree::UxTree, root: NodeId, replacement: Option<uxtree::UxTree>) {
    let mut pending = vec![root];
    let mut retired = std::collections::HashSet::new();
    while let Some(id) = pending.pop() {
        if retired.insert(id)
            && let Some((_, node)) = tree.nodes.iter().find(|(n, _)| *n == id)
        {
            pending.extend(node.children().iter().copied());
        }
    }
    tree.nodes.retain(|(id, _)| !retired.contains(id));
    if let Some(replacement) = replacement {
        // The document root keeps its host identity and position among siblings.
        if !tree
            .nodes
            .iter()
            .any(|(_, node)| node.children().contains(&root))
        {
            if let Some((_, node)) = tree.nodes.iter_mut().find(|(id, _)| *id == tree.root) {
                let mut children = node.children().to_vec();
                children.push(root);
                node.set_children(children);
            }
        }
        tree.nodes.extend(replacement.nodes);
    } else {
        for (_, node) in &mut tree.nodes {
            let children: Vec<_> = node
                .children()
                .iter()
                .copied()
                .filter(|id| !retired.contains(id))
                .collect();
            node.set_children(children);
        }
    }
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
                let graft = uxtree::graft::graft_node(
                    Role::Group,
                    tree_id,
                    (placement.rect.x as f64, placement.rect.y as f64),
                    (placement.rect.w as f64, placement.rect.h as f64),
                );
                for semantic in projection.nodes().iter().filter(|n| !n.actions.is_empty()) {
                    frame.admissions.insert(
                        (tree_id, NodeId(semantic.id.get())),
                        super::a11y_bridge::Admission::PoolDocument(PoolDocumentRoute {
                            key,
                            local: semantic.id,
                            placement,
                            snapshot: snapshot.clone(),
                        }),
                    );
                }
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
