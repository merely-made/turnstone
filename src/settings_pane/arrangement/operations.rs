// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn apply_operation(
    model: &mut Composition,
    operation: ArrangementOperation,
    sources: &Sources,
) -> Result<(), ArrangementError> {
    match operation {
        ArrangementOperation::AddSetting {
            reference,
            setting_id,
            container,
            index,
        } => {
            if !sources
                .get(&reference)
                .is_some_and(|source| source.contains_key(&setting_id))
            {
                return Err(ArrangementError::InvalidOperation(
                    "new row target is unavailable",
                ));
            }
            let key = ComponentKey(format!(
                "turnstone.setting.instance:{}",
                uuid::Uuid::new_v4()
            ));
            let descriptor = ComponentDescriptor {
                key: key.clone(),
                kind: ComponentKind(SETTING_KIND.into()),
                config: Value::Null,
                binding: binding(&reference, &setting_id),
            };
            let children = container_children(&mut model.placement, &container)?;
            children.insert(index.min(children.len()), PlacementNode::Component { key });
            model.components.push(descriptor);
            Ok(())
        },
        ArrangementOperation::MoveUp(key) => move_adjacent(model, &key, false),
        ArrangementOperation::MoveDown(key) => move_adjacent(model, &key, true),
        ArrangementOperation::MoveTo {
            key,
            container,
            index,
        } => {
            let node = remove_placement(model, &key)?;
            let children = container_children(&mut model.placement, &container)?;
            let index = index.min(children.len());
            children.insert(index, node);
            model.unplaced.retain(|item| item != &key);
            Ok(())
        },
        ArrangementOperation::AddGroup { key, parent, index } => {
            insert_container(model, ContainerKind::Group, key, parent, index)
        },
        ArrangementOperation::AddDrawer { key, parent, index } => {
            insert_container(model, ContainerKind::Drawer, key, parent, index)
        },
        ArrangementOperation::RemoveEmptyContainer(container) => {
            remove_empty_container(model, &container)
        },
        ArrangementOperation::Hide(key) => {
            if model.unplaced.contains(&key) {
                return Err(ArrangementError::InvalidOperation("row is already hidden"));
            }
            remove_placement(model, &key)?;
            model.unplaced.push(key);
            Ok(())
        },
        ArrangementOperation::Show {
            key,
            container,
            index,
        } => {
            if !model.unplaced.contains(&key) {
                return Err(ArrangementError::InvalidOperation("row is not hidden"));
            }
            let children = container_children(&mut model.placement, &container)?;
            children.insert(
                index.min(children.len()),
                PlacementNode::Component { key: key.clone() },
            );
            model.unplaced.retain(|item| item != &key);
            Ok(())
        },
        ArrangementOperation::Duplicate(key) => {
            let descriptor = model
                .components
                .iter()
                .find(|item| item.key == key)
                .cloned()
                .ok_or_else(|| ArrangementError::UnknownComponent(key.clone()))?;
            let reference = descriptor
                .binding
                .get("reference")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let setting_id = descriptor
                .binding
                .get("setting_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !sources
                .get(reference)
                .is_some_and(|source| source.contains_key(setting_id))
            {
                return Err(ArrangementError::InvalidOperation(
                    "cannot duplicate an unavailable binding",
                ));
            }
            let hidden = model.unplaced.contains(&key);
            let new_key = ComponentKey(format!(
                "turnstone.setting.instance:{}",
                uuid::Uuid::new_v4()
            ));
            let duplicate = ComponentDescriptor {
                key: new_key.clone(),
                ..descriptor
            };
            model.components.push(duplicate);
            if hidden {
                model.unplaced.push(new_key);
            } else {
                let location = locate(model, &key)
                    .ok_or_else(|| ArrangementError::UnknownComponent(key.clone()))?;
                let container = location
                    .containers
                    .last()
                    .cloned()
                    .unwrap_or(root_container());
                let children = container_children(&mut model.placement, &container)?;
                children.insert(
                    (location.index + 1).min(children.len()),
                    PlacementNode::Component { key: new_key },
                );
            }
            Ok(())
        },
        ArrangementOperation::Delete { key, approval } => {
            if approval.key != key {
                return Err(ArrangementError::InvalidOperation(
                    "delete approval is for another row",
                ));
            }
            remove_placement(model, &key).ok();
            model.unplaced.retain(|item| item != &key);
            let old = model.components.len();
            model.components.retain(|item| item.key != key);
            if old == model.components.len() {
                return Err(ArrangementError::UnknownComponent(key));
            }
            Ok(())
        },
        ArrangementOperation::Rebind {
            key,
            reference,
            setting_id,
            draft_clean,
            no_pending_write,
        } => {
            if !draft_clean || !no_pending_write {
                return Err(ArrangementError::InvalidOperation(
                    "rebind requires a clean row with no pending write",
                ));
            }
            let spec = sources
                .get(&reference)
                .and_then(|source| source.get(&setting_id))
                .ok_or(ArrangementError::InvalidOperation(
                    "new binding target is unavailable",
                ))?;
            let _ = spec;
            let descriptor = model
                .components
                .iter_mut()
                .find(|item| item.key == key)
                .ok_or_else(|| ArrangementError::UnknownComponent(key.clone()))?;
            descriptor.binding = binding(&reference, &setting_id);
            Ok(())
        },
    }
}

fn move_adjacent(
    model: &mut Composition,
    key: &ComponentKey,
    down: bool,
) -> Result<(), ArrangementError> {
    fn recurse(nodes: &mut [PlacementNode], key: &ComponentKey, down: bool) -> bool {
        for index in 0..nodes.len() {
            match &mut nodes[index] {
                PlacementNode::Component { key: found } if found == key => {
                    let target = if down {
                        index + 1
                    } else {
                        index.wrapping_sub(1)
                    };
                    if target >= nodes.len() {
                        return false;
                    }
                    nodes.swap(index, target);
                    return true;
                },
                PlacementNode::Group { children, .. } | PlacementNode::Drawer { children, .. } => {
                    if recurse(children, key, down) {
                        return true;
                    }
                },
                _ => {},
            }
        }
        false
    }
    if model.unplaced.contains(key) || !recurse(&mut model.placement, key, down) {
        return Err(ArrangementError::InvalidOperation(
            "row has no movable neighbor",
        ));
    }
    Ok(())
}

fn insert_container(
    model: &mut Composition,
    kind: ContainerKind,
    key: String,
    parent: ContainerKey,
    index: usize,
) -> Result<(), ArrangementError> {
    if key.is_empty() || kind == ContainerKind::Root {
        return Err(ArrangementError::InvalidOperation(
            "container identity is invalid",
        ));
    }
    let children = container_children(&mut model.placement, &parent)?;
    let node = match kind {
        ContainerKind::Group => PlacementNode::Group {
            key: key.into(),
            children: Vec::new(),
        },
        ContainerKind::Drawer => PlacementNode::Drawer {
            key: key.into(),
            children: Vec::new(),
        },
        ContainerKind::Root => unreachable!(),
    };
    children.insert(index.min(children.len()), node);
    Ok(())
}

fn remove_empty_container(
    model: &mut Composition,
    target: &ContainerKey,
) -> Result<(), ArrangementError> {
    if target.kind == ContainerKind::Root {
        return Err(ArrangementError::InvalidOperation(
            "cannot remove the root container",
        ));
    }
    fn remove(nodes: &mut Vec<PlacementNode>, target: &ContainerKey) -> bool {
        if let Some(index) = nodes.iter().position(|node| match node {
            PlacementNode::Group { key, children } => {
                target.kind == ContainerKind::Group && key.0 == target.key && children.is_empty()
            },
            PlacementNode::Drawer { key, children } => {
                target.kind == ContainerKind::Drawer && key.0 == target.key && children.is_empty()
            },
            _ => false,
        }) {
            nodes.remove(index);
            return true;
        }
        for node in nodes {
            match node {
                PlacementNode::Group { children, .. } | PlacementNode::Drawer { children, .. } => {
                    if remove(children, target) {
                        return true;
                    }
                },
                _ => {},
            }
        }
        false
    }
    if remove(&mut model.placement, target) {
        Ok(())
    } else {
        Err(ArrangementError::InvalidOperation(
            "container is missing or not empty",
        ))
    }
}

fn remove_placement(
    model: &mut Composition,
    key: &ComponentKey,
) -> Result<PlacementNode, ArrangementError> {
    fn remove(nodes: &mut Vec<PlacementNode>, key: &ComponentKey) -> Option<PlacementNode> {
        if let Some(index) = nodes.iter().position(
            |node| matches!(node, PlacementNode::Component { key: candidate } if candidate == key),
        ) {
            return Some(nodes.remove(index));
        }
        for node in nodes {
            match node {
                PlacementNode::Group { children, .. } | PlacementNode::Drawer { children, .. } => {
                    if let Some(found) = remove(children, key) {
                        return Some(found);
                    }
                },
                _ => {},
            }
        }
        None
    }
    remove(&mut model.placement, key)
        .or_else(|| {
            if model.unplaced.contains(key) {
                Some(PlacementNode::Component { key: key.clone() })
            } else {
                None
            }
        })
        .ok_or_else(|| ArrangementError::UnknownComponent(key.clone()))
}

fn container_children<'a>(
    placement: &'a mut Vec<PlacementNode>,
    container: &ContainerKey,
) -> Result<&'a mut Vec<PlacementNode>, ArrangementError> {
    if container.kind == ContainerKind::Root {
        return Ok(placement);
    }
    fn find<'a>(
        nodes: &'a mut [PlacementNode],
        target: &ContainerKey,
    ) -> Option<&'a mut Vec<PlacementNode>> {
        for node in nodes {
            match node {
                PlacementNode::Group { key, children } => {
                    if target.kind == ContainerKind::Group && key.0 == target.key {
                        return Some(children);
                    }
                    if let Some(found) = find(children, target) {
                        return Some(found);
                    }
                },
                PlacementNode::Drawer { key, children } => {
                    if target.kind == ContainerKind::Drawer && key.0 == target.key {
                        return Some(children);
                    }
                    if let Some(found) = find(children, target) {
                        return Some(found);
                    }
                },
                _ => {},
            }
        }
        None
    }
    find(placement, container).ok_or_else(|| ArrangementError::UnknownContainer(container.clone()))
}

