// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Transitional projection from the durable Frisket leaf payload into the
//! blueprint station model.
//!
//! Frisket still owns the pre-A8 on-disk payload while A5 proves floating
//! presentation through the live compositor.  This conversion is deliberately
//! one-way: it supplies the first blueprint for a space at the moment a pane
//! enters the float layer.  The blueprint then owns that space's visible
//! stations; Frisket remains the renderer/persistence lookup until A8 replaces
//! the legacy frame sidecar.

use super::{
    ChromeBlueprint, ContextBinding, FrisketLayout, LayoutBranch, LayoutNode, NormalizationPolicy,
    PaneConfig, PaneContent, PaneId, PaneNode, PaneSource, PaneSpec, SourceRef, SpaceBlueprint,
    SpaceId,
};

/// Project a legacy pane tree into an equivalent tiled blueprint.
///
/// The source descriptors are conservative fixed bindings because the legacy
/// leaf already carries its resolved graph/member payload.  Context following
/// remains the registry's long-term format concern, not a reason to retarget a
/// live legacy pane while it is being lifted into the float layer.
pub fn blueprint_from_frisket(layout: &FrisketLayout) -> SpaceBlueprint {
    let mut panes = Vec::new();
    let tiled = project_node(&layout.root, &mut panes);
    SpaceBlueprint {
        id: SpaceId::new(layout.id.as_str()),
        label: layout.label.clone(),
        panes,
        tiled: Some(tiled),
        floating: Vec::new(),
        chrome: ChromeBlueprint::default(),
        normalization: NormalizationPolicy::default(),
    }
}

fn project_node(node: &PaneNode, panes: &mut Vec<PaneSpec>) -> LayoutNode {
    match node {
        PaneNode::Leaf {
            pane_id,
            content,
            graph_id,
        } => {
            panes.push(project_spec(*pane_id, content, *graph_id));
            LayoutNode::Pane(*pane_id)
        }
        PaneNode::Split {
            axis,
            ratio,
            first,
            second,
        } => LayoutNode::Split {
            axis: *axis,
            children: vec![
                LayoutBranch {
                    fraction: *ratio,
                    tree: project_node(first, panes),
                },
                LayoutBranch {
                    fraction: 1.0 - *ratio,
                    tree: project_node(second, panes),
                },
            ],
        },
    }
}

fn project_spec(id: PaneId, content: &PaneContent, graph: super::GraphId) -> PaneSpec {
    let source = match content {
        PaneContent::Workbench | PaneContent::Orrery => SourceRef::Forme {
            graph,
            forme: mere::forme::FormeRef::Identity(graph.0),
        },
        PaneContent::Tile(member) => SourceRef::Member {
            graph,
            member: *member,
        },
        content if content.follows_active_graph() => SourceRef::Graph(graph),
        _ => SourceRef::Application,
    };
    PaneSpec {
        id,
        kind: content.kind_id(),
        source: PaneSource::Fixed(source),
        context: ContextBinding::Own,
        config: PaneConfig::empty(format!("turnstone.{}", content.kind_id().as_str())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_preserves_leaf_identity_and_binary_ratios() {
        let layout = FrisketLayout {
            id: super::super::FrisketId::new("proof"),
            label: "proof".into(),
            root: PaneNode::Split {
                axis: super::super::SplitAxis::Horizontal,
                ratio: 0.7,
                first: Box::new(PaneNode::Leaf {
                    pane_id: PaneId(3),
                    content: PaneContent::Orrery,
                    graph_id: super::super::GraphId::nil(),
                }),
                second: Box::new(PaneNode::Leaf {
                    pane_id: PaneId(4),
                    content: PaneContent::Roster,
                    graph_id: super::super::GraphId::nil(),
                }),
            },
        };
        let blueprint = blueprint_from_frisket(&layout);

        assert_eq!(blueprint.tiled_panes(), vec![PaneId(3), PaneId(4)]);
        let LayoutNode::Split { children, .. } = blueprint.tiled.expect("split") else {
            panic!("legacy split projects as a blueprint split");
        };
        assert_eq!(children.len(), 2);
        assert!((children[0].fraction - 0.7).abs() < f32::EPSILON);
        assert!((children[1].fraction - 0.3).abs() < f32::EPSILON);
    }
}
