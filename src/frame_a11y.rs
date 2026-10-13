// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! One namespaced frame tree, with existing pane bodies below actual apertures.
use crate::frame_controls::{FrameControls, FrameRoute};
use accesskit::{Action, NodeId, Role, TreeUpdate};
use genet_scripted_dom::NodeId as DomNodeId;
use layout_dom_api::{LayoutDom, LocalName, Namespace};
use std::collections::{HashMap, HashSet};
use uxtree::{UxTree, node_id_for_path};
use workbench::TileId;

pub(crate) enum FixtureTarget {
    Tab(u64),
    Close(u64),
    Divider(Vec<usize>, usize),
}
impl FixtureTarget {
    pub fn matches(&self, control: &crate::frame_controls::Control) -> bool {
        use crate::frame_controls::Control;
        match (self, control) {
            (Self::Tab(a), Control::Tab(b)) | (Self::Close(a), Control::Close(b)) => *a == b.0,
            (
                Self::Divider(path, index),
                Control::Divider {
                    path: p, index: i, ..
                },
            ) => *path == p.0 && index == i,
            _ => false,
        }
    }
}
/// Fixture grammar only: delivery still traverses the platform action queue.
pub(crate) fn fixture_request(text: &str) -> Result<(Action, FixtureTarget), String> {
    let mut words = text.split_whitespace();
    let action = match words.next() {
        Some("focus") => Action::Focus,
        Some("click") => Action::Click,
        Some("inc") => Action::Increment,
        Some("dec") => Action::Decrement,
        _ => return Err("frame-a11y wants focus|click|inc|dec".into()),
    };
    let number = |text: Option<&str>| -> Result<u64, String> {
        text.ok_or("missing frame target")?
            .parse()
            .map_err(|_| "invalid frame target number".into())
    };
    let target = match words.next() {
        Some("tab") => FixtureTarget::Tab(number(words.next())?),
        Some("close") => FixtureTarget::Close(number(words.next())?),
        Some("divider") => {
            let path = words.next().ok_or("missing divider path")?;
            let path = if path == "." {
                Vec::new()
            } else {
                path.split('.')
                    .map(|s| {
                        s.parse::<usize>()
                            .map_err(|_| "invalid divider path".to_owned())
                    })
                    .collect::<Result<Vec<_>, _>>()?
            };
            FixtureTarget::Divider(
                path,
                number(words.next())?
                    .try_into()
                    .map_err(|_| "divider index overflow")?,
            )
        },
        _ => return Err("frame-a11y wants tab|close|divider".into()),
    };
    if words.next().is_some() {
        return Err("extra frame-a11y arguments".into());
    }
    Ok((action, target))
}

pub(crate) struct FrameProjection {
    root: NodeId,
    nodes: Vec<(NodeId, accesskit::Node)>,
    pub routes: HashMap<NodeId, FrameRoute>,
    slots: HashMap<TileId, NodeId>,
    pub focus: Option<NodeId>,
}
pub(crate) fn project(
    update: TreeUpdate,
    map: HashMap<NodeId, DomNodeId>,
    controls: &FrameControls,
    focused: bool,
) -> FrameProjection {
    let local_root = update.tree.as_ref().expect("full frame observation").root;
    let stamp = controls.stamp();
    let remap: HashMap<_, _> = update
        .nodes
        .iter()
        .map(|(local, _)| {
            (
                *local,
                node_id_for_path(&format!(
                    "turnstone/frame/{}/revision/{}/dom/{}",
                    stamp.admission, stamp.revision, local.0
                )),
            )
        })
        .collect();
    let mut routes = HashMap::new();
    let mut slots = HashMap::new();
    let mut nodes = Vec::new();
    let dom = controls.dom.borrow();
    for (local, mut node) in update.nodes {
        let global = remap[&local];
        node.set_children(
            node.children()
                .iter()
                .filter_map(|id| remap.get(id).copied())
                .collect::<Vec<_>>(),
        );
        if local == local_root {
            node.set_role(Role::Group);
            node.set_label("Pane controls");
        }
        if let Some(&dom_node) = map.get(&local) {
            if let Some(id) = dom
                .attribute(
                    dom_node,
                    &Namespace::default(),
                    &LocalName::from(cambium::FRISKET_TILE_ATTR),
                )
                .and_then(|s| s.parse::<u64>().ok())
            {
                slots.insert(TileId(id), global);
            }
            if !node.is_hidden() {
                if let Some(control) = controls.control_at(dom_node) {
                    node.add_action(Action::Focus);
                    if let Some(value) = controls.divider_value(&control) {
                        node.set_numeric_value(value);
                        node.set_min_numeric_value(5.0);
                        node.set_max_numeric_value(95.0);
                        node.set_numeric_value_step(1.0);
                        node.add_action(Action::Increment);
                        node.add_action(Action::Decrement);
                    } else {
                        node.add_action(Action::Click);
                    }
                    routes.insert(global, controls.route(control));
                }
            }
        }
        nodes.push((global, node));
    }
    let focus = focused.then(|| remap.get(&update.focus).copied()).flatten();
    FrameProjection {
        root: remap[&local_root],
        nodes,
        routes,
        slots,
        focus,
    }
}

/// Replace the former structural containers; preserve each pane body subtree.
pub(crate) fn graft(
    tree: &mut UxTree,
    parent: NodeId,
    pane_roots: &HashMap<TileId, NodeId>,
    mut frame: FrameProjection,
) -> Option<(HashMap<NodeId, FrameRoute>, Option<NodeId>)> {
    let (_, old) = tree.nodes.iter().find(|(id, _)| *id == parent)?;
    let keep: HashSet<_> = frame
        .slots
        .keys()
        .filter_map(|tile| pane_roots.get(tile).copied())
        .collect();
    let mut pending = old.children().to_vec();
    let mut retired = HashSet::new();
    while let Some(id) = pending.pop() {
        if keep.contains(&id) || !retired.insert(id) {
            continue;
        }
        if let Some((_, node)) = tree.nodes.iter().find(|(candidate, _)| *candidate == id) {
            pending.extend(node.children().iter().copied());
        }
    }
    for (tile, slot) in &frame.slots {
        if let Some(body) = pane_roots
            .get(tile)
            .filter(|body| tree.nodes.iter().any(|(id, _)| id == *body))
        {
            if let Some((_, node)) = frame.nodes.iter_mut().find(|(id, _)| id == slot) {
                node.set_children(vec![*body]);
            }
        }
    }
    tree.nodes.retain(|(id, _)| !retired.contains(id));
    tree.nodes
        .iter_mut()
        .find(|(id, _)| *id == parent)?
        .1
        .set_children(vec![frame.root]);
    tree.nodes.extend(frame.nodes);
    Some((frame.routes, frame.focus))
}