fn locate(model: &Composition, key: &ComponentKey) -> Option<PlacementLocation> {
    fn walk(
        nodes: &[PlacementNode],
        path: &[ContainerKey],
        key: &ComponentKey,
    ) -> Option<PlacementLocation> {
        for (index, node) in nodes.iter().enumerate() {
            match node {
                PlacementNode::Component { key: found } if found == key => {
                    return Some(PlacementLocation {
                        containers: path.to_vec(),
                        index,
                    });
                },
                PlacementNode::Group {
                    key: group,
                    children,
                } => {
                    let mut next = path.to_vec();
                    next.push(ContainerKey {
                        kind: ContainerKind::Group,
                        key: group.0.clone(),
                    });
                    if let Some(found) = walk(children, &next, key) {
                        return Some(found);
                    }
                },
                PlacementNode::Drawer {
                    key: drawer,
                    children,
                } => {
                    let mut next = path.to_vec();
                    next.push(ContainerKey {
                        kind: ContainerKind::Drawer,
                        key: drawer.0.clone(),
                    });
                    if let Some(found) = walk(children, &next, key) {
                        return Some(found);
                    }
                },
                _ => {},
            }
        }
        None
    }
    walk(&model.placement, &[], key)
}

fn root_container() -> ContainerKey {
    ContainerKey {
        kind: ContainerKind::Root,
        key: String::new(),
    }
}

