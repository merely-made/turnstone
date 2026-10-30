// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

/// Surface ids named directly by a delta. Resource ids need graph projection.
///
/// Exhaustive by construction: a new `CapturedDelta` variant fails to compile
/// here until it is classified, which is the same discipline `ring_of` uses
/// for actions. A variant that silently touched nothing would be a behavior
/// that silently stopped waking.
pub fn touched_ids(delta: &CapturedDelta) -> Vec<&str> {
    use CapturedDelta as D;
    match delta {
        D::ReplayAddNodeWithIdIfMissing { id, .. } => vec![id.as_str()],
        D::ReplaySetShownResourceById { surface_id, .. } => vec![surface_id.as_str()],

        // Resource UUIDs are not Surface UUIDs and must not become bare scopes.
        // touched_surface_ids resolves these through the graph explicitly.
        D::ReplaySetResourceRecordById { .. } | D::ReplaySetResourceEdgesByIds { .. } => Vec::new(),

        D::ReplayRemoveNodeById { node_id, .. }
        | D::ReplaySetNodeTitleById { node_id, .. }
        | D::ReplaySetNodeUrlById { node_id, .. }
        | D::ReplaySetNodeImageById { node_id, .. }
        | D::ReplaySetNodeThumbnailById { node_id, .. }
        | D::ReplaySetNodeFaviconById { node_id, .. }
        | D::ReplaySetNodeMimeHintById { node_id, .. }
        | D::ReplaySetNodeContentById { node_id, .. }
        | D::ReplaySetNodeNestedById { node_id, .. }
        | D::ReplaySetNodePinnedById { node_id, .. }
        | D::ReplaySetNodeFacetById { node_id, .. }
        | D::ReplayRemoveNodeFacetById { node_id, .. }
        | D::ReplayInsertNodeTagById { node_id, .. }
        | D::ReplayRemoveNodeTagById { node_id, .. }
        | D::ReplaySetNodeBodyById { node_id, .. }
        | D::ReplayNavigateNodeById { node_id, .. }
        | D::ReplayNodeHistoryBackById { node_id, .. }
        | D::ReplayNodeHistoryForwardById { node_id, .. }
        | D::ReplayAppendNodePropertyById { node_id, .. }
        | D::ReplayAddNodeClassificationById { node_id, .. }
        | D::ReplayRemoveNodeClassificationById { node_id, .. }
        | D::ReplaySetNodeClassificationStatusById { node_id, .. }
        | D::ReplaySetNodePrimaryClassificationById { node_id, .. }
        | D::ReplayRecordNodeDerivationById { node_id, .. }
        | D::ReplaySetNodeTagIconOverrideById { node_id, .. }
        | D::ReplayAppendFrameLayoutHintById { node_id, .. }
        | D::ReplayRemoveFrameLayoutHintById { node_id, .. }
        | D::ReplayMoveFrameLayoutHintById { node_id, .. }
        | D::ReplaySetFrameSplitOfferSuppressedById { node_id, .. }
        | D::ReplayUpdateNodeHistoryById { node_id, .. }
        | D::ReplayTouchNodeLastVisitedById { node_id, .. } => vec![node_id.as_str()],

        D::ReplayAssertRelationByIds { from_id, to_id, .. }
        | D::ReplayRetractRelationsByIds { from_id, to_id, .. }
        | D::ReplayAppendTraversalByIds { from_id, to_id, .. }
        | D::ReplaySetEdgesByIds { from_id, to_id, .. }
        | D::ReplaySetEdgeSemanticPredicateByIds { from_id, to_id, .. }
        | D::ReplayAssertSemanticPredicateByIds { from_id, to_id, .. } => {
            vec![from_id.as_str(), to_id.as_str()]
        },

        D::ReplayBranchHistoryByIds {
            child_id,
            parent_id,
        } => vec![child_id.as_str(), parent_id.as_str()],

        // Session-level and field/coupling deltas name no node. The physics
        // tier is deliberately outside this vocabulary: spatial influence
        // reaches the graph through fields, never through a petition, so a
        // field change is not a thing a behavior is woken by.
        D::ReplaySetImportRecords { .. }
        | D::ReplayAddField { .. }
        | D::ReplayRetireFieldById { .. }
        | D::ReplayRemoveFieldById { .. }
        | D::ReplayAddCoupling { .. }
        | D::ReplaySetFieldCouplingStrengthByFieldId { .. }
        | D::ReplayActivateFieldById { .. }
        | D::ReplayRetractCouplingById { .. } => Vec::new(),
    }
}
