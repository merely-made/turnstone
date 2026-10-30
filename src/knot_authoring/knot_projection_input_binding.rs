// Copyright 2026 Mark Alan Boykin
// SPDX-License-Identifier: MPL-2.0

use super::*;

pub(super) fn knot_projection_input_binding(editable: &EditableTextV1) -> ProjectionInputBinding {
    ProjectionInputBinding {
        source: SourceBinding {
            authority: KNOT_PROJECTION_AUTHORITY.to_owned(),
            domain: KNOT_PROJECTION_DOMAIN.to_owned(),
            resource: editable.address.clone(),
        },
        expects_generation: editable
            .public_revision
            .as_ref()
            .map(|revision| PublicSourceRevision::new(knot::hex32(revision))),
        revision_evidence: if editable.public_revision.is_some() {
            RevisionEvidence::PublicGeneration
        } else {
            RevisionEvidence::RuntimeVerified
        },
    }
}