pub(super) fn effect(
    edits: Vec<CompositionEdit>,
    before: &Composition,
    after: &Composition,
) -> ArrangementEffect {
    let mut effect = ArrangementEffect {
        edits: edits.clone(),
        ..ArrangementEffect::default()
    };
    for edit in edits {
        match edit {
            CompositionEdit::Remove(key) => {
                effect.removed.insert(key.clone());
                effect.prepare_routes.insert(key);
            },
            CompositionEdit::ChangeBinding { key, .. } => {
                effect.rebound.insert(key.clone());
                effect.prepare_routes.insert(key);
            },
            CompositionEdit::ReplaceKind { key, .. } => {
                effect.prepare_routes.insert(key);
            },
            _ => {},
        }
    }
    // CompositionEdit's semantic locations omit ancestor sibling indexes. The
    // retained runtime tracks them, so prepare every child whose full route
    // signature changes (for example when inserting a group before a group).
    let old = route_signatures(before);
    let new = route_signatures(after);
    let keys = old
        .keys()
        .chain(new.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    for key in keys {
        if old.get(&key) != new.get(&key) {
            effect.prepare_routes.insert(key);
        }
    }
    effect
}

fn route_signatures(
    model: &Composition,
) -> BTreeMap<ComponentKey, (Vec<(ContainerKey, usize)>, bool)> {
    fn visit(
        nodes: &[PlacementNode],
        container: &ContainerKey,
        signature: &[(ContainerKey, usize)],
        hidden: bool,
        out: &mut BTreeMap<ComponentKey, (Vec<(ContainerKey, usize)>, bool)>,
    ) {
        for (index, node) in nodes.iter().enumerate() {
            match node {
                PlacementNode::Component { key } => {
                    let mut route = signature.to_vec();
                    route.push((container.clone(), index));
                    out.insert(key.clone(), (route, hidden));
                },
                PlacementNode::Group { key, children } => {
                    let next = ContainerKey {
                        kind: ContainerKind::Group,
                        key: key.0.clone(),
                    };
                    let mut route = signature.to_vec();
                    route.push((container.clone(), index));
                    visit(children, &next, &route, hidden, out);
                },
                PlacementNode::Drawer { key, children } => {
                    let next = ContainerKey {
                        kind: ContainerKind::Drawer,
                        key: key.0.clone(),
                    };
                    let mut route = signature.to_vec();
                    route.push((container.clone(), index));
                    visit(children, &next, &route, hidden, out);
                },
            }
        }
    }
    let root = root_container();
    let mut out = BTreeMap::new();
    visit(&model.placement, &root, &[], false, &mut out);
    for key in &model.unplaced {
        out.insert(key.clone(), (Vec::new(), true));
    }
    out
}
