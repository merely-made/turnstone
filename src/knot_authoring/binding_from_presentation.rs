// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn binding_from_presentation(
    target: InstanceId,
    presentation: ResolvedPresentation,
    address: &str,
) -> Option<DocumentBinding> {
    let ResolvedContent::EditableText(editable) = presentation.content else {
        return None;
    };
    if !address_matches(address, &editable.address) {
        return None;
    }
    let save_action = presentation
        .semantics
        .actions
        .iter()
        .find(|action| action.intent.0 == EDITABLE_TEXT_SAVE_INTENT)?
        .clone();
    let clip_action = presentation
        .semantics
        .actions
        .iter()
        .find(|action| action.intent.0 == KNOT_CLIP_INSERT_INTENT)?
        .clone();
    let resolve_action = presentation
        .semantics
        .actions
        .iter()
        .find(|action| action.intent.0 == KNOT_TRANSCLUSION_RESOLVE_INTENT)
        .cloned();
    let run_action = presentation
        .semantics
        .actions
        .iter()
        .find(|action| action.intent.0 == KNOT_BLOCK_RUN_INTENT)
        .cloned();
    Some(DocumentBinding {
        target,
        save_action,
        clip_action,
        resolve_action,
        run_action,
        editable,
    })
}
