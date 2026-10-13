// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

//! Lossless admission of the former live station authority. Unsupported
//! container forms refuse before either layout or host custody changes.
use super::workspace_bridge::OuterWorkspace;
use super::{LayoutNode, PaneContent, SpaceBlueprint};
use workbench::{FloatingTile, TabStack, TileBranch, TileTree, Workspace};

pub(crate) fn import(
    blueprint: &SpaceBlueprint,
    payloads: &OuterWorkspace,
) -> Result<OuterWorkspace, String> {
    payloads.validate()?;
    if !super::validate_spaces(std::slice::from_ref(blueprint)).is_empty() {
        return Err("Legacy space violates pane custody".into());
    }
    if blueprint.panes.len() != payloads.panes.len() {
        return Err("Legacy stations and host payloads disagree".into());
    }
    let mut frame = payloads.clone();
    for spec in &blueprint.panes {
        let (content, _) = frame
            .panes
            .get(&spec.id.0)
            .ok_or("Legacy pane has no host payload")?;
        if content.kind_id() != spec.kind {
            return Err("Legacy pane kind disagrees with payload".into());
        }
        if matches!(content, PaneContent::Registered(_)) {
            frame.descriptors.insert(
                spec.id.0,
                serde_json::to_value(spec).map_err(|e| e.to_string())?,
            );
        }
    }
    fn tile(frame: &OuterWorkspace, id: super::PaneId) -> Result<workbench::Tile, String> {
        frame
            .workspace
            .find(workbench::TileId(id.0))
            .cloned()
            .ok_or("Legacy station has no tile".into())
    }
    fn tree(frame: &OuterWorkspace, node: &LayoutNode) -> Result<TileTree, String> {
        Ok(match node {
            LayoutNode::Pane(id) => TileTree::single(tile(frame, *id)?),
            LayoutNode::Split { axis, children } => TileTree::Split {
                axis: if *axis == super::SplitAxis::Horizontal {
                    workbench::SplitAxis::Row
                } else {
                    workbench::SplitAxis::Column
                },
                children: children
                    .iter()
                    .map(|child| Ok(TileBranch::new(child.fraction, tree(frame, &child.tree)?)))
                    .collect::<Result<_, String>>()?,
            },
            LayoutNode::Tabs { children, active } => {
                let tabs = children
                    .iter()
                    .map(|child| match child {
                        LayoutNode::Pane(id) => tile(frame, *id),
                        _ => Err("Nested legacy tab sublayouts need an explicit migration".into()),
                    })
                    .collect::<Result<_, String>>()?;
                TileTree::Stack(TabStack {
                    tabs,
                    active: *active,
                })
            },
            LayoutNode::Grid { .. } => return Err("Legacy grid needs an explicit migration".into()),
        })
    }
    let tiled = blueprint
        .tiled
        .as_ref()
        .map(|node| tree(&frame, node))
        .transpose()?
        .unwrap_or_else(TileTree::empty);
    let mut workspace = Workspace::new(tiled);
    let mut floats = blueprint.floating.iter().collect::<Vec<_>>();
    floats.sort_by_key(|f| (f.z, f.pane.0));
    for old in floats {
        let r = old.rect;
        let c = old.constraints;
        if !workspace.insert_floating(FloatingTile {
            tile: tile(&frame, old.pane)?,
            rect: workbench::RelativeRect {
                x: r.x,
                y: r.y,
                width: r.width,
                height: r.height,
            },
            constraints: workbench::FloatSizeConstraints {
                min_width: c.min_width,
                min_height: c.min_height,
                max_width: c.max_width,
                max_height: c.max_height,
            },
            z: old.z,
            pinned: old.pinned,
            visible: old.visible,
        }) {
            return Err("Legacy float duplicates a station".into());
        }
    }
    frame.workspace = workspace;
    frame.validate()?;
    Ok(frame)
}
