// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0
#[path = "../../../../../src/frame_a11y.rs"]
mod frame_a11y;
#[path = "../../../../../src/frame_controls.rs"]
mod frame_controls;
#[path = "../../../../../src/frame_style.rs"]
mod frame_style;
#[path = "../../../../../src/scenario.rs"]
mod scenario;

#[path = "../../../../../src/foreign_a11y.rs"]
mod foreign_a11y;
#[path = "../../../../../src/window_projection.rs"]
mod window_projection;
#[path = "../../../../../src/panes/workspace.rs"]
mod workspace;
#[path = "../../../../../src/panes/workspace_store.rs"]
mod workspace_store;

#[cfg(test)]
mod tests {
    use super::{
        frame_a11y,
        frame_controls::{Control, FrameControls},
        frame_style,
    };
    use accesskit::{Action, Node, NodeId, Role};
    use cambium::{Key, KeyEvent, NamedKey};
    use genet_livery::{Device, InteractionStates, StyleSet, TextSystem};
    use layout_dom_api::LayoutDom;
    use std::collections::{HashMap, HashSet};
    use workbench::{
        ContentSource, SplitAxis, TabStack, Tile, TileBranch, TileEvent, TileId, TilePath,
        TileTree, Workspace, WorkspaceEvent,
    };
    #[test]
    fn native_fixture_controls_and_keys_parse_through_actual_parsers() {
        for source in [
            include_str!("../../../../../scenarios/frame_controls_keyboard.scn"),
            include_str!("../../../../../scenarios/frame_controls_accessibility.scn"),
            include_str!("../../../../../scenarios/frame_controls_reopen.scn"),
            include_str!("../../../../../scenarios/frame_stations.scn"),
            include_str!("../../../../../scenarios/frame_stations_reopen.scn"),
            include_str!("../../../../../scenarios/frame_lens_accessibility.scn"),
        ] {
            taproot::Scenario::parse(source).unwrap();
            for line in source.lines().filter(|line| line.starts_with("key ")) {
                super::scenario::parse(line).unwrap();
            }
            for line in source
                .lines()
                .filter_map(|line| line.strip_prefix("frame-a11y "))
            {
                frame_a11y::fixture_request(line).unwrap();
            }
        }
        for source in [
            include_str!("../../../../../scenarios/frame_stations_reopen.scn"),
            include_str!("../../../../../scenarios/frame_lens_accessibility.scn"),
        ] {
            for line in source
                .lines()
                .filter_map(|l| l.strip_prefix("frame-a11y-window "))
            {
                frame_a11y::fixture_window_request(line).unwrap();
            }
        }
        for text in [
            "-1 focus tab 0",
            "1 focus close 2 extra",
            "9 erase tab 1",
            "missing focus tab 0",
        ] {
            assert!(frame_a11y::fixture_window_request(text).is_err());
        }
        for text in [
            "focus tab 0 extra",
            "click close -1",
            "inc divider nope 0",
            "erase tab 0",
        ] {
            assert!(frame_a11y::fixture_request(text).is_err());
        }
    }
    fn tile(id: u64) -> Tile {
        Tile {
            id: TileId(id),
            title: format!("Page {id}"),
            content: ContentSource::Open {
                kind: "test".into(),
                id: id.to_string(),
            },
            accent: None,
        }
    }
    fn stack() -> Workspace {
        Workspace::new(TileTree::Stack(TabStack {
            tabs: vec![tile(1), tile(2), tile(3)],
            active: 0,
        }))
    }
    fn split() -> Workspace {
        Workspace::new(TileTree::Split {
            axis: SplitAxis::Row,
            children: vec![
                TileBranch::new(0.2, TileTree::single(tile(1))),
                TileBranch::new(0.3, TileTree::single(tile(2))),
                TileBranch::new(0.5, TileTree::single(tile(3))),
            ],
        })
    }
    fn controls(w: &Workspace) -> FrameControls {
        let mut c = FrameControls::default();
        c.sync(7, w.clone(), Some(TileId(1)));
        c
    }
    fn key(c: &mut FrameControls, k: NamedKey) -> (Vec<WorkspaceEvent>, bool) {
        let order = c.order();
        c.key(KeyEvent::new(Key::Named(k)), &order)
    }
    fn install(
        c: &mut FrameControls,
        w: &mut Workspace,
        events: Vec<WorkspaceEvent>,
        current: u64,
    ) {
        for event in events {
            w.apply(&event);
        }
        c.sync(7, w.clone(), Some(TileId(current)));
    }
    fn projection(c: &FrameControls, focused: bool) -> frame_a11y::FrameProjection {
        let dom = c.dom.borrow();
        let sheet = frame_style::stylesheet(None, &Default::default(), "", 1.0);
        let device = Device::screen(800.0, 600.0);
        let styles = genet_livery::resolve_styles(
            &*dom,
            &StyleSet::cambium(&[&sheet]),
            &device,
            &InteractionStates::default(),
        );
        let (_, layout) = genet_livery::layout_with_text_system(
            &*dom,
            &styles,
            800.0,
            600.0,
            device.viewport_sizes,
            &mut TextSystem::new(),
            &Default::default(),
        )
        .unwrap();
        let neutral = genet_render::document_a11y_projection(
            &*dom,
            &layout,
            focused.then(|| c.focus_node()).flatten(),
            0,
        );
        let tree = genet_render::accesskit_tree_from_projection(neutral);
        let map = super::frame_controls::nodes(&dom)
            .into_iter()
            .map(|n| (NodeId(dom.opaque_id(n)), n))
            .collect();
        frame_a11y::project(tree, map, c, focused)
    }
    fn graft(
        c: &FrameControls,
        focused: bool,
    ) -> (
        uxtree::UxTree,
        HashMap<NodeId, super::frame_controls::FrameRoute>,
        Option<NodeId>,
    ) {
        let root = NodeId(100001);
        let body = NodeId(100002);
        let inactive = NodeId(100003);
        let mut container = Node::new(Role::Group);
        container.set_children(vec![body, inactive]);
        let mut body_node = Node::new(Role::Group);
        body_node.set_label("active body");
        let mut inactive_node = Node::new(Role::Group);
        inactive_node.set_label("inactive body");
        let mut tree = uxtree::UxTree {
            root,
            nodes: vec![
                (root, container),
                (body, body_node),
                (inactive, inactive_node),
            ],
        };
        let (routes, focus) = frame_a11y::graft(
            &mut tree,
            root,
            &[(TileId(1), body), (TileId(99), inactive)].into(),
            projection(c, focused),
        )
        .unwrap();
        (tree, routes, focus)
    }
    #[test]
    fn shared_tab_arrows_emit_host_events_and_reconcile_focus_to_selected_tile() {
        let mut w = stack();
        let mut c = controls(&w);
        c.set_focus(Some(Control::Tab(TileId(1))));
        let before = serde_json::to_string(&w).unwrap();
        let (events, exit) = key(&mut c, NamedKey::ArrowRight);
        assert!(!exit);
        assert!(matches!(
            events.as_slice(),
            [WorkspaceEvent::Tile(TileEvent::Activated(TileId(2)))]
        ));
        assert_eq!(before, serde_json::to_string(&w).unwrap());
        install(&mut c, &mut w, events, 2);
        assert_eq!(c.focus(), Some(&Control::Tab(TileId(2))));
        assert!(c.focus_node().is_some());
        let (events, _) = key(&mut c, NamedKey::End);
        install(&mut c, &mut w, events, 3);
        assert_eq!(c.focus(), Some(&Control::Tab(TileId(3))));
        let (events, _) = key(&mut c, NamedKey::Home);
        install(&mut c, &mut w, events, 1);
        assert_eq!(c.focus(), Some(&Control::Tab(TileId(1))));
    }
    #[test]
    fn close_control_enter_closes_only_its_tile_then_repairs_focus() {
        let mut w = stack();
        let mut c = controls(&w);
        c.set_focus(Some(Control::Close(TileId(1))));
        let (events, _) = key(&mut c, NamedKey::Enter);
        assert!(matches!(
            events.as_slice(),
            [WorkspaceEvent::Tile(TileEvent::Closed(TileId(1)))]
        ));
        install(&mut c, &mut w, events, 2);
        assert_eq!(c.focus(), Some(&Control::Tab(TileId(2))));
    }
    #[test]
    fn inactive_close_does_not_activate_or_drop_other_accesses() {
        let mut w = stack();
        let mut c = controls(&w);
        let events = c.click(&Control::Close(TileId(2)));
        assert!(matches!(
            events.as_slice(),
            [WorkspaceEvent::Tile(TileEvent::Closed(TileId(2)))]
        ));
        install(&mut c, &mut w, events, 1);
        let TileTree::Stack(s) = w.tiled() else {
            panic!()
        };
        assert_eq!(s.tabs[s.active].id, TileId(1));
        assert_eq!(s.tabs.len(), 2);
    }
    #[test]
    fn traversal_uses_roving_tabs_and_exits_in_both_directions() {
        let w = stack();
        let mut c = controls(&w);
        let order = c.order();
        assert!(order.contains(&Control::Tab(TileId(1))));
        assert!(!order.contains(&Control::Tab(TileId(2))));
        c.set_focus(order.last().cloned());
        assert!(key(&mut c, NamedKey::Tab).1);
        assert!(c.focus().is_none());
        c.set_focus(order.first().cloned());
        let mut ev = KeyEvent::new(Key::Named(NamedKey::Tab));
        ev.mods.shift = true;
        assert!(c.key(ev, &order).1);
        assert!(c.focus().is_none());
    }
    #[test]
    fn escape_exits_and_plain_body_hotkeys_do_not_emit_workspace_edits() {
        let w = stack();
        let mut c = controls(&w);
        c.set_focus(Some(Control::Tab(TileId(1))));
        let order = c.order();
        assert!(
            c.key(KeyEvent::new(Key::Character("i".into())), &order)
                .0
                .is_empty()
        );
        assert!(key(&mut c, NamedKey::Escape).1);
        assert!(c.focus().is_none());
    }
    #[test]
    fn ctrl_w_closes_the_exact_focused_handle() {
        let w = stack();
        let mut c = controls(&w);
        c.set_focus(Some(Control::Close(TileId(3))));
        let order = c.order();
        let mut ev = KeyEvent::new(Key::Character("w".into()));
        ev.mods.ctrl = true;
        assert!(matches!(
            c.key(ev, &order).0.as_slice(),
            [WorkspaceEvent::Tile(TileEvent::Closed(TileId(3)))]
        ));
    }
    #[test]
    fn nary_keyboard_resize_preserves_other_shares_and_boundary_focus() {
        let mut w = split();
        let mut c = controls(&w);
        let divider = c
            .order()
            .into_iter()
            .find(|c| matches!(c, Control::Divider { index: 0, .. }))
            .unwrap();
        c.set_focus(Some(divider.clone()));
        let (events, _) = key(&mut c, NamedKey::ArrowRight);
        install(&mut c, &mut w, events, 1);
        let TileTree::Split { children, .. } = w.tiled() else {
            panic!()
        };
        assert!((children[0].fraction - 0.205).abs() < 0.0001);
        assert!((children[1].fraction - 0.295).abs() < 0.0001);
        assert_eq!(children[2].fraction, 0.5);
        assert_eq!(c.focus(), Some(&divider));
        let (events, _) = key(&mut c, NamedKey::Home);
        install(&mut c, &mut w, events, 1);
        assert!((c.divider_value(&divider).unwrap() - 5.0).abs() < 0.001);
        let (events, _) = key(&mut c, NamedKey::End);
        install(&mut c, &mut w, events, 1);
        assert!((c.divider_value(&divider).unwrap() - 95.0).abs() < 0.001);
    }
    #[test]
    fn vertical_divider_uses_vertical_keys_and_shift_step() {
        let mut w = Workspace::new(TileTree::Split {
            axis: SplitAxis::Column,
            children: vec![
                TileBranch::new(0.5, TileTree::single(tile(1))),
                TileBranch::new(0.5, TileTree::single(tile(2))),
            ],
        });
        let mut c = controls(&w);
        let divider = c
            .order()
            .into_iter()
            .find(|c| matches!(c, Control::Divider { .. }))
            .unwrap();
        c.set_focus(Some(divider.clone()));
        assert!(key(&mut c, NamedKey::ArrowRight).0.is_empty());
        let mut ev = KeyEvent::new(Key::Named(NamedKey::ArrowDown));
        ev.mods.shift = true;
        let order = c.order();
        let events = c.key(ev, &order).0;
        install(&mut c, &mut w, events, 1);
        assert!((c.divider_value(&divider).unwrap() - 60.0).abs() < 0.001);
    }
    #[test]
    fn scope_and_revision_changes_reject_old_platform_routes() {
        let mut w = stack();
        let mut c = controls(&w);
        let route = c.route(Control::Close(TileId(2)));
        assert!(c.admits(&route));
        let events = c.click(&Control::Close(TileId(3)));
        install(&mut c, &mut w, events, 1);
        assert!(!c.admits(&route));
        let fresh = c.route(Control::Close(TileId(2)));
        c.sync(8, w, Some(TileId(1)));
        assert!(!c.admits(&fresh));
    }
    #[test]
    fn unchanged_sync_keeps_revision_and_focus() {
        let w = stack();
        let mut c = controls(&w);
        c.set_focus(Some(Control::Tab(TileId(1))));
        let stamp = c.stamp();
        c.sync(7, w, Some(TileId(1)));
        assert_eq!(c.stamp(), stamp);
    }
    #[test]
    fn changed_divider_membership_does_not_retarget_old_focus() {
        let mut w = split();
        let mut c = controls(&w);
        let divider = c
            .order()
            .into_iter()
            .find(|c| matches!(c, Control::Divider { .. }))
            .unwrap();
        c.set_focus(Some(divider));
        w.apply(&TileEvent::Closed(TileId(1)).into());
        c.sync(7, w, Some(TileId(2)));
        assert_eq!(c.focus(), Some(&Control::Tab(TileId(2))));
    }
    #[test]
    fn composed_tree_preserves_body_once_and_has_no_orphans_or_inactive_body() {
        let c = controls(&stack());
        let (tree, routes, _) = graft(&c, false);
        assert!(
            tree.nodes
                .iter()
                .any(|(_, n)| n.label() == Some("active body"))
        );
        assert!(
            !tree
                .nodes
                .iter()
                .any(|(_, n)| n.label() == Some("inactive body"))
        );
        let ids: HashSet<_> = tree.nodes.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids.len(), tree.nodes.len());
        let mut pending = vec![tree.root];
        let mut visited = HashSet::new();
        while let Some(id) = pending.pop() {
            assert!(visited.insert(id));
            let node = &tree.nodes.iter().find(|(n, _)| *n == id).unwrap().1;
            pending.extend(node.children());
        }
        assert_eq!(ids, visited);
        assert!(
            routes
                .values()
                .any(|r| r.control == Control::Close(TileId(2)))
        );
    }
    #[test]
    fn exact_bounds_focus_and_separator_actions_reach_the_platform_projection() {
        let mut c = controls(&split());
        let divider = c
            .order()
            .into_iter()
            .find(|c| matches!(c, Control::Divider { .. }))
            .unwrap();
        c.set_focus(Some(divider.clone()));
        let (tree, routes, focus) = graft(&c, true);
        let (id, route) = routes.iter().find(|(_, r)| r.control == divider).unwrap();
        assert!(c.admits(route));
        assert_eq!(focus, Some(*id));
        let node = &tree.nodes.iter().find(|(n, _)| n == id).unwrap().1;
        assert!(node.supports_action(Action::Increment));
        assert!(node.supports_action(Action::Decrement));
        assert!(node.supports_action(Action::Focus));
        let bounds = node.bounds().unwrap();
        assert!(bounds.x1 > bounds.x0 && bounds.y1 > bounds.y0);
        assert_eq!(node.min_numeric_value(), Some(5.0));
        assert_eq!(node.max_numeric_value(), Some(95.0));
    }
    #[test]
    fn independent_frame_doms_get_disjoint_namespaces() {
        let a = controls(&stack());
        let b = controls(&stack());
        let (a, _, _) = graft(&a, false);
        let (b, _, _) = graft(&b, false);
        let ids: HashSet<_> = a
            .nodes
            .iter()
            .filter(|(id, _)| id.0 < 100001 || id.0 > 100003)
            .map(|(id, _)| *id)
            .collect();
        assert!(
            b.nodes
                .iter()
                .filter(|(id, _)| id.0 < 100001 || id.0 > 100003)
                .all(|(id, _)| !ids.contains(id))
        );
    }
    #[test]
    fn floating_header_keyboard_moves_and_resizes_without_changing_station_or_payload_key() {
        let mut w = split();
        w.apply(
            &workbench::FloatEvent::Float {
                tile: TileId(2),
                rect: workbench::RelativeRect {
                    x: 0.1,
                    y: 0.2,
                    width: 0.4,
                    height: 0.5,
                },
            }
            .into(),
        );
        let mut c = controls(&w);
        c.set_focus(Some(Control::Tab(TileId(2))));
        let order = c.order();
        assert!(order.contains(&Control::Tab(TileId(2))));
        let before = w.floating()[0].clone();
        let mut event = KeyEvent::new(Key::Named(NamedKey::ArrowRight));
        event.mods.alt = true;
        let edits = c.key(event, &order).0;
        install(&mut c, &mut w, edits, 2);
        assert!((w.floating()[0].rect.x - 0.11).abs() < 0.0001);
        assert_eq!(w.floating()[0].tile, before.tile);
        let mut event = KeyEvent::new(Key::Named(NamedKey::ArrowDown));
        event.mods.alt = true;
        event.mods.shift = true;
        let edits = c.key(event, &order).0;
        install(&mut c, &mut w, edits, 2);
        assert!((w.floating()[0].rect.height - 0.51).abs() < 0.0001);
        assert_eq!(c.focus(), Some(&Control::Tab(TileId(2))));
        let (mut tree, routes, focus) = graft(&c, true);
        let map = super::window_projection::namespace(
            &mut tree,
            &mut focus.clone(),
            super::window_projection::WindowScope {
                slot: 1,
                instance: 3,
                session: 7,
            },
        );
        assert!(
            routes
                .values()
                .any(|r| r.control == Control::Close(TileId(2)))
        );
        assert!(!map.is_empty());
    }
    #[test]
    fn floating_body_grafts_below_its_actual_surface_slot_only_once() {
        let mut w = stack();
        w.apply(
            &workbench::FloatEvent::Float {
                tile: TileId(1),
                rect: workbench::RelativeRect {
                    x: 0.1,
                    y: 0.2,
                    width: 0.4,
                    height: 0.5,
                },
            }
            .into(),
        );
        let c = controls(&w);
        let (tree, _, _) = graft(&c, false);
        let body = NodeId(100002);
        let parents = tree
            .nodes
            .iter()
            .filter(|(_, node)| node.children().contains(&body))
            .collect::<Vec<_>>();
        assert_eq!(parents.len(), 1);
        assert_eq!(parents[0].1.label(), Some("Page 1 pane content"));
    }
}
